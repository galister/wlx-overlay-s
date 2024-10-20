use crate::gen_id;

use super::display;

pub struct Process {
    pub auth_key: String,
    pub child: std::process::Child,
    pub display_handle: display::DisplayHandle,

    pub exec_path: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Drop for Process {
    fn drop(&mut self) {
        log::info!(
            "Sending SIGTERM (graceful exit) to process {}",
            self.exec_path.as_str()
        );
        self.terminate();
    }
}

impl Process {
    pub fn is_running(&mut self) -> bool {
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

    pub fn terminate(&mut self) {
        unsafe {
            // Gracefully stop process
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
    }
}

gen_id!(ProcessVec, Process, ProcessCell, ProcessHandle);

pub fn find_by_pid(processes: &ProcessVec, pid: u32) -> Option<ProcessHandle> {
    for (idx, cell) in processes.vec.iter().enumerate() {
        if let Some(cell) = cell {
            if cell.obj.child.id() == pid {
                return Some(ProcessVec::get_handle(cell, idx));
            }
        }
    }
    None
}
