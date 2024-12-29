use wayvr_ipc::packet_server;

use crate::gen_id;

use super::display;

pub struct WayVRProcess {
    pub auth_key: String,
    pub child: std::process::Child,
    pub display_handle: display::DisplayHandle,

    pub exec_path: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub struct ExternalProcess {
    pub pid: u32,
    pub display_handle: display::DisplayHandle,
}

pub enum Process {
    Managed(WayVRProcess),     // Process spawned by WayVR
    External(ExternalProcess), // External process not directly controlled by us
}

impl Process {
    pub fn display_handle(&self) -> display::DisplayHandle {
        match self {
            Process::Managed(p) => p.display_handle,
            Process::External(p) => p.display_handle,
        }
    }

    pub fn is_running(&mut self) -> bool {
        match self {
            Process::Managed(p) => p.is_running(),
            Process::External(p) => p.is_running(),
        }
    }

    pub fn terminate(&mut self) {
        match self {
            Process::Managed(p) => p.terminate(),
            Process::External(p) => p.terminate(),
        }
    }

    pub fn to_packet(&self, handle: ProcessHandle) -> packet_server::Process {
        match self {
            Process::Managed(p) => packet_server::Process {
                name: p.get_name().unwrap_or(String::from("unknown")),
                display_handle: p.display_handle.as_packet(),
                handle: handle.as_packet(),
            },
            Process::External(p) => packet_server::Process {
                name: p.get_name().unwrap_or(String::from("unknown")),
                display_handle: p.display_handle.as_packet(),
                handle: handle.as_packet(),
            },
        }
    }
}

impl Drop for WayVRProcess {
    fn drop(&mut self) {
        log::info!(
            "Sending SIGTERM (graceful exit) to process {}",
            self.exec_path.as_str()
        );
        self.terminate();
    }
}

impl WayVRProcess {
    fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_exit_status)) => false,
            Ok(None) => true,
            Err(e) => {
                // this shouldn't happen
                log::error!("Child::try_wait failed: {}", e);
                false
            }
        }
    }

    fn terminate(&mut self) {
        unsafe {
            // Gracefully stop process
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
    }

    pub fn get_name(&self) -> Option<String> {
        get_exec_name_from_pid(self.child.id())
    }
}

fn get_exec_name_from_pid(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/exe", pid);
    match std::fs::read_link(&path) {
        Ok(buf) => {
            if let Some(process_name) = buf.file_name().and_then(|s| s.to_str()) {
                return Some(String::from(process_name));
            }
            None
        }
        Err(_) => None,
    }
}

impl ExternalProcess {
    fn is_running(&self) -> bool {
        if self.pid == 0 {
            false
        } else {
            std::fs::metadata(format!("/proc/{}", self.pid)).is_ok()
        }
    }

    fn terminate(&mut self) {
        if self.pid != 0 {
            unsafe {
                // send SIGINT (^C)
                libc::kill(self.pid as i32, libc::SIGINT);
            }
        }
        self.pid = 0;
    }

    pub fn get_name(&self) -> Option<String> {
        get_exec_name_from_pid(self.pid)
    }
}

gen_id!(ProcessVec, Process, ProcessCell, ProcessHandle);

pub fn find_by_pid(processes: &ProcessVec, pid: u32) -> Option<ProcessHandle> {
    for (idx, cell) in processes.vec.iter().enumerate() {
        if let Some(cell) = cell {
            match &cell.obj {
                Process::Managed(wayvr_process) => {
                    if wayvr_process.child.id() == pid {
                        return Some(ProcessVec::get_handle(cell, idx));
                    }
                }
                Process::External(external_process) => {
                    if external_process.pid == pid {
                        return Some(ProcessVec::get_handle(cell, idx));
                    }
                }
            }
        }
    }
    None
}

impl ProcessHandle {
    pub fn from_packet(handle: packet_server::ProcessHandle) -> Self {
        Self {
            generation: handle.generation,
            idx: handle.idx,
        }
    }

    pub fn as_packet(&self) -> packet_server::ProcessHandle {
        packet_server::ProcessHandle {
            idx: self.idx,
            generation: self.generation,
        }
    }
}
