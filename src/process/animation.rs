use indexmap::IndexMap;
use thiserror::Error as ThisError;

use crate::{
    debug,
    import::FileManager,
    input,
    utilities::mathematics::{EULER_ROTATION, Matrix4, Quaternion, Vector3, create_space_transform},
    warn,
};

#[derive(Debug, ThisError)]
pub enum ProcessingAnimationError {
    #[error("No Animation File Selected")]
    NoFileSource,
    #[error("Animation File Source Not Loaded")]
    FileSourceNotLoaded,
    #[error("Model Has Too Many Animations")]
    TooManyAnimations,
}

pub fn process_animations(
    input_data: &input::SourceInput,
    source_files: &FileManager,
    processed_bone_data: &super::BoneData,
) -> Result<super::AnimationData, ProcessingAnimationError> {
    let mut remapped_animations = IndexMap::new();
    let mut processed_animations = IndexMap::new();
    let mut model_frame_count = 0;
    for imputed_animation in &input_data.animations {
        // Check if the animation is used in any sequence.
        if !input_data
            .sequences
            .iter()
            .any(|sequence| sequence.animations.iter().any(|row| row.contains(&imputed_animation.animation_identifier)))
        {
            warn!("Animation \"{}\" Not Used!", imputed_animation.name);
            continue;
        }
        remapped_animations.insert(imputed_animation.animation_identifier, processed_animations.len());

        let processed_animation_name = imputed_animation.name.clone();
        debug_assert!(!processed_animations.contains_key(&processed_animation_name));

        // Gather imported animation data.
        let imported_file = source_files
            .get_file_data(imputed_animation.source_file_path.as_ref().ok_or(ProcessingAnimationError::NoFileSource)?)
            .ok_or(ProcessingAnimationError::FileSourceNotLoaded)?;
        let imported_animation = &imported_file.animations[imputed_animation.source_animation];

        let frame_count = imported_animation.frame_count.get();
        model_frame_count += frame_count;

        // All the import bones with all frames of animation global transforms.
        let mut imported_bone_animation_transforms: Vec<Vec<Matrix4>> = Vec::with_capacity(imported_file.skeleton.len());
        for (import_bone_index, import_bone) in imported_file.skeleton.values().enumerate() {
            /// Convert channel keyframes to a continuous set of values.
            fn bake_channel_keyframes<T: Copy>(channel: &IndexMap<usize, T>, frame_count: usize, default: T) -> Vec<T> {
                let mut baked_channel = Vec::with_capacity(frame_count);

                for frame in 0..frame_count {
                    if let Some(keyframe) = channel.get(&frame) {
                        baked_channel.push(*keyframe);
                        continue;
                    }

                    if let Some(last_value) = baked_channel.last() {
                        baked_channel.push(*last_value);
                        continue;
                    }

                    baked_channel.push(default);
                }

                baked_channel
            }

            let location_channel = match imported_animation.channels.get(&import_bone_index) {
                Some(import_channel) => bake_channel_keyframes(&import_channel.location, frame_count, import_bone.location),
                None => vec![import_bone.location; frame_count],
            };
            let rotation_channel = match imported_animation.channels.get(&import_bone_index) {
                Some(import_channel) => bake_channel_keyframes(&import_channel.rotation, frame_count, import_bone.rotation),
                None => vec![import_bone.rotation; frame_count],
            };

            let mut imported_animation_transform = Vec::with_capacity(frame_count);
            for frame in 0..frame_count {
                let location = location_channel[frame];
                let rotation = rotation_channel[frame];
                let transform = Matrix4::from_rotation_translation(rotation, location);

                if let Some(parent_transform) = import_bone.parent.map(|parent_index| imported_bone_animation_transforms[parent_index][frame]) {
                    imported_animation_transform.push(parent_transform * transform);
                    continue;
                }

                let space_transform = create_space_transform(imported_file.up, imported_file.forward);
                imported_animation_transform.push(space_transform.inverse() * transform);
            }
            imported_bone_animation_transforms.push(imported_animation_transform);
        }

        // All the proceed bones with all frames of animation global transforms using import animation.
        let mut processed_bone_animation_transforms: Vec<Vec<Matrix4>> = Vec::with_capacity(processed_bone_data.processed_bones.len());
        for (processed_bone_name, processed_bone) in &processed_bone_data.processed_bones {
            if let Some(imported_animation_transform) = imported_file
                .skeleton
                .get_index_of(processed_bone_name)
                .map(|import_bone_index| &imported_bone_animation_transforms[import_bone_index])
            {
                processed_bone_animation_transforms.push(imported_animation_transform.clone());
                continue;
            }

            if let Some(processed_parent_bone_animation_transform) = processed_bone
                .parent
                .map(|processed_bone_parent_index| &processed_bone_animation_transforms[processed_bone_parent_index])
            {
                processed_bone_animation_transforms.push(
                    processed_parent_bone_animation_transform
                        .iter()
                        .map(|parent_transform| parent_transform * Matrix4::from_rotation_translation(processed_bone.rotation, processed_bone.location))
                        .collect(),
                );
                continue;
            }

            processed_bone_animation_transforms.push(vec![processed_bone.world_transform; frame_count]);
        }

        // TODO: Implement animation processing.
        // TODO: Add a check if the position data is going to be out of bounds.

        // All the proceed bones with all frames of animation local transforms.
        let mut processed_bone_animation_local_transforms: Vec<Vec<Matrix4>> = Vec::with_capacity(processed_bone_data.processed_bones.len());
        for (processed_bone_index, processed_bone) in processed_bone_data.processed_bones.values().enumerate() {
            let mut processed_bone_animation_local_transform = Vec::with_capacity(frame_count);
            let processed_bone_animation_transform = &processed_bone_animation_transforms[processed_bone_index];
            if let Some(processed_parent_bone_animation_transform) = processed_bone
                .parent
                .map(|processed_bone_parent_index| &processed_bone_animation_transforms[processed_bone_parent_index])
            {
                for frame in 0..frame_count {
                    let parent_transform = processed_parent_bone_animation_transform[frame];
                    let transform = processed_bone_animation_transform[frame];
                    processed_bone_animation_local_transform.push(parent_transform.inverse() * transform);
                }
                processed_bone_animation_local_transforms.push(processed_bone_animation_local_transform);
                continue;
            }
            processed_bone_animation_local_transforms.push(processed_bone_animation_transform.clone());
        }

        // Split animation into sections
        let frames_per_sections = 30; // TODO: Make this configurable.
        let animation_section_split_threshold = 120; // TODO: Make this configurable.

        let section_count = if frame_count >= animation_section_split_threshold {
            (frame_count / frames_per_sections) + 2
        } else {
            1
        };
        let section_frame_count = if frame_count >= animation_section_split_threshold {
            frames_per_sections
        } else {
            frame_count
        };

        let mut processed_animation = super::Animation {
            frame_count,
            sections: Vec::with_capacity(section_count),
        };

        for section in 0..section_count {
            let section_frame_start = (section * section_frame_count).min(frame_count - 1);
            let section_frame_end = ((section + 1) * section_frame_count).min(frame_count - 1);

            let mut section_data = Vec::with_capacity(processed_bone_data.processed_bones.len());
            for (index_bone, channel_data) in processed_bone_animation_local_transforms.iter().enumerate() {
                let bone = &processed_bone_data.processed_bones[index_bone];
                let mut raw_position = Vec::with_capacity(section_frame_count);
                let mut raw_rotation = Vec::with_capacity(section_frame_count);
                let mut delta_position = Vec::with_capacity(section_frame_count);
                let mut delta_rotation = Vec::with_capacity(section_frame_count);

                // TODO: If animation is delta then skip subtracting from bone
                for frame in channel_data.iter().take(section_frame_end + 1).skip(section_frame_start) {
                    let (_, rotation, location) = frame.to_scale_rotation_translation();
                    raw_position.push(location);
                    raw_rotation.push(rotation);
                    delta_position.push(location - bone.location);
                    let rotation_euler = Vector3::from(rotation.to_euler(EULER_ROTATION));
                    let delta_euler = rotation_euler - Vector3::from(bone.rotation.to_euler(EULER_ROTATION));
                    delta_rotation.push(Quaternion::from_euler(EULER_ROTATION, delta_euler.x, delta_euler.y, delta_euler.z));
                }

                section_data.push(super::AnimatedBoneData {
                    bone: index_bone as u8,
                    raw_position,
                    raw_rotation,
                    delta_position,
                    delta_rotation,
                });
            }

            processed_animation.sections.push(section_data);
        }

        processed_animations.insert(processed_animation_name, processed_animation);
    }

    debug!("Model uses {model_frame_count} frames.");

    if processed_animations.len() > (i16::MAX as usize + 1) {
        return Err(ProcessingAnimationError::TooManyAnimations);
    }

    let mut animation_scales = vec![(Vector3::default(), Vector3::default()); processed_bone_data.processed_bones.len()];
    for processed_animation in processed_animations.values() {
        for sections in &processed_animation.sections {
            for section in sections {
                for position in &section.delta_position {
                    for axis in 0..3 {
                        let value = position[axis].abs();
                        if value > animation_scales[section.bone as usize].0[axis] {
                            animation_scales[section.bone as usize].0[axis] = value;
                        }
                    }
                }

                for rotation in &section.delta_rotation {
                    let (roll, pitch, yaw) = rotation.to_euler(EULER_ROTATION);

                    if roll.abs() > animation_scales[section.bone as usize].1[0] {
                        animation_scales[section.bone as usize].1[0] = roll.abs();
                    }

                    if pitch.abs() > animation_scales[section.bone as usize].1[1] {
                        animation_scales[section.bone as usize].1[1] = pitch.abs();
                    }

                    if yaw.abs() > animation_scales[section.bone as usize].1[2] {
                        animation_scales[section.bone as usize].1[2] = yaw.abs();
                    }
                }
            }
        }
    }

    for (position, rotation) in &mut animation_scales {
        for axis in 0..3 {
            position[axis] /= i16::MAX as f64;
            rotation[axis] /= i16::MAX as f64;
        }
    }

    Ok(super::AnimationData {
        processed_animations,
        animation_scales,
        remapped_animations,
    })
}
