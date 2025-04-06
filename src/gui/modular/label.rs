use chrono::Local;
use chrono_tz::Tz;
use glam::Vec4;
use smallvec::SmallVec;
use std::{
    io::Read,
    process::{self, Stdio},
    sync::Arc,
    time::Instant,
};

use crate::{
    gui::modular::FALLBACK_COLOR,
    overlays::toast::{error_toast, error_toast_str},
    state::AppState,
};

use serde::Deserialize;

use super::{color_parse_or_default, ExecArgs, GuiColor, ModularControl, ModularData};

#[derive(Deserialize)]
#[serde(untagged)]
pub enum TimezoneDef {
    Idx(usize),
    Str(Arc<str>),
}

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
        timezone: Option<TimezoneDef>,
    },
    Timezone {
        timezone: usize,
    },
    Timer {
        format: Arc<str>,
    },
    Battery {
        device: usize,
        low_threshold: f32,
        low_color: Arc<str>,
        charging_color: Arc<str>,
    },
    DragMultiplier,
    Ipd,
}

pub enum LabelData {
    Battery {
        device: usize,
        low_threshold: f32,
        normal_color: GuiColor,
        low_color: GuiColor,
        charging_color: GuiColor,
    },
    Clock {
        format: Arc<str>,
        timezone: Option<Tz>,
    },
    Timer {
        format: Arc<str>,
        start: Instant,
    },
    Exec {
        last_exec: Instant,
        interval: f32,
        command: Vec<Arc<str>>,
        child: Option<process::Child>,
    },
    Ipd {
        last_ipd: f32,
    },
    DragMultiplier,
}

pub fn modular_label_init(label: &mut ModularControl, content: &LabelContent, app: &AppState) {
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
            let tz_str = match timezone {
                Some(TimezoneDef::Idx(idx)) => {
                    if let Some(tz) = app.session.config.timezones.get(*idx) {
                        Some(tz.as_str())
                    } else {
                        log::error!("Timezone index out of range '{}'", idx);
                        label.set_fg_color(*FALLBACK_COLOR);
                        None
                    }
                }
                Some(TimezoneDef::Str(tz_str)) => Some(tz_str.as_ref()),
                None => None,
            };

            Some(LabelData::Clock {
                format: format.clone(),
                timezone: tz_str.and_then(|tz| {
                    tz.parse()
                        .map_err(|_| {
                            log::error!("Failed to parse timezone '{}'", &tz);
                            label.set_fg_color(*FALLBACK_COLOR);
                        })
                        .ok()
                }),
            })
        }
        LabelContent::Timezone { timezone } => {
            if let Some(tz) = app.session.config.timezones.get(*timezone) {
                let pretty_tz = tz.split('/').next_back().map(|x| x.replace("_", " "));

                if let Some(pretty_tz) = pretty_tz {
                    label.set_text(&pretty_tz);
                    return;
                } else {
                    log::error!("Timezone name not valid '{}'", &tz);
                }
            } else {
                log::error!("Timezone index out of range '{}'", &timezone);
            }
            label.set_fg_color(*FALLBACK_COLOR);
            label.set_text("Error");
            None
        }
        LabelContent::Timer { format } => Some(LabelData::Timer {
            format: format.clone(),
            start: Instant::now(),
        }),
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
        LabelContent::Ipd => Some(LabelData::Ipd { last_ipd: -1. }),
        LabelContent::DragMultiplier => Some(LabelData::DragMultiplier),
    };

    if let Some(state) = state {
        label.state = Some(ModularData::Label(Box::new(state)));
        label.on_update = Some(label_update);
    }
}

pub(super) fn label_update(control: &mut ModularControl, _: &mut (), app: &mut AppState) {
    // want panic
    let ModularData::Label(data) = control.state.as_mut().unwrap() else {
        panic!("Label control has no state");
    };
    match data.as_mut() {
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
                    .unwrap_or_else(|| ("".into(), Vec4::ZERO));

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
        LabelData::Timer { format, start } => {
            let mut format = format.clone().to_lowercase();
            let duration = start.elapsed().as_secs();
            format = format.replace("%s", &format!("{:02}", (duration % 60)));
            format = format.replace("%m", &format!("{:02}", ((duration / 60) % 60)));
            format = format.replace("%h", &format!("{:02}", ((duration / 60) / 60)));
            control.set_text(&format);
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
                            error_toast(
                                app,
                                "LabelData::Exec: Child process exited with code",
                                code,
                            );
                        } else {
                            if let Some(mut stdout) = proc.stdout.take() {
                                let mut buf = String::new();
                                if stdout.read_to_string(&mut buf).is_ok() {
                                    control.set_text(&buf);
                                } else {
                                    error_toast_str(
                                        app,
                                        "LabelData::Exec: Failed to read stdout for child process",
                                    );
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
                        error_toast(app, "Error checking child process", e);
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
                        error_toast(app, &format!("Failed to spawn process {:?}", args), e);
                    }
                };
            }
        }
        LabelData::Ipd { last_ipd } => {
            if (app.input_state.ipd - *last_ipd).abs() > 0.05 {
                *last_ipd = app.input_state.ipd;
                control.set_text(&format!("{:.1}", app.input_state.ipd));
            }
        }
        LabelData::DragMultiplier => {
            control.set_text(&format!("{:.1}", app.session.config.space_drag_multiplier));
        }
    }
}
