use eframe::egui;

use crate::app::state::AppState;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            let _ = ui.add_enabled(false, egui::Button::new("Open"));

            let play_label = if state.playback.playing {
                "Pause"
            } else {
                "Play"
            };
            if ui.button(play_label).clicked() {
                state.playback.playing = !state.playback.playing;
            }

            if ui.button("Stop").clicked() {
                state.playback.playing = false;
                state.playback.position_seconds = 0.0;
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
}
