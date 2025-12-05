use std::path::PathBuf;

use crate::utilities::mathematics::Vector3;

#[derive(Clone, Debug, Default)]
pub struct SourceInput {
    /// The name of the output mdl file.
    pub model_name: String,
    /// The path to where the mdl is exported.
    pub export_path: Option<PathBuf>,
    pub body_groups: Vec<BodyGroup>,
    pub define_bones: Vec<DefineBone>,
    pub animation_identifier_generator: usize,
    pub animations: Vec<Animation>,
    pub sequences: Vec<Sequence>,
}

/// A struct to define a body part for the model.
#[derive(Clone, Debug)]
pub struct BodyGroup {
    /// The unique name of body group.
    pub name: String,
    /// The models used by the body part.
    pub models: Vec<Model>,
}

impl Default for BodyGroup {
    fn default() -> Self {
        Self {
            name: String::from("New Body Group"),
            models: Default::default(),
        }
    }
}

/// A struct to define a model for a body part.
#[derive(Clone, Debug)]
pub struct Model {
    /// The unique name of model.
    pub name: String,
    /// This specify if the model will have no mesh.
    pub blank: bool,
    /// The source file to get the mesh data from.
    pub source_file_path: Option<PathBuf>,
    /// All the parts to use in the source file.
    pub enabled_source_parts: Vec<bool>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            name: String::from("New Model"),
            blank: Default::default(),
            source_file_path: Default::default(),
            enabled_source_parts: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DefineBone {
    /// The unique name of the bone to define.
    pub name: String,
    /// Specifies if the the define bone has a parent.
    pub has_parent: bool,
    /// The name of the parent bone if has a parent.
    pub parent: String,
    /// The position of the bone relative to the parent.
    pub location: Vector3,
    /// The rotation of the bone relative to the parent.
    /// These are as pitch, yaw, and roll for compatibility.
    pub rotation: Vector3,
}

impl Default for DefineBone {
    fn default() -> Self {
        Self {
            name: String::from("New Bone"),
            has_parent: Default::default(),
            parent: Default::default(),
            location: Default::default(),
            rotation: Default::default(),
        }
    }
}

/// A struct to define an animation for the model.
#[derive(Clone, Debug)]
pub struct Animation {
    /// The unique name of the animation.
    pub name: String,
    /// The source file to get the animation data from.
    pub source_file_path: Option<PathBuf>,
    /// The animation to get in the source file.
    pub source_animation: usize,
    /// A unique values used by sequences to find the correct animation as animations order and name can be changed.
    pub animation_identifier: usize,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            name: String::from("New Animation"),
            source_file_path: Default::default(),
            source_animation: Default::default(),
            animation_identifier: Default::default(),
        }
    }
}

/// A struct the define a sequence for a model.
#[derive(Clone, Debug)]
pub struct Sequence {
    /// The unique name of the sequence.
    pub name: String,
    /// A N by N grid of animations used by the sequence.
    pub animations: Vec<Vec<usize>>,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            name: String::from("New Sequence"),
            animations: Default::default(),
        }
    }
}
