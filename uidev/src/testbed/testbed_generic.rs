use std::rc::Rc;

use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::{
    components::{
        Component,
        button::{ButtonClickCallback, ComponentButton},
        checkbox::ComponentCheckbox,
    },
    drawing::Color,
    event::EventListenerCollection,
    globals::WguiGlobals,
    i18n::Translation,
    layout::{Layout, Widget},
    parser::{ParseDocumentExtra, ParseDocumentParams, ParserState},
    widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

pub struct TestbedGeneric {
    pub layout: Layout,

    #[allow(dead_code)]
    state: ParserState,
}

fn button_click_callback(
    button: Component,
    label: Widget,
    text: &'static str,
) -> ButtonClickCallback {
    Box::new(move |e| {
        label.get_as_mut::<WidgetLabel>().set_text(
            &mut e.state.globals.i18n(),
            Translation::from_raw_text(text),
        );

        button.try_cast::<ComponentButton>()?.set_text(
            e.state,
            e.alterables,
            Translation::from_raw_text("this button has been clicked"),
        );

        Ok(())
    })
}

fn handle_button_click(button: Rc<ComponentButton>, label: Widget, text: &'static str) {
    button.on_click(button_click_callback(
        Component(button.clone()),
        label,
        text,
    ));
}

impl TestbedGeneric {
    pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
        const XML_PATH: &str = "gui/various_widgets.xml";

        let globals = WguiGlobals::new(Box::new(assets::Asset {}))?;

        let extra = ParseDocumentExtra {
            on_custom_attrib: Some(Box::new(move |par| {
                if par.attrib == "my_custom" {
                    let mut rect = par.get_widget_as::<WidgetRectangle>().unwrap();
                    rect.params.color = match par.value {
                        "red" => Color::new(1.0, 0.0, 0.0, 1.0),
                        "green" => Color::new(0.0, 1.0, 0.0, 1.0),
                        "blue" => Color::new(0.0, 0.0, 1.0, 1.0),
                        _ => Color::new(1.0, 1.0, 1.0, 1.0),
                    }
                }
            })),
            dev_mode: false,
        };

        let (layout, state) = wgui::parser::new_layout_from_assets(
            listeners,
            &ParseDocumentParams {
                globals,
                path: XML_PATH,
                extra,
            },
        )?;

        let label_cur_option = state.fetch_widget(&layout.state, "label_current_option")?;

        let button_click_me = state.fetch_component_as::<ComponentButton>("button_click_me")?;
        let button = button_click_me.clone();
        button_click_me.on_click(Box::new(move |e| {
            button.set_text(
                e.state,
                e.alterables,
                Translation::from_raw_text("congrats!"),
            );
            Ok(())
        }));

        let button_red = state.fetch_component_as::<ComponentButton>("button_red")?;
        let button_aqua = state.fetch_component_as::<ComponentButton>("button_aqua")?;
        let button_yellow = state.fetch_component_as::<ComponentButton>("button_yellow")?;

        handle_button_click(button_red, label_cur_option.clone(), "Clicked red");
        handle_button_click(button_aqua, label_cur_option.clone(), "Clicked aqua");
        handle_button_click(button_yellow, label_cur_option.clone(), "Clicked yellow");

        let cb_first = state.fetch_component_as::<ComponentCheckbox>("cb_first")?;
        let label = label_cur_option.clone();
        cb_first.on_toggle(Box::new(move |e| {
            let mut widget = label.get_as_mut::<WidgetLabel>();
            widget.set_text(
                &mut e.state.globals.i18n(),
                Translation::from_raw_text(&format!("checkbox toggle: {}", e.checked)),
            );
            Ok(())
        }));

        Ok(Self { layout, state })
    }
}

impl Testbed for TestbedGeneric {
    fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
        self.layout
            .update(Vec2::new(width, height), timestep_alpha)?;
        Ok(())
    }

    fn layout(&mut self) -> &mut Layout {
        &mut self.layout
    }
}
