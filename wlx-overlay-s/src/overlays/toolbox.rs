use std::time::Duration;

use glam::{Affine3A, Vec3};

use crate::{
    gui::{panel::GuiPanel, timer::GuiTimer},
    state::AppState,
    windowing::{
        window::{OverlayWindowConfig, OverlayWindowState, Positioning},
        Z_ORDER_WATCH,
    },
};

pub const EDIT_NAME: &str = "edit";

struct EditState {
    num_sets: usize,
    current_set: usize,
}

#[allow(clippy::significant_drop_tightening)]
pub fn create_edit(
    app: &mut AppState,
    num_sets: usize,
    current_set: usize,
) -> anyhow::Result<OverlayWindowConfig> {
    let state = EditState {
        num_sets,
        current_set,
    };
    let mut panel = GuiPanel::new_from_template(
        app,
        "gui/watch.xml",
        state,
        Some(Box::new(
            move |id, widget, doc_params, layout, parser_state| {
                if &*id != "sets" {
                    return Ok(());
                }

                for idx in 0..num_sets {
                    let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                    params.insert("display".into(), (idx + 1).to_string().into());
                    params.insert("handle".into(), idx.to_string().into());
                    parser_state.instantiate_template(doc_params, "Set", layout, widget, params)?;
                }
                Ok(())
            },
        )),
    )?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: app.session.config.watch_hand as _,
        lerp: 1.0,
    };

    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: EDIT_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                app.session.config.watch_rot,
                app.session.config.watch_pos,
            ) * Affine3A::from_translation(Vec3::Y * 0.075),
            ..OverlayWindowState::default()
        },
        show_on_spawn: true,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
