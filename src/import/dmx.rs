use datamodel::{
    Element, ElementClass, SerializationError,
    attribute::{AttributeElement, AttributeElementArray, AttributeVariable, Quaternion, Time, UUID, Vector2, Vector3},
    deserialize,
};
use indexmap::{IndexMap, IndexSet};
use std::{fs::File, io::BufReader, num::NonZeroUsize};
use thiserror::Error as ThisError;

use crate::{import::FileData, utilities::mathematics as Math};

type Integer = i32;
type IntegerArray = Vec<i32>;
type FloatArray = Vec<f32>;
type Vector2Array = Vec<Vector2>;
type Vector3Array = Vec<Vector3>;
type QuaternionArray = Vec<Quaternion>;

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

    if file_header.format != "model" {
        return Err(ParseDMXError::FormatNotModel(file_header.format.clone()));
    }

    if file_header.format_version < 1 || file_header.format_version > 18 {
        return Err(ParseDMXError::UnsupportedFormatVersion(file_header.format_version));
    }

    let file_format_data = FileModelData::from_element(file_root);

    let mut file_data = super::FileData {
        up: Math::AxisDirection::PositiveZ,
        forward: Math::AxisDirection::NegativeY,
        ..Default::default()
    };

    let mut joints = IndexSet::new();
    if let Some(model) = file_format_data.model.get() {
        let joint_list = if file_header.format_version < 8 {
            model.joint_transforms.get()
        } else {
            model.joint_list.get()
        };

        for (joint_index, joint) in joint_list.iter().enumerate() {
            if let Some(joint) = joint {
                joints.insert(Element::clone(joint));
                continue;
            }
            return Err(ParseDMXError::NullElementInArray(
                joint_index,
                if file_header.format_version < 8 { "jointTransforms" } else { "jointList" },
                *model.into_element().get_id(),
            ));
        }
    }

    let skeleton = file_format_data.skeleton.get().ok_or(ParseDMXError::MissingRequiredAttribute(
        "skeleton",
        "Element",
        *file_format_data.clone().into_element().get_id(),
    ))?;
    fn load_joints(
        parent: &Element,
        parent_index: Option<usize>,
        joints: Option<&IndexSet<Element>>,
        file_data: &mut FileData,
        format_version: i32,
    ) -> Result<(), ParseDMXError> {
        let parent_dag = Dag::from_element(Element::clone(parent));

        if file_data.parts.contains_key(parent_dag.name.get().as_str()) {
            return Err(ParseDMXError::DuplicateJointName(parent_dag.name.get().clone(), *parent.get_id()));
        }

        let parent_transform = parent_dag
            .transform
            .get()
            .ok_or(ParseDMXError::MissingRequiredAttribute("transform", "Element", *parent.get_id()))?;
        let joint_position = *parent_transform.position.get();
        let joint_rotation = *parent_transform.orientation.get();

        if let Some(joints) = joints {
            let joint = if format_version < 8 { &parent_transform.into_element() } else { parent };
            if !joints.contains(joint) {
                return Err(ParseDMXError::JointNotInJointList(*parent.get_id()));
            }
        }

        let bone_index = Some(file_data.skeleton.len());

        file_data.skeleton.insert(
            parent_dag.name.get().clone(),
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

        for child in parent_dag.children.get().iter().flatten() {
            load_joints(child, bone_index, joints, file_data, format_version)?;
        }

        Ok(())
    }
    for child in skeleton.children.get().iter().flatten() {
        load_joints(
            child,
            None,
            if file_format_data.model.get().is_some() { Some(&joints) } else { None },
            &mut file_data,
            file_header.format_version,
        )?;
    }

    if let Some(model) = file_format_data.model.get() {
        fn load_mesh(parent: &Element, parent_transform: Math::Matrix4, joints: &IndexSet<Element>, file_data: &mut FileData) -> Result<(), ParseDMXError> {
            let parent_dag = Dag::from_element(Element::clone(parent));

            let mesh_transform = parent_dag
                .transform
                .get()
                .ok_or(ParseDMXError::MissingRequiredAttribute("transform", "Element", *parent.get_id()))?;
            let mesh_position = *mesh_transform.position.get();
            let mesh_rotation = *mesh_transform.orientation.get();
            let current_transform = parent_transform
                * Math::Matrix4::from_rotation_translation(
                    Math::Quaternion::from_xyzw(mesh_rotation.x as f64, mesh_rotation.y as f64, mesh_rotation.z as f64, mesh_rotation.w as f64),
                    Math::Vector3::new(mesh_position.x as f64, mesh_position.y as f64, mesh_position.z as f64),
                );

            if let Some(shape) = parent_dag.shape.get_as::<Mesh>()
                && !shape.base_states.get::<VertexData>().is_empty()
            {
                let bind_state = shape
                    .base_states
                    .get::<VertexData>()
                    .iter()
                    .flatten()
                    .find(|&state| state.name.get().eq("bind"))
                    .cloned()
                    .ok_or(ParseDMXError::MeshMissingBindSate(*parent.get_id()))?;

                let position_indices = bind_state.position_indices.get();
                let positions = bind_state.positions.get();
                let normals_indices = bind_state.normals_indices.get();
                let normals = bind_state.normals.get();
                let texture_coordinate_indices = bind_state.texture_coordinate_indices.get();
                let texture_coordinates = bind_state.texture_coordinates.get();

                if normals_indices.len() != position_indices.len() {
                    return Err(ParseDMXError::MissedMatchedArray(
                        "normalsIndices",
                        "positionsIndices",
                        *bind_state.name.owner().get_id(),
                    ));
                }
                if texture_coordinate_indices.len() != position_indices.len() {
                    return Err(ParseDMXError::MissedMatchedArray(
                        "textureCoordinatesIndices",
                        "positionsIndices",
                        *bind_state.name.owner().get_id(),
                    ));
                }

                if file_data.parts.contains_key(parent_dag.name.get().as_str()) {
                    return Err(ParseDMXError::DuplicatePartName(parent_dag.name.get().clone(), *parent.get_id()));
                }

                let part = file_data.parts.entry(shape.name.get().clone()).or_default();

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
                let mut position_index_map: IndexMap<i32, Vec<usize>> = IndexMap::new();
                let mut normal_index_map: IndexMap<i32, Vec<usize>> = IndexMap::new();
                let mut vertex_remap = Vec::with_capacity(position_indices.len());
                for vertex_index in 0..position_indices.len() {
                    let unique_vertex = UniqueVertex {
                        position: validate_index(position_indices[vertex_index], positions.len(), "positionsIndices", &bind_state.name.owner())?,
                        normal: validate_index(normals_indices[vertex_index], normals.len(), "normalsIndices", &bind_state.name.owner())?,
                        texture_coordinate: validate_index(
                            texture_coordinate_indices[vertex_index],
                            texture_coordinates.len(),
                            "textureCoordinatesIndices",
                            &bind_state.name.owner(),
                        )?,
                    };

                    if let Some(unique_index) = unique_vertices.get_index_of(&unique_vertex) {
                        vertex_remap.push(unique_index);
                        continue;
                    }

                    vertex_remap.push(unique_vertices.len());
                    position_index_map
                        .entry(position_indices[vertex_index])
                        .or_default()
                        .push(unique_vertices.len());
                    normal_index_map.entry(normals_indices[vertex_index]).or_default().push(unique_vertices.len());
                    unique_vertices.insert(unique_vertex);

                    let position = positions[position_indices[vertex_index] as usize];
                    let normal = normals[normals_indices[vertex_index] as usize];
                    let texture_coordinate = texture_coordinates[texture_coordinate_indices[vertex_index] as usize];

                    let mut vertex = super::Vertex {
                        location: Math::Vector3::new(position.x as f64, position.y as f64, position.z as f64),
                        normal: Math::Vector3::new(normal.x as f64, normal.y as f64, normal.z as f64),
                        texture_coordinate: Math::Vector2::new(
                            texture_coordinate.x as f64,
                            if *bind_state.flip_coordinates.get() {
                                texture_coordinate.y as f64
                            } else {
                                1.0 + texture_coordinate.y as f64
                            },
                        ),
                        ..Default::default()
                    };

                    let joint_count = (*bind_state.joint_count.get()).max(0) as usize;
                    if joint_count == 0 {
                        let parent_bone = file_data.skeleton.get_index_of(parent_dag.name.get().as_str()).unwrap_or_default();
                        vertex.links.insert(parent_bone, 1.0);
                        part.vertices.push(vertex);
                        continue;
                    }

                    let joint_indices = bind_state.joint_indices.get();
                    let joint_weights = bind_state.joint_weights.get();
                    if joint_indices.len() != positions.len() * joint_count {
                        return Err(ParseDMXError::MissedMatchedArray(
                            "jointIndices",
                            "positions",
                            *bind_state.name.owner().get_id(),
                        ));
                    }
                    if joint_weights.len() != joint_indices.len() {
                        return Err(ParseDMXError::MissedMatchedArray(
                            "jointWeights",
                            "jointIndices",
                            *bind_state.name.owner().get_id(),
                        ));
                    }

                    for joint_count_index in 0..joint_count {
                        let joint_index = joint_indices[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                        let joint_weight = joint_weights[(position_indices[vertex_index] as usize * joint_count) + joint_count_index];
                        let joint_element = &joints[validate_index(joint_index, joints.len(), "jointIndices", &bind_state.name.owner())? as usize];
                        let joint_dag = Dag::from_element(Element::clone(joint_element));
                        let joint_link = file_data.skeleton.get_index_of(joint_dag.name.get().as_str()).unwrap_or_default();
                        vertex.links.insert(joint_link, joint_weight as f64);
                    }
                    part.vertices.push(vertex);
                }

                let face_sets = shape.face_sets.get::<FaceSet>();
                for face_set in face_sets.iter().flatten() {
                    let face_indices = face_set.faces.get();
                    let material = face_set.material.get().unwrap();
                    let material_name = material.material_name.get();

                    let mut faces = Vec::new();
                    let mut face = Vec::new();
                    for &face_index in face_indices.iter() {
                        if face_index == -1 {
                            if face.len() < 3 {
                                return Err(ParseDMXError::IncompleteFace(*face_set.faces.owner().get_id()));
                            }
                            face.reverse();
                            faces.push(face.clone());
                            face.clear();
                            continue;
                        }

                        face.push(vertex_remap[validate_index(face_index, vertex_remap.len(), "faces", &bind_state.name.owner())? as usize]);
                    }
                    part.faces.entry(material_name.clone()).or_default().extend(faces);
                }

                for delta_state in shape.delta_states.get::<VertexDeltaData>().iter().flatten() {
                    if part.flexes.contains_key(delta_state.name.get().as_str()) {
                        return Err(ParseDMXError::DuplicateFlexName(delta_state.name.get().clone(), *shape.name.owner().get_id()));
                    }

                    let flex = part.flexes.entry(delta_state.name.get().clone()).or_default();

                    let delta_positions_indices = delta_state.position_indices.get();
                    let delta_positions = delta_state.positions.get();
                    if delta_positions.len() != delta_positions_indices.len() {
                        return Err(ParseDMXError::MissedMatchedArray(
                            "positionsIndices",
                            "positions",
                            *delta_state.name.owner().get_id(),
                        ));
                    }
                    for (delta_position_index, delta_position) in delta_positions.iter().enumerate() {
                        let delta_positions_index = delta_positions_indices[delta_position_index];
                        for &unique_vertex_index in position_index_map.get(&delta_positions_index).unwrap() {
                            let unique_vertex = &part.vertices[unique_vertex_index];
                            let unique_vertex_location = unique_vertex.location;
                            let delta_location = Math::Vector3::new(delta_position.x as f64, delta_position.y as f64, delta_position.z as f64);
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

                    let delta_normals_indices = delta_state.normals_indices.get();
                    let delta_normals = delta_state.normals.get();
                    if delta_normals.len() != delta_normals_indices.len() {
                        return Err(ParseDMXError::MissedMatchedArray(
                            "normalsIndices",
                            "normals",
                            *delta_state.name.owner().get_id(),
                        ));
                    }
                    for (delta_normal_index, delta_normal) in delta_normals.iter().enumerate() {
                        let delta_normals_index = delta_normals_indices[delta_normal_index];
                        for &unique_vertex_index in normal_index_map.get(&delta_normals_index).unwrap() {
                            let unique_vertex = &part.vertices[unique_vertex_index];
                            let unique_vertex_normal = unique_vertex.normal;
                            let delta_normal = Math::Vector3::new(delta_normal.x as f64, delta_normal.y as f64, delta_normal.z as f64);
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
                }

                for part_vertex in &mut part.vertices {
                    part_vertex.location = current_transform.transform_point3(part_vertex.location);
                    part_vertex.normal = current_transform.transform_vector3(part_vertex.normal);
                }
            }

            for child in parent_dag.children.get().iter().flatten() {
                load_mesh(child, current_transform, joints, file_data)?;
            }

            Ok(())
        }

        for child in model.children.get().iter().flatten() {
            load_mesh(child, Math::Matrix4::IDENTITY, &joints, &mut file_data)?;
        }
    }

    if let Some(animation_list) = file_format_data.animation_list.get() {
        let animations = animation_list.animations.get::<ChannelsClip>();
        for animation_clip in animations.iter().flatten() {
            if file_data.animations.contains_key(animation_clip.name.get().as_str()) {
                return Err(ParseDMXError::DuplicateAnimationName(
                    animation_clip.name.get().clone(),
                    *animation_clip.name.owner().get_id(),
                ));
            }

            let animation = file_data.animations.entry(animation_clip.name.get().clone()).or_default();

            let mut frame_rate = *animation_clip.frame_rate.get() as f32;
            if frame_rate <= 0.0 {
                frame_rate = 30.0;
            }
            let time_frame = animation_clip.time_frame.get().ok_or(ParseDMXError::MissingRequiredAttribute(
                "timeFrame",
                "Element",
                *animation_clip.name.owner().get_id(),
            ))?;
            let start = if file_header.format_version < 2 {
                Time(*time_frame.start_time.get()).as_seconds()
            } else {
                time_frame.start.get().as_seconds()
            };
            let duration = if file_header.format_version < 2 {
                Time(*time_frame.duration_time.get()).as_seconds()
            } else {
                time_frame.duration.get().as_seconds()
            };
            let start_frame = (start * frame_rate).ceil() as usize;
            let end_frame = ((start + duration) * frame_rate).ceil() as usize;
            let frame_count = end_frame - start_frame + 1;
            animation.frame_count = NonZeroUsize::new(frame_count).unwrap_or(NonZeroUsize::MIN);

            for channel in animation_clip.channels.get::<Channel>().iter().flatten() {
                let transform = Transform::from_element(channel.to_element.get().ok_or(ParseDMXError::MissingRequiredAttribute(
                    "toElement",
                    "Element",
                    *channel.to_element.owner().get_id(),
                ))?);
                let target_channel = channel.to_attribute.get();
                let log = channel
                    .log
                    .get()
                    .ok_or(ParseDMXError::MissingRequiredAttribute("log", "Element", *channel.log.owner().get_id()))?;
                let layers = log.layers.get::<Element>();
                let layer_index = *channel.to_index.get();
                if layer_index < 0 || layer_index as usize >= layers.len() {
                    return Err(ParseDMXError::InvalidIndex(
                        "toIndex",
                        *channel.to_element.owner().get_id(),
                        layer_index,
                        layers.len(),
                    ));
                }
                if let Some(layer) = &layers[layer_index as usize] {
                    let times = {
                        let times_type = if file_header.format_version < 2 { "IntegerArray" } else { "TimeArray" };
                        let times_attribute =
                            layer
                                .get_attribute("times")
                                .ok_or(ParseDMXError::MissingRequiredAttribute("times", times_type, *layer.get_id()))?;

                        let raw_times = times_attribute.get_inner();
                        match &*raw_times {
                            datamodel::attribute::AttributeValue::IntegerArray(times) => {
                                times.iter().map(|&time| Time(time).as_seconds()).collect::<FloatArray>()
                            }
                            datamodel::attribute::AttributeValue::TimeArray(times) => times.iter().map(|time| time.as_seconds()).collect::<FloatArray>(),
                            _ => return Err(ParseDMXError::MissingRequiredAttribute("times", "IntegerArray", *layer.get_id())),
                        }
                    };

                    if target_channel.eq("position") {
                        let log_layer = Vector3LogLayer::from_element(Element::clone(layer));
                        let values = log_layer.values.get();

                        if values.len() != times.len() {
                            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
                        }

                        let bone = file_data
                            .skeleton
                            .get_index_of(transform.name.get().as_str())
                            .ok_or(ParseDMXError::InvalidAnimationJointTarget(
                                *channel.to_element.owner().get_id(),
                                *transform.name.owner().get_id(),
                            ))?;
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
                        let log_layer = QuaternionLogLayer::from_element(Element::clone(layer));
                        let values = log_layer.values.get();

                        if values.len() != times.len() {
                            return Err(ParseDMXError::MissedMatchedArray("times", "values", *layer.get_id()));
                        }

                        let bone = file_data
                            .skeleton
                            .get_index_of(transform.name.get().as_str())
                            .ok_or(ParseDMXError::InvalidAnimationJointTarget(
                                *channel.to_element.owner().get_id(),
                                *transform.name.owner().get_id(),
                            ))?;
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

#[derive(Clone, ElementClass)]
#[class_name("DmElement")]
struct FileModelData {
    skeleton: AttributeElement<Dag>,
    model: AttributeElement<Model>,
    #[attribute_name("animationList")]
    animation_list: AttributeElement<AnimationList>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeTransform")]
struct Transform {
    name: AttributeVariable<String>,
    position: AttributeVariable<Vector3>,
    orientation: AttributeVariable<Quaternion>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeDag")]
struct Dag {
    name: AttributeVariable<String>,
    transform: AttributeElement<Transform>,
    shape: AttributeElement<Element>,
    children: AttributeElementArray<Dag>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeVertexData")]
pub struct VertexData {
    name: AttributeVariable<String>,
    #[attribute_name("jointCount")]
    joint_count: AttributeVariable<i32>,
    #[attribute_name("flipVCoordinates")]
    flip_coordinates: AttributeVariable<bool>,
    #[attribute_name("positionsIndices")]
    position_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("positions")]
    positions: AttributeVariable<Vector3Array>,
    #[attribute_name("normalsIndices")]
    normals_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("normals")]
    normals: AttributeVariable<Vector3Array>,
    #[attribute_name("textureCoordinatesIndices")]
    texture_coordinate_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("textureCoordinates")]
    texture_coordinates: AttributeVariable<Vector2Array>,
    #[attribute_name("jointIndices")]
    joint_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("jointWeights")]
    joint_weights: AttributeVariable<FloatArray>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeVertexDeltaData")]
pub struct VertexDeltaData {
    name: AttributeVariable<String>,
    #[attribute_name("positionsIndices")]
    position_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("positions")]
    positions: AttributeVariable<Vector3Array>,
    #[attribute_name("normalsIndices")]
    normals_indices: AttributeVariable<IntegerArray>,
    #[attribute_name("normals")]
    normals: AttributeVariable<Vector3Array>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeMaterial")]
pub struct Material {
    #[attribute_name("mtlName")]
    material_name: AttributeVariable<String>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeFaceSet")]
pub struct FaceSet {
    faces: AttributeVariable<IntegerArray>,
    material: AttributeElement<Material>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeMesh")]
pub struct Mesh {
    name: AttributeVariable<String>,
    #[attribute_name("baseStates")]
    base_states: AttributeElementArray<VertexData>,
    #[attribute_name("deltaStates")]
    delta_states: AttributeElementArray<VertexDeltaData>,
    #[attribute_name("faceSets")]
    face_sets: AttributeElementArray<FaceSet>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeModel")]
struct Model {
    name: AttributeVariable<String>,
    children: AttributeElementArray<Dag>,
    #[attribute_name("jointTransforms")]
    joint_transforms: AttributeElementArray<Transform>,
    #[attribute_name("jointList")]
    joint_list: AttributeElementArray<Dag>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeTimeFrame")]
struct TimeFrame {
    #[attribute_name("startTime")]
    start_time: AttributeVariable<Integer>,
    start: AttributeVariable<Time>,
    #[attribute_name("durationTime")]
    duration_time: AttributeVariable<Integer>,
    duration: AttributeVariable<Time>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeVector3LogLayer")]
struct Vector3LogLayer {
    values: AttributeVariable<Vector3Array>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeQuaternionLogLayer")]
struct QuaternionLogLayer {
    values: AttributeVariable<QuaternionArray>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeLog")]
struct Log {
    layers: AttributeElementArray<Element>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeChannel")]
struct Channel {
    #[attribute_name("toElement")]
    to_element: AttributeElement<Element>,
    #[attribute_name("toAttribute")]
    to_attribute: AttributeVariable<String>,
    #[attribute_name("toIndex")]
    to_index: AttributeVariable<Integer>,
    log: AttributeElement<Log>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeChannelsClip")]
struct ChannelsClip {
    name: AttributeVariable<String>,
    #[attribute_name("timeFrame")]
    time_frame: AttributeElement<TimeFrame>,
    channels: AttributeElementArray<Channel>,
    #[attribute_name("frameRate")]
    frame_rate: AttributeVariable<Integer>,
}

#[derive(Clone, ElementClass)]
#[class_name("DmeAnimationList")]
struct AnimationList {
    animations: AttributeElementArray<ChannelsClip>,
}
