use super::egl_data;
use smithay::backend::{egl as smithay_egl, renderer::gles::ffi};

pub fn get_egl_display(data: &egl_data::EGLData) -> anyhow::Result<smithay_egl::EGLDisplay> {
    Ok(unsafe { smithay_egl::EGLDisplay::from_raw(data.display.as_ptr(), data.config.as_ptr())? })
}

pub fn get_egl_context(
    data: &egl_data::EGLData,
    display: &smithay_egl::EGLDisplay,
) -> anyhow::Result<smithay_egl::EGLContext> {
    let display_ptr = display.get_display_handle().handle;
    debug_assert!(display_ptr == data.display.as_ptr());
    let config_ptr = data.config.as_ptr();
    let context_ptr = data.context.as_ptr();
    Ok(unsafe { smithay_egl::EGLContext::from_raw(display_ptr, config_ptr, context_ptr)? })
}

pub fn create_framebuffer_texture(
    gl: &ffi::Gles2,
    width: u32,
    height: u32,
    tex_format: u32,
    internal_format: u32,
) -> u32 {
    unsafe {
        let mut tex = 0;
        gl.GenTextures(1, &mut tex);
        gl.BindTexture(ffi::TEXTURE_2D, tex);
        gl.TexParameteri(
            ffi::TEXTURE_2D,
            ffi::TEXTURE_MIN_FILTER,
            ffi::NEAREST as i32,
        );
        gl.TexParameteri(
            ffi::TEXTURE_2D,
            ffi::TEXTURE_MAG_FILTER,
            ffi::NEAREST as i32,
        );
        gl.TexImage2D(
            ffi::TEXTURE_2D,
            0,
            internal_format as i32,
            width as i32,
            height as i32,
            0,
            tex_format,
            ffi::UNSIGNED_BYTE,
            std::ptr::null(),
        );
        gl.BindTexture(ffi::TEXTURE_2D, 0);
        tex
    }
}
