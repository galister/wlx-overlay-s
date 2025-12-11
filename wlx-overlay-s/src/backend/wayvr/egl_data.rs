use std::sync::Arc;

use crate::backend::wayvr::egl_ex::{
    PFNEGLGETPLATFORMDISPLAYEXTPROC, PFNEGLQUERYDMABUFFORMATSEXTPROC,
    PFNEGLQUERYDMABUFMODIFIERSEXTPROC,
};

use super::egl_ex;
use anyhow::Context;

#[derive(Debug)]
pub struct EGLData {
    pub egl: khronos_egl::Instance<khronos_egl::Static>,
    pub display: khronos_egl::Display,
    pub config: khronos_egl::Config,
    pub context: khronos_egl::Context,
}

#[macro_export]
macro_rules! bind_egl_function {
    ($func_type:ident, $func:expr) => {
        std::mem::transmute_copy::<_, $func_type>($func).unwrap()
    };
}

#[derive(Debug, Clone)]
pub struct DMAbufModifierInfo {
    pub modifiers: Vec<u64>,
    pub fourcc: u32,
}

#[derive(Debug, Clone)]
pub struct RenderDMAbufData {
    pub fd: i32,
    pub stride: i32,
    pub offset: i32,
    pub mod_info: DMAbufModifierInfo,
}

#[derive(Debug, Clone)]
pub struct RenderSoftwarePixelsData {
    pub data: Arc<[u8]>,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone)]
pub enum RenderData {
    Dmabuf(RenderDMAbufData),
    Software(Option<RenderSoftwarePixelsData>), // will be set if the next image data is available
}

fn load_egl_func(
    egl: &khronos_egl::Instance<khronos_egl::Static>,
    func_name: &str,
) -> anyhow::Result<extern "system" fn()> {
    let raw_fn = egl
        .get_proc_address(func_name)
        .ok_or_else(|| anyhow::anyhow!("Required EGL function {func_name} not found"))?;
    Ok(raw_fn)
}

fn get_disp(
    egl: &khronos_egl::Instance<khronos_egl::Static>,
) -> anyhow::Result<khronos_egl::Display> {
    unsafe {
        if let Ok(func) = load_egl_func(egl, "eglGetPlatformDisplayEXT") {
            let egl_get_platform_display_ext =
                bind_egl_function!(PFNEGLGETPLATFORMDISPLAYEXTPROC, &func);

            let display_ext = egl_get_platform_display_ext(
                egl_ex::EGL_PLATFORM_WAYLAND_EXT, // platform
                std::ptr::null_mut(),             // void *native_display
                std::ptr::null_mut(),             // EGLint *attrib_list
            );

            if display_ext.is_null() {
                log::warn!("eglGetPlatformDisplayEXT failed, using eglGetDisplay instead");
            } else {
                return Ok(khronos_egl::Display::from_ptr(display_ext));
            }
        }

        egl
            .get_display(khronos_egl::DEFAULT_DISPLAY)
            .context(
                "Both eglGetPlatformDisplayEXT and eglGetDisplay failed. This shouldn't happen unless you don't have any display manager running. Cannot continue, check your EGL installation."
            )
    }
}

impl EGLData {
    pub fn new() -> anyhow::Result<Self> {
        let egl = khronos_egl::Instance::new(khronos_egl::Static);
        let display = get_disp(&egl)?;

        let (major, minor) = egl.initialize(display)?;
        log::debug!("EGL version: {major}.{minor}");

        let attrib_list = [
            khronos_egl::RED_SIZE,
            8,
            khronos_egl::GREEN_SIZE,
            8,
            khronos_egl::BLUE_SIZE,
            8,
            khronos_egl::SURFACE_TYPE,
            khronos_egl::WINDOW_BIT,
            khronos_egl::RENDERABLE_TYPE,
            khronos_egl::OPENGL_BIT,
            khronos_egl::NONE,
        ];

        let config = egl
            .choose_first_config(display, &attrib_list)?
            .context("Failed to get EGL config")?;

        egl.bind_api(khronos_egl::OPENGL_ES_API)?;

        log::debug!("eglCreateContext");

        // Require OpenGL ES 3.0
        let context_attrib_list = [
            khronos_egl::CONTEXT_MAJOR_VERSION,
            3,
            khronos_egl::CONTEXT_MINOR_VERSION,
            0,
            khronos_egl::NONE,
        ];

        let context = egl.create_context(display, config, None, &context_attrib_list)?;

        log::debug!("eglMakeCurrent");

        egl.make_current(display, None, None, Some(context))?;

        Ok(Self {
            egl,
            display,
            config,
            context,
        })
    }

    fn query_dmabuf_mod_info(&self) -> anyhow::Result<DMAbufModifierInfo> {
        let target_fourcc = 0x3432_4258; //XB24

        unsafe {
            let egl_query_dmabuf_formats_ext = bind_egl_function!(
                PFNEGLQUERYDMABUFFORMATSEXTPROC,
                &load_egl_func(&self.egl, "eglQueryDmaBufFormatsEXT")?
            );

            // Query format count
            let mut num_formats: khronos_egl::Int = 0;
            egl_query_dmabuf_formats_ext(
                self.display.as_ptr(),
                0,
                std::ptr::null_mut(),
                &raw mut num_formats,
            );

            // Retrieve format list
            let mut formats: Vec<i32> = vec![0; num_formats as usize];
            egl_query_dmabuf_formats_ext(
                self.display.as_ptr(),
                num_formats,
                formats.as_mut_ptr(),
                &raw mut num_formats,
            );

            /*for (idx, format) in formats.iter().enumerate() {
                let bytes = format.to_le_bytes();
                log::trace!(
                    "idx {}, format {}{}{}{} (hex {:#x})",
                    idx,
                    bytes[0] as char,
                    bytes[1] as char,
                    bytes[2] as char,
                    bytes[3] as char,
                    format
                );
            }*/

            let egl_query_dmabuf_modifiers_ext = bind_egl_function!(
                PFNEGLQUERYDMABUFMODIFIERSEXTPROC,
                &load_egl_func(&self.egl, "eglQueryDmaBufModifiersEXT")?
            );

            let mut num_mods: khronos_egl::Int = 0;

            // Query modifier count
            egl_query_dmabuf_modifiers_ext(
                self.display.as_ptr(),
                target_fourcc,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut num_mods,
            );

            if num_mods == 0 {
                anyhow::bail!("eglQueryDmaBufModifiersEXT modifier count is zero");
            }

            let mut mods: Vec<u64> = vec![0; num_mods as usize];
            egl_query_dmabuf_modifiers_ext(
                self.display.as_ptr(),
                target_fourcc,
                num_mods,
                mods.as_mut_ptr(),
                std::ptr::null_mut(),
                &raw mut num_mods,
            );

            if mods[0] == 0xFFFF_FFFF_FFFF_FFFF {
                anyhow::bail!("modifier is -1")
            }

            log::trace!("Modifier list:");
            for modifier in &mods {
                log::trace!("{modifier:#x}");
            }

            // We should not change these modifier values. Passing all of them to the Vulkan dmabuf
            // texture system causes significant graphical corruption due to invalid memory layout and
            // tiling on this specific GPU model (very probably others also have the same issue).
            // It is not guaranteed that this modifier will be present in other models.
            // If not, the full list of modifiers will be passed. Further testing is required.
            // For now, it looks like only NAVI32-based gpus have this problem.
            let mod_whitelist: [u64; 2] = [
                0x200_0000_2086_bf04, /* AMD RX 7800 XT, Navi32 */
                0x200_0000_1866_bf04, /* AMD RX 7600 XT, Navi33 */
            ];

            for modifier in &mod_whitelist {
                if mods.contains(modifier) {
                    log::warn!("Using whitelisted dmabuf tiling modifier: {modifier:#x}");
                    mods = vec![*modifier, 0x0 /* also important (???) */];
                    break;
                }
            }

            Ok(DMAbufModifierInfo {
                modifiers: mods,
                fourcc: target_fourcc as u32,
            })
        }
    }

    pub fn create_dmabuf_data(
        &self,
        egl_image: &khronos_egl::Image,
    ) -> anyhow::Result<RenderDMAbufData> {
        use egl_ex::PFNEGLEXPORTDMABUFIMAGEMESAPROC as FUNC;
        unsafe {
            let egl_export_dmabuf_image_mesa =
                bind_egl_function!(FUNC, &load_egl_func(&self.egl, "eglExportDMABUFImageMESA")?);

            let mut fds: [i32; 3] = [0; 3];
            let mut strides: [i32; 3] = [0; 3];
            let mut offsets: [i32; 3] = [0; 3];

            let ret = egl_export_dmabuf_image_mesa(
                self.display.as_ptr(),
                egl_image.as_ptr(),
                fds.as_mut_ptr(),
                strides.as_mut_ptr(),
                offsets.as_mut_ptr(),
            );

            if ret != khronos_egl::TRUE {
                anyhow::bail!("eglExportDMABUFImageMESA failed with return code {ret}");
            }

            if fds[0] <= 0 {
                anyhow::bail!("fd is <=0 (got {})", fds[0]);
            }

            // many planes in RGB data?
            if fds[1] != 0 || strides[1] != 0 || offsets[1] != 0 {
                anyhow::bail!("multi-planar data received, packed RGB expected");
            }

            if strides[0] < 0 {
                anyhow::bail!("strides is < 0");
            }

            if offsets[0] < 0 {
                anyhow::bail!("offsets is < 0");
            }

            let mod_info = self.query_dmabuf_mod_info()?;

            Ok(RenderDMAbufData {
                fd: fds[0],
                stride: strides[0],
                offset: offsets[0],
                mod_info,
            })
        }
    }

    pub fn create_egl_image(&self, gl_tex_id: u32) -> anyhow::Result<khronos_egl::Image> {
        unsafe {
            Ok(self.egl.create_image(
                self.display,
                self.context,
                khronos_egl::GL_TEXTURE_2D as std::ffi::c_uint,
                khronos_egl::ClientBuffer::from_ptr(gl_tex_id as *mut std::ffi::c_void),
                &[khronos_egl::ATTRIB_NONE],
            )?)
        }
    }
}
