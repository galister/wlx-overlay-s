use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::{event::EventListenerCollection, layout::Layout};

pub struct TestbedGeneric {
    pub layout: Layout,
}

impl TestbedGeneric {
    pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
        const XML_PATH: &str = "gui/testbed.xml";

        let (layout, _res) =
            wgui::parser::new_layout_from_assets(Box::new(assets::Asset {}), XML_PATH)?;

        Ok(Self { layout })
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
