use super::{Animation, Bone, Channel, FlexVertex, Part, Vertex};
use crate::{
    import::dmx::ParseDMXError::DuplicateAnimationName,
    utilities::mathematics::{AxisDirection, Matrix4, Quaternion as Quat, Vector2 as Vec2, Vector3 as Vec3},
};
use datamodel::{
    SerializationError,
    attribute::{Quaternion, Time, UUID, Vector2, Vector3},
    deserialize,
    element::Element,
};
use indexmap::{IndexMap, IndexSet};
use std::{fs::File, io::BufReader, num::NonZeroUsize};
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum ParseDMXError {
    #[error("Failed To Deserialize DMX File: {0}")]
    DeserializationError(#[from] SerializationError),
    #[error("DMX File Format Is Not A Model: Gotten {0}")]
    FormatNotModel(String),
    #[error("DMX File Format Version Is Not Supported: Supported Versions 1 - 18, Gotten {0}")]
    UnsupportedFormatVersion(i32),
    #[error("Missing Required Attribute \"{0}\" Of Type \"{1}\" On Element \"{2}\"")]
    MissingRequiredAttribute(&'static str, &'static str, UUID),
    #[error("Duplicate Joint Name \"{0}\" Element \"{1}\"")]
    DuplicateJointName(String, UUID),
    #[error("Duplicate Part Name \"{0}\" Element \"{1}\"")]
    DuplicatePartName(String, UUID),
    #[error("Duplicate Flex Name \"{0}\" Element \"{1}\"")]
    DuplicateFlexName(String, UUID),
    #[error("Duplicate Animation Name \"{0}\" Element \"{1}\"")]
    DuplicateAnimationName(String, UUID),
    #[error("Joint \"{0}\" In \"{1}\" Was Not In DAG Hierarchy")]
    JointNotInHierarchy(UUID, &'static str),
    #[error("Mesh \"{0}\" Is Missing Bind State")]
    MeshMissingBindSate(UUID),
    #[error("\"{0}\" Array Length Is Not The Same As \"{1}\" For Element \"{2}\"")]
    MissedMatchedArray(&'static str, &'static str, UUID),
    #[error("Index In \"{0}\" For Element \"{1}\" Was Invalid: Gotten {2}, Max: {3}")]
    InvalidIndex(&'static str, UUID, i32, usize),
    #[error("Face In \"{0}\" Has Less Than 3 Indices")]
    IncompleteFace(UUID),
    #[error("Animation Channel \"{0}\" Target Element \"{1}\" Was Not A Joint")]
    InvalidAnimationJointTarget(UUID, UUID),
    #[error("Animation Log \"{0}\" Layer \"{1}\" Is Null")]
    AnimationLayerNull(UUID, usize),
}

type Integer = i32;
type Float = f32;
type Boolean = bool;
type ElementArray = Vec<Option<Element>>;
type IntegerArray = Vec<i32>;
type FloatArray = Vec<f32>;
type TimeArray = Vec<Time>;
type Vector2Array = Vec<Vector2>;
type Vector3Array = Vec<Vector3>;
type QuaternionArray = Vec<Quaternion>;

macro_rules! get_attribute {
    ($element:expr, $value_name:expr, $value_type:ty) => {
        $element
            .get_value::<$value_type>($value_name)
            .map_err(|_| ParseDMXError::MissingRequiredAttribute($value_name, stringify!($value_type), *$element.get_id()))
    };
}

pub fn load_dmx(mut file_buffer: BufReader<File>, file_name: String) -> Result<super::FileData, ParseDMXError> {
    let (file_header, file_root) = deserialize(&mut file_buffer)?;

    if file_header.format != "model" {
        return Err(ParseDMXError::FormatNotModel(file_header.format));
    }

    if file_header.format_version < 1 || file_header.format_version > 18 {
        return Err(ParseDMXError::UnsupportedFormatVersion(file_header.format_version));
    }

    let hierarchy = get_attribute!(file_root, "skeleton", Element)?;
    let skeleton = load_armature(&hierarchy)?;

    let parts = if let Ok(model) = file_root.get_value::<Element>("model") {
        let joint_list = load_joint_list(&model, &skeleton, file_header.format_version)?;
        load_model(&model, &skeleton, joint_list)?
    } else {
        IndexMap::new()
    };

    let animations = if let Ok(animation_list) = file_root.get_value::<Element>("animationList") {
        load_animations(&animation_list, file_header.format_version, &skeleton)?
    } else {
        IndexMap::from([(file_name, Default::default())])
    };

    Ok(super::FileData {
        up: AxisDirection::PositiveZ,
        forward: AxisDirection::NegativeY,
        skeleton,
        animations,
        parts,
    })
}

fn load_armature(hierarchy: &Element) -> Result<IndexMap<String, Bone>, ParseDMXError> {
    let mut armature = IndexMap::new();

    if let Ok(children) = hierarchy.get_value::<ElementArray>("children") {
        for child in children.iter().flatten() {
            load_joint(&mut armature, None, child)?;
        }
    }

    Ok(armature)
}

fn convert_vector3(vector: Vector3) -> Vec3 {
    Vec3::new(vector.x, vector.y, vector.z)
}

fn convert_quaternion(quaternion: Quaternion) -> Quat {
    Quat::from_xyzw(quaternion.x, quaternion.y, quaternion.z, quaternion.w)
}

fn load_joint(skeleton: &mut IndexMap<String, Bone>, parent_index: Option<usize>, joint: &Element) -> Result<(), ParseDMXError> {
    let joint_name = get_attribute!(joint, "name", String)?;
    let joint_transform = get_attribute!(joint, "transform", Element)?;
    let joint_position = *get_attribute!(joint_transform, "position", Vector3)?;
    let joint_rotation = *get_attribute!(joint_transform, "orientation", Quaternion)?;

    let joint_index = Some(skeleton.len());

    if skeleton
        .insert(
            joint_name.clone(),
            Bone {
                parent: parent_index,
                location: convert_vector3(joint_position),
                rotation: convert_quaternion(joint_rotation),
            },
        )
        .is_some()
    {
        return Err(ParseDMXError::DuplicateJointName(joint_name.clone(), *joint.get_id()));
    }

    if let Ok(children) = joint.get_value::<ElementArray>("children") {
        for child in children.iter().flatten() {
            load_joint(skeleton, joint_index, child)?;
        }
    }

    Ok(())
}

fn load_joint_list(model: &Element, skeleton: &IndexMap<String, Bone>, format_version: i32) -> Result<Vec<usize>, ParseDMXError> {
    let list_name = if format_version < 8 { "jointTransforms" } else { "jointList" };
    let list = get_attribute!(model, list_name, ElementArray)?;
    let mut joint_list = Vec::with_capacity(list.len());
    for joint in list.iter().flatten() {
        let joint_name = get_attribute!(joint, "name", String)?;
        let joint_index = skeleton
            .get_index_of(joint_name.as_str())
            .ok_or(ParseDMXError::JointNotInHierarchy(*joint.get_id(), list_name))?;
        joint_list.push(joint_index);
    }
    Ok(joint_list)
}

fn load_model(model: &Element, skeleton: &IndexMap<String, Bone>, joint_list: Vec<usize>) -> Result<IndexMap<String, Part>, ParseDMXError> {
    let mut parts = IndexMap::new();

    if let Ok(children) = model.get_value::<ElementArray>("children") {
        for child in children.iter().flatten() {
            load_part(&mut parts, child, Matrix4::IDENTITY, skeleton, &joint_list)?;
        }
    }

    Ok(parts)
}

fn load_part(
    parts: &mut IndexMap<String, Part>,
    part: &Element,
    transform: Matrix4,
    armature: &IndexMap<String, Bone>,
    joint_list: &[usize],
) -> Result<(), ParseDMXError> {
    let part_transform = get_attribute!(part, "transform", Element)?;
    let part_position = *get_attribute!(part_transform, "position", Vector3)?;
    let part_rotation = *get_attribute!(part_transform, "orientation", Quaternion)?;
    let current_transform = transform * Matrix4::from_rotation_translation(convert_quaternion(part_rotation), convert_vector3(part_position));

    if let Ok(shape) = part.get_value::<Element>("shape")
        && let Ok(base_states) = shape.get_value::<ElementArray>("baseStates")
    {
        let bind_state = base_states
            .iter()
            .flatten()
            .find(|state| get_attribute!(state, "name", String).is_ok_and(|name| name.eq("bind")))
            .ok_or(ParseDMXError::MeshMissingBindSate(*part.get_id()))?;

        let part_name = get_attribute!(shape, "name", String)?;
        let vertex_data = load_vertices(bind_state, &part_name, armature, joint_list)?;
        let mut vertices = vertex_data.vertices;
        let faces = load_faces(&shape, vertex_data.vertex_remap)?;
        let flexes = load_flexes(
            &shape,
            current_transform,
            &vertices,
            vertex_data.position_index_map,
            vertex_data.normal_index_map,
        )?;

        for vertex in &mut vertices {
            vertex.location = current_transform.transform_point3(vertex.location);
            vertex.normal = current_transform.transform_vector3(vertex.normal);
        }

        if parts
            .insert(
                part_name.clone(),
                Part {
                    vertices,
                    faces,
                    flexes,
                    used_bones: vertex_data.used_bones,
                },
            )
            .is_some()
        {
            return Err(ParseDMXError::DuplicatePartName(part_name.clone(), *shape.get_id()));
        }
    };

    if let Ok(children) = part.get_value::<ElementArray>("children") {
        for child in children.iter().flatten() {
            load_part(parts, child, current_transform, armature, joint_list)?;
        }
    }

    Ok(())
}

fn validate_index(index: i32, length: usize, name: &'static str, bind_state: &Element) -> Result<i32, ParseDMXError> {
    if index < 0 || index as usize >= length {
        return Err(ParseDMXError::InvalidIndex(name, *bind_state.get_id(), index, length));
    }
    Ok(index)
}

struct VertexData {
    vertices: Vec<Vertex>,
    vertex_remap: Vec<usize>,
    position_index_map: Vec<Vec<usize>>,
    normal_index_map: Vec<Vec<usize>>,
    used_bones: IndexSet<usize>,
}

fn load_vertices(bind_state: &Element, part_name: &str, skeleton: &IndexMap<String, Bone>, joint_list: &[usize]) -> Result<VertexData, ParseDMXError> {
    let position_indices = get_attribute!(bind_state, "positionsIndices", IntegerArray)?;
    let positions = get_attribute!(bind_state, "positions", Vector3Array)?;
    let normals_indices = get_attribute!(bind_state, "normalsIndices", IntegerArray)?;
    let normals = get_attribute!(bind_state, "normals", Vector3Array)?;
    let texture_coordinate_indices = get_attribute!(bind_state, "textureCoordinatesIndices", IntegerArray)?;
    let texture_coordinates = get_attribute!(bind_state, "textureCoordinates", Vector2Array)?;

    if normals_indices.len() != position_indices.len() {
        return Err(ParseDMXError::MissedMatchedArray("normalsIndices", "positionsIndices", *bind_state.get_id()));
    }

    if texture_coordinate_indices.len() != position_indices.len() {
        return Err(ParseDMXError::MissedMatchedArray(
            "textureCoordinatesIndices",
            "positionsIndices",
            *bind_state.get_id(),
        ));
    }

    #[derive(Eq, PartialEq, Hash)]
    struct UniqueVertex {
        position: i32,
        normal: i32,
        texture_coordinate: i32,
    }
    let mut unique_vertices = IndexSet::new();
    let mut vertex_remap = Vec::with_capacity(position_indices.len());
    let mut vertices = Vec::new();
    let mut position_index_map = vec![Vec::new(); positions.len()];
    let mut normal_index_map = vec![Vec::new(); normals.len()];
    let mut used_bones = IndexSet::new();

    let flip_coordinate = get_attribute!(bind_state, "flipVCoordinates", Boolean).map(|flip| *flip).unwrap_or_default();
    let joint_count = get_attribute!(bind_state, "jointCount", Integer).map(|count| *count).unwrap_or_default().max(0) as usize;
    let root_bone = skeleton.get_index_of(part_name).unwrap_or_default();

    let joint_links = if joint_count > 0 {
        let joint_indices = get_attribute!(bind_state, "jointIndices", IntegerArray)?;
        let joint_weights = get_attribute!(bind_state, "jointWeights", FloatArray)?;

        if joint_indices.len() != positions.len() * joint_count {
            return Err(ParseDMXError::MissedMatchedArray("jointIndices", "positions", *bind_state.get_id()));
        }

        if joint_weights.len() != joint_indices.len() {
            return Err(ParseDMXError::MissedMatchedArray("jointWeights", "jointIndices", *bind_state.get_id()));
        }

        Some((joint_indices, joint_weights))
    } else {
        None
    };

    for vertex_index in 0..position_indices.len() {
        let unique_vertex = UniqueVertex {
            position: validate_index(position_indices[vertex_index], positions.len(), "positionsIndices", bind_state)?,
            normal: validate_index(normals_indices[vertex_index], normals.len(), "normalsIndices", bind_state)?,
            texture_coordinate: validate_index(
                texture_coordinate_indices[vertex_index],
                texture_coordinates.len(),
                "textureCoordinatesIndices",
                bind_state,
            )?,
        };

        let (unique_vertex_index, new_unique) = unique_vertices.insert_full(unique_vertex);
        vertex_remap.push(unique_vertex_index);
        if !new_unique {
            continue;
        }
        position_index_map[position_indices[vertex_index] as usize].push(unique_vertex_index);
        normal_index_map[normals_indices[vertex_index] as usize].push(unique_vertex_index);

        let location = convert_vector3(positions[position_indices[vertex_index] as usize]);
        let normal = convert_vector3(normals[normals_indices[vertex_index] as usize]);
        let texture_coordinate = Vec2::new(
            texture_coordinates[texture_coordinate_indices[vertex_index] as usize].x,
            if flip_coordinate {
                texture_coordinates[texture_coordinate_indices[vertex_index] as usize].y
            } else {
                1.0 + texture_coordinates[texture_coordinate_indices[vertex_index] as usize].y
            },
        );

        let links = if let Some((joint_indices, joint_weights)) = joint_links.as_ref() {
            let mut links = IndexMap::with_capacity(joint_count);
            for joint_count_index in 0..joint_count {
                let joint_index = joint_indices[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                let joint_weight = joint_weights[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                let joint_link = joint_list[validate_index(joint_index, joint_list.len(), "jointIndices", bind_state)? as usize];
                *links.entry(joint_link).or_default() += joint_weight;
                used_bones.insert(joint_link);
            }
            links
        } else {
            used_bones.insert(root_bone);
            IndexMap::from([(root_bone, 1.0)])
        };

        vertices.push(Vertex {
            location,
            normal,
            texture_coordinate,
            links,
        });
    }

    Ok(VertexData {
        vertices,
        vertex_remap,
        position_index_map,
        normal_index_map,
        used_bones,
    })
}

fn load_faces(shape: &Element, vertex_remap: Vec<usize>) -> Result<IndexMap<String, Vec<Vec<usize>>>, ParseDMXError> {
    let mut material_list: IndexMap<String, Vec<Vec<usize>>> = IndexMap::new();
    let face_sets = get_attribute!(shape, "faceSets", ElementArray)?;
    for face_set in face_sets.iter().flatten() {
        let face_indices = get_attribute!(face_set, "faces", IntegerArray)?;
        let material = get_attribute!(face_set, "material", Element)?;
        let material_name = get_attribute!(material, "mtlName", String)?;

        let mut faces = Vec::new();
        let mut face = Vec::new();
        for &face_index in face_indices.iter() {
            if face_index == -1 {
                if face.len() < 3 {
                    return Err(ParseDMXError::IncompleteFace(*face_set.get_id()));
                }
                face.reverse();
                faces.push(std::mem::take(&mut face));
                continue;
            }

            face.push(vertex_remap[validate_index(face_index, vertex_remap.len(), "faces", shape)? as usize]);
        }
        material_list.entry(material_name.clone()).or_default().extend(faces);
    }
    Ok(material_list)
}

fn load_flexes(
    shape: &Element,
    current_transform: Matrix4,
    vertices: &[Vertex],
    position_index_map: Vec<Vec<usize>>,
    normal_index_map: Vec<Vec<usize>>,
) -> Result<IndexMap<String, IndexMap<usize, FlexVertex>>, ParseDMXError> {
    let mut flexes = IndexMap::new();
    let Ok(delta_states) = shape.get_value::<ElementArray>("deltaStates") else {
        return Ok(flexes);
    };

    for delta_state in delta_states.iter().flatten() {
        let flex_name = get_attribute!(delta_state, "name", String)?;
        let mut flex = IndexMap::new();

        let delta_positions_indices = get_attribute!(delta_state, "positionsIndices", IntegerArray)?;
        let delta_positions = get_attribute!(delta_state, "positions", Vector3Array)?;
        if delta_positions.len() != delta_positions_indices.len() {
            return Err(ParseDMXError::MissedMatchedArray("positionsIndices", "positions", *delta_state.get_id()));
        }
        for (delta_position_index, delta_position) in delta_positions.iter().enumerate() {
            let delta_positions_index = validate_index(
                delta_positions_indices[delta_position_index],
                position_index_map.len(),
                "positionsIndices",
                delta_state,
            )?;
            for &unique_vertex_index in &position_index_map[delta_positions_index as usize] {
                let unique_vertex = &vertices[unique_vertex_index];
                let unique_vertex_location = unique_vertex.location;
                let delta_location = convert_vector3(*delta_position);
                let transformed_location = current_transform.transform_point3(unique_vertex_location + delta_location);

                flex.insert(
                    unique_vertex_index,
                    super::FlexVertex {
                        location: transformed_location,
                        normal: current_transform.transform_vector3(unique_vertex.normal),
                    },
                );
            }
        }

        let delta_normals_indices = get_attribute!(delta_state, "normalsIndices", IntegerArray)?;
        let delta_normals = get_attribute!(delta_state, "normals", Vector3Array)?;
        if delta_normals.len() != delta_normals_indices.len() {
            return Err(ParseDMXError::MissedMatchedArray("normalsIndices", "normals", *delta_state.get_id()));
        }
        for (delta_normal_index, delta_normal) in delta_normals.iter().enumerate() {
            let delta_normals_index = validate_index(delta_normals_indices[delta_normal_index], normal_index_map.len(), "normalsIndices", delta_state)?;
            for &unique_vertex_index in &normal_index_map[delta_normals_index as usize] {
                let unique_vertex = &vertices[unique_vertex_index];
                let unique_vertex_normal = unique_vertex.normal;
                let delta_normal = convert_vector3(*delta_normal);
                let transformed_normal = current_transform.transform_vector3(unique_vertex_normal + delta_normal);

                if let Some(flexed_vertex) = flex.get_mut(&unique_vertex_index) {
                    flexed_vertex.normal = transformed_normal;
                    continue;
                }

                flex.insert(
                    unique_vertex_index,
                    super::FlexVertex {
                        location: current_transform.transform_point3(unique_vertex.location),
                        normal: transformed_normal,
                    },
                );
            }
        }

        if flexes.insert(flex_name.clone(), flex).is_some() {
            return Err(ParseDMXError::DuplicateFlexName(flex_name.clone(), *shape.get_id()));
        }
    }

    Ok(flexes)
}

fn load_animations(animation_list: &Element, format_version: i32, skeleton: &IndexMap<String, Bone>) -> Result<IndexMap<String, Animation>, ParseDMXError> {
    let mut animations = IndexMap::new();

    for animation in get_attribute!(animation_list, "animations", ElementArray)?.iter().flatten() {
        let animation_name = get_attribute!(animation, "name", String)?;
        let time_frame = get_attribute!(animation, "timeFrame", Element)?;
        let start = if format_version < 2 {
            Time(*get_attribute!(time_frame, "startTime", Integer)?).as_seconds()
        } else {
            get_attribute!(time_frame, "start", Time)?.as_seconds()
        };
        let duration = if format_version < 2 {
            Time(*get_attribute!(time_frame, "durationTime", Integer)?).as_seconds()
        } else {
            get_attribute!(time_frame, "duration", Time)?.as_seconds()
        };
        let mut frame_rate = get_attribute!(animation, "frameRate", Float).map(|rate| *rate).unwrap_or_default();
        if frame_rate <= 0.0 {
            frame_rate = 30.0;
        }
        let start_frame = (start * frame_rate).ceil() as usize;
        let end_frame = ((start + duration) * frame_rate).ceil() as usize;
        let frame_count = NonZeroUsize::new(end_frame - start_frame + 1).unwrap_or(NonZeroUsize::MIN);

        let mut channels = IndexMap::new();
        for channel in get_attribute!(animation, "channels", ElementArray)?.iter().flatten() {
            load_channel(&mut channels, channel, format_version, skeleton, start_frame, frame_rate)?;
        }

        if animations.insert(animation_name.clone(), Animation { frame_count, channels }).is_some() {
            return Err(DuplicateAnimationName(animation_name.clone(), *animation.get_id()));
        }
    }

    Ok(animations)
}

fn load_channel(
    channels: &mut IndexMap<usize, Channel>,
    channel: &Element,
    format_version: i32,
    skeleton: &IndexMap<String, Bone>,
    start_frame: usize,
    frame_rate: f32,
) -> Result<(), ParseDMXError> {
    let target_element = get_attribute!(channel, "toElement", Element)?;
    let target_name = get_attribute!(target_element, "name", String)?;
    let bone = skeleton
        .get_index_of(target_name.as_str())
        .ok_or(ParseDMXError::InvalidAnimationJointTarget(*channel.get_id(), *target_element.get_id()))?;
    let animation_channel = channels.entry(bone).or_default();

    let target_attribute = get_attribute!(channel, "toAttribute", String)?;
    let layer_index = get_attribute!(channel, "toIndex", Integer).map(|layer| *layer).unwrap_or_default();
    let log = get_attribute!(channel, "log", Element)?;
    let layers = get_attribute!(log, "layers", ElementArray)?;
    let target_layer = layers[validate_index(layer_index, layers.len(), "toIndex", channel)? as usize].as_ref();
    let Some(layer) = target_layer else {
        return Err(ParseDMXError::AnimationLayerNull(*log.get_id(), layer_index as usize));
    };
    let times = if format_version < 2 {
        get_attribute!(layer, "times", IntegerArray)?
            .iter()
            .map(|&time| Time(time).as_seconds())
            .collect::<Vec<_>>()
    } else {
        get_attribute!(layer, "times", TimeArray)?
            .iter()
            .map(|time| time.as_seconds())
            .collect::<Vec<_>>()
    };

    if target_attribute.eq("position") {
        let values = get_attribute!(layer, "values", Vector3Array)?;
        if values.len() != times.len() {
            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
        }

        for (frame, time) in times.into_iter().enumerate() {
            let time_frame = (time * frame_rate).ceil() as usize;

            if time_frame < start_frame {
                continue;
            }

            animation_channel.location.insert(time_frame, convert_vector3(values[frame]));
        }

        return Ok(());
    }

    if target_attribute.eq("orientation") {
        let values = get_attribute!(layer, "values", QuaternionArray)?;
        if values.len() != times.len() {
            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
        }

        for (frame, time) in times.into_iter().enumerate() {
            let time_frame = (time * frame_rate).ceil() as usize;

            if time_frame < start_frame {
                continue;
            }

            animation_channel.rotation.insert(time_frame, convert_quaternion(values[frame]));
        }

        return Ok(());
    }

    Ok(())
}
