use bitflags::bitflags;
use indexmap::{IndexMap, IndexSet};
use thiserror::Error as ThisError;

use crate::{
    debug,
    import::FileManager,
    info, input,
    utilities::mathematics::{BoundingBox, Matrix4, Quaternion, Vector2, Vector3, Vector4},
    verbose,
};

mod animation;
mod bones;
mod mesh;
mod sequences;

use animation::{ProcessingAnimationError, process_animations};
use bones::{ProcessingBoneError, process_bones};
use mesh::{ProcessingMeshError, process_meshes};
use sequences::{ProcessingSequenceError, process_sequences};

#[derive(Debug, Default)]
pub struct ProcessedData {
    pub bone_data: BoneData,
    pub animation_data: AnimationData,
    pub sequence_data: IndexMap<String, Sequence>,
    pub model_data: ModelData,
}

#[derive(Debug, Default)]
pub struct BoneData {
    pub processed_bones: IndexMap<String, Bone>,
    /// Indexes of all processed bones sorted by name.
    pub sorted_bones_by_name: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct Bone {
    /// The index of the parent bone. None if the bone is a root bone.
    pub parent: Option<usize>,
    /// The location of the bone relative to the parent bone.
    pub location: Vector3,
    /// The rotation of the bone relative to the parent bone.
    pub rotation: Quaternion,
    /// The flags the bone has.
    pub flags: BoneFlags,
    /// The transforms in world space.
    pub world_transform: Matrix4,
}

bitflags! {
    #[derive(Debug, Default)]
    pub struct BoneFlags: i32 {
        const USED_BY_VERTEX = 0x00000400;
    }
}

#[derive(Debug, Default)]
pub struct AnimationData {
    pub processed_animations: IndexMap<String, Animation>,
    /// The scales for location an rotation for the run length encoding.
    pub animation_scales: Vec<(Vector3, Vector3)>,
    /// Used by sequence to get the correct animation when unused animations are removed.
    pub remapped_animations: Vec<usize>,
}

#[derive(Debug, Default)]
pub struct Animation {
    pub frame_count: usize,
    pub sections: Vec<Vec<AnimatedBoneData>>,
}

#[derive(Debug, Default)]
pub struct AnimatedBoneData {
    pub bone: u8,
    pub raw_position: Vec<Vector3>,
    pub raw_rotation: Vec<Quaternion>,
    pub delta_position: Vec<Vector3>,
    pub delta_rotation: Vec<Quaternion>,
}

#[derive(Debug, Default)]
pub struct Sequence {
    pub animations: Vec<Vec<i16>>,
}

#[derive(Debug, Default)]
pub struct ModelData {
    pub body_parts: IndexMap<String, BodyPart>,
    pub bounding_box: BoundingBox,
    pub hitboxes: IndexMap<u8, BoundingBox>,
    pub materials: IndexSet<String>,
}

#[derive(Debug, Default)]
pub struct BodyPart {
    pub models: Vec<Model>,
}

#[derive(Debug, Default)]
pub struct Model {
    pub name: String,
    pub meshes: Vec<Mesh>,
}

#[derive(Debug, Default)]
pub struct Mesh {
    pub material: i32,
    pub vertex_data: Vec<Vertex>,
    pub strip_groups: Vec<StripGroup>,
}

#[derive(Debug, Default)]
pub struct Vertex {
    pub weights: [f32; 3],
    pub bones: [u8; 3],
    pub bone_count: u8,
    pub position: Vector3,
    pub normal: Vector3,
    pub texture_coordinate: Vector2,
    pub tangent: Vector4,
}

#[derive(Debug, Default)]
pub struct StripGroup {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u16>,
    pub strips: Vec<Strip>,
}

#[derive(Debug, Default)]
pub struct MeshVertex {
    pub bone_count: u8,
    pub vertex_index: u16,
    pub bones: [u8; 3],
}

#[derive(Debug, Default)]
pub struct Strip {
    pub indices_count: i32,
    pub indices_offset: i32,
    pub vertex_count: i32,
    pub vertex_offset: i32,
    pub bone_count: i16,
    pub hardware_bones: Vec<HardwareBone>,
}

#[derive(Debug, Default)]
pub struct HardwareBone {
    pub hardware_bone: i32,
    pub bone_table_bone: i32,
}

#[derive(Debug, ThisError)]
pub enum ProcessingDataError {
    #[error("Model Has No Bones")]
    NoBones,
    #[error("Model Has No Sequences")]
    NoSequences,
    #[error("Model Has No Animations")]
    NoAnimations,
    #[error("Failed To Process Bone Data: {0}")]
    ProcessingBoneError(#[from] ProcessingBoneError),
    #[error("Failed To Process Animation Data: {0}")]
    ProcessingAnimationError(#[from] ProcessingAnimationError),
    #[error("Failed To Process Sequence Data: {0}")]
    ProcessingSequenceError(#[from] ProcessingSequenceError),
    #[error("Failed To Process Mesh Data: {0}")]
    ProcessingMeshError(#[from] ProcessingMeshError),
}

pub const MAX_HARDWARE_BONES_PER_STRIP: usize = 53;
pub const VERTEX_CACHE_SIZE: usize = 16;

/// The tolerance for floating point numbers until they are considered equal.
pub const FLOAT_TOLERANCE: f64 = f32::EPSILON as f64;

pub fn process(input: &input::CompilationData, file_manager: &FileManager) -> Result<ProcessedData, ProcessingDataError> {
    debug!("Processing Bones.");
    let processed_bone_data = process_bones(input, file_manager)?;
    info!("Model uses {} bones.", processed_bone_data.processed_bones.len());

    if processed_bone_data.processed_bones.is_empty() {
        return Err(ProcessingDataError::NoBones);
    }

    debug!("Processing Animations.");
    let processed_animation_data = process_animations(input, file_manager, &processed_bone_data)?;
    verbose!("Model has {} animations.", processed_animation_data.processed_animations.len());

    if processed_animation_data.processed_animations.is_empty() {
        return Err(ProcessingDataError::NoAnimations);
    }

    debug!("Processing Sequences.");
    let processed_sequences = process_sequences(input, &processed_animation_data.remapped_animations)?;
    info!("Model has {} sequences.", processed_sequences.len());

    if processed_sequences.is_empty() {
        return Err(ProcessingDataError::NoSequences);
    }

    debug!("Processing Mesh Data.");
    let processed_mesh = process_meshes(input, file_manager, &processed_bone_data)?;
    verbose!("Model has {} materials.", processed_mesh.materials.len());
    info!("Model has {} body parts.", processed_mesh.body_parts.len());

    Ok(ProcessedData {
        bone_data: processed_bone_data,
        animation_data: processed_animation_data,
        sequence_data: processed_sequences,
        model_data: processed_mesh,
    })
}
