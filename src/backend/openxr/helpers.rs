use std::path::PathBuf;

use anyhow::{bail, ensure};
use glam::{Affine3A, Quat, Vec3, Vec3A};
use openxr as xr;
use xr::OverlaySessionCreateFlagsEXTX;

pub(super) fn init_xr() -> Result<(xr::Instance, xr::SystemId), anyhow::Error> {
    let entry = xr::Entry::linked();

    let Ok(available_extensions) = entry.enumerate_extensions() else {
        bail!("Failed to enumerate OpenXR extensions.");
    };
    ensure!(
        available_extensions.khr_vulkan_enable2,
        "Missing KHR_vulkan_enable2 extension."
    );
    ensure!(
        available_extensions.extx_overlay,
        "Missing EXTX_overlay extension."
    );

    let mut enabled_extensions = xr::ExtensionSet::default();
    enabled_extensions.khr_vulkan_enable2 = true;
    enabled_extensions.extx_overlay = true;
    if available_extensions.khr_binding_modification && available_extensions.ext_dpad_binding {
        enabled_extensions.khr_binding_modification = true;
        enabled_extensions.ext_dpad_binding = true;
    } else {
        log::warn!("Missing EXT_dpad_binding extension.");
    }
    if available_extensions.ext_hp_mixed_reality_controller {
        enabled_extensions.ext_hp_mixed_reality_controller = true;
    } else {
        log::warn!("Missing EXT_hp_mixed_reality_controller extension.");
    }

    //#[cfg(not(debug_assertions))]
    let layers = [];
    //#[cfg(debug_assertions)]
    //let layers = [
    //    "XR_APILAYER_LUNARG_api_dump",
    //    "XR_APILAYER_LUNARG_standard_validation",
    //];

    let Ok(xr_instance) = entry.create_instance(
        &xr::ApplicationInfo {
            application_name: "wlx-overlay-s",
            application_version: 0,
            engine_name: "wlx-overlay-s",
            engine_version: 0,
        },
        &enabled_extensions,
        &layers,
    ) else {
        bail!("Failed to create OpenXR instance.");
    };

    let Ok(instance_props) = xr_instance.properties() else {
        bail!("Failed to query OpenXR instance properties.");
    };
    log::info!(
        "Using OpenXR runtime: {} {}",
        instance_props.runtime_name,
        instance_props.runtime_version
    );

    let Ok(system) = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY) else {
        bail!("Failed to access OpenXR HMD system.");
    };

    let vk_target_version_xr = xr::Version::new(1, 1, 0);

    let Ok(reqs) = xr_instance.graphics_requirements::<xr::Vulkan>(system) else {
        bail!("Failed to query OpenXR Vulkan requirements.");
    };

    if vk_target_version_xr < reqs.min_api_version_supported
        || vk_target_version_xr.major() > reqs.max_api_version_supported.major()
    {
        bail!(
            "OpenXR runtime requires Vulkan version > {}, < {}.0.0",
            reqs.min_api_version_supported,
            reqs.max_api_version_supported.major() + 1
        );
    }

    Ok((xr_instance, system))
}
pub(super) unsafe fn create_overlay_session(
    instance: &xr::Instance,
    system: xr::SystemId,
    info: &xr::vulkan::SessionCreateInfo,
) -> Result<xr::sys::Session, xr::sys::Result> {
    let overlay = xr::sys::SessionCreateInfoOverlayEXTX {
        ty: xr::sys::SessionCreateInfoOverlayEXTX::TYPE,
        next: std::ptr::null(),
        create_flags: OverlaySessionCreateFlagsEXTX::EMPTY,
        session_layers_placement: 5,
    };
    let binding = xr::sys::GraphicsBindingVulkanKHR {
        ty: xr::sys::GraphicsBindingVulkanKHR::TYPE,
        next: &overlay as *const _ as *const _,
        instance: info.instance,
        physical_device: info.physical_device,
        device: info.device,
        queue_family_index: info.queue_family_index,
        queue_index: info.queue_index,
    };
    let info = xr::sys::SessionCreateInfo {
        ty: xr::sys::SessionCreateInfo::TYPE,
        next: &binding as *const _ as *const _,
        create_flags: Default::default(),
        system_id: system,
    };
    let mut out = xr::sys::Session::NULL;
    let x = (instance.fp().create_session)(instance.as_raw(), &info, &mut out);
    if x.into_raw() >= 0 {
        Ok(out)
    } else {
        Err(x)
    }
}

pub(super) fn hmd_pose_from_views(views: &[xr::View]) -> (Affine3A, f32) {
    let ipd;
    let pos = {
        let pos0: Vec3 = unsafe { std::mem::transmute(views[0].pose.position) };
        let pos1: Vec3 = unsafe { std::mem::transmute(views[1].pose.position) };
        ipd = (pos0.distance(pos1) * 1000.0).round() * 0.1;
        (pos0 + pos1) * 0.5
    };
    let rot = {
        let rot0: Quat = unsafe { std::mem::transmute(views[0].pose.orientation) };
        let rot1: Quat = unsafe { std::mem::transmute(views[1].pose.orientation) };
        rot0.lerp(rot1, 0.5)
    };

    (Affine3A::from_rotation_translation(rot, pos), ipd)
}

pub(super) fn transform_to_norm_quat(transform: &Affine3A) -> Quat {
    let norm_mat3 = transform
        .matrix3
        .mul_scalar(1.0 / transform.matrix3.x_axis.length());
    Quat::from_mat3a(&norm_mat3).normalize()
}

pub(super) fn translation_rotation_to_posef(translation: Vec3A, mut rotation: Quat) -> xr::Posef {
    if !rotation.is_finite() {
        rotation = Quat::IDENTITY;
    }

    xr::Posef {
        orientation: xr::Quaternionf {
            x: rotation.x,
            y: rotation.y,
            z: rotation.z,
            w: rotation.w,
        },
        position: xr::Vector3f {
            x: translation.x,
            y: translation.y,
            z: translation.z,
        },
    }
}

pub(super) fn transform_to_posef(transform: &Affine3A) -> xr::Posef {
    let translation = transform.translation;
    let rotation = transform_to_norm_quat(transform);
    translation_rotation_to_posef(translation, rotation)
}

pub(super) fn find_libmonado() -> Result<libloading::Library, anyhow::Error> {
    //query active linux processes
    let output = std::process::Command::new("ps")
        .arg("aux")
        .output()?
        .stdout;

    //find monado-service file location
    let mut monado_service = None;
    let lines = String::from_utf8(output)?;
    for line in lines.lines() {
        if line.contains("monado-service") {
            let mut parts = line.split_whitespace();
            monado_service = parts.nth(8);
            break;
        }
    }

    //if monado-service is not found, return error
    let monado_service = monado_service.ok_or_else(|| anyhow::anyhow!("monado-service not found"))?;

    log::info!("monado-service: {}", monado_service);

    //monado service is in /bin. go one folder up and look in /lib for libmonado.so
    let mut libmonado = std::path::Path::new(monado_service)
        .parent()
        .ok_or_else(|| anyhow::anyhow!("monado-service path has no parent"))?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("monado-service folder has no parent"))?
        .join("lib")
        .join("libmonado.so");

    if !libmonado.exists() {
        log::info!("libmonado.so wasn't found automatically. falling back to environment variable...");
        libmonado = PathBuf::from(std::env::var("LIBMONADO_PATH")?);
    }

    //load libmonado.so
    let libmonado = unsafe {
        libloading::Library::new(libmonado)?
    };
    Ok(libmonado)
}
