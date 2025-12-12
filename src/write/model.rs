use bitflags::bitflags;

use crate::{
    utilities::mathematics::{BoundingBox, Matrix4, Quaternion, Vector3},
    write::MAX_LOD_COUNT,
};

use super::{FileWriteError, FileWriter};

#[derive(Debug, Default)]
pub struct Header {
    pub this: usize,
    pub identifier: HeaderIdentifier,
    pub version: HeaderVersions,
    pub checksum: i32,
    pub checksum_index: usize,
    pub length_index: usize,
    pub eye_position: Vector3,
    pub illumination_position: Vector3,
    pub hull: BoundingBox,
    pub view: BoundingBox,
    pub flags: HeaderFlags,
    pub bones: Vec<Bone>,
    pub bone_index: usize,
    pub bone_controllers: Vec<()>,
    pub bone_controller_index: usize,
    pub hitbox_sets: Vec<HitboxSet>,
    pub hitbox_set_index: usize,
    pub animation_descriptions: Vec<AnimationDescription>,
    pub animation_description_index: usize,
    pub sequence_descriptions: Vec<SequenceDescription>,
    pub sequence_description_index: usize,
    pub materials: Vec<Material>,
    pub material_index: usize,
    pub material_paths: Vec<String>,
    pub material_path_index: usize,
    pub material_replacements: Vec<Vec<i16>>,
    pub material_replacement_index: usize,
    pub body_parts: Vec<BodyPart>,
    pub body_part_index: usize,
    pub attachments: Vec<()>,
    pub attachment_index: usize,
    pub nodes: Vec<()>,
    pub node_index: usize,
    pub node_name_index: usize,
    pub flex_descriptions: Vec<()>,
    pub flex_description_index: usize,
    pub flex_controllers: Vec<()>,
    pub flex_controller_index: usize,
    pub flex_rules: Vec<()>,
    pub flex_rule_index: usize,
    pub ik_chains: Vec<()>,
    pub ik_chain_index: usize,
    pub mouths: Vec<()>,
    pub mouth_index: usize,
    pub pose_parameters: Vec<()>,
    pub pose_parameter_index: usize,
    pub surface_property: String,
    pub keyvalues: String,
    pub ik_auto_play_locks: Vec<()>,
    pub ik_auto_play_lock_index: usize,
    pub mass: f32,
    pub contents: HeaderContents,
    pub include_models: Vec<()>,
    pub include_model_index: usize,
    pub animation_block_name: String,
    pub animation_blocks: Vec<()>,
    pub animation_block_index: usize,
    pub bone_table_by_name: Vec<u8>,
    pub bone_table_by_name_index: usize,
    pub constant_directional_light_dot: u8,
    pub allowed_root_lod: u8,
    pub flex_controller_uis: Vec<()>,
    pub flex_controller_ui_index: usize,
    pub flex_scale: f32,
    pub second_header: SecondHeader,
    pub second_header_index: usize,
}

impl Header {
    pub fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_integer(self.identifier.to_integer());
        writer.write_integer(self.version.to_integer());
        self.checksum_index = writer.write_integer_index();
        // TODO: When compiling to l4d2 does not see that header 2 exists and will use this to load the vtx and vvd file.
        writer.write_char_array("Model Compiled With Source Wrench!", 64);
        self.length_index = writer.write_integer_index();
        debug_assert!(self.eye_position.is_finite());
        writer.write_vector3(self.eye_position);
        debug_assert!(self.illumination_position.is_finite());
        writer.write_vector3(self.illumination_position);
        debug_assert!(self.hull.is_valid());
        debug_assert!(self.hull.minimum.is_finite());
        writer.write_vector3(self.hull.minimum);
        debug_assert!(self.hull.maximum.is_finite());
        writer.write_vector3(self.hull.maximum);
        debug_assert!(self.view.is_valid());
        debug_assert!(self.view.minimum.is_finite());
        writer.write_vector3(self.view.minimum);
        debug_assert!(self.view.maximum.is_finite());
        writer.write_vector3(self.view.maximum);
        debug_assert!(!self.flags.contains(HeaderFlags::FORCE_OPAQUE | HeaderFlags::TRANSLUCENT_TWO_PASS));
        writer.write_integer(self.flags.bits());
        writer.write_array_size_integer(&self.bones)?;
        self.bone_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.bone_controllers)?;
        self.bone_controller_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.hitbox_sets)?;
        self.hitbox_set_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.animation_descriptions)?;
        self.animation_description_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.sequence_descriptions)?;
        self.sequence_description_index = writer.write_integer_index();
        writer.write_integer(0); // Activity List Version
        writer.write_integer(0); // Events Indexed
        writer.write_array_size_integer(&self.materials)?;
        self.material_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.material_paths)?;
        self.material_path_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.materials)?;
        writer.write_array_size_integer(&self.material_replacements)?;
        self.material_replacement_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.body_parts)?;
        self.body_part_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.attachments)?;
        self.attachment_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.nodes)?;
        self.node_index = writer.write_integer_index();
        self.node_name_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.flex_descriptions)?;
        self.flex_description_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.flex_controllers)?;
        self.flex_controller_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.flex_rules)?;
        self.flex_rule_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.ik_chains)?;
        self.ik_chain_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.mouths)?;
        self.mouth_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.pose_parameters)?;
        self.pose_parameter_index = writer.write_integer_index();
        writer.write_string_to_table(self.this, &self.surface_property);
        writer.write_string_to_table(self.this, &self.keyvalues);
        writer.write_array_size_integer(self.keyvalues.as_bytes())?;
        writer.write_array_size_integer(&self.ik_auto_play_locks)?;
        self.ik_auto_play_lock_index = writer.write_integer_index();
        debug_assert!(self.mass.is_finite());
        writer.write_float(self.mass);
        debug_assert!(!self.contents.contains(HeaderContents::GRATE | HeaderContents::SOLID));
        writer.write_integer(self.contents.bits());
        writer.write_array_size_integer(&self.include_models)?;
        self.include_model_index = writer.write_integer_index();
        writer.write_integer(0); // Unused Virtual Model
        writer.write_string_to_table(self.this, &self.animation_block_name);
        writer.write_array_size_integer(&self.animation_blocks)?;
        self.animation_block_index = writer.write_integer_index();
        writer.write_integer(0); // Unused Animation Block Model
        self.bone_table_by_name_index = writer.write_integer_index();
        writer.write_integer(0); // Unused Vertex Base
        writer.write_integer(0); // Unused Index Base
        writer.write_unsigned_byte(self.constant_directional_light_dot);
        writer.write_unsigned_byte(0); // Root LOD
        debug_assert!(self.allowed_root_lod as usize <= MAX_LOD_COUNT);
        writer.write_unsigned_byte(self.allowed_root_lod);
        writer.write_unsigned_byte_array(&[0; 1]); // Unused
        writer.write_integer(0); // Unused
        writer.write_array_size_integer(&self.flex_controller_uis)?;
        self.flex_controller_ui_index = writer.write_integer_index();
        debug_assert!(self.flex_scale.is_finite());
        writer.write_float(self.flex_scale);
        writer.write_integer_array(&[0; 1]); // Unused
        self.second_header_index = writer.write_integer_index();
        writer.write_integer_array(&[0; 1]); // Unused

        writer.write_to_integer_offset(self.second_header_index, writer.this() - self.this)?;
        self.second_header.write_data(writer)?;

        self.write_bones(writer)?;

        self.write_hitbox_sets(writer)?;

        self.write_bone_table_by_name(writer)?;

        self.write_animations(writer)?;

        self.write_sequences(writer)?;

        self.write_body_parts(writer)?;

        self.write_materials(writer)?;

        self.write_material_paths(writer)?;

        self.write_material_replacements(writer)?;

        self.second_header.write_liner_bones(writer, &self.bones)?;

        writer.write_string_table()?;

        self.checksum = writer.checksum();
        writer.write_to_integer(self.checksum_index, self.checksum);
        writer.write_to_integer(self.length_index, writer.this() as i32); // FIXME: Check this value!

        Ok(())
    }

    fn write_bones(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.bone_index, writer.this() - self.this)?;

        for bone in &mut self.bones {
            bone.write_data(writer);
        }
        writer.align(4);

        Ok(())
    }

    fn write_hitbox_sets(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.hitbox_set_index, writer.this() - self.this)?;

        for hitbox_set in &mut self.hitbox_sets {
            hitbox_set.write_data(writer)?;
        }
        writer.align(4);

        for hitbox_set in &mut self.hitbox_sets {
            hitbox_set.write_hitboxes(writer)?;
        }
        writer.align(4);

        Ok(())
    }

    fn write_bone_table_by_name(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.bone_table_by_name_index, writer.this() - self.this)?;

        debug_assert!(self.bone_table_by_name.len() == self.bones.len());
        writer.write_unsigned_byte_array(&self.bone_table_by_name);
        writer.align(4);

        Ok(())
    }

    fn write_animations(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.animation_description_index, writer.this() - self.this)?;

        for animation_description in &mut self.animation_descriptions {
            animation_description.write_data(writer)?;
        }
        writer.align(4);

        for animation_description in &mut self.animation_descriptions {
            animation_description.write_sections(writer)?;
            writer.align(16);
            animation_description.write_animation_data(writer)?;
            writer.align(4);
        }

        Ok(())
    }

    fn write_sequences(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.sequence_description_index, writer.this() - self.this)?;

        for sequence_description in &mut self.sequence_descriptions {
            sequence_description.write_data(writer)?;
        }

        for sequence_description in &mut self.sequence_descriptions {
            sequence_description.write_animations(writer)?;
            writer.align(4);
            sequence_description.write_bone_weights(writer)?;
            writer.align(4);
        }

        Ok(())
    }

    fn write_body_parts(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.body_part_index, writer.this() - self.this)?;

        for body_part in &mut self.body_parts {
            body_part.write_data(writer)?;
        }

        for body_part in &mut self.body_parts {
            body_part.write_models(writer)?;
        }
        writer.align(4);

        for body_part in &mut self.body_parts {
            body_part.write_model_mesh_data(writer)?;
        }
        writer.align(4);

        Ok(())
    }

    fn write_materials(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.material_index, writer.this() - self.this)?;

        for material in &mut self.materials {
            material.write_data(writer);
        }
        writer.align(4);

        Ok(())
    }

    fn write_material_paths(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.material_path_index, writer.this() - self.this)?;

        for material_path in &self.material_paths {
            writer.write_string_to_table(self.this, material_path);
        }
        writer.align(4);

        Ok(())
    }

    fn write_material_replacements(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.material_replacement_index, writer.this() - self.this)?;

        for material_replacement in &self.material_replacements {
            writer.write_short_array(material_replacement);
        }
        writer.align(4);

        Ok(())
    }
}

#[derive(Debug, Default)]
pub enum HeaderIdentifier {
    #[default]
    Model,
}

impl HeaderIdentifier {
    fn to_integer(&self) -> i32 {
        match self {
            Self::Model => (84 << 24) + (83 << 16) + (68 << 8) + 73,
        }
    }
}

#[derive(Debug, Default)]
pub enum HeaderVersions {
    #[default]
    TwentyThirteen,
}

impl HeaderVersions {
    fn to_integer(&self) -> i32 {
        match self {
            Self::TwentyThirteen => 48,
        }
    }
}

bitflags! {
    #[derive(Debug, Default)]
    pub struct HeaderFlags: i32 {
        const AUTO_GENERATED_HITBOX          = 0x00000001;
        const FORCE_OPAQUE                   = 0x00000004;
        const TRANSLUCENT_TWO_PASS           = 0x00000008;
        const STATIC_PROP                    = 0x00000010;
        const HAS_SHADOW_LOD                 = 0x00000040;
        const USE_SHADOW_LOD_MATERIALS       = 0x00000100;
        const OBSOLETE                       = 0x00000200;
        const NO_FORCED_FADE                 = 0x00000800;
        const FORCE_PHONEME_CROSS_FADE       = 0x00001000;
        const CONSTANT_DIRECTIONAL_LIGHT_DOT = 0x00002000;
        const AMBIENT_BOOST                  = 0x00010000;
        const DO_NOT_CAST_SHADOWS            = 0x00020000;
        const CAST_TEXTURE_SHADOWS           = 0x00040000;
        const VERT_ANIM_FIXED_POINT_SCALE    = 0x00200000;
    }

    #[derive(Debug, Default)]
    pub struct HeaderContents: i32 {
        const SOLID   = 0x00000001;
        const GRATE   = 0x00000008;
        const MONSTER = 0x02000000;
        const LADDER  = 0x20000000;
    }
}

#[derive(Debug, Default)]
pub struct SecondHeader {
    pub this: usize,
    pub source_bone_transforms: Vec<()>,
    pub source_bone_transform_index: usize,
    pub illumination_position_attachment_index: i32,
    pub max_eye_deflection: f32,
    pub linear_bone_index: usize,
    pub name: String,
    pub bone_flex_drivers: Vec<()>,
    pub bone_flex_driver_index: usize,
}

impl SecondHeader {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_array_size_integer(&self.source_bone_transforms)?;
        self.source_bone_transform_index = writer.write_integer_index();
        debug_assert!(self.illumination_position_attachment_index >= 0);
        writer.write_integer(self.illumination_position_attachment_index);
        debug_assert!(self.max_eye_deflection.is_finite());
        writer.write_float(self.max_eye_deflection);
        self.linear_bone_index = writer.write_integer_index();
        writer.write_string_to_table(self.this, &self.name);
        writer.write_array_size_integer(&self.bone_flex_drivers)?;
        self.bone_flex_driver_index = writer.write_integer_index();
        writer.write_unsigned_long(0); // virtual Model
        writer.write_unsigned_long(0); // Animation Block Model
        writer.write_unsigned_long(0); // Vertex Base
        writer.write_unsigned_long(0); // Index Base
        writer.write_integer_array(&[0; 48]); // Reserved

        Ok(())
    }

    fn write_liner_bones(&mut self, writer: &mut FileWriter, bones: &[Bone]) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.linear_bone_index, writer.this() - self.this)?;
        let this = writer.this();

        writer.write_array_size_integer(bones)?;
        let flags_index = writer.write_integer_index();
        let parent_index = writer.write_integer_index();
        let position_index = writer.write_integer_index();
        let quaternion_index = writer.write_integer_index();
        let rotation_index = writer.write_integer_index();
        let pose_index = writer.write_integer_index();
        let animation_position_scale_index = writer.write_integer_index();
        let animation_rotation_scale_index = writer.write_integer_index();
        let alignment_index = writer.write_integer_index();
        writer.write_integer_array(&[0; 6]); // Unused

        writer.write_to_integer_offset(flags_index, writer.this() - this)?;
        for bone in bones {
            writer.write_integer(bone.flags.bits());
        }

        writer.write_to_integer_offset(parent_index, writer.this() - this)?;
        for bone in bones {
            writer.write_integer(bone.parent);
        }

        writer.write_to_integer_offset(position_index, writer.this() - this)?;
        for bone in bones {
            writer.write_vector3(bone.position);
        }

        writer.write_to_integer_offset(quaternion_index, writer.this() - this)?;
        for bone in bones {
            writer.write_quaternion(bone.quaternion);
        }

        writer.write_to_integer_offset(rotation_index, writer.this() - this)?;
        for bone in bones {
            writer.write_euler(bone.rotation);
        }

        writer.write_to_integer_offset(pose_index, writer.this() - this)?;
        for bone in bones {
            let entries = bone.pose.to_cols_array_2d();
            writer.write_float_array(&[
                entries[0][0] as f32,
                entries[1][0] as f32,
                entries[2][0] as f32,
                entries[3][0] as f32,
                entries[0][1] as f32,
                entries[1][1] as f32,
                entries[2][1] as f32,
                entries[3][1] as f32,
                entries[0][2] as f32,
                entries[1][2] as f32,
                entries[2][2] as f32,
                entries[3][2] as f32,
            ]);
        }

        writer.write_to_integer_offset(animation_position_scale_index, writer.this() - this)?;
        for bone in bones {
            writer.write_vector3(bone.animation_position_scale);
        }

        writer.write_to_integer_offset(animation_rotation_scale_index, writer.this() - this)?;
        for bone in bones {
            writer.write_vector3(bone.animation_rotation_scale);
        }

        writer.write_to_integer_offset(alignment_index, writer.this() - this)?;
        for bone in bones {
            writer.write_quaternion(bone.alignment);
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Bone {
    pub this: usize,
    pub name: String,
    pub parent: i32,
    pub bone_controller: [i32; 6],
    pub position: Vector3,
    pub quaternion: Quaternion,
    pub rotation: Quaternion,
    pub animation_position_scale: Vector3,
    pub animation_rotation_scale: Vector3,
    pub pose: Matrix4,
    pub alignment: Quaternion,
    pub flags: BoneFlags,
    #[allow(dead_code)]
    pub procedural: Option<()>,
    pub procedural_index: usize,
    pub physics_bone: i32,
    pub surface_property: String,
    pub contents: HeaderContents,
}

impl Bone {
    fn write_data(&mut self, writer: &mut FileWriter) {
        self.this = writer.this();

        writer.write_string_to_table(self.this, &self.name);
        writer.write_integer(self.parent);
        writer.write_integer_array(&self.bone_controller);
        debug_assert!(self.position.is_finite());
        writer.write_vector3(self.position);
        debug_assert!(self.quaternion.is_finite());
        writer.write_quaternion(self.quaternion);
        debug_assert!(self.rotation.is_finite());
        writer.write_euler(self.rotation);
        debug_assert!(self.animation_position_scale.is_finite());
        writer.write_vector3(self.animation_position_scale);
        debug_assert!(self.animation_rotation_scale.is_finite());
        writer.write_vector3(self.animation_rotation_scale);
        debug_assert!(self.pose.is_finite());
        let entries = self.pose.to_cols_array_2d();
        writer.write_float_array(&[
            entries[0][0] as f32,
            entries[1][0] as f32,
            entries[2][0] as f32,
            entries[3][0] as f32,
            entries[0][1] as f32,
            entries[1][1] as f32,
            entries[2][1] as f32,
            entries[3][1] as f32,
            entries[0][2] as f32,
            entries[1][2] as f32,
            entries[2][2] as f32,
            entries[3][2] as f32,
        ]);
        debug_assert!(self.alignment.is_finite());
        writer.write_quaternion(self.alignment);
        writer.write_integer(self.flags.bits());
        writer.write_integer(0); // Procural type
        self.procedural_index = writer.write_integer_index();
        writer.write_integer(self.physics_bone);
        writer.write_string_to_table(self.this, &self.surface_property);
        debug_assert!(!self.contents.contains(HeaderContents::GRATE | HeaderContents::SOLID));
        writer.write_integer(self.contents.bits());
        writer.write_integer_array(&[0; 8]); // Unused
    }
}

bitflags! {
    #[derive(Debug, Default)]
    pub struct BoneFlags: i32 {
        const ALWAYS_PROCEDURAL       = 0x00000004;
        const SCREEN_ALIGN_SPHERE     = 0x00000008;
        const SCREEN_ALIGN_CYLINDER   = 0x00000010;
        const USED_BY_HITBOX          = 0x00000100;
        const USED_BY_ATTACHMENT      = 0x00000200;
        const USED_BY_VERTEX_AT_LOD0  = 0x00000400;
        const USED_BY_VERTEX_AT_LOD1  = 0x00000800;
        const USED_BY_VERTEX_AT_LOD2  = 0x00001000;
        const USED_BY_VERTEX_AT_LOD3  = 0x00002000;
        const USED_BY_VERTEX_AT_LOD4  = 0x00004000;
        const USED_BY_VERTEX_AT_LOD5  = 0x00008000;
        const USED_BY_VERTEX_AT_LOD6  = 0x00010000;
        const USED_BY_VERTEX_AT_LOD7  = 0x00020000;
        const USED_BY_BONE_MERGE      = 0x00040000;
        const FIXED_ALIGNMENT         = 0x00100000;
        const HAS_SAVE_FRAME_POSITION = 0x00200000;
        const HAS_SAVE_FRAME_ROTATION = 0x00400000;
    }
}

#[derive(Debug, Default)]
pub struct HitboxSet {
    pub this: usize,
    pub name: String,
    pub hitboxes: Vec<Hitbox>,
    pub hitbox_index: usize,
}

impl HitboxSet {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_string_to_table(self.this, &self.name);
        writer.write_array_size_integer(&self.hitboxes)?;
        self.hitbox_index = writer.write_integer_index();

        Ok(())
    }

    fn write_hitboxes(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.hitbox_index, writer.this() - self.this)?;

        for hitbox in &mut self.hitboxes {
            hitbox.write_data(writer);
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Hitbox {
    pub this: usize,
    pub bone: i32,
    pub group: HitboxGroup,
    pub bounding: BoundingBox,
    pub name: Option<String>,
}

impl Hitbox {
    fn write_data(&mut self, writer: &mut FileWriter) {
        self.this = writer.this();

        debug_assert!(self.bone >= 0);
        writer.write_integer(self.bone);
        writer.write_integer(self.group.to_integer());
        debug_assert!(self.bounding.is_valid());
        debug_assert!(self.bounding.minimum.is_finite());
        writer.write_vector3(self.bounding.minimum);
        debug_assert!(self.bounding.maximum.is_finite());
        writer.write_vector3(self.bounding.maximum);
        if let Some(hitbox_name) = &self.name {
            writer.write_string_to_table(self.this, hitbox_name);
        } else {
            writer.write_integer(0);
        }
        writer.write_integer_array(&[0; 8]);
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub enum HitboxGroup {
    #[default]
    Generic,
    Head,
    Chest,
    Stomach,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl HitboxGroup {
    fn to_integer(&self) -> i32 {
        match self {
            Self::Generic => 0,
            Self::Head => 1,
            Self::Chest => 2,
            Self::Stomach => 3,
            Self::LeftArm => 4,
            Self::RightArm => 5,
            Self::LeftLeg => 6,
            Self::RightLeg => 7,
        }
    }
}

#[derive(Debug, Default)]
pub struct AnimationDescription {
    pub this: usize,
    pub name: String,
    pub fps: f32,
    pub flags: AnimationDescriptionFlags,
    pub frame_count: i32,
    pub movements: Vec<()>,
    pub movement_index: usize,
    pub animation_block: i32,
    pub animation_index: usize,
    pub ik_rules: Vec<()>,
    pub ik_rule_index: usize,
    pub ik_rule_block_index: usize,
    pub local_hierarchies: Vec<()>,
    pub local_hierarchy_index: usize,
    pub sections: Vec<AnimationSection>,
    pub section_index: usize,
    pub section_frame_count: i32,
    pub zero_frame_span: i16,
    pub zero_frames: Vec<()>,
    pub zero_frame_index: usize,
}

impl AnimationDescription {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_negative_offset(writer.this())?;
        writer.write_string_to_table(self.this, &self.name);
        debug_assert!(self.fps.is_finite());
        writer.write_float(self.fps);
        writer.write_integer(self.flags.bits());
        debug_assert!(self.frame_count >= 1);
        writer.write_integer(self.frame_count);
        writer.write_array_size_integer(&self.movements)?;
        self.movement_index = writer.write_integer_index();
        writer.write_integer_array(&[0; 6]); // Unused
        debug_assert!(self.animation_block >= 0);
        writer.write_integer(self.animation_block);
        self.animation_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.ik_rules)?;
        self.ik_rule_index = writer.write_integer_index();
        self.ik_rule_block_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.local_hierarchies)?;
        self.local_hierarchy_index = writer.write_integer_index();
        self.section_index = writer.write_integer_index();
        debug_assert!(self.section_frame_count >= 0);
        writer.write_integer(self.section_frame_count);
        debug_assert!(self.zero_frame_span >= 0);
        writer.write_short(self.zero_frame_span);
        writer.write_array_size_short(&self.zero_frames)?;
        self.zero_frame_index = writer.write_integer_index();
        writer.write_float(0.0); // Zero Frame Stall Time

        Ok(())
    }

    fn write_sections(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        debug_assert!(!self.sections.is_empty());

        if self.sections.len() == 1 {
            return Ok(());
        }

        writer.write_to_integer_offset(self.section_index, writer.this() - self.this)?;
        for section in &mut self.sections {
            section.write_data(writer);
        }

        Ok(())
    }

    fn write_animation_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.animation_index, writer.this() - self.this)?; // This is for crowbar as studioMDL does write to this value but is ignored if sections exist.

        if self.sections.len() == 1 {
            let section = &mut self.sections[0];
            section.write_animation(writer)?;
            return Ok(());
        }

        for section in &mut self.sections {
            writer.write_to_integer_offset(section.animation_index, writer.this() - self.this)?;
            section.write_animation(writer)?;
        }

        Ok(())
    }
}

bitflags! {
    #[derive(Debug, Default)]
    pub struct AnimationDescriptionFlags: i32 {
        const LOOPING   = 0x0001;
        const DELTA     = 0x0004;
        const ALL_ZEROS = 0x0020;
    }
}

#[derive(Debug, Default)]
pub struct AnimationSection {
    pub this: usize,
    pub animation_block: i32,
    pub animation_index: usize,
    pub animation_data: Vec<Animation>,
}

impl AnimationSection {
    fn write_data(&mut self, writer: &mut FileWriter) {
        self.this = writer.this();

        debug_assert!(self.animation_block >= 0);
        writer.write_integer(self.animation_block);
        self.animation_index = writer.write_integer_index();
    }

    fn write_animation(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        debug_assert!(!self.animation_data.is_empty());

        for animation in &mut self.animation_data {
            animation.write_data(writer)?;
            writer.write_to_short_offset(animation.next_offset, writer.this() - animation.this)?;
        }

        writer.write_to_short_offset(self.animation_data.last().unwrap().next_offset, 0)?;
        writer.write_integer(0); // This is for crowbar as studioMDL does write this as well.

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Animation {
    pub this: usize,
    pub bone: u8,
    pub delta: bool,
    pub next_offset: usize,
    pub position: Option<AnimationData<Vector3>>,
    pub rotation: Option<AnimationData<Quaternion>>,
}

impl Animation {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_unsigned_byte(self.bone);

        let mut flags = AnimationFlags::empty();

        if self.delta {
            flags |= AnimationFlags::DELTA;
        }

        if let Some(data) = &self.rotation {
            match data {
                AnimationData::Raw(_) => flags |= AnimationFlags::RAW_ROTATION,
                AnimationData::Compressed(_) => flags |= AnimationFlags::COMPRESSED_ROTATION,
            }
        }

        if let Some(data) = &self.position {
            match data {
                AnimationData::Raw(_) => flags |= AnimationFlags::RAW_POSITION,
                AnimationData::Compressed(_) => flags |= AnimationFlags::COMPRESSED_POSITION,
            }
        }

        debug_assert!(!flags.contains(AnimationFlags::RAW_ROTATION | AnimationFlags::COMPRESSED_POSITION));
        debug_assert!(!flags.contains(AnimationFlags::COMPRESSED_ROTATION | AnimationFlags::RAW_POSITION));
        writer.write_unsigned_byte(flags.bits());
        self.next_offset = writer.write_short_index();

        if let Some(data) = &mut self.rotation {
            match data {
                AnimationData::Raw(raw) => {
                    writer.write_quaternion64(*raw);
                }
                AnimationData::Compressed(compressed) => {
                    compressed.write_data(writer);
                }
            }
        }

        if let Some(data) = &mut self.position {
            match data {
                AnimationData::Raw(raw) => {
                    writer.write_vector48(*raw);
                }
                AnimationData::Compressed(compressed) => {
                    compressed.write_data(writer);
                }
            }
        }

        if let Some(AnimationData::Compressed(compressed)) = &mut self.rotation {
            compressed.write_values(writer)?;
        }

        if let Some(AnimationData::Compressed(compressed)) = &mut self.position {
            compressed.write_values(writer)?;
        }

        Ok(())
    }
}

bitflags! {
    #[derive(Debug, Default)]
    struct AnimationFlags: u8 {
        const RAW_POSITION        = 0x01;
        const COMPRESSED_POSITION = 0x04;
        const COMPRESSED_ROTATION = 0x08;
        const DELTA               = 0x10;
        const RAW_ROTATION        = 0x20;
    }
}

#[derive(Debug)]
pub enum AnimationData<T: Default> {
    Raw(T),
    Compressed(CompressedAnimation),
}

impl<T: Default> Default for AnimationData<T> {
    fn default() -> Self {
        Self::Raw(Default::default())
    }
}

#[derive(Debug, Default)]
pub struct CompressedAnimation {
    pub this: usize,
    pub offsets: [usize; 3],
    pub values: [Option<Vec<CompressedAnimationEntry>>; 3],
}

impl CompressedAnimation {
    fn write_data(&mut self, writer: &mut FileWriter) {
        self.this = writer.this();

        for axis in 0..3 {
            self.offsets[axis] = writer.write_short_index();
        }
    }

    fn write_values(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        for axis in 0..3 {
            if let Some(values) = &self.values[axis] {
                writer.write_to_short_offset(self.offsets[axis], writer.this() - self.this)?;

                for value in values {
                    match value {
                        CompressedAnimationEntry::Header(header) => {
                            writer.write_unsigned_byte(header.valid);
                            writer.write_unsigned_byte(header.total);
                        }
                        CompressedAnimationEntry::Value(value) => {
                            writer.write_short(*value);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum CompressedAnimationEntry {
    Header(CompressedAnimationEntryHeader),
    Value(i16),
}

impl Default for CompressedAnimationEntry {
    fn default() -> Self {
        Self::Header(Default::default())
    }
}

#[derive(Debug, Default)]
pub struct CompressedAnimationEntryHeader {
    pub valid: u8,
    pub total: u8,
}

#[derive(Debug, Default)]
pub struct SequenceDescription {
    pub this: usize,
    pub name: String,
    pub activity: String,
    pub flags: SequenceDescriptionFlags,
    pub activity_weight: i32,
    pub events: Vec<()>,
    pub event_index: usize,
    pub bounding: BoundingBox,
    pub animations: Vec<i16>,
    pub animation_index: usize,
    pub blend_size: [i32; 2],
    pub parameter_index: [i32; 2],
    pub parameter_start: [f32; 2],
    pub parameter_end: [f32; 2],
    pub fade_in_time: f32,
    pub fade_out_time: f32,
    pub entry_node: i32,
    pub exit_node: i32,
    pub reverse_transition: bool,
    pub ik_rule_count: i32,
    pub auto_layers: Vec<()>,
    pub auto_layer_index: usize,
    pub weight_list: Vec<f32>,
    pub weight_list_index: usize,
    #[allow(dead_code)]
    pub pose_keys: Vec<()>,
    pub pose_key_index: usize,
    pub ik_locks: Vec<()>,
    pub ik_lock_index: usize,
    pub keyvalues: String,
    pub cycle_pose: i32,
    pub activity_modifiers: Vec<()>,
    pub activity_modifier_index: usize,
}

impl SequenceDescription {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_negative_offset(writer.this())?;
        writer.write_string_to_table(self.this, &self.name);
        writer.write_string_to_table(self.this, &self.activity);
        writer.write_integer(self.flags.bits());
        writer.write_integer(0); // Activity
        writer.write_integer(self.activity_weight);
        writer.write_array_size_integer(&self.events)?;
        self.event_index = writer.write_integer_index();
        debug_assert!(self.bounding.is_valid());
        debug_assert!(self.bounding.minimum.is_finite());
        writer.write_vector3(self.bounding.minimum);
        debug_assert!(self.bounding.maximum.is_finite());
        writer.write_vector3(self.bounding.maximum);
        writer.write_integer(0); // Blend Count
        self.animation_index = writer.write_integer_index();
        writer.write_integer(0); // Movement Index
        writer.write_integer_array(&self.blend_size);
        writer.write_integer_array(&self.parameter_index);
        writer.write_float_array(&self.parameter_start);
        writer.write_float_array(&self.parameter_end);
        writer.write_integer(0); // Parameter Parent
        writer.write_float(self.fade_in_time);
        writer.write_float(self.fade_out_time);
        writer.write_integer(self.entry_node);
        writer.write_integer(self.exit_node);
        writer.write_integer(self.reverse_transition as i32);
        writer.write_float(0.0); // Entry Phase
        writer.write_float(0.0); // Exit Phase
        writer.write_float(0.0); // Last Frame
        writer.write_integer(0); // Next Sequence
        writer.write_integer(0); // Pose
        writer.write_integer(self.ik_rule_count);
        writer.write_array_size_integer(&self.auto_layers)?;
        self.auto_layer_index = writer.write_integer_index();
        self.weight_list_index = writer.write_integer_index();
        self.pose_key_index = writer.write_integer_index();
        writer.write_array_size_integer(&self.ik_locks)?;
        self.ik_lock_index = writer.write_integer_index();
        writer.write_string_to_table(self.this, &self.keyvalues);
        writer.write_array_size_integer(self.keyvalues.as_bytes())?;
        writer.write_integer(self.cycle_pose);
        writer.write_array_size_integer(&self.activity_modifiers)?;
        self.activity_modifier_index = writer.write_integer_index();
        writer.write_integer_array(&[0; 5]); // Unused

        Ok(())
    }

    fn write_animations(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.animation_index, writer.this() - self.this)?;

        writer.write_short_array(&self.animations);

        Ok(())
    }

    fn write_bone_weights(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.weight_list_index, writer.this() - self.this)?;

        writer.write_float_array(&self.weight_list);

        Ok(())
    }
}

bitflags! {
    #[derive(Debug, Default)]
    pub struct SequenceDescriptionFlags: i32 {
        const LOOPING    = 0x0001;
        const SNAP       = 0x0002;
        const DELTA      = 0x0004;
        const AUTO_PLAY  = 0x0008;
        const POST       = 0x0010;
        const CYCLE_POSE = 0x0080;
        const REALTIME   = 0x0100;
        const LOCAL      = 0x0200;
        const ACTIVITY   = 0x1000;
        const EVENT      = 0x2000;
        const WORLD      = 0x4000;
    }
}

#[derive(Debug, Default)]
pub struct BodyPart {
    pub this: usize,
    pub name: String,
    pub models: Vec<Model>,
    pub model_index: usize,
    pub base: i32,
}

impl BodyPart {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_string_to_table(self.this, &self.name);
        writer.write_array_size_integer(&self.models)?;
        writer.write_integer(self.base);
        self.model_index = writer.write_integer_index();

        Ok(())
    }

    fn write_models(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.model_index, writer.this() - self.this)?;

        for model in &mut self.models {
            model.write_data(writer)?;
        }

        Ok(())
    }

    fn write_model_mesh_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        for model in &mut self.models {
            model.write_model_mesh_data(writer)?;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Model {
    pub this: usize,
    pub name: String,
    pub meshes: Vec<Mesh>,
    pub mesh_index: usize,
    pub vertex_count: i32,
    pub vertex_offset: i32,
    pub tangent_offset: i32,
    pub eyeballs: Vec<()>,
    pub eyeball_index: usize,
}

impl Model {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_char_array(&self.name, 64);
        writer.write_integer(0); // Type
        writer.write_float(0.0); // Bounding Radius
        writer.write_array_size_integer(&self.meshes)?;
        self.mesh_index = writer.write_integer_index();
        writer.write_integer(self.vertex_count);
        writer.write_integer(self.vertex_offset);
        writer.write_integer(self.tangent_offset);
        writer.write_integer(0); // Attachment Count
        writer.write_integer(0); // Attachment Index
        writer.write_array_size_integer(&self.eyeballs)?;
        self.eyeball_index = writer.write_integer_index();
        writer.write_unsigned_long(0); // Vertex Data
        writer.write_unsigned_long(0); // Tangent Data
        writer.write_integer_array(&[0; 6]); // Unused

        Ok(())
    }

    fn write_model_mesh_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        writer.write_to_integer_offset(self.mesh_index, writer.this() - self.this)?;

        for mesh in &mut self.meshes {
            mesh.model_index = writer.this() - self.this;
            mesh.write_data(writer)?;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Mesh {
    pub this: usize,
    pub material: i32,
    pub model_index: usize,
    pub vertex_count: i32,
    pub vertex_offset: i32,
    pub flexes: Vec<()>,
    pub flex_index: usize,
    pub eyeball_index: Option<i32>,
    pub identifier: i32,
    pub vertex_lod_count: [i32; MAX_LOD_COUNT],
}

impl Mesh {
    fn write_data(&mut self, writer: &mut FileWriter) -> Result<(), FileWriteError> {
        self.this = writer.this();

        writer.write_integer(self.material);
        writer.write_negative_offset(self.model_index)?;
        writer.write_integer(self.vertex_count);
        writer.write_integer(self.vertex_offset);
        writer.write_array_size_integer(&self.flexes)?;
        self.flex_index = writer.write_integer_index();
        writer.write_integer(self.eyeball_index.is_some() as i32);
        writer.write_integer(self.eyeball_index.unwrap_or_default());
        writer.write_integer(self.identifier);
        writer.write_vector3(Vector3::default()); // Center
        writer.write_integer(0); // Unused Model Vertex Data
        writer.write_integer_array(&self.vertex_lod_count);
        writer.write_unsigned_long(0); // Model Vertex Data
        writer.write_integer_array(&[0; 6]); // Unused

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Material {
    pub this: usize,
    pub name: String,
}

impl Material {
    fn write_data(&mut self, writer: &mut FileWriter) {
        self.this = writer.this();

        writer.write_string_to_table(self.this, &self.name);
        writer.write_integer(0); // Flags
        writer.write_integer(0); // Used
        writer.write_integer(0); // Unused
        writer.write_unsigned_long(0); // Material
        writer.write_unsigned_long(0); // Client Material
        writer.write_integer_array(&[0; 8]); // Unused
    }
}
