use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::{
    components::button::ComponentButton, event::EventListenerCollection, globals::WguiGlobals,
    i18n::Translation, layout::Layout, parser::ParserState,
};

pub struct TestbedGeneric {
    pub layout: Layout,

    #[allow(dead_code)]
    state: ParserState,
}

impl TestbedGeneric {
    pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
        const XML_PATH: &str = "gui/various_widgets.xml";

        let globals = WguiGlobals::new(Box::new(assets::Asset {}))?;

        let (mut layout, state) =
            wgui::parser::new_layout_from_assets(globals, listeners, XML_PATH, false)?;

        let label_current_option = state.fetch_widget("label_current_option")?;
        let b1 = state.fetch_component_as::<ComponentButton>("button_red")?;
        let b2 = state.fetch_component_as::<ComponentButton>("button_aqua")?;
        let b3 = state.fetch_component_as::<ComponentButton>("button_yellow")?;

        b1.set_text(
            &mut layout.state,
            Translation::from_raw_text("hello, world!"),
        );

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
