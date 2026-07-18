use indexmap::IndexMap;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::path::PathBuf;
use thiserror::Error as ThisError;

use crate::{
    debug,
    import::FileManager,
    input,
    utilities::mathematics::{AxisDirection, EULER_ROTATION, Matrix4, Quaternion, Vector3, create_space_transform},
    verbose, warn,
};

#[derive(Debug, ThisError)]
pub enum ProcessingBoneError {
    #[error("Model \"{0}\" In Model Group \"{1}\" Has No File Source")]
    NoModelFileSource(String, String),
    #[error("Animation \"{0}\" Has No File Source")]
    NoAnimationFileSource(String),
    #[error("File Source Not Loaded \"{0}\"")]
    FileSourceNotLoaded(PathBuf),
    #[error("Model Has Too Many Bone")]
    TooManyBones,
    #[error("Bone \"{0}\" Enforced Parent Bone \"{1}\" Doesn't Exist")]
    ParentNotFound(String, String),
    #[error("Duplicate Chain Name \"{0}\"")]
    DuplicateChainName(String),
    #[error("Foot Bone \"{0}\" Missing For Ik Chain \"{1}\"")]
    MissingIkBone(String, String),
    #[error("Ik Chain Bone \"{0}\" Must Have A Parent")]
    IkBoneIsRoot(String),
}

pub fn process_bones(input_data: &input::SourceInput, source_files: &FileManager) -> Result<super::BoneData, ProcessingBoneError> {
    let mut processed_bones = IndexMap::new();

    for input_model_group in &input_data.model_groups {
        for input_model in &input_model_group.models {
            if input_model.blank {
                continue;
            }

            load_bones_from_model_source(input_model, &input_model_group.name, source_files, &mut processed_bones)?;
        }
    }

    for input_animation in &input_data.animations {
        load_bones_from_animation_source(input_animation, source_files, &mut processed_bones)?;
    }

    verbose!("Loaded {} source bones", processed_bones.len());

    add_define_bones(&input_data.bone_properties, &mut processed_bones)?;

    enforce_bone_hierarchy(&input_data.bone_properties, &mut processed_bones)?;

    // TODO: Check for circular bone hierarchy.

    add_flags_from_property(&input_data.bone_properties, &mut processed_bones)?;

    enforce_bone_transforms(&input_data.bone_properties, &mut processed_bones);

    create_bone_world_transform_matrixes(&mut processed_bones); // This can move to enforce bone transform if needed world transforms.

    collapse_unused_bones(&mut processed_bones);

    if processed_bones.len() > (i8::MAX as usize) + 1 {
        return Err(ProcessingBoneError::TooManyBones);
    }

    // TODO: Enforce the order of the bone properties to the processed bones order.

    // Update bones local location and orientation.
    for source_bone_index in 0..processed_bones.len() {
        if let Some(parent_matrix) = processed_bones[source_bone_index]
            .parent
            .map(|parent_index| processed_bones[parent_index].world_transform)
        {
            let bone = &mut processed_bones[source_bone_index];
            let local_pose = parent_matrix.inverse() * bone.world_transform;
            bone.rotation = Quaternion::from_affine3(&local_pose);
            bone.location = local_pose.translation;
            continue;
        }

        let bone = &mut processed_bones[source_bone_index];
        bone.rotation = Quaternion::from_affine3(&bone.world_transform);
        bone.location = bone.world_transform.translation;
    }

    let ik_chains = create_ik_chains(&input_data.bone_properties, &processed_bones)?;

    let mut sorted_bones_by_name = (0..processed_bones.len() as u8).collect::<Vec<_>>();
    sorted_bones_by_name.sort_by(|from, to| {
        let bone_from = processed_bones.get_index(*from as usize).unwrap().0;
        let bone_to = processed_bones.get_index(*to as usize).unwrap().0;
        bone_from.cmp(bone_to)
    });

    Ok(super::BoneData {
        processed_bones,
        sorted_bones_by_name,
        ik_chains,
    })
}

fn load_bones_from_model_source(
    model: &input::Model,
    model_group_name: &str,
    source_files: &FileManager,
    processed_bones: &mut IndexMap<String, super::Bone>,
) -> Result<(), ProcessingBoneError> {
    let source_file_path = model
        .source_file_path
        .as_ref()
        .ok_or(ProcessingBoneError::NoModelFileSource(model.name.clone(), model_group_name.to_string()))?;

    let imported_file = source_files
        .get_file_data(source_file_path)
        .ok_or(ProcessingBoneError::FileSourceNotLoaded(source_file_path.clone()))?;

    for (import_bone_index, (import_bone_name, import_bone)) in imported_file.skeleton.iter().enumerate() {
        let mut bone_flags = super::BoneFlags::default();
        for (import_part_name, import_part) in &imported_file.parts {
            if model.disabled_parts.contains(import_part_name) {
                continue;
            }

            if import_part.vertices.par_iter().any(|vertex| vertex.links.contains_key(&import_bone_index)) {
                bone_flags.insert(super::BoneFlags::USED_BY_VERTEX);
                bone_flags.insert(super::BoneFlags::USED_BY_HITBOX); // TODO: Remove when added bone hitboxes.
            }
        }

        // TODO: Add warns if the bone transform and parent is different from the already loaded bones.

        if let Some(global_bone) = processed_bones.get_mut(import_bone_name) {
            global_bone.flags.insert(bone_flags);
            continue;
        }

        let parent_index = import_bone.parent.map(|index| {
            let (parent_name, _) = imported_file.skeleton.get_index(index).expect("Source Bone Parent Index Should Be Valid");
            processed_bones.get_index_of(parent_name).expect("Parent Bone Should Already Be Loaded")
        });

        let source_transform = create_space_transform(imported_file.up, imported_file.forward);
        let bone_matrix = Matrix4::from_rotation_translation(import_bone.rotation, import_bone.location);
        let bone_transform = if parent_index.is_none() {
            source_transform.inverse() * bone_matrix
        } else {
            bone_matrix
        };

        debug!("Loaded bone \"{import_bone_name}\" from {source_file_path:?}");
        processed_bones.insert(
            import_bone_name.clone(),
            super::Bone {
                parent: parent_index,
                location: bone_transform.translation,
                rotation: Quaternion::from_affine3(&bone_transform),
                flags: bone_flags,
                ..Default::default()
            },
        );
    }

    Ok(())
}

fn load_bones_from_animation_source(
    animation: &input::Animation,
    source_files: &FileManager,
    processed_bones: &mut IndexMap<String, super::Bone>,
) -> Result<(), ProcessingBoneError> {
    let source_file_path = animation
        .source_file_path
        .as_ref()
        .ok_or(ProcessingBoneError::NoAnimationFileSource(animation.name.clone()))?;

    let imported_file = source_files
        .get_file_data(source_file_path)
        .ok_or(ProcessingBoneError::FileSourceNotLoaded(source_file_path.clone()))?;

    for (import_bone_name, import_bone) in &imported_file.skeleton {
        let bone_flags = super::BoneFlags::default();

        // TODO: Add flags for animated bones.

        if let Some(global_bone) = processed_bones.get_mut(import_bone_name) {
            global_bone.flags.insert(bone_flags);
            continue;
        }

        let parent_index = import_bone.parent.map(|index| {
            let (parent_name, _) = imported_file.skeleton.get_index(index).expect("Source Bone Parent Index Should Be Valid");
            processed_bones.get_index_of(parent_name).expect("Parent Bone Should Already Be Loaded")
        });

        let source_transform = create_space_transform(imported_file.up, imported_file.forward);
        let bone_matrix = Matrix4::from_rotation_translation(import_bone.rotation, import_bone.location);
        let bone_transform = if parent_index.is_none() {
            source_transform.inverse() * bone_matrix
        } else {
            bone_matrix
        };

        debug!("Loaded bone \"{import_bone_name}\" from {source_file_path:?}");
        processed_bones.insert(
            import_bone_name.clone(),
            super::Bone {
                parent: parent_index,
                location: bone_transform.translation,
                rotation: Quaternion::from_affine3(&bone_transform),
                flags: bone_flags,
                ..Default::default()
            },
        );
    }

    Ok(())
}

fn add_define_bones(bone_properties: &[input::BoneProperty], processed_bones: &mut IndexMap<String, super::Bone>) -> Result<(), ProcessingBoneError> {
    let mut define_count = 0;
    for property in bone_properties {
        // TODO: Change this to a boolean to specify that the bone is defined.
        if !property.define_location && !property.define_rotation {
            continue;
        }

        if let Some(process_bone) = processed_bones.get_mut(&property.name) {
            process_bone.flags.insert(super::BoneFlags::BONE_DEFINED);
            continue;
        }

        let parent = if property.define_parent {
            if property.parent.is_empty() {
                None
            } else {
                if let Some(parent_index) = processed_bones.get_index_of(&property.parent) {
                    Some(parent_index)
                } else {
                    return Err(ProcessingBoneError::ParentNotFound(property.name.clone(), property.parent.clone()));
                }
            }
        } else {
            None
        };

        let location = if property.define_location { property.location } else { Vector3::ZERO };
        let rotation = if property.define_rotation {
            Quaternion::from_euler(
                EULER_ROTATION,
                property.rotation.z.to_radians(),
                property.rotation.x.to_radians(),
                property.rotation.y.to_radians(),
            )
        } else {
            Quaternion::IDENTITY
        };

        // TODO: This should be past from input.
        let space_transform = create_space_transform(AxisDirection::PositiveZ, AxisDirection::NegativeY);
        let define_matrix = Matrix4::from_rotation_translation(rotation, location);
        let define_transform = space_transform.inverse() * define_matrix;

        debug!("Defined bone \"{}\"", property.name);
        processed_bones.insert(
            property.name.clone(),
            super::Bone {
                parent,
                location: define_transform.translation,
                rotation: Quaternion::from_affine3(&define_transform),
                flags: super::BoneFlags::BONE_DEFINED,
                ..Default::default()
            },
        );
        define_count += 1;
    }
    verbose!("Added {define_count} defined bones");
    Ok(())
}

fn enforce_bone_hierarchy(bone_properties: &[input::BoneProperty], processed_bones: &mut IndexMap<String, super::Bone>) -> Result<(), ProcessingBoneError> {
    for property in bone_properties {
        if !property.define_parent {
            continue;
        }

        if !processed_bones.contains_key(&property.name) {
            continue;
        }

        if let Some(process_bone) = processed_bones.get_mut(&property.name)
            && property.parent.is_empty()
        {
            debug!("Enforcing \"{}\" as a root", property.name);
            process_bone.parent = None;
            continue;
        }

        let parent_index = processed_bones
            .get_index_of(&property.parent)
            .ok_or(ProcessingBoneError::ParentNotFound(property.name.clone(), property.parent.clone()))?;

        if let Some(process_bone) = processed_bones.get_mut(&property.name) {
            debug!("Enforcing \"{}\" parented to \"{}\"", property.name, property.parent);
            process_bone.parent = Some(parent_index);
        }
    }

    Ok(())
}

fn add_flags_from_property(bone_properties: &[input::BoneProperty], processed_bones: &mut IndexMap<String, super::Bone>) -> Result<(), ProcessingBoneError> {
    for property in bone_properties {
        let process_bone = match processed_bones.get_mut(&property.name) {
            Some(process_bone) => process_bone,
            None => {
                warn!("Bone \"{}\" does not exist to apply properties to it", property.name);
                continue;
            }
        };

        if property.ik_chain {
            process_bone.flags.insert(super::BoneFlags::USED_BY_ATTACHMENT);

            if let Some(knee_bone_index) = process_bone.parent {
                let (knee_bone_name, knee_bone) = processed_bones
                    .get_index_mut(knee_bone_index)
                    .expect("Ik Foot Parent Bone Index Should Be Valid");

                knee_bone.flags.insert(super::BoneFlags::USED_BY_ATTACHMENT);

                if let Some(hip_bone_index) = knee_bone.parent {
                    let (_, hip_bone) = processed_bones
                        .get_index_mut(hip_bone_index)
                        .expect("Ik Knee Parent Bone Index Should Be Valid");
                    hip_bone.flags.insert(super::BoneFlags::USED_BY_ATTACHMENT);
                } else {
                    return Err(ProcessingBoneError::IkBoneIsRoot(knee_bone_name.clone()));
                }
            } else {
                return Err(ProcessingBoneError::IkBoneIsRoot(property.name.clone()));
            }
        }
    }

    Ok(())
}

fn create_bone_world_transform_matrixes(processed_bones: &mut IndexMap<String, super::Bone>) {
    for source_bone_index in 0..processed_bones.len() {
        if let Some(parent_matrix) = processed_bones[source_bone_index]
            .parent
            .map(|parent_index| processed_bones[parent_index].world_transform)
        {
            let bone = &mut processed_bones[source_bone_index];
            let transform_matrix = parent_matrix * Matrix4::from_rotation_translation(bone.rotation, bone.location);
            bone.world_transform = transform_matrix;
            continue;
        }

        let bone = &mut processed_bones[source_bone_index];
        bone.world_transform = Matrix4::from_rotation_translation(bone.rotation, bone.location);
    }
}

fn enforce_bone_transforms(bone_properties: &[input::BoneProperty], processed_bones: &mut IndexMap<String, super::Bone>) {
    for property in bone_properties {
        if !property.define_parent {
            continue;
        }

        if !property.define_location && !property.define_rotation {
            continue;
        }

        if let Some(process_bone) = processed_bones.get_mut(&property.name) {
            let location = if property.define_location { property.location } else { process_bone.location };
            let rotation = if property.define_rotation {
                Quaternion::from_euler(
                    EULER_ROTATION,
                    property.rotation.z.to_radians(),
                    property.rotation.x.to_radians(),
                    property.rotation.y.to_radians(),
                )
            } else {
                process_bone.rotation
            };

            // TODO: This should be past from input.
            let space_transform = create_space_transform(AxisDirection::PositiveZ, AxisDirection::NegativeY);
            let define_matrix = Matrix4::from_rotation_translation(rotation, location);
            let define_transform = space_transform.inverse() * define_matrix;

            process_bone.location = define_transform.translation;
            process_bone.rotation = Quaternion::from_affine3(&define_transform);
        }
    }
}

fn collapse_unused_bones(processed_bones: &mut IndexMap<String, super::Bone>) {
    let mut current_bone_index = 0;
    let mut collapse_count = 0;
    while current_bone_index < processed_bones.len() {
        let (current_bone_name, current_bone) = processed_bones.get_index(current_bone_index).expect("Current Bone Index Should Be Valid");
        if !current_bone.flags.is_empty() {
            current_bone_index += 1;
            continue;
        }

        collapse_count += 1;
        debug!("Collapsing \"{current_bone_name}\"!");

        let current_bone_parent = current_bone.parent;
        processed_bones.shift_remove_index(current_bone_index);

        for bone_index in current_bone_index..processed_bones.len() {
            let bone = &mut processed_bones[bone_index];
            if let Some(bone_parent) = bone.parent {
                if bone_parent == current_bone_index {
                    bone.parent = current_bone_parent;
                    continue;
                }

                if bone_parent >= current_bone_index {
                    bone.parent = Some(bone_parent - 1);
                }
            }
        }
    }
    verbose!("Collapsed {collapse_count} bones.");
}

fn create_ik_chains(
    bone_properties: &[input::BoneProperty],
    bones: &IndexMap<String, super::Bone>,
) -> Result<IndexMap<String, super::IKChain>, ProcessingBoneError> {
    let mut ik_chains = IndexMap::new();

    for property in bone_properties {
        if !property.ik_chain {
            continue;
        }

        if ik_chains.contains_key(&property.ik_chain_name) {
            return Err(ProcessingBoneError::DuplicateChainName(property.ik_chain_name.clone()));
        }

        let mut ik_chain = super::IKChain::default();

        let (foot_bone_index, foot_bone_name, foot_bone) = bones
            .get_full(&property.name)
            .ok_or(ProcessingBoneError::MissingIkBone(property.name.clone(), property.ik_chain_name.clone()))?;

        ik_chain.links[2] = foot_bone_index as i32;

        if let Some(knee_bone_index) = foot_bone.parent {
            let (knee_bone_name, knee_bone) = bones.get_index(knee_bone_index).expect("Ik Foot Parent Bone Index Should Be Valid");

            ik_chain.links[1] = knee_bone_index as i32;

            if let Some(hip_bone_index) = knee_bone.parent {
                ik_chain.links[0] = hip_bone_index as i32;
            } else {
                return Err(ProcessingBoneError::IkBoneIsRoot(knee_bone_name.clone()));
            }
        } else {
            return Err(ProcessingBoneError::IkBoneIsRoot(foot_bone_name.clone()));
        }

        ik_chain.knee_direction = property.ik_chain_knee.normalize_or_zero();

        if property.ik_chain_auto_play {
            ik_chain.auto_play_lock = Some(super::IkLock {
                chain: ik_chains.len() as i32,
                position_weight: property.ik_chain_position_lock,
                rotation_weight: 1.0 - property.ik_chain_rotation_lock,
            });
        }

        ik_chains.insert(property.ik_chain_name.clone(), ik_chain);
    }

    Ok(ik_chains)
}
