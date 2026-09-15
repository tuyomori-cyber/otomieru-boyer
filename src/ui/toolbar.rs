use eframe::egui;

use crate::app::input::{
    MouseInputSettings, MousePreset, VariableMemoModifier, WheelAction, WheelModifier,
};
use crate::app::state::{AppState, PITCH_CLASS_NAMES};
use crate::audio::preview_tone::PreviewTimbre;
use crate::model::{EQ_BAND_COUNT, EQ_BAND_FREQUENCIES_HZ, ScalePreset};
use crate::ui::spectrogram::layer_color;

#[derive(Debug, Default, Clone, Copy)]
pub struct ToolbarActions {
    pub open_requested: bool,
    pub save_requested: bool,
    pub undo_requested: bool,
    pub play_pause_requested: bool,
    pub seek_to_start_requested: bool,
    pub stop_requested: bool,
    pub clear_loop_range_requested: bool,
    pub comparison_enabled_changed: Option<bool>,
    pub mouse_input_changed: bool,
    pub reset_visualization_requested: bool,
}

pub fn show(ctx: &egui::Context, state: &mut AppState) -> ToolbarActions {
    let mut actions = ToolbarActions::default();

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.menu_button("ファイル", |ui| {
                if ui.button("音源を開く…").clicked() {
                    actions.open_requested = true;
                    ui.close();
                }
                if ui
                    .add_enabled(
                        state.track.is_some() && state.project.editing.dirty,
                        egui::Button::new("保存").shortcut_text("Ctrl+S"),
                    )
                    .clicked()
                {
                    actions.save_requested = true;
                    ui.close();
                }
            });
            ui.menu_button("編集", |ui| {
                if ui
                    .add_enabled(
                        state.project.can_undo(),
                        egui::Button::new("元に戻す").shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    actions.undo_requested = true;
                    ui.close();
                }
            });
            ui.menu_button("再生", |ui| {
                let play_label = if state.playback.playing {
                    "一時停止"
                } else {
                    "再生"
                };
                if ui
                    .add_enabled(state.track.is_some(), egui::Button::new(play_label))
                    .clicked()
                {
                    actions.play_pause_requested = true;
                    ui.close();
                }
                if ui
                    .add_enabled(state.track.is_some(), egui::Button::new("先頭へ戻る"))
                    .clicked()
                {
                    actions.seek_to_start_requested = true;
                    ui.close();
                }
                if ui
                    .add_enabled(state.track.is_some(), egui::Button::new("停止"))
                    .clicked()
                {
                    actions.stop_requested = true;
                    ui.close();
                }
            });
            ui.menu_button("表示", |ui| {
                if ui.button("時間・音高表示を初期化").clicked() {
                    actions.reset_visualization_requested = true;
                    ui.close();
                }
            });
            ui.menu_button("分析ツール", |ui| {
                if ui.button("分析ツールを開く…").clicked() {
                    state.analysis_tools_popup_open = true;
                    ui.close();
                }
            });
            ui.menu_button("設定", |ui| {
                if ui.button("マウス操作…").clicked() {
                    state.mouse_settings_popup_open = true;
                    ui.close();
                }
            });
            ui.menu_button("ヘルプ", |ui| {
                if ui.button("操作ガイド").clicked() {
                    state.help_popup_open = true;
                    ui.close();
                }
            });

            let duration = state
                .track
                .as_ref()
                .map(|track| track.duration_seconds)
                .unwrap_or(0.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(format!(
                    "{} / {}",
                    format_mm_ss(state.display_playhead_position_seconds),
                    format_mm_ss(duration)
                ));
            });
        });
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            let dsp_controls_enabled = state.track.is_some() && !state.playback.playing;

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

            ui.add_enabled_ui(dsp_controls_enabled, |ui| {
                egui::ComboBox::from_label("Speed")
                    .selected_text(format!("{:.2}x", state.playback.dsp.speed_ratio))
                    .show_ui(ui, |ui| {
                        for speed in [0.50_f32, 0.75, 1.00, 1.25, 1.50] {
                            ui.selectable_value(
                                &mut state.playback.dsp.speed_ratio,
                                speed,
                                format!("{speed:.2}x"),
                            );
                        }
                    });
            });

            ui.separator();

            ui.add_enabled_ui(dsp_controls_enabled, |ui| {
                egui::ComboBox::from_label("Pitch")
                    .selected_text(format_pitch_label(state.playback.dsp.pitch_shift_semitones))
                    .show_ui(ui, |ui| {
                        for semitones in [-12_i32, 0, 12] {
                            ui.selectable_value(
                                &mut state.playback.dsp.pitch_shift_semitones,
                                semitones,
                                format_pitch_label(semitones),
                            );
                        }
                    });
            });

            ui.separator();

            ui.checkbox(&mut state.playback.loop_enabled, "Loop");
            if ui
                .add_enabled(
                    state.selection.normalized().is_some(),
                    egui::Button::new("消去"),
                )
                .on_disabled_hover_text("消去するループ範囲がありません。")
                .on_hover_text("ループ範囲を消去します。比較中の場合は比較も終了します。")
                .clicked()
            {
                actions.clear_loop_range_requested = true;
            }

            let comparison_available = state.track.is_some()
                && state.playback.loop_enabled
                && state.selection.normalized().is_some();
            let mut comparison_enabled = state.playback.comparison_enabled && comparison_available;
            if ui
                .add_enabled(
                    comparison_available,
                    egui::Checkbox::new(&mut comparison_enabled, "比較"),
                )
                .on_disabled_hover_text("有効なループ範囲を指定してLoopをONにしてください。")
                .changed()
            {
                actions.comparison_enabled_changed = Some(comparison_enabled);
            }
            if comparison_available && let Some(phase) = state.playback.comparison_phase() {
                ui.label(format!("比較: {}", phase.label()));
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Heat");
            ui.add_sized(
                [110.0, ui.spacing().interact_size.y],
                egui::Slider::new(&mut state.spectrogram_gain_db, -24.0..=24.0)
                    .suffix(" dB")
                    .step_by(1.0),
            );

            ui.add_sized(
                [110.0, ui.spacing().interact_size.y],
                egui::Slider::new(&mut state.preview_tone_amplitude, 0.0..=0.5)
                    .text("試聴音量")
                    .suffix("")
                    .custom_formatter(|amplitude, _| format!("{:.0}%", amplitude * 100.0)),
            );
            ui.add_sized(
                [110.0, ui.spacing().interact_size.y],
                egui::Slider::new(&mut state.source_audio_volume, 0.0..=1.0)
                    .text("原曲音量")
                    .suffix("")
                    .custom_formatter(|volume, _| format!("{:.0}%", volume * 100.0)),
            );
            ui.add_enabled_ui(!state.playback.comparison_enabled, |ui| {
                ui.checkbox(&mut state.source_audio_muted, "Mute")
                    .on_hover_text(
                        "原曲だけを一時的に無音化します。原曲音量の設定値は維持されます。",
                    );
            })
            .response
            .on_disabled_hover_text("比較中の原曲MuteはOriginal / Notes / Mixが自動制御します。");
        });

        ui.add_space(4.0);
    });

    show_analysis_tools_dialog(ctx, state);
    actions.mouse_input_changed = show_mouse_settings_dialog(ctx, state);
    show_help_dialog(ctx, state);

    actions
}

fn show_help_dialog(ctx: &egui::Context, state: &mut AppState) {
    let mut help_popup_open = state.help_popup_open;
    egui::Window::new("操作ガイド")
        .open(&mut help_popup_open)
        .resizable(true)
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.heading("耳コピの基本操作");
            ui.label("Space: 再生／一時停止　　Ctrl+S: 保存　　Ctrl+Z: 元に戻す");
            ui.separator();
            ui.strong("再生と比較");
            ui.label("上部ツールバーで再生速度・ピッチ・Loop・比較を操作します。");
            ui.label("タイムラインをドラッグしてループ範囲を作成・調整します。");
            ui.separator();
            ui.strong("音高メモ");
            if let Some(memo_modifier) = state.mouse_input.variable_memo_modifier {
                ui.label(format!(
                    "左ダブルクリック: 0.5秒メモを追加　　{}+左ドラッグ: 可変長メモを追加",
                    memo_modifier.label()
                ));
            } else {
                ui.label("左ダブルクリック: 0.5秒メモを追加　　可変長設置: 無効（設定から変更）");
            }
            ui.label("右クリックでメモを削除します。再生中は有効なLoop範囲内で編集できます。");
            ui.separator();
            ui.strong("表示移動");
            ui.label(format!(
                "時間ズーム: {} + ホイール　　音高ズーム: {} + ホイール　　時間移動: {} + ホイール",
                state.mouse_input.time_zoom_modifier.label(),
                state.mouse_input.pitch_zoom_modifier.label(),
                state.mouse_input.time_pan_modifier.label(),
            ));
            ui.label(
                "下部バーのドラッグでも時間表示を、右側バーのドラッグでも音高表示を移動できます。",
            );
            ui.separator();
            ui.small("分析用の調整は分析ツールから、マウス操作は設定から変更できます。");
        });
    state.help_popup_open = help_popup_open;
}

fn show_analysis_tools_dialog(ctx: &egui::Context, state: &mut AppState) {
    let mut analysis_tools_popup_open = state.analysis_tools_popup_open;
    egui::Window::new("分析ツール")
        .open(&mut analysis_tools_popup_open)
        .resizable(true)
        .scroll([true, true])
        .default_width(360.0)
        .default_height(540.0)
        .min_size([280.0, 220.0])
        .show(ctx, |ui| {
            show_settings_section(
                ui,
                "settings_equalizer",
                "EQ",
                "再生停止中に調整できます。表示にも反映されます。",
                |ui| show_equalizer_settings(ui, state),
            );
            ui.separator();

            show_settings_section(
                ui,
                "settings_fundamental",
                "基音強調",
                "倍音列から基音候補を強調します。再生音には影響しません。",
                |ui| show_fundamental_settings(ui, state),
            );
            ui.separator();

            show_settings_section(ui, "settings_timbre", "音色", "", |ui| {
                show_timbre_settings(ui, state);
            });
            ui.separator();

            show_settings_section(
                ui,
                "settings_tuning",
                "チューニング",
                "元音源に合わせて、スペクトログラム押下時の試聴音だけを調整します。",
                |ui| {
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut state.project.data.project_settings.preview_reference_a4_hz,
                                430.0..=450.0,
                            )
                            .text("A4 基準ピッチ")
                            .suffix(" Hz")
                            .step_by(0.1),
                        )
                        .changed()
                    {
                        state.project.editing.dirty = true;
                    }
                },
            );
            ui.separator();

            show_settings_section(
                ui,
                "settings_layers",
                "レイヤー",
                "入力先を選択し、レイヤーごとの表示・試聴設定を調整します。",
                |ui| show_layer_settings(ui, state),
            );
        });
    state.analysis_tools_popup_open = analysis_tools_popup_open;
}

fn show_mouse_settings_dialog(ctx: &egui::Context, state: &mut AppState) -> bool {
    let mut mouse_settings_popup_open = state.mouse_settings_popup_open;
    let mut mouse_input_changed = false;
    egui::Window::new("マウス操作設定")
        .open(&mut mouse_settings_popup_open)
        .resizable(true)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.label("ホイール操作と可変長メモ設置の修飾キーを設定します。");
            ui.separator();
            mouse_input_changed |= show_mouse_input_settings(ui, &mut state.mouse_input);
        });
    state.mouse_settings_popup_open = mouse_settings_popup_open;
    mouse_input_changed
}

fn show_settings_section(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    label: &'static str,
    help: &'static str,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(id_salt),
        true,
    )
    .show_header(ui, |ui| {
        let label_response = ui.strong(label);
        if !help.is_empty() {
            label_response.on_hover_text_at_pointer(help);
            let info_button = ui
                .add_sized([28.0, 22.0], egui::Button::new("ⓘ").frame(false))
                .on_hover_text_at_pointer(help);
            egui::Popup::menu(&info_button)
                .id(ui.make_persistent_id((id_salt, "help")))
                .show(|ui| {
                    ui.set_max_width(300.0);
                    ui.label(help);
                });
        }
    })
    .body(body);
}

fn show_equalizer_settings(ui: &mut egui::Ui, state: &mut AppState) {
    let mut changed = false;
    ui.add_enabled_ui(state.track.is_some() && !state.playback.playing, |ui| {
        for (index, frequency_hz) in EQ_BAND_FREQUENCIES_HZ.iter().copied().enumerate() {
            changed |= ui
                .add(
                    egui::Slider::new(
                        &mut state.project.data.project_settings.equalizer_gains_db[index],
                        -12.0..=12.0,
                    )
                    .text(format_frequency(frequency_hz))
                    .suffix(" dB")
                    .step_by(0.5),
                )
                .changed();
        }
        if ui.button("Reset EQ").clicked() {
            changed |=
                state.project.data.project_settings.equalizer_gains_db != [0.0; EQ_BAND_COUNT];
            state.project.data.project_settings.equalizer_gains_db = [0.0; EQ_BAND_COUNT];
        }
    });
    state.project.editing.dirty |= changed;
}

fn show_fundamental_settings(ui: &mut egui::Ui, state: &mut AppState) {
    let analysis = &mut state.project.data.project_settings.fundamental_analysis;
    let mut changed = ui
        .add(
            egui::Slider::new(&mut analysis.emphasis, 0.0..=100.0)
                .text("強調")
                .suffix(" %")
                .step_by(5.0),
        )
        .changed();
    let mut scale_changed = false;
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Root")
            .selected_text(PITCH_CLASS_NAMES[analysis.scale_root])
            .show_ui(ui, |ui| {
                for (index, name) in PITCH_CLASS_NAMES.iter().enumerate() {
                    scale_changed |= ui
                        .selectable_value(&mut analysis.scale_root, index, *name)
                        .changed();
                }
            });
        egui::ComboBox::from_label("Scale")
            .selected_text(analysis.scale_preset.label())
            .show_ui(ui, |ui| {
                for preset in ScalePreset::ALL {
                    scale_changed |= ui
                        .selectable_value(&mut analysis.scale_preset, preset, preset.label())
                        .changed();
                }
            });
    });
    if scale_changed {
        analysis.apply_scale_preset();
    }
    changed |= scale_changed;
    ui.label("強調する音（クリックで個別に変更）");
    egui::Grid::new("emphasized_pitch_classes")
        .num_columns(4)
        .show(ui, |ui| {
            for (index, name) in PITCH_CLASS_NAMES.iter().enumerate() {
                changed |= ui
                    .checkbox(&mut analysis.emphasized_pitch_classes[index], *name)
                    .changed();
                if index % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    changed |= ui
        .add(
            egui::Slider::new(&mut analysis.unemphasized_pitch_attenuation, 0.0..=100.0)
                .text("指定外の減衰")
                .suffix(" %")
                .step_by(5.0),
        )
        .changed();
    state.project.editing.dirty |= changed;
}

fn show_timbre_settings(ui: &mut egui::Ui, state: &mut AppState) {
    for timbre in PreviewTimbre::ALL {
        ui.radio_value(&mut state.preview_timbre, timbre, timbre.label());
    }
}

fn show_mouse_input_settings(ui: &mut egui::Ui, settings: &mut MouseInputSettings) -> bool {
    let mut changed = false;
    let preset_label = settings
        .preset()
        .map(MousePreset::label)
        .unwrap_or("カスタム");
    egui::ComboBox::from_label("操作プリセット")
        .selected_text(preset_label)
        .show_ui(ui, |ui| {
            for preset in MousePreset::ALL {
                if ui
                    .selectable_label(settings.preset() == Some(preset), preset.label())
                    .clicked()
                {
                    *settings = MouseInputSettings::for_preset(preset);
                    changed = true;
                }
            }
        });
    ui.small("個別に変更すると表示は「カスタム」になります。");

    ui.add_space(4.0);
    ui.strong("ホイール操作");
    changed |= show_wheel_modifier_selector(ui, settings, WheelAction::TimeZoom, "時間ズーム");
    changed |= show_wheel_modifier_selector(ui, settings, WheelAction::PitchZoom, "音高ズーム");
    changed |= show_wheel_modifier_selector(ui, settings, WheelAction::TimePan, "時間移動");

    ui.add_space(4.0);
    ui.strong("可変長メモ設置");
    egui::ComboBox::from_label("左ドラッグの修飾キー")
        .selected_text(
            settings
                .variable_memo_modifier
                .map(VariableMemoModifier::label)
                .unwrap_or("無効"),
        )
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(&mut settings.variable_memo_modifier, None, "無効")
                .changed();
            for modifier in VariableMemoModifier::ALL {
                changed |= ui
                    .selectable_value(
                        &mut settings.variable_memo_modifier,
                        Some(modifier),
                        modifier.label(),
                    )
                    .changed();
            }
        });
    ui.small("修飾キーを押しながらスペクトログラムを左ドラッグすると可変長メモを作成します。");

    changed
}

fn show_wheel_modifier_selector(
    ui: &mut egui::Ui,
    settings: &mut MouseInputSettings,
    action: WheelAction,
    label: &str,
) -> bool {
    let current = match action {
        WheelAction::TimeZoom => settings.time_zoom_modifier,
        WheelAction::PitchZoom => settings.pitch_zoom_modifier,
        WheelAction::TimePan => settings.time_pan_modifier,
    };
    let mut changed = false;
    egui::ComboBox::from_label(label)
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for modifier in WheelModifier::ALL {
                if ui
                    .selectable_label(modifier == current, modifier.label())
                    .clicked()
                {
                    changed |= settings.set_wheel_modifier(action, modifier);
                }
            }
        });
    changed
}

fn show_layer_settings(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("一括設定");
    ui.horizontal_wrapped(|ui| {
        let mut all_volume = state
            .project
            .data
            .layers
            .first()
            .map(|layer| layer.volume)
            .unwrap_or(0.0);
        if ui
            .add(
                egui::Slider::new(&mut all_volume, 0.0..=1.0)
                    .text("全レイヤー音量")
                    .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
            )
            .changed()
        {
            state.project.set_all_layers_volume(all_volume);
        }

        let mut all_visible = state.project.data.layers.iter().all(|layer| layer.visible);
        if ui.checkbox(&mut all_visible, "全表示").changed() {
            state.project.set_all_layers_visible(all_visible);
        }

        let mut all_muted = state.project.data.layers.iter().all(|layer| layer.muted);
        if ui.checkbox(&mut all_muted, "全ミュート").changed() {
            state.project.set_all_layers_muted(all_muted);
        }

        let mut all_opacity = state
            .project
            .data
            .layers
            .first()
            .map(|layer| layer.opacity)
            .unwrap_or(0.0);
        if ui
            .add(
                egui::Slider::new(&mut all_opacity, 0.0..=1.0)
                    .text("全レイヤー透明度")
                    .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
            )
            .changed()
        {
            state.project.set_all_layers_opacity(all_opacity);
        }
    });
    ui.separator();

    let selected_layer_id = state.project.editing.selected_layer_id;
    let mut next_selected_layer_id = None;
    let mut persistent_changed = false;

    for layer in &mut state.project.data.layers {
        let layer_id = layer.id;
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(selected_layer_id == Some(layer_id), "入力")
                    .on_hover_text("このレイヤーを音高メモの入力先にします")
                    .clicked()
                {
                    next_selected_layer_id = Some(layer_id);
                }
                ui.label(egui::RichText::new("■").color(layer_color(layer_id.get())));
                persistent_changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut layer.name)
                            .desired_width(120.0)
                            .hint_text("レイヤー名"),
                    )
                    .changed();
                ui.checkbox(&mut layer.visible, "表示");
                let mute_label = if layer.muted { "Unmute" } else { "Mute" };
                if ui.button(mute_label).clicked() {
                    layer.muted = !layer.muted;
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut layer.volume, 0.0..=1.0)
                        .text("音量")
                        .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                );
                ui.add(
                    egui::Slider::new(&mut layer.opacity, 0.0..=1.0)
                        .text("透明度")
                        .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                );
            });
        });
    }

    if let Some(layer_id) = next_selected_layer_id {
        state.project.editing.selected_layer_id = Some(layer_id);
        state.project.editing.selected_memo = None;
    }
    state.project.editing.dirty |= persistent_changed;
}

fn format_frequency(frequency_hz: f32) -> String {
    if frequency_hz >= 1_000.0 {
        format!("{:.0} kHz", frequency_hz / 1_000.0)
    } else {
        format!("{frequency_hz:.0} Hz")
    }
}

fn format_mm_ss(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).floor() as u64;
    let minutes = total_seconds / 60;
    let secs = total_seconds % 60;
    format!("{minutes:02}:{secs:02}")
}

fn format_pitch_label(semitones: i32) -> String {
    match semitones {
        -12 => "-1 oct".to_owned(),
        12 => "+1 oct".to_owned(),
        0 => "0 st".to_owned(),
        _ => format!("{semitones:+} st"),
    }
}
