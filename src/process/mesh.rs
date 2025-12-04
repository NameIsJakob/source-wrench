use indexmap::{IndexMap, IndexSet};
use kdtree::{KdTree, distance::squared_euclidean};
use std::sync::Arc;
use thiserror::Error as ThisError;

use crate::{
    debug, import, input,
    utilities::mathematics::{BoundingBox, Matrix4, Vector2, Vector3, Vector4, create_space_transform},
    verbose, warn,
};

#[derive(Debug, ThisError)]
pub enum ProcessingMeshError {
    #[error("No Animation File Selected")]
    NoFileSource,
    #[error("Model File Source Not Loaded")]
    FileSourceNotLoaded,
    #[error("Face Was Incomplete")]
    IncompleteFace,
    #[error("Model Has Too Many Materials")]
    TooManyMaterials,
    #[error("Model Has Too Many Body Parts")]
    TooManyBodyParts,
    #[error("Part Has Too Many Meshes")]
    TooMeshes,
}

#[derive(Debug, Default)]
struct WeightLink {
    bone: u8,
    weight: f64,
}

#[derive(Debug, Default)]
struct TriangleVertex {
    position: Vector3,
    normal: Vector3,
    texture_coordinate: Vector2,
    links: Vec<WeightLink>,
}

#[derive(Debug, Default)]
struct TriangleList {
    vertices: Vec<TriangleVertex>,
    tangents: Vec<Vector4>,
    triangles: Vec<[usize; 3]>,
}

pub fn process_meshes(
    input_data: &input::SourceInput,
    source_files: &import::FileManager,
    processed_bone_data: &super::BoneData,
) -> Result<super::ModelData, ProcessingMeshError> {
    let mut processed_model_data = super::ModelData::default();

    for imputed_body_group in input_data.body_groups.iter() {
        let processed_body_part_name = imputed_body_group.name.clone();
        debug_assert!(!processed_model_data.body_parts.contains_key(&processed_body_part_name));

        let mut processed_body_part = super::BodyPart::default();

        for imputed_model in &imputed_body_group.models {
            if imputed_model.blank {
                processed_body_part.models.push(super::Model::default());
                continue;
            }

            let mut processed_model = super::Model {
                name: imputed_model.name.clone(),
                ..Default::default()
            };

            if processed_model.name.len() > 64 {
                warn!("Model Part Name Longer That 64! Trimming!");
                processed_model.name.truncate(64);
            }

            let imported_file = source_files
                .get_file_data(imputed_model.source_file_path.as_ref().ok_or(ProcessingMeshError::NoFileSource)?)
                .ok_or(ProcessingMeshError::FileSourceNotLoaded)?;

            let triangle_lists = create_triangle_lists(
                &imputed_model.enabled_source_parts,
                imported_file,
                &mut processed_model_data.materials,
                processed_bone_data,
            )?;

            if triangle_lists.is_empty() {
                warn!("Model Had No Parts! Defaulting To Blank!");
                processed_body_part.models.push(super::Model::default());
                continue;
            }

            let mut bad_vertex_count = 0;
            let mut culled_vertex_count = 0;
            let mut face_count = 0;
            let mut vertex_count = 0;
            let mut strip_count = 0;
            for (material_index, mut triangle_list) in triangle_lists {
                reorder_triangle_vertex_order(&mut triangle_list);
                sort_vertices_by_hardware_bones(&mut triangle_list);
                bad_vertex_count += calculate_vertex_tangents(&mut triangle_list);
                culled_vertex_count += cull_weight_links(&mut triangle_list);
                let meshes = convert_to_meshes(
                    material_index,
                    triangle_list,
                    processed_bone_data,
                    &mut processed_model_data.bounding_box,
                    &mut processed_model_data.hitboxes,
                );
                face_count += meshes.1;
                vertex_count += meshes.2;
                strip_count += meshes.3;
                processed_model.meshes.extend(meshes.0);
                if processed_model.meshes.len() > i32::MAX as usize {
                    return Err(ProcessingMeshError::TooMeshes);
                }
            }

            if bad_vertex_count > 0 {
                warn!("{} Had {bad_vertex_count} Bad Vertices!", imputed_model.name);
            }

            if culled_vertex_count > 0 {
                warn!("{} Had {culled_vertex_count} Weight Culled Vertices!", imputed_model.name);
            }

            verbose!(
                "{} has {face_count} faces, {vertex_count} vertices and {} indices.",
                imputed_model.name,
                face_count * 3
            );

            debug!("{} has {strip_count} strips.", imputed_model.name);

            processed_body_part.models.push(processed_model);
        }

        processed_model_data.body_parts.insert(processed_body_part_name, processed_body_part);
    }

    if processed_model_data.body_parts.len() > i32::MAX as usize {
        return Err(ProcessingMeshError::TooManyBodyParts);
    }

    if processed_model_data.materials.len() > (i16::MAX as usize + 1) {
        return Err(ProcessingMeshError::TooManyMaterials);
    }

    // TODO: Check if bounding box is too large

    Ok(processed_model_data)
}

/// Combines parts into triangle lists for each material.
fn create_triangle_lists(
    enabled_parts: &[bool],
    imported_file: Arc<import::FileData>,
    material_table: &mut IndexSet<String>,
    processed_bone_data: &super::BoneData,
) -> Result<IndexMap<usize, TriangleList>, ProcessingMeshError> {
    let mut triangle_lists: IndexMap<usize, TriangleList> = IndexMap::new();
    let mut vertex_trees = IndexMap::new();

    for (imputed_part_index, imputed_part_enabled) in enabled_parts.iter().enumerate() {
        if !*imputed_part_enabled {
            continue;
        }

        let import_part = &imported_file.parts[imputed_part_index];

        for (material, faces) in &import_part.polygons {
            let material_index = material_table.insert_full(material.clone()).0;

            let triangle_list = triangle_lists.entry(material_index).or_default();
            let vertex_tree = vertex_trees.entry(material_index).or_insert(KdTree::new(3));

            for face in faces {
                if face.len() < 3 {
                    return Err(ProcessingMeshError::IncompleteFace);
                }

                let triangulated_face = triangulate_face(face, &import_part.vertices);

                for mut triangle in triangulated_face {
                    for vertex_index in &mut triangle {
                        let import_vertex = &import_part.vertices[*vertex_index];

                        let mut mapped_links = Vec::with_capacity(import_vertex.links.len());

                        for (link, weight) in &import_vertex.links {
                            let (import_bone_name, _) = imported_file.skeleton.get_index(*link).unwrap();

                            let mapped_index = match processed_bone_data.processed_bones.get_full(import_bone_name) {
                                Some((index, _, _)) => index,
                                None => todo!("Find the collapsed bone index!"),
                            };

                            mapped_links.push(WeightLink {
                                bone: mapped_index.try_into().unwrap(),
                                weight: *weight,
                            });
                        }

                        let source_transform = create_space_transform(imported_file.up, imported_file.forward).inverse();

                        let triangle_vertex = TriangleVertex {
                            position: source_transform.transform_point3(import_vertex.location),
                            normal: source_transform.transform_vector3(import_vertex.normal.normalize()),
                            texture_coordinate: Vector2::new(import_vertex.texture_coordinate.x, 1.0 - import_vertex.texture_coordinate.y), // For DirectX?
                            links: mapped_links,
                        };

                        let neighbors = vertex_tree
                            .within(&triangle_vertex.position.to_array(), super::FLOAT_TOLERANCE, &squared_euclidean)
                            .unwrap();

                        if let Some(&(_, index)) = neighbors.iter().find(|(_, i)| vertex_equals(&triangle_vertex, &triangle_list.vertices[**i])) {
                            *vertex_index = *index;
                            continue;
                        }

                        vertex_tree.add(triangle_vertex.position.to_array(), triangle_list.vertices.len()).unwrap();

                        *vertex_index = triangle_list.vertices.len();
                        triangle_list.vertices.push(triangle_vertex);
                    }

                    triangle_list.triangles.push(triangle);
                }
            }
        }
    }

    Ok(triangle_lists)
}

/// Triangulates a face into a triangles.
fn triangulate_face(face: &[usize], vertices: &[import::Vertex]) -> Vec<[usize; 3]> {
    if face.len() == 3 {
        return vec![[face[0], face[1], face[2]]];
    }

    if face.len() == 4 {
        return vec![[face[0], face[1], face[2]], [face[2], face[3], face[0]]];
    }

    // TODO: Implement a better triangulation algorithm.

    let mut triangles = Vec::new();

    let index_count = face.len();
    let mut minimum_distance = f64::MAX;
    let mut minimum_index = 0;

    for loop_index in 0..index_count {
        let mut distance = 0.0;

        let center = vertices[face[loop_index]].location;
        for distance_loop_index in 2..index_count - 1 {
            let edge_index = (loop_index + distance_loop_index) % index_count;
            let edge = vertices[face[edge_index]].location;
            distance += (edge - center).length();
        }

        if distance < minimum_distance {
            minimum_index = loop_index;
            minimum_distance = distance;
        }
    }

    for triangle_build_index in 1..index_count - 1 {
        triangles.push([
            face[minimum_index],
            face[(minimum_index + triangle_build_index) % index_count],
            face[(minimum_index + triangle_build_index + 1) % index_count],
        ]);
    }

    triangles
}

/// Compares two triangle vertices for equality.
fn vertex_equals(from: &TriangleVertex, to: &TriangleVertex) -> bool {
    if (from.normal.x - to.normal.x).abs() > super::FLOAT_TOLERANCE
        || (from.normal.y - to.normal.y).abs() > super::FLOAT_TOLERANCE
        || (from.normal.z - to.normal.z).abs() > super::FLOAT_TOLERANCE
    {
        return false;
    }

    if (from.texture_coordinate.x - to.texture_coordinate.x).abs() > super::FLOAT_TOLERANCE
        || (from.texture_coordinate.y - to.texture_coordinate.y).abs() > super::FLOAT_TOLERANCE
    {
        return false;
    }

    if from.links.len() != to.links.len() {
        return false;
    }

    if from
        .links
        .iter()
        .zip(to.links.iter())
        .any(|(from_link, to_link)| from_link.bone != to_link.bone || from_link.weight != to_link.weight)
    {
        return false;
    }

    true
}

/// Reorders the triangle vertex order to be clockwise.
fn reorder_triangle_vertex_order(triangle_list: &mut TriangleList) {
    for triangle in &mut triangle_list.triangles {
        let edge1 = triangle_list.vertices[triangle[1]].position - triangle_list.vertices[triangle[0]].position;
        let edge2 = triangle_list.vertices[triangle[2]].position - triangle_list.vertices[triangle[0]].position;
        let computed_normal = edge1.cross(edge2).normalize();
        if computed_normal.length() >= 0.0 {
            triangle.reverse();
        }
    }
}

/// Sorts the vertices to decrease the amount of strips.
fn sort_vertices_by_hardware_bones(_triangle_list: &mut TriangleList) {
    // TODO: Implement this function.
}

/// Calculates the tangents for each vertex.
fn calculate_vertex_tangents(triangle_list: &mut TriangleList) -> usize {
    let mut tangents = vec![Vector3::default(); triangle_list.vertices.len()];
    let mut bi_tangents = vec![Vector3::default(); triangle_list.vertices.len()];

    for face in &triangle_list.triangles {
        let edge1 = triangle_list.vertices[face[1]].position - triangle_list.vertices[face[0]].position;
        let edge2 = triangle_list.vertices[face[2]].position - triangle_list.vertices[face[0]].position;
        let delta_uv1 = triangle_list.vertices[face[1]].texture_coordinate - triangle_list.vertices[face[0]].texture_coordinate;
        let delta_uv2 = triangle_list.vertices[face[2]].texture_coordinate - triangle_list.vertices[face[0]].texture_coordinate;

        let denominator = delta_uv1.x * delta_uv2.y - delta_uv2.x * delta_uv1.y;

        if denominator.abs() < f64::EPSILON {
            for vertex_index in 0..3 {
                tangents[face[vertex_index]] += Vector3::new(1.0, 0.0, 0.0);
                bi_tangents[face[vertex_index]] += Vector3::new(0.0, 1.0, 0.0);
            }
            continue;
        }

        let area = 1.0 / denominator;

        let tangent = Vector3::new(
            area * (delta_uv2.y * edge1.x - delta_uv1.y * edge2.x),
            area * (delta_uv2.y * edge1.y - delta_uv1.y * edge2.y),
            area * (delta_uv2.y * edge1.z - delta_uv1.y * edge2.z),
        );

        let bi_tangent = Vector3::new(
            area * (delta_uv1.x * edge2.x - delta_uv2.x * edge1.x),
            area * (delta_uv1.x * edge2.y - delta_uv2.x * edge1.y),
            area * (delta_uv1.x * edge2.z - delta_uv2.x * edge1.z),
        );

        for vertex_index in 0..3 {
            tangents[face[vertex_index]] += tangent;
            bi_tangents[face[vertex_index]] += bi_tangent;
        }
    }

    triangle_list.tangents.reserve(triangle_list.vertices.len());
    let mut bad_vertex_count = 0;
    for index in 0..triangle_list.vertices.len() {
        let normalized_tangent = tangents[index].normalize();
        let normalized_bi_tangent = bi_tangents[index].normalize();

        let vertex_normal = triangle_list.vertices[index].normal;
        let orthogonalized_tangent = (normalized_tangent - vertex_normal * normalized_tangent.dot(vertex_normal)).normalize();

        let cross_product = vertex_normal.cross(normalized_tangent);
        let sign = if cross_product.dot(normalized_bi_tangent) < 0.0 { -1.0 } else { 1.0 };

        let vertex_tangent = Vector4::new(orthogonalized_tangent.x, orthogonalized_tangent.y, orthogonalized_tangent.z, sign);

        triangle_list.tangents.push(vertex_tangent);

        // This is what source considers a bad vertex.
        let tangent_dot = orthogonalized_tangent.dot(vertex_normal);
        if !(-0.95..=0.95).contains(&tangent_dot) {
            bad_vertex_count += 1;
        }
    }
    bad_vertex_count
}

/// Culls the weight links to a maximum of 3.
fn cull_weight_links(triangle_list: &mut TriangleList) -> usize {
    let mut culled_vertex_count = 0;

    for vertex in &mut triangle_list.vertices {
        if vertex.links.len() <= 3 {
            continue;
        }

        vertex.links.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap());
        vertex.links.truncate(3);
        culled_vertex_count += 1;
    }

    culled_vertex_count
}

/// Converts a triangle list into a list of processed meshes.
fn convert_to_meshes(
    material_index: usize,
    triangle_list: TriangleList,
    processed_bone_data: &super::BoneData,
    bounding_box: &mut BoundingBox,
    hitboxes: &mut IndexMap<u8, BoundingBox>,
) -> (Vec<super::Mesh>, usize, usize, usize) {
    let mut processed_meshes = Vec::new();

    let mut processed_mesh = super::Mesh {
        material: material_index.try_into().unwrap(),
        ..Default::default()
    };

    let mut triangle_count = 0;
    let mut vertex_count = 0;
    let mut strip_count = 0;

    let mut processed_strip_group = super::StripGroup::default();
    let mut processed_strip = super::Strip::default();

    let mut mapped_indices: IndexMap<usize, usize> = IndexMap::new();
    let mut hardware_bones: IndexSet<u8> = IndexSet::new();
    for triangle in triangle_list.triangles {
        let new_vertex_count = triangle
            .iter()
            .filter_map(|&value| if !mapped_indices.contains_key(&value) { Some(value) } else { None })
            .collect::<Vec<usize>>();

        let unique_new_vertices = new_vertex_count.iter().collect::<IndexSet<_>>();

        let new_hardware_bone_count = triangle
            .iter()
            .map(|index| &triangle_list.vertices[*index])
            .flat_map(|vertex| vertex.links.iter().filter(|link| !hardware_bones.contains(&link.bone)))
            .map(|link| link.bone)
            .collect::<Vec<u8>>();

        let unique_new_hardware_bones = new_hardware_bone_count.iter().collect::<IndexSet<_>>();

        if processed_strip_group.vertices.len() + unique_new_vertices.len() > (u16::MAX as usize + 1) {
            processed_strip_group.strips.push(processed_strip);
            processed_mesh.strip_groups.push(processed_strip_group);
            processed_meshes.push(processed_mesh);

            mapped_indices.clear();
            hardware_bones.clear();

            processed_mesh = super::Mesh {
                material: material_index.try_into().unwrap(),
                ..Default::default()
            };
            processed_strip_group = super::StripGroup::default();
            processed_strip = super::Strip::default();
            strip_count += 1;
        }

        if hardware_bones.len() + unique_new_hardware_bones.len() > super::MAX_HARDWARE_BONES_PER_STRIP {
            let new_processed_strip = super::Strip {
                indices_offset: processed_strip.indices_offset + processed_strip.indices_count,
                vertex_offset: processed_strip.vertex_offset + processed_strip.vertex_count,
                ..Default::default()
            };

            hardware_bones.clear();
            processed_strip_group.strips.push(processed_strip);
            processed_strip = new_processed_strip;
            strip_count += 1;
        }

        for index in triangle {
            if mapped_indices.contains_key(&index) {
                processed_strip_group.indices.push((*mapped_indices.get(&index).unwrap()).try_into().unwrap());
                processed_strip.indices_count += 1;
                continue;
            }

            let vertex_data = &triangle_list.vertices[index];

            let mut vertex_weights = [0.0; 3];
            let mut weight_bones = [0; 3];
            let mut weight_count = 0;

            for link in &vertex_data.links {
                debug_assert!(weight_count < 3, "Vertex has more than 3 weights!");

                // Merge links with the same bone
                if let Some(existing_link) = weight_bones.iter().take(weight_count).position(|&bone| bone == link.bone) {
                    vertex_weights[existing_link] += link.weight as f32;
                    continue;
                }

                if link.weight < super::FLOAT_TOLERANCE {
                    continue;
                }

                vertex_weights[weight_count] = link.weight as f32;
                weight_bones[weight_count] = link.bone;

                weight_count += 1;
            }

            debug_assert!(weight_count > 0, "Vertex has no weights!");

            let sum = vertex_weights.iter().take(weight_count).sum::<f32>();

            for weight in vertex_weights.iter_mut().take(weight_count) {
                *weight /= sum;
            }

            let processed_vertex = super::Vertex {
                weights: vertex_weights,
                bones: weight_bones,
                bone_count: weight_count as u8,
                position: vertex_data.position,
                normal: vertex_data.normal,
                texture_coordinate: vertex_data.texture_coordinate,
                tangent: triangle_list.tangents[index],
            };

            bounding_box.add_point(processed_vertex.position);

            for (index, link) in processed_vertex.bones.iter().take(processed_vertex.bone_count.into()).enumerate() {
                let bone = &processed_bone_data.processed_bones[*link as usize];
                let local_point = (bone.world_transform.inverse() * Matrix4::from_translation(processed_vertex.position)).translation;
                hitboxes
                    .entry(*link)
                    .or_default()
                    .add_point(local_point * processed_vertex.weights[index] as f64);
            }

            let mut processed_mesh_vertex = super::MeshVertex {
                vertex_index: processed_strip_group.vertices.len().try_into().unwrap(),
                bone_count: weight_count as u8,
                ..Default::default()
            };

            processed_strip.bone_count = if weight_count as i16 > processed_strip.bone_count {
                weight_count as i16
            } else {
                processed_strip.bone_count
            };

            for (bone_index, bone) in weight_bones.iter().enumerate().take(weight_count) {
                let (hardware_bone_index, new_hardware_bone) = hardware_bones.insert_full(*bone);

                processed_mesh_vertex.bones[bone_index] = hardware_bone_index.try_into().unwrap();
                if new_hardware_bone {
                    let processed_hardware_bone = super::HardwareBone {
                        hardware_bone: hardware_bone_index.try_into().unwrap(),
                        bone_table_bone: *bone as i32,
                    };
                    processed_strip.hardware_bones.push(processed_hardware_bone);
                }
            }

            processed_strip_group.indices.push(processed_strip_group.vertices.len().try_into().unwrap());
            mapped_indices.insert(index, processed_strip_group.vertices.len());
            processed_strip.indices_count += 1;

            processed_strip_group.vertices.push(processed_mesh_vertex);
            processed_mesh.vertex_data.push(processed_vertex);
            processed_strip.vertex_count += 1;

            vertex_count += 1;
        }

        triangle_count += 1;
    }

    processed_strip_group.strips.push(processed_strip);
    strip_count += 1;
    processed_mesh.strip_groups.push(processed_strip_group);
    processed_meshes.push(processed_mesh);

    (processed_meshes, triangle_count, vertex_count, strip_count)
}
