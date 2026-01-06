use std::rc::Rc;

use chrono::Local;
use chrono_tz::Tz;
use wgui::{
    drawing,
    event::{self, EventCallback},
    i18n::Translation,
    layout::Layout,
    parser::{CustomAttribsInfoOwned, parse_color_hex},
    widget::{EventResult, label::WidgetLabel},
};

use crate::state::AppState;

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
        let num_devices = app.input_state.devices.len();
        let suffix = if num_devices > 3 { "" } else { "%" };
        let text = format!("{soc}{suffix}");
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
