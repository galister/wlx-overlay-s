use std::{
    mem::MaybeUninit,
    os::fd::{FromRawFd, IntoRawFd},
    sync::Arc,
};

use anyhow::Context;
use smallvec::SmallVec;
use vulkano::{
    VulkanError, VulkanObject,
    device::Device,
    format::Format,
    image::{Image, ImageCreateInfo, ImageTiling, ImageUsage, SubresourceLayout, sys::RawImage},
    memory::{
        DedicatedAllocation, DeviceMemory, ExternalMemoryHandleType, ExternalMemoryHandleTypes,
        MemoryAllocateInfo, MemoryImportInfo, MemoryPropertyFlags, ResourceMemory,
        allocator::{MemoryAllocator, MemoryTypeFilter},
    },
    sync::Sharing,
};
use wgui::gfx::WGfx;
use wlx_capture::frame::{
    DRM_FORMAT_ABGR8888, DRM_FORMAT_ABGR2101010, DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR8888,
    DRM_FORMAT_XBGR2101010, DRM_FORMAT_XRGB8888, DmabufFrame, DrmFormat, FourCC,
};

pub const DRM_FORMAT_MOD_INVALID: u64 = 0xff_ffff_ffff_ffff;

pub trait WGfxDmabuf {
    fn dmabuf_texture_ex(
        &self,
        frame: DmabufFrame,
        tiling: ImageTiling,
        layouts: Vec<SubresourceLayout>,
        modifiers: &[u64],
    ) -> anyhow::Result<Arc<Image>>;

    fn dmabuf_texture(&self, frame: DmabufFrame) -> anyhow::Result<Arc<Image>>;
}

impl WGfxDmabuf for WGfx {
    fn dmabuf_texture_ex(
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
            .context("failed to get memory type index")?;

        debug_assert!(self.device.enabled_extensions().khr_external_memory_fd);
        debug_assert!(self.device.enabled_extensions().khr_external_memory);
        debug_assert!(self.device.enabled_extensions().ext_external_memory_dma_buf);

        // only do the 1st
        unsafe {
            let Some(fd) = frame.planes[0].fd else {
                anyhow::bail!("DMA-buf plane has no FD");
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
                    anyhow::bail!("Failed to bind memory to image: {}", e.0);
                }
            }
        }
    }

    fn dmabuf_texture(&self, frame: DmabufFrame) -> anyhow::Result<Arc<Image>> {
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
}

#[allow(clippy::all, clippy::pedantic)]
pub(super) unsafe fn create_dmabuf_image(
    device: Arc<Device>,
    create_info: ImageCreateInfo,
) -> Result<RawImage, VulkanError> {
    let &ImageCreateInfo {
        flags,
        image_type,
        format,
        ref view_formats,
        extent,
        array_layers,
        mip_levels,
        samples,
        tiling,
        usage,
        stencil_usage,
        ref sharing,
        initial_layout,
        ref drm_format_modifiers,
        ref drm_format_modifier_plane_layouts,
        external_memory_handle_types,
        _ne: _,
    } = &create_info;

    let (sharing_mode, queue_family_index_count, p_queue_family_indices) = match sharing {
        Sharing::Exclusive => (ash::vk::SharingMode::EXCLUSIVE, 0, &[] as _),
        Sharing::Concurrent(queue_family_indices) => (
            ash::vk::SharingMode::CONCURRENT,
            queue_family_indices.len() as u32,
            queue_family_indices.as_ptr(),
        ),
    };

    let mut create_info_vk = ash::vk::ImageCreateInfo {
        flags: flags.into(),
        image_type: image_type.into(),
        format: format.into(),
        extent: ash::vk::Extent3D {
            width: extent[0],
            height: extent[1],
            depth: extent[2],
        },
        mip_levels,
        array_layers,
        samples: samples.into(),
        tiling: tiling.into(),
        usage: usage.into(),
        sharing_mode,
        queue_family_index_count,
        p_queue_family_indices,
        initial_layout: initial_layout.into(),
        ..Default::default()
    };

    let mut drm_format_modifier_explicit_info_vk = None;
    let drm_format_modifier_plane_layouts_vk: SmallVec<[_; 4]>;
    let mut drm_format_modifier_list_info_vk = None;
    let mut external_memory_info_vk = None;
    let mut format_list_info_vk = None;
    let format_list_view_formats_vk: Vec<_>;
    let mut stencil_usage_info_vk = None;

    if drm_format_modifiers.len() == 1 {
        drm_format_modifier_plane_layouts_vk = drm_format_modifier_plane_layouts
            .iter()
            .map(|subresource_layout| {
                let &SubresourceLayout {
                    offset,
                    size,
                    row_pitch,
                    array_pitch,
                    depth_pitch,
                } = subresource_layout;

                ash::vk::SubresourceLayout {
                    offset,
                    size,
                    row_pitch,
                    array_pitch: array_pitch.unwrap_or(0),
                    depth_pitch: depth_pitch.unwrap_or(0),
                }
            })
            .collect();

        let next = drm_format_modifier_explicit_info_vk.insert(
            ash::vk::ImageDrmFormatModifierExplicitCreateInfoEXT {
                drm_format_modifier: drm_format_modifiers[0],
                drm_format_modifier_plane_count: drm_format_modifier_plane_layouts_vk.len() as u32,
                p_plane_layouts: drm_format_modifier_plane_layouts_vk.as_ptr(),
                ..Default::default()
            },
        );

        next.p_next = create_info_vk.p_next;
        create_info_vk.p_next = next as *const _ as *const _;
    } else if drm_format_modifiers.len() > 1 {
        let next = drm_format_modifier_list_info_vk.insert(
            ash::vk::ImageDrmFormatModifierListCreateInfoEXT {
                drm_format_modifier_count: drm_format_modifiers.len() as u32,
                p_drm_format_modifiers: drm_format_modifiers.as_ptr(),
                ..Default::default()
            },
        );

        next.p_next = create_info_vk.p_next;
        create_info_vk.p_next = next as *const _ as *const _;
    }

    if !external_memory_handle_types.is_empty() {
        let next = external_memory_info_vk.insert(ash::vk::ExternalMemoryImageCreateInfo {
            handle_types: external_memory_handle_types.into(),
            ..Default::default()
        });

        next.p_next = create_info_vk.p_next;
        create_info_vk.p_next = next as *const _ as *const _;
    }

    if !view_formats.is_empty() {
        format_list_view_formats_vk = view_formats
            .iter()
            .copied()
            .map(ash::vk::Format::from)
            .collect();

        let next = format_list_info_vk.insert(ash::vk::ImageFormatListCreateInfo {
            view_format_count: format_list_view_formats_vk.len() as u32,
            p_view_formats: format_list_view_formats_vk.as_ptr(),
            ..Default::default()
        });

        next.p_next = create_info_vk.p_next;
        create_info_vk.p_next = next as *const _ as *const _;
    }

    if let Some(stencil_usage) = stencil_usage {
        let next = stencil_usage_info_vk.insert(ash::vk::ImageStencilUsageCreateInfo {
            stencil_usage: stencil_usage.into(),
            ..Default::default()
        });

        next.p_next = create_info_vk.p_next;
        create_info_vk.p_next = next as *const _ as *const _;
    }

    unsafe {
        let handle = {
            let fns = device.fns();
            let mut output = MaybeUninit::uninit();
            (fns.v1_0.create_image)(
                device.handle(),
                &create_info_vk,
                std::ptr::null(),
                output.as_mut_ptr(),
            )
            .result()
            .map_err(VulkanError::from)?;
            output.assume_init()
        };

        RawImage::from_handle(device, handle, create_info)
    }
}

pub fn get_drm_formats(device: Arc<Device>) -> Vec<DrmFormat> {
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

pub fn fourcc_to_vk(fourcc: FourCC) -> anyhow::Result<Format> {
    match fourcc.value {
        DRM_FORMAT_ABGR8888 | DRM_FORMAT_XBGR8888 => Ok(Format::R8G8B8A8_UNORM),
        DRM_FORMAT_ARGB8888 | DRM_FORMAT_XRGB8888 => Ok(Format::B8G8R8A8_UNORM),
        DRM_FORMAT_ABGR2101010 | DRM_FORMAT_XBGR2101010 => Ok(Format::A2B10G10R10_UNORM_PACK32),
        _ => anyhow::bail!("Unsupported format {fourcc}"),
    }
}
