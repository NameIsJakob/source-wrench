use chrono::Duration;
use datamodel::{
    Element,
    attribute::{ElementArray, Quaternion, Vector2, Vector3},
    deserialize,
};
use indexmap::IndexSet;
use std::{fs::File, io::BufReader, num::NonZero};
use thiserror::Error as ThisError;
use uuid::Uuid as UUID;

use crate::utilities::mathematics as Math;

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
    #[error("DMX File Format Is Not A Model")]
    FormatNotModel,
    #[error("DMX File Format Version Is Not Supported")]
    UnsupportedFormatVersion,
    #[error("DMX File Missing Skeleton Element")]
    MissingSkeleton,
    #[error("Missing Required Attribute \"{0}\" Of Type \"{1}\" On Element \"{2}\"")]
    MissingRequiredAttribute(&'static str, &'static str, UUID),
    #[error("Duplicate Joint Name \"{0}\" Element \"{1}\"")]
    DuplicateJointName(String, UUID),
    #[error("\"{0}\" Array Length Is Not The Same As \"{1}\" For Element \"{2}\"")]
    MissedMatchedArray(&'static str, &'static str, UUID),
    #[error("Duplicate Part Name \"{0}\" Element \"{1}\"")]
    DuplicatePartName(String, UUID),
    #[error("Duplicate Flex Name \"{0}\" Element \"{1}\"")]
    DuplicateFlexName(String, UUID),
    #[error("Duplicate Animation Name \"{0}\" Element \"{1}\"")]
    DuplicateAnimationName(String, UUID),
}

// FIXME: There is a lot of unchecked accesses to arrays, these need to be checked and if out of bounds then it should error.

pub fn load_dmx(mut file_buffer: BufReader<File>, file_name: String) -> Result<super::FileData, ParseDMXError> {
    let (file_header, file_root) = deserialize(&mut file_buffer).unwrap();

    if file_header.get_format() != "model" {
        return Err(ParseDMXError::FormatNotModel);
    }

    if file_header.format_version < 1 || file_header.format_version > 18 {
        return Err(ParseDMXError::UnsupportedFormatVersion);
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

    let skeleton = file_root.get_value::<Element>("skeleton").ok_or(ParseDMXError::MissingSkeleton)?;
    fn load_joints(current_joint: &Element, parent_index: Option<usize>, file_data: &mut super::FileData) -> Result<(), ParseDMXError> {
        if file_data.parts.contains_key(current_joint.get_name().as_str()) {
            return Err(ParseDMXError::DuplicateJointName(current_joint.get_name().clone(), *current_joint.get_id()));
        }

        let joint_index = Some(file_data.skeleton.len());

        let current_transform = get_attribute!(current_joint, "transform", Element)?;
        let position = get_attribute!(current_transform, "position", Vector3)?;
        let orientation = get_attribute!(current_transform, "orientation", Quaternion)?;

        file_data.skeleton.insert(
            current_joint.get_name().clone(),
            super::Bone {
                parent: parent_index,
                location: Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64),
                rotation: Math::Quaternion::new(orientation.x as f64, orientation.y as f64, orientation.z as f64, orientation.w as f64),
            },
        );

        let joints = match current_joint.get_value::<ElementArray>("children") {
            Some(children) => children,
            None => return Ok(()),
        };

        for joint in joints.iter().flatten() {
            load_joints(joint, joint_index, file_data)?;
        }

        Ok(())
    }

    let joints = get_attribute!(skeleton, "children", ElementArray)?;
    for joint in joints.iter().flatten() {
        load_joints(joint, None, &mut file_data)?;
    }

    if let Some(model) = file_root.get_value::<Element>("model") {
        let joint_list = if file_header.format_version < 8 {
            get_attribute!(model, "jointTransforms", ElementArray)?
        } else {
            get_attribute!(model, "jointList", ElementArray)?
        };
        let joints = joint_list.iter().flatten().collect::<Vec<_>>();

        fn load_meshes(
            current_mesh: &Element,
            joints: &[&Element],
            parent_transform: Math::Matrix4,
            file_data: &mut super::FileData,
        ) -> Result<(), ParseDMXError> {
            let current_transform = get_attribute!(current_mesh, "transform", Element)?;
            let position = get_attribute!(current_transform, "position", Vector3)?;
            let orientation = get_attribute!(current_transform, "orientation", Quaternion)?;

            let transform = parent_transform
                * Math::Matrix4::new(
                    Math::Quaternion::new(orientation.x as f64, orientation.y as f64, orientation.z as f64, orientation.w as f64),
                    Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64),
                );

            if let Some(shape) = current_mesh.get_value::<Element>("shape") {
                if let Some(states) = shape.get_value::<ElementArray>("baseStates") {
                    if let Some(bind_state) = states.iter().flatten().find(|state| state.get_name().eq("bind")) {
                        if file_data.parts.contains_key(shape.get_name().as_str()) {
                            return Err(ParseDMXError::DuplicatePartName(shape.get_name().clone(), *shape.get_id()));
                        }

                        let mut part = super::Part::default();

                        let positions_indices = get_attribute!(bind_state, "positionsIndices", IntegerArray)?;
                        let positions = get_attribute!(bind_state, "positions", Vector3Array)?;

                        let normals_indices = get_attribute!(bind_state, "normalsIndices", IntegerArray)?;
                        let normals = get_attribute!(bind_state, "normals", Vector3Array)?;

                        if normals_indices.len() != positions_indices.len() {
                            return Err(ParseDMXError::MissedMatchedArray("normalsIndices", "positionsIndices", *bind_state.get_id()));
                        }

                        let texture_coordinate_indices = get_attribute!(bind_state, "textureCoordinatesIndices", IntegerArray)?;
                        let texture_coordinates = get_attribute!(bind_state, "textureCoordinates", Vector2Array)?;

                        if texture_coordinate_indices.len() != positions_indices.len() {
                            return Err(ParseDMXError::MissedMatchedArray(
                                "textureCoordinatesIndices",
                                "positionsIndices",
                                *bind_state.get_id(),
                            ));
                        }

                        let joint_count = get_attribute!(bind_state, "jointCount", Integer)?;

                        #[derive(Eq, PartialEq, Hash)]
                        struct UniqueVertex {
                            position: i32,
                            normal: i32,
                            texture_coordinate: i32,
                        }
                        let mut unique_vertices = IndexSet::new();
                        let mut vertex_remap = Vec::new();

                        let inverse_transform = transform.inverse();
                        let rotation = inverse_transform.rotation();
                        let translation = inverse_transform.translation();

                        for index in 0..positions_indices.len() {
                            let unique = UniqueVertex {
                                position: positions_indices[index],
                                normal: normals_indices[index],
                                texture_coordinate: texture_coordinate_indices[index],
                            };

                            if let Some(unique_index) = unique_vertices.get_index_of(&unique) {
                                vertex_remap.push(unique_index);
                                continue;
                            }

                            vertex_remap.push(unique_vertices.len());
                            unique_vertices.insert(unique);

                            let position = positions[positions_indices[index] as usize];
                            let normal = normals[normals_indices[index] as usize];
                            let texture_coordinate = texture_coordinates[texture_coordinate_indices[index] as usize];

                            let vertex_position =
                                rotation.rotate_vector(Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64)) + translation;
                            let vertex_normal = rotation.rotate_vector(Math::Vector3::new(normal.x as f64, normal.y as f64, normal.z as f64));
                            let vertex_texture_coordinate = Math::Vector2::new(texture_coordinate.x as f64, texture_coordinate.y as f64);

                            let mut vertex = super::Vertex {
                                location: vertex_position,
                                normal: vertex_normal,
                                texture_coordinate: vertex_texture_coordinate,
                                ..Default::default()
                            };

                            if *joint_count == 0 {
                                vertex.links.insert(file_data.skeleton.len() - 1, 1.0);
                                part.vertices.push(vertex);
                                continue;
                            }

                            let joint_indices = get_attribute!(bind_state, "jointIndices", IntegerArray)?;

                            if joint_indices.len() != positions.len() * *joint_count as usize {
                                return Err(ParseDMXError::MissedMatchedArray("jointIndices", "positions", *bind_state.get_id()));
                            }

                            let joint_weights = get_attribute!(bind_state, "jointWeights", FloatArray)?;

                            if joint_weights.len() != joint_indices.len() {
                                return Err(ParseDMXError::MissedMatchedArray("jointWeights", "jointIndices", *bind_state.get_id()));
                            }

                            for link_index in 0..*joint_count {
                                let joint_index = joint_indices[(positions_indices[index] * *joint_count + link_index) as usize];
                                let joint_weight = joint_weights[(positions_indices[index] * *joint_count + link_index) as usize];

                                if joint_weight == 0.0 {
                                    continue;
                                }

                                let link = file_data.skeleton.get_index_of(joints[joint_index as usize].get_name().as_str()).unwrap();
                                vertex.links.insert(link, joint_weight as f64);
                            }
                            part.vertices.push(vertex);
                        }

                        let face_sets = get_attribute!(shape, "faceSets", ElementArray)?;

                        for face_set in face_sets.iter().flatten() {
                            let face_indexes = get_attribute!(face_set, "faces", IntegerArray)?;
                            let material = get_attribute!(face_set, "material", Element)?;
                            let material_name = get_attribute!(material, "mtlName", String)?;
                            let mut faces = Vec::new();
                            let mut face = Vec::new();
                            for &face_index in face_indexes.iter() {
                                if face_index == -1 {
                                    faces.push(face.clone());
                                    face.clear();
                                    continue;
                                }
                                face.push(vertex_remap[face_index as usize]);
                            }
                            part.polygons.insert(material_name.clone(), faces);
                        }

                        if let Some(delta_states) = shape.get_value::<ElementArray>("deltaStates") {
                            for delta_state in delta_states.iter().flatten() {
                                if part.flexes.contains_key(delta_state.get_name().as_str()) {
                                    return Err(ParseDMXError::DuplicateFlexName(delta_state.get_name().clone(), *shape.get_id()));
                                }

                                let flex = part.flexes.entry(delta_state.get_name().clone()).or_default();

                                let positions_indices = get_attribute!(delta_state, "positionsIndices", IntegerArray)?;
                                let positions = get_attribute!(delta_state, "positions", Vector3Array)?;

                                for (position_index, &position_vertex_index) in positions_indices.iter().enumerate() {
                                    let vertex = flex.entry(vertex_remap[position_vertex_index as usize]).or_default();
                                    let position = positions[position_index];
                                    let vertex_position = rotation.rotate_vector(Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64));
                                    vertex.location = vertex_position;
                                }

                                let normals_indices = get_attribute!(delta_state, "normalsIndices", IntegerArray)?;
                                let normals = get_attribute!(delta_state, "normals", Vector3Array)?;

                                for (normal_index, &normal_vertex_index) in normals_indices.iter().enumerate() {
                                    let vertex = flex.entry(vertex_remap[normal_vertex_index as usize]).or_default();
                                    let normal = normals[normal_index];
                                    let vertex_normal = rotation.rotate_vector(Math::Vector3::new(normal.x as f64, normal.y as f64, normal.z as f64));
                                    vertex.normal = vertex_normal;
                                }
                            }
                        }

                        file_data.parts.insert(shape.get_name().clone(), part);
                    };
                }
            }

            let meshes = match current_mesh.get_value::<ElementArray>("children") {
                Some(children) => children,
                None => return Ok(()),
            };
            for mesh in meshes.iter().flatten() {
                load_meshes(mesh, joints, transform, file_data)?;
            }

            Ok(())
        }

        let meshes = get_attribute!(skeleton, "children", ElementArray)?;
        for mesh in meshes.iter().flatten() {
            load_meshes(mesh, &joints, Math::Matrix4::default(), &mut file_data)?;
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

            animation.frame_count = NonZero::new(frame_count).unwrap();

            let channels = get_attribute!(animation_clip, "channels", ElementArray)?;
            for channel in channels.iter().flatten() {
                let joint_transform = get_attribute!(channel, "toElement", Element)?;
                let target_channel = get_attribute!(channel, "toAttribute", String)?;
                let log = get_attribute!(channel, "log", Element)?;

                let layers = get_attribute!(log, "layers", ElementArray)?;
                // Should this use "toIndex"?
                if let Some(layer) = layers.iter().flatten().next() {
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

                        let bone = file_data.skeleton.get_index_of(joint_transform.get_name().as_str()).unwrap();
                        let animation_channel = animation.channels.entry(bone).or_default();

                        for (frame, time) in times.into_iter().enumerate() {
                            let time_frame = (time * frame_rate).ceil() as usize;

                            if time_frame < start_frame {
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

                        let bone = file_data.skeleton.get_index_of(joint_transform.get_name().as_str()).unwrap();
                        let animation_channel = animation.channels.entry(bone).or_default();

                        for (frame, time) in times.into_iter().enumerate() {
                            let time_frame = (time * frame_rate).ceil() as usize;

                            if time_frame < start_frame {
                                continue;
                            }

                            let rotation = values[frame];

                            animation_channel.rotation.insert(
                                time_frame,
                                Math::Quaternion::new(rotation.x as f64, rotation.y as f64, rotation.z as f64, rotation.w as f64),
                            );
                        }
                    }
                }
            }
        }
    } else {
        file_data.animations.insert(file_name, super::Animation::default());
    }

    Ok(file_data)
}
