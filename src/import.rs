use indexmap::IndexMap;
use notify::Watcher;
use parking_lot::RwLock;
use std::{
    fs::File,
    io::{BufReader, Error as IoError},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, mpsc::channel},
    thread,
};
use thiserror::Error as ThisError;

use crate::{
    debug, error,
    utilities::mathematics::{AxisDirection, Quaternion, Vector2, Vector3},
};

mod dmx;
mod obj;
mod smd;

use dmx::ParseDMXError;
use obj::ParseOBJError;
use smd::ParseSMDError;

pub const SUPPORTED_FILES: [&str; 3] = ["smd", "obj", "dmx"];

/// All data that is gathered from a loaded file.
#[derive(Debug, Default)]
pub struct FileData {
    /// The direction that the file considers up.
    pub up: AxisDirection,
    /// The direction that the file considers forward.
    pub forward: AxisDirection,
    /// All bones gathered in the file mapped to a bone name.
    ///
    /// All files should contain at least one bone.
    pub skeleton: IndexMap<String, Bone>,
    /// All animations in the file mapped to a animation name.
    ///
    /// All files should contain at least one animation.
    pub animations: IndexMap<String, Animation>,
    /// All parts in the file mapped to a part name.
    pub parts: IndexMap<String, Part>,
}

/// Data of a bone in a file.
#[derive(Debug, Default)]
pub struct Bone {
    /// An index to the file skeleton that the bone is parented to.
    ///
    /// Is [`None`] when bone is a root bone.
    pub parent: Option<usize>,
    /// The position relative to the parent.
    ///
    /// If [`parent`][Self::parent] is [`None`] then location is absolute.
    pub location: Vector3,
    /// The orientation relative to the parent.
    ///
    /// If [`parent`][Self::parent] is [`None`] then rotation is absolute.
    pub rotation: Quaternion,
}

/// Data of an animation in a file.
#[derive(Debug)]
pub struct Animation {
    /// The amount of frames the animation stores.
    pub frame_count: NonZeroUsize,
    /// All channels in animation mapped to an index for a bone in the file skeleton.
    pub channels: IndexMap<usize, Channel>,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            frame_count: NonZeroUsize::MIN,
            channels: Default::default(),
        }
    }
}

/// Data of an animation channel for a bone in a file.
#[derive(Debug, Default)]
pub struct Channel {
    /// Locational keyed data of the channel mapped to a frame.
    pub location: IndexMap<usize, Vector3>,
    /// Rotational keyed data of the channel mapped to a frame.
    pub rotation: IndexMap<usize, Quaternion>,
}

/// Data of a part for a file.
#[derive(Debug, Default)]
pub struct Part {
    /// All vertices that the part uses.
    pub vertices: Vec<Vertex>,
    /// List of faces the part has mapped to a material name.
    ///
    /// A face is a list of indices to the parts vertex list.
    ///
    /// All faces should be clockwise order.
    pub faces: IndexMap<String, Vec<Vec<usize>>>,
    /// List of flex data mapped to a flex name.
    ///
    /// A flex is a list of [FlexVertex] mapped to a vertex index.
    pub flexes: IndexMap<String, IndexMap<usize, FlexVertex>>,
}

/// Data of a vertex for a file.
#[derive(Debug, Default)]
pub struct Vertex {
    /// The location of the vertex. The location is absolute.
    pub location: Vector3,
    /// The normal direction of the vertex.
    pub normal: Vector3,
    /// The UV position of the vertex.
    pub texture_coordinate: Vector2,
    /// List of weights the vertex has mapped to a bone in the file skeleton.
    pub links: IndexMap<usize, f64>,
}

/// Data of a flexed vertex for a file.
#[derive(Debug, Default)]
pub struct FlexVertex {
    /// The location of the flexed vertex.
    pub location: Vector3,
    /// The normal of the flexed vertex.
    pub normal: Vector3,
}

#[derive(Debug, ThisError)]
enum ParseFileError {
    #[error("Failed To Open File")]
    FailedFileOpen(#[from] IoError),
    #[error("File Does Not Exist")]
    FileDoesNotExist,
    #[error("File Does Not Have Extension")]
    FileDoesNotHaveExtension,
    #[error("File Does Not Have Name")]
    FileDoesNotHaveName,
    #[error("File Format Is Not Supported")]
    UnsupportedFileFormat,
    #[error("Unhandled File Read Error: {0}")]
    UnhandledReadError(String),
    // When supporting another file format, put it under this comment.
    #[error("Failed To Parse SMD File: {0}")]
    FailedSMDFileParse(#[from] ParseSMDError),
    #[error("Failed To Parse OBJ File: {0}")]
    FailedOBJFileParse(#[from] ParseOBJError),
    #[error("Failed To Parse DMX File: {0}")]
    FailedDMXFileParse(#[from] ParseDMXError),
}

#[derive(Clone, Debug, Default)]
pub enum FileStatus {
    #[default]
    Loading,
    Loaded(Arc<FileData>),
    Failed,
}

#[derive(Clone, Debug, Default)]
pub struct FileManager {
    /// A thread safe storage of loaded [FileStatus] with a reference count. If the reference count reaches zero then the file is unloaded.
    loaded_files: Arc<RwLock<IndexMap<PathBuf, (usize, FileStatus)>>>,
    file_watcher: Option<Arc<RwLock<notify::RecommendedWatcher>>>,
}

impl FileManager {
    pub fn start_file_watch(&mut self) -> Result<(), notify::Error> {
        let (tx, rx) = channel();

        let watcher = notify::recommended_watcher(tx)?;
        self.file_watcher = Some(Arc::new(RwLock::new(watcher)));

        let manager = self.clone();
        std::thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(event) => match event {
                        Ok(event) => {
                            let mut paths = event.paths; // Does this need to be looped over?
                            let file_path = paths.remove(0);

                            match event.kind {
                                notify::EventKind::Modify(_) => {
                                    if matches!(manager.get_file_status(&file_path), Some(FileStatus::Loading)) {
                                        continue;
                                    }

                                    let mut loaded_files = manager.loaded_files.write();

                                    if let Some((_, status)) = loaded_files.get_mut(&file_path) {
                                        *status = FileStatus::Loading;
                                    }

                                    // FIXME: Not the best solution be it is a solution.
                                    drop(loaded_files);
                                    std::thread::sleep(std::time::Duration::from_millis(50));

                                    manager.load_file_data(file_path);
                                }
                                notify::EventKind::Remove(remove_kind) => {
                                    let mut loaded_files = manager.loaded_files.write();

                                    debug_assert!(!matches!(remove_kind, notify::event::RemoveKind::File));

                                    if let Some((_, status)) = loaded_files.get_mut(&file_path) {
                                        *status = FileStatus::Failed;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(error) => {
                            error!("Fail To Watch File: {error}!");
                        }
                    },
                    Err(error) => {
                        error!("Fail To Watch Files: {error}!");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Loads the file data if not loaded else increase the reference count by one.
    pub fn load_file(&mut self, file_path: PathBuf) {
        let mut files = self.loaded_files.write();
        if let Some((existing_count, _)) = files.get_mut(&file_path) {
            *existing_count += 1;
            return;
        }
        files.insert(file_path.clone(), (1, FileStatus::Loading));

        if let Some(watcher) = &self.file_watcher {
            let mut watch = watcher.write();
            let _ = watch.watch(&file_path, notify::RecursiveMode::NonRecursive);
        }

        self.load_file_data(file_path);
    }

    /// This spawns a new thread and loads the specified file to the manager.
    fn load_file_data(&self, file_path: PathBuf) {
        let manager = self.clone();
        thread::spawn(move || {
            let loaded_file = (|| {
                if !file_path.try_exists()? {
                    return Err(ParseFileError::FileDoesNotExist);
                }

                let file_extension = file_path.extension().ok_or(ParseFileError::FileDoesNotHaveExtension)?;
                let file_name = file_path.file_stem().ok_or(ParseFileError::FileDoesNotHaveName)?.to_string_lossy().to_string();
                let file_buffer = BufReader::new(File::open(&file_path)?);

                // If a file parser panics that means it has a unhandled error. Any unhandled errors must be handled and added to parser's error enum.
                let loaded_file = match std::panic::catch_unwind(|| {
                    Ok(match file_extension.to_string_lossy().to_lowercase().as_str() {
                        "smd" => smd::load_smd(file_buffer, file_name)?,
                        "obj" => obj::load_obj(file_buffer, file_name)?,
                        "dmx" => dmx::load_dmx(file_buffer, file_name)?,
                        _ => return Err(ParseFileError::UnsupportedFileFormat),
                    })
                }) {
                    Ok(read_file) => read_file,
                    Err(read_error) => {
                        if let Some(error_message) = read_error.downcast_ref::<&str>() {
                            Err(ParseFileError::UnhandledReadError(error_message.to_string()))
                        } else if let Ok(error_message) = read_error.downcast::<String>() {
                            Err(ParseFileError::UnhandledReadError(error_message.to_string()))
                        } else {
                            Err(ParseFileError::UnhandledReadError("NON STRING PANIC!".to_string()))
                        }
                    }
                }?;

                debug!(
                    "Loaded \"{}\" file: \"{}\".",
                    file_extension.to_string_lossy().to_uppercase(),
                    file_path.as_os_str().to_string_lossy()
                );

                Ok(loaded_file)
            })();

            let mut loaded_files = manager.loaded_files.write();

            let file_data = match loaded_file {
                Ok(data) => data,
                Err(error) => {
                    error!("Fail To Load File: {error}!");

                    if let Some((_, status)) = loaded_files.get_mut(&file_path) {
                        *status = FileStatus::Failed;
                    }

                    return;
                }
            };

            debug_assert!(!file_data.skeleton.is_empty(), "File source must have 1 bone!");
            debug_assert!(!file_data.animations.is_empty(), "File source must have 1 animation!");
            debug_assert!(!file_data.forward.is_parallel(file_data.up), "File Source Directions are parallel!");

            if let Some((_, status)) = loaded_files.get_mut(&file_path) {
                *status = FileStatus::Loaded(Arc::new(file_data));
            }
        });
    }

    /// Decreases the reference count of a path by one. If the count is zero then it unloads the file data.
    pub fn unload_file(&mut self, file_path: &Path) {
        let mut loaded_files = self.loaded_files.write();
        if let Some((existing_count, _)) = loaded_files.get_mut(file_path) {
            let current_count = *existing_count - 1;

            if current_count == 0 {
                debug!("Unloaded {}!", file_path.as_os_str().to_string_lossy());
                loaded_files.shift_remove(file_path);

                if let Some(watcher) = &self.file_watcher {
                    let mut watch = watcher.write();
                    let _ = watch.unwatch(file_path);
                }

                return;
            }

            *existing_count = current_count;
        }
    }

    /// Returns the status of a loaded file. If the path was unloaded then there will be no status.
    pub fn get_file_status(&self, file_path: &Path) -> Option<FileStatus> {
        self.loaded_files.read().get(file_path).map(|(_, status)| status).cloned()
    }

    /// Returns the file data of a path if successfully loaded.
    pub fn get_file_data(&self, file_path: &Path) -> Option<Arc<FileData>> {
        self.loaded_files
            .read()
            .get(file_path)
            .and_then(|(_, status)| if let FileStatus::Loaded(data) = status { Some(data.clone()) } else { None })
    }
}
