use eframe::egui;

use crate::app::state::AppState;

#[derive(Debug, Default, Clone, Copy)]
pub struct ToolbarActions {
    pub open_requested: bool,
    pub play_pause_requested: bool,
    pub seek_to_start_requested: bool,
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

            ui.monospace(format!("{:05.2} sec", state.playback.position_seconds));
        });

        ui.add_space(4.0);
    });

    actions
}
