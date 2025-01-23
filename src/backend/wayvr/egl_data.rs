use super::egl_ex;
use anyhow::anyhow;

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
pub struct DMAbufData {
    pub fd: i32,
    pub stride: i32,
    pub offset: i32,
    pub mod_info: DMAbufModifierInfo,
}

impl EGLData {
    pub fn load_func(&self, func_name: &str) -> anyhow::Result<extern "system" fn()> {
        let raw_fn = self.egl.get_proc_address(func_name).ok_or(anyhow::anyhow!(
            "Required EGL function {} not found",
            func_name
        ))?;
        Ok(raw_fn)
    }

    pub fn new() -> anyhow::Result<EGLData> {
        unsafe {
            let egl = khronos_egl::Instance::new(khronos_egl::Static);

            let display = egl
                .get_display(khronos_egl::DEFAULT_DISPLAY)
                .ok_or(anyhow!(
                    "eglGetDisplay failed. This shouldn't happen unless you don't have any display manager running. Cannot continue, check your EGL installation."
                ))?;

            let (major, minor) = egl.initialize(display)?;
            log::debug!("EGL version: {}.{}", major, minor);

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
                .ok_or(anyhow!("Failed to get EGL config"))?;

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

            Ok(EGLData {
                egl,
                display,
                config,
                context,
            })
        }
    }

    fn query_dmabuf_mod_info(&self) -> anyhow::Result<DMAbufModifierInfo> {
        let target_fourcc = 0x34324258; //XB24

        unsafe {
            use egl_ex::PFNEGLQUERYDMABUFFORMATSEXTPROC;
            use egl_ex::PFNEGLQUERYDMABUFMODIFIERSEXTPROC;

            let egl_query_dmabuf_formats_ext = bind_egl_function!(
                PFNEGLQUERYDMABUFFORMATSEXTPROC,
                &self.load_func("eglQueryDmaBufFormatsEXT")?
            );

            // Query format count
            let mut num_formats: khronos_egl::Int = 0;
            egl_query_dmabuf_formats_ext(
                self.display.as_ptr(),
                0,
                std::ptr::null_mut(),
                &mut num_formats,
            );

            // Retrieve formt list
            let mut formats: Vec<i32> = vec![0; num_formats as usize];
            egl_query_dmabuf_formats_ext(
                self.display.as_ptr(),
                num_formats,
                formats.as_mut_ptr(),
                &mut num_formats,
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
                &self.load_func("eglQueryDmaBufModifiersEXT")?
            );

            let mut num_mods: khronos_egl::Int = 0;

            // Query modifier count
            egl_query_dmabuf_modifiers_ext(
                self.display.as_ptr(),
                target_fourcc,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut num_mods,
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
                &mut num_mods,
            );

            if mods[0] == 0xFFFFFFFFFFFFFFFF {
                anyhow::bail!("modifier is -1")
            }

            log::trace!("Modifier list:");
            for modifier in &mods {
                log::trace!("{:#x}", modifier);
            }

            // We should not change these modifier values. Passing all of them to the Vulkan dmabuf
            // texture system causes significant graphical corruption due to invalid memory layout and
            // tiling on this specific GPU model (very probably others also have the same issue).
            // It is not guaranteed that this modifier will be present in other models.
            // If not, the full list of modifiers will be passed. Further testing is required.
            // For now, it looks like only NAVI32-based gpus have this problem.
            let mod_whitelist: [u64; 2] = [
                0x20000002086bf04, /* AMD RX 7800 XT, Navi32 */
                0x20000001866bf04, /* AMD RX 7600 XT, Navi33 */
            ];

            for modifier in &mod_whitelist {
                if mods.contains(modifier) {
                    log::warn!("Using whitelisted dmabuf tiling modifier: {:#x}", modifier);
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

    pub fn create_dmabuf_data(&self, egl_image: &khronos_egl::Image) -> anyhow::Result<DMAbufData> {
        use egl_ex::PFNEGLEXPORTDMABUFIMAGEMESAPROC as FUNC;
        unsafe {
            let egl_export_dmabuf_image_mesa =
                bind_egl_function!(FUNC, &self.load_func("eglExportDMABUFImageMESA")?);

            let mut fds: [i32; 3] = [0; 3];
            let mut strides: [i32; 3] = [0; 3];
            let mut offsets: [i32; 3] = [0; 3];

            if egl_export_dmabuf_image_mesa(
                self.display.as_ptr(),
                egl_image.as_ptr(),
                fds.as_mut_ptr(),
                strides.as_mut_ptr(),
                offsets.as_mut_ptr(),
            ) != khronos_egl::TRUE
            {
                anyhow::bail!("eglExportDMABUFImageMESA failed");
            }

            // many planes in RGB data?
            debug_assert!(fds[1] == 0);
            debug_assert!(strides[1] == 0);
            debug_assert!(offsets[1] == 0);

            let mod_info = self.query_dmabuf_mod_info()?;

            Ok(DMAbufData {
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
