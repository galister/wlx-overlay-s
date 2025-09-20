use std::{fmt::Display, os::fd::RawFd};

#[derive(Debug, Clone, Copy, Default)]
pub struct FourCC {
    pub value: u32,
}

impl PartialEq for FourCC {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl From<u32> for FourCC {
    fn from(value: u32) -> Self {
        Self { value }
    }
}

impl From<FourCC> for u32 {
    fn from(fourcc: FourCC) -> Self {
        fourcc.value
    }
}

impl Display for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 0..4 {
            if let Some(c) = char::from_u32((self.value >> (i * 8)) & 0xFF) {
                write!(f, "{c}")?
            } else {
                write!(f, "?")?
            }
        }
        Ok(())
    }
}

pub const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // AR24
pub const DRM_FORMAT_ABGR8888: u32 = 0x34324241; // AB24
pub const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // XR24
pub const DRM_FORMAT_XBGR8888: u32 = 0x34324258; // XB24
pub const DRM_FORMAT_ABGR2101010: u32 = 0x30334241; // AB30
pub const DRM_FORMAT_XBGR2101010: u32 = 0x30334258; // XB30

#[cfg(feature = "egl")]
#[rustfmt::skip]
const EGL_DMABUF_PLANE_ATTRS: [isize; 20] = [
//  FD     Offset Stride ModLo  ModHi
    0x3272,0x3273,0x3274,0x3443,0x3444,
    0x3275,0x3276,0x3277,0x3445,0x3446,
    0x3278,0x3279,0x327A,0x3447,0x3448,
    0x3440,0x3441,0x3442,0x3449,0x344A,
];

pub enum WlxFrame {
    Dmabuf(DmabufFrame),
    MemFd(MemFdFrame),
    MemPtr(MemPtrFrame),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Transform {
    #[default]
    Undefined,
    Normal,
    Rotated90,
    Rotated180,
    Rotated270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameFormat {
    pub width: u32,
    pub height: u32,
    pub fourcc: FourCC,
    pub modifier: u64,
    pub transform: Transform,
}

impl FrameFormat {
    pub fn get_mod_hi(&self) -> u32 {
        (self.modifier >> 32) as _
    }
    pub fn get_mod_lo(&self) -> u32 {
        (self.modifier & 0xFFFFFFFF) as _
    }
    pub fn set_mod(&mut self, mod_hi: u32, mod_low: u32) {
        self.modifier = ((mod_hi as u64) << 32) + mod_low as u64;
    }
}

#[derive(Clone, Copy, Default)]
pub struct FramePlane {
    pub fd: Option<RawFd>,
    pub offset: u32,
    pub stride: i32,
}

#[derive(Default, Clone)]
pub struct DrmFormat {
    pub fourcc: FourCC,
    pub modifiers: Vec<u64>,
}

#[derive(Default)]
pub struct DmabufFrame {
    pub format: FrameFormat,
    pub num_planes: usize,
    pub planes: [FramePlane; 4],
    pub mouse: Option<MouseMeta>,
}

impl DmabufFrame {
    #[cfg(feature = "egl")]
    /// Get the attributes for creating an EGLImage.
    /// Pacics if fd is None; check using `is_valid` first.
    pub fn get_egl_image_attribs(&self) -> Vec<isize> {
        let mut vec: Vec<isize> = vec![
            0x3057, // WIDTH
            self.format.width as _,
            0x3056, // HEIGHT
            self.format.height as _,
            0x3271, // LINUX_DRM_FOURCC_EXT,
            self.format.fourcc.value as _,
        ];

        for i in 0..self.num_planes {
            let mut a = i * 5usize;
            vec.push(EGL_DMABUF_PLANE_ATTRS[a]);
            vec.push(self.planes[i].fd.unwrap() as _); // safe to unwrap due to contract
            a += 1;
            vec.push(EGL_DMABUF_PLANE_ATTRS[a]);
            vec.push(self.planes[i].offset as _);
            a += 1;
            vec.push(EGL_DMABUF_PLANE_ATTRS[a]);
            vec.push(self.planes[i].stride as _);
            a += 1;
            vec.push(EGL_DMABUF_PLANE_ATTRS[a]);
            vec.push(self.format.get_mod_lo() as _);
            a += 1;
            vec.push(EGL_DMABUF_PLANE_ATTRS[a]);
            vec.push(self.format.get_mod_hi() as _);
        }
        vec.push(0x3038); // NONE

        vec
    }

    /// Returns true if all planes have a valid file descriptor.
    pub fn is_valid(&self) -> bool {
        for i in 0..self.num_planes {
            if self.planes[i].fd.is_none() {
                return false;
            }
        }
        true
    }
}

#[derive(Default)]
pub struct MemFdFrame {
    pub format: FrameFormat,
    pub plane: FramePlane,
    pub mouse: Option<MouseMeta>,
}

#[derive(Default)]
pub struct MemPtrFrame {
    pub format: FrameFormat,
    pub ptr: usize,
    pub size: usize,
    pub mouse: Option<MouseMeta>,
}

#[derive(Default, Clone)]
pub struct MouseMeta {
    pub x: f32,
    pub y: f32,
}
