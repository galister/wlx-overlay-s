use std::{
    collections::HashMap,
    error::Error,
    io::Cursor,
    os::fd::{FromRawFd, IntoRawFd},
    slice::Iter,
    sync::{Arc, RwLock},
};

use ash::vk::{self, SubmitInfo};
use smallvec::{smallvec, SmallVec};
use vulkano::instance::InstanceCreateFlags;
use vulkano::{
    buffer::{
        allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo},
        Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer,
    },
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        sys::{CommandBufferBeginInfo, RawRecordingCommandBuffer},
        CommandBuffer, CommandBufferExecFuture, CommandBufferInheritanceInfo,
        CommandBufferInheritanceRenderPassInfo, CommandBufferInheritanceRenderPassType,
        CommandBufferLevel, CommandBufferUsage, CopyBufferToImageInfo, RecordingCommandBuffer,
        RenderPassBeginInfo, SubpassBeginInfo, SubpassContents, SubpassEndInfo,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, DescriptorSet, WriteDescriptorSet,
    },
    device::physical::PhysicalDeviceType,
    device::Features,
    device::{
        physical::PhysicalDevice, Device, DeviceCreateInfo, DeviceExtensions, Queue,
        QueueCreateInfo, QueueFlags,
    },
    format::Format,
    image::{
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        sys::RawImage,
        view::ImageView,
        Image, ImageCreateInfo, ImageLayout, ImageTiling, ImageType, ImageUsage, SampleCount,
        SubresourceLayout,
    },
    instance::InstanceExtensions,
    instance::{Instance, InstanceCreateInfo},
    memory::{
        allocator::{
            AllocationCreateInfo, GenericMemoryAllocatorCreateInfo, MemoryAllocator,
            MemoryTypeFilter, StandardMemoryAllocator,
        },
        DedicatedAllocation, DeviceMemory, ExternalMemoryHandleType, ExternalMemoryHandleTypes,
        MemoryAllocateInfo, MemoryImportInfo, MemoryPropertyFlags, ResourceMemory,
    },
    pipeline::{
        graphics::{
            color_blend::{AttachmentBlend, ColorBlendAttachmentState, ColorBlendState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            vertex_input::{Vertex, VertexDefinition},
            viewport::{Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
        DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout,
        PipelineShaderStageCreateInfo,
    },
    render_pass::{
        AttachmentDescription, AttachmentLoadOp, AttachmentReference, AttachmentStoreOp,
        Framebuffer, FramebufferCreateInfo, RenderPass, RenderPassCreateInfo, Subpass,
        SubpassDescription,
    },
    shader::ShaderModule,
    sync::{
        fence::Fence, future::NowFuture, AccessFlags, DependencyInfo, GpuFuture,
        ImageMemoryBarrier, PipelineStages,
    },
    DeviceSize, VulkanLibrary, VulkanObject,
};

use wlx_capture::frame::{
    DmabufFrame, FourCC, DRM_FORMAT_ABGR8888, DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR8888,
    DRM_FORMAT_XRGB8888,
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

    pub native_format: Format,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,

    pub quad_verts: Subbuffer<[Vert2Uv]>,
    pub quad_indices: Subbuffer<[u16]>,

    pub shared_shaders: RwLock<HashMap<&'static str, Arc<ShaderModule>>>,
    pub shared_images: RwLock<HashMap<&'static str, Arc<ImageView>>>,
}

impl WlxGraphics {
    #[cfg(feature = "openxr")]
    pub fn new_openxr(xr_instance: openxr::Instance, system: openxr::SystemId) -> Arc<Self> {
        use std::ffi::{self, c_char, CString};

        use vulkano::Handle;

        let vk_target_version = vk::make_api_version(0, 1, 3, 0); // Vulkan 1.1 guarantees multiview support
        let target_version = vulkano::Version::V1_3;
        let library = VulkanLibrary::new().unwrap();
        let vk_entry = unsafe { ash::Entry::load().unwrap() };

        let vk_app_info = vk::ApplicationInfo::builder()
            .application_version(0)
            .engine_version(0)
            .api_version(vk_target_version);

        let instance = unsafe {
            let vk_instance = xr_instance
                .create_vulkan_instance(
                    system,
                    std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr),
                    &vk::InstanceCreateInfo::builder().application_info(&vk_app_info) as *const _
                        as *const _,
                )
                .expect("XR error creating Vulkan instance")
                .map_err(vk::Result::from_raw)
                .expect("Vulkan error creating Vulkan instance");

            Instance::from_handle(
                library,
                ash::vk::Instance::from_raw(vk_instance as _),
                InstanceCreateInfo {
                    ..Default::default()
                },
            )
        };

        let physical_device = unsafe {
            PhysicalDevice::from_handle(
                instance.clone(),
                vk::PhysicalDevice::from_raw(
                    xr_instance
                        .vulkan_graphics_device(system, instance.handle().as_raw() as _)
                        .unwrap() as _,
                ),
            )
        }
        .unwrap();

        let vk_device_properties = physical_device.properties();
        if vk_device_properties.api_version < target_version {
            panic!(
                "Vulkan physical device doesn't support Vulkan {}",
                target_version
            );
        }

        log::info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

        let queue_family_index = physical_device
            .queue_family_properties()
            .iter()
            .enumerate()
            .position(|(_, q)| q.queue_flags.intersects(QueueFlags::GRAPHICS))
            .expect("Vulkan device has no graphics queue") as u32;

        let device_extensions = DeviceExtensions {
            khr_external_memory: true,
            khr_external_memory_fd: true,
            ext_external_memory_dma_buf: true,
            ext_image_drm_format_modifier: true,
            ..DeviceExtensions::empty()
        };

        let device_extensions_raw = device_extensions
            .clone()
            .into_iter()
            .filter_map(|(name, enabled)| {
                if enabled {
                    Some(ffi::CString::new(name).unwrap().into_raw() as *const c_char)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let (device, mut queues) = unsafe {
            let vk_device = xr_instance
                .create_vulkan_device(
                    system,
                    std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr),
                    physical_device.handle().as_raw() as _,
                    &vk::DeviceCreateInfo::builder()
                        .queue_create_infos(&[vk::DeviceQueueCreateInfo::builder()
                            .queue_family_index(queue_family_index)
                            .queue_priorities(&[1.0])
                            .build()])
                        .enabled_extension_names(&device_extensions_raw)
                        as *const _ as *const _,
                )
                .expect("XR error creating Vulkan device")
                .map_err(vk::Result::from_raw)
                .expect("Vulkan error creating Vulkan device");

            vulkano::device::Device::from_handle(
                physical_device,
                vk::Device::from_raw(vk_device as _),
                DeviceCreateInfo {
                    queue_create_infos: vec![QueueCreateInfo {
                        queue_family_index,
                        ..Default::default()
                    }],
                    enabled_extensions: device_extensions,
                    ..Default::default()
                },
            )
        };

        // Drop the CStrings
        device_extensions_raw
            .into_iter()
            .for_each(|c_string| unsafe {
                let _ = CString::from_raw(c_string as _);
            });

        let queue = queues.next().unwrap();

        let memory_allocator = memory_allocator(device.clone());
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            StandardCommandBufferAllocatorCreateInfo {
                secondary_buffer_count: 32,
                ..Default::default()
            },
        ));
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
            device.clone(),
            Default::default(),
        ));

        let (quad_verts, quad_indices) = Self::default_quad(memory_allocator.clone());

        let me = Self {
            instance,
            device,
            queue,
            native_format: Format::R8G8B8A8_UNORM,
            memory_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
            shared_images: RwLock::new(HashMap::new()),
            shared_shaders: RwLock::new(HashMap::new()),
        };

        Arc::new(me)
    }

    #[cfg(feature = "openvr")]
    pub fn new_openvr(
        vk_instance_extensions: InstanceExtensions,
        mut vk_device_extensions_fn: impl FnMut(&PhysicalDevice) -> DeviceExtensions,
    ) -> Arc<Self> {
        #[cfg(debug_assertions)]
        let layers = vec!["VK_LAYER_KHRONOS_validation".to_owned()];
        #[cfg(not(debug_assertions))]
        let layers = vec![];

        let library = VulkanLibrary::new().unwrap();

        log::debug!("Instance exts for runtime: {:?}", &vk_instance_extensions);

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: vk_instance_extensions,
                enabled_layers: layers,
                ..Default::default()
            },
        )
        .unwrap();

        let device_extensions = DeviceExtensions {
            khr_external_memory: true,
            khr_external_memory_fd: true,
            ext_external_memory_dma_buf: true,
            ext_image_drm_format_modifier: true,
            ..DeviceExtensions::empty()
        };

        log::debug!("Device exts for app: {:?}", &device_extensions);

        let (physical_device, my_extensions, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter_map(|p| {
                let runtime_extensions = vk_device_extensions_fn(&p);
                log::debug!(
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
                    .position(|(_, q)| q.queue_flags.intersects(QueueFlags::GRAPHICS))
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

        log::info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

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

        let memory_allocator = memory_allocator(device.clone());
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            StandardCommandBufferAllocatorCreateInfo {
                secondary_buffer_count: 32,
                ..Default::default()
            },
        ));
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
            device.clone(),
            Default::default(),
        ));

        let (quad_verts, quad_indices) = Self::default_quad(memory_allocator.clone());

        let me = Self {
            instance,
            device,
            queue,
            memory_allocator,
            native_format: Format::R8G8B8A8_UNORM,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
            shared_images: RwLock::new(HashMap::new()),
            shared_shaders: RwLock::new(HashMap::new()),
        };

        Arc::new(me)
    }

    fn default_quad(
        memory_allocator: Arc<StandardMemoryAllocator>,
    ) -> (Subbuffer<[Vert2Uv]>, Subbuffer<[u16]>) {
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
            memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            vertices.into_iter(),
        )
        .unwrap();

        let quad_indices = Buffer::from_iter(
            memory_allocator,
            BufferCreateInfo {
                usage: BufferUsage::INDEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            INDICES.iter().cloned(),
        )
        .unwrap();

        (quad_verts, quad_indices)
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
            self.memory_allocator.clone(),
            BufferCreateInfo {
                usage,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_HOST
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            contents.cloned(),
        )
        .unwrap()
    }

    pub fn dmabuf_texture(&self, frame: DmabufFrame) -> Option<Arc<Image>> {
        let extent = [frame.format.width, frame.format.height, 1];

        let format = fourcc_to_vk(frame.format.fourcc);

        let layouts: Vec<SubresourceLayout> = (0..frame.num_planes)
            .into_iter()
            .map(|i| {
                let plane = &frame.planes[i];
                SubresourceLayout {
                    offset: plane.offset as _,
                    size: 0,
                    row_pitch: plane.stride as _,
                    array_pitch: None,
                    depth_pitch: None,
                }
            })
            .collect();

        let external_memory_handle_types = ExternalMemoryHandleTypes::DMA_BUF;

        let image = RawImage::new(
            self.device.clone(),
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format,
                extent,
                usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_SRC,
                external_memory_handle_types,
                tiling: ImageTiling::DrmFormatModifier,
                drm_format_modifiers: vec![frame.format.modifier],
                drm_format_modifier_plane_layouts: layouts,
                ..Default::default()
            },
        )
        .unwrap();

        let requirements = image.memory_requirements()[0];
        let memory_type_index = self
            .memory_allocator
            .find_memory_type_index(
                requirements.memory_type_bits,
                MemoryTypeFilter::PREFER_DEVICE,
            )
            .unwrap();

        debug_assert!(self.device.enabled_extensions().khr_external_memory_fd);
        debug_assert!(self.device.enabled_extensions().khr_external_memory);
        debug_assert!(self.device.enabled_extensions().ext_external_memory_dma_buf);

        let mut allocations: SmallVec<[ResourceMemory; 4]> = smallvec![];

        unsafe {
            let Some(fd) = frame.planes[0].fd else {
                log::error!("DMA-buf plane has no FD");
                return None;
            };

            let file = std::fs::File::from_raw_fd(fd);
            let new_file = file.try_clone().unwrap();
            file.into_raw_fd();

            let memory = DeviceMemory::import(
                self.device.clone(),
                MemoryAllocateInfo {
                    allocation_size: requirements.layout.size(),
                    memory_type_index,
                    dedicated_allocation: Some(DedicatedAllocation::Image(&image)),
                    ..Default::default()
                },
                MemoryImportInfo::Fd {
                    file: new_file,
                    handle_type: ExternalMemoryHandleType::DmaBuf,
                },
            )
            .unwrap();

            allocations.push(ResourceMemory::new_dedicated(memory));
        }

        match unsafe { image.bind_memory(allocations) } {
            Ok(image) => Some(Arc::new(image)),
            Err(e) => {
                log::warn!("Failed to bind memory to image: {}", e.0.to_string());
                return None;
            }
        }
    }

    pub fn render_texture(&self, width: u32, height: u32, format: Format) -> Arc<Image> {
        log::debug!(
            "Render texture: {}x{} {}MB",
            width,
            height,
            (width * height * 4) / (1024 * 1024)
        );
        Image::new(
            self.memory_allocator.clone(),
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format,
                extent: [width, height, 1],
                usage: ImageUsage::TRANSFER_SRC
                    | ImageUsage::SAMPLED
                    | ImageUsage::COLOR_ATTACHMENT,
                ..Default::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap()
    }

    pub fn create_pipeline(
        self: &Arc<Self>,
        render_target: Arc<ImageView>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
    ) -> Arc<WlxPipeline> {
        Arc::new(WlxPipeline::new(
            render_target,
            self.clone(),
            vert,
            frag,
            format,
        ))
    }

    pub fn create_pipeline_with_layouts(
        self: &Arc<Self>,
        render_target: Arc<ImageView>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
        initial_layout: ImageLayout,
        final_layout: ImageLayout,
    ) -> Arc<WlxPipeline> {
        Arc::new(WlxPipeline::new_with_layout(
            render_target,
            self.clone(),
            vert,
            frag,
            format,
            initial_layout,
            final_layout,
        ))
    }

    pub fn create_command_buffer(self: &Arc<Self>, usage: CommandBufferUsage) -> WlxCommandBuffer {
        let command_buffer = RecordingCommandBuffer::new(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferLevel::Primary,
            CommandBufferBeginInfo {
                usage,
                inheritance_info: None,
                ..Default::default()
            },
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

        let command_buffer = unsafe {
            let mut builder = RawRecordingCommandBuffer::new(
                self.command_buffer_allocator.clone(),
                self.queue.queue_family_index(),
                CommandBufferLevel::Primary,
                CommandBufferBeginInfo {
                    usage: CommandBufferUsage::OneTimeSubmit,
                    inheritance_info: None,
                    ..Default::default()
                },
            )
            .unwrap();

            builder
                .pipeline_barrier(&DependencyInfo {
                    image_memory_barriers: smallvec![barrier],
                    ..Default::default()
                })
                .unwrap();
            builder.end().unwrap()
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

pub struct WlxCommandBuffer {
    pub graphics: Arc<WlxGraphics>,
    pub command_buffer: RecordingCommandBuffer,
}

impl WlxCommandBuffer {
    pub fn begin_render_pass(&mut self, pipeline: &WlxPipeline) {
        self.command_buffer
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some([0.0, 0.0, 0.0, 1.0].into())],
                    ..RenderPassBeginInfo::framebuffer(pipeline.framebuffer.clone())
                },
                SubpassBeginInfo {
                    contents: SubpassContents::SecondaryCommandBuffers,
                    ..Default::default()
                },
            )
            .unwrap();
    }

    pub fn run_ref(&mut self, pass: &WlxPass) -> &mut Self {
        let _ = self
            .command_buffer
            .execute_commands(pass.command_buffer.clone())
            .unwrap();
        self
    }

    pub fn texture2d(
        &mut self,
        width: u32,
        height: u32,
        format: Format,
        data: &[u8],
    ) -> Arc<Image> {
        log::debug!(
            "Texture2D: {}x{} {}MB",
            width,
            height,
            data.len() / (1024 * 1024)
        );
        let image = Image::new(
            self.graphics.memory_allocator.clone(),
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format,
                extent: [width, height, 1],
                usage: ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC | ImageUsage::SAMPLED,
                ..Default::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();

        let buffer: Subbuffer<[u8]> = Buffer::new_slice(
            self.graphics.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_HOST
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            data.len() as DeviceSize,
        )
        .unwrap();

        buffer.write().unwrap().copy_from_slice(data);

        self.command_buffer
            .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(buffer, image.clone()))
            .unwrap();

        image
    }

    #[allow(dead_code)]
    pub fn texture2d_png(&mut self, bytes: Vec<u8>) -> Arc<Image> {
        let cursor = Cursor::new(bytes);
        let decoder = png::Decoder::new(cursor);
        let mut reader = decoder.read_info().unwrap();
        let info = reader.info();
        let width = info.width;
        let height = info.height;
        let mut image_data = Vec::new();
        image_data.resize((info.width * info.height * 4) as usize, 0);
        reader.next_frame(&mut image_data).unwrap();
        self.texture2d(width, height, Format::R8G8B8A8_UNORM, &image_data)
    }
}

impl WlxCommandBuffer {
    pub fn end_render_pass(&mut self) {
        self.command_buffer
            .end_render_pass(SubpassEndInfo::default())
            .unwrap();
    }

    pub fn build(self) -> Arc<CommandBuffer> {
        self.command_buffer.end().unwrap()
    }

    pub fn build_and_execute(self) -> CommandBufferExecFuture<NowFuture> {
        let queue = self.graphics.queue.clone();
        self.build().execute(queue).unwrap()
    }

    pub fn build_and_execute_now(self) {
        let mut exec = self.build_and_execute();
        exec.flush().unwrap();
        exec.cleanup_finished();
    }
}

pub struct WlxPipeline {
    pub graphics: Arc<WlxGraphics>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub render_pass: Arc<RenderPass>,
    pub framebuffer: Arc<Framebuffer>,
    pub view: Arc<ImageView>,
    pub format: Format,
}

impl WlxPipeline {
    fn new(
        render_target: Arc<ImageView>,
        graphics: Arc<WlxGraphics>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
    ) -> Self {
        let render_pass = vulkano::single_pass_renderpass!(
            graphics.device.clone(),
            attachments: {
                color: {
                    format: format,
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {},
            },
        )
        .unwrap();

        Self::new_from_pass(render_target, render_pass, graphics, vert, frag, format)
    }

    fn new_with_layout(
        render_target: Arc<ImageView>,
        graphics: Arc<WlxGraphics>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
        initial_layout: ImageLayout,
        final_layout: ImageLayout,
    ) -> Self {
        let render_pass_description = RenderPassCreateInfo {
            attachments: vec![AttachmentDescription {
                format,
                samples: SampleCount::Sample1,
                load_op: AttachmentLoadOp::Clear,
                store_op: AttachmentStoreOp::Store,
                initial_layout,
                final_layout,
                ..Default::default()
            }],
            subpasses: vec![SubpassDescription {
                color_attachments: vec![Some(AttachmentReference {
                    attachment: 0,
                    layout: ImageLayout::ColorAttachmentOptimal,
                    ..Default::default()
                })],
                ..Default::default()
            }],
            ..Default::default()
        };

        let render_pass =
            RenderPass::new(graphics.device.clone(), render_pass_description).unwrap();

        Self::new_from_pass(render_target, render_pass, graphics, vert, frag, format)
    }

    fn new_from_pass(
        render_target: Arc<ImageView>,
        render_pass: Arc<RenderPass>,
        graphics: Arc<WlxGraphics>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
    ) -> Self {
        let vep = vert.entry_point("main").unwrap();
        let fep = frag.entry_point("main").unwrap();

        let vertex_input_state = Vert2Uv::per_vertex()
            .definition(&vep.info().input_interface)
            .unwrap();

        let stages = smallvec![
            PipelineShaderStageCreateInfo::new(vep),
            PipelineShaderStageCreateInfo::new(fep),
        ];

        let layout = PipelineLayout::new(
            graphics.device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(graphics.device.clone())
                .unwrap(),
        )
        .unwrap();

        let framebuffer = Framebuffer::new(
            render_pass.clone(),
            FramebufferCreateInfo {
                attachments: vec![render_target.clone()],
                ..Default::default()
            },
        )
        .unwrap();

        let pipeline = GraphicsPipeline::new(
            graphics.device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages,
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                color_blend_state: Some(ColorBlendState {
                    attachments: vec![ColorBlendAttachmentState {
                        blend: Some(AttachmentBlend::alpha()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState::default()),
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(Subpass::from(render_pass.clone(), 0).unwrap().into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )
        .unwrap();

        Self {
            graphics,
            pipeline,
            format,
            render_pass,
            framebuffer,
            view: render_target,
        }
    }

    pub fn inner(&self) -> Arc<GraphicsPipeline> {
        self.pipeline.clone()
    }

    pub fn uniform_sampler(
        &self,
        set: usize,
        texture: Arc<ImageView>,
        filter: Filter,
    ) -> Arc<DescriptorSet> {
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

        DescriptorSet::new(
            self.graphics.descriptor_set_allocator.clone(),
            layout.clone(),
            [WriteDescriptorSet::image_view_sampler(0, texture, sampler)],
            [],
        )
        .unwrap()
    }

    pub fn uniform_buffer<T>(&self, set: usize, data: Vec<T>) -> Arc<DescriptorSet>
    where
        T: BufferContents + Copy,
    {
        let uniform_buffer = SubbufferAllocator::new(
            self.graphics.memory_allocator.clone(),
            SubbufferAllocatorCreateInfo {
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
        );

        let uniform_buffer_subbuffer = {
            let subbuffer = uniform_buffer.allocate_slice(data.len() as _).unwrap();
            subbuffer.write().unwrap().copy_from_slice(data.as_slice());
            subbuffer
        };

        let layout = self.pipeline.layout().set_layouts().get(set).unwrap();
        DescriptorSet::new(
            self.graphics.descriptor_set_allocator.clone(),
            layout.clone(),
            [WriteDescriptorSet::buffer(0, uniform_buffer_subbuffer)],
            [],
        )
        .unwrap()
    }

    pub fn create_pass(
        self: &Arc<Self>,
        dimensions: [f32; 2],
        vertex_buffer: Subbuffer<[Vert2Uv]>,
        index_buffer: Subbuffer<[u16]>,
        descriptor_sets: Vec<Arc<DescriptorSet>>,
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

#[allow(dead_code)]
pub struct WlxPass {
    pipeline: Arc<WlxPipeline>,
    vertex_buffer: Subbuffer<[Vert2Uv]>,
    index_buffer: Subbuffer<[u16]>,
    descriptor_sets: Vec<Arc<DescriptorSet>>,
    pub command_buffer: Arc<CommandBuffer>,
}

impl WlxPass {
    fn new(
        pipeline: Arc<WlxPipeline>,
        dimensions: [f32; 2],
        vertex_buffer: Subbuffer<[Vert2Uv]>,
        index_buffer: Subbuffer<[u16]>,
        descriptor_sets: Vec<Arc<DescriptorSet>>,
    ) -> Self {
        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: dimensions,
            depth_range: 0.0..=1.0,
        };

        let pipeline_inner = pipeline.inner().clone();
        let mut command_buffer = RecordingCommandBuffer::new(
            pipeline.graphics.command_buffer_allocator.clone(),
            pipeline.graphics.queue.queue_family_index(),
            CommandBufferLevel::Secondary,
            CommandBufferBeginInfo {
                usage: CommandBufferUsage::MultipleSubmit,
                inheritance_info: Some(CommandBufferInheritanceInfo {
                    render_pass: Some(CommandBufferInheritanceRenderPassType::BeginRenderPass(
                        CommandBufferInheritanceRenderPassInfo {
                            subpass: Subpass::from(pipeline.render_pass.clone(), 0).unwrap(),
                            framebuffer: None,
                        },
                    )),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();

        unsafe {
            command_buffer
                .set_viewport(0, smallvec![viewport])
                .unwrap()
                .bind_pipeline_graphics(pipeline_inner)
                .unwrap()
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    pipeline.inner().layout().clone(),
                    0,
                    descriptor_sets.clone(),
                )
                .unwrap()
                .bind_vertex_buffers(0, vertex_buffer.clone())
                .unwrap()
                .bind_index_buffer(index_buffer.clone())
                .unwrap()
                .draw_indexed(index_buffer.len() as u32, 1, 0, 0, 0)
                .or_else(|err| {
                    if let Some(source) = err.source() {
                        log::error!("Failed to draw: {}", source);
                    }
                    Err(err)
                })
                .unwrap()
        };

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            descriptor_sets,
            command_buffer: command_buffer.end().unwrap(),
        }
    }
}

pub fn fourcc_to_vk(fourcc: FourCC) -> Format {
    match fourcc.value {
        DRM_FORMAT_ABGR8888 => Format::R8G8B8A8_UNORM,
        DRM_FORMAT_XBGR8888 => Format::R8G8B8A8_UNORM,
        DRM_FORMAT_ARGB8888 => Format::B8G8R8A8_UNORM,
        DRM_FORMAT_XRGB8888 => Format::B8G8R8A8_UNORM,
        _ => panic!("Unsupported memfd format {}", fourcc),
    }
}

fn memory_allocator(device: Arc<Device>) -> Arc<StandardMemoryAllocator> {
    let props = device.physical_device().memory_properties();

    let mut block_sizes = vec![0; props.memory_types.len()];
    let mut memory_type_bits = u32::MAX;

    for (index, memory_type) in props.memory_types.iter().enumerate() {
        const LARGE_HEAP_THRESHOLD: DeviceSize = 1024 * 1024 * 1024;

        let heap_size = props.memory_heaps[memory_type.heap_index as usize].size;

        block_sizes[index] = if heap_size >= LARGE_HEAP_THRESHOLD {
            48 * 1024 * 1024
        } else {
            24 * 1024 * 1024
        };

        if memory_type.property_flags.intersects(
            MemoryPropertyFlags::LAZILY_ALLOCATED
                | MemoryPropertyFlags::PROTECTED
                | MemoryPropertyFlags::DEVICE_COHERENT
                | MemoryPropertyFlags::RDMA_CAPABLE,
        ) {
            // VUID-VkMemoryAllocateInfo-memoryTypeIndex-01872
            // VUID-vkAllocateMemory-deviceCoherentMemory-02790
            // Lazily allocated memory would just cause problems for suballocation in general.
            memory_type_bits &= !(1 << index);
        }
    }

    let create_info = GenericMemoryAllocatorCreateInfo {
        block_sizes: &block_sizes,
        memory_type_bits,
        ..Default::default()
    };

    Arc::new(StandardMemoryAllocator::new(device, create_info))
}
