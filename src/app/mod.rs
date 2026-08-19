pub mod state;

use eframe::egui;

use crate::app::state::AppState;
use crate::ui::{piano, spectrogram, toolbar};

pub struct OtomieruApp {
    state: AppState,
}

impl OtomieruApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::default(),
        }
    }
}

impl eframe::App for OtomieruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        toolbar::show(ctx, &mut self.state);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    piano::show(ui, &self.state);
                    spectrogram::show(ui, &self.state);
                });

                ui.add_space(8.0);
                ui.label(self.state.status_message());
            });
        });
    }
}
