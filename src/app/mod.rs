pub mod state;

use eframe::egui;

use crate::app::state::AppState;
use crate::audio::decoder::decode_file;
use crate::audio::player::AudioPlayer;
use crate::ui::{piano, spectrogram, toolbar};

pub struct OtomieruApp {
    state: AppState,
    player: AudioPlayer,
}

impl OtomieruApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::default(),
            player: AudioPlayer::default(),
        }
    }

    fn open_audio_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("audio", &["wav", "mp3", "flac", "ogg", "m4a", "aac"])
            .pick_file()
        else {
            self.state
                .set_status("ファイル選択をキャンセルしました。");
            return;
        };

        self.state
            .set_status(format!("読み込み中: {}", path.display()));

        match decode_file(&path) {
            Ok(decoded) => {
                let track = crate::model::Track::from_decoded(decoded);
                match self.player.load_track(&track) {
                    Ok(()) => self.state.set_loaded_track(path, track),
                    Err(error) => {
                        self.state.track = None;
                        self.state.loaded_file_path = None;
                        self.state
                            .set_status(format!("再生準備に失敗しました: {error}"));
                    }
                }
            }
            Err(error) => {
                self.state.track = None;
                self.state.loaded_file_path = None;
                self.state
                    .set_status(format!("読み込みに失敗しました: {error}"));
            }
        }
    }
}

impl eframe::App for OtomieruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let actions = toolbar::show(ctx, &mut self.state);
        if actions.open_requested {
            self.open_audio_file();
        }
        if actions.play_pause_requested {
            if self.player.transport_state() == crate::audio::player::TransportState::Playing {
                self.player.pause();
            } else if let Err(error) = self.player.play() {
                self.state
                    .set_status(format!("再生開始に失敗しました: {error}"));
            }
        }
        if actions.stop_requested {
            self.player.stop();
        }

        self.state.playback.playing =
            self.player.transport_state() == crate::audio::player::TransportState::Playing;
        self.state.playback.position_seconds = self.player.current_position_seconds();

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

        ctx.request_repaint();
    }
}
