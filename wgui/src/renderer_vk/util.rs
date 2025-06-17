use vulkano::buffer::BufferContents;

// binary compatible mat4 which could be transparently used by vulkano BufferContents
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, BufferContents)]
pub struct WMat4(pub [f32; 16]);

impl WMat4 {
	pub fn from_glam(mat: &glam::Mat4) -> WMat4 {
		WMat4(*mat.as_ref())
	}
}

impl Default for WMat4 {
	fn default() -> Self {
		Self(*glam::Mat4::IDENTITY.as_ref())
	}
}
