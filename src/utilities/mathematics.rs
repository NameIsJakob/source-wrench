pub type Matrix4 = glam::DAffine3;
pub const EULER_ROTATION: glam::EulerRot = glam::EulerRot::XYZEx;
pub type Quaternion = glam::DQuat;
pub type Vector2 = glam::DVec2;
pub type Vector3 = glam::DVec3;
pub type Vector4 = glam::DVec4;

#[derive(Clone, Copy, Debug, Default)]
pub struct BoundingBox {
    pub minimum: Vector3,
    pub maximum: Vector3,
}

impl BoundingBox {
    pub fn is_valid(&self) -> bool {
        self.minimum.x <= self.maximum.x && self.minimum.y <= self.maximum.y && self.minimum.z <= self.maximum.z
    }

    pub fn add_point(&mut self, point: Vector3) {
        if !self.is_valid() {
            self.minimum = point;
            self.maximum = point;
            return;
        }

        self.minimum.x = self.minimum.x.min(point.x);
        self.minimum.y = self.minimum.y.min(point.y);
        self.minimum.z = self.minimum.z.min(point.z);

        self.maximum.x = self.maximum.x.max(point.x);
        self.maximum.y = self.maximum.y.max(point.y);
        self.maximum.z = self.maximum.z.max(point.z);
    }

    pub fn center(&self) -> Vector3 {
        (self.minimum + self.maximum) * 0.5
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub enum AxisDirection {
    #[default]
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl AxisDirection {
    pub fn as_vector(self) -> Vector3 {
        match self {
            AxisDirection::PositiveX => Vector3::new(1.0, 0.0, 0.0),
            AxisDirection::NegativeX => Vector3::new(-1.0, 0.0, 0.0),
            AxisDirection::PositiveY => Vector3::new(0.0, 1.0, 0.0),
            AxisDirection::NegativeY => Vector3::new(0.0, -1.0, 0.0),
            AxisDirection::PositiveZ => Vector3::new(0.0, 0.0, 1.0),
            AxisDirection::NegativeZ => Vector3::new(0.0, 0.0, -1.0),
        }
    }

    pub fn is_parallel(self, other: Self) -> bool {
        self.as_vector().cross(other.as_vector()).length() < f64::EPSILON
    }
}

pub fn create_space_transform(up: AxisDirection, forward: AxisDirection) -> Matrix4 {
    debug_assert!(!forward.is_parallel(up));

    let forward_direction = forward.as_vector();
    let up_direction = up.as_vector();
    let left_direction = up_direction.cross(forward_direction);

    Matrix4::from_cols(forward_direction, left_direction, up_direction, Vector3::ZERO)
}
