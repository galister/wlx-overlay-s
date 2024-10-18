//eglExportDMABUFImageMESA
pub type PFNEGLEXPORTDMABUFIMAGEMESAPROC = Option<
	unsafe extern "C" fn(
		dpy: khronos_egl::EGLDisplay,
		image: khronos_egl::EGLImage,
		fds: *mut i32,
		strides: *mut khronos_egl::Int,
		offsets: *mut khronos_egl::Int,
	) -> khronos_egl::Boolean,
>;

//eglQueryDmaBufModifiersEXT
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

//eglQueryDmaBufFormatsEXT
pub type PFNEGLQUERYDMABUFFORMATSEXTPROC = Option<
	unsafe extern "C" fn(
		dpy: khronos_egl::EGLDisplay,
		max_formats: khronos_egl::Int,
		formats: *mut khronos_egl::Int,
		num_formats: *mut khronos_egl::Int,
	) -> khronos_egl::Boolean,
>;
