use indexmap::IndexMap;
use std::{
    fs::File,
    io::{BufRead, BufReader, Error},
    num::{NonZero, ParseFloatError, ParseIntError},
};
use thiserror::Error as ThisError;

use crate::utilities::mathematics::{AxisDirection, EULER_ROTATION, Quaternion, Vector2, Vector3};

#[derive(Debug, ThisError)]
pub enum ParseSMDError {
    #[error("IO Error: {0}")]
    IOError(#[from] Error),
    #[error("Invalid Quote Delimiter At Line {0}, Column {1}")]
    InvalidQuoteDelimiter(usize, usize),
    #[error("Unfinished Quote Block At Line {0}, Column {1}")]
    UnfinishedQuoteBlock(usize, usize),
    #[error("Unexpected End Of File")]
    UnexpectedEndOfFile,
    #[error("Missing Command Argument '{0}' On Line {1}")]
    MissingArgument(&'static str, usize),
    #[error("Unknown Command Argument '{0}' On Line {1}")]
    UnknownArgument(String, usize),
    #[error("Failed To Parse Integer Argument: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("Failed To Parse Float Argument: {0}")]
    ParseFloatError(#[from] ParseFloatError),
    #[error("Duplicate Version Command On Line {0}")]
    DuplicateVersionCommand(usize),
    #[error("Missing Version Command")]
    MissingVersionCommand,
    #[error("Invalid Version Number {0}")]
    InvalidVersionNumber(usize),
    #[error("Duplicate Node Id {0} On Line {1}")]
    DuplicateNodeId(usize, usize),
    #[error("Invalid Node Id {0} On Line {1}")]
    InvalidNodeId(usize, usize),
    #[error("No Nodes Specified")]
    NoNodesSpecified,
    #[error("Frames Are Not Sequential On Line {0}")]
    NonSequentialFrames(usize),
    #[error("Missing Frame Before Nodes On Line {0}")]
    MissingFirstFrame(usize),
    #[error("Duplicate Node Key {0} On Line {1}")]
    DuplicateNodeKey(usize, usize),
    #[error("No Frames Specified")]
    NoFramesSpecified,
    #[error("Duplicate Node Link {0} On Line {1}")]
    DuplicateNodeLink(usize, usize),
    #[error("Unknown Studio Command '{0}' On Line {1}")]
    UnknownStudioCommand(String, usize),
    #[error("Node {0} Does Not Have A Bind On First Frame")]
    MissingBoneBind(usize),
}

pub fn load_smd(file_buffer: BufReader<File>, file_name: String) -> Result<super::FileData, ParseSMDError> {
    let mut reader = FileReader::new(file_buffer);

    let mut version = None;
    let mut nodes = IndexMap::new();
    let mut frames = Vec::new();
    let mut triangles = IndexMap::new();

    struct Node {
        name: String,
        parent: Option<usize>,
    }

    struct Key {
        position: Vector3,
        rotation: Quaternion,
    }

    struct Vertex {
        position: Vector3,
        normal: Vector3,
        texture_coordinate: Vector2,
        links: IndexMap<usize, f64>,
        #[allow(dead_code)]
        extra_texture_coordinates: Vec<Vector2>,
    }

    while let Some(token) = reader.next_token(false)? {
        if let Some(token_string) = token.get_string() {
            match token_string.as_str() {
                "version" => {
                    if version.is_some() {
                        return Err(ParseSMDError::DuplicateVersionCommand(reader.line));
                    }
                    let version_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                    let version_string = version_token
                        .get_string()
                        .ok_or(ParseSMDError::MissingArgument("Version Number", reader.line))?;
                    let version_number = version_string.parse()?;
                    if !(1..=3).contains(&version_number) {
                        return Err(ParseSMDError::InvalidVersionNumber(version_number));
                    }
                    version = Some(version_number);

                    if let Some(check_token) = reader.next_token(false)? {
                        if let Some(check_token_string) = check_token.get_string() {
                            return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                        }
                    }
                }
                "nodes" => {
                    if version.is_none() {
                        return Err(ParseSMDError::MissingVersionCommand);
                    }

                    loop {
                        let token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                        if let Some(token_string) = token.get_string() {
                            if token_string == "end" {
                                break;
                            }

                            let node_id = token_string.parse()?;

                            if nodes.contains_key(&node_id) {
                                return Err(ParseSMDError::DuplicateNodeId(node_id, reader.line));
                            }

                            let name_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let node_name = name_token.get_string().ok_or(ParseSMDError::MissingArgument("Node name", reader.line))?;

                            let parent_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let parent_string = parent_token.get_string().ok_or(ParseSMDError::MissingArgument("Node Parent", reader.line))?;
                            let node_parent = if parent_string.eq("-1") { None } else { Some(parent_string.parse()?) };

                            if let Some(parent) = node_parent {
                                if !nodes.contains_key(&parent) {
                                    return Err(ParseSMDError::InvalidNodeId(node_id, reader.line));
                                }
                            }

                            nodes.insert(
                                node_id,
                                Node {
                                    name: node_name,
                                    parent: node_parent,
                                },
                            );

                            if let Some(check_token) = reader.next_token(false)? {
                                if let Some(check_token_string) = check_token.get_string() {
                                    return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                }
                            }
                        }
                    }
                }
                "skeleton" => {
                    if version.is_none() {
                        return Err(ParseSMDError::MissingVersionCommand);
                    }

                    if nodes.is_empty() {
                        return Err(ParseSMDError::NoNodesSpecified);
                    }

                    loop {
                        let token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                        if let Some(token_string) = token.get_string() {
                            if token_string == "end" {
                                break;
                            }

                            if token_string == "time" {
                                let frame_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                let frame_string = frame_token.get_string().ok_or(ParseSMDError::MissingArgument("Frame Number", reader.line))?;
                                let frame_number = frame_string.parse()?;

                                if frames.len() != frame_number {
                                    return Err(ParseSMDError::NonSequentialFrames(reader.line));
                                }

                                frames.push(IndexMap::new());
                                continue;
                            }

                            let node_id = token_string.parse::<usize>()?;

                            if !nodes.contains_key(&node_id) {
                                return Err(ParseSMDError::InvalidNodeId(node_id, reader.line));
                            }

                            let last_frame = frames.last_mut().ok_or(ParseSMDError::MissingFirstFrame(reader.line))?;

                            if last_frame.contains_key(&node_id) {
                                return Err(ParseSMDError::DuplicateNodeKey(node_id, reader.line));
                            }

                            let position_x_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let position_x_string = position_x_token.get_string().ok_or(ParseSMDError::MissingArgument("Position X", reader.line))?;
                            let position_x = position_x_string.parse()?;

                            let position_y_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let position_y_string = position_y_token.get_string().ok_or(ParseSMDError::MissingArgument("Position Y", reader.line))?;
                            let position_y = position_y_string.parse()?;

                            let position_z_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let position_z_string = position_z_token.get_string().ok_or(ParseSMDError::MissingArgument("Position Z", reader.line))?;
                            let position_z = position_z_string.parse()?;

                            let rotation_x_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let rotation_x_string = rotation_x_token.get_string().ok_or(ParseSMDError::MissingArgument("Rotation X", reader.line))?;
                            let rotation_x = rotation_x_string.parse()?;

                            let rotation_y_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let rotation_y_string = rotation_y_token.get_string().ok_or(ParseSMDError::MissingArgument("Rotation Y", reader.line))?;
                            let rotation_y = rotation_y_string.parse()?;

                            let rotation_z_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                            let rotation_z_string = rotation_z_token.get_string().ok_or(ParseSMDError::MissingArgument("Rotation Z", reader.line))?;
                            let rotation_z = rotation_z_string.parse()?;

                            last_frame.insert(
                                node_id,
                                Key {
                                    position: Vector3::new(position_x, position_y, position_z),
                                    rotation: Quaternion::from_euler(EULER_ROTATION, rotation_x, rotation_y, rotation_z),
                                },
                            );

                            if let Some(check_token) = reader.next_token(false)? {
                                if let Some(check_token_string) = check_token.get_string() {
                                    return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                }
                            }
                        }
                    }
                }
                "triangles" => {
                    if version.is_none() {
                        return Err(ParseSMDError::MissingVersionCommand);
                    }

                    if nodes.is_empty() {
                        return Err(ParseSMDError::NoNodesSpecified);
                    }

                    if frames.is_empty() {
                        return Err(ParseSMDError::NoFramesSpecified);
                    }

                    loop {
                        let token = reader.next_token(true)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;

                        if let Some(token_string) = token.get_string() {
                            if token_string.is_empty() {
                                continue;
                            }
                            if token_string == "end" {
                                break;
                            }

                            let mut vertices = Vec::with_capacity(3);

                            if let Some(check_token) = reader.next_token(false)? {
                                if let Some(check_token_string) = check_token.get_string() {
                                    return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                }
                            }

                            loop {
                                if vertices.len() == 3 {
                                    break;
                                }

                                let token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;

                                if let Some(token_string) = token.get_string() {
                                    let node_id = token_string.parse::<usize>()?;

                                    if !nodes.contains_key(&node_id) {
                                        return Err(ParseSMDError::InvalidNodeId(node_id, reader.line));
                                    }

                                    let position_x_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let position_x_string = position_x_token.get_string().ok_or(ParseSMDError::MissingArgument("Position X", reader.line))?;
                                    let position_x = position_x_string.parse()?;

                                    let position_y_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let position_y_string = position_y_token.get_string().ok_or(ParseSMDError::MissingArgument("Position Y", reader.line))?;
                                    let position_y = position_y_string.parse()?;

                                    let position_z_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let position_z_string = position_z_token.get_string().ok_or(ParseSMDError::MissingArgument("Position Z", reader.line))?;
                                    let position_z = position_z_string.parse()?;

                                    let normal_x_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let normal_x_string = normal_x_token.get_string().ok_or(ParseSMDError::MissingArgument("Normal X", reader.line))?;
                                    let normal_x = normal_x_string.parse()?;

                                    let normal_y_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let normal_y_string = normal_y_token.get_string().ok_or(ParseSMDError::MissingArgument("Normal Y", reader.line))?;
                                    let normal_y = normal_y_string.parse()?;

                                    let normal_z_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let normal_z_string = normal_z_token.get_string().ok_or(ParseSMDError::MissingArgument("Normal Z", reader.line))?;
                                    let normal_z = normal_z_string.parse()?;

                                    let texture_coordinate_u_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let texture_coordinate_u_string = texture_coordinate_u_token
                                        .get_string()
                                        .ok_or(ParseSMDError::MissingArgument("Texture Coordinate U", reader.line))?;
                                    let texture_coordinate_u = texture_coordinate_u_string.parse()?;

                                    let texture_coordinate_v_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                    let texture_coordinate_v_string = texture_coordinate_v_token
                                        .get_string()
                                        .ok_or(ParseSMDError::MissingArgument("Texture Coordinate V", reader.line))?;
                                    let texture_coordinate_v = texture_coordinate_v_string.parse()?;

                                    if let Some(link_count_token) = reader.next_token(false)? {
                                        if let Some(link_count_string) = link_count_token.get_string() {
                                            let link_count = link_count_string.parse()?;
                                            let mut links = IndexMap::with_capacity(link_count);

                                            for _ in 0..link_count {
                                                let link_id_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                                let link_id_token_string =
                                                    link_id_token.get_string().ok_or(ParseSMDError::MissingArgument("Link Id", reader.line))?;
                                                let link_id = link_id_token_string.parse()?;

                                                if !nodes.contains_key(&link_id) {
                                                    return Err(ParseSMDError::InvalidNodeId(node_id, reader.line));
                                                }

                                                if links.contains_key(&link_id) {
                                                    return Err(ParseSMDError::DuplicateNodeLink(node_id, reader.line));
                                                }

                                                let link_weight_token = reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                                let link_weight_token_string = link_weight_token
                                                    .get_string()
                                                    .ok_or(ParseSMDError::MissingArgument("Link Weight", reader.line))?;
                                                let link_weight = link_weight_token_string.parse()?;

                                                links.insert(link_id, link_weight);
                                            }

                                            let weight_count = links.values().sum::<f64>();

                                            if version.unwrap() < 3 {
                                                if weight_count == 0.0 {
                                                    vertices.push(Vertex {
                                                        position: Vector3::new(position_x, position_y, position_z),
                                                        normal: Vector3::new(normal_x, normal_y, normal_z),
                                                        texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                        links: IndexMap::from([(node_id, 1.0)]),
                                                        extra_texture_coordinates: Vec::new(),
                                                    });
                                                    if let Some(check_token) = reader.next_token(false)? {
                                                        if let Some(check_token_string) = check_token.get_string() {
                                                            return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                                        }
                                                    }
                                                    continue;
                                                }

                                                vertices.push(Vertex {
                                                    position: Vector3::new(position_x, position_y, position_z),
                                                    normal: Vector3::new(normal_x, normal_y, normal_z),
                                                    texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                    links,
                                                    extra_texture_coordinates: Vec::new(),
                                                });
                                                if let Some(check_token) = reader.next_token(false)? {
                                                    if let Some(check_token_string) = check_token.get_string() {
                                                        return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                                    }
                                                }
                                                continue;
                                            }

                                            if let Some(extra_texture_coordinates_token) = reader.next_token(false)? {
                                                if let Some(extra_texture_coordinates_string) = extra_texture_coordinates_token.get_string() {
                                                    let extra_texture_coordinate_count = extra_texture_coordinates_string.parse()?;
                                                    let mut extra_texture_coordinates = Vec::with_capacity(extra_texture_coordinate_count);

                                                    for _ in 0..extra_texture_coordinate_count {
                                                        let extra_texture_coordinate_u_token =
                                                            reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                                        let extra_texture_coordinate_u_string = extra_texture_coordinate_u_token
                                                            .get_string()
                                                            .ok_or(ParseSMDError::MissingArgument("Extra Texture Coordinate U", reader.line))?;
                                                        let extra_texture_coordinate_u = extra_texture_coordinate_u_string.parse()?;

                                                        let extra_texture_coordinate_v_token =
                                                            reader.next_token(false)?.ok_or(ParseSMDError::UnexpectedEndOfFile)?;
                                                        let extra_texture_coordinate_v_string = extra_texture_coordinate_v_token
                                                            .get_string()
                                                            .ok_or(ParseSMDError::MissingArgument("Extra Texture Coordinate V", reader.line))?;
                                                        let extra_texture_coordinate_v = extra_texture_coordinate_v_string.parse()?;

                                                        extra_texture_coordinates.push(Vector2::new(extra_texture_coordinate_u, extra_texture_coordinate_v));
                                                    }

                                                    if weight_count == 0.0 {
                                                        vertices.push(Vertex {
                                                            position: Vector3::new(position_x, position_y, position_z),
                                                            normal: Vector3::new(normal_x, normal_y, normal_z),
                                                            texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                            links: IndexMap::from([(node_id, 1.0)]),
                                                            extra_texture_coordinates,
                                                        });
                                                        if let Some(check_token) = reader.next_token(false)? {
                                                            if let Some(check_token_string) = check_token.get_string() {
                                                                return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                                            }
                                                        }
                                                        continue;
                                                    }

                                                    vertices.push(Vertex {
                                                        position: Vector3::new(position_x, position_y, position_z),
                                                        normal: Vector3::new(normal_x, normal_y, normal_z),
                                                        texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                        links,
                                                        extra_texture_coordinates,
                                                    });
                                                    if let Some(check_token) = reader.next_token(false)? {
                                                        if let Some(check_token_string) = check_token.get_string() {
                                                            return Err(ParseSMDError::UnknownArgument(check_token_string, reader.line));
                                                        }
                                                    }
                                                    continue;
                                                }
                                            }

                                            if weight_count == 0.0 {
                                                vertices.push(Vertex {
                                                    position: Vector3::new(position_x, position_y, position_z),
                                                    normal: Vector3::new(normal_x, normal_y, normal_z),
                                                    texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                    links: IndexMap::from([(node_id, 1.0)]),
                                                    extra_texture_coordinates: Vec::new(),
                                                });
                                                continue;
                                            }

                                            vertices.push(Vertex {
                                                position: Vector3::new(position_x, position_y, position_z),
                                                normal: Vector3::new(normal_x, normal_y, normal_z),
                                                texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                                links,
                                                extra_texture_coordinates: Vec::new(),
                                            });
                                            continue;
                                        }
                                    }

                                    vertices.push(Vertex {
                                        position: Vector3::new(position_x, position_y, position_z),
                                        normal: Vector3::new(normal_x, normal_y, normal_z),
                                        texture_coordinate: Vector2::new(texture_coordinate_u, texture_coordinate_v),
                                        links: IndexMap::from([(node_id, 1.0)]),
                                        extra_texture_coordinates: Vec::new(),
                                    });
                                }
                            }

                            let vertex3 = vertices.pop().unwrap();
                            let vertex2 = vertices.pop().unwrap();
                            let vertex1 = vertices.pop().unwrap();

                            let material: &mut Vec<[Vertex; 3]> = triangles.entry(token_string).or_default();
                            material.push([vertex1, vertex2, vertex3]);
                        }
                    }
                }
                _ => {
                    return Err(ParseSMDError::UnknownStudioCommand(token_string, reader.line));
                }
            }
        }
    }

    if nodes.is_empty() {
        return Err(ParseSMDError::NoNodesSpecified);
    }

    if frames.is_empty() {
        return Err(ParseSMDError::NoFramesSpecified);
    }

    let bind_frame = &frames[0];
    let mut import_bones = IndexMap::with_capacity(nodes.len());
    let mut node_remap = IndexMap::with_capacity(nodes.len());
    for (id, node) in nodes {
        let bind_pose = bind_frame.get(&id).ok_or(ParseSMDError::MissingBoneBind(id))?;
        node_remap.insert(id, import_bones.len());

        import_bones.insert(
            node.name,
            super::Bone {
                parent: node.parent.map(|parent_id| *node_remap.get(&parent_id).unwrap()),
                location: bind_pose.position,
                rotation: bind_pose.rotation,
            },
        );
    }

    let mut animation = super::Animation {
        frame_count: NonZero::new(frames.len()).unwrap(),
        channels: IndexMap::with_capacity(import_bones.len()),
    };

    for (frame, keys) in frames.into_iter().enumerate() {
        for (node, key) in keys {
            let bone = *node_remap.get(&node).unwrap();
            let channel = animation.channels.entry(bone).or_default();
            channel.location.insert(frame, key.position);
            channel.rotation.insert(frame, key.rotation);
        }
    }
    animation.channels.sort_keys();

    let mut parts = IndexMap::new();

    if !triangles.is_empty() {
        let mut part = super::Part::default();

        for (material, vertices) in triangles {
            let polygon_list = part.polygons.entry(material).or_default();

            for triangle in vertices {
                let mut polygon = Vec::with_capacity(3);
                for vertex in triangle {
                    polygon.push(part.vertices.len());

                    part.vertices.push(super::Vertex {
                        location: vertex.position,
                        normal: vertex.normal,
                        texture_coordinate: vertex.texture_coordinate,
                        links: vertex.links,
                    });
                }
                polygon_list.push(polygon);
            }
        }

        parts.insert(file_name.clone(), part);
    }

    Ok(super::FileData {
        up: AxisDirection::PositiveZ,
        forward: AxisDirection::NegativeY,
        skeleton: import_bones,
        animations: IndexMap::from_iter([(file_name, animation)]),
        parts,
    })
}

struct FileReader<B: BufRead> {
    buffer: B,
    current_line: String,
    line: usize,
    column: usize,
}

enum ReadToken {
    Text(String),
    Quoted(String),
    Comment,
    LineEnd,
}

impl ReadToken {
    fn get_string(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Quoted(quote) => Some(quote),
            Self::Comment => None,
            Self::LineEnd => None,
        }
    }
}

impl<B: BufRead> FileReader<B> {
    fn new(buffer: B) -> Self {
        Self {
            buffer,
            current_line: String::new(),
            line: 0,
            column: 0,
        }
    }

    fn next_token(&mut self, greedy: bool) -> Result<Option<ReadToken>, ParseSMDError> {
        if self.current_line.len() == self.column {
            let new_line = match self.next_line()? {
                Some(line) => line,
                None => return Ok(None),
            };
            self.current_line = new_line;
            self.line += 1;
            self.column = 0;
            return Ok(Some(ReadToken::LineEnd));
        }

        let mut line_characters = self.current_line[self.column..].chars();
        let mut token = None;

        loop {
            let current_character = line_characters.next();
            self.column += 1;
            match current_character {
                Some(';') => {
                    if matches!(token, Some(ReadToken::Comment)) {
                        continue;
                    }

                    if let Some(ReadToken::Quoted(ref mut quote)) = token {
                        quote.push(';');
                        continue;
                    }

                    if let Some(ReadToken::Text(_)) = token {
                        self.column -= 1;
                        break;
                    }

                    token = Some(ReadToken::Comment);
                }
                Some('#') => {
                    if matches!(token, Some(ReadToken::Comment)) {
                        continue;
                    }

                    if let Some(ReadToken::Quoted(ref mut quote)) = token {
                        quote.push('#');
                        continue;
                    }

                    if let Some(ReadToken::Text(_)) = token {
                        self.column -= 1;
                        break;
                    }

                    token = Some(ReadToken::Comment);
                }
                Some('/') => {
                    if matches!(token, Some(ReadToken::Comment)) {
                        continue;
                    }

                    if let Some(ReadToken::Quoted(ref mut quote)) = token {
                        quote.push('/');
                        continue;
                    }

                    if let Some(ReadToken::Text(ref mut text)) = token {
                        if matches!(text.chars().last(), Some('/')) {
                            text.pop();
                            self.column -= 2;
                            break;
                        }
                        text.push('/');
                        continue;
                    }

                    token = Some(ReadToken::Comment);
                }
                Some('"') => {
                    if matches!(token, Some(ReadToken::Text(_))) {
                        return Err(ParseSMDError::InvalidQuoteDelimiter(self.line, self.column));
                    }

                    if matches!(token, Some(ReadToken::Quoted(_))) {
                        break;
                    }

                    token = Some(ReadToken::Quoted(String::with_capacity(32)));
                }
                Some(character) => {
                    if matches!(token, Some(ReadToken::Comment)) {
                        continue;
                    }

                    if let Some(ReadToken::Quoted(ref mut quote)) = token {
                        quote.push(character);
                        continue;
                    }

                    if let Some(ReadToken::Text(ref mut text)) = token {
                        if character.is_whitespace() && !greedy {
                            self.column -= 1;
                            break;
                        }

                        text.push(character);
                        continue;
                    }

                    if character.is_whitespace() && !greedy {
                        continue;
                    }

                    let mut text = String::with_capacity(32);
                    text.push(character);
                    token = Some(ReadToken::Text(text));
                }
                None => {
                    if matches!(token, Some(ReadToken::Quoted(_))) {
                        return Err(ParseSMDError::UnfinishedQuoteBlock(self.line, self.column));
                    }

                    if token.is_none() {
                        token = Some(ReadToken::LineEnd);
                    }

                    self.column -= 1;
                    break;
                }
            }
        }

        Ok(token)
    }

    fn next_line(&mut self) -> Result<Option<String>, ParseSMDError> {
        let mut line = String::new();
        let byte_count = self.buffer.read_line(&mut line)?;
        if byte_count == 0 {
            return Ok(None);
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Some(line))
    }
}
