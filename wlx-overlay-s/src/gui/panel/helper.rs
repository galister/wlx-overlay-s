use regex::Regex;
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    process::Child,
    sync::{
        Arc, LazyLock,
        mpsc::{self, Receiver},
    },
    thread::JoinHandle,
};

static ENV_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)}|\$([A-Z_][A-Z0-9_]*)").unwrap() // want panic
});

pub(super) fn expand_env_vars(template: &str) -> String {
    ENV_VAR_REGEX
        .replace_all(template, |caps: &regex::Captures| {
            let var_name = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
            std::env::var(var_name)
                .inspect_err(|e| log::warn!("Unable to substitute env var {var_name}: {e:?}"))
                .unwrap_or_default()
        })
        .into_owned()
}

pub(super) struct PipeReaderThread {
    receiver: Receiver<String>,
    handle: JoinHandle<bool>,
}

impl PipeReaderThread {
    pub fn new_from_child(mut c: Child) -> Self {
        const BUF_LEN: usize = 128;
        let (sender, receiver) = mpsc::sync_channel::<String>(4);

        let handle = std::thread::spawn({
            move || {
                let stdout = c.stdout.take().unwrap();
                let mut reader = BufReader::new(stdout);

                loop {
                    let mut buf = String::with_capacity(BUF_LEN);
                    match reader.read_line(&mut buf) {
                        Ok(0) => {
                            // EOF reached
                            break;
                        }
                        Ok(_) => {
                            let _ = sender.try_send(buf);
                        }
                        Err(e) => {
                            log::error!("Error reading pipe: {e:?}");
                            break;
                        }
                    }
                }
                c.wait()
                    .inspect_err(|e| log::error!("Failed to wait for child process: {e:?}"))
                    .is_ok_and(|c| c.success())
            }
        });

        Self { receiver, handle }
    }

    pub fn new_from_fifo(path: Arc<str>) -> Self {
        const BUF_LEN: usize = 128;
        let (sender, receiver) = mpsc::sync_channel::<String>(4);

        let handle = std::thread::spawn({
            move || {
                let Ok(mut reader) = fs::File::open(&*path)
                    .inspect_err(|e| {
                        log::warn!("Failed to open fifo: {e:?}");
                    })
                    .map(|r| BufReader::new(r))
                else {
                    return false;
                };

                loop {
                    let mut buf = String::with_capacity(BUF_LEN);
                    match reader.read_line(&mut buf) {
                        Ok(0) => {
                            // EOF reached
                            break;
                        }
                        Ok(_) => {
                            let _ = sender.try_send(buf);
                        }
                        Err(e) => {
                            log::error!("Error reading fifo: {e:?}");
                            break;
                        }
                    }
                }

                true
            }
        });

        Self { receiver, handle }
    }

    pub fn get_last_line(&mut self) -> Option<String> {
        self.receiver.try_iter().last()
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub fn check_success(self) -> bool {
        self.handle.join().unwrap_or(false)
    }
}
