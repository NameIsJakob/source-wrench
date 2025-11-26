use std::path::PathBuf;

use crate::utilities::mathematics::Vector3;

#[derive(Clone, Debug, Default)]
pub struct SourceInput {
    /// The name of the output mdl file.
    pub model_name: String,
    /// The path to where the mdl is exported.
    pub export_path: Option<PathBuf>,
    pub body_groups: Vec<BodyPart>,
    pub define_bones: Vec<DefineBone>,
    pub animations: Vec<Animation>,
    pub sequences: Vec<Sequence>,
}

/// A struct to define a body part for the model.
#[derive(Clone, Debug)]
pub struct BodyPart {
    pub name: String,
    /// The models used by the body part.
    pub models: Vec<Model>,
}

impl Default for BodyPart {
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
    pub name: String,
    pub has_parent: bool,
    pub parent: String,
    pub location: Vector3,
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
    pub name: String,
    /// The source file to get the animation data from.
    pub source_file_path: Option<PathBuf>,
    /// The animation to get in the source file.
    pub source_animation: usize,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            name: String::from("New Animation"),
            source_file_path: Default::default(),
            source_animation: Default::default(),
        }
    }
}

/// A struct the define a sequence for a model.
#[derive(Clone, Debug)]
pub struct Sequence {
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
