use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::{
    event::EventListenerCollection, globals::WguiGlobals, layout::Layout, parser::ParserState,
};

pub struct TestbedAny {
    pub layout: Layout,

    #[allow(dead_code)]
    state: ParserState,
}

impl TestbedAny {
    pub fn new(
        name: &str,
        listeners: &mut EventListenerCollection<(), ()>,
    ) -> anyhow::Result<Self> {
        let path = format!("gui/{name}.xml");

        let globals = WguiGlobals::new(Box::new(assets::Asset {}))?;

        let (layout, state) =
            wgui::parser::new_layout_from_assets(globals, listeners, &path, false)?;
        Ok(Self { layout, state })
    }
}

impl Testbed for TestbedAny {
    fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
        self.layout
            .update(Vec2::new(width, height), timestep_alpha)?;
        Ok(())
    }

    fn layout(&mut self) -> &mut Layout {
        &mut self.layout
    }
}
