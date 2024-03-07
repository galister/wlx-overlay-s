use std::ffi::CStr;

use glam::Affine3A;
use ovr_overlay::{pose::Matrix3x4, settings::SettingsManager, sys::HmdMatrix34_t};
use thiserror::Error;

use crate::backend::common::{BackendError, ColorChannel};

pub trait Affine3AConvert {
    fn from_affine(affine: &Affine3A) -> Self;
    fn to_affine(&self) -> Affine3A;
}

impl Affine3AConvert for Matrix3x4 {
    fn from_affine(affine: &Affine3A) -> Self {
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
    fn from_affine(affine: &Affine3A) -> Self {
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

#[derive(Error, Debug)]
pub(super) enum OVRError {
    #[error("ovr input error: {0}")]
    InputError(&'static str),
}

impl From<ovr_overlay::errors::EVRInputError> for OVRError {
    fn from(e: ovr_overlay::errors::EVRInputError) -> Self {
        OVRError::InputError(e.description())
    }
}

impl From<OVRError> for BackendError {
    fn from(e: OVRError) -> Self {
        BackendError::Fatal(anyhow::Error::new(e))
    }
}

use cstr::cstr;
const STEAMVR_SECTION: &CStr = cstr!("steamvr");
const COLOR_GAIN_CSTR: [&CStr; 3] = [
    cstr!("hmdDisplayColorGainR"),
    cstr!("hmdDisplayColorGainG"),
    cstr!("hmdDisplayColorGainB"),
];

pub(super) fn adjust_gain(
    settings: &mut SettingsManager,
    ch: ColorChannel,
    delta: f32,
) -> Option<()> {
    let current = [
        settings
            .get_float(STEAMVR_SECTION, COLOR_GAIN_CSTR[0])
            .ok()?,
        settings
            .get_float(STEAMVR_SECTION, COLOR_GAIN_CSTR[1])
            .ok()?,
        settings
            .get_float(STEAMVR_SECTION, COLOR_GAIN_CSTR[2])
            .ok()?,
    ];

    // prevent user from turning everything black
    let mut min = if current[0] + current[1] + current[2] < 0.11 {
        0.1
    } else {
        0.0
    };

    match ch {
        ColorChannel::R => {
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[0],
                    (current[0] + delta).clamp(min, 1.0),
                )
                .ok()?;
        }
        ColorChannel::G => {
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[1],
                    (current[1] + delta).clamp(min, 1.0),
                )
                .ok()?;
        }
        ColorChannel::B => {
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[2],
                    (current[2] + delta).clamp(min, 1.0),
                )
                .ok()?;
        }
        ColorChannel::All => {
            min *= 0.3333;
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[0],
                    (current[0] + delta).clamp(min, 1.0),
                )
                .ok()?;
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[1],
                    (current[1] + delta).clamp(min, 1.0),
                )
                .ok()?;
            settings
                .set_float(
                    STEAMVR_SECTION,
                    COLOR_GAIN_CSTR[2],
                    (current[2] + delta).clamp(min, 1.0),
                )
                .ok()?;
        }
    }

    Some(())
}
