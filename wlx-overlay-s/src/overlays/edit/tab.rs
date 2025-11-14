use std::{collections::HashMap, rc::Rc};

use wgui::{
    components::button::ComponentButton,
    event::CallbackDataCommon,
    layout::WidgetID,
    parser::Fetchable,
    taffy::{Display, Style},
};

use crate::overlays::edit::EditModeWrapPanel;

static TABS: [&str; 4] = ["none", "pos", "alpha", "curve"];
static BUTTON_PREFIX: &str = "top_";
static PANE_PREFIX: &str = "tab_";

#[derive(Clone)]
struct TabData {
    button: Option<Rc<ComponentButton>>,
    pane: WidgetID,
    name: &'static str,
}

#[derive(Default)]
pub(super) struct ButtonPaneTabSwitcher {
    tabs: HashMap<&'static str, Rc<TabData>>,
    active_tab: Option<Rc<TabData>>,
}

impl ButtonPaneTabSwitcher {
    pub fn new(panel: &mut EditModeWrapPanel) -> anyhow::Result<Self> {
        let mut tabs = HashMap::new();

        for tab_name in &TABS {
            let name = format!("{BUTTON_PREFIX}{tab_name}");
            let button = panel.parser_state.fetch_component_as(&name).ok();

            let name = format!("{PANE_PREFIX}{tab_name}");
            let pane = panel.parser_state.get_widget_id(&name)?;

            tabs.insert(
                *tab_name,
                Rc::new(TabData {
                    button: button.clone(),
                    pane,
                    name: tab_name,
                }),
            );
        }
        Ok(Self {
            tabs,
            active_tab: None,
        })
    }

    pub fn tab_button_clicked(&mut self, common: &mut CallbackDataCommon, mut tab: &str) {
        // deactivate active tab
        if let Some(old_tab) = self.active_tab.take() {
            set_tab_active(common, &old_tab, false);

            if old_tab.name == tab {
                // close current tab
                tab = "none";
            }
        }
        let data = self.tabs[tab].clone();
        set_tab_active(common, &data, true);
        self.active_tab = Some(data);
    }

    pub fn reset(&mut self, common: &mut CallbackDataCommon) {
        if let Some(data) = self.active_tab.take() {
            set_tab_active(common, &data, false);
        }

        let data = self.tabs["none"].clone();
        set_tab_active(common, &data, true);
        self.active_tab = Some(data);
    }
}

fn set_tab_active(common: &mut CallbackDataCommon, data: &TabData, active: bool) {
    let style = Style {
        display: if active {
            Display::Block
        } else {
            Display::None
        },
        ..Default::default()
    };
    common.alterables.set_style(data.pane, style);
    if let Some(button) = data.button.as_ref() {
        button.set_sticky_state(common, active);
    }
}
