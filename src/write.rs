use half::f16;
use indexmap::IndexMap;
use std::fs::write;
use thiserror::Error as ThisError;

use crate::{
    process::{self, CompiledData, FLOAT_TOLERANCE, MAX_HARDWARE_BONES_PER_STRIP, VERTEX_CACHE_SIZE},
    utilities::mathematics::{EULER_ROTATION, Quaternion, Vector2, Vector3, Vector4},
};

mod mesh;
mod model;
mod vertex;

pub const MAX_LOD_COUNT: usize = 8;

#[derive(Debug, ThisError)]
pub enum FileWriteError {
    #[error("Array Size Larger Than 2,147,483,647")]
    ArraySizeIntegerTooLarge,
    #[error("Array Size Larger Than 32,767")]
    ArraySizeShortTooLarge,
    #[error("Offset Larger Than 2,147,483,647")]
    IntegerOffsetTooLarge,
    #[error("offset Smaller Than -2,147,483,648")]
    IntegerOffsetTooSmall,
    #[error("Offset Larger Than 32,767")]
    ShortOffsetTooLarge,
}

#[derive(Debug, Default)]
pub struct FileWriter {
    buffer: Vec<u8>,
    string_table: IndexMap<String, Vec<(usize, usize)>>,
}

impl FileWriter {
    pub fn write_unsigned_byte(&mut self, value: u8) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_unsigned_byte_array(&mut self, values: &[u8]) {
        for &value in values {
            self.write_unsigned_byte(value);
        }
    }

    pub fn write_short(&mut self, value: i16) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_short_array(&mut self, values: &[i16]) {
        for &value in values {
            self.write_short(value);
        }
    }

    pub fn write_unsigned_short(&mut self, value: u16) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_unsigned_short_array(&mut self, values: &[u16]) {
        for &value in values {
            self.write_unsigned_short(value);
        }
    }

    pub fn write_integer(&mut self, value: i32) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_integer_array(&mut self, values: &[i32]) {
        for &value in values {
            self.write_integer(value);
        }
    }

    pub fn write_float(&mut self, value: f32) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_float_array(&mut self, values: &[f32]) {
        for &value in values {
            self.write_float(value);
        }
    }

    pub fn write_unsigned_long(&mut self, value: u64) {
        self.buffer.extend(value.to_le_bytes());
    }

    pub fn write_char_array(&mut self, value: &str, length: usize) {
        let mut bytes = value.as_bytes().to_vec();
        bytes.resize(length, 0);
        self.buffer.extend(bytes);
    }

    pub fn write_vector2(&mut self, value: Vector2) {
        self.write_float(value.x as f32);
        self.write_float(value.y as f32);
    }

    pub fn write_vector3(&mut self, value: Vector3) {
        self.write_float(value.x as f32);
        self.write_float(value.y as f32);
        self.write_float(value.z as f32);
    }

    pub fn write_vector4(&mut self, value: Vector4) {
        self.write_float(value.x as f32);
        self.write_float(value.y as f32);
        self.write_float(value.z as f32);
        self.write_float(value.w as f32);
    }

    pub fn write_quaternion(&mut self, value: Quaternion) {
        self.write_float(value.x as f32);
        self.write_float(value.y as f32);
        self.write_float(value.z as f32);
        self.write_float(value.w as f32);
    }

    pub fn write_euler(&mut self, value: Quaternion) {
        let (roll, pitch, yaw) = value.to_euler(EULER_ROTATION);
        self.write_float(roll as f32);
        self.write_float(pitch as f32);
        self.write_float(yaw as f32);
    }

    pub fn write_string_to_table(&mut self, base: usize, value: &str) {
        let string_offset = self.write_integer_index();

        match self.string_table.get_mut(value) {
            Some(table) => {
                table.push((base, string_offset));
            }
            None => {
                self.string_table.insert(String::from(value), vec![(base, string_offset)]);
            }
        }
    }

    pub fn write_string_table(&mut self) -> Result<(), FileWriteError> {
        let mut entries = self.string_table.drain(..).collect::<Vec<_>>();

        entries.sort_by(|(to, _), (from, _)| to.cmp(from));

        for (string, string_entries) in entries {
            let string_index = self.buffer.len();
            self.write_null_terminated_string(&string);

            for (base, index) in string_entries {
                self.write_to_integer_offset(index, string_index - base)?;
            }
        }

        Ok(())
    }

    pub fn write_null_terminated_string(&mut self, value: &str) {
        self.buffer.extend_from_slice(value.as_bytes());
        self.buffer.push(0);
    }

    pub fn write_integer_index(&mut self) -> usize {
        let this = self.this();
        self.write_integer(0);
        this
    }

    pub fn write_short_index(&mut self) -> usize {
        let this = self.this();
        self.write_short(0);
        this
    }

    pub fn write_to_integer_offset(&mut self, index: usize, offset: usize) -> Result<(), FileWriteError> {
        if offset > i32::MAX as usize {
            return Err(FileWriteError::IntegerOffsetTooLarge);
        }

        let bytes = (offset as i32).to_le_bytes();

        self.buffer[index..index + bytes.len()].clone_from_slice(&bytes as &[u8]);
        Ok(())
    }

    pub fn write_to_integer(&mut self, index: usize, value: i32) {
        let bytes = value.to_le_bytes();

        self.buffer[index..index + bytes.len()].clone_from_slice(&bytes as &[u8]);
    }

    pub fn write_to_short_offset(&mut self, index: usize, offset: usize) -> Result<(), FileWriteError> {
        if offset > i16::MAX as usize {
            return Err(FileWriteError::ShortOffsetTooLarge);
        }

        let bytes = (offset as i16).to_le_bytes();

        self.buffer[index..index + bytes.len()].clone_from_slice(&bytes as &[u8]);
        Ok(())
    }

    pub fn write_negative_offset(&mut self, offset: usize) -> Result<(), FileWriteError> {
        if offset > i32::MIN.unsigned_abs() as usize {
            return Err(FileWriteError::IntegerOffsetTooSmall);
        }

        self.write_integer(-(offset as i32));
        Ok(())
    }

    pub fn write_quaternion64(&mut self, value: Quaternion) {
        let x = ((value.x * 1048576.0) as i64 + 1048576).clamp(0, 2097151) as u64;
        let y = ((value.y * 1048576.0) as i64 + 1048576).clamp(0, 2097151) as u64;
        let z = ((value.z * 1048576.0) as i64 + 1048576).clamp(0, 2097151) as u64;
        let w = if value.w < 0.0 { 1 } else { 0 };
        self.write_unsigned_long((w << 63) | (z << 42) | (y << 21) | x);
    }

    pub fn write_vector48(&mut self, value: Vector3) {
        self.buffer.extend(f16::from_f64(value.x).to_le_bytes());
        self.buffer.extend(f16::from_f64(value.y).to_le_bytes());
        self.buffer.extend(f16::from_f64(value.z).to_le_bytes());
    }

    pub fn write_array_size_integer<T>(&mut self, array: &[T]) -> Result<(), FileWriteError> {
        let size = array.len();

        if size > i32::MAX as usize {
            return Err(FileWriteError::ArraySizeIntegerTooLarge);
        }

        self.write_integer(size as i32);
        Ok(())
    }

    pub fn write_array_size_short<T>(&mut self, array: &[T]) -> Result<(), FileWriteError> {
        let size = array.len();

        if size > i16::MAX as usize {
            return Err(FileWriteError::ArraySizeShortTooLarge);
        }

        self.write_short(size as i16);
        Ok(())
    }

    pub fn this(&self) -> usize {
        self.buffer.len()
    }

    pub fn align(&mut self, alignment: usize) {
        let remainder = self.buffer.len() % alignment;

        if remainder == 0 {
            return;
        }

        let padding = alignment - remainder;

        self.buffer.resize(self.buffer.len() + padding, 0);
    }

    pub fn checksum(&self) -> i32 {
        // TODO: Implement a better checksum.
        self.buffer.iter().fold(0, |acc, &byte| acc.wrapping_add(byte as i32))
    }
}

pub fn write_files(file_name: String, model_name: String, compiled_data: CompiledData, export_path: String) -> Result<(), FileWriteError> {
    let mut mdl_header = model::Header {
        version: model::HeaderVersions::TwentyThirteen,
        hull: compiled_data.model_data.bounding_box, // TODO: If the model has no mesh use sequence bounding box.
        illumination_position: compiled_data.model_data.bounding_box.center(), // TODO: If input, use the input value.
        flags: model::HeaderFlags::FORCE_OPAQUE | model::HeaderFlags::AUTO_GENERATED_HITBOX,
        surface_property: String::from("default"),
        contents: model::HeaderContents::SOLID,
        second_header: model::SecondHeader {
            name: model_name,
            ..Default::default()
        },
        ..Default::default()
    };

    for (bone_index, (bone_name, processed_bone)) in compiled_data.bone_data.processed_bones.into_iter().enumerate() {
        let bone = model::Bone {
            name: bone_name,
            parent: match processed_bone.parent {
                Some(index) => index as i32,
                None => -1,
            },
            bone_controller: [-1; 6],
            position: processed_bone.location,
            rotation: processed_bone.rotation,
            quaternion: processed_bone.rotation,
            animation_position_scale: compiled_data.animation_data.animation_scales[bone_index].0,
            animation_rotation_scale: compiled_data.animation_data.animation_scales[bone_index].1,
            pose: processed_bone.world_transform.inverse(),
            flags: model::BoneFlags::from_bits_truncate(processed_bone.flags.bits()),
            physics_bone: -1,
            surface_property: String::from("default"),
            contents: model::HeaderContents::SOLID,
            ..Default::default()
        };
        mdl_header.bones.push(bone);
    }

    mdl_header.bone_table_by_name = compiled_data.bone_data.sorted_bones_by_name;

    let mut hitbox_set = model::HitboxSet {
        name: String::from("default"),
        hitboxes: Vec::with_capacity(compiled_data.model_data.hitboxes.len()),
        ..Default::default()
    };

    for (bone, bounding) in compiled_data.model_data.hitboxes {
        hitbox_set.hitboxes.push(model::Hitbox {
            bone: bone as i32,
            bounding,
            ..Default::default()
        });
    }

    mdl_header.hitbox_sets.push(hitbox_set);

    write_animations(compiled_data.animation_data, &mut mdl_header);

    for (processed_sequence_name, processed_sequence) in compiled_data.sequence_data {
        let sequence_description = model::SequenceDescription {
            name: processed_sequence_name,
            activity_weight: -1,
            fade_in_time: 0.2,
            fade_out_time: 0.2,
            blend_size: [processed_sequence.animations.len() as i32, processed_sequence.animations[0].len() as i32],
            parameter_index: [-1; 2],
            animations: processed_sequence.animations.into_iter().flatten().collect(),
            weight_list: vec![1.0; mdl_header.bones.len()],
            ..Default::default()
        };

        mdl_header.sequence_descriptions.push(sequence_description);
    }

    let mut vvd_header = vertex::Header {
        version: 4,
        lod_count: 1,
        ..Default::default()
    };
    let mut vtx_header = mesh::Header {
        version: 7,
        vertex_cache_size: VERTEX_CACHE_SIZE as i32,
        max_bones_per_strip: MAX_HARDWARE_BONES_PER_STRIP as u16,
        max_bones_per_triangle: 9,
        max_bones_per_vertex: 3,
        ..Default::default()
    };

    mdl_header.material_paths.push(String::from(""));

    write_model_groups(compiled_data.model_data.model_groups, &mut mdl_header, &mut vtx_header, &mut vvd_header);

    for processed_material in compiled_data.model_data.materials {
        let material = model::Material {
            name: processed_material,
            ..Default::default()
        };
        mdl_header.materials.push(material);
    }

    mdl_header.material_replacements.push((0..mdl_header.materials.len() as i16).collect());

    let mut mdl_writer = FileWriter::default();
    mdl_header.write_data(&mut mdl_writer)?;
    let mut vvd_writer = FileWriter::default();
    vvd_header.checksum = mdl_header.checksum;
    vvd_header.write_data(&mut vvd_writer)?;
    let mut vtx_writer = FileWriter::default();
    vtx_header.checksum = mdl_header.checksum;
    vtx_header.write_data(&mut vtx_writer)?;

    // // FIXME: This is a temporary solution to write the files.
    let _ = write(format!("{}/{}.{}", export_path, file_name, "mdl"), mdl_writer.buffer);
    let _ = write(format!("{}/{}.{}", export_path, file_name, "vvd"), vvd_writer.buffer);
    let _ = write(format!("{}/{}.{}", export_path, file_name, "dx90.vtx"), vtx_writer.buffer);

    Ok(())
}

fn write_animations(animations: process::AnimationData, header: &mut model::Header) {
    for (processed_animation_name, processed_animation) in animations.processed_animations {
        let mut animation_description = model::AnimationDescription {
            name: processed_animation_name,
            fps: 30.0,
            frame_count: processed_animation.frame_count as i32,
            // TODO: section_frame_count should use the imported frame count.
            section_frame_count: if processed_animation.sections.len() > 1 { 30 } else { 0 },
            sections: Vec::with_capacity(processed_animation.sections.len()),
            ..Default::default()
        };

        for mut section in processed_animation.sections {
            let mut animation_section = model::AnimationSection {
                animation_data: Vec::with_capacity(section.len()),
                ..Default::default()
            };

            section.sort_by(|to, from| to.bone.cmp(&from.bone));

            for animation_bone_data in section {
                let scale = animations.animation_scales[animation_bone_data.bone as usize].1;
                let mut scaled_rotation_axis = [
                    Vec::with_capacity(animation_bone_data.delta_rotation.len()),
                    Vec::with_capacity(animation_bone_data.delta_rotation.len()),
                    Vec::with_capacity(animation_bone_data.delta_rotation.len()),
                ];
                for rotation in &animation_bone_data.delta_rotation {
                    let (roll, pitch, yaw) = rotation.to_euler(EULER_ROTATION);

                    scaled_rotation_axis[0].push(if roll.abs() > FLOAT_TOLERANCE { (roll / scale[0]) as i16 } else { 0 });
                    scaled_rotation_axis[1].push(if pitch.abs() > FLOAT_TOLERANCE { (pitch / scale[1]) as i16 } else { 0 });
                    scaled_rotation_axis[2].push(if yaw.abs() > FLOAT_TOLERANCE { (yaw / scale[2]) as i16 } else { 0 });
                }

                let scale = animations.animation_scales[animation_bone_data.bone as usize].0;
                let mut scaled_position_axis = [
                    Vec::with_capacity(animation_bone_data.delta_position.len()),
                    Vec::with_capacity(animation_bone_data.delta_position.len()),
                    Vec::with_capacity(animation_bone_data.delta_position.len()),
                ];
                for position in &animation_bone_data.delta_position {
                    for axis in 0..3 {
                        scaled_position_axis[axis].push(if position[axis].abs() > FLOAT_TOLERANCE {
                            (position[axis] / scale[axis]) as i16
                        } else {
                            0
                        });
                    }
                }

                fn encode_run_length(values: &[i16]) -> Vec<model::CompressedAnimationEntry> {
                    let mut encoding = Vec::new();

                    let mut current_total = 0;
                    let mut current_valid = Vec::new();

                    for &value in values {
                        // Check if the current header is full.
                        if current_total == u8::MAX {
                            encoding.push(model::CompressedAnimationEntry::Header(model::CompressedAnimationEntryHeader {
                                total: current_total,
                                valid: current_valid.len() as u8,
                            }));
                            encoding.extend(current_valid.into_iter().map(model::CompressedAnimationEntry::Value));
                            current_total = 0;
                            current_valid = Vec::new();
                        }

                        // Check if the current header is empty.
                        if current_valid.is_empty() {
                            current_total += 1;
                            current_valid.push(value);
                            continue;
                        }

                        // Check if the previous value is the same as the current value.
                        if current_valid[current_valid.len() - 1] == value {
                            current_total += 1;
                            continue;
                        }

                        // If the current value is not the same as the previous value and the values length is not equal to the total.
                        if current_valid.len() as u8 != current_total {
                            encoding.push(model::CompressedAnimationEntry::Header(model::CompressedAnimationEntryHeader {
                                total: current_total,
                                valid: current_valid.len() as u8,
                            }));
                            encoding.extend(current_valid.into_iter().map(model::CompressedAnimationEntry::Value));

                            current_total = 1;
                            current_valid = vec![value];
                            continue;
                        }

                        current_total += 1;
                        current_valid.push(value);
                    }

                    encoding.push(model::CompressedAnimationEntry::Header(model::CompressedAnimationEntryHeader {
                        total: current_total,
                        valid: current_valid.len() as u8,
                    }));
                    encoding.extend(current_valid.into_iter().map(model::CompressedAnimationEntry::Value));

                    encoding
                }

                let encoded_rotation_axis = [
                    encode_run_length(&scaled_rotation_axis[0]),
                    encode_run_length(&scaled_rotation_axis[1]),
                    encode_run_length(&scaled_rotation_axis[2]),
                ];
                let encoded_position_axis = [
                    encode_run_length(&scaled_position_axis[0]),
                    encode_run_length(&scaled_position_axis[1]),
                    encode_run_length(&scaled_position_axis[2]),
                ];

                let mut rotation = None;
                let mut position = None;

                if encoded_rotation_axis[0].len() == 2
                    && encoded_rotation_axis[1].len() == 2
                    && encoded_rotation_axis[2].len() == 2
                    && encoded_position_axis[0].len() == 2
                    && encoded_position_axis[1].len() == 2
                    && encoded_position_axis[2].len() == 2
                {
                    match (&encoded_rotation_axis[0][1], &encoded_rotation_axis[1][1], &encoded_rotation_axis[2][1]) {
                        (
                            &model::CompressedAnimationEntry::Value(x),
                            &model::CompressedAnimationEntry::Value(y),
                            &model::CompressedAnimationEntry::Value(z),
                        ) => {
                            if x != 0 || y != 0 || z != 0 {
                                rotation = Some(model::AnimationData::Raw(animation_bone_data.raw_rotation[0]));
                            }
                        }
                        _ => {
                            unreachable!("All the values should be model::FileAnimationEncoding::Value");
                        }
                    }

                    match (&encoded_position_axis[0][1], &encoded_position_axis[1][1], &encoded_position_axis[2][1]) {
                        (
                            &model::CompressedAnimationEntry::Value(x),
                            &model::CompressedAnimationEntry::Value(y),
                            &model::CompressedAnimationEntry::Value(z),
                        ) => {
                            if x != 0 || y != 0 || z != 0 {
                                position = Some(model::AnimationData::Raw(animation_bone_data.raw_position[0]));
                            }
                        }
                        _ => {
                            unreachable!("All the values should be model::FileAnimationEncoding::Value");
                        }
                    }
                } else {
                    let mut animation_axis = model::CompressedAnimation::default();

                    let [x_encoded, y_encoded, z_encoded] = encoded_rotation_axis;

                    if x_encoded.len() > 2 || matches!(&x_encoded[1], &model::CompressedAnimationEntry::Value(x) if x != 0) {
                        animation_axis.values[0] = Some(x_encoded);
                    }

                    if y_encoded.len() > 2 || matches!(&y_encoded[1], &model::CompressedAnimationEntry::Value(y) if y != 0) {
                        animation_axis.values[1] = Some(y_encoded);
                    }

                    if z_encoded.len() > 2 || matches!(&z_encoded[1], &model::CompressedAnimationEntry::Value(z) if z != 0) {
                        animation_axis.values[2] = Some(z_encoded);
                    }

                    if animation_axis.values[0].is_some() || animation_axis.values[1].is_some() || animation_axis.values[2].is_some() {
                        rotation = Some(model::AnimationData::Compressed(animation_axis));
                    }

                    let mut animation_axis = model::CompressedAnimation::default();

                    let [x_encoded, y_encoded, z_encoded] = encoded_position_axis;

                    if x_encoded.len() > 2 || matches!(&x_encoded[1], &model::CompressedAnimationEntry::Value(x) if x != 0) {
                        animation_axis.values[0] = Some(x_encoded);
                    }

                    if y_encoded.len() > 2 || matches!(&y_encoded[1], &model::CompressedAnimationEntry::Value(y) if y != 0) {
                        animation_axis.values[1] = Some(y_encoded);
                    }

                    if z_encoded.len() > 2 || matches!(&z_encoded[1], &model::CompressedAnimationEntry::Value(z) if z != 0) {
                        animation_axis.values[2] = Some(z_encoded);
                    }

                    if animation_axis.values[0].is_some() || animation_axis.values[1].is_some() || animation_axis.values[2].is_some() {
                        position = Some(model::AnimationData::Compressed(animation_axis));
                    }
                }

                if rotation.is_none() && position.is_none() {
                    continue;
                }

                animation_section.animation_data.push(model::Animation {
                    bone: animation_bone_data.bone,
                    position,
                    rotation,
                    ..Default::default()
                });
            }

            if animation_section.animation_data.is_empty() {
                animation_section.animation_data.push(model::Animation {
                    bone: u8::MAX,
                    ..Default::default()
                });
            }

            animation_description.sections.push(animation_section);
        }

        header.animation_descriptions.push(animation_description);
    }
}

fn write_model_groups(
    processed_model_groups: IndexMap<String, process::ModelGroup>,
    header: &mut model::Header,
    mesh_header: &mut mesh::Header,
    vertex_header: &mut vertex::Header,
) {
    let mut mesh_id = 0;
    let mut previous_base = None;
    for (processed_model_group_name, processed_model_group) in processed_model_groups {
        let mut model_body_part = model::BodyPart {
            name: processed_model_group_name,
            models: Vec::with_capacity(processed_model_group.models.len()),
            base: match previous_base {
                Some((previous_base, previous_count)) => previous_base * previous_count as i32,
                None => 1,
            },
            ..Default::default()
        };
        previous_base = Some((model_body_part.base, processed_model_group.models.len()));

        let mut mesh_body_part_header = mesh::BodyPartHeader::default();

        for (processed_model_name, processed_model) in processed_model_group.models {
            let mut model_model = model::Model {
                name: processed_model_name,
                meshes: Vec::with_capacity(processed_model.meshes.len()),
                vertex_count: processed_model.meshes.iter().map(|mesh| mesh.vertex_data.len()).sum::<usize>() as i32,
                vertex_offset: (vertex_header.vertices.len() * 48) as i32, // FIXME: Add a check for this.
                tangent_offset: (vertex_header.tangents.len() * 16) as i32, // FIXME: Add a check for this.
                ..Default::default()
            };

            let mut mesh_model_header = mesh::ModelHeader::default();
            let mut mesh_model_lod_header = mesh::ModelLODHeader::default();

            let mut vertex_count = 0;
            for processed_mesh in processed_model.meshes {
                let model_mesh = model::Mesh {
                    material: processed_mesh.material,
                    vertex_count: processed_mesh.vertex_data.len() as i32,
                    vertex_offset: vertex_count as i32,
                    identifier: mesh_id,
                    vertex_lod_count: [processed_mesh.vertex_data.len() as i32; 8],
                    ..Default::default()
                };

                mesh_id += 1;
                vertex_count += processed_mesh.vertex_data.len();

                for processed_vertex in processed_mesh.vertex_data {
                    vertex_header.vertices.push(vertex::Vertex {
                        weights: processed_vertex.weights,
                        bones: processed_vertex.bones,
                        bone_count: processed_vertex.bone_count,
                        position: processed_vertex.position,
                        normal: processed_vertex.normal,
                        texture_coordinate: processed_vertex.texture_coordinate,
                        ..Default::default()
                    });
                    vertex_header.tangents.push(processed_vertex.tangent);
                }

                let mut mesh_mesh_header = mesh::MeshHeader::default();

                for processed_strip_group in processed_mesh.strip_groups {
                    let mut mesh_strip_group_header = mesh::StripGroupHeader {
                        flags: mesh::StripGroupHeaderFlags::IS_HARDWARE_SKINNED,
                        indices: processed_strip_group.indices,
                        ..Default::default()
                    };

                    for processed_mesh_vertex in processed_strip_group.vertices {
                        mesh_strip_group_header.vertices.push(mesh::Vertex {
                            bone_count: processed_mesh_vertex.bone_count,
                            vertex_id: processed_mesh_vertex.vertex_index,
                            bone_ids: processed_mesh_vertex.bones,
                            ..Default::default()
                        });
                    }

                    for processed_strip in processed_strip_group.strips {
                        let mut mesh_strip_header = mesh::StripHeader {
                            flags: mesh::StripHeaderFlags::IS_TRIANGLE_LIST,
                            indices_count: processed_strip.indices_count,
                            indices_offset: processed_strip.indices_offset,
                            vertices_count: processed_strip.vertex_count,
                            vertices_offset: processed_strip.vertex_offset,
                            bone_count: processed_strip.bone_count,
                            ..Default::default()
                        };

                        for bone_change in processed_strip.hardware_bones {
                            let mesh_bone_state_change = mesh::BoneStateChangeHeader {
                                hardware_id: bone_change.hardware_bone,
                                bone_table_index: bone_change.bone_table_bone,
                                ..Default::default()
                            };

                            mesh_strip_header.bone_state_changes.push(mesh_bone_state_change);
                        }

                        mesh_strip_group_header.strips.push(mesh_strip_header);
                    }

                    mesh_mesh_header.strip_groups.push(mesh_strip_group_header);
                }

                mesh_model_lod_header.meshes.push(mesh_mesh_header);
                model_model.meshes.push(model_mesh);
            }

            model_body_part.models.push(model_model);
            mesh_model_header.model_lods.push(mesh_model_lod_header);
            mesh_body_part_header.models.push(mesh_model_header);
        }

        header.body_parts.push(model_body_part);
        mesh_header.body_parts.push(mesh_body_part_header);
    }

    mesh_header.material_replacement_lists.push(mesh::MaterialReplacementListHeader::default());
    vertex_header.lod_vertex_count = [vertex_header.vertices.len() as i32; MAX_LOD_COUNT];
}
