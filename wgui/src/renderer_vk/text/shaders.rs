pub mod vert_atlas {
	vulkano_shaders::shader! {
			ty: "vertex",
			path: "src/renderer_vk/shaders/text.vert",
	}
}

pub mod frag_atlas {
	vulkano_shaders::shader! {
			ty: "fragment",
			path: "src/renderer_vk/shaders/text.frag",
	}
}
