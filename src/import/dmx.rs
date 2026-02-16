use datamodel::{
    Element, SerializationError,
    attribute::{Duration, ElementArray, Quaternion, UUID, Vector2, Vector3},
    deserialize,
};
use indexmap::IndexSet;
use std::{fs::File, io::BufReader, num::NonZeroUsize};
use thiserror::Error as ThisError;

use crate::{import::FileData, utilities::mathematics as Math};

type Integer = i32;
type IntegerArray = Vec<i32>;
type Time = Duration;
type TimeArray = Vec<Duration>;
type FloatArray = Vec<f32>;
type Vector2Array = Vec<Vector2>;
type Vector3Array = Vec<Vector3>;
type QuaternionArray = Vec<Quaternion>;

#[derive(Debug, ThisError)]
pub enum ParseDMXError {
    #[error("Failed To Deserilize DMX File: {0}")]
    DeserializationError(#[from] SerializationError),
    #[error("DMX File Format Is Not A Model: Gotten {0}")]
    FormatNotModel(String),
    #[error("DMX File Format Version Is Not Supported: Supported Versions 1 - 18, Gotten {0}")]
    UnsupportedFormatVersion(i32),
    #[error("Missing Required Attribute \"{0}\" Of Type \"{1}\" On Element \"{2}\"")]
    MissingRequiredAttribute(&'static str, &'static str, UUID),
    #[error("Element Entry \"{0}\" In Element Array \"{1}\" For Element \"{2}\" Was Null")]
    NullElementInArray(usize, &'static str, UUID),
    #[error("Duplicate Joint Name \"{0}\" Element \"{1}\"")]
    DuplicateJointName(String, UUID),
    #[error("Duplicate Part Name \"{0}\" Element \"{1}\"")]
    DuplicatePartName(String, UUID),
    #[error("Duplicate Flex Name \"{0}\" Element \"{1}\"")]
    DuplicateFlexName(String, UUID),
    #[error("Duplicate Animation Name \"{0}\" Element \"{1}\"")]
    DuplicateAnimationName(String, UUID),
    #[error("Joint \"{0}\" Was Not In Joint List")]
    JointNotInJointList(UUID),
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
}

pub fn load_dmx(mut file_buffer: BufReader<File>, file_name: String) -> Result<super::FileData, ParseDMXError> {
    let (file_header, file_root) = deserialize(&mut file_buffer)?;

    if file_header.get_format() != "model" {
        return Err(ParseDMXError::FormatNotModel(file_header.get_format().to_owned()));
    }

    if file_header.format_version < 1 || file_header.format_version > 18 {
        return Err(ParseDMXError::UnsupportedFormatVersion(file_header.format_version));
    }

    let mut file_data = super::FileData {
        up: Math::AxisDirection::PositiveZ,
        forward: Math::AxisDirection::NegativeY,
        ..Default::default()
    };

    macro_rules! get_attribute {
        ($element:expr, $value_name:expr, $value_type:ty) => {
            $element
                .get_value::<$value_type>($value_name)
                .ok_or(ParseDMXError::MissingRequiredAttribute(
                    $value_name,
                    stringify!($value_type),
                    *$element.get_id(),
                ))
        };
    }

    let skeleton = get_attribute!(file_root, "skeleton", Element)?;
    let model = file_root.get_value::<Element>("model");

    let mut joints = IndexSet::new();
    if let Some(model) = &model {
        let joint_list_name = if file_header.format_version < 8 { "jointTransforms" } else { "jointList" };
        let joint_list = get_attribute!(model, joint_list_name, ElementArray)?;

        for (joint_index, joint) in joint_list.iter().enumerate() {
            if let Some(joint) = joint {
                joints.insert(Element::clone(joint));
                continue;
            }
            return Err(ParseDMXError::NullElementInArray(joint_index, joint_list_name, *model.get_id()));
        }
    }

    fn load_joints(
        parent: &Element,
        parent_index: Option<usize>,
        joints: Option<&IndexSet<Element>>,
        file_data: &mut FileData,
        format_version: i32,
    ) -> Result<(), ParseDMXError> {
        if file_data.parts.contains_key(parent.get_name().as_str()) {
            return Err(ParseDMXError::DuplicateJointName(parent.get_name().clone(), *parent.get_id()));
        }

        let joint_transform = get_attribute!(parent, "transform", Element)?;
        let joint_position = get_attribute!(joint_transform, "position", Vector3)?;
        let joint_rotation = get_attribute!(joint_transform, "orientation", Quaternion)?;

        if let Some(joints) = joints {
            let joint = if format_version < 8 { &Element::clone(&joint_transform) } else { parent };
            if !joints.contains(joint) {
                return Err(ParseDMXError::JointNotInJointList(*parent.get_id()));
            }
        }

        let bone_index = Some(file_data.skeleton.len());

        file_data.skeleton.insert(
            parent.get_name().clone(),
            super::Bone {
                parent: parent_index,
                location: Math::Vector3::new(joint_position.x as f64, joint_position.y as f64, joint_position.z as f64),
                rotation: Math::Quaternion::from_xyzw(
                    joint_rotation.x as f64,
                    joint_rotation.y as f64,
                    joint_rotation.z as f64,
                    joint_rotation.w as f64,
                ),
            },
        );

        let child_joints = match parent.get_value::<ElementArray>("children") {
            Some(children) => children,
            None => return Ok(()),
        };

        for child in child_joints.iter().flatten() {
            load_joints(child, bone_index, joints, file_data, format_version)?;
        }

        Ok(())
    }
    if let Some(children) = skeleton.get_value::<ElementArray>("children") {
        for child in children.iter().flatten() {
            load_joints(
                child,
                None,
                if model.is_some() { Some(&joints) } else { None },
                &mut file_data,
                file_header.format_version,
            )?;
        }
    }

    if let Some(model) = &model {
        fn load_mesh(parent: &Element, parent_transform: Math::Matrix4, joints: &IndexSet<Element>, file_data: &mut FileData) -> Result<(), ParseDMXError> {
            let mesh_transform = get_attribute!(parent, "transform", Element)?;
            let mesh_position = get_attribute!(mesh_transform, "position", Vector3)?;
            let mesh_rotation = get_attribute!(mesh_transform, "orientation", Quaternion)?;
            let current_transform = parent_transform
                * Math::Matrix4::from_rotation_translation(
                    Math::Quaternion::from_xyzw(mesh_rotation.x as f64, mesh_rotation.y as f64, mesh_rotation.z as f64, mesh_rotation.w as f64),
                    Math::Vector3::new(mesh_position.x as f64, mesh_position.y as f64, mesh_position.z as f64),
                );

            if let Some(shape) = parent.get_value::<Element>("shape")
                && let Some(base_states) = shape.get_value::<ElementArray>("baseStates")
            {
                let bind_state = base_states
                    .iter()
                    .flatten()
                    .find(|state| state.get_name().eq("bind"))
                    .ok_or(ParseDMXError::MeshMissingBindSate(*parent.get_id()))?;

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

                if file_data.parts.contains_key(parent.get_name().as_str()) {
                    return Err(ParseDMXError::DuplicatePartName(parent.get_name().clone(), *parent.get_id()));
                }

                let part = file_data.parts.entry(shape.get_name().clone()).or_default();

                fn validate_index(index: i32, length: usize, name: &'static str, bind_state: &Element) -> Result<i32, ParseDMXError> {
                    if index < 0 || index as usize >= length {
                        return Err(ParseDMXError::InvalidIndex(name, *bind_state.get_id(), index, length));
                    }
                    Ok(index)
                }

                #[derive(Eq, PartialEq, Hash)]
                struct UniqueVertex {
                    position: i32,
                    normal: i32,
                    texture_coordinate: i32,
                }
                let mut unique_vertices = IndexSet::new();
                let mut vertex_remap = Vec::with_capacity(position_indices.len());
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

                    if let Some(unique_index) = unique_vertices.get_index_of(&unique_vertex) {
                        vertex_remap.push(unique_index);
                        continue;
                    }

                    vertex_remap.push(unique_vertices.len());
                    unique_vertices.insert(unique_vertex);

                    let position = positions[position_indices[vertex_index] as usize];
                    let normal = normals[normals_indices[vertex_index] as usize];
                    let texture_coordinate = texture_coordinates[texture_coordinate_indices[vertex_index] as usize];

                    let mut vertex = super::Vertex {
                        location: Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64),
                        normal: Math::Vector3::new(normal.x as f64, normal.y as f64, normal.z as f64),
                        texture_coordinate: Math::Vector2::new(texture_coordinate.x as f64, texture_coordinate.y as f64),
                        ..Default::default()
                    };

                    let joint_count = bind_state.get_value::<i32>("jointCount").map(|count| (*count).max(0)).unwrap_or_default() as usize;
                    if joint_count == 0 {
                        let parent_bone = file_data.skeleton.get_index_of(parent.get_name().as_str()).unwrap_or_default();
                        vertex.links.insert(parent_bone, 1.0);
                        part.vertices.push(vertex);
                        continue;
                    }

                    let joint_indices = get_attribute!(bind_state, "jointIndices", IntegerArray)?;
                    let joint_weights = get_attribute!(bind_state, "jointWeights", FloatArray)?;
                    if joint_indices.len() != positions.len() * joint_count {
                        return Err(ParseDMXError::MissedMatchedArray("jointIndices", "positions", *bind_state.get_id()));
                    }
                    if joint_weights.len() != joint_indices.len() {
                        return Err(ParseDMXError::MissedMatchedArray("jointWeights", "jointIndices", *bind_state.get_id()));
                    }

                    for joint_count_index in 0..joint_count {
                        let joint_index = joint_indices[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                        let joint_weight = joint_weights[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                        let joint_element = &joints[validate_index(joint_index, joints.len(), "jointIndices", bind_state)? as usize];
                        let joint_link = file_data.skeleton.get_index_of(joint_element.get_name().as_str()).unwrap_or_default();
                        vertex.links.insert(joint_link, joint_weight as f64);
                    }
                    part.vertices.push(vertex);
                }

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
                            faces.push(face.clone());
                            face.clear();
                            continue;
                        }

                        face.push(vertex_remap[validate_index(face_index, vertex_remap.len(), "faces", bind_state)? as usize]);
                    }
                    part.faces.insert(material_name.clone(), faces);
                }

                if let Some(delta_states) = shape.get_value::<ElementArray>("deltaStates") {
                    for delta_state in delta_states.iter().flatten() {
                        if part.flexes.contains_key(delta_state.get_name().as_str()) {
                            return Err(ParseDMXError::DuplicateFlexName(delta_state.get_name().clone(), *shape.get_id()));
                        }

                        let flex = part.flexes.entry(delta_state.get_name().clone()).or_default();

                        if let Some(positions_indices) = delta_state.get_value::<IntegerArray>("positionsIndices") {
                            let positions = get_attribute!(delta_state, "positions", Vector3Array)?;
                            if positions.len() != positions_indices.len() {
                                return Err(ParseDMXError::MissedMatchedArray("positionsIndices", "positions", *bind_state.get_id()));
                            }
                            for (position_index, &position_vertex_index) in positions_indices.iter().enumerate() {
                                let vertex_position =
                                    part.vertices[validate_index(position_vertex_index, vertex_remap.len(), "positionsIndices", bind_state)? as usize].location;
                                let position_delta = positions[position_index];
                                let delta_position =
                                    vertex_position + Math::Vector3::new(position_delta.x as f64, position_delta.y as f64, position_delta.z as f64);
                                let transformed_position = current_transform.transform_point3(delta_position);

                                let delta_vertex = flex.entry(vertex_remap[position_vertex_index as usize]).or_default();
                                delta_vertex.location = transformed_position;
                            }
                        }

                        if let Some(normals_indices) = delta_state.get_value::<IntegerArray>("normalsIndices") {
                            let normals = get_attribute!(delta_state, "normals", Vector3Array)?;
                            if normals.len() != normals_indices.len() {
                                return Err(ParseDMXError::MissedMatchedArray("normalsIndices", "normals", *bind_state.get_id()));
                            }
                            for (normal_index, &normal_vertex_index) in normals_indices.iter().enumerate() {
                                let vertex_normal =
                                    part.vertices[validate_index(normal_vertex_index, vertex_remap.len(), "normalsIndices", bind_state)? as usize].normal;
                                let normal_delta = normals[normal_index];
                                let delta_normal = vertex_normal + Math::Vector3::new(normal_delta.x as f64, normal_delta.y as f64, normal_delta.z as f64);
                                let transformed_normal = current_transform.transform_vector3(delta_normal);

                                let delta_vertex = flex.entry(vertex_remap[normal_vertex_index as usize]).or_default();
                                delta_vertex.normal = transformed_normal;
                            }
                        }
                    }
                }

                for part_vertex in &mut part.vertices {
                    part_vertex.location = current_transform.transform_point3(part_vertex.location);
                    part_vertex.normal = current_transform.transform_vector3(part_vertex.normal);
                }
            }

            let child_meshes = match parent.get_value::<ElementArray>("children") {
                Some(children) => children,
                None => return Ok(()),
            };

            for child in child_meshes.iter().flatten() {
                load_mesh(child, current_transform, joints, file_data)?;
            }

            Ok(())
        }

        if let Some(children) = model.get_value::<ElementArray>("children") {
            for child in children.iter().flatten() {
                load_mesh(child, Math::Matrix4::IDENTITY, &joints, &mut file_data)?;
            }
        }
    }

    if let Some(animation_list) = file_root.get_value::<Element>("animationList") {
        let animations = get_attribute!(animation_list, "animations", ElementArray)?;
        for animation_clip in animations.iter().flatten() {
            if file_data.animations.contains_key(animation_clip.get_name().as_str()) {
                return Err(ParseDMXError::DuplicateAnimationName(
                    animation_clip.get_name().clone(),
                    *animation_clip.get_id(),
                ));
            }

            let animation = file_data.animations.entry(animation_clip.get_name().clone()).or_default();

            let frame_rate = *get_attribute!(animation_clip, "frameRate", Integer)? as f64;
            let time_frame = get_attribute!(animation_clip, "timeFrame", Element)?;
            let start = if file_header.format_version < 2 {
                get_attribute!(time_frame, "startTime", Integer)
                    .map(|t| *t as f64 / 10_000.0)
                    .unwrap_or_default()
            } else {
                get_attribute!(time_frame, "start", Time).map(|t| t.as_seconds_f64()).unwrap_or_default()
            };
            let duration = if file_header.format_version < 2 {
                *get_attribute!(time_frame, "durationTime", Integer)? as f64 / 10_000.0
            } else {
                get_attribute!(time_frame, "duration", Time)?.as_seconds_f64()
            };
            let start_frame = (start * frame_rate).ceil() as usize;
            let end_frame = ((start + duration) * frame_rate).ceil() as usize;
            let frame_count = end_frame - start_frame + 1;
            animation.frame_count = NonZeroUsize::new(frame_count).unwrap_or(NonZeroUsize::MIN);

            let channels = get_attribute!(animation_clip, "channels", ElementArray)?;
            for channel in channels.iter().flatten() {
                let joint_transform = get_attribute!(channel, "toElement", Element)?;
                let target_channel = get_attribute!(channel, "toAttribute", String)?;
                let log = get_attribute!(channel, "log", Element)?;
                let layers = get_attribute!(log, "layers", ElementArray)?;
                let layer_index = channel.get_value::<i32>("toIndex").map(|index| *index).unwrap_or_default();
                if layer_index < 0 || layer_index as usize >= layers.len() {
                    return Err(ParseDMXError::InvalidIndex("toIndex", *channel.get_id(), layer_index, layers.len()));
                }
                if let Some(layer) = &layers[layer_index as usize] {
                    let times = if file_header.format_version < 2 {
                        get_attribute!(layer, "times", IntegerArray)?
                            .iter()
                            .map(|&time| time as f64 / 10_000.0)
                            .collect::<Vec<_>>()
                    } else {
                        get_attribute!(layer, "times", TimeArray)?
                            .iter()
                            .map(|time| time.as_seconds_f64())
                            .collect::<Vec<_>>()
                    };

                    if target_channel.eq("position") {
                        let values = get_attribute!(layer, "values", Vector3Array)?;

                        if values.len() != times.len() {
                            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
                        }

                        let bone = file_data
                            .skeleton
                            .get_index_of(joint_transform.get_name().as_str())
                            .ok_or(ParseDMXError::InvalidAnimationJointTarget(*channel.get_id(), *joint_transform.get_id()))?;
                        let animation_channel = animation.channels.entry(bone).or_default();

                        for (frame, time) in times.into_iter().enumerate() {
                            let time_frame = (time * frame_rate).ceil() as usize;

                            if time_frame < start_frame || time_frame > frame_count {
                                continue;
                            }

                            let position = values[frame];

                            animation_channel
                                .location
                                .insert(time_frame, Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64));
                        }
                        continue;
                    }

                    if target_channel.eq("orientation") {
                        let values = get_attribute!(layer, "values", QuaternionArray)?;

                        if values.len() != times.len() {
                            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
                        }

                        let bone = file_data
                            .skeleton
                            .get_index_of(joint_transform.get_name().as_str())
                            .ok_or(ParseDMXError::InvalidAnimationJointTarget(*channel.get_id(), *joint_transform.get_id()))?;
                        let animation_channel = animation.channels.entry(bone).or_default();

                        for (frame, time) in times.into_iter().enumerate() {
                            let time_frame = (time * frame_rate).ceil() as usize;

                            if time_frame < start_frame || time_frame > frame_count {
                                continue;
                            }

                            let rotation = values[frame];

                            animation_channel.rotation.insert(
                                time_frame,
                                Math::Quaternion::from_xyzw(rotation.x as f64, rotation.y as f64, rotation.z as f64, rotation.w as f64),
                            );
                        }
                    }
                }
            }
        }
    } else {
        file_data.animations.insert(file_name, Default::default());
    }

    Ok(file_data)
}
