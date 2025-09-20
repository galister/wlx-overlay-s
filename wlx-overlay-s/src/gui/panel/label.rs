use std::{
    cell::RefCell,
    fs, io,
    os::unix::fs::FileTypeExt,
    process::{Child, ChildStdout, Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

use chrono::Local;
use chrono_tz::Tz;
use interprocess::os::unix::fifo_file::create_fifo;
use wgui::{
    drawing,
    event::{self, EventCallback, EventListenerCollection, ListenerHandleVec},
    i18n::Translation,
    layout::Layout,
    parser::{parse_color_hex, CustomAttribsInfoOwned},
    widget::label::WidgetLabel,
};

use crate::state::AppState;

use super::helper::{expand_env_vars, read_label_from_pipe};

pub(super) fn setup_custom_label<S>(
    layout: &mut Layout,
    attribs: &CustomAttribsInfoOwned,
    listeners: &mut EventListenerCollection<AppState, S>,
    listener_handles: &mut ListenerHandleVec,
    app: &AppState,
) {
    let Some(source) = attribs.get_value("source") else {
        log::warn!("custom label with no source!");
        return;
    };

    let callback: EventCallback<AppState, S> = match source {
        "shell" => {
            let Some(exec) = attribs.get_value("exec") else {
                log::warn!("label with shell source but no exec attribute!");
                return;
            };
            let state = ShellLabelState {
                exec: exec.to_string(),
                mut_state: RefCell::new(ShellLabelMutableState {
                    child: None,
                    reader: None,
                    next_try: Instant::now(),
                }),
                carry_over: RefCell::new(None),
            };
            Box::new(move |common, data, _app, _| {
                shell_on_tick(&state, common, data);
                Ok(())
            })
        }
        "fifo" => {
            let Some(path) = attribs.get_value("path") else {
                log::warn!("label with fifo source but no path attribute!");
                return;
            };
            let state = FifoLabelState {
                path: expand_env_vars(path),
                carry_over: RefCell::new(None),
                mut_state: RefCell::new(FifoLabelMutableState {
                    reader: None,
                    next_try: Instant::now(),
                }),
            };
            Box::new(move |common, data, _app, _| {
                pipe_on_tick(&state, common, data);
                Ok(())
            })
        }
        "battery" => {
            let Some(device) = attribs
                .get_value("device")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                log::warn!("label with battery source but no device attribute!");
                return;
            };

            let state = BatteryLabelState {
                low_color: attribs
                    .get_value("low_color")
                    .and_then(|s| parse_color_hex(s))
                    .unwrap_or(BAT_LOW),
                normal_color: attribs
                    .get_value("normal_color")
                    .and_then(|s| parse_color_hex(s))
                    .unwrap_or(BAT_NORMAL),
                charging_color: attribs
                    .get_value("charging_color")
                    .and_then(|s| parse_color_hex(s))
                    .unwrap_or(BAT_CHARGING),
                low_threshold: attribs
                    .get_value("low_threshold")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(BAT_LOW_THRESHOLD),
                device,
            };
            Box::new(move |common, data, app, _| {
                battery_on_tick(&state, common, data, app);
                Ok(())
            })
        }
        "clock" => {
            let Some(display) = attribs.get_value("display") else {
                log::warn!("label with clock source but no display attribute!");
                return;
            };

            let format = match display {
                "name" => {
                    let maybe_pretty_tz = attribs
                        .get_value("timezone")
                        .and_then(|tz| tz.parse::<usize>().ok())
                        .and_then(|tz_idx| app.session.config.timezones.get(tz_idx))
                        .and_then(|tz_name| {
                            tz_name.split('/').next_back().map(|x| x.replace('_', " "))
                        });

                    let pretty_tz = match maybe_pretty_tz.as_ref() {
                        Some(x) => x.as_str(),
                        None => "Local",
                    };

                    let mut i18n = layout.state.globals.i18n();
                    layout
                        .state
                        .widgets
                        .get_as::<WidgetLabel>(attribs.widget_id)
                        .unwrap()
                        .set_text_simple(&mut *i18n, Translation::from_raw_text(&pretty_tz));

                    // does not need to be dynamic
                    return;
                }
                "date" => "%x",
                "dow" => "%A",
                "time" => {
                    if app.session.config.clock_12h {
                        "%I:%M %p"
                    } else {
                        "%H:%M"
                    }
                }
                unk => {
                    log::warn!("Unknown display value for clock label source: {unk}");
                    return;
                }
            };

            let tz_str = attribs
                .get_value("timezone")
                .and_then(|tz| tz.parse::<usize>().ok())
                .and_then(|tz_idx| app.session.config.timezones.get(tz_idx));

            let state = ClockLabelState {
                timezone: tz_str.and_then(|tz| {
                    tz.parse()
                        .inspect_err(|e| log::warn!("Invalid timezone: {e:?}"))
                        .ok()
                }),
                format: format.into(),
            };

            Box::new(move |common, data, _app, _| {
                clock_on_tick(&state, common, data);
                Ok(())
            })
        }
        "ipd" => Box::new(|common, data, app, _| {
            ipd_on_tick(common, data, app);
            Ok(())
        }),
        unk => {
            log::warn!("Unknown source value for label: {unk}");
            return;
        }
    };

    listeners.register(
        listener_handles,
        attribs.widget_id,
        wgui::event::EventListenerKind::InternalStateChange,
        callback,
    );
}

struct ShellLabelMutableState {
    child: Option<Child>,
    reader: Option<io::BufReader<ChildStdout>>,
    next_try: Instant,
}

struct ShellLabelState {
    exec: String,
    mut_state: RefCell<ShellLabelMutableState>,
    carry_over: RefCell<Option<String>>,
}

fn shell_on_tick(
    state: &ShellLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    let mut mut_state = state.mut_state.borrow_mut();

    if let Some(mut child) = mut_state.child.take() {
        match child.try_wait() {
            // not exited yet
            Ok(None) => {
                if let Some(text) = mut_state.reader.as_mut().and_then(|r| {
                    read_label_from_pipe("child process", r, &mut *state.carry_over.borrow_mut())
                }) {
                    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
                    label.set_text(common, Translation::from_raw_text(&text));
                }
                mut_state.child = Some(child);
                return;
            }
            // exited successfully
            Ok(Some(code)) if code.success() => {
                if let Some(text) = mut_state.reader.as_mut().and_then(|r| {
                    read_label_from_pipe("child process", r, &mut *state.carry_over.borrow_mut())
                }) {
                    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
                    label.set_text(common, Translation::from_raw_text(&text));
                }
                mut_state.child = None;
                return;
            }
            // exited with failure
            Ok(Some(code)) => {
                mut_state.child = None;
                mut_state.next_try = Instant::now() + Duration::from_secs(15);
                log::warn!("Label process exited with code {}", code);
                return;
            }
            // lost
            Err(_) => {
                mut_state.child = None;
                mut_state.next_try = Instant::now() + Duration::from_secs(15);
                log::warn!("Label child process lost.");
                return;
            }
        }
    } else {
        if mut_state.next_try > Instant::now() {
            return;
        }
    }

    match Command::new("sh")
        .arg("-c")
        .arg(&state.exec)
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            let stdout = child.stdout.take().unwrap();
            mut_state.child = Some(child);
            mut_state.reader = Some(io::BufReader::new(stdout));
        }
        Err(e) => {
            log::warn!("Failed to run shell script '{}': {e:?}", &state.exec)
        }
    }
}

struct FifoLabelMutableState {
    reader: Option<io::BufReader<fs::File>>,
    next_try: Instant,
}

struct FifoLabelState {
    path: String,
    mut_state: RefCell<FifoLabelMutableState>,
    carry_over: RefCell<Option<String>>,
}

impl FifoLabelState {
    fn try_remove_fifo(&self) -> anyhow::Result<()> {
        let meta = match fs::metadata(&self.path) {
            Ok(meta) => meta,
            Err(e) => {
                if fs::exists(&self.path).unwrap_or(true) {
                    anyhow::bail!("Could not stat existing file at {}: {e:?}", &self.path);
                }
                return Ok(());
            }
        };

        if !meta.file_type().is_fifo() {
            anyhow::bail!("Existing file at {} is not a FIFO", &self.path);
        }

        if let Err(e) = fs::remove_file(&self.path) {
            anyhow::bail!("Unable to remove existing FIFO at {}: {e:?}", &self.path);
        };

        Ok(())
    }
}

impl Drop for FifoLabelState {
    fn drop(&mut self) {
        if let Err(e) = self.try_remove_fifo() {
            log::debug!("{e:?}");
        }
    }
}

fn pipe_on_tick(
    state: &FifoLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    let mut mut_state = state.mut_state.borrow_mut();

    let reader = match mut_state.reader.as_mut() {
        Some(f) => f,
        None => {
            if mut_state.next_try > Instant::now() {
                return;
            }

            if let Err(e) = state.try_remove_fifo() {
                mut_state.next_try = Instant::now() + Duration::from_secs(15);
                log::warn!("Requested FIFO path is taken: {e:?}");
                return;
            }

            if let Err(e) = create_fifo(&state.path, 0o777) {
                mut_state.next_try = Instant::now() + Duration::from_secs(15);
                log::warn!("Failed to create FIFO: {e:?}");
                return;
            }

            mut_state.reader = fs::File::open(&state.path)
                .inspect_err(|e| {
                    log::warn!("Failed to open FIFO: {e:?}");
                    mut_state.next_try = Instant::now() + Duration::from_secs(15);
                })
                .map(|f| io::BufReader::new(f))
                .ok();

            mut_state.reader.as_mut().unwrap()
        }
    };

    if let Some(text) =
        read_label_from_pipe(&state.path, reader, &mut *state.carry_over.borrow_mut())
    {
        let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
        label.set_text(common, Translation::from_raw_text(&text));
    }
}

const BAT_LOW: drawing::Color = drawing::Color::new(0.69, 0.38, 0.38, 1.);
const BAT_NORMAL: drawing::Color = drawing::Color::new(0.55, 0.84, 0.79, 1.);
const BAT_CHARGING: drawing::Color = drawing::Color::new(0.38, 0.50, 0.62, 1.);
const BAT_LOW_THRESHOLD: u32 = 30;

struct BatteryLabelState {
    device: usize,
    low_color: drawing::Color,
    normal_color: drawing::Color,
    charging_color: drawing::Color,
    low_threshold: u32,
}

fn battery_on_tick(
    state: &BatteryLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
    app: &AppState,
) {
    let device = app.input_state.devices.get(state.device);

    let tags = ["", "H", "L", "R", "T"];

    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();

    if let Some(device) = device {
        if let Some(soc) = device.soc {
            let soc = (soc * 100.).min(99.) as u32;
            let text = format!("{}{}", tags[device.role as usize], soc);
            let color = if device.charging {
                state.charging_color
            } else if soc < state.low_threshold {
                state.low_color
            } else {
                state.normal_color
            };
            label.set_color(common, color, false);
            label.set_text(common, Translation::from_raw_text(&text));
            return;
        }
    }
    label.set_text(common, Translation::default());
}

struct ClockLabelState {
    timezone: Option<Tz>,
    format: Rc<str>,
}

fn clock_on_tick(
    state: &ClockLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    let date_time = state.timezone.as_ref().map_or_else(
        || format!("{}", Local::now().format(&state.format)),
        |tz| format!("{}", Local::now().with_timezone(tz).format(&state.format)),
    );

    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
    label.set_text(common, Translation::from_raw_text(&date_time));
}

fn ipd_on_tick(
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
    app: &AppState,
) {
    let text = app.input_state.ipd.to_string();
    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
    label.set_text(common, Translation::from_raw_text(&text));
}
