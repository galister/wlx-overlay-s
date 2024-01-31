use glam::Affine3A;
use ovr_overlay::{pose::Matrix3x4, sys::HmdMatrix34_t};

pub trait Affine3AConvert {
    fn from_affine(affine: Affine3A) -> Self;
    fn to_affine(&self) -> Affine3A;
}

impl Affine3AConvert for Matrix3x4 {
    fn from_affine(affine: Affine3A) -> Self {
        Matrix3x4([
            [
                affine.matrix3.x_axis.x,
                affine.matrix3.y_axis.x,
                affine.matrix3.z_axis.x,
                affine.translation.x,
            ],
            [
                affine.matrix3.x_axis.y,
                affine.matrix3.y_axis.y,
                affine.matrix3.z_axis.y,
                affine.translation.y,
            ],
            [
                affine.matrix3.x_axis.z,
                affine.matrix3.y_axis.z,
                affine.matrix3.z_axis.z,
                affine.translation.z,
            ],
        ])
    }

    fn to_affine(&self) -> Affine3A {
        Affine3A::from_cols_array_2d(&[
            [self.0[0][0], self.0[1][0], self.0[2][0]],
            [self.0[0][1], self.0[1][1], self.0[2][1]],
            [self.0[0][2], self.0[1][2], self.0[2][2]],
            [self.0[0][3], self.0[1][3], self.0[2][3]],
        ])
    }
}

impl Affine3AConvert for HmdMatrix34_t {
    fn from_affine(affine: Affine3A) -> Self {
        HmdMatrix34_t {
            m: [
                [
                    affine.matrix3.x_axis.x,
                    affine.matrix3.y_axis.x,
                    affine.matrix3.z_axis.x,
                    affine.translation.x,
                ],
                [
                    affine.matrix3.x_axis.y,
                    affine.matrix3.y_axis.y,
                    affine.matrix3.z_axis.y,
                    affine.translation.y,
                ],
                [
                    affine.matrix3.x_axis.z,
                    affine.matrix3.y_axis.z,
                    affine.matrix3.z_axis.z,
                    affine.translation.z,
                ],
            ],
        }
    }

    fn to_affine(&self) -> Affine3A {
        Affine3A::from_cols_array_2d(&[
            [self.m[0][0], self.m[1][0], self.m[2][0]],
            [self.m[0][1], self.m[1][1], self.m[2][1]],
            [self.m[0][2], self.m[1][2], self.m[2][2]],
            [self.m[0][3], self.m[1][3], self.m[2][3]],
        ])
    }
}
