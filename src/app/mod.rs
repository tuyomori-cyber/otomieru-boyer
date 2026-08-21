pub mod state;

use eframe::egui;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::audio::decoder::decode_file;
use crate::audio::player::{AudioPlayer, TransportState, UI_REPAINT_INTERVAL};
use crate::audio::preview_tone::{PreviewTonePlayer, PreviewToneRequest};
use crate::model::PlaybackDspSettings;
use crate::ui::{piano, spectrogram, timeline, toolbar};

pub struct OtomieruApp {
    state: AppState,
    player: AudioPlayer,
    preview_tone_player: Option<PreviewTonePlayer>,
    last_applied_dsp_settings: Option<PlaybackDspSettings>,
}

impl OtomieruApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_japanese_fonts(&cc.egui_ctx);
        Self {
            state: AppState::default(),
            player: AudioPlayer::default(),
            preview_tone_player: PreviewTonePlayer::new().ok(),
            last_applied_dsp_settings: None,
        }
    }

    fn open_audio_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("audio", &["wav", "mp3", "flac", "ogg", "m4a", "aac"])
            .pick_file()
        else {
            self.state.set_status("ファイル選択をキャンセルしました。");
            return;
        };

        self.state
            .set_status(format!("読み込み中: {}", path.display()));

        match decode_file(&path) {
            Ok(decoded) => {
                let track = crate::model::Track::from_decoded(decoded);
                match self.player.load_track(&track) {
                    Ok(()) => {
                        self.state.set_loaded_track(path, track);
                        self.last_applied_dsp_settings = None;
                    }
                    Err(error) => {
                        self.state.track = None;
                        self.state.loaded_file_path = None;
                        self.last_applied_dsp_settings = None;
                        self.state
                            .set_status(format!("再生準備に失敗しました: {error}"));
                    }
                }
            }
            Err(error) => {
                self.state.track = None;
                self.state.loaded_file_path = None;
                self.last_applied_dsp_settings = None;
                self.state
                    .set_status(format!("読み込みに失敗しました: {error}"));
            }
        }
    }

    fn sync_transport_state(&mut self) {
        self.player.set_loop_enabled(
            self.state.playback.loop_enabled && self.state.selection.normalized().is_some(),
        );
        self.player
            .set_loop_range(self.state.selection.normalized());
    }

    fn sync_dsp_settings(&mut self) {
        if self.state.track.is_none() {
            self.last_applied_dsp_settings = None;
            return;
        }

        if !self.state.playback.playing
            && self.last_applied_dsp_settings != Some(self.state.playback.dsp)
        {
            self.player.set_dsp_settings(self.state.playback.dsp);
            self.last_applied_dsp_settings = Some(self.state.playback.dsp);
        }
    }
}

fn configure_japanese_fonts(ctx: &egui::Context) {
    const FONT_PATHS: [&str; 3] = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/ipaexg.ttf",
    ];

    let Some(font_bytes) = FONT_PATHS.iter().find_map(|path| std::fs::read(path).ok()) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    let font_name = "japanese-ui".to_owned();
    fonts.font_data.insert(
        font_name.clone(),
        Arc::new(egui::FontData::from_owned(font_bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(font_name.clone());
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for OtomieruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let space_pressed =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let actions = toolbar::show(ctx, &mut self.state);
        if actions.open_requested {
            self.open_audio_file();
        }
        self.sync_transport_state();
        self.sync_dsp_settings();
        if actions.seek_to_start_requested {
            self.player.seek_to_start();
            self.state.playback.position_seconds = 0.0;
        }
        if actions.play_pause_requested || (space_pressed && self.state.track.is_some()) {
            if self.state.playback.playing {
                self.player.pause();
                self.state.playback.playing = false;
            } else if let Err(error) = self.player.play() {
                self.state
                    .set_status(format!("再生開始に失敗しました: {error}"));
            } else {
                self.state.playback.playing = true;
            }
        }
        if actions.stop_requested {
            self.player.stop();
            self.state.playback.playing = false;
            self.state.playback.position_seconds = 0.0;
        }

        let previous_position_seconds = self.state.playback.position_seconds;
        let snapshot = self.player.snapshot();
        self.state.playback.playing = snapshot.transport == TransportState::Playing;
        self.state.playback.position_seconds = snapshot.position_seconds;
        self.state
            .follow_playhead_if_needed(previous_position_seconds);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let _timeline_actions = timeline::show(ui, &mut self.state, 108.0);
                self.sync_transport_state();
                self.sync_dsp_settings();
                ui.add_space(4.0);

                // ステータス行と余白を残し、それ以外をスペクトラムとピアノへ使う。
                let visualization_height = (ui.available_height() - 32.0).max(240.0);
                ui.horizontal(|ui| {
                    piano::show(ui, &self.state, visualization_height);
                    let spectrogram_actions =
                        spectrogram::show(ui, &self.state, visualization_height);
                    if let Some(seconds) = spectrogram_actions.seek_seconds {
                        self.player.seek_to_seconds(seconds);
                        self.state.playback.position_seconds = seconds;
                    }
                    if let Some(view_start_seconds) = spectrogram_actions.view_start_seconds {
                        self.state.set_view_start_seconds(view_start_seconds);
                    }
                    if let Some((anchor_seconds, factor)) = spectrogram_actions.zoom_at {
                        self.state.zoom_view_at(anchor_seconds, factor);
                    }
                    if let Some((anchor_midi, factor)) = spectrogram_actions.pitch_zoom_at {
                        self.state.zoom_pitch_at(anchor_midi, factor);
                    }
                    if let Some(midi_note) = spectrogram_actions.preview_midi_note {
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.update_preview(PreviewToneRequest { midi_note });
                            self.state.preview_tone_active = true;
                        }
                    }
                    if spectrogram_actions.stop_preview {
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.stop_preview();
                        }
                        self.state.preview_tone_active = false;
                    }
                });

                ui.add_space(8.0);
                ui.label(self.state.status_message());
            });
        });

        if self.state.playback.playing {
            ctx.request_repaint_after(UI_REPAINT_INTERVAL);
        }
    }
}
