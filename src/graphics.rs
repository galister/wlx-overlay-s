use std::{error::Error, io::Cursor, slice::Iter, sync::Arc};

use ash::vk::SubmitInfo;
use log::{debug, error, info};
use smallvec::smallvec;
use vulkano::{
    buffer::{
        allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo},
        Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer,
    },
    command_buffer::{
        allocator::{
            CommandBufferAllocator, CommandBufferBuilderAlloc, StandardCommandBufferAllocator,
        },
        sys::{CommandBufferBeginInfo, UnsafeCommandBufferBuilder},
        AutoCommandBufferBuilder, CommandBufferExecFuture, CommandBufferInheritanceInfo,
        CommandBufferInheritanceRenderPassType, CommandBufferInheritanceRenderingInfo,
        CommandBufferLevel, CommandBufferUsage, PrimaryAutoCommandBuffer,
        PrimaryCommandBufferAbstract, RenderingAttachmentInfo, RenderingInfo,
        SecondaryAutoCommandBuffer, SubpassContents,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, PersistentDescriptorSet, WriteDescriptorSet,
    },
    device::{
        physical::{PhysicalDevice, PhysicalDeviceType},
        Device, DeviceCreateInfo, DeviceExtensions, Features, Queue, QueueCreateInfo, QueueFlags,
    },
    format::Format,
    image::{
        sys::Image, AttachmentImage, ImageAccess, ImageCreateFlags, ImageDimensions, ImageError,
        ImageLayout, ImageUsage, ImageViewAbstract, ImmutableImage, MipmapsCount, StorageImage,
        SubresourceData, SwapchainImage,
    },
    instance::{Instance, InstanceCreateInfo, InstanceExtensions},
    memory::allocator::{AllocationCreateInfo, MemoryUsage, StandardMemoryAllocator},
    pipeline::{
        graphics::{
            color_blend::{AttachmentBlend, ColorBlendState},
            input_assembly::InputAssemblyState,
            render_pass::PipelineRenderingCreateInfo,
            vertex_input::Vertex,
            viewport::{Viewport, ViewportState},
        },
        GraphicsPipeline, Pipeline, PipelineBindPoint,
    },
    render_pass::{LoadOp, StoreOp},
    sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
    shader::ShaderModule,
    swapchain::{CompositeAlpha, Surface, Swapchain, SwapchainCreateInfo},
    sync::{
        fence::Fence, future::NowFuture, AccessFlags, DependencyInfo, ImageMemoryBarrier,
        PipelineStages,
    },
    Version, VulkanLibrary, VulkanObject,
};
use vulkano_win::VkSurfaceBuild;
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};
use wlx_capture::frame::{
    DmabufFrame, DRM_FORMAT_ABGR8888, DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888,
};

#[repr(C)]
#[derive(BufferContents, Vertex, Copy, Clone, Debug)]
pub struct Vert2Uv {
    #[format(R32G32_SFLOAT)]
    pub in_pos: [f32; 2],
    #[format(R32G32_SFLOAT)]
    pub in_uv: [f32; 2],
}

pub const INDICES: [u16; 6] = [2, 1, 0, 1, 2, 3];

pub struct WlxGraphics {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,

    pub surface: Arc<Surface>,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,

    pub quad_verts: Subbuffer<[Vert2Uv]>,
    pub quad_indices: Subbuffer<[u16]>,
}

impl WlxGraphics {
    pub fn new(
        vk_instance_extensions: InstanceExtensions,
        mut vk_device_extensions_fn: impl FnMut(&PhysicalDevice) -> DeviceExtensions,
    ) -> (Arc<Self>, EventLoop<()>) {
        #[cfg(debug_assertions)]
        let layers = vec!["VK_LAYER_KHRONOS_validation".to_owned()];
        #[cfg(not(debug_assertions))]
        let layers = vec![];

        let library = VulkanLibrary::new().unwrap();
        let library_extensions = vulkano_win::required_extensions(&library);
        let required_extensions = library_extensions.union(&vk_instance_extensions);

        debug!("Instance exts for app: {:?}", &required_extensions);
        debug!("Instance exts for runtime: {:?}", &vk_instance_extensions);

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                enabled_extensions: required_extensions,
                enabled_layers: layers,
                enumerate_portability: true,
                ..Default::default()
            },
        )
        .unwrap();

        let mut device_extensions = DeviceExtensions {
            khr_swapchain: true,
            khr_external_memory: true,
            khr_external_memory_fd: true,
            ext_external_memory_dma_buf: true,
            ext_image_drm_format_modifier: true,
            ..DeviceExtensions::empty()
        };

        debug!("Device exts for app: {:?}", &device_extensions);

        // TODO headless
        let event_loop = EventLoop::new();
        let surface = WindowBuilder::new()
            .build_vk_surface(&event_loop, instance.clone())
            .unwrap();

        let (physical_device, my_extensions, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| {
                p.api_version() >= Version::V1_3 || p.supported_extensions().khr_dynamic_rendering
            })
            .filter_map(|p| {
                let runtime_extensions = vk_device_extensions_fn(&p);
                debug!(
                    "Device exts for {}: {:?}",
                    p.properties().device_name,
                    &runtime_extensions
                );
                let my_extensions = runtime_extensions.union(&device_extensions);
                if p.supported_extensions().contains(&my_extensions) {
                    Some((p, my_extensions))
                } else {
                    None
                }
            })
            .filter_map(|(p, my_extensions)| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.surface_support(i as u32, &surface).unwrap_or(false)
                    })
                    .map(|i| (p, my_extensions, i as u32))
            })
            .min_by_key(|(p, _, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .expect("no suitable physical device found");

        info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

        if physical_device.api_version() < Version::V1_3 {
            device_extensions.khr_dynamic_rendering = true;
        }

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: my_extensions,
                enabled_features: Features {
                    dynamic_rendering: true,
                    ..Features::empty()
                },
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let queue = queues.next().unwrap();

        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        ));
        let descriptor_set_allocator =
            Arc::new(StandardDescriptorSetAllocator::new(device.clone()));

        let vertices = [
            Vert2Uv {
                in_pos: [0., 0.],
                in_uv: [0., 0.],
            },
            Vert2Uv {
                in_pos: [0., 1.],
                in_uv: [0., 1.],
            },
            Vert2Uv {
                in_pos: [1., 0.],
                in_uv: [1., 0.],
            },
            Vert2Uv {
                in_pos: [1., 1.],
                in_uv: [1., 1.],
            },
        ];
        let quad_verts = Buffer::from_iter(
            &memory_allocator,
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                usage: MemoryUsage::Upload,
                ..Default::default()
            },
            vertices.into_iter(),
        )
        .unwrap();

        let quad_indices = Buffer::from_iter(
            &memory_allocator,
            BufferCreateInfo {
                usage: BufferUsage::INDEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                usage: MemoryUsage::Upload,
                ..Default::default()
            },
            INDICES.iter().cloned(),
        )
        .unwrap();

        let me = Self {
            instance,
            device,
            queue,
            surface,
            memory_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
        };

        (Arc::new(me), event_loop)
    }

    pub fn create_swapchain(
        &self,
        format: Option<Format>,
    ) -> (Arc<Swapchain>, Vec<Arc<SwapchainImage>>) {
        let (min_image_count, composite_alpha, image_format) = if let Some(format) = format {
            (1, CompositeAlpha::Opaque, format)
        } else {
            let surface_capabilities = self
                .device
                .physical_device()
                .surface_capabilities(&self.surface, Default::default())
                .unwrap();

            let composite_alpha = surface_capabilities
                .supported_composite_alpha
                .into_iter()
                .next()
                .unwrap();

            let image_format = Some(
                self.device
                    .physical_device()
                    .surface_formats(&self.surface, Default::default())
                    .unwrap()[0]
                    .0,
            );
            (
                surface_capabilities.min_image_count,
                composite_alpha,
                image_format.unwrap(),
            )
        };
        let window = self
            .surface
            .object()
            .unwrap()
            .downcast_ref::<Window>()
            .unwrap();
        let swapchain = Swapchain::new(
            self.device.clone(),
            self.surface.clone(),
            SwapchainCreateInfo {
                min_image_count,
                image_format: Some(image_format),
                image_extent: window.inner_size().into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha,
                ..Default::default()
            },
        )
        .unwrap();

        swapchain
    }

    pub fn upload_verts(
        &self,
        width: f32,
        height: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Subbuffer<[Vert2Uv]> {
        let rw = width;
        let rh = height;

        let x0 = x / rw;
        let y0 = y / rh;

        let x1 = w / rw + x0;
        let y1 = h / rh + y0;

        let vertices = [
            Vert2Uv {
                in_pos: [x0, y0],
                in_uv: [0.0, 0.0],
            },
            Vert2Uv {
                in_pos: [x0, y1],
                in_uv: [0.0, 1.0],
            },
            Vert2Uv {
                in_pos: [x1, y0],
                in_uv: [1.0, 0.0],
            },
            Vert2Uv {
                in_pos: [x1, y1],
                in_uv: [1.0, 1.0],
            },
        ];
        self.upload_buffer(BufferUsage::VERTEX_BUFFER, vertices.iter())
    }

    pub fn upload_buffer<T>(&self, usage: BufferUsage, contents: Iter<'_, T>) -> Subbuffer<[T]>
    where
        T: BufferContents + Clone,
    {
        Buffer::from_iter(
            &self.memory_allocator,
            BufferCreateInfo {
                usage,
                ..Default::default()
            },
            AllocationCreateInfo {
                usage: MemoryUsage::Upload,
                ..Default::default()
            },
            contents.cloned(),
        )
        .unwrap()
    }

    pub fn dmabuf_texture(&self, frame: DmabufFrame) -> Result<Arc<StorageImage>, ImageError> {
        let dimensions = ImageDimensions::Dim2d {
            width: frame.format.width,
            height: frame.format.height,
            array_layers: 1,
        };

        let format = match frame.format.fourcc {
            DRM_FORMAT_ABGR8888 => Format::R8G8B8A8_UNORM,
            DRM_FORMAT_XBGR8888 => Format::R8G8B8A8_UNORM,
            DRM_FORMAT_ARGB8888 => Format::B8G8R8A8_UNORM,
            DRM_FORMAT_XRGB8888 => Format::B8G8R8A8_UNORM,
            _ => panic!("Unsupported dmabuf format {:x}", frame.format.fourcc),
        };

        let planes = frame
            .planes
            .iter()
            .take(frame.num_planes)
            .filter_map(|plane| {
                let Some(fd) = plane.fd else {
                    return None;
                };
                Some(SubresourceData {
                    fd,
                    offset: plane.offset as _,
                    row_pitch: plane.stride as _,
                })
            })
            .collect();

        StorageImage::new_from_dma_buf_fd(
            &self.memory_allocator,
            self.device.clone(),
            dimensions,
            format,
            ImageUsage::SAMPLED | ImageUsage::TRANSFER_SRC,
            ImageCreateFlags::empty(),
            [self.queue.queue_family_index()],
            planes,
            frame.format.modifier,
        )
    }

    pub fn render_texture(&self, width: u32, height: u32, format: Format) -> Arc<AttachmentImage> {
        let tex = AttachmentImage::with_usage(
            &self.memory_allocator,
            [width, height],
            format,
            ImageUsage::SAMPLED | ImageUsage::TRANSFER_SRC | ImageUsage::COLOR_ATTACHMENT,
        )
        .unwrap();

        tex
    }

    pub fn create_pipeline(
        self: &Arc<Self>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
    ) -> Arc<WlxPipeline> {
        Arc::new(WlxPipeline::new(self.clone(), vert, frag, format))
    }

    pub fn create_command_buffer(
        self: &Arc<Self>,
        usage: CommandBufferUsage,
    ) -> WlxCommandBuffer<PrimaryAutoCommandBuffer> {
        let command_buffer = AutoCommandBufferBuilder::primary(
            &self.command_buffer_allocator,
            self.queue.queue_family_index(),
            usage,
        )
        .unwrap();
        WlxCommandBuffer {
            graphics: self.clone(),
            command_buffer,
        }
    }

    pub fn transition_layout(
        &self,
        image: Arc<Image>,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    ) -> Fence {
        let barrier = ImageMemoryBarrier {
            src_stages: PipelineStages::ALL_TRANSFER,
            src_access: AccessFlags::TRANSFER_WRITE,
            dst_stages: PipelineStages::ALL_TRANSFER,
            dst_access: AccessFlags::TRANSFER_READ,
            old_layout,
            new_layout,
            subresource_range: image.subresource_range(),
            ..ImageMemoryBarrier::image(image)
        };

        let builder_alloc = self
            .command_buffer_allocator
            .allocate(
                self.queue.queue_family_index(),
                CommandBufferLevel::Primary,
                1,
            )
            .unwrap()
            .next()
            .unwrap();

        let command_buffer = unsafe {
            let mut builder = UnsafeCommandBufferBuilder::new(
                &builder_alloc.inner(),
                CommandBufferBeginInfo {
                    usage: CommandBufferUsage::OneTimeSubmit,
                    ..Default::default()
                },
            )
            .unwrap();

            builder.pipeline_barrier(&DependencyInfo {
                image_memory_barriers: smallvec![barrier],
                ..Default::default()
            });
            builder.build().unwrap()
        };

        let fence = vulkano::sync::fence::Fence::new(
            self.device.clone(),
            vulkano::sync::fence::FenceCreateInfo::default(),
        )
        .unwrap();

        let fns = self.device.fns();
        unsafe {
            (fns.v1_0.queue_submit)(
                self.queue.handle(),
                1,
                [SubmitInfo::builder()
                    .command_buffers(&[command_buffer.handle()])
                    .build()]
                .as_ptr(),
                fence.handle(),
            )
        }
        .result()
        .unwrap();

        fence
    }
}

pub struct WlxCommandBuffer<T> {
    graphics: Arc<WlxGraphics>,
    command_buffer: AutoCommandBufferBuilder<T, Arc<StandardCommandBufferAllocator>>,
}

impl<T> WlxCommandBuffer<T> {
    pub fn inner(&self) -> &AutoCommandBufferBuilder<T, Arc<StandardCommandBufferAllocator>> {
        &self.command_buffer
    }

    pub fn inner_mut(
        &mut self,
    ) -> &mut AutoCommandBufferBuilder<T, Arc<StandardCommandBufferAllocator>> {
        &mut self.command_buffer
    }

    pub fn to_inner(self) -> AutoCommandBufferBuilder<T, Arc<StandardCommandBufferAllocator>> {
        self.command_buffer
    }

    pub fn begin(
        mut self,
        render_target: Arc<dyn ImageViewAbstract>,
        want_layout: Option<ImageLayout>,
    ) -> Self {
        if let Some(want_layout) = want_layout {
            let mut barrier =
                ImageMemoryBarrier::image(render_target.image().inner().image.clone());
            barrier.old_layout = ImageLayout::ColorAttachmentOptimal;
            barrier.new_layout = want_layout;
        }

        self.command_buffer
            .begin_rendering(RenderingInfo {
                contents: SubpassContents::SecondaryCommandBuffers,
                color_attachments: vec![Some(RenderingAttachmentInfo {
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: Some([0.0, 0.0, 0.0, 0.0].into()),
                    ..RenderingAttachmentInfo::image_view(render_target.clone())
                })],
                ..Default::default()
            })
            .unwrap();
        self
    }

    pub fn run_ref(&mut self, pass: &WlxPass) -> &mut Self {
        let _ = self
            .command_buffer
            .execute_commands(pass.command_buffer.clone())
            .unwrap();
        self
    }

    pub fn run(mut self, pass: &WlxPass) -> Self {
        let _ = self
            .command_buffer
            .execute_commands(pass.command_buffer.clone());
        self
    }

    pub fn texture2d(
        &mut self,
        width: u32,
        height: u32,
        format: Format,
        data: Vec<u8>,
    ) -> Arc<ImmutableImage> {
        let dimensions = ImageDimensions::Dim2d {
            width,
            height,
            array_layers: 1,
        };

        ImmutableImage::from_iter(
            &self.graphics.memory_allocator,
            data,
            dimensions,
            MipmapsCount::One,
            format,
            &mut self.command_buffer,
        )
        .unwrap()
    }

    pub fn texture2d_png(&mut self, bytes: Vec<u8>) -> Arc<ImmutableImage> {
        let cursor = Cursor::new(bytes);
        let decoder = png::Decoder::new(cursor);
        let mut reader = decoder.read_info().unwrap();
        let info = reader.info();
        let width = info.width;
        let height = info.height;
        let mut image_data = Vec::new();
        image_data.resize((info.width * info.height * 4) as usize, 0);
        reader.next_frame(&mut image_data).unwrap();
        self.texture2d(width, height, Format::R8G8B8A8_UNORM, image_data)
    }
}

impl WlxCommandBuffer<PrimaryAutoCommandBuffer> {
    pub fn end_render_and_continue(&mut self) {
        self.command_buffer.end_rendering().unwrap();
    }

    pub fn end_render(self) -> PrimaryAutoCommandBuffer {
        let mut buf = self.command_buffer;
        buf.end_rendering().unwrap();

        buf.build().unwrap()
    }

    pub fn end(self) -> PrimaryAutoCommandBuffer {
        self.command_buffer.build().unwrap()
    }

    pub fn end_render_and_execute(self) -> CommandBufferExecFuture<NowFuture> {
        let mut buf = self.command_buffer;
        buf.end_rendering().unwrap();
        let buf = buf.build().unwrap();
        buf.execute(self.graphics.queue.clone()).unwrap()
    }

    pub fn end_and_execute(self) -> CommandBufferExecFuture<NowFuture> {
        let buf = self.command_buffer;
        let buf = buf.build().unwrap();
        buf.execute(self.graphics.queue.clone()).unwrap()
    }
}

pub struct WlxPipeline {
    pub graphics: Arc<WlxGraphics>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub format: Format,
}

impl WlxPipeline {
    fn new(
        graphics: Arc<WlxGraphics>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
    ) -> Self {
        let vep = vert.entry_point("main").unwrap();
        let fep = frag.entry_point("main").unwrap();
        let pipeline = GraphicsPipeline::start()
            .render_pass(PipelineRenderingCreateInfo {
                color_attachment_formats: vec![Some(format)],
                ..Default::default()
            })
            .color_blend_state(ColorBlendState::default().blend(AttachmentBlend::alpha()))
            .vertex_input_state(Vert2Uv::per_vertex())
            .input_assembly_state(InputAssemblyState::new())
            .vertex_shader(vep, ())
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            .fragment_shader(fep, ())
            .build(graphics.device.clone())
            .unwrap();

        Self {
            graphics,
            pipeline,
            format,
        }
    }

    pub fn inner(&self) -> Arc<GraphicsPipeline> {
        self.pipeline.clone()
    }

    pub fn graphics(&self) -> Arc<WlxGraphics> {
        self.graphics.clone()
    }

    pub fn uniform_sampler(
        &self,
        set: usize,
        texture: Arc<dyn ImageViewAbstract>,
        filter: Filter,
    ) -> Arc<PersistentDescriptorSet> {
        let sampler = Sampler::new(
            self.graphics.device.clone(),
            SamplerCreateInfo {
                mag_filter: filter,
                min_filter: filter,
                address_mode: [SamplerAddressMode::Repeat; 3],
                ..Default::default()
            },
        )
        .unwrap();

        let layout = self.pipeline.layout().set_layouts().get(set).unwrap();

        PersistentDescriptorSet::new(
            &self.graphics.descriptor_set_allocator,
            layout.clone(),
            [WriteDescriptorSet::image_view_sampler(0, texture, sampler)],
        )
        .unwrap()
    }

    pub fn uniform_buffer<T>(&self, set: usize, data: Vec<T>) -> Arc<PersistentDescriptorSet>
    where
        T: BufferContents + Copy,
    {
        let uniform_buffer = SubbufferAllocator::new(
            self.graphics.memory_allocator.clone(),
            SubbufferAllocatorCreateInfo {
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                ..Default::default()
            },
        );

        let uniform_buffer_subbuffer = {
            let subbuffer = uniform_buffer.allocate_slice(data.len() as _).unwrap();
            subbuffer.write().unwrap().copy_from_slice(data.as_slice());
            subbuffer
        };

        let layout = self.pipeline.layout().set_layouts().get(set).unwrap();
        PersistentDescriptorSet::new(
            &self.graphics.descriptor_set_allocator,
            layout.clone(),
            [WriteDescriptorSet::buffer(0, uniform_buffer_subbuffer)],
        )
        .unwrap()
    }

    pub fn create_pass(
        self: &Arc<Self>,
        dimensions: [f32; 2],
        vertex_buffer: Subbuffer<[Vert2Uv]>,
        index_buffer: Subbuffer<[u16]>,
        descriptor_sets: Vec<Arc<PersistentDescriptorSet>>,
    ) -> WlxPass {
        WlxPass::new(
            self.clone(),
            dimensions,
            vertex_buffer,
            index_buffer,
            descriptor_sets,
        )
    }
}

pub struct WlxPass {
    pipeline: Arc<WlxPipeline>,
    vertex_buffer: Subbuffer<[Vert2Uv]>,
    index_buffer: Subbuffer<[u16]>,
    descriptor_sets: Vec<Arc<PersistentDescriptorSet>>,
    pub command_buffer: Arc<SecondaryAutoCommandBuffer>,
}

impl WlxPass {
    fn new(
        pipeline: Arc<WlxPipeline>,
        dimensions: [f32; 2],
        vertex_buffer: Subbuffer<[Vert2Uv]>,
        index_buffer: Subbuffer<[u16]>,
        descriptor_sets: Vec<Arc<PersistentDescriptorSet>>,
    ) -> Self {
        let viewport = Viewport {
            origin: [0.0, 0.0],
            dimensions,
            depth_range: 0.0..1.0,
        };

        let pipeline_inner = pipeline.inner().clone();
        let mut command_buffer = AutoCommandBufferBuilder::secondary(
            &pipeline.graphics.command_buffer_allocator,
            pipeline.graphics.queue.queue_family_index(),
            CommandBufferUsage::MultipleSubmit,
            CommandBufferInheritanceInfo {
                render_pass: Some(CommandBufferInheritanceRenderPassType::BeginRendering(
                    CommandBufferInheritanceRenderingInfo {
                        color_attachment_formats: vec![Some(pipeline.format)],
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
        )
        .unwrap();

        command_buffer
            .set_viewport(0, [viewport])
            .bind_pipeline_graphics(pipeline_inner)
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                pipeline.inner().layout().clone(),
                0,
                descriptor_sets.clone(),
            )
            .bind_vertex_buffers(0, vertex_buffer.clone())
            .bind_index_buffer(index_buffer.clone())
            .draw_indexed(index_buffer.len() as u32, 1, 0, 0, 0)
            .or_else(|err| {
                if let Some(source) = err.source() {
                    error!("Failed to draw: {}", source);
                }
                Err(err)
            })
            .unwrap();

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            descriptor_sets,
            command_buffer: Arc::new(command_buffer.build().unwrap()),
        }
    }
}
