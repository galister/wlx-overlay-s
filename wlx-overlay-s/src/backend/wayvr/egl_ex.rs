#![allow(clippy::all)]

pub const EGL_PLATFORM_WAYLAND_EXT: khronos_egl::Enum = 0x31D8;

// eglGetPlatformDisplayEXT
// https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_platform_base.txt
pub type PFNEGLGETPLATFORMDISPLAYEXTPROC = Option<
    unsafe extern "C" fn(
        platform: khronos_egl::Enum,
        native_display: *mut std::ffi::c_void,
        attrib_list: *mut khronos_egl::Enum,
    ) -> khronos_egl::EGLDisplay,
>;

// eglExportDMABUFImageMESA
// https://registry.khronos.org/EGL/extensions/MESA/EGL_MESA_image_dma_buf_export.txt
pub type PFNEGLEXPORTDMABUFIMAGEMESAPROC = Option<
    unsafe extern "C" fn(
        dpy: khronos_egl::EGLDisplay,
        image: khronos_egl::EGLImage,
        fds: *mut i32,
        strides: *mut khronos_egl::Int,
        offsets: *mut khronos_egl::Int,
    ) -> khronos_egl::Boolean,
>;

// eglQueryDmaBufModifiersEXT
// https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_image_dma_buf_import_modifiers.txt
pub type PFNEGLQUERYDMABUFMODIFIERSEXTPROC = Option<
    unsafe extern "C" fn(
        dpy: khronos_egl::EGLDisplay,
        format: khronos_egl::Int,
        max_modifiers: khronos_egl::Int,
        modifiers: *mut u64,
        external_only: *mut khronos_egl::Boolean,
        num_modifiers: *mut khronos_egl::Int,
    ) -> khronos_egl::Boolean,
>;

// eglQueryDmaBufFormatsEXT
// https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_image_dma_buf_import_modifiers.txt
pub type PFNEGLQUERYDMABUFFORMATSEXTPROC = Option<
    unsafe extern "C" fn(
        dpy: khronos_egl::EGLDisplay,
        max_formats: khronos_egl::Int,
        formats: *mut khronos_egl::Int,
        num_formats: *mut khronos_egl::Int,
    ) -> khronos_egl::Boolean,
>;
