use crate::{
    import::{self, FileData, FileManager},
    input,
    utilities::mathematics::{Matrix4, Vector2, Vector3, Vector4, create_space_transform},
    verbose, warn,
};
use indexmap::{IndexMap, IndexSet};
use smallvec::SmallVec;
use std::sync::Arc;
use thiserror::Error as ThisError;

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

    let mut flex_controller_remap = IndexMap::new();
    for input_flex_controller in &input_data.flex_controllers {
        flex_controller_remap.insert(input_flex_controller.identifier, model_data.flex_data.controllers.len());
        model_data.flex_data.controllers.push(input_flex_controller.name.clone());
    }

    let mut flex_key_remap = IndexMap::new();
    for input_flex_key in &input_data.flex_keys {
        flex_key_remap.insert(input_flex_key.identifier, model_data.flex_data.keys.len());
        if let Some(&assigned_controller) = flex_controller_remap.get(&input_flex_key.assigned_controller) {
            model_data.flex_data.keys.push((input_flex_key.name.clone(), assigned_controller));
        } else {
            warn!("Flex Key {} Does Not Have A Controller Assigned", input_flex_key.name)
        }
    }

    for input_model_group in &input_data.model_groups {
        let processed_model_group = process_model_group(input_model_group, source_files, &mut model_data, processed_bone_data, &flex_key_remap)?;
        model_data.model_groups.insert(input_model_group.name.clone(), processed_model_group);
        if model_data.model_groups.len() > (i32::MAX as usize) + 1 {
            return Err(ProcessingMeshError::TooManyModelGroups);
        }
    }

    // TODO: Get the Max of all models groups flex count and sum them. If the sum is greater than the max flex count then need to cull them.

    Ok(model_data)
}

fn process_model_group(
    input_model_group: &input::ModelGroup,
    source_files: &FileManager,
    model_data: &mut super::ModelData,
    processed_bone_data: &super::BoneData,
    flex_key_remap: &IndexMap<usize, usize>,
) -> Result<super::ModelGroup, ProcessingMeshError> {
    let mut model_group_models = IndexMap::with_capacity(input_model_group.models.len());
    for input_model in &input_model_group.models {
        let mut processed_model = super::Model::default();
        if input_model.blank {
            model_group_models.insert(input_model.name.clone(), processed_model);
            continue;
        }

        let input_model_file_source = load_file_source(source_files, input_model, &input_model_group.name)?;

        let mut triangle_count = 0;
        let mut vertex_count = 0;
        let mut strip_count = 0;
        let triangle_meshes = create_triangle_meshes(&input_model_file_source, model_data, input_model, processed_bone_data, flex_key_remap)?;
        for (triangle_mesh_material_index, mut triangle_mesh) in triangle_meshes {
            mesh_optimize_flexed_vertices(&mut triangle_mesh);
            mesh_merge_vertices(&mut triangle_mesh);
            mesh_create_bounding_boxes(&triangle_mesh, processed_bone_data, model_data);
            let mesh_vertex_tangents = calculate_mesh_vertex_tangents(&triangle_mesh);

            for mut triangle_strip in create_triangle_strips(triangle_mesh, mesh_vertex_tangents) {
                strip_optimize_vertex_cache(&mut triangle_strip);
                // strip_optimize_overdraw(&mut triangle_strip);
                // strip_optimize_bone_table(&mut triangle_strip);

                let (finalized_strip, added_triangles, added_vertices, added_strips) = strip_finalize(triangle_strip, triangle_mesh_material_index);
                triangle_count += added_triangles;
                vertex_count += added_vertices;
                strip_count += added_strips;
                processed_model.meshes.push(finalized_strip);
                if processed_model.meshes.len() > (i32::MAX as usize) + 1 {
                    return Err(ProcessingMeshError::TooManyMeshes(input_model.name.clone(), input_model_group.name.clone()));
                }
            }
        }

        verbose!(
            "Model \"{}\" in model group \"{}\" has {} triangles, {} vertices, and {} strips.",
            input_model.name.clone(),
            input_model_group.name.clone(),
            triangle_count,
            vertex_count,
            strip_count
        );

        model_group_models.insert(input_model.name.clone(), processed_model);
    }
    Ok(super::ModelGroup { models: model_group_models })
}

fn load_file_source(source_files: &FileManager, input_model: &input::Model, input_model_group_name: &str) -> Result<Arc<FileData>, ProcessingMeshError> {
    let Some(input_model_file_source_path) = input_model.source_file_path.as_ref() else {
        return Err(ProcessingMeshError::MissingFilePath(
            input_model.name.clone(),
            input_model_group_name.to_owned(),
        ));
    };
    let Some(input_model_file_source) = source_files.get_file_data(input_model_file_source_path) else {
        return Err(ProcessingMeshError::FileNotLoaded(input_model.name.clone(), input_model_group_name.to_owned()));
    };
    Ok(input_model_file_source)
}

const MAX_VERTEX_LINKS: usize = 3;

#[derive(Clone, Copy)]
struct VertexLink {
    bone: usize,
    weight: f32,
}

#[derive(Clone, Copy)]
struct VertexFlex {
    position: Vector3,
    normal: Vector3,
}

struct ListVertex {
    position: Vector3,
    normal: Vector3,
    texture_coordinate: Vector2,
    tangent: Vector4,
    links: SmallVec<[VertexLink; MAX_VERTEX_LINKS]>,
    flex: IndexMap<usize, VertexFlex>,
}

#[derive(Default)]
struct Mesh;

#[derive(Default)]
struct Strip;

type Triangle = [usize; 3];

#[derive(Default)]
struct TriangleList<State> {
    vertices: Vec<ListVertex>,
    triangles: Vec<Triangle>,
    state: std::marker::PhantomData<State>,
}

fn create_triangle_meshes(
    file_source: &Arc<FileData>,
    model_data: &mut super::ModelData,
    input_model: &input::Model,
    processed_bone_data: &super::BoneData,
    flex_key_remap: &IndexMap<usize, usize>,
) -> Result<IndexMap<usize, TriangleList<Mesh>>, ProcessingMeshError> {
    let material_count = file_source.parts.iter().fold(
        0,
        |count, (part_name, _)| {
            if input_model.disabled_parts.contains(part_name) { count + 1 } else { count }
        },
    );
    let mut triangle_meshes = IndexMap::with_capacity(material_count);

    let link_remap = create_link_remap_table(file_source, processed_bone_data);
    let space_transform = create_space_transform(file_source.up, file_source.forward).inverse();
    for (import_part_name, import_part) in &file_source.parts {
        if input_model.disabled_parts.contains(import_part_name) {
            continue;
        }

        for (material, faces) in &import_part.faces {
            let (material_index, _) = model_data.materials.insert_full(material.clone());
            if model_data.materials.len() > (i16::MAX as usize) + 1 {
                return Err(ProcessingMeshError::TooManyMaterials);
            }
            let mesh = triangle_meshes.entry(material_index).or_insert(TriangleList::default());

            let triangulated_mesh = faces.iter().flat_map(|face| triangulate_face(face, &import_part.vertices)).collect::<Vec<_>>();
            let input_part_flex_data = input_model.flexes.get(import_part_name);
            let (triangle_vertices, triangles) = gather_triangulated_mesh_vertices(
                triangulated_mesh,
                input_part_flex_data,
                import_part,
                flex_key_remap,
                mesh.vertices.len(),
                &link_remap,
                &space_transform,
            );

            mesh.vertices.extend(triangle_vertices);
            mesh.triangles.extend(triangles);
        }
    }

    Ok(triangle_meshes)
}

// FIXME: This should be done in the bone processing stage!
fn create_link_remap_table(file_source: &Arc<FileData>, processed_bone_data: &super::BoneData) -> Vec<usize> {
    let mut import_bone_remap = Vec::with_capacity(file_source.skeleton.len());

    for (import_bone_name, import_bone) in &file_source.skeleton {
        if let Some(processed_bone_index) = processed_bone_data.processed_bones.get_index_of(import_bone_name) {
            import_bone_remap.push(processed_bone_index);
            continue;
        }

        import_bone_remap.push(get_parent_link(import_bone, file_source, processed_bone_data));
    }

    import_bone_remap
}

fn get_parent_link(import_bone: &import::Bone, file_source: &Arc<FileData>, processed_bone_data: &super::BoneData) -> usize {
    if let Some(import_bone_parent_index) = import_bone.parent
        && let Some((import_bone_parent_name, import_bone_parent)) = file_source.skeleton.get_index(import_bone_parent_index)
    {
        if let Some(processed_bone_index) = processed_bone_data.processed_bones.get_index_of(import_bone_parent_name) {
            return processed_bone_index;
        }
        return get_parent_link(import_bone_parent, file_source, processed_bone_data);
    }

    0
}

fn triangulate_face(face: &[usize], vertices: &[import::Vertex]) -> Vec<Triangle> {
    if face.len() == 3 {
        return vec![[face[0], face[1], face[2]]];
    }

    if face.len() == 4 {
        return vec![[face[0], face[1], face[2]], [face[2], face[3], face[0]]];
    }

    // TODO: Implement a better triangulation algorithm.

    let mut triangles = Vec::new();

    let index_count = face.len();
    let mut minimum_distance = f32::MAX;
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

fn gather_triangulated_mesh_vertices(
    triangulated_mesh: Vec<Triangle>,
    input_part_flex_data: Option<&IndexMap<String, input::Flex>>,
    import_part: &import::Part,
    flex_key_remap: &IndexMap<usize, usize>,
    vertex_offset: usize,
    link_remap: &[usize],
    space_transform: &Matrix4,
) -> (Vec<ListVertex>, Vec<Triangle>) {
    let mut triangle_vertices = Vec::with_capacity(triangulated_mesh.len() * 3);
    let mut triangles = Vec::with_capacity(triangulated_mesh.len());

    let mut current_triangle = [0; 3];
    for vertex_index in triangulated_mesh.into_flattened() {
        let import_vertex = &import_part.vertices[vertex_index];

        let position = space_transform.transform_point3(import_vertex.location);
        let normal = space_transform.transform_vector3(import_vertex.normal);
        let texture_coordinate = Vector2::new(import_vertex.texture_coordinate.x, 1.0 - import_vertex.texture_coordinate.y);
        let links = remap_vertex_links(link_remap, import_vertex);
        let flex = gather_vertex_flexes(
            import_part,
            input_part_flex_data,
            vertex_index,
            position,
            normal,
            space_transform,
            flex_key_remap,
        );

        let triangle_vertex_index = vertex_offset + triangle_vertices.len();
        let triangle_index_position = triangle_vertex_index % 3;
        current_triangle[triangle_index_position] = triangle_vertex_index;
        if triangle_index_position == 2 {
            triangles.push(current_triangle);
        }

        triangle_vertices.push(ListVertex {
            position,
            normal,
            texture_coordinate,
            tangent: Vector4::ZERO,
            links,
            flex,
        });
    }

    (triangle_vertices, triangles)
}

fn remap_vertex_links(link_remap: &[usize], vertex: &import::Vertex) -> SmallVec<[VertexLink; MAX_VERTEX_LINKS]> {
    let mut remapped_links = vertex
        .links
        .iter()
        .map(|(&link, &weight)| VertexLink {
            bone: link_remap[link],
            weight,
        })
        .collect::<Vec<_>>();
    remapped_links.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Less));

    let mut vertex_links = SmallVec::<[VertexLink; 3]>::new();
    for mapped_link in remapped_links {
        if let Some(existing_link) = vertex_links.iter_mut().find(|link| link.bone == mapped_link.bone) {
            existing_link.weight += mapped_link.weight;
            continue;
        }
        if vertex_links.len() == MAX_VERTEX_LINKS {
            continue;
        }
        vertex_links.push(mapped_link);
    }

    let weight_sum = vertex_links.iter().map(|link| link.weight).sum::<f32>();
    for vertex_link in &mut vertex_links {
        vertex_link.weight /= weight_sum;
    }

    vertex_links
}

fn gather_vertex_flexes(
    import_part: &import::Part,
    input_part_flex_data: Option<&IndexMap<String, input::Flex>>,
    vertex_index: usize,
    vertex_position: Vector3,
    vertex_normal: Vector3,
    space_transform: &Matrix4,
    flex_key_remap: &IndexMap<usize, usize>,
) -> IndexMap<usize, VertexFlex> {
    let Some(part_flex_data) = input_part_flex_data else {
        return IndexMap::new();
    };

    let mut flexes = IndexMap::new();
    for (import_flex_name, import_flex) in &import_part.flexes {
        let Some(flexed_vertex) = import_flex.get(&vertex_index) else {
            continue;
        };
        let Some(&mapped_assigned_flex_key) = part_flex_data
            .get(import_flex_name)
            .and_then(|input_flex_data| input_flex_data.assigned_flex_key.as_ref())
            .and_then(|assigned_flex_key| flex_key_remap.get(assigned_flex_key))
        else {
            continue;
        };
        let flex = flexes.entry(mapped_assigned_flex_key).or_insert(VertexFlex {
            position: Vector3::ZERO,
            normal: Vector3::ZERO,
        });
        flex.position += space_transform.transform_point3(flexed_vertex.location) - vertex_position;
        flex.normal += space_transform.transform_vector3(flexed_vertex.normal) - vertex_normal;
    }

    flexes
}

fn mesh_optimize_flexed_vertices(mesh: &mut TriangleList<Mesh>) {
    let flex_scale = compute_flex_scale(mesh);
    mesh.vertices.iter_mut().for_each(|vertex| {
        vertex.flex.retain(|_, flex| {
            let position_compressed_x = (flex.position.x / flex_scale) as i16;
            let position_compressed_y = (flex.position.y / flex_scale) as i16;
            let position_compressed_z = (flex.position.z / flex_scale) as i16;
            let no_position_delta = position_compressed_x != 0 || position_compressed_y != 0 || position_compressed_z != 0;
            let normal_compressed_x = (flex.normal.x / flex_scale) as i16;
            let normal_compressed_y = (flex.normal.y / flex_scale) as i16;
            let normal_compressed_z = (flex.normal.z / flex_scale) as i16;
            let no_normal_delta = normal_compressed_x != 0 || normal_compressed_y != 0 || normal_compressed_z != 0;
            no_position_delta || no_normal_delta
        });
    });
}

fn compute_flex_scale(mesh: &TriangleList<Mesh>) -> f32 {
    let mut max_value = 0.0f32;
    for vertex in &mesh.vertices {
        for flexed_vertex in vertex.flex.values() {
            let position_delta = flexed_vertex.position - vertex.position;
            let normal_delta = flexed_vertex.normal - vertex.normal;
            let position_max = position_delta.max_element();
            let normal_max = normal_delta.max_element();
            max_value = max_value.max(position_max);
            max_value = max_value.max(normal_max);
        }
    }
    max_value / i16::MAX as f32
}

fn mesh_merge_vertices(mesh: &mut TriangleList<Mesh>) {
    #[derive(Eq, Hash, PartialEq)]
    struct PositionCell {
        x: isize,
        y: isize,
        z: isize,
    }

    let mut vertex_remap = Vec::with_capacity(mesh.vertices.len());
    let mut vertex_grid = IndexMap::with_capacity(mesh.vertices.len());
    for vertex in std::mem::take(&mut mesh.vertices) {
        let cell = PositionCell {
            x: (vertex.position.x / 0.000001).floor() as isize,
            y: (vertex.position.y / 0.000001).floor() as isize,
            z: (vertex.position.z / 0.000001).floor() as isize,
        };
        let grid_cell = vertex_grid.entry(cell).or_insert(Vec::with_capacity(4));
        if let Some(&equal_index) = grid_cell.iter().find(|&&neighbor| vertex_equals(&vertex, &mesh.vertices[neighbor])) {
            vertex_remap.push(equal_index);
            continue;
        }
        vertex_remap.push(mesh.vertices.len());
        grid_cell.push(mesh.vertices.len());
        mesh.vertices.push(vertex);
    }

    for triangle in &mut mesh.triangles {
        for index in triangle {
            *index = vertex_remap[*index];
        }
    }
}

fn vertex_equals(from: &ListVertex, to: &ListVertex) -> bool {
    if compress_normal(from.normal) != compress_normal(to.normal) {
        return false;
    }

    if !from.texture_coordinate.abs_diff_eq(to.texture_coordinate, super::FLOAT_TOLERANCE) {
        return false;
    }

    if from.links.len() != to.links.len() {
        return false;
    }

    if from
        .links
        .iter()
        .zip(to.links.iter())
        .any(|(from_link, to_link)| from_link.bone != to_link.bone || (from_link.weight - to_link.weight).abs() >= super::FLOAT_TOLERANCE)
    {
        return false;
    }

    true
}

// Translation of https://github.com/ValveSoftware/source-sdk-2013/blob/22288b919617be6c8ca3cefd7cca979cbb39a88c/src/public/mathlib/mathlib.h#L2051
fn compress_normal(normal: Vector3) -> u16 {
    let x_sign = if normal.x < 0.0 { -1.0 } else { 1.0 };
    let y_sign = if normal.y < 0.0 { -1.0 } else { 1.0 };
    let z_sign = if normal.z < 0.0 { -1.0 } else { 1.0 };
    let t_sign = 1.0;

    let x_sign_bit = 0.5 * (1.0 - x_sign);
    let y_sign_bit = 0.5 * (1.0 - y_sign);
    let z_sign_bit = 0.5 * (1.0 - z_sign);
    let t_sign_bit = 0.0;

    let absolute_x = x_sign * normal.x;
    let absolute_y = y_sign * normal.y;
    let absolute_z = z_sign * normal.z;

    let mut x_bits = absolute_x / (absolute_x + absolute_y + absolute_z);
    let mut y_bits = absolute_y / (absolute_x + absolute_y + absolute_z);

    x_bits *= 63.0;
    y_bits *= 63.0;

    x_bits *= x_sign - x_sign_bit;
    y_bits *= y_sign - y_sign_bit;
    x_bits += 64.0;
    y_bits += 64.0;

    x_bits *= z_sign - z_sign_bit;
    y_bits *= t_sign - t_sign_bit;

    x_bits += 128.0;
    y_bits += 128.0;

    let byte_x = x_bits as u8;
    let byte_y = y_bits as u8;

    u16::from_ne_bytes([byte_x, byte_y])
}

fn mesh_create_bounding_boxes(mesh: &TriangleList<Mesh>, processed_bone_data: &super::BoneData, model_data: &mut super::ModelData) {
    for vertex in &mesh.vertices {
        model_data.bounding_box.add_point(vertex.position);

        for link in &vertex.links {
            let bone = &processed_bone_data.processed_bones[link.bone];
            let local_location = (bone.world_transform.inverse() * Matrix4::from_translation(vertex.position)).translation;
            model_data.hitboxes.entry(link.bone).or_default().add_point(local_location * link.weight);
        }
    }
}

fn calculate_mesh_vertex_tangents(triangle_list: &TriangleList<Mesh>) -> Vec<Vector4> {
    let mut tangents = vec![Vector3::default(); triangle_list.vertices.len()];
    let mut bi_tangents = vec![Vector3::default(); triangle_list.vertices.len()];

    for face in &triangle_list.triangles {
        let edge1 = triangle_list.vertices[face[1]].position - triangle_list.vertices[face[0]].position;
        let edge2 = triangle_list.vertices[face[2]].position - triangle_list.vertices[face[0]].position;
        let delta_uv1 = triangle_list.vertices[face[1]].texture_coordinate - triangle_list.vertices[face[0]].texture_coordinate;
        let delta_uv2 = triangle_list.vertices[face[2]].texture_coordinate - triangle_list.vertices[face[0]].texture_coordinate;

        let denominator = delta_uv1.x * delta_uv2.y - delta_uv2.x * delta_uv1.y;

        if denominator.abs() < f32::EPSILON {
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

fn create_triangle_strips(mesh: TriangleList<Mesh>, tangents: Vec<Vector4>) -> Vec<TriangleList<Strip>> {
    let mut triangle_lists = Vec::new();

    let mut triangle_list = TriangleList::default();
    let mut mapped_indices = IndexMap::new();
    let mut list_vertices = Vec::new();
    for triangle in mesh.triangles {
        let new_unique_indices = IndexSet::<usize>::from_iter(triangle.iter().cloned());
        let new_indices_count = new_unique_indices.iter().filter(|&index| !mapped_indices.contains_key(index)).count();
        if mapped_indices.len() + new_indices_count > (u16::MAX as usize + 1) {
            triangle_list.vertices = std::mem::take(&mut list_vertices);
            mapped_indices.clear();
            triangle_lists.push(std::mem::take(&mut triangle_list));
        }
        let mapped_triangle = triangle.map(|index| {
            *mapped_indices.entry(index).or_insert_with(|| {
                list_vertices.push(ListVertex {
                    position: mesh.vertices[index].position,
                    normal: mesh.vertices[index].normal,
                    texture_coordinate: mesh.vertices[index].texture_coordinate,
                    tangent: tangents[index],
                    links: mesh.vertices[index].links.clone(),
                    flex: mesh.vertices[index].flex.clone(),
                });
                list_vertices.len() - 1
            })
        });
        triangle_list.triangles.push(mapped_triangle);
    }
    triangle_list.vertices = list_vertices;
    triangle_lists.push(triangle_list);

    triangle_lists
}

/// Translation of https://github.com/zeux/meshoptimizer/blob/73583c335e541c139821d0de2bf5f12960a04941/src/vcacheoptimizer.cpp#L169
fn strip_optimize_vertex_cache(list: &mut TriangleList<Strip>) {
    let triangle_count = list.triangles.len();
    let index_count = triangle_count * 3;
    let vertex_count = list.vertices.len();

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

    for triangle in &list.triangles {
        for &vertex_index in triangle {
            adjacency.counts[vertex_index] += 1;
        }
    }

    let mut offset = 0;
    for vertex_index in 0..vertex_count {
        adjacency.offsets[vertex_index] = offset;
        offset += adjacency.counts[vertex_index];
    }

    for (triangle_index, triangle) in list.triangles.iter().enumerate() {
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
    for (triangle_index, triangle) in list.triangles.iter().enumerate() {
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
        let current_triangle = list.triangles[current_triangle_index];
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

    list.triangles = destination;
}

fn strip_finalize(triangle_strip: TriangleList<Strip>, material_index: usize) -> (super::Mesh, usize, usize, usize) {
    let mut processed_mesh = super::Mesh {
        material: material_index as i32,
        ..Default::default()
    };

    let triangle_count = triangle_strip.triangles.len();
    let mut processed_strip_group = super::StripGroup::default();
    let mut processed_strip = super::Strip::default();

    let mut mesh_mapped_indices = IndexMap::new();
    let mut strip_mapped_indices = IndexMap::new();
    let mut hardware_bones = IndexSet::new();
    for triangle in triangle_strip.triangles {
        let new_hardware_bone_count = triangle
            .iter()
            .flat_map(|&index| &triangle_strip.vertices[index].links)
            .map(|link| link.bone as u8)
            .filter(|bone| !hardware_bones.contains(bone))
            .count();
        if hardware_bones.len() + new_hardware_bone_count > super::MAX_HARDWARE_BONES_PER_STRIP {
            strip_mapped_indices.clear();
            let new_processed_strip = super::Strip {
                indices_offset: processed_strip.indices_offset + processed_strip.indices_count,
                vertex_offset: processed_strip.vertex_offset + processed_strip.vertex_count,
                ..Default::default()
            };

            hardware_bones.clear();
            processed_strip_group.strips.push(processed_strip);
            processed_strip = new_processed_strip;
        }

        for index in triangle {
            let mesh_mapped_index = mesh_mapped_indices.entry(index).or_insert_with(|| {
                let vertex_data = &triangle_strip.vertices[index];
                debug_assert!(!vertex_data.links.is_empty() && vertex_data.links.len() <= 3);
                let weight_count = vertex_data.links.len() as u8;
                let mut vertex_weights = [0.0; 3];
                let mut weight_bones = [0; 3];
                for (link_index, link) in vertex_data.links.iter().enumerate() {
                    vertex_weights[link_index] = link.weight;
                    weight_bones[link_index] = link.bone as u8;
                }

                for (&flex_key, flexed_vertex) in &vertex_data.flex {
                    let flexed_vertex = super::FlexVertex {
                        vertex_index: processed_mesh.vertex_data.len() as u16,
                        location_delta: flexed_vertex.position,
                        normal_delta: flexed_vertex.normal,
                    };

                    if let Some(existing_flex) = processed_mesh.flexes.iter_mut().find(|flex| flex.flex_key_index == flex_key as i32) {
                        existing_flex.flexed_vertices.push(flexed_vertex);
                    } else {
                        processed_mesh.flexes.push(super::Flex {
                            flex_key_index: flex_key as i32,
                            flexed_vertices: vec![flexed_vertex],
                        });
                    }
                }

                let processed_vertex = super::Vertex {
                    weights: vertex_weights,
                    bones: weight_bones,
                    bone_count: weight_count,
                    position: vertex_data.position,
                    normal: vertex_data.normal,
                    texture_coordinate: vertex_data.texture_coordinate,
                    tangent: vertex_data.tangent,
                };
                processed_mesh.vertex_data.push(processed_vertex);
                processed_mesh.vertex_data.len() - 1
            });

            let strip_mapped_index = strip_mapped_indices.entry(*mesh_mapped_index).or_insert_with(|| {
                let processed_vertex = &processed_mesh.vertex_data[*mesh_mapped_index];
                let mut processed_mesh_vertex = super::MeshVertex {
                    vertex_index: *mesh_mapped_index as u16,
                    bone_count: processed_vertex.bone_count,
                    ..Default::default()
                };
                processed_strip.bone_count = processed_strip.bone_count.max(processed_vertex.bone_count as i16);
                for (link_index, &link) in processed_vertex.bones.iter().take(processed_vertex.bone_count as usize).enumerate() {
                    let (hardware_bone_index, new_hardware_bone) = hardware_bones.insert_full(link);

                    processed_mesh_vertex.bones[link_index] = hardware_bone_index as u8;
                    if new_hardware_bone {
                        let processed_hardware_bone = super::HardwareBone {
                            hardware_bone: hardware_bone_index as i32,
                            bone_table_bone: link as i32,
                        };
                        processed_strip.hardware_bones.push(processed_hardware_bone);
                    }
                }
                processed_strip_group.vertices.push(processed_mesh_vertex);
                processed_strip.vertex_count += 1;
                (processed_strip_group.vertices.len() - 1) as u16
            });

            processed_strip_group.indices.push(*strip_mapped_index);
            processed_strip.indices_count += 1;
        }
    }

    processed_strip_group.strips.push(processed_strip);
    let strip_count = processed_strip_group.strips.len();
    processed_mesh.strip_groups.push(processed_strip_group);

    (processed_mesh, triangle_count, mesh_mapped_indices.len(), strip_count)
}
