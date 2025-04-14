pub mod dds;
pub mod dmabuf;

use std::{
    collections::HashMap,
    os::fd::{FromRawFd, IntoRawFd},
    slice::Iter,
    sync::{Arc, OnceLock, RwLock},
};

use anyhow::{anyhow, bail};
use ash::vk::SubmitInfo;
use dmabuf::create_dmabuf_image;
use smallvec::smallvec;

#[cfg(feature = "openvr")]
use vulkano::instance::InstanceCreateFlags;

#[cfg(feature = "openxr")]
use {ash::vk, std::os::raw::c_void};

use vulkano::{
    buffer::{
        allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo},
        Buffer, BufferContents, BufferCreateInfo, BufferUsage, IndexBuffer, Subbuffer,
    },
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferBeginInfo, CommandBufferExecFuture,
        CommandBufferInheritanceInfo, CommandBufferInheritanceRenderPassType,
        CommandBufferInheritanceRenderingInfo, CommandBufferLevel, CommandBufferUsage,
        CopyBufferToImageInfo, PrimaryAutoCommandBuffer, PrimaryCommandBufferAbstract,
        RecordingCommandBuffer, RenderingAttachmentInfo, RenderingInfo, SecondaryAutoCommandBuffer,
        SubpassContents,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, DescriptorSet, WriteDescriptorSet,
    },
    device::{
        physical::{PhysicalDevice, PhysicalDeviceType}, Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures,
        Queue, QueueCreateInfo, QueueFlags,
    },
    format::Format,
    image::{
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        view::ImageView,
        Image, ImageCreateInfo, ImageLayout, ImageTiling, ImageType, ImageUsage, SubresourceLayout,
    },
    instance::{Instance, InstanceCreateInfo, InstanceExtensions},
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
            color_blend::{
                AttachmentBlend, BlendFactor, BlendOp, ColorBlendAttachmentState, ColorBlendState,
            },
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            subpass::PipelineRenderingCreateInfo,
            vertex_input::{Vertex, VertexDefinition},
            viewport::{Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
        DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout,
    },
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    shader::ShaderModule,
    sync::{
        fence::Fence, future::NowFuture, AccessFlags, DependencyInfo, GpuFuture,
        ImageMemoryBarrier, PipelineStages,
    },
    DeviceSize, VulkanObject,
};

use wlx_capture::frame::{
    DmabufFrame, DrmFormat, FourCC, DRM_FORMAT_ABGR2101010, DRM_FORMAT_ABGR8888,
    DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR2101010, DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888,
};

pub type Vert2Buf = Subbuffer<[Vert2Uv]>;
pub type IndexBuf = IndexBuffer;

pub const DRM_FORMAT_MOD_INVALID: u64 = 0xff_ffff_ffff_ffff;

#[repr(C)]
#[derive(BufferContents, Vertex, Copy, Clone, Debug)]
pub struct Vert2Uv {
    #[format(R32G32_SFLOAT)]
    pub in_pos: [f32; 2],
    #[format(R32G32_SFLOAT)]
    pub in_uv: [f32; 2],
}

pub const INDICES: [u16; 6] = [2, 1, 0, 1, 2, 3];

pub const BLEND_ALPHA: AttachmentBlend = AttachmentBlend {
    src_color_blend_factor: BlendFactor::SrcAlpha,
    dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
    color_blend_op: BlendOp::Add,
    src_alpha_blend_factor: BlendFactor::One,
    dst_alpha_blend_factor: BlendFactor::One,
    alpha_blend_op: BlendOp::Max,
};

pub struct WlxGraphics {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub graphics_queue: Arc<Queue>,
    pub transfer_queue: Arc<Queue>,
    pub capture_queue: Option<Arc<Queue>>,

    pub native_format: Format,
    pub texture_filtering: Filter,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,

    pub quad_verts: Vert2Buf,
    pub quad_indices: IndexBuf,

    pub shared_shaders: RwLock<HashMap<&'static str, Arc<ShaderModule>>>,
    pub drm_formats: Vec<DrmFormat>,
}

const fn get_dmabuf_extensions() -> DeviceExtensions {
    DeviceExtensions {
        khr_external_memory: true,
        khr_external_memory_fd: true,
        ext_external_memory_dma_buf: true,
        ..DeviceExtensions::empty()
    }
}

static VULKAN_LIBRARY: OnceLock<Arc<vulkano::VulkanLibrary>> = OnceLock::new();
fn get_vulkan_library() -> &'static Arc<vulkano::VulkanLibrary> {
    VULKAN_LIBRARY.get_or_init(|| vulkano::VulkanLibrary::new().unwrap()) // want panic
}

#[cfg(feature = "openxr")]
unsafe extern "system" fn get_instance_proc_addr(
    instance: openxr::sys::platform::VkInstance,
    name: *const std::ffi::c_char,
) -> Option<unsafe extern "system" fn()> {
    use vulkano::Handle;
    let instance = ash::vk::Instance::from_raw(instance as _);
    let library = get_vulkan_library();
    library.get_instance_proc_addr(instance, name)
}

impl WlxGraphics {
    #[cfg(feature = "openxr")]
    #[allow(clippy::too_many_lines)]
    pub fn new_openxr(
        xr_instance: openxr::Instance,
        system: openxr::SystemId,
    ) -> anyhow::Result<Arc<Self>> {
        use std::ffi::{self, CString};

        use vulkano::{
            descriptor_set::allocator::StandardDescriptorSetAllocatorCreateInfo, Handle, Version,
        };

        let instance_extensions = InstanceExtensions {
            khr_get_physical_device_properties2: true,
            ..InstanceExtensions::empty()
        };

        let instance_extensions_raw = instance_extensions
            .into_iter()
            .filter_map(|(name, enabled)| {
                if enabled {
                    Some(ffi::CString::new(name).unwrap().into_raw().cast_const())
                // want panic
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let vk_target_version = vk::make_api_version(0, 1, 3, 0);
        let target_version = vulkano::Version::V1_3;
        let library = get_vulkan_library();

        let vk_app_info_raw = vk::ApplicationInfo::default()
            .application_version(0)
            .engine_version(0)
            .api_version(vk_target_version);

        let instance = unsafe {
            let vk_instance = xr_instance
                .create_vulkan_instance(
                    system,
                    get_instance_proc_addr,
                    std::ptr::from_ref(
                        &vk::InstanceCreateInfo::default()
                            .application_info(&vk_app_info_raw)
                            .enabled_extension_names(&instance_extensions_raw),
                    )
                    .cast(),
                )
                .expect("XR error creating Vulkan instance")
                .map_err(vk::Result::from_raw)
                .expect("Vulkan error creating Vulkan instance");

            Instance::from_handle(
                library.clone(),
                ash::vk::Instance::from_raw(vk_instance as _),
                InstanceCreateInfo {
                    application_version: Version::major_minor(0, 0),
                    engine_version: Version::major_minor(0, 0),
                    max_api_version: Some(Version::V1_3),
                    enabled_extensions: instance_extensions,
                    ..Default::default()
                },
            )
        };

        let physical_device = unsafe {
            PhysicalDevice::from_handle(
                instance.clone(),
                vk::PhysicalDevice::from_raw(
                    xr_instance.vulkan_graphics_device(system, instance.handle().as_raw() as _)?
                        as _,
                ),
            )
        }?;

        let vk_device_properties = physical_device.properties();
        assert!(
            (vk_device_properties.api_version >= target_version),
            "Vulkan physical device doesn't support Vulkan {target_version}"
        );

        log::info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

        let queue_families = try_all_queue_families(physical_device.as_ref())
            .expect("vkPhysicalDevice does not have a GRAPHICS / TRANSFER queue.");

        let mut device_extensions = DeviceExtensions::empty();
        let dmabuf_extensions = get_dmabuf_extensions();

        if physical_device
            .supported_extensions()
            .contains(&dmabuf_extensions)
        {
            device_extensions = device_extensions.union(&dmabuf_extensions);
            device_extensions.ext_image_drm_format_modifier = physical_device
                .supported_extensions()
                .ext_image_drm_format_modifier;
        }

        let texture_filtering = if physical_device.supported_extensions().ext_filter_cubic {
            device_extensions.ext_filter_cubic = true;
            Filter::Cubic
        } else {
            Filter::Linear
        };

        let device_extensions_raw = device_extensions
            .into_iter()
            .filter_map(|(name, enabled)| {
                if enabled {
                    Some(ffi::CString::new(name).unwrap().into_raw().cast_const())
                // want panic
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let features = DeviceFeatures {
            dynamic_rendering: true,
            ..Default::default()
        };

        let queue_create_infos = queue_families
            .iter()
            .map(|fam| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(fam.queue_family_index)
                    .queue_priorities(&fam.priorities)
            })
            .collect::<Vec<_>>();

        let mut device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions_raw);

        let mut dynamic_rendering =
            vk::PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true);

        dynamic_rendering.p_next = device_create_info.p_next.cast_mut();
        device_create_info.p_next = &raw mut dynamic_rendering as *const c_void;

        let (device, queues) = unsafe {
            let vk_device = xr_instance
                .create_vulkan_device(
                    system,
                    get_instance_proc_addr,
                    physical_device.handle().as_raw() as _,
                    (&raw const device_create_info).cast(),
                )
                .expect("XR error creating Vulkan device")
                .map_err(vk::Result::from_raw)
                .expect("Vulkan error creating Vulkan device");

            vulkano::device::Device::from_handle(
                physical_device,
                vk::Device::from_raw(vk_device as _),
                DeviceCreateInfo {
                    queue_create_infos: queue_families
                        .iter()
                        .map(|fam| QueueCreateInfo {
                            queue_family_index: fam.queue_family_index,
                            queues: fam.priorities.clone(),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    enabled_extensions: device_extensions,
                    enabled_features: features,
                    ..Default::default()
                },
            )
        };

        log::debug!(
            "  DMA-buf supported: {}",
            device.enabled_extensions().ext_external_memory_dma_buf
        );
        log::debug!(
            "  DRM format modifiers supported: {}",
            device.enabled_extensions().ext_image_drm_format_modifier
        );

        // Drop the CStrings
        device_extensions_raw
            .into_iter()
            .for_each(|c_string| unsafe {
                let _ = CString::from_raw(c_string.cast_mut());
            });

        let (graphics_queue, transfer_queue, capture_queue) = unwrap_queues(queues.collect());

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
            StandardDescriptorSetAllocatorCreateInfo::default(),
        ));

        let (quad_verts, quad_indices) = Self::default_quad(memory_allocator.clone())?;
        let drm_formats = Self::get_drm_formats(device.clone());

        let me = Self {
            instance,
            device,
            graphics_queue,
            transfer_queue,
            capture_queue,
            native_format: Format::R8G8B8A8_SRGB,
            texture_filtering,
            memory_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
            shared_shaders: RwLock::new(HashMap::new()),
            drm_formats,
        };

        Ok(Arc::new(me))
    }

    #[allow(clippy::too_many_lines)]
    #[cfg(feature = "openvr")]
    pub fn new_openvr(
        mut vk_instance_extensions: InstanceExtensions,
        mut vk_device_extensions_fn: impl FnMut(&PhysicalDevice) -> DeviceExtensions,
    ) -> anyhow::Result<Arc<Self>> {
        //#[cfg(debug_assertions)]
        //let layers = vec!["VK_LAYER_KHRONOS_validation".to_owned()];
        //#[cfg(not(debug_assertions))]

        use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocatorCreateInfo;

        let layers = vec![];

        log::debug!("Instance exts for runtime: {:?}", &vk_instance_extensions);

        vk_instance_extensions.khr_get_physical_device_properties2 = true;

        let instance = Instance::new(
            get_vulkan_library().clone(),
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: vk_instance_extensions,
                enabled_layers: layers,
                ..Default::default()
            },
        )?;

        let dmabuf_extensions = get_dmabuf_extensions();

        let (physical_device, my_extensions, queue_families) = instance
            .enumerate_physical_devices()?
            .filter_map(|p| {
                let mut my_extensions = vk_device_extensions_fn(&p);

                if !p.supported_extensions().contains(&my_extensions) {
                    log::debug!(
                        "Not using {} due to missing extensions:",
                        p.properties().device_name,
                    );
                    for (ext, missing) in p.supported_extensions().difference(&my_extensions) {
                        if missing {
                            log::debug!("  {ext}");
                        }
                    }
                    return None;
                }

                if p.supported_extensions().contains(&dmabuf_extensions) {
                    my_extensions = my_extensions.union(&dmabuf_extensions);
                    my_extensions.ext_image_drm_format_modifier =
                        p.supported_extensions().ext_image_drm_format_modifier;
                }

                if p.supported_extensions().ext_filter_cubic {
                    my_extensions.ext_filter_cubic = true;
                }

                log::debug!(
                    "Device exts for {}: {:?}",
                    p.properties().device_name,
                    &my_extensions
                );
                Some((p, my_extensions))
            })
            .filter_map(|(p, my_extensions)| {
                try_all_queue_families(p.as_ref()).map(|families| (p, my_extensions, families))
            })
            .min_by_key(|(p, _, families)| {
                prio_from_device_type(p) * 10 + prio_from_families(families)
            })
            .expect("no suitable physical device found");

        log::info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

        let texture_filtering = if physical_device.supported_extensions().ext_filter_cubic {
            Filter::Cubic
        } else {
            Filter::Linear
        };

        let (device, queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: my_extensions,
                enabled_features: DeviceFeatures {
                    dynamic_rendering: true,
                    ..DeviceFeatures::empty()
                },
                queue_create_infos: queue_families
                    .iter()
                    .map(|fam| QueueCreateInfo {
                        queue_family_index: fam.queue_family_index,
                        queues: fam.priorities.clone(),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
                ..Default::default()
            },
        )?;

        log::debug!(
            "  DMA-buf supported: {}",
            device.enabled_extensions().ext_external_memory_dma_buf
        );
        log::debug!(
            "  DRM format modifiers supported: {}",
            device.enabled_extensions().ext_image_drm_format_modifier
        );

        let (graphics_queue, transfer_queue, capture_queue) = unwrap_queues(queues.collect());

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
            StandardDescriptorSetAllocatorCreateInfo::default(),
        ));

        let (quad_verts, quad_indices) = Self::default_quad(memory_allocator.clone())?;
        let drm_formats = Self::get_drm_formats(device.clone());

        let me = Self {
            instance,
            device,
            graphics_queue,
            transfer_queue,
            capture_queue,
            memory_allocator,
            native_format: Format::R8G8B8A8_SRGB,
            texture_filtering,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
            shared_shaders: RwLock::new(HashMap::new()),
            drm_formats,
        };

        Ok(Arc::new(me))
    }

    #[cfg(feature = "uidev")]
    #[allow(clippy::type_complexity, clippy::too_many_lines)]
    pub fn new_window() -> anyhow::Result<(
        Arc<Self>,
        winit::event_loop::EventLoop<()>,
        Arc<winit::window::Window>,
        Arc<vulkano::swapchain::Surface>,
    )> {
        use vulkano::{
            descriptor_set::allocator::StandardDescriptorSetAllocatorCreateInfo,
            instance::InstanceCreateFlags,
            swapchain::{Surface, SurfaceInfo},
        };
        use winit::{event_loop::EventLoop, window::Window};

        let event_loop = EventLoop::new().unwrap(); // want panic
        let mut vk_instance_extensions = Surface::required_extensions(&event_loop).unwrap();
        vk_instance_extensions.khr_get_physical_device_properties2 = true;
        log::debug!("Instance exts for runtime: {:?}", &vk_instance_extensions);

        let instance = Instance::new(
            get_vulkan_library().clone(),
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: vk_instance_extensions,
                ..Default::default()
            },
        )?;

        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(), // want panic
        );
        let surface = Surface::from_window(instance.clone(), window.clone())?;

        let mut device_extensions = DeviceExtensions::empty();
        device_extensions.khr_swapchain = true;

        log::debug!("Device exts for app: {:?}", &device_extensions);

        let (physical_device, my_extensions, queue_families) = instance
            .enumerate_physical_devices()?
            .filter_map(|p| {
                if p.supported_extensions().contains(&device_extensions) {
                    Some((p, device_extensions))
                } else {
                    log::debug!(
                        "Not using {} because it does not implement the following device extensions:",
                        p.properties().device_name,
                    );
                    for (ext, missing) in p.supported_extensions().difference(&device_extensions) {
                        if missing {
                            log::debug!("  {ext}");
                        }
                    }
                    None
                }
            })
            .filter_map(|(p, my_extensions)| 
                try_all_queue_families(p.as_ref()).map(|families| (p, my_extensions, families))
            )
            .min_by_key(|(p, _, _)| prio_from_device_type(p)
            )
            .expect("no suitable physical device found");

        log::info!(
            "Using vkPhysicalDevice: {}",
            physical_device.properties().device_name,
        );

        let (device, queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: my_extensions,
                enabled_features: DeviceFeatures {
                    dynamic_rendering: true,
                    ..DeviceFeatures::empty()
                },
                queue_create_infos: queue_families
                    .iter()
                    .map(|fam| QueueCreateInfo {
                        queue_family_index: fam.queue_family_index,
                        queues: fam.priorities.clone(),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
                ..Default::default()
            },
        )?;

        let (graphics_queue, transfer_queue, capture_queue) = unwrap_queues(queues.collect());

        let native_format = device
            .physical_device()
            .surface_formats(&surface, SurfaceInfo::default())
            .unwrap()[0] // want panic
            .0;
        log::info!("Using surface format: {native_format:?}");

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
            StandardDescriptorSetAllocatorCreateInfo::default(),
        ));

        let (quad_verts, quad_indices) = Self::default_quad(memory_allocator.clone())?;
        let drm_formats = Self::get_drm_formats(device.clone());

        let me = Self {
            instance,
            device,
            graphics_queue,
            transfer_queue,
            capture_queue,
            memory_allocator,
            native_format,
            texture_filtering: Filter::Linear,
            command_buffer_allocator,
            descriptor_set_allocator,
            quad_indices,
            quad_verts,
            shared_shaders: RwLock::new(HashMap::new()),
            drm_formats,
        };

        Ok((Arc::new(me), event_loop, window, surface))
    }
    fn default_quad(
        memory_allocator: Arc<StandardMemoryAllocator>,
    ) -> anyhow::Result<(Vert2Buf, IndexBuf)> {
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
        )?;

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
            INDICES.iter().copied(),
        )?;

        Ok((quad_verts, IndexBuffer::U16(quad_indices)))
    }

    pub fn upload_verts(
        &self,
        width: f32,
        height: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> anyhow::Result<Vert2Buf> {
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

    pub fn upload_buffer<T>(
        &self,
        usage: BufferUsage,
        contents: Iter<'_, T>,
    ) -> anyhow::Result<Subbuffer<[T]>>
    where
        T: BufferContents + Clone,
    {
        Ok(Buffer::from_iter(
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
        )?)
    }

    fn get_drm_formats(device: Arc<Device>) -> Vec<DrmFormat> {
        let possible_formats = [
            DRM_FORMAT_ABGR8888.into(),
            DRM_FORMAT_XBGR8888.into(),
            DRM_FORMAT_ARGB8888.into(),
            DRM_FORMAT_XRGB8888.into(),
            DRM_FORMAT_ABGR2101010.into(),
            DRM_FORMAT_XBGR2101010.into(),
        ];

        let mut final_formats = vec![];

        for &f in &possible_formats {
            let Ok(vk_fmt) = fourcc_to_vk(f) else {
                continue;
            };
            let Ok(props) = device.physical_device().format_properties(vk_fmt) else {
                continue;
            };
            let mut fmt = DrmFormat {
                fourcc: f,
                modifiers: props
                    .drm_format_modifier_properties
                    .iter()
                    // important bit: only allow single-plane
                    .filter(|m| m.drm_format_modifier_plane_count == 1)
                    .map(|m| m.drm_format_modifier)
                    .collect(),
            };
            fmt.modifiers.push(DRM_FORMAT_MOD_INVALID); // implicit modifiers support
            final_formats.push(fmt);
        }
        log::debug!("Supported DRM formats:");
        for f in &final_formats {
            log::debug!("  {} {:?}", f.fourcc, f.modifiers);
        }
        final_formats
    }

    pub fn dmabuf_texture_ex(
        &self,
        frame: DmabufFrame,
        tiling: ImageTiling,
        layouts: Vec<SubresourceLayout>,
        modifiers: &[u64],
    ) -> anyhow::Result<Arc<Image>> {
        let extent = [frame.format.width, frame.format.height, 1];
        let format = fourcc_to_vk(frame.format.fourcc)?;

        let image = unsafe {
            create_dmabuf_image(
                self.device.clone(),
                ImageCreateInfo {
                    format,
                    extent,
                    usage: ImageUsage::SAMPLED,
                    external_memory_handle_types: ExternalMemoryHandleTypes::DMA_BUF,
                    tiling,
                    drm_format_modifiers: modifiers.to_owned(),
                    drm_format_modifier_plane_layouts: layouts,
                    ..Default::default()
                },
            )?
        };

        let requirements = image.memory_requirements()[0];
        let memory_type_index = self
            .memory_allocator
            .find_memory_type_index(
                requirements.memory_type_bits,
                MemoryTypeFilter {
                    required_flags: MemoryPropertyFlags::DEVICE_LOCAL,
                    ..Default::default()
                },
            )
            .ok_or_else(|| anyhow!("failed to get memory type index"))?;

        debug_assert!(self.device.enabled_extensions().khr_external_memory_fd);
        debug_assert!(self.device.enabled_extensions().khr_external_memory);
        debug_assert!(self.device.enabled_extensions().ext_external_memory_dma_buf);

        // only do the 1st
        unsafe {
            let Some(fd) = frame.planes[0].fd else {
                bail!("DMA-buf plane has no FD");
            };

            let file = std::fs::File::from_raw_fd(fd);
            let new_file = file.try_clone()?;
            let _ = file.into_raw_fd();

            let memory = DeviceMemory::allocate_unchecked(
                self.device.clone(),
                MemoryAllocateInfo {
                    allocation_size: requirements.layout.size(),
                    memory_type_index,
                    dedicated_allocation: Some(DedicatedAllocation::Image(&image)),
                    ..Default::default()
                },
                Some(MemoryImportInfo::Fd {
                    file: new_file,
                    handle_type: ExternalMemoryHandleType::DmaBuf,
                }),
            )?;

            let mem_alloc = ResourceMemory::new_dedicated(memory);
            match image.bind_memory_unchecked([mem_alloc]) {
                Ok(image) => Ok(Arc::new(image)),
                Err(e) => {
                    bail!("Failed to bind memory to image: {}", e.0);
                }
            }
        }
    }

    pub fn dmabuf_texture(&self, frame: DmabufFrame) -> anyhow::Result<Arc<Image>> {
        let mut modifiers: Vec<u64> = vec![];
        let mut tiling: ImageTiling = ImageTiling::Optimal;
        let mut layouts: Vec<SubresourceLayout> = vec![];

        if frame.format.modifier != DRM_FORMAT_MOD_INVALID {
            (0..frame.num_planes).for_each(|i| {
                let plane = &frame.planes[i];
                layouts.push(SubresourceLayout {
                    offset: plane.offset.into(),
                    size: 0,
                    row_pitch: plane.stride as _,
                    array_pitch: None,
                    depth_pitch: None,
                });
                modifiers.push(frame.format.modifier);
            });
            tiling = ImageTiling::DrmFormatModifier;
        }

        self.dmabuf_texture_ex(frame, tiling, layouts, &modifiers)
    }

    pub fn render_texture(
        &self,
        width: u32,
        height: u32,
        format: Format,
    ) -> anyhow::Result<Arc<Image>> {
        log::debug!(
            "Render texture: {}x{} {}MB",
            width,
            height,
            (width * height * 4) / (1024 * 1024)
        );
        Ok(Image::new(
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
        )?)
    }

    pub fn create_pipeline(
        self: &Arc<Self>,
        vert: Arc<ShaderModule>,
        frag: Arc<ShaderModule>,
        format: Format,
        blend: Option<AttachmentBlend>,
    ) -> anyhow::Result<Arc<WlxPipeline>> {
        Ok(Arc::new(WlxPipeline::new(
            self.clone(),
            vert,
            frag,
            format,
            blend,
        )?))
    }

    /// Creates a CommandBuffer to be used for graphics workloads on the main thread.
    pub fn create_command_buffer(
        self: &Arc<Self>,
        usage: CommandBufferUsage,
    ) -> anyhow::Result<WlxCommandBuffer> {
        let command_buffer = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.graphics_queue.queue_family_index(),
            usage,
        )?;
        Ok(WlxCommandBuffer {
            graphics: self.clone(),
            queue: self.graphics_queue.clone(),
            command_buffer,
            dummy: None,
        })
    }

    /// Creates a CommandBuffer to be used for texture uploads on the main thread.
    pub fn create_uploads_command_buffer(
        self: &Arc<Self>,
        queue: Arc<Queue>,
        usage: CommandBufferUsage,
    ) -> anyhow::Result<WlxUploadsBuffer> {
        let command_buffer = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            queue.queue_family_index(),
            usage,
        )?;
        Ok(WlxUploadsBuffer {
            graphics: self.clone(),
            queue,
            command_buffer,
            dummy: None,
        })
    }

    pub fn transition_layout(
        &self,
        image: Arc<Image>,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    ) -> anyhow::Result<Fence> {
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
            let mut builder = RecordingCommandBuffer::new(
                self.command_buffer_allocator.clone(),
                self.graphics_queue.queue_family_index(),
                CommandBufferLevel::Primary,
                CommandBufferBeginInfo {
                    usage: CommandBufferUsage::OneTimeSubmit,
                    inheritance_info: None,
                    ..Default::default()
                },
            )?;

            builder.pipeline_barrier(&DependencyInfo {
                image_memory_barriers: smallvec![barrier],
                ..Default::default()
            })?;
            builder.end()?
        };

        let fence = vulkano::sync::fence::Fence::new(
            self.device.clone(),
            vulkano::sync::fence::FenceCreateInfo::default(),
        )?;

        let fns = self.device.fns();
        unsafe {
            (fns.v1_0.queue_submit)(
                self.graphics_queue.handle(),
                1,
                [SubmitInfo::default().command_buffers(&[command_buffer.handle()])].as_ptr(),
                fence.handle(),
            )
        }
        .result()?;

        Ok(fence)
    }
}

pub type WlxCommandBuffer = AnyCommandBuffer<GraphicsBuffer>;
pub type WlxUploadsBuffer = AnyCommandBuffer<UploadBuffer>;

pub struct GraphicsBuffer;
pub struct UploadBuffer;

pub struct AnyCommandBuffer<T> {
    pub graphics: Arc<WlxGraphics>,
    pub command_buffer: AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pub queue: Arc<Queue>,
    dummy: Option<T>,
}

impl<T> AnyCommandBuffer<T> {
    pub fn build_and_execute(self) -> anyhow::Result<CommandBufferExecFuture<NowFuture>> {
        let queue = self.queue.clone();
        Ok(self.command_buffer.build()?.execute(queue)?)
    }

    pub fn build_and_execute_now(self) -> anyhow::Result<()> {
        let mut exec = self.build_and_execute()?;
        exec.flush()?;
        exec.cleanup_finished();
        Ok(())
    }
}

impl AnyCommandBuffer<GraphicsBuffer> {
    pub fn begin_rendering(&mut self, render_target: Arc<ImageView>) -> anyhow::Result<()> {
        self.command_buffer.begin_rendering(RenderingInfo {
            contents: SubpassContents::SecondaryCommandBuffers,
            color_attachments: vec![Some(RenderingAttachmentInfo {
                load_op: AttachmentLoadOp::Clear,
                store_op: AttachmentStoreOp::Store,
                clear_value: Some([0.0, 0.0, 0.0, 0.0].into()),
                ..RenderingAttachmentInfo::image_view(render_target)
            })],
            ..Default::default()
        })?;
        Ok(())
    }

    pub fn build(self) -> anyhow::Result<Arc<PrimaryAutoCommandBuffer>> {
        Ok(self.command_buffer.build()?)
    }

    pub fn run_ref(&mut self, pass: &WlxPass) -> anyhow::Result<()> {
        self.command_buffer
            .execute_commands(pass.command_buffer.clone())?;
        Ok(())
    }

    pub fn end_rendering(&mut self) -> anyhow::Result<()> {
        self.command_buffer.end_rendering()?;
        Ok(())
    }
}

impl AnyCommandBuffer<UploadBuffer> {
    pub fn texture2d_raw(
        &mut self,
        width: u32,
        height: u32,
        format: Format,
        data: &[u8],
    ) -> anyhow::Result<Arc<Image>> {
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
        )?;

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
        )?;

        buffer.write()?.copy_from_slice(data);

        self.command_buffer
            .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(buffer, image.clone()))?;

        Ok(image)
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
        blend: Option<AttachmentBlend>,
    ) -> anyhow::Result<Self> {
        let vep = vert.entry_point("main").unwrap(); // want panic
        let fep = frag.entry_point("main").unwrap(); // want panic

        let vertex_input_state = Vert2Uv::per_vertex().definition(&vep)?;

        let stages = smallvec![
            vulkano::pipeline::PipelineShaderStageCreateInfo::new(vep),
            vulkano::pipeline::PipelineShaderStageCreateInfo::new(fep),
        ];

        let layout = PipelineLayout::new(
            graphics.device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(graphics.device.clone())?,
        )?;

        let subpass = PipelineRenderingCreateInfo {
            color_attachment_formats: vec![Some(format)],
            ..Default::default()
        };

        let pipeline = GraphicsPipeline::new(
            graphics.device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages,
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState {
                    attachments: vec![ColorBlendAttachmentState {
                        blend,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                dynamic_state: std::iter::once(DynamicState::Viewport).collect(),
                subpass: Some(subpass.into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )?;

        Ok(Self {
            graphics,
            pipeline,
            format,
        })
    }
    pub fn create_pass(
        self: &Arc<Self>,
        dimensions: [f32; 2],
        vertex_buffer: Vert2Buf,
        index_buffer: IndexBuf,
        descriptor_sets: Vec<Arc<DescriptorSet>>,
    ) -> anyhow::Result<WlxPass> {
        WlxPass::new(
            self.clone(),
            dimensions,
            vertex_buffer,
            index_buffer,
            descriptor_sets,
        )
    }

    pub fn create_pass_for_target(
        self: &Arc<Self>,
        tgt: Arc<ImageView>,
        descriptor_sets: Vec<Arc<DescriptorSet>>,
    ) -> anyhow::Result<WlxPass> {
        let extent = tgt.image().extent();
        WlxPass::new(
            self.clone(),
            [extent[0] as _, extent[1] as _],
            self.graphics.quad_verts.clone(),
            self.graphics.quad_indices.clone(),
            descriptor_sets,
        )
    }
}

impl WlxPipeline {
    pub fn inner(&self) -> Arc<GraphicsPipeline> {
        self.pipeline.clone()
    }

    pub fn uniform_sampler(
        &self,
        set: usize,
        texture: Arc<ImageView>,
        filter: Filter,
    ) -> anyhow::Result<Arc<DescriptorSet>> {
        let sampler = Sampler::new(
            self.graphics.device.clone(),
            SamplerCreateInfo {
                mag_filter: filter,
                min_filter: filter,
                address_mode: [SamplerAddressMode::Repeat; 3],
                ..Default::default()
            },
        )?;

        let layout = self.pipeline.layout().set_layouts().get(set).unwrap(); // want panic

        Ok(DescriptorSet::new(
            self.graphics.descriptor_set_allocator.clone(),
            layout.clone(),
            [WriteDescriptorSet::image_view_sampler(0, texture, sampler)],
            [],
        )?)
    }

    pub fn uniform_buffer<T>(&self, set: usize, data: Vec<T>) -> anyhow::Result<Arc<DescriptorSet>>
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
            let subbuffer = uniform_buffer.allocate_slice(data.len() as _)?;
            subbuffer.write()?.copy_from_slice(data.as_slice());
            subbuffer
        };

        let layout = self.pipeline.layout().set_layouts().get(set).unwrap(); // want panic
        Ok(DescriptorSet::new(
            self.graphics.descriptor_set_allocator.clone(),
            layout.clone(),
            [WriteDescriptorSet::buffer(0, uniform_buffer_subbuffer)],
            [],
        )?)
    }
}

pub struct WlxPass {
    pub command_buffer: Arc<SecondaryAutoCommandBuffer>,
}

impl WlxPass {
    fn new(
        pipeline: Arc<WlxPipeline>,
        dimensions: [f32; 2],
        vertex_buffer: Vert2Buf,
        index_buffer: IndexBuf,
        descriptor_sets: Vec<Arc<DescriptorSet>>,
    ) -> anyhow::Result<Self> {
        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: dimensions,
            depth_range: 0.0..=1.0,
        };
        let pipeline_inner = pipeline.inner();
        let mut command_buffer = AutoCommandBufferBuilder::secondary(
            pipeline.graphics.command_buffer_allocator.clone(),
            pipeline.graphics.graphics_queue.queue_family_index(),
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
        )?;

        unsafe {
            command_buffer
                .set_viewport(0, smallvec![viewport])?
                .bind_pipeline_graphics(pipeline_inner)?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    pipeline.inner().layout().clone(),
                    0,
                    descriptor_sets,
                )?
                .bind_vertex_buffers(0, vertex_buffer)?
                .bind_index_buffer(index_buffer.clone())?
                .draw_indexed(index_buffer.len() as u32, 1, 0, 0, 0)?
        };

        Ok(Self {
            command_buffer: command_buffer.build()?,
        })
    }
}

#[derive(Default)]
pub struct CommandBuffers {
    inner: Vec<Arc<PrimaryAutoCommandBuffer>>,
}

impl CommandBuffers {
    pub fn push(&mut self, buffer: Arc<PrimaryAutoCommandBuffer>) {
        self.inner.push(buffer);
    }
    pub fn execute_now(self, queue: Arc<Queue>) -> anyhow::Result<Option<Box<dyn GpuFuture>>> {
        let mut buffers = self.inner.into_iter();
        let Some(first) = buffers.next() else {
            return Ok(None);
        };

        let future = first.execute(queue)?;
        let mut future: Box<dyn GpuFuture> = Box::new(future);

        for buf in buffers {
            future = Box::new(future.then_execute_same_queue(buf)?);
        }

        Ok(Some(future))
    }
    #[cfg(feature = "uidev")]
    pub fn execute_after(
        self,
        queue: Arc<Queue>,
        future: Box<dyn GpuFuture>,
    ) -> anyhow::Result<Box<dyn GpuFuture>> {
        let mut buffers = self.inner.into_iter();
        let Some(first) = buffers.next() else {
            return Ok(future);
        };

        let future = future.then_execute(queue, first)?;
        let mut future: Box<dyn GpuFuture> = Box::new(future);

        for buf in buffers {
            future = Box::new(future.then_execute_same_queue(buf)?);
        }

        Ok(future)
    }
}

pub fn fourcc_to_vk(fourcc: FourCC) -> anyhow::Result<Format> {
    match fourcc.value {
        DRM_FORMAT_ABGR8888 | DRM_FORMAT_XBGR8888 => Ok(Format::R8G8B8A8_UNORM),
        DRM_FORMAT_ARGB8888 | DRM_FORMAT_XRGB8888 => Ok(Format::B8G8R8A8_UNORM),
        DRM_FORMAT_ABGR2101010 | DRM_FORMAT_XBGR2101010 => Ok(Format::A2B10G10R10_UNORM_PACK32),
        _ => bail!("Unsupported format {}", fourcc),
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

#[derive(Debug)]
struct QueueFamilyLayout {
    queue_family_index: u32,
    priorities: Vec<f32>,
}

fn prio_from_device_type(physical_device: &PhysicalDevice) -> u32 {
    match physical_device.properties().device_type {
        PhysicalDeviceType::DiscreteGpu => 0,
        PhysicalDeviceType::IntegratedGpu => 1,
        PhysicalDeviceType::VirtualGpu => 2,
        PhysicalDeviceType::Cpu => 3,
        _ => 4,
    }
}

const fn prio_from_families(families: &[QueueFamilyLayout]) -> u32 {
    match families.len() {
        2 | 3 => 0,
        _ => 1,
    }
}

fn unwrap_queues(queues: Vec<Arc<Queue>>) -> (Arc<Queue>, Arc<Queue>, Option<Arc<Queue>>) {
    match queues[..] {
        [ref g, ref t, ref c] => (g.clone(), t.clone(), Some(c.clone())),
        [ref gt, ref c] => (gt.clone(), gt.clone(), Some(c.clone())),
        [ref gt] => (gt.clone(), gt.clone(), None),
        _ => unreachable!(),
    }
}

fn try_all_queue_families(physical_device: &PhysicalDevice) -> Option<Vec<QueueFamilyLayout>> {
    queue_families_priorities(
        physical_device,
        vec![
            // main-thread graphics + uploads
            QueueFlags::GRAPHICS | QueueFlags::TRANSFER,
            // capture-thread uploads
            QueueFlags::TRANSFER,
        ],
    )
    .or_else(|| {
        queue_families_priorities(
            physical_device,
            vec![
                // main thread graphics
                QueueFlags::GRAPHICS,
                // main thread uploads
                QueueFlags::TRANSFER,
                // capture thread uploads
                QueueFlags::TRANSFER,
            ],
        )
    })
    .or_else(|| {
        queue_families_priorities(
            physical_device,
            // main thread-only. software capture not supported.
            vec![QueueFlags::GRAPHICS | QueueFlags::TRANSFER],
        )
    })
}

fn queue_families_priorities(
    physical_device: &PhysicalDevice,
    mut requested_queues: Vec<QueueFlags>,
) -> Option<Vec<QueueFamilyLayout>> {
    let mut result = Vec::with_capacity(3);

    for (idx, props) in physical_device.queue_family_properties().iter().enumerate() {
        let mut remaining = props.queue_count;
        let mut want = 0usize;

        requested_queues.retain(|requested| {
            if props.queue_flags.intersects(*requested) && remaining > 0 {
                remaining -= 1;
                want += 1;
                false
            } else {
                true
            }
        });

        if want > 0 {
            result.push(QueueFamilyLayout {
                queue_family_index: idx as u32,
                priorities: std::iter::repeat_n(1.0, want).collect(),
            });
        }
    }

    if requested_queues.is_empty() {
        log::debug!("Selected GPU queue families: {result:?}");
        Some(result)
    } else {
        None
    }
}
