use indexmap::{IndexMap, IndexSet};
use kdtree::{KdTree, distance::squared_euclidean};
use std::sync::Arc;
use thiserror::Error as ThisError;

use crate::{
    import::{self, FileData, FileManager},
    input,
    utilities::mathematics::{Matrix4, Vector2, Vector3, Vector4, create_space_transform},
    verbose, warn,
};

#[derive(Debug, ThisError)]
pub enum ProcessingMeshError {
    #[error("Model \"{0}\" In Model Group \"{1}\" Is Missing File Path")]
    MissingFilePath(String, String),
    #[error("Model \"{0}\" In Model Group \"{1}\" File Is Not Loaded")]
    FileNotLoaded(String, String),
    #[error("Model Has Too Many Materials")]
    TooManyMaterials,
    #[error("Model \"{0}\" In Model Group \"{1}\" Has Too many Meshes")]
    TooManyMeshes(String, String),
    #[error("Model Has Too Many Model Groups")]
    TooManyModelGroups,
}

pub fn process_meshes(
    input_data: &input::SourceInput,
    source_files: &FileManager,
    processed_bone_data: &super::BoneData,
) -> Result<super::ModelData, ProcessingMeshError> {
    let mut model_data = super::ModelData::default();

    for input_model_group in &input_data.model_groups {
        let mut processed_model_group = super::ModelGroup::default();

        for input_model in &input_model_group.models {
            let mut processed_model = super::Model::default();
            if input_model.blank {
                continue;
            }

            let import_file = source_files
                .get_file_data(
                    input_model
                        .source_file_path
                        .as_ref()
                        .ok_or(ProcessingMeshError::MissingFilePath(input_model.name.clone(), input_model_group.name.clone()))?,
                )
                .ok_or(ProcessingMeshError::FileNotLoaded(input_model.name.clone(), input_model_group.name.clone()))?;

            let triangle_lists = create_triangle_lists(Arc::clone(&import_file), &mut model_data, input_model);
            if model_data.materials.len() > (i16::MAX as usize) + 1 {
                return Err(ProcessingMeshError::TooManyMaterials);
            }

            let mut vertex_link_cull_count = 0;
            let mut vertex_count = 0;
            let mut triangle_count = 0;
            for (material_index, mut triangle_list) in triangle_lists {
                debug_assert!(!triangle_list.triangles.is_empty());
                vertices_remap_links(&mut triangle_list, Arc::clone(&import_file), processed_bone_data, &mut vertex_link_cull_count);
                optimize_merge_vertices(&mut triangle_list);
                optimize_vertex_cache(&mut triangle_list);
                update_bounding_boxes(&triangle_list, &mut model_data, processed_bone_data);
                let vertex_tangents = calculate_vertex_tangents(&triangle_list);
                let processed_meshes = finalize_triangle_list(material_index, triangle_list, vertex_tangents, &mut vertex_count, &mut triangle_count);
                processed_model.meshes.extend(processed_meshes);
                if processed_model.meshes.len() > (i32::MAX as usize) + 1 {
                    return Err(ProcessingMeshError::TooManyMeshes(input_model.name.clone(), input_model_group.name.clone()));
                }
            }

            if vertex_link_cull_count > 0 {
                warn!(
                    "Culled {} Vertices Weight Link's For Model \"{}\" In Model Group \"{}\"!",
                    vertex_link_cull_count, input_model.name, input_model_group.name
                );
            }

            verbose!(
                "Model \"{}\" in model group \"{}\" has {} triangles with {} vertices",
                input_model.name,
                input_model_group.name,
                triangle_count,
                vertex_count
            );

            processed_model_group.models.insert(input_model.name.clone(), processed_model);
        }
        model_data.model_groups.insert(input_model_group.name.clone(), processed_model_group);

        if model_data.model_groups.len() > (i32::MAX as usize) + 1 {
            return Err(ProcessingMeshError::TooManyModelGroups);
        }
    }

    // Add bones to the size of the bounding box
    for processed_bone in processed_bone_data.processed_bones.values() {
        model_data.bounding_box.add_point(processed_bone.world_transform.translation);
    }

    Ok(model_data)
}

#[derive(Default)]
struct TriangleList {
    vertices: Vec<TriangleVertex>,
    triangles: Vec<[usize; 3]>,
}

struct TriangleVertex {
    location: Vector3,
    normal: Vector3,
    texture_coordinate: Vector2,
    links: Vec<TriangleVertexLink>,
}

struct TriangleVertexLink {
    bone: usize,
    weight: f64,
}

/// Create triangle lists structures for a model.
fn create_triangle_lists(import_file: Arc<FileData>, model_data: &mut super::ModelData, processed_model: &input::Model) -> IndexMap<usize, TriangleList> {
    let mut triangle_lists = IndexMap::new();

    for (import_part_name, import_part) in &import_file.parts {
        if processed_model.disabled_parts.contains(import_part_name) {
            continue;
        }

        for (material, faces) in &import_part.faces {
            let (material_index, _) = model_data.materials.insert_full(material.clone());
            let triangle_list: &mut TriangleList = triangle_lists.entry(material_index).or_default();

            for face in faces {
                debug_assert!(face.len() >= 3, "Imported File Has A Face With Less Than 3 Vertices!");
                let triangulated_face = triangulate_face(face, &import_part.vertices);
                for face_triangle in triangulated_face {
                    let mut triangle = [0; 3];
                    for (index_index, vertex_index) in face_triangle.into_iter().enumerate() {
                        let import_vertex = &import_part.vertices[vertex_index];
                        let space_transform = create_space_transform(import_file.up, import_file.forward).inverse();
                        let vertex = TriangleVertex {
                            location: space_transform.transform_point3(import_vertex.location),
                            normal: space_transform.transform_vector3(import_vertex.normal),
                            texture_coordinate: Vector2::new(import_vertex.texture_coordinate.x, 1.0 - import_vertex.texture_coordinate.y), // For DirectX?
                            links: import_vertex.links.iter().map(|(&bone, &weight)| TriangleVertexLink { bone, weight }).collect(),
                        };
                        triangle[index_index] = triangle_list.vertices.len();
                        triangle_list.vertices.push(vertex);
                    }
                    triangle_list.triangles.push(triangle);
                }
            }
        }
    }

    triangle_lists
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

/// Remaps the import files vertex links to the processed bones.
fn vertices_remap_links(
    triangle_list: &mut TriangleList,
    import_file: Arc<FileData>,
    processed_bone_data: &super::BoneData,
    vertex_link_cull_count: &mut usize,
) {
    // TODO: Transforms should take into account define bones.
    let mut import_bone_transforms = Vec::with_capacity(import_file.skeleton.len());
    let mut import_bone_processed_bone_mapping = Vec::with_capacity(import_file.skeleton.len());
    for (import_bone_name, import_bone) in &import_file.skeleton {
        import_bone_processed_bone_mapping.push(processed_bone_data.processed_bones.get_index_of(import_bone_name));
        if let Some(parent_transform) = import_bone.parent.map(|parent_index| import_bone_transforms[parent_index]) {
            let import_bone_transform = Matrix4::from_rotation_translation(import_bone.rotation, import_bone.location);
            import_bone_transforms.push(parent_transform * import_bone_transform);
            continue;
        }
        let space_transform = create_space_transform(import_file.up, import_file.forward).inverse();
        let import_bone_transform = Matrix4::from_rotation_translation(import_bone.rotation, import_bone.location);
        import_bone_transforms.push(space_transform * import_bone_transform);
    }

    let mut import_bone_remap = Vec::with_capacity(import_file.skeleton.len());
    for (import_bone_index, import_bone) in import_file.skeleton.values().enumerate() {
        if let Some(processed_bone_index) = import_bone_processed_bone_mapping[import_bone_index] {
            import_bone_remap.push(processed_bone_index);
            continue;
        }

        let mut import_bone_parent = import_bone.parent;

        // TODO: Should this take into account bone hierarchy define bones?
        loop {
            if let Some(import_bone_parent_index) = import_bone_parent {
                if let Some(processed_bone_index) = import_bone_processed_bone_mapping[import_bone_parent_index] {
                    import_bone_remap.push(processed_bone_index);
                    break;
                }

                import_bone_parent = import_file.skeleton[import_bone_parent_index].parent;
                continue;
            }
            import_bone_remap.push(0);
            break;
        }
    }

    for vertex in &mut triangle_list.vertices {
        // Map links
        vertex.links.iter_mut().for_each(|link| link.bone = import_bone_remap[link.bone]);
        // Merge links
        let mut unique_links = IndexMap::new();
        for link in &vertex.links {
            *unique_links.entry(link.bone).or_insert(0.0) += link.weight;
        }
        vertex.links = unique_links.into_iter().map(|(bone, weight)| TriangleVertexLink { bone, weight }).collect();
        // TODO: Transform Vertex
        // Limit links
        vertex.links.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Less));
        if vertex.links.len() > 3 {
            *vertex_link_cull_count += 1;
        }
        vertex.links.truncate(3);
        // Normalize links
        let weight_sum = vertex.links.iter().map(|link| link.weight).sum::<f64>();
        debug_assert!(weight_sum > super::FLOAT_TOLERANCE);
        for link in &mut vertex.links {
            link.weight /= weight_sum;
        }
        vertex.links.sort_by(|a, b| a.bone.cmp(&b.bone));
    }
}

/// Merges similar vertices together.
fn optimize_merge_vertices(triangle_list: &mut TriangleList) {
    let mut unique_vertices = Vec::new();
    let mut indices_remap = Vec::with_capacity(triangle_list.vertices.len());
    let mut vertex_tree = KdTree::new(3);
    'vertices: for vertex in triangle_list.vertices.drain(..) {
        if let Ok(neighbors) = vertex_tree.within(&vertex.location.to_array(), super::FLOAT_TOLERANCE, &squared_euclidean) {
            for (_, &neighbor) in neighbors {
                if vertex_equals(&vertex, &unique_vertices[neighbor]) {
                    indices_remap.push(neighbor);
                    continue 'vertices;
                }
            }
        }
        indices_remap.push(unique_vertices.len());
        let _ = vertex_tree.add(vertex.location.to_array(), unique_vertices.len());
        unique_vertices.push(vertex);
    }

    triangle_list.vertices = unique_vertices;

    for triangle in &mut triangle_list.triangles {
        for index in triangle {
            *index = indices_remap[*index];
        }
    }
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

/// Translation of https://github.com/zeux/meshoptimizer/blob/73583c335e541c139821d0de2bf5f12960a04941/src/vcacheoptimizer.cpp#L169
fn optimize_vertex_cache(triangle_list: &mut TriangleList) {
    let triangle_count = triangle_list.triangles.len();
    let index_count = triangle_count * 3;
    let vertex_count = triangle_list.vertices.len();

    struct TriangleAdjacency {
        counts: Vec<usize>,
        offsets: Vec<usize>,
        data: Vec<usize>,
    }

    let mut adjacency = TriangleAdjacency {
        counts: vec![0; vertex_count],
        offsets: vec![0; vertex_count],
        data: vec![0; index_count],
    };

    for triangle in &triangle_list.triangles {
        for &vertex_index in triangle {
            adjacency.counts[vertex_index] += 1;
        }
    }

    let mut offset = 0;
    for vertex_index in 0..vertex_count {
        adjacency.offsets[vertex_index] = offset;
        offset += adjacency.counts[vertex_index];
    }

    for (triangle_index, triangle) in triangle_list.triangles.iter().enumerate() {
        for &index in triangle {
            adjacency.data[adjacency.offsets[index]] = triangle_index;
            adjacency.offsets[index] += 1;
        }
    }

    for vertex_index in 0..vertex_count {
        adjacency.offsets[vertex_index] -= adjacency.counts[vertex_index];
    }

    const CACHE_SIZE: usize = 16;
    const VALENCE_SIZE: usize = 8;
    const CACHE_SCORES: [f64; CACHE_SIZE + 1] = [
        0.0, 0.779, 0.791, 0.789, 0.981, 0.843, 0.726, 0.847, 0.882, 0.867, 0.799, 0.642, 0.613, 0.600, 0.568, 0.372, 0.234,
    ];
    const VALENCE_SCORES: [f64; VALENCE_SIZE + 1] = [0.0, 0.995, 0.713, 0.450, 0.404, 0.059, 0.005, 0.147, 0.006];

    let mut vertex_scores = vec![0.0; vertex_count];
    for vertex_index in 0..vertex_count {
        vertex_scores[vertex_index] = VALENCE_SCORES[adjacency.counts[vertex_index].min(VALENCE_SIZE)]
    }

    let mut triangle_scores = vec![0.0; triangle_count];
    for (triangle_index, triangle) in triangle_list.triangles.iter().enumerate() {
        for &index in triangle {
            triangle_scores[triangle_index] += vertex_scores[index];
        }
    }

    let mut triangle_optimized = vec![false; triangle_count];

    let mut destination = Vec::with_capacity(triangle_count);

    let mut cache = [0; CACHE_SIZE + 4];
    let mut cache_count = 0;

    let mut current_triangle_index = 0;
    let mut input_cursor = 1;

    loop {
        let current_triangle = triangle_list.triangles[current_triangle_index];
        destination.push(current_triangle);

        triangle_optimized[current_triangle_index] = true;
        triangle_scores[current_triangle_index] = 0.0;

        let mut cache_write = 0;
        let mut cache_new = [0; CACHE_SIZE + 4];
        cache_new[cache_write] = current_triangle[0];
        cache_write += 1;
        cache_new[cache_write] = current_triangle[1];
        cache_write += 1;
        cache_new[cache_write] = current_triangle[2];
        cache_write += 1;

        for &cached_index in cache.iter().take(cache_count) {
            cache_new[cache_write] = cached_index;
            if cached_index != current_triangle[0] && cached_index != current_triangle[1] && cached_index != current_triangle[2] {
                cache_write += 1;
            }
        }

        cache = cache_new;
        cache_count = cache_write.min(CACHE_SIZE);

        for vertex_index in current_triangle {
            let neighbors_start = adjacency.offsets[vertex_index];
            let neighbors_end = neighbors_start + adjacency.counts[vertex_index];
            let neighbor_last = adjacency.data[neighbors_end - 1];
            let neighbors = &mut adjacency.data[neighbors_start..neighbors_end];
            for neighbor_triangle in neighbors {
                if *neighbor_triangle == current_triangle_index {
                    *neighbor_triangle = neighbor_last;
                    adjacency.counts[vertex_index] -= 1;
                    break;
                }
            }
        }

        let mut best_triangle = None;
        let mut best_score = 0.0;

        for (cache_index, &cached_index) in cache.iter().take(cache_write).enumerate() {
            if adjacency.counts[cached_index] == 0 {
                continue;
            }

            let cache_position = if cache_index < CACHE_SIZE { cache_index + 1 } else { 0 };
            let score = CACHE_SCORES[cache_position] + VALENCE_SCORES[adjacency.counts[cached_index].min(VALENCE_SIZE)];
            let score_difference = score - vertex_scores[cached_index];

            vertex_scores[cached_index] = score;

            let neighbors_start = adjacency.offsets[cached_index];
            let neighbors_end = neighbors_start + adjacency.counts[cached_index];
            let neighbors = &adjacency.data[neighbors_start..neighbors_end];
            for &neighbor in neighbors {
                let neighbor_score = triangle_scores[neighbor] + score_difference;

                if best_score < neighbor_score {
                    best_triangle = Some(neighbor);
                    best_score = neighbor_score;
                }

                triangle_scores[neighbor] = neighbor_score;
            }
        }

        if best_triangle.is_none() {
            while input_cursor < triangle_count {
                if !triangle_optimized[input_cursor] {
                    best_triangle = Some(input_cursor);
                    break;
                }
                input_cursor += 1;
            }
        }

        if let Some(next_triangle) = best_triangle {
            current_triangle_index = next_triangle;
            continue;
        }

        break;
    }

    triangle_list.triangles = destination;
}

/// Increases model bounding box and bone bounding boxes size with the vertices.
fn update_bounding_boxes(triangle_list: &TriangleList, model_data: &mut super::ModelData, processed_bone_data: &super::BoneData) {
    for vertex in &triangle_list.vertices {
        model_data.bounding_box.add_point(vertex.location);

        for link in &vertex.links {
            let bone = &processed_bone_data.processed_bones[link.bone];
            let local_location = (bone.world_transform.inverse() * Matrix4::from_translation(vertex.location)).translation;
            model_data.hitboxes.entry(link.bone).or_default().add_point(local_location * link.weight);
        }
    }
}

/// Calculates vertex tangents for a triangle list.
fn calculate_vertex_tangents(triangle_list: &TriangleList) -> Vec<Vector4> {
    let mut tangents = vec![Vector3::default(); triangle_list.vertices.len()];
    let mut bi_tangents = vec![Vector3::default(); triangle_list.vertices.len()];

    for face in &triangle_list.triangles {
        let edge1 = triangle_list.vertices[face[1]].location - triangle_list.vertices[face[0]].location;
        let edge2 = triangle_list.vertices[face[2]].location - triangle_list.vertices[face[0]].location;
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

    let mut vertex_tangents = Vec::with_capacity(triangle_list.vertices.len());
    for index in 0..triangle_list.vertices.len() {
        let normalized_tangent = tangents[index].normalize();
        let normalized_bi_tangent = bi_tangents[index].normalize();

        let vertex_normal = triangle_list.vertices[index].normal;
        let orthogonalized_tangent = (normalized_tangent - vertex_normal * normalized_tangent.dot(vertex_normal)).normalize();

        let cross_product = vertex_normal.cross(normalized_tangent);
        let sign = if cross_product.dot(normalized_bi_tangent) < 0.0 { -1.0 } else { 1.0 };

        let vertex_tangent = Vector4::new(orthogonalized_tangent.x, orthogonalized_tangent.y, orthogonalized_tangent.z, sign);

        vertex_tangents.push(vertex_tangent);
    }
    vertex_tangents
}

/// Finalizes a triangle list to a processed mesh
fn finalize_triangle_list(
    material_index: usize,
    triangle_list: TriangleList,
    vertex_tangents: Vec<Vector4>,
    vertex_count: &mut usize,
    triangle_count: &mut usize,
) -> Vec<super::Mesh> {
    let mut processed_meshes = Vec::new();

    let mut processed_mesh = super::Mesh {
        material: material_index as i32,
        ..Default::default()
    };
    let mut processed_strip_group = super::StripGroup::default();
    let mut processed_strip = super::Strip::default();

    let mut mapped_indices: IndexMap<usize, usize> = IndexMap::new();
    let mut hardware_bones: IndexSet<usize> = IndexSet::new();
    for triangle in triangle_list.triangles {
        let new_unique_indices = IndexSet::<usize>::from_iter(triangle.iter().cloned());
        let new_indices_count = new_unique_indices.iter().filter(|&index| !mapped_indices.contains_key(index)).count();
        if mapped_indices.len() + new_indices_count > (u16::MAX as usize + 1) {
            processed_strip_group.strips.push(processed_strip);
            processed_mesh.strip_groups.push(processed_strip_group);
            processed_meshes.push(processed_mesh);

            mapped_indices.clear();
            hardware_bones.clear();
            processed_strip = super::Strip::default();
            processed_strip_group = super::StripGroup::default();
            processed_mesh = super::Mesh {
                material: material_index as i32,
                ..Default::default()
            };
        }

        let new_hardware_bone_count = new_unique_indices
            .iter()
            .flat_map(|&index| &triangle_list.vertices[index].links)
            .map(|link| link.bone)
            .filter(|bone| !hardware_bones.contains(bone))
            .count();

        if hardware_bones.len() + new_hardware_bone_count > super::MAX_HARDWARE_BONES_PER_STRIP {
            // FIXME: If a index gets a vertex in a different strip then it uses the bone table of the current strip and not the different strip.
            // Mesh processing should handle this but as of right now it doesn't.
            // For now we just create a new strip group, this a bad way as it creates duplicate vertices.
            processed_strip_group.strips.push(processed_strip);
            processed_mesh.strip_groups.push(processed_strip_group);
            processed_meshes.push(processed_mesh);

            mapped_indices.clear();
            hardware_bones.clear();
            processed_strip = super::Strip::default();
            processed_strip_group = super::StripGroup::default();
            processed_mesh = super::Mesh {
                material: material_index as i32,
                ..Default::default()
            };

            // This is the proper way it should be handled
            // let new_processed_strip = super::Strip {
            //     indices_offset: processed_strip.indices_offset + processed_strip.indices_count,
            //     vertex_offset: processed_strip.vertex_offset + processed_strip.vertex_count,
            //     ..Default::default()
            // };

            // hardware_bones.clear();
            // processed_strip_group.strips.push(processed_strip);
            // processed_strip = new_processed_strip;
        }

        for index in triangle {
            if let Some(&mapped_index) = mapped_indices.get(&index) {
                processed_strip_group.indices.push(mapped_index as u16);
                processed_strip.indices_count += 1;
                continue;
            }

            let vertex_data = &triangle_list.vertices[index];

            debug_assert!(!vertex_data.links.is_empty() && vertex_data.links.len() <= 3);
            let weight_count = vertex_data.links.len() as u8;
            let mut vertex_weights = [0.0; 3];
            let mut weight_bones = [0; 3];
            for (link_index, link) in vertex_data.links.iter().enumerate() {
                vertex_weights[link_index] = link.weight as f32;
                debug_assert!(link.bone <= u8::MAX as usize);
                weight_bones[link_index] = link.bone as u8;
            }

            let processed_vertex = super::Vertex {
                weights: vertex_weights,
                bones: weight_bones,
                bone_count: weight_count,
                position: vertex_data.location,
                normal: vertex_data.normal,
                texture_coordinate: vertex_data.texture_coordinate,
                tangent: vertex_tangents[index],
            };

            let mut processed_mesh_vertex = super::MeshVertex {
                vertex_index: processed_strip_group.vertices.len() as u16,
                bone_count: weight_count,
                ..Default::default()
            };

            processed_strip.bone_count = processed_strip.bone_count.max(weight_count as i16);

            for (link_index, link) in vertex_data.links.iter().enumerate() {
                let (hardware_bone_index, new_hardware_bone) = hardware_bones.insert_full(link.bone);

                processed_mesh_vertex.bones[link_index] = hardware_bone_index as u8;
                if new_hardware_bone {
                    let processed_hardware_bone = super::HardwareBone {
                        hardware_bone: hardware_bone_index as i32,
                        bone_table_bone: link.bone as i32,
                    };
                    processed_strip.hardware_bones.push(processed_hardware_bone);
                }
            }

            processed_strip_group.indices.push(processed_strip_group.vertices.len() as u16);
            mapped_indices.insert(index, processed_strip_group.vertices.len());
            processed_strip.indices_count += 1;

            processed_strip_group.vertices.push(processed_mesh_vertex);
            processed_mesh.vertex_data.push(processed_vertex);
            processed_strip.vertex_count += 1;
            *vertex_count += 1;
        }
        *triangle_count += 1;
    }

    processed_strip_group.strips.push(processed_strip);
    processed_mesh.strip_groups.push(processed_strip_group);
    processed_meshes.push(processed_mesh);

    processed_meshes
}
