use eframe::egui;

use crate::app::state::AppState;

#[derive(Debug, Default, Clone, Copy)]
pub struct ToolbarActions {
    pub open_requested: bool,
    pub play_pause_requested: bool,
    pub seek_to_start_requested: bool,
    pub seek_seconds: Option<f64>,
    pub stop_requested: bool,
}

pub fn show(ctx: &egui::Context, state: &mut AppState) -> ToolbarActions {
    let mut actions = ToolbarActions::default();

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open").clicked() {
                actions.open_requested = true;
            }

            let play_label = if state.playback.playing {
                "Pause"
            } else {
                "Play"
            };
            if ui
                .add_enabled(state.track.is_some(), egui::Button::new(play_label))
                .clicked()
            {
                actions.play_pause_requested = true;
            }

            if ui
                .add_enabled(state.track.is_some(), egui::Button::new("|<"))
                .clicked()
            {
                actions.seek_to_start_requested = true;
            }

            if ui
                .add_enabled(state.track.is_some(), egui::Button::new("Stop"))
                .clicked()
            {
                actions.stop_requested = true;
            }

            ui.separator();

            egui::ComboBox::from_label("Speed")
                .selected_text(format!("{:.2}x", state.playback.speed))
                .show_ui(ui, |ui| {
                    for speed in [0.50_f32, 0.75, 1.00] {
                        ui.selectable_value(
                            &mut state.playback.speed,
                            speed,
                            format!("{speed:.2}x"),
                        );
                    }
                });

            ui.separator();

            ui.checkbox(&mut state.playback.loop_enabled, "Loop");

            ui.separator();

            let duration = state
                .track
                .as_ref()
                .map(|track| track.duration_seconds)
                .unwrap_or(0.0);

            let mut seek_value = state.playback.position_seconds;
            let slider = egui::Slider::new(&mut seek_value, 0.0..=duration.max(0.0))
                .show_value(false)
                .min_decimals(2)
                .max_decimals(2);

            let response = ui.add_enabled(state.track.is_some(), slider);
            if response.changed() {
                actions.seek_seconds = Some(seek_value);
            }

            ui.monospace(format!(
                "{} / {}",
                format_mm_ss(state.playback.position_seconds),
                format_mm_ss(duration)
            ));
        });

        ui.add_space(4.0);
    });

    actions
}

fn format_mm_ss(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).floor() as u64;
    let minutes = total_seconds / 60;
    let secs = total_seconds % 60;
    format!("{minutes:02}:{secs:02}")
}
