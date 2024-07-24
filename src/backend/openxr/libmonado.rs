use std::{env, ffi::c_void, fs};

use anyhow::bail;
use libloading::{Library, Symbol};
use serde::Deserialize;

#[repr(C)]
#[derive(Default, Debug)]
struct MndPose {
    orientation: [f32; 4],
    position: [f32; 3],
}

const MND_REFERENCE_TYPE_STAGE: i32 = 3;

const MND_SUCCESS: i32 = 0;
const MND_ERROR_BAD_SPACE_TYPE: i32 = -7;

type GetDeviceCount = extern "C" fn(*mut c_void, *mut u32) -> i32;
type GetDeviceInfo = extern "C" fn(*mut c_void, u32, *mut u32, *mut *const char) -> i32;
type GetDeviceFromRole = extern "C" fn(*mut c_void, *const std::os::raw::c_char, *mut i32) -> i32;
type GetDeviceBatteryStatus =
    extern "C" fn(*mut c_void, u32, *mut bool, *mut bool, *mut f32) -> i32;

type PlaySpaceMove = extern "C" fn(*mut c_void, f32, f32, f32) -> i32;
type ApplyStageOffset = extern "C" fn(*mut c_void, *const MndPose) -> i32;

// New implementation
type GetReferenceSpaceOffset = extern "C" fn(*mut c_void, i32, *mut MndPose) -> i32;
type SetReferenceSpaceOffset = extern "C" fn(*mut c_void, i32, *const MndPose) -> i32;

// TODO: Clean up after merge into upstream Monado
enum MoverImpl {
    None,
    PlaySpaceMove(PlaySpaceMove),
    ApplyStageOffset(ApplyStageOffset),
    SpaceOffsetApi {
        get_reference: GetReferenceSpaceOffset,
        set_reference: SetReferenceSpaceOffset,
    },
}

pub struct LibMonado {
    libmonado: Library,
    mnd_root: *mut c_void,
    mover: MoverImpl,
}

impl Drop for LibMonado {
    fn drop(&mut self) {
        unsafe {
            type RootDestroy = extern "C" fn(*mut *mut c_void) -> i32;
            let Ok(root_destroy) = self.libmonado.get::<RootDestroy>(b"mnd_root_destroy\0") else {
                return;
            };
            root_destroy(&mut self.mnd_root);
        }
    }
}

impl LibMonado {
    pub fn new() -> anyhow::Result<Self> {
        let lib_path = if let Ok(path) = env::var("LIBMONADO_PATH") {
            path
        } else if let Some(path) = xr_runtime_manifest()
            .map(|manifest| manifest.runtime.mnd_libmonado_path)
            .ok()
            .flatten()
        {
            path
        } else {
            bail!("Monado: libmonado not found. Update your Monado/WiVRn or set LIBMONADO_PATH to point at your libmonado.so");
        };

        let (libmonado, mnd_root) = unsafe {
            let libmonado = libloading::Library::new(lib_path)?;
            let root_create: Symbol<extern "C" fn(*mut *mut c_void) -> i32> =
                libmonado.get(b"mnd_root_create\0")?;

            let mut root: *mut c_void = std::ptr::null_mut();
            let ret = root_create(&mut root);
            if ret != 0 {
                anyhow::bail!("Failed to create libmonado root, code: {}", ret);
            }

            (libmonado, root)
        };

        let space_api = unsafe {
            if let (Ok(get_reference), Ok(set_reference)) = (
                libmonado.get(b"mnd_root_get_reference_space_offset\0"),
                libmonado.get(b"mnd_root_set_reference_space_offset\0"),
            ) {
                log::info!("Monado: using space offset API");

                let get_reference: GetReferenceSpaceOffset = *get_reference;
                let set_reference: SetReferenceSpaceOffset = *set_reference;

                MoverImpl::SpaceOffsetApi {
                    get_reference,
                    set_reference,
                }
            } else if let Ok(playspace_move) = libmonado.get(b"mnd_root_playspace_move\0") {
                log::warn!("Monado: using playspace_move, which is obsolete. Consider updating.");
                MoverImpl::PlaySpaceMove(*playspace_move)
            } else if let Ok(apply_stage_offset) = libmonado.get(b"mnd_root_apply_stage_offset\0") {
                log::warn!(
                    "Monado: using apply_stage_offset, which is obsolete. Consider updating."
                );
                MoverImpl::ApplyStageOffset(*apply_stage_offset)
            } else {
                MoverImpl::None
            }
        };

        Ok(Self {
            libmonado,
            mnd_root,
            mover: space_api,
        })
    }

    pub fn mover_supported(&self) -> bool {
        !matches!(self.mover, MoverImpl::None)
    }
}

#[derive(Deserialize)]
struct XrRuntimeManifestRuntime {
    name: String,
    library_path: String,
    mnd_libmonado_path: Option<String>,
}

#[derive(Deserialize)]
struct XrRuntimeManifest {
    file_format_version: String,
    runtime: XrRuntimeManifestRuntime,
}

fn xr_runtime_manifest() -> anyhow::Result<XrRuntimeManifest> {
    let xdg_dirs = xdg::BaseDirectories::new()?; // only fails if $HOME unset
    let mut file = xdg_dirs.get_config_home();
    file.push("openxr/1/active_runtime.json");

    let json = fs::read_to_string(file)?;
    let manifest = serde_json::from_str(&json)?;
    Ok(manifest)
}
