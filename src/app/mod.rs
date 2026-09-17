pub mod input;
pub mod state;
pub mod tool_palette;

use eframe::egui;
use std::sync::Arc;
use std::time::Instant;

use crate::analysis::stft::StftSettings;
use crate::app::state::{AppState, UiFrameMetrics};
use crate::audio::decoder::decode_file;
use crate::audio::player::{AudioPlayer, PlayerSnapshot, TransportState, UI_REPAINT_INTERVAL};
use crate::audio::preview_tone::{MemoToneRequest, PreviewTonePlayer, PreviewToneRequest};
use crate::model::{PlaybackDspSettings, ProjectData, ProjectState};
use crate::persistence::{
    AppSettings, AudioIdentity, AudioMismatch, CacheLoadOutcome, LoadOutcome, load_app_settings,
    load_sidecar, load_spectrogram_cache, save_app_settings, save_sidecar, save_spectrogram_cache,
    sidecar_path,
};
use crate::ui::{piano, spectrogram, timeline, toolbar};

/// レイヤーの音量（0〜100%）に掛ける、音高メモ再生専用の基準振幅。
/// スペクトログラム押下時の確認音量とは独立させる。
const MEMO_TONE_BASE_AMPLITUDE: f32 = 0.16;

pub struct OtomieruApp {
    state: AppState,
    player: AudioPlayer,
    preview_tone_player: Option<PreviewTonePlayer>,
    last_applied_dsp_settings: Option<PlaybackDspSettings>,
    playhead_interpolator: PlayheadInterpolator,
    ui_frame_monitor: UiFrameMonitor,
    spectrogram_cache: spectrogram::SpectrogramCache,
    comparison_loop_range: Option<(f64, f64)>,
}

impl OtomieruApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_japanese_fonts(&cc.egui_ctx);
        let player = AudioPlayer::default();
        let comparison_control = player.comparison_control();
        let mut state = AppState::default();
        match load_app_settings() {
            Ok(Some(mut settings)) => {
                let migrated = settings.mouse_input.normalize_for_platform();
                if migrated && let Err(error) = save_app_settings(&settings) {
                    state.set_status(format!("操作設定を保存できませんでした: {error}"));
                }
                state.mouse_input = settings.mouse_input;
                state.tool_palette_items = settings.tool_palette_items;
                state.tool_palette_order = settings.tool_palette_order;
                state.playback.comparison_sequence = settings.comparison_sequence;
            }
            Ok(None) => {}
            Err(error) => state.set_status(format!("操作設定を読み込めませんでした: {error}")),
        }
        player.set_comparison_sequence(&state.playback.comparison_sequence);
        Self {
            state,
            player,
            preview_tone_player: PreviewTonePlayer::new(comparison_control).ok(),
            last_applied_dsp_settings: None,
            playhead_interpolator: PlayheadInterpolator::default(),
            ui_frame_monitor: UiFrameMonitor::default(),
            spectrogram_cache: spectrogram::SpectrogramCache::default(),
            comparison_loop_range: None,
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

        let load_started_at = Instant::now();
        self.state
            .set_status(format!("読み込み中: {}", path.display()));
        self.spectrogram_cache.clear();

        match decode_file(&path) {
            Ok(decoded) => {
                let mut track = crate::model::Track::from_decoded_without_spectrogram(decoded);
                let analysis_status = self.load_or_build_spectrogram(&path, &mut track);
                match self.player.load_track(&track) {
                    Ok(()) => {
                        self.player.disable_comparison();
                        self.state.set_loaded_track(path, track);
                        self.load_project_sidecar();
                        if self.state.status_text.is_empty() {
                            self.state.set_status(analysis_status);
                        } else {
                            self.state.status_text.push_str(" | ");
                            self.state.status_text.push_str(&analysis_status);
                        }
                        self.state.status_text.push_str(&format!(
                            " | 読み込み時間: {}",
                            format_load_duration(load_started_at.elapsed())
                        ));
                        self.playhead_interpolator.reset(0.0);
                        self.last_applied_dsp_settings = None;
                        self.comparison_loop_range = None;
                    }
                    Err(error) => {
                        self.player.disable_comparison();
                        self.state.track = None;
                        self.state.loaded_file_path = None;
                        self.state.display_playhead_position_seconds = 0.0;
                        self.playhead_interpolator.reset(0.0);
                        self.last_applied_dsp_settings = None;
                        self.comparison_loop_range = None;
                        self.state
                            .set_status(format!("再生準備に失敗しました: {error}"));
                    }
                }
            }
            Err(error) => {
                self.state.track = None;
                self.state.loaded_file_path = None;
                self.state.display_playhead_position_seconds = 0.0;
                self.playhead_interpolator.reset(0.0);
                self.last_applied_dsp_settings = None;
                self.player.disable_comparison();
                self.comparison_loop_range = None;
                self.state
                    .set_status(format!("読み込みに失敗しました: {error}"));
            }
        }
    }

    /// 音源そのものは毎回デコードするが、重いSTFT・スペクトログラム生成は
    /// 音源と解析条件が一致するキャッシュを利用する。
    fn load_or_build_spectrogram(
        &self,
        path: &std::path::Path,
        track: &mut crate::model::Track,
    ) -> String {
        let settings = StftSettings::default();
        let identity = match AudioIdentity::from_audio(path, track) {
            Ok(identity) => identity,
            Err(error) => {
                track.rebuild_spectrogram();
                return format!(
                    "解析しました（キャッシュ照合情報を取得できませんでした: {error}）"
                );
            }
        };

        match load_spectrogram_cache(path, &identity, settings) {
            Ok((CacheLoadOutcome::Hit, Some(spectrogram))) => {
                track.spectrogram = Some(spectrogram);
                "解析キャッシュを利用しました。".to_owned()
            }
            Ok((CacheLoadOutcome::NotFound, _)) => {
                track.rebuild_spectrogram();
                self.save_spectrogram_cache_status(path, &identity, settings, track, "解析しました")
            }
            Ok((CacheLoadOutcome::Invalid, _)) => {
                track.rebuild_spectrogram();
                self.save_spectrogram_cache_status(
                    path,
                    &identity,
                    settings,
                    track,
                    "解析キャッシュを再生成しました",
                )
            }
            Ok((CacheLoadOutcome::Hit, None)) => {
                track.rebuild_spectrogram();
                self.save_spectrogram_cache_status(
                    path,
                    &identity,
                    settings,
                    track,
                    "解析キャッシュを再生成しました",
                )
            }
            Err(error) => {
                track.rebuild_spectrogram();
                self.save_spectrogram_cache_status(
                    path,
                    &identity,
                    settings,
                    track,
                    &format!("解析しました（解析キャッシュを読み込めませんでした: {error}）"),
                )
            }
        }
    }

    fn save_spectrogram_cache_status(
        &self,
        path: &std::path::Path,
        identity: &AudioIdentity,
        settings: StftSettings,
        track: &crate::model::Track,
        prefix: &str,
    ) -> String {
        let Some(spectrogram) = track.spectrogram.as_ref() else {
            return prefix.to_owned();
        };
        match save_spectrogram_cache(path, identity, settings, spectrogram) {
            Ok(_) => format!("{prefix}。解析キャッシュを保存しました。"),
            Err(error) => format!("{prefix}（解析キャッシュを保存できませんでした: {error}）"),
        }
    }

    fn sync_transport_state(&mut self) {
        self.player.set_loop_enabled(
            self.state.playback.loop_enabled && self.state.selection.normalized().is_some(),
        );
        self.player
            .set_loop_range(self.state.selection.normalized());
    }

    fn set_comparison_enabled(&mut self, enabled: bool) {
        let loop_range = self
            .state
            .playback
            .loop_enabled
            .then(|| self.state.selection.normalized())
            .flatten();
        if enabled {
            let Some((loop_start, _)) = loop_range else {
                return;
            };
            self.state.playback.comparison_enabled = true;
            self.state.playback.comparison_sequence_index = 0;
            self.comparison_loop_range = loop_range;
            self.player.enable_comparison_from_start();
            self.player.seek_to_seconds(loop_start);
            self.state.playback.position_seconds = loop_start;
            self.state.display_playhead_position_seconds = loop_start;
            self.playhead_interpolator.reset(loop_start);
        } else {
            self.state.playback.comparison_enabled = false;
            self.state.playback.comparison_sequence_index = 0;
            self.comparison_loop_range = None;
            self.player.disable_comparison();
        }
    }

    fn sync_comparison_state(&mut self) {
        let loop_range = self
            .state
            .playback
            .loop_enabled
            .then(|| self.state.selection.normalized())
            .flatten();
        if loop_range.is_none() {
            self.set_comparison_enabled(false);
            return;
        }
        if self.state.playback.comparison_enabled && self.comparison_loop_range != loop_range {
            self.state.playback.comparison_sequence_index = 0;
            self.comparison_loop_range = loop_range;
            self.player.reset_comparison_to_start();
        }
    }

    fn load_project_sidecar(&mut self) {
        let (Some(audio_path), Some(track)) = (
            self.state.loaded_file_path.as_deref(),
            self.state.track.as_ref(),
        ) else {
            return;
        };
        let identity = match AudioIdentity::from_audio(audio_path, track) {
            Ok(identity) => identity,
            Err(error) => {
                self.state
                    .set_status(format!("プロジェクト照合情報の取得に失敗しました: {error}"));
                return;
            }
        };
        match load_sidecar(audio_path, &identity) {
            Ok(LoadOutcome::NotFound) => {}
            Ok(LoadOutcome::Loaded {
                project,
                audio_mismatches,
            }) => {
                self.state.project = ProjectState::from_data(project);
                self.state.sync_project_playback_settings();
                if audio_mismatches.is_empty() {
                    self.state.set_status("プロジェクトを読み込みました。");
                } else {
                    self.state.set_status(format!(
                        "警告: sidecarの音源情報と{}が一致しません。プロジェクトは読み込みました。",
                        mismatch_labels(&audio_mismatches)
                    ));
                }
            }
            Err(error) => {
                self.state.set_status(format!(
                    "プロジェクトを読み込めませんでした。新規プロジェクトを使用します: {error}"
                ));
            }
        }
    }

    fn save_project_sidecar(&mut self) {
        let (Some(audio_path), Some(track)) = (
            self.state.loaded_file_path.as_deref(),
            self.state.track.as_ref(),
        ) else {
            return;
        };
        let result = AudioIdentity::from_audio(audio_path, track)
            .and_then(|identity| save_sidecar(audio_path, identity, &self.state.project.data));
        match result {
            Ok(path) => {
                self.state.project.mark_saved();
                self.state
                    .set_status(format!("プロジェクトを保存しました: {}", path.display()));
            }
            Err(error) => {
                let destination = sidecar_path(audio_path)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| audio_path.display().to_string());
                self.state.set_status(format!(
                    "プロジェクト保存に失敗しました ({destination}): {error}"
                ));
            }
        }
    }

    fn sync_dsp_settings(&mut self) {
        self.state.sync_project_playback_settings();
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

    fn sync_pitch_memo_playback(&mut self, snapshot: &PlayerSnapshot) {
        let Some(preview_tone_player) = &mut self.preview_tone_player else {
            return;
        };
        let requests = if snapshot.transport == TransportState::Playing {
            active_memo_tone_requests(&self.state.project.data, snapshot.position_seconds)
        } else {
            Vec::new()
        };
        let settings = &self.state.project.data.project_settings;
        preview_tone_player.sync_memo_voices(
            &requests,
            self.state.preview_timbre,
            settings.preview_reference_a4_hz,
            snapshot.transport_generation,
        );
    }
}

fn active_memo_tone_requests(project: &ProjectData, position_seconds: f64) -> Vec<MemoToneRequest> {
    project
        .layers
        .iter()
        .filter(|layer| !layer.muted && layer.volume > 0.0)
        .flat_map(|layer| {
            layer.memos.iter().filter_map(move |memo| {
                let midi_note = u8::try_from(memo.pitch_midi)
                    .ok()
                    .filter(|midi_note| *midi_note <= 127)?;
                let end_sec = memo.start_sec + memo.duration_sec;
                (position_seconds >= memo.start_sec && position_seconds < end_sec).then_some(
                    MemoToneRequest {
                        memo_key: memo.id.get(),
                        midi_note,
                        amplitude: MEMO_TONE_BASE_AMPLITUDE * layer.volume,
                    },
                )
            })
        })
        .collect()
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
        self.state.ui_frame_metrics = self.ui_frame_monitor.observe(ctx.pixels_per_point());
        let space_pressed =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let the_world_pressed = (!ctx.wants_keyboard_input())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::T));
        let save_pressed =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        let undo_pressed = consume_project_undo_shortcut(ctx);
        let actions = toolbar::show(ctx, &mut self.state);
        if actions.app_settings_changed || actions.comparison_sequence_changed {
            let settings = AppSettings {
                mouse_input: self.state.mouse_input.clone(),
                tool_palette_items: self.state.tool_palette_items.clone(),
                tool_palette_order: self.state.tool_palette_order.clone(),
                comparison_sequence: self.state.playback.comparison_sequence.clone(),
                ..AppSettings::default()
            };
            if let Err(error) = save_app_settings(&settings) {
                self.state
                    .set_status(format!("ユーザー設定を保存できませんでした: {error}"));
            }
        }
        if actions.open_requested {
            self.open_audio_file();
        }
        if actions.save_requested || (save_pressed && self.state.track.is_some()) {
            self.save_project_sidecar();
        }
        if !self.state.playback.the_world_active
            && (undo_pressed || actions.undo_requested)
            && self.state.project.undo()
        {
            self.state
                .set_status("直前の音高メモ編集を取り消しました。");
        }
        if actions.clear_loop_range_requested && !self.state.playback.the_world_active {
            self.state.selection.clear();
            self.state
                .set_status("ループ範囲を消去しました。曲先頭へは |< を使います。");
        }
        if actions.reset_visualization_requested {
            self.state.reset_visualization_view();
            ctx.request_repaint();
        }
        if let Some(enabled) = actions.comparison_enabled_changed
            && !self.state.playback.the_world_active
        {
            self.set_comparison_enabled(enabled);
        }
        if actions.comparison_sequence_changed {
            self.player
                .set_comparison_sequence(&self.state.playback.comparison_sequence);
            if self.state.playback.comparison_enabled {
                self.set_comparison_enabled(true);
            }
        }
        self.sync_comparison_state();
        self.sync_transport_state();
        self.player
            .set_source_volume(self.state.source_audio_volume);
        self.player.set_source_muted(self.state.source_audio_muted);
        self.sync_dsp_settings();
        if actions.seek_to_start_requested && !self.state.playback.the_world_active {
            self.player.seek_to_start();
            self.player.reset_comparison_to_start();
            self.state.playback.comparison_sequence_index = 0;
            self.state.playback.position_seconds = 0.0;
            self.state.display_playhead_position_seconds = 0.0;
            self.state.reset_view_to_start();
            self.playhead_interpolator.reset(0.0);
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
            self.player.reset_comparison_to_start();
            self.state.playback.comparison_sequence_index = 0;
            self.state.playback.playing = false;
            self.state.playback.the_world_active = false;
            self.state.playback.position_seconds = 0.0;
            self.state.display_playhead_position_seconds = 0.0;
            self.playhead_interpolator.reset(0.0);
        }
        if actions.toggle_the_world_requested || the_world_pressed {
            self.state.playback.the_world_active = self.player.toggle_the_world();
        }

        let previous_position_seconds = self.state.playback.position_seconds;
        let snapshot = self.player.snapshot();
        self.state.playback.playing = snapshot.transport == TransportState::Playing;
        self.state.playback.position_seconds = snapshot.position_seconds;
        self.state.playback.comparison_enabled = snapshot.comparison.phase.is_some();
        self.state.playback.comparison_sequence_index =
            snapshot.comparison.sequence_index.unwrap_or(0);
        self.state.playback.the_world_active = snapshot.the_world_active;
        self.sync_pitch_memo_playback(&snapshot);
        self.state.display_playhead_position_seconds = self.playhead_interpolator.update(
            snapshot.position_seconds,
            self.state.playback.playing,
            if snapshot.the_world_active {
                0.0
            } else {
                self.state.playback.dsp.speed_ratio as f64
            },
        );
        self.state
            .follow_playhead_if_needed(previous_position_seconds);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let _timeline_actions = timeline::show(ui, &mut self.state, 108.0);
                self.sync_comparison_state();
                self.sync_transport_state();
                self.sync_dsp_settings();
                ui.add_space(4.0);

                // ステータス行と余白を残し、それ以外をスペクトラムとピアノへ使う。
                let visualization_height = (ui.available_height() - 32.0).max(240.0);
                ui.horizontal(|ui| {
                    piano::show(ui, &self.state, visualization_height);
                    let spectrogram_actions = spectrogram::show(
                        ui,
                        &mut self.state,
                        visualization_height,
                        &mut self.spectrogram_cache,
                    );
                    if let Some(seconds) = spectrogram_actions.seek_seconds
                        && !self.state.playback.the_world_active
                    {
                        self.player.seek_to_seconds(seconds);
                        self.state.playback.position_seconds = seconds;
                        self.state.display_playhead_position_seconds = seconds;
                        self.playhead_interpolator.reset(seconds);
                    }
                    if let Some(view_start_seconds) = spectrogram_actions.view_start_seconds {
                        self.state.set_view_start_seconds(view_start_seconds);
                        // 表示範囲の更新はスペクトログラム描画の後に適用されるため、
                        // 停止中でも次フレームを明示的に要求する。
                        ctx.request_repaint();
                    }
                    if let Some(pitch_view_center_midi) = spectrogram_actions.pitch_view_center_midi
                    {
                        self.state
                            .set_pitch_view_center_midi(pitch_view_center_midi);
                        ctx.request_repaint();
                    }
                    if let Some((anchor_seconds, factor)) = spectrogram_actions.zoom_at {
                        self.state.zoom_view_at(anchor_seconds, factor);
                        ctx.request_repaint();
                    }
                    if let Some((anchor_midi, factor)) = spectrogram_actions.pitch_zoom_at {
                        self.state.zoom_pitch_at(anchor_midi, factor);
                        ctx.request_repaint();
                    }
                    if let Some(midi_note) = spectrogram_actions.preview_midi_note {
                        let preview_changed = self.state.preview_midi_note != Some(midi_note);
                        self.state.preview_midi_note = Some(midi_note);
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.update_preview(PreviewToneRequest {
                                midi_note,
                                timbre: self.state.preview_timbre,
                                amplitude: self.state.preview_tone_amplitude,
                                reference_a4_hz: self
                                    .state
                                    .project
                                    .data
                                    .project_settings
                                    .preview_reference_a4_hz,
                            });
                        }
                        if preview_changed {
                            ctx.request_repaint();
                        }
                    }
                    if spectrogram_actions.stop_preview {
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.stop_preview();
                        }
                        if self.state.preview_midi_note.take().is_some() {
                            ctx.request_repaint();
                        }
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

/// テキスト入力にフォーカスがある間は、TextEdit自身のUndoを優先する。
///
/// アプリ全体のメモUndoは、テキスト編集ではないときだけ処理する。
fn consume_project_undo_shortcut(ctx: &egui::Context) -> bool {
    if !should_handle_project_undo(ctx.wants_keyboard_input()) {
        return false;
    }
    ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
}

fn should_handle_project_undo(wants_keyboard_input: bool) -> bool {
    !wants_keyboard_input
}

fn mismatch_labels(mismatches: &[AudioMismatch]) -> String {
    mismatches
        .iter()
        .map(|mismatch| mismatch.label())
        .collect::<Vec<_>>()
        .join("・")
}

#[derive(Default)]
struct UiFrameMonitor {
    last_frame_at: Option<Instant>,
    smoothed_frames_per_second: f32,
}

impl UiFrameMonitor {
    fn observe(&mut self, pixels_per_point: f32) -> UiFrameMetrics {
        self.observe_at(Instant::now(), pixels_per_point)
    }

    fn observe_at(&mut self, now: Instant, pixels_per_point: f32) -> UiFrameMetrics {
        let frame_time_ms = self
            .last_frame_at
            .map(|previous| now.duration_since(previous).as_secs_f32() * 1_000.0)
            .unwrap_or(0.0);
        self.last_frame_at = Some(now);

        if frame_time_ms > 0.0 && frame_time_ms < 250.0 {
            let instantaneous_fps = 1_000.0 / frame_time_ms;
            self.smoothed_frames_per_second = if self.smoothed_frames_per_second <= 0.0 {
                instantaneous_fps
            } else {
                self.smoothed_frames_per_second * 0.85 + instantaneous_fps * 0.15
            };
        } else if frame_time_ms >= 250.0 {
            self.smoothed_frames_per_second = 0.0;
        }

        UiFrameMetrics {
            frames_per_second: self.smoothed_frames_per_second,
            frame_time_ms,
            pixels_per_point,
        }
    }
}

#[derive(Default)]
struct PlayheadInterpolator {
    anchor_position_seconds: f64,
    anchor_time: Option<Instant>,
    last_snapshot_position_seconds: Option<f64>,
}

impl PlayheadInterpolator {
    fn reset(&mut self, position_seconds: f64) {
        self.anchor_position_seconds = position_seconds;
        self.anchor_time = Some(Instant::now());
        self.last_snapshot_position_seconds = Some(position_seconds);
    }

    fn update(&mut self, snapshot_position_seconds: f64, playing: bool, speed_ratio: f64) -> f64 {
        self.update_at(
            snapshot_position_seconds,
            playing,
            speed_ratio,
            Instant::now(),
        )
    }

    fn update_at(
        &mut self,
        snapshot_position_seconds: f64,
        playing: bool,
        speed_ratio: f64,
        now: Instant,
    ) -> f64 {
        let previous_snapshot = self.last_snapshot_position_seconds;
        let position_changed = previous_snapshot
            .map(|previous| (snapshot_position_seconds - previous).abs() > 1e-6)
            .unwrap_or(true);
        let position_rewound = previous_snapshot
            .map(|previous| snapshot_position_seconds + 1e-6 < previous)
            .unwrap_or(false);

        if !playing || position_changed || position_rewound || self.anchor_time.is_none() {
            self.anchor_position_seconds = snapshot_position_seconds;
            self.anchor_time = Some(now);
        }
        self.last_snapshot_position_seconds = Some(snapshot_position_seconds);

        if !playing {
            return snapshot_position_seconds;
        }

        let elapsed = now
            .duration_since(self.anchor_time.expect("playing playhead has an anchor"))
            .as_secs_f64();
        self.anchor_position_seconds + elapsed * speed_ratio.max(0.0)
    }
}

fn format_load_duration(duration: std::time::Duration) -> String {
    format!("{:.2}秒", duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::{
        PlayheadInterpolator, UiFrameMonitor, active_memo_tone_requests, format_load_duration,
        should_handle_project_undo,
    };
    use crate::model::ProjectState;
    use std::time::{Duration, Instant};

    #[test]
    fn memo_tone_is_active_at_its_start_but_not_at_its_end() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 1.0, 0.5, 60).unwrap();

        assert_eq!(active_memo_tone_requests(&project.data, 1.0).len(), 1);
        assert!(active_memo_tone_requests(&project.data, 1.5).is_empty());
    }

    #[test]
    fn memo_tone_honors_mute_and_volume_but_not_visibility() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 0.0, 1.0, 64).unwrap();
        let layer = &mut project.data.layers[0];
        layer.visible = false;
        layer.volume = 0.25;

        let requests = active_memo_tone_requests(&project.data, 0.5);
        assert_eq!(requests.len(), 1);
        assert!((requests[0].amplitude - 0.04).abs() < f32::EPSILON);

        project.data.layers[0].muted = true;
        assert!(active_memo_tone_requests(&project.data, 0.5).is_empty());
    }

    #[test]
    fn overlapping_memos_are_returned_as_independent_voices() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 0.0, 2.0, 60).unwrap();
        project.add_memo(layer_id, 0.5, 1.0, 67).unwrap();

        let requests = active_memo_tone_requests(&project.data, 1.0);

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].midi_note, 60);
        assert_eq!(requests[1].midi_note, 67);
        assert_ne!(requests[0].memo_key, requests[1].memo_key);
    }

    #[test]
    fn invalid_midi_notes_are_not_sent_to_the_audio_thread() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 0.0, 1.0, 128).unwrap();

        assert!(active_memo_tone_requests(&project.data, 0.5).is_empty());
    }

    #[test]
    fn interpolator_advances_between_audio_snapshots() {
        let now = Instant::now();
        let mut interpolator = PlayheadInterpolator::default();

        assert_eq!(interpolator.update_at(1.0, true, 1.0, now), 1.0);
        let interpolated = interpolator.update_at(1.0, true, 1.0, now + Duration::from_millis(10));

        assert!((interpolated - 1.01).abs() < 1e-6);
    }

    #[test]
    fn interpolator_resets_immediately_on_a_rewind_or_pause() {
        let now = Instant::now();
        let mut interpolator = PlayheadInterpolator::default();
        interpolator.update_at(5.0, true, 1.0, now);

        assert_eq!(
            interpolator.update_at(1.0, true, 1.0, now + Duration::from_millis(10)),
            1.0
        );
        assert_eq!(
            interpolator.update_at(1.0, false, 1.0, now + Duration::from_millis(20)),
            1.0
        );
    }

    #[test]
    fn frame_monitor_reports_update_cadence_and_scale() {
        let now = Instant::now();
        let mut monitor = UiFrameMonitor::default();
        monitor.observe_at(now, 1.0);

        let metrics = monitor.observe_at(now + Duration::from_millis(20), 1.5);

        assert!((metrics.frame_time_ms - 20.0).abs() < 1e-3);
        assert!((metrics.frames_per_second - 50.0).abs() < 1e-3);
        assert_eq!(metrics.pixels_per_point, 1.5);
    }

    #[test]
    fn project_undo_defers_to_focused_text_input() {
        assert!(!should_handle_project_undo(true));
        assert!(should_handle_project_undo(false));
    }

    #[test]
    fn load_duration_is_displayed_in_seconds() {
        assert_eq!(format_load_duration(Duration::from_millis(1_234)), "1.23秒");
    }
}
