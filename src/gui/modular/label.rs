use chrono::Local;
use chrono_tz::Tz;
use glam::Vec3;
use smallvec::SmallVec;
use std::{
    io::Read,
    process::{self, Stdio},
    sync::Arc,
    time::Instant,
};

use crate::{gui::modular::FALLBACK_COLOR, state::AppState};

use serde::Deserialize;

use super::{color_parse_or_default, ExecArgs, ModularControl, ModularData};

#[derive(Deserialize)]
#[serde(tag = "source")]
pub enum LabelContent {
    Static {
        text: Arc<str>,
    },
    Exec {
        command: ExecArgs,
        interval: f32,
    },
    Clock {
        format: Arc<str>,
        timezone: Option<Arc<str>>,
    },
    Battery {
        device: usize,
        low_threshold: f32,
        low_color: Arc<str>,
        charging_color: Arc<str>,
    },
}

pub enum LabelData {
    Battery {
        device: usize,
        low_threshold: f32,
        normal_color: Vec3,
        low_color: Vec3,
        charging_color: Vec3,
    },
    Clock {
        format: Arc<str>,
        timezone: Option<Tz>,
    },
    Exec {
        last_exec: Instant,
        interval: f32,
        command: Vec<Arc<str>>,
        child: Option<process::Child>,
    },
}

pub fn modular_label_init(label: &mut ModularControl, content: &LabelContent) {
    let state = match content {
        LabelContent::Battery {
            device,
            low_threshold,
            low_color,
            charging_color,
        } => Some(LabelData::Battery {
            device: *device,
            low_threshold: *low_threshold,
            normal_color: label.fg_color,
            low_color: color_parse_or_default(low_color),
            charging_color: color_parse_or_default(charging_color),
        }),
        LabelContent::Clock { format, timezone } => {
            let tz: Option<Tz> = timezone.as_ref().map(|tz| {
                tz.parse().unwrap_or_else(|_| {
                    log::error!("Failed to parse timezone '{}'", &tz);
                    label.set_fg_color(FALLBACK_COLOR);
                    Tz::UTC
                })
            });

            Some(LabelData::Clock {
                format: format.clone(),
                timezone: tz,
            })
        }
        LabelContent::Exec { command, interval } => Some(LabelData::Exec {
            last_exec: Instant::now(),
            interval: *interval,
            command: command.clone(),
            child: None,
        }),
        LabelContent::Static { text } => {
            label.set_text(text);
            None
        }
    };

    if let Some(state) = state {
        label.state = Some(ModularData::Label(state));
        label.on_update = Some(label_update);
    }
}

pub(super) fn label_update(control: &mut ModularControl, _: &mut (), app: &mut AppState) {
    let ModularData::Label(data) = control.state.as_mut().unwrap() else {
        panic!("Label control has no state");
    };
    match data {
        LabelData::Battery {
            device,
            low_threshold,
            normal_color,
            low_color,
            charging_color,
        } => {
            let device = app.input_state.devices.get(*device);

            let tags = ["", "H", "L", "R", "T"];

            if let Some(device) = device {
                let (text, color) = device
                    .soc
                    .map(|soc| {
                        let text = format!(
                            "{}{}",
                            tags[device.role as usize],
                            (soc * 100.).min(99.) as u32
                        );
                        let color = if device.charging {
                            *charging_color
                        } else if soc < *low_threshold {
                            *low_color
                        } else {
                            *normal_color
                        };
                        (text, color)
                    })
                    .unwrap_or_else(|| ("".into(), Vec3::ZERO));

                control.set_text(&text);
                control.set_fg_color(color);
            } else {
                control.set_text("");
            }
        }
        LabelData::Clock { format, timezone } => {
            let format = format.clone();
            if let Some(tz) = timezone {
                let date = Local::now().with_timezone(tz);
                control.set_text(&format!("{}", &date.format(&format)));
            } else {
                let date = Local::now();
                control.set_text(&format!("{}", &date.format(&format)));
            }
        }
        LabelData::Exec {
            last_exec,
            interval,
            command,
            child,
        } => {
            if let Some(mut proc) = child.take() {
                match proc.try_wait() {
                    Ok(Some(code)) => {
                        if !code.success() {
                            log::error!("Child process exited with code: {}", code);
                        } else {
                            if let Some(mut stdout) = proc.stdout.take() {
                                let mut buf = String::new();
                                if stdout.read_to_string(&mut buf).is_ok() {
                                    control.set_text(&buf);
                                } else {
                                    log::error!("Failed to read stdout for child process");
                                    return;
                                }
                                return;
                            }
                            log::error!("No stdout for child process");
                            return;
                        }
                    }
                    Ok(None) => {
                        *child = Some(proc);
                        // not exited yet
                        return;
                    }
                    Err(e) => {
                        *child = None;
                        log::error!("Error checking child process: {:?}", e);
                        return;
                    }
                }
            }

            if Instant::now()
                .saturating_duration_since(*last_exec)
                .as_secs_f32()
                > *interval
            {
                *last_exec = Instant::now();
                let args = command
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<SmallVec<[&str; 8]>>();

                match process::Command::new(args[0])
                    .args(&args[1..])
                    .stdout(Stdio::piped())
                    .spawn()
                {
                    Ok(proc) => {
                        *child = Some(proc);
                    }
                    Err(e) => {
                        log::error!("Failed to spawn process {:?}: {:?}", args, e);
                    }
                };
            }
        }
    }
}
