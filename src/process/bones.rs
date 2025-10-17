use indexmap::IndexMap;
use std::path::PathBuf;
use thiserror::Error as ThisError;

use crate::{
    debug,
    import::FileManager,
    input,
    utilities::mathematics::{Matrix4, Quaternion, create_space_transform},
    verbose,
};

#[derive(Debug, ThisError)]
pub enum ProcessingBoneError {
    #[error("Model \"{0}\" In Body Group \"{1}\" Has No File Source")]
    NoModelFileSource(String, String),
    #[error("Animation \"{0}\" Has No File Source")]
    NoAnimationFileSource(String),
    #[error("File Source Not Loaded \"{0}\"")]
    FileSourceNotLoaded(PathBuf),
    #[error("Model Has Too Many Bone")]
    TooManyBones,
}

pub fn process_bones(input: &input::CompilationData, import: &FileManager) -> Result<super::BoneData, ProcessingBoneError> {
    let mut processed_bones: IndexMap<String, super::Bone> = IndexMap::new();

    // TODO: Declare define bones.

    for input_body_part in &input.body_groups {
        for input_model in &input_body_part.models {
            if input_model.blank {
                continue;
            }

            let source_file_path = input_model
                .source_file_path
                .as_ref()
                .ok_or(ProcessingBoneError::NoModelFileSource(input_model.name.clone(), input_body_part.name.clone()))?;

            let imported_file = import
                .get_file_data(source_file_path)
                .ok_or(ProcessingBoneError::FileSourceNotLoaded(source_file_path.clone()))?;

            for (import_bone_index, (import_bone_name, import_bone)) in imported_file.skeleton.iter().enumerate() {
                let mut bone_flags = super::BoneFlags::default();

                for (import_part_index, (_, import_part)) in imported_file.parts.iter().enumerate() {
                    if !input_model.enabled_source_parts[import_part_index] {
                        continue;
                    }

                    for vertex in &import_part.vertices {
                        if vertex.links.contains_key(&import_bone_index) {
                            bone_flags.insert(super::BoneFlags::USED_BY_VERTEX);
                        }
                    }
                }

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
        }
    }

    for input_animation in &input.animations {
        let source_file_path = input_animation
            .source_file_path
            .as_ref()
            .ok_or(ProcessingBoneError::NoAnimationFileSource(input_animation.name.clone()))?;

        let imported_file = import
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
    }

    debug!("Model uses {} source bones.", processed_bones.len());

    // TODO: Enforce define bone's transforms.

    // TODO: Tag bones from input data.

    // Create all world transform matrixes for source bones.
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

    // TODO: Enforce skeleton hierarchy.

    // Collapse unused Bones.
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

    if processed_bones.len() > (i8::MAX as usize) + 1 {
        return Err(ProcessingBoneError::TooManyBones);
    }

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

    let mut sorted_bones_by_name = (0..processed_bones.len() as u8).collect::<Vec<_>>();
    sorted_bones_by_name.sort_by(|from, to| {
        let bone_from = processed_bones.get_index(*from as usize).unwrap().0;
        let bone_to = processed_bones.get_index(*to as usize).unwrap().0;
        bone_from.cmp(bone_to)
    });

    Ok(super::BoneData {
        processed_bones,
        sorted_bones_by_name,
    })
}
