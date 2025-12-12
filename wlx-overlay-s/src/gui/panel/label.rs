use std::{
    cell::RefCell,
    fs,
    os::unix::fs::FileTypeExt,
    process::{Command, Stdio},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use chrono::Local;
use chrono_tz::Tz;
use interprocess::os::unix::fifo_file::create_fifo;
use wgui::{
    drawing,
    event::{self, EventCallback},
    i18n::Translation,
    layout::Layout,
    parser::{CustomAttribsInfoOwned, parse_color_hex},
    widget::{EventResult, label::WidgetLabel},
};

use crate::{gui::panel::helper::PipeReaderThread, state::AppState};

use super::helper::expand_env_vars;

#[allow(clippy::too_many_lines)]
pub(super) fn setup_custom_label<S: 'static>(
    layout: &mut Layout,
    attribs: &CustomAttribsInfoOwned,
    app: &AppState,
) {
    let Some(source) = attribs.get_value("_source") else {
        log::warn!("custom label with no source!");
        return;
    };

    let callback: EventCallback<AppState, S> = match source {
        "shell" => {
            let Some(exec) = attribs.get_value("_exec") else {
                log::warn!("label with shell source but no exec attribute!");
                return;
            };
            let state = ShellLabelState {
                exec: exec.to_string(),
                mut_state: RefCell::new(PipeLabelMutableState {
                    reader: None,
                    next_try: Instant::now(),
                }),
                carry_over: RefCell::new(None),
            };
            Box::new(move |common, data, _, _| {
                let _ = shell_on_tick(&state, common, data).inspect_err(|e| log::error!("{e:?}"));
                Ok(EventResult::Pass)
            })
        }
        "fifo" => {
            let Some(path) = attribs.get_value("_path") else {
                log::warn!("label with fifo source but no path attribute!");
                return;
            };
            let state = FifoLabelState {
                path: expand_env_vars(path).into(),
                carry_over: RefCell::new(None),
                mut_state: RefCell::new(PipeLabelMutableState {
                    reader: None,
                    next_try: Instant::now(),
                }),
            };
            Box::new(move |common, data, _, _| {
                fifo_on_tick(&state, common, data);
                Ok(EventResult::Pass)
            })
        }
        "battery" => {
            let Some(device) = attribs
                .get_value("_device")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                log::warn!("label with battery source but no device attribute!");
                return;
            };

            let state = BatteryLabelState {
                low_color: attribs
                    .get_value("_low_color")
                    .and_then(parse_color_hex)
                    .unwrap_or(BAT_LOW),
                normal_color: attribs
                    .get_value("_normal_color")
                    .and_then(parse_color_hex)
                    .unwrap_or(BAT_NORMAL),
                charging_color: attribs
                    .get_value("_charging_color")
                    .and_then(parse_color_hex)
                    .unwrap_or(BAT_CHARGING),
                low_threshold: attribs
                    .get_value("_low_threshold")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(BAT_LOW_THRESHOLD),
                device,
            };
            Box::new(move |common, data, app, _| {
                battery_on_tick(&state, common, data, app);
                Ok(EventResult::Pass)
            })
        }
        "clock" => {
            let Some(display) = attribs.get_value("_display") else {
                log::warn!("label with clock source but no display attribute!");
                return;
            };

            let format = match display {
                "name" => {
                    let maybe_pretty_tz = attribs
                        .get_value("_timezone")
                        .and_then(|tz| tz.parse::<usize>().ok())
                        .and_then(|tz_idx| app.session.config.timezones.get(tz_idx))
                        .and_then(|tz_name| {
                            tz_name.split('/').next_back().map(|x| x.replace('_', " "))
                        });

                    let pretty_tz = maybe_pretty_tz.as_ref().map_or("Local", |x| x.as_str());

                    let mut globals = layout.state.globals.get();
                    layout
                        .state
                        .widgets
                        .get_as::<WidgetLabel>(attribs.widget_id)
                        .unwrap()
                        .set_text_simple(&mut globals, Translation::from_raw_text(pretty_tz));

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
                .get_value("_timezone")
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

            Box::new(move |common, data, _, _| {
                clock_on_tick(&state, common, data);
                Ok(EventResult::Pass)
            })
        }
        "ipd" => Box::new(|common, data, app, _| {
            ipd_on_tick(common, data, app);
            Ok(EventResult::Pass)
        }),
        unk => {
            log::warn!("Unknown source value for label: {unk}");
            return;
        }
    };

    layout.add_event_listener(
        attribs.widget_id,
        wgui::event::EventListenerKind::InternalStateChange,
        callback,
    );
}

struct PipeLabelMutableState {
    reader: Option<PipeReaderThread>,
    next_try: Instant,
}

struct ShellLabelState {
    exec: String,
    mut_state: RefCell<PipeLabelMutableState>,
    carry_over: RefCell<Option<String>>,
}

fn shell_on_tick(
    state: &ShellLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) -> anyhow::Result<()> {
    let mut mut_state = state.mut_state.borrow_mut();

    if let Some(reader) = mut_state.reader.as_mut() {
        if let Some(text) = reader.get_last_line() {
            let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
            label.set_text(common, Translation::from_raw_text(&text));
        }

        if reader.is_finished() && !mut_state.reader.take().unwrap().check_success() {
            mut_state.next_try = Instant::now() + Duration::from_secs(15);
        }
        return Ok(());
    } else if mut_state.next_try > Instant::now() {
        return Ok(());
    }

    let child = Command::new("sh")
        .arg("-c")
        .arg(&state.exec)
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run shell script: '{}'", &state.exec))?;

    mut_state.reader = Some(PipeReaderThread::new_from_child(child));

    Ok(())
}

struct FifoLabelState {
    path: Arc<str>,
    mut_state: RefCell<PipeLabelMutableState>,
    carry_over: RefCell<Option<String>>,
}

impl FifoLabelState {
    fn try_remove_fifo(&self) -> anyhow::Result<()> {
        let meta = match fs::metadata(&*self.path) {
            Ok(meta) => meta,
            Err(e) => {
                if fs::exists(&*self.path).unwrap_or(true) {
                    anyhow::bail!("Could not stat existing file at {}: {e:?}", &self.path);
                }
                return Ok(());
            }
        };

        if !meta.file_type().is_fifo() {
            anyhow::bail!("Existing file at {} is not a FIFO", &self.path);
        }

        if let Err(e) = fs::remove_file(&*self.path) {
            anyhow::bail!("Unable to remove existing FIFO at {}: {e:?}", &self.path);
        }

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

fn fifo_on_tick(
    state: &FifoLabelState,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    let mut mut_state = state.mut_state.borrow_mut();

    let Some(reader) = mut_state.reader.as_mut() else {
        if mut_state.next_try > Instant::now() {
            return;
        }

        if let Err(e) = state.try_remove_fifo() {
            mut_state.next_try = Instant::now() + Duration::from_secs(15);
            log::warn!("Requested FIFO path is taken: {e:?}");
            return;
        }

        if let Err(e) = create_fifo(&*state.path, 0o777) {
            mut_state.next_try = Instant::now() + Duration::from_secs(15);
            log::warn!("Failed to create FIFO: {e:?}");
            return;
        }

        mut_state.reader = Some(PipeReaderThread::new_from_fifo(state.path.clone()));
        return;
    };

    if let Some(text) = reader.get_last_line() {
        let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
        label.set_text(common, Translation::from_raw_text(&text));
    }

    if reader.is_finished() && !mut_state.reader.take().unwrap().check_success() {
        mut_state.next_try = Instant::now() + Duration::from_secs(15);
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
    let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();

    if let Some(device) = device
        && let Some(soc) = device.soc
    {
        let soc = (soc * 100.).min(99.) as u32;
        let text = soc.to_string();
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
