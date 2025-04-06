use std::{mem::MaybeUninit, sync::Arc};

use smallvec::SmallVec;
use vulkano::{
    device::Device,
    image::{sys::RawImage, ImageCreateInfo, SubresourceLayout},
    sync::Sharing,
    VulkanError, VulkanObject,
};

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
