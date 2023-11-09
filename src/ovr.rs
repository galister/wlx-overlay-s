use std::{path::Path, sync::Arc};

use vulkano::{
    image::{
        sys::{Image, RawImage},
        ImageViewAbstract,
    },
    Handle, VulkanObject,
};

use crate::graphics::WlxGraphics;

pub struct OpenVrState {
    pub context: ovr_overlay::Context,
}


pub struct OvrTextureData {
    image_handle: u64,
    device: u64,
    physical: u64,
    instance: u64,
    queue: u64,
    queue_family_index: u32,
    width: u32,
    height: u32,
    format: u32,
    sample_count: u32,
}

impl OvrTextureData {
    pub fn new(graphics: Arc<WlxGraphics>, view: Arc<dyn ImageViewAbstract>) -> OvrTextureData {
        let image = view.image();

        let device = graphics.device.handle().as_raw();
        let physical = graphics.device.physical_device().handle().as_raw();
        let instance = graphics.instance.handle().as_raw();
        let queue = graphics.queue.handle().as_raw();
        let queue_family_index = graphics.queue.queue_family_index();

        let (width, height) = {
            let dim = image.dimensions();
            (dim.width() as u32, dim.height() as u32)
        };

        let sample_count = image.samples() as u32;
        let format = image.format() as u32;

        let image_handle = image.inner().image.handle().as_raw();

        OvrTextureData {
            image_handle,
            device,
            physical,
            instance,
            queue,
            queue_family_index,
            width,
            height,
            format,
            sample_count,
        }
    }
}
