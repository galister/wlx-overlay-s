use wgui::layout::LayoutUpdateResult;

use crate::state::AppState;

pub fn process_layout_result(app: &mut AppState, res: LayoutUpdateResult) {
    app.audio_sample_player
        .play_wgui_samples(&mut app.audio_system, res.sounds_to_play);
}
