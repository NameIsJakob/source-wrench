use indexmap::{IndexMap, map::Entry};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    num::NonZero,
};
use thiserror::Error as ThisError;

use crate::{
    utilities::mathematics::{AxisDirection, Vector2, Vector3},
    warn,
};

#[derive(Debug, ThisError)]
pub enum ParseOBJError {
    #[error("Unknown Keyword On Line {0}")]
    UnknownKeyword(usize),
    #[error("Failed To Parse Integer On Line {0}")]
    FailedIntegerParse(usize),
    #[error("Failed To Parse Float On Line {0}")]
    FailedFloatParse(usize),
    #[error("Missing {0} Argument On Line {1}")]
    MissingArgument(&'static str, usize),
    #[error("Index Out Of Bounds On Line {0}")]
    BogusIndex(usize),
    #[error("Duplicate Object Names")]
    DuplicateObjects,
}

pub fn load_obj(file_buffer: BufReader<File>, file_name: String) -> Result<super::FileData, ParseOBJError> {
    let lines = file_buffer.lines().map_while(Result::ok);

    let mut file_data = super::FileData {
        up: AxisDirection::PositiveZ,
        forward: AxisDirection::PositiveX,
        skeleton: IndexMap::from([(String::from("default"), super::Bone::default())]),
        animations: IndexMap::from([(
            file_name,
            super::Animation {
                frame_count: NonZero::new(1).unwrap(),
                channels: IndexMap::new(),
            },
        )]),
        ..Default::default()
    };

    let mut vertex_data = Vec::new();
    let mut texture_coordinate_data = Vec::new();
    let mut normal_data = Vec::new();
    let mut object_name = String::new();
    let mut object_data = super::Part::default();
    let mut current_material = String::from("debug/debugempty");
    let mut warned_no_material = false;

    for (current_line_count, current_line) in lines.enumerate() {
        let mut line_arguments = current_line.split_whitespace();
        let command = line_arguments.next();

        match command {
            Some("v") => {
                let x_position = match line_arguments.next() {
                    Some(x_position) => match x_position.parse::<f64>() {
                        Ok(x_position) => x_position,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("X Position", current_line_count)),
                };

                let y_position = match line_arguments.next() {
                    Some(y_position) => match y_position.parse::<f64>() {
                        Ok(y_position) => y_position,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("Y Position", current_line_count)),
                };

                let z_position = match line_arguments.next() {
                    Some(z_position) => match z_position.parse::<f64>() {
                        Ok(z_position) => z_position,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("Z Position", current_line_count)),
                };

                vertex_data.push(Vector3::new(x_position, y_position, z_position));
            }
            Some("vt") => {
                let u_texture_coordinate = match line_arguments.next() {
                    Some(u_texture_coordinate) => match u_texture_coordinate.parse::<f64>() {
                        Ok(u_texture_coordinate) => u_texture_coordinate,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("U Texture Coordinate", current_line_count)),
                };

                let v_texture_coordinate = match line_arguments.next() {
                    Some(v_texture_coordinate) => match v_texture_coordinate.parse::<f64>() {
                        Ok(v_texture_coordinate) => v_texture_coordinate,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("V Texture Coordinate", current_line_count)),
                };

                texture_coordinate_data.push(Vector2::new(u_texture_coordinate, v_texture_coordinate));
            }
            Some("vn") => {
                let x_normal = match line_arguments.next() {
                    Some(x_normal) => match x_normal.parse::<f64>() {
                        Ok(x_normal) => x_normal,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("X Normal", current_line_count)),
                };

                let y_normal = match line_arguments.next() {
                    Some(y_normal) => match y_normal.parse::<f64>() {
                        Ok(y_normal) => y_normal,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("Y Normal", current_line_count)),
                };

                let z_normal = match line_arguments.next() {
                    Some(z_normal) => match z_normal.parse::<f64>() {
                        Ok(z_normal) => z_normal,
                        Err(_) => return Err(ParseOBJError::FailedFloatParse(current_line_count)),
                    },
                    None => return Err(ParseOBJError::MissingArgument("Z Normal", current_line_count)),
                };

                normal_data.push(Vector3::new(x_normal, y_normal, z_normal));
            }
            Some("f") => {
                let mut points = Vec::new();

                for point in line_arguments {
                    if point == "#" {
                        continue;
                    }

                    let mut point_arguments = point.split('/');

                    let vertex_index = match point_arguments.next() {
                        Some(vertex_index) => match vertex_index.parse::<usize>() {
                            Ok(vertex_index) => vertex_index,
                            Err(_) => return Err(ParseOBJError::FailedIntegerParse(current_line_count)),
                        },
                        None => return Err(ParseOBJError::MissingArgument("Vertex Index", current_line_count)),
                    };

                    if vertex_index > vertex_data.len() {
                        return Err(ParseOBJError::BogusIndex(current_line_count));
                    }

                    let texture_coordinate_index = match point_arguments.next() {
                        Some(texture_coordinate_index) => match texture_coordinate_index.parse::<usize>() {
                            Ok(texture_coordinate_index) => texture_coordinate_index,
                            Err(_) => return Err(ParseOBJError::FailedIntegerParse(current_line_count)),
                        },
                        None => return Err(ParseOBJError::MissingArgument("Texture Coordinate Index", current_line_count)),
                    };

                    if texture_coordinate_index > texture_coordinate_data.len() {
                        return Err(ParseOBJError::BogusIndex(current_line_count));
                    }

                    let normal_index = match point_arguments.next() {
                        Some(normal_index) => match normal_index.parse::<usize>() {
                            Ok(normal_index) => normal_index,
                            Err(_) => return Err(ParseOBJError::FailedIntegerParse(current_line_count)),
                        },
                        None => return Err(ParseOBJError::MissingArgument("Normal Index", current_line_count)),
                    };

                    if normal_index > normal_data.len() {
                        return Err(ParseOBJError::BogusIndex(current_line_count));
                    }

                    points.push(object_data.vertices.len());
                    object_data.vertices.push(super::Vertex {
                        location: vertex_data[vertex_index - 1],
                        normal: normal_data[normal_index - 1],
                        texture_coordinate: texture_coordinate_data[texture_coordinate_index - 1],
                        links: IndexMap::from([(0, 1.0)]),
                    });
                }

                if points.len() < 3 {
                    continue;
                }

                if current_material == "debug/debugempty" && !warned_no_material {
                    warn!(
                        "Object {} faces has no materials! Defaulting to {}!",
                        if object_name.is_empty() { "Object" } else { &object_name },
                        &current_material
                    );
                    warned_no_material = true;
                }

                let current_material = match object_data.polygons.entry(current_material.clone()) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => entry.insert(Vec::new()),
                };

                current_material.push(points);
            }
            Some("g") => {
                vertex_data.clear();
                texture_coordinate_data.clear();
                normal_data.clear();
            }
            Some("o") => {
                if object_name.is_empty() {
                    object_name = match line_arguments.next() {
                        Some(name) => name.to_string(),
                        None => return Err(ParseOBJError::MissingArgument("Object Name", current_line_count)),
                    };
                    continue;
                }

                if file_data.parts.contains_key(&object_name) {
                    return Err(ParseOBJError::DuplicateObjects);
                }

                file_data.parts.insert(object_name, object_data);
                object_name = match line_arguments.next() {
                    Some(name) => name.to_string(),
                    None => return Err(ParseOBJError::MissingArgument("Object Name", current_line_count)),
                };
                object_data = super::Part::default();

                current_material = String::from("debug/debugempty");
                warned_no_material = false;
            }
            Some("usemtl") => {
                current_material = match line_arguments.next() {
                    Some(current_material) => current_material.to_string(),
                    None => return Err(ParseOBJError::MissingArgument("Material Name", current_line_count)),
                };
            }
            Some("#") => continue,
            Some("vp") => continue,
            Some("cstype") => continue,
            Some("deg") => continue,
            Some("bmat") => continue,
            Some("step") => continue,
            Some("p") => continue,
            Some("l") => continue,
            Some("curv") => continue,
            Some("curv2") => continue,
            Some("surf") => continue,
            Some("parm") => continue,
            Some("trim") => continue,
            Some("hole") => continue,
            Some("scrv") => continue,
            Some("sp") => continue,
            Some("end") => continue,
            Some("con") => continue,
            Some("s") => continue,
            Some("mg") => continue,
            Some("bevel") => continue,
            Some("c_interp") => continue,
            Some("d_interp") => continue,
            Some("lod") => continue,
            Some("mtllib") => continue,
            Some("shadow_obj") => continue,
            Some("trace_obj") => continue,
            Some("ctech") => continue,
            Some("stech") => continue,
            Some(_) => return Err(ParseOBJError::UnknownKeyword(current_line_count)),
            None => continue,
        }
    }

    if object_name.is_empty() && file_data.parts.contains_key("Object") {
        return Err(ParseOBJError::DuplicateObjects);
    }

    file_data
        .parts
        .insert(if object_name.is_empty() { String::from("Object") } else { object_name }, object_data);

    Ok(file_data)
}
