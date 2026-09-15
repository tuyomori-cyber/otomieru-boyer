use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, TextureHandle, Vec2};

use crate::app::input::WheelAction;
use crate::app::state::AppState;
use crate::model::{
    DEFAULT_MEMO_DURATION_SECONDS, EqualizerSettings, PitchMemoLayer, SelectedMemo,
};

const MEMO_HANDLE_HIT_RADIUS: f32 = 7.0;
const MIN_MEMO_DURATION_SECONDS: f64 = 0.01;
const TIME_PAN_VIEW_FRACTION: f64 = 0.15;
const MEMO_EDIT_DRAG_ID: &str = "pitch-memo-edit-drag";
const MEMO_CREATE_DRAG_ID: &str = "pitch-memo-create-drag";
const VIEW_SCROLLBAR_DRAG_ID: &str = "view-scrollbar-drag";
const PITCH_SCROLLBAR_WIDTH: f32 = 10.0;
const PITCH_SCROLLBAR_MARGIN: f32 = 6.0;

#[derive(Debug, Default, Clone, Copy)]
pub struct SpectrogramActions {
    pub seek_seconds: Option<f64>,
    pub view_start_seconds: Option<f64>,
    pub pitch_view_center_midi: Option<f64>,
    pub zoom_at: Option<(f64, f64)>,
    pub pitch_zoom_at: Option<(f64, f64)>,
    pub preview_midi_note: Option<u8>,
    pub stop_preview: bool,
}

#[derive(Default)]
pub struct SpectrogramCache {
    key: Option<SpectrogramCacheKey>,
    texture: Option<TextureHandle>,
}

impl SpectrogramCache {
    pub fn clear(&mut self) {
        self.key = None;
        self.texture = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpectrogramCacheKey {
    data_address: usize,
    view_start_bits: u64,
    view_end_bits: u64,
    first_pitch: usize,
    last_pitch_exclusive: usize,
    width_pixels: usize,
    height_pixels: usize,
    gain_bits: u32,
    emphasis_bits: u32,
    emphasized_pitch_classes: [bool; 12],
    attenuation_bits: u32,
    equalizer_gain_bits: [u32; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
struct MemoHit {
    selected: SelectedMemo,
    edge: Option<MemoEdge>,
}

#[derive(Debug, Clone, Copy)]
enum MemoHoverCursor {
    Resize,
    Move(egui::Pos2),
}

#[derive(Debug, Clone, Copy)]
enum ViewScrollbarDrag {
    Time { thumb_offset_ratio: f64 },
    Pitch { thumb_offset_ratio: f64 },
}

#[derive(Debug, Clone, Copy)]
struct MemoResizeDrag {
    selected: SelectedMemo,
    edge: MemoEdge,
    original_start_sec: f64,
    original_duration_sec: f64,
    pitch_midi: i32,
    proposed_start_sec: f64,
    proposed_duration_sec: f64,
}

#[derive(Debug, Clone, Copy)]
struct MemoMoveDrag {
    selected: SelectedMemo,
    duration_sec: f64,
    start_offset_sec: f64,
    pitch_offset_midi: i32,
    proposed_start_sec: f64,
    proposed_pitch_midi: i32,
}

#[derive(Debug, Clone, Copy)]
struct MemoCreateDrag {
    layer_id: crate::model::LayerId,
    pressed_sec: f64,
    proposed_end_sec: f64,
    pitch_midi: i32,
}

impl MemoCreateDrag {
    fn proposed_bounds(self, audio_duration: f64) -> (f64, f64, i32) {
        let (start_sec, duration_sec) =
            created_memo_bounds(self.pressed_sec, self.proposed_end_sec, audio_duration);
        (start_sec, duration_sec, self.pitch_midi)
    }
}

#[derive(Debug, Clone, Copy)]
enum MemoEditDrag {
    Resize(MemoResizeDrag),
    Move(MemoMoveDrag),
}

impl MemoEditDrag {
    fn selected(self) -> SelectedMemo {
        match self {
            Self::Resize(drag) => drag.selected,
            Self::Move(drag) => drag.selected,
        }
    }

    fn proposed_bounds(self) -> (f64, f64, i32) {
        match self {
            Self::Resize(drag) => (
                drag.proposed_start_sec,
                drag.proposed_duration_sec,
                drag.pitch_midi,
            ),
            Self::Move(drag) => (
                drag.proposed_start_sec,
                drag.duration_sec,
                drag.proposed_pitch_midi,
            ),
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut AppState,
    height: f32,
    cache: &mut SpectrogramCache,
) -> SpectrogramActions {
    let mut actions = SpectrogramActions::default();
    let pixels_per_point = ui.ctx().pixels_per_point();
    let desired_size = Vec2::new((ui.available_width() - 8.0).max(240.0), height.max(240.0));
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, Color32::from_rgb(15, 24, 35));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(56, 82, 102)),
        egui::StrokeKind::Inside,
    );

    let view_start = state.current_view_start_seconds();
    let view_end = state.current_view_end_seconds();
    let view_duration = (view_end - view_start).max(0.001);
    let playhead_visible = state.display_playhead_position_seconds >= view_start
        && state.display_playhead_position_seconds <= view_end;
    let normalized =
        ((state.display_playhead_position_seconds - view_start) / view_duration).clamp(0.0, 1.0);
    let content_rect = egui::Rect::from_min_max(
        rect.left_top(),
        rect.right_bottom()
            - egui::vec2(PITCH_SCROLLBAR_WIDTH + PITCH_SCROLLBAR_MARGIN * 2.0, 40.0),
    );
    let current_x = egui::lerp(
        content_rect.left()..=content_rect.right(),
        normalized as f32,
    );
    let pitch_scrollbar_rect = egui::Rect::from_min_max(
        egui::pos2(
            content_rect.right() + PITCH_SCROLLBAR_MARGIN,
            content_rect.top(),
        ),
        egui::pos2(rect.right() - PITCH_SCROLLBAR_MARGIN, content_rect.bottom()),
    );
    draw_spectrogram_body(&painter, content_rect, state, view_start, view_end, cache);
    let (memo_edit_drag, memo_create_drag) =
        handle_pitch_memo_interaction(ui, &response, state, content_rect, view_start, view_end);
    let memo_hover_cursor = state
        .can_edit_pitch_memos()
        .then(|| memo_hover_cursor(&response, state, content_rect, view_start, view_end))
        .flatten();
    if let Some(cursor) = memo_hover_cursor {
        ui.output_mut(|output| {
            output.cursor_icon = match cursor {
                MemoHoverCursor::Resize => egui::CursorIcon::ResizeHorizontal,
                // 一部のLinuxカーソルテーマではMoveが□に代替されるため、
                // 中央の移動カーソルはアプリ側で描画する。
                MemoHoverCursor::Move(_) => egui::CursorIcon::None,
            };
        });
    }
    draw_pitch_memos(
        &painter,
        content_rect,
        state,
        view_start,
        view_end,
        memo_edit_drag.as_ref(),
    );
    if let Some(create_drag) = memo_create_drag {
        draw_memo_creation_preview(
            &painter,
            content_rect,
            state,
            view_start,
            view_end,
            create_drag,
        );
    }
    if let Some(MemoHoverCursor::Move(position)) = memo_hover_cursor {
        draw_memo_move_cursor(&painter, position);
    }
    draw_pitch_scrollbar(&painter, pitch_scrollbar_rect, state);

    let page_bar_height = 16.0;
    let page_bar_margin = 14.0;
    let page_bar_rect = egui::Rect::from_min_max(
        egui::pos2(content_rect.left() + page_bar_margin, rect.bottom() - 28.0),
        egui::pos2(
            content_rect.right() - page_bar_margin,
            rect.bottom() - 28.0 + page_bar_height,
        ),
    );

    if playhead_visible {
        draw_subpixel_playhead(
            &painter,
            pixels_per_point,
            content_rect.top(),
            content_rect.bottom(),
            current_x,
        );
    }

    painter.rect_filled(
        page_bar_rect,
        999.0,
        Color32::from_rgba_premultiplied(255, 255, 255, 24),
    );
    painter.rect_stroke(
        page_bar_rect,
        999.0,
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 48)),
        egui::StrokeKind::Inside,
    );

    let duration = state
        .track
        .as_ref()
        .map(|track| track.duration_seconds)
        .unwrap_or(0.0)
        .max(0.001);

    let current_view_left = egui::lerp(
        page_bar_rect.left()..=page_bar_rect.right(),
        (view_start / duration).clamp(0.0, 1.0) as f32,
    );
    let current_view_right = egui::lerp(
        page_bar_rect.left()..=page_bar_rect.right(),
        (view_end / duration).clamp(0.0, 1.0) as f32,
    );
    let current_view_rect = egui::Rect::from_min_max(
        egui::pos2(current_view_left, page_bar_rect.top()),
        egui::pos2(
            current_view_right.max(current_view_left + 2.0),
            page_bar_rect.bottom(),
        ),
    );
    painter.rect_filled(current_view_rect, 4.0, Color32::from_rgb(90, 168, 204));

    for segment in 1..state.spectrogram_view().total_segments {
        let x = egui::lerp(
            page_bar_rect.left()..=page_bar_rect.right(),
            segment as f32 / state.spectrogram_view().total_segments as f32,
        );
        painter.line_segment(
            [
                egui::pos2(x, page_bar_rect.top()),
                egui::pos2(x, page_bar_rect.bottom()),
            ],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 30)),
        );
    }

    let playhead_bar_x = egui::lerp(
        page_bar_rect.left()..=page_bar_rect.right(),
        (state.display_playhead_position_seconds / duration).clamp(0.0, 1.0) as f32,
    );
    draw_subpixel_playhead(
        &painter,
        pixels_per_point,
        page_bar_rect.top(),
        page_bar_rect.bottom(),
        playhead_bar_x,
    );

    painter.text(
        rect.left_top() + egui::vec2(16.0, 16.0),
        Align2::LEFT_TOP,
        "Spectrogram",
        FontId::proportional(18.0),
        Color32::from_rgb(221, 235, 245),
    );

    painter.text(
        rect.left_top() + egui::vec2(16.0, 42.0),
        Align2::LEFT_TOP,
        format!(
            "View | {:.2} - {:.2} sec | {:.1}x\nUI: {:.1} FPS | {:.1} ms | Scale: {:.2}",
            view_start,
            view_end,
            state.view_zoom,
            state.ui_frame_metrics.frames_per_second,
            state.ui_frame_metrics.frame_time_ms,
            state.ui_frame_metrics.pixels_per_point,
        ),
        FontId::proportional(16.0),
        Color32::from_rgb(175, 205, 220),
    );

    painter.text(
        page_bar_rect.left_bottom() + egui::vec2(0.0, 20.0),
        Align2::LEFT_BOTTOM,
        "0s",
        FontId::proportional(13.0),
        Color32::from_rgb(165, 188, 204),
    );
    painter.text(
        page_bar_rect.right_bottom() + egui::vec2(0.0, 20.0),
        Align2::RIGHT_BOTTOM,
        format!("{:.0}s", duration),
        FontId::proportional(13.0),
        Color32::from_rgb(165, 188, 204),
    );

    let scrollbar_drag_id = ui.id().with(VIEW_SCROLLBAR_DRAG_ID);
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(pressed_pos) = ui.input(|input| input.pointer.press_origin())
    {
        let drag = if page_bar_rect.contains(pressed_pos) {
            let thumb_offset_ratio = if current_view_rect.contains(pressed_pos) {
                ((pressed_pos.x - current_view_rect.left()) / current_view_rect.width())
                    .clamp(0.0, 1.0) as f64
            } else {
                0.5
            };
            Some(ViewScrollbarDrag::Time { thumb_offset_ratio })
        } else if pitch_scrollbar_rect.contains(pressed_pos) {
            let pitch_thumb_rect = pitch_scrollbar_thumb_rect(pitch_scrollbar_rect, state);
            let thumb_offset_ratio = if pitch_thumb_rect.contains(pressed_pos) {
                ((pressed_pos.y - pitch_thumb_rect.top()) / pitch_thumb_rect.height())
                    .clamp(0.0, 1.0) as f64
            } else {
                0.5
            };
            Some(ViewScrollbarDrag::Pitch { thumb_offset_ratio })
        } else {
            None
        };
        if let Some(drag) = drag {
            ui.ctx()
                .data_mut(|data| data.insert_temp(scrollbar_drag_id, drag));
        }
    }

    let scrollbar_drag = ui
        .ctx()
        .data(|data| data.get_temp::<ViewScrollbarDrag>(scrollbar_drag_id));
    if let Some(drag) = scrollbar_drag
        && let Some(pointer_pos) = response.interact_pointer_pos()
    {
        match drag {
            ViewScrollbarDrag::Time { thumb_offset_ratio } => {
                let pointer_t = ((pointer_pos.x - page_bar_rect.left()) / page_bar_rect.width())
                    .clamp(0.0, 1.0) as f64;
                actions.view_start_seconds = Some(time_view_start_for_thumb_position(
                    duration,
                    view_duration,
                    pointer_t,
                    thumb_offset_ratio,
                ));
            }
            ViewScrollbarDrag::Pitch { thumb_offset_ratio } => {
                let pointer_t = ((pointer_pos.y - pitch_scrollbar_rect.top())
                    / pitch_scrollbar_rect.height())
                .clamp(0.0, 1.0) as f64;
                actions.pitch_view_center_midi = Some(pitch_scroll_center_for_thumb_position(
                    state.full_pitch_view(),
                    state.pitch_view(),
                    pointer_t,
                    thumb_offset_ratio,
                ));
            }
        }
    } else if let Some(pointer_pos) = response.hover_pos() {
        if pitch_scrollbar_rect.contains(pointer_pos) {
            if response.clicked() {
                let pointer_t = ((pointer_pos.y - pitch_scrollbar_rect.top())
                    / pitch_scrollbar_rect.height())
                .clamp(0.0, 1.0) as f64;
                actions.pitch_view_center_midi = Some(pitch_scroll_center_for_thumb_position(
                    state.full_pitch_view(),
                    state.pitch_view(),
                    pointer_t,
                    0.5,
                ));
            }
        } else if page_bar_rect.contains(pointer_pos) {
            if response.clicked() {
                let t = ((pointer_pos.x - page_bar_rect.left()) / page_bar_rect.width())
                    .clamp(0.0, 1.0) as f64;
                actions.view_start_seconds = Some(time_view_start_for_thumb_position(
                    duration,
                    view_duration,
                    t,
                    0.5,
                ));
            }
        } else if content_rect.contains(pointer_pos) {
            let (raw_scroll_delta, modifiers) =
                ui.input(|input| (input.raw_scroll_delta, input.modifiers));
            let scroll_delta = scroll_delta_for_modifiers(raw_scroll_delta, modifiers);
            if scroll_delta.abs() > f32::EPSILON
                && memo_edit_drag.is_none()
                && let Some(action) = state.mouse_input.wheel_action(modifiers)
            {
                match action {
                    WheelAction::TimePan => {
                        actions.view_start_seconds = Some(
                            view_start + time_pan_delta_for_scroll(scroll_delta, view_duration),
                        );
                    }
                    WheelAction::PitchZoom => {
                        let factor = if scroll_delta > 0.0 { 1.25 } else { 0.8 };
                        let pitch_view = state.pitch_view();
                        let pointer_t = ((content_rect.bottom() - pointer_pos.y)
                            / content_rect.height())
                        .clamp(0.0, 1.0) as f64;
                        actions.pitch_zoom_at = Some((
                            pitch_view.min_midi_note as f64
                                + pointer_t * pitch_view.pitch_count() as f64,
                            factor,
                        ));
                    }
                    WheelAction::TimeZoom => {
                        let factor = if scroll_delta > 0.0 { 1.25 } else { 0.8 };
                        let pointer_t = ((pointer_pos.x - content_rect.left())
                            / content_rect.width())
                        .clamp(0.0, 1.0) as f64;
                        actions.zoom_at = Some((view_start + view_duration * pointer_t, factor));
                    }
                }
            }
            if !state.playback.playing
                && response.clicked()
                && !response.double_clicked_by(egui::PointerButton::Primary)
                && !state.mouse_input.variable_memo_modifier_matches(modifiers)
            {
                let t = ((pointer_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
                actions.seek_seconds = Some(view_start + view_duration * t);
            }

            let pointer_down = ui.input(|input| input.pointer.primary_down());
            if pointer_down
                && !state.mouse_input.variable_memo_modifier_matches(modifiers)
                && memo_create_drag.is_none()
                && let Some(track) = &state.track
                && track.spectrogram.is_some()
            {
                let pitch_t = ((content_rect.bottom() - pointer_pos.y) / content_rect.height())
                    .clamp(0.0, 0.999_999);
                let pitch_view = state.pitch_view();
                let midi_note = pitch_view.min_midi_note
                    + (pitch_t * pitch_view.pitch_count() as f32).floor() as usize;
                actions.preview_midi_note = Some(midi_note as u8);
            } else if state.preview_midi_note.is_some() {
                actions.stop_preview = true;
            }
        }
    } else if state.preview_midi_note.is_some() && !ui.input(|input| input.pointer.primary_down()) {
        actions.stop_preview = true;
    }

    if response.drag_stopped_by(egui::PointerButton::Primary) {
        ui.ctx()
            .data_mut(|data| data.remove::<ViewScrollbarDrag>(scrollbar_drag_id));
    }

    actions
}

fn handle_pitch_memo_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    state: &mut AppState,
    content_rect: egui::Rect,
    view_start: f64,
    view_end: f64,
) -> (Option<MemoEditDrag>, Option<MemoCreateDrag>) {
    let audio_duration = state
        .track
        .as_ref()
        .map(|track| track.duration_seconds)
        .unwrap_or(0.0);
    if audio_duration <= 0.0 || !state.can_edit_pitch_memos() {
        return (None, None);
    }

    if response.double_clicked_by(egui::PointerButton::Primary)
        && !ui.input(|input| {
            state
                .mouse_input
                .variable_memo_modifier_matches(input.modifiers)
        })
        && let Some(pointer_pos) = response.interact_pointer_pos()
        && content_rect.contains(pointer_pos)
        && let Some(layer_id) = state.project.editing.selected_layer_id
    {
        let duration_sec = DEFAULT_MEMO_DURATION_SECONDS.min(audio_duration);
        let clicked_sec = x_to_time(pointer_pos.x, view_start, view_end, content_rect);
        let start_sec = clicked_sec.min((audio_duration - duration_sec).max(0.0));
        let pitch_midi = y_to_pitch(pointer_pos.y, state, content_rect);
        state
            .project
            .add_memo(layer_id, start_sec, duration_sec, pitch_midi);
    }

    let create_drag_id = ui.id().with(MEMO_CREATE_DRAG_ID);
    if response.drag_started_by(egui::PointerButton::Primary)
        && ui.input(|input| {
            state
                .mouse_input
                .variable_memo_modifier_matches(input.modifiers)
        })
        && let Some(pressed_pos) = ui.input(|input| input.pointer.press_origin())
        && content_rect.contains(pressed_pos)
        && let Some(layer_id) = state.project.editing.selected_layer_id
    {
        let pressed_sec = x_to_time(pressed_pos.x, view_start, view_end, content_rect);
        let drag = MemoCreateDrag {
            layer_id,
            pressed_sec,
            proposed_end_sec: pressed_sec,
            pitch_midi: y_to_pitch(pressed_pos.y, state, content_rect),
        };
        ui.ctx()
            .data_mut(|data| data.insert_temp(create_drag_id, drag));
    }

    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(pointer_pos) = response.interact_pointer_pos()
        && let Some(mut drag) = ui
            .ctx()
            .data(|data| data.get_temp::<MemoCreateDrag>(create_drag_id))
    {
        drag.proposed_end_sec = x_to_time(pointer_pos.x, view_start, view_end, content_rect);
        ui.ctx()
            .data_mut(|data| data.insert_temp(create_drag_id, drag));
    }

    let stopped_create_drag = response
        .drag_stopped_by(egui::PointerButton::Primary)
        .then(|| {
            let drag = ui
                .ctx()
                .data(|data| data.get_temp::<MemoCreateDrag>(create_drag_id));
            ui.ctx()
                .data_mut(|data| data.remove::<MemoCreateDrag>(create_drag_id));
            drag
        })
        .flatten();
    if let Some(drag) = stopped_create_drag {
        let (start_sec, duration_sec, pitch_midi) = drag.proposed_bounds(audio_duration);
        state
            .project
            .add_memo(drag.layer_id, start_sec, duration_sec, pitch_midi);
    }

    let drag_id = ui.id().with(MEMO_EDIT_DRAG_ID);
    if response.drag_started_by(egui::PointerButton::Secondary)
        && let Some(pressed_pos) = ui.input(|input| input.pointer.press_origin())
        && let Some(hit) = find_memo_hit(pressed_pos, state, content_rect, view_start, view_end)
        && let Some(memo) = state.project.memo(hit.selected)
    {
        let pressed_sec = x_to_time(pressed_pos.x, view_start, view_end, content_rect);
        let pressed_pitch_midi = y_to_pitch(pressed_pos.y, state, content_rect);
        let drag = match hit.edge {
            Some(edge) => MemoEditDrag::Resize(MemoResizeDrag {
                selected: hit.selected,
                edge,
                original_start_sec: memo.start_sec,
                original_duration_sec: memo.duration_sec,
                pitch_midi: memo.pitch_midi,
                proposed_start_sec: memo.start_sec,
                proposed_duration_sec: memo.duration_sec,
            }),
            None => MemoEditDrag::Move(MemoMoveDrag {
                selected: hit.selected,
                duration_sec: memo.duration_sec,
                start_offset_sec: pressed_sec - memo.start_sec,
                pitch_offset_midi: pressed_pitch_midi - memo.pitch_midi,
                proposed_start_sec: memo.start_sec,
                proposed_pitch_midi: memo.pitch_midi,
            }),
        };
        state.project.editing.selected_layer_id = Some(hit.selected.layer_id);
        state.project.editing.selected_memo = Some(hit.selected);
        ui.ctx().data_mut(|data| data.insert_temp(drag_id, drag));
    }

    if response.dragged_by(egui::PointerButton::Secondary)
        && let Some(pointer_pos) = response.interact_pointer_pos()
        && let Some(mut drag) = ui.ctx().data(|data| data.get_temp::<MemoEditDrag>(drag_id))
    {
        let pointer_sec = x_to_time(pointer_pos.x, view_start, view_end, content_rect);
        let pointer_pitch_midi = y_to_pitch(pointer_pos.y, state, content_rect);
        match &mut drag {
            MemoEditDrag::Resize(drag) => {
                (drag.proposed_start_sec, drag.proposed_duration_sec) = resized_memo_bounds(
                    drag.edge,
                    drag.original_start_sec,
                    drag.original_duration_sec,
                    pointer_sec,
                    audio_duration,
                );
            }
            MemoEditDrag::Move(drag) => {
                (drag.proposed_start_sec, drag.proposed_pitch_midi) = moved_memo_position(
                    drag.duration_sec,
                    drag.start_offset_sec,
                    drag.pitch_offset_midi,
                    pointer_sec,
                    pointer_pitch_midi,
                    audio_duration,
                );
            }
        }
        ui.ctx().data_mut(|data| data.insert_temp(drag_id, drag));
    }

    let stopped_drag = response
        .drag_stopped_by(egui::PointerButton::Secondary)
        .then(|| {
            let drag = ui.ctx().data(|data| data.get_temp::<MemoEditDrag>(drag_id));
            ui.ctx()
                .data_mut(|data| data.remove::<MemoEditDrag>(drag_id));
            drag
        })
        .flatten();
    if let Some(drag) = stopped_drag {
        let (start_sec, duration_sec, pitch_midi) = drag.proposed_bounds();
        state
            .project
            .update_memo(drag.selected(), start_sec, duration_sec, pitch_midi);
    } else if response.clicked_by(egui::PointerButton::Secondary)
        && let Some(pointer_pos) = response.interact_pointer_pos()
        && content_rect.contains(pointer_pos)
        && let Some(hit) = find_memo_hit(pointer_pos, state, content_rect, view_start, view_end)
    {
        state.project.delete_memo(hit.selected);
    }

    (
        ui.ctx().data(|data| data.get_temp::<MemoEditDrag>(drag_id)),
        ui.ctx()
            .data(|data| data.get_temp::<MemoCreateDrag>(create_drag_id)),
    )
}

fn memo_hover_cursor(
    response: &egui::Response,
    state: &AppState,
    content_rect: egui::Rect,
    view_start: f64,
    view_end: f64,
) -> Option<MemoHoverCursor> {
    let pointer_pos = response.hover_pos()?;
    if !content_rect.contains(pointer_pos) {
        return None;
    }
    let hit = find_memo_hit(pointer_pos, state, content_rect, view_start, view_end)?;
    Some(if hit.edge.is_some() {
        MemoHoverCursor::Resize
    } else {
        MemoHoverCursor::Move(pointer_pos)
    })
}

fn draw_memo_move_cursor(painter: &egui::Painter, position: egui::Pos2) {
    let dark = Stroke::new(3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 210));
    let light = Stroke::new(1.2, Color32::WHITE);
    for stroke in [dark, light] {
        painter.line_segment(
            [
                position + egui::vec2(-8.0, 0.0),
                position + egui::vec2(8.0, 0.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                position + egui::vec2(0.0, -8.0),
                position + egui::vec2(0.0, 8.0),
            ],
            stroke,
        );
        for (tip, first, second) in [
            (
                egui::vec2(-8.0, 0.0),
                egui::vec2(-4.5, -3.5),
                egui::vec2(-4.5, 3.5),
            ),
            (
                egui::vec2(8.0, 0.0),
                egui::vec2(4.5, -3.5),
                egui::vec2(4.5, 3.5),
            ),
            (
                egui::vec2(0.0, -8.0),
                egui::vec2(-3.5, -4.5),
                egui::vec2(3.5, -4.5),
            ),
            (
                egui::vec2(0.0, 8.0),
                egui::vec2(-3.5, 4.5),
                egui::vec2(3.5, 4.5),
            ),
        ] {
            painter.line_segment([position + tip, position + first], stroke);
            painter.line_segment([position + tip, position + second], stroke);
        }
    }
}

fn draw_pitch_memos(
    painter: &egui::Painter,
    content_rect: egui::Rect,
    state: &AppState,
    view_start: f64,
    view_end: f64,
    edit_drag: Option<&MemoEditDrag>,
) {
    let painter = painter.with_clip_rect(content_rect);
    let selected_layer_id = state.project.editing.selected_layer_id;

    for layer in state
        .project
        .data
        .layers
        .iter()
        .filter(|layer| Some(layer.id) != selected_layer_id)
    {
        draw_memo_layer(
            &painter,
            content_rect,
            state,
            layer,
            view_start,
            view_end,
            edit_drag,
        );
    }
    if let Some(layer) = state
        .project
        .data
        .layers
        .iter()
        .find(|layer| Some(layer.id) == selected_layer_id)
    {
        draw_memo_layer(
            &painter,
            content_rect,
            state,
            layer,
            view_start,
            view_end,
            edit_drag,
        );
    }
}

fn draw_memo_creation_preview(
    painter: &egui::Painter,
    content_rect: egui::Rect,
    state: &AppState,
    view_start: f64,
    view_end: f64,
    create_drag: MemoCreateDrag,
) {
    let Some(layer) = state
        .project
        .data
        .layers
        .iter()
        .find(|layer| layer.id == create_drag.layer_id && layer.visible)
    else {
        return;
    };
    let audio_duration = state
        .track
        .as_ref()
        .map(|track| track.duration_seconds)
        .unwrap_or(0.0);
    let (start_sec, duration_sec, pitch_midi) = create_drag.proposed_bounds(audio_duration);
    let Some(geometry) = memo_geometry(
        start_sec,
        duration_sec,
        pitch_midi,
        state,
        content_rect,
        view_start,
        view_end,
    ) else {
        return;
    };

    let base_color = layer_color(layer.id.get());
    let painter = painter.with_clip_rect(content_rect);
    painter.rect_filled(
        geometry.rect,
        2.0,
        Color32::from_rgba_unmultiplied(base_color.r(), base_color.g(), base_color.b(), 96),
    );
    painter.rect_stroke(
        geometry.rect,
        2.0,
        Stroke::new(1.5, Color32::WHITE),
        egui::StrokeKind::Inside,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_memo_layer(
    painter: &egui::Painter,
    content_rect: egui::Rect,
    state: &AppState,
    layer: &PitchMemoLayer,
    view_start: f64,
    view_end: f64,
    edit_drag: Option<&MemoEditDrag>,
) {
    if !layer.visible {
        return;
    }
    let base_color = layer_color(layer.id.get());
    let alpha = (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let fill =
        Color32::from_rgba_unmultiplied(base_color.r(), base_color.g(), base_color.b(), alpha);
    for memo in &layer.memos {
        let selected = SelectedMemo {
            layer_id: layer.id,
            memo_id: memo.id,
        };
        let (start_sec, duration_sec, pitch_midi) = edit_drag
            .filter(|drag| drag.selected() == selected)
            .map(|drag| drag.proposed_bounds())
            .unwrap_or((memo.start_sec, memo.duration_sec, memo.pitch_midi));
        let Some(geometry) = memo_geometry(
            start_sec,
            duration_sec,
            pitch_midi,
            state,
            content_rect,
            view_start,
            view_end,
        ) else {
            continue;
        };
        painter.rect_filled(geometry.rect, 2.0, fill);
        let is_selected = state.project.editing.selected_memo == Some(selected);
        painter.rect_stroke(
            geometry.rect,
            2.0,
            Stroke::new(
                if is_selected { 2.0 } else { 1.0 },
                memo_stroke_color(base_color, is_selected, alpha),
            ),
            egui::StrokeKind::Inside,
        );
        for x in [geometry.start_handle_x, geometry.end_handle_x]
            .into_iter()
            .flatten()
        {
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(x, geometry.rect.center().y),
                    egui::vec2(3.0, geometry.rect.height().max(5.0)),
                ),
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 210),
            );
        }
    }
}

fn memo_stroke_color(base_color: Color32, selected: bool, alpha: u8) -> Color32 {
    let color = if selected { Color32::WHITE } else { base_color };
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[derive(Debug, Clone, Copy)]
struct MemoGeometry {
    rect: egui::Rect,
    start_handle_x: Option<f32>,
    end_handle_x: Option<f32>,
}

fn memo_geometry(
    start_sec: f64,
    duration_sec: f64,
    pitch_midi: i32,
    state: &AppState,
    content_rect: egui::Rect,
    view_start: f64,
    view_end: f64,
) -> Option<MemoGeometry> {
    let end_sec = start_sec + duration_sec;
    if end_sec < view_start || start_sec > view_end {
        return None;
    }
    let pitch_view = state.pitch_view();
    if pitch_midi < pitch_view.min_midi_note as i32 || pitch_midi > pitch_view.max_midi_note as i32
    {
        return None;
    }
    let view_duration = (view_end - view_start).max(0.001);
    let raw_start_x = time_to_x(start_sec, view_start, view_duration, content_rect);
    let raw_end_x = time_to_x(end_sec, view_start, view_duration, content_rect);
    let start_x = raw_start_x.clamp(content_rect.left(), content_rect.right());
    let end_x = raw_end_x.clamp(content_rect.left(), content_rect.right());
    let pitch_index = pitch_midi as usize - pitch_view.min_midi_note;
    let pitch_count = pitch_view.pitch_count().max(1);
    let bottom = egui::lerp(
        content_rect.bottom()..=content_rect.top(),
        pitch_index as f32 / pitch_count as f32,
    );
    let top = egui::lerp(
        content_rect.bottom()..=content_rect.top(),
        (pitch_index + 1) as f32 / pitch_count as f32,
    );
    let rect = egui::Rect::from_min_max(
        egui::pos2(start_x, top + 1.0),
        egui::pos2(end_x.max(start_x + 1.0), bottom - 1.0),
    );
    Some(MemoGeometry {
        rect,
        start_handle_x: (start_sec >= view_start && start_sec <= view_end).then_some(raw_start_x),
        end_handle_x: (end_sec >= view_start && end_sec <= view_end).then_some(raw_end_x),
    })
}

fn find_memo_hit(
    pointer_pos: egui::Pos2,
    state: &AppState,
    content_rect: egui::Rect,
    view_start: f64,
    view_end: f64,
) -> Option<MemoHit> {
    let selected_layer_id = state.project.editing.selected_layer_id;
    if let Some(layer) = state
        .project
        .data
        .layers
        .iter()
        .find(|layer| Some(layer.id) == selected_layer_id && layer.visible)
        && let Some(hit) = find_memo_hit_in_layer(
            pointer_pos,
            state,
            layer,
            content_rect,
            view_start,
            view_end,
        )
    {
        return Some(hit);
    }
    state
        .project
        .data
        .layers
        .iter()
        .rev()
        .filter(|layer| Some(layer.id) != selected_layer_id && layer.visible)
        .find_map(|layer| {
            find_memo_hit_in_layer(
                pointer_pos,
                state,
                layer,
                content_rect,
                view_start,
                view_end,
            )
        })
}

fn find_memo_hit_in_layer(
    pointer_pos: egui::Pos2,
    state: &AppState,
    layer: &PitchMemoLayer,
    content_rect: egui::Rect,
    view_start: f64,
    view_end: f64,
) -> Option<MemoHit> {
    layer.memos.iter().rev().find_map(|memo| {
        let geometry = memo_geometry(
            memo.start_sec,
            memo.duration_sec,
            memo.pitch_midi,
            state,
            content_rect,
            view_start,
            view_end,
        )?;
        if !geometry.rect.expand(2.0).contains(pointer_pos) {
            return None;
        }
        let start_distance = geometry
            .start_handle_x
            .map(|x| (pointer_pos.x - x).abs())
            .unwrap_or(f32::INFINITY);
        let end_distance = geometry
            .end_handle_x
            .map(|x| (pointer_pos.x - x).abs())
            .unwrap_or(f32::INFINITY);
        let edge = if start_distance <= MEMO_HANDLE_HIT_RADIUS && start_distance <= end_distance {
            Some(MemoEdge::Start)
        } else if end_distance <= MEMO_HANDLE_HIT_RADIUS {
            Some(MemoEdge::End)
        } else {
            None
        };
        Some(MemoHit {
            selected: SelectedMemo {
                layer_id: layer.id,
                memo_id: memo.id,
            },
            edge,
        })
    })
}

fn resized_memo_bounds(
    edge: MemoEdge,
    original_start_sec: f64,
    original_duration_sec: f64,
    pointer_sec: f64,
    audio_duration: f64,
) -> (f64, f64) {
    let original_end_sec = original_start_sec + original_duration_sec;
    match edge {
        MemoEdge::Start => {
            let start_sec =
                pointer_sec.clamp(0.0, (original_end_sec - MIN_MEMO_DURATION_SECONDS).max(0.0));
            (start_sec, original_end_sec - start_sec)
        }
        MemoEdge::End => {
            let end_sec = pointer_sec.clamp(
                original_start_sec + MIN_MEMO_DURATION_SECONDS,
                audio_duration.max(original_start_sec + MIN_MEMO_DURATION_SECONDS),
            );
            (original_start_sec, end_sec - original_start_sec)
        }
    }
}

fn created_memo_bounds(pressed_sec: f64, released_sec: f64, audio_duration: f64) -> (f64, f64) {
    let audio_duration = audio_duration.max(0.0);
    let pressed_sec = pressed_sec.clamp(0.0, audio_duration);
    let released_sec = released_sec.clamp(0.0, audio_duration);
    let (mut start_sec, mut end_sec) = if pressed_sec <= released_sec {
        (pressed_sec, released_sec)
    } else {
        (released_sec, pressed_sec)
    };

    if end_sec - start_sec < MIN_MEMO_DURATION_SECONDS {
        if released_sec >= pressed_sec {
            end_sec = (start_sec + MIN_MEMO_DURATION_SECONDS).min(audio_duration);
            start_sec = (end_sec - MIN_MEMO_DURATION_SECONDS).max(0.0);
        } else {
            start_sec = (end_sec - MIN_MEMO_DURATION_SECONDS).max(0.0);
            end_sec = (start_sec + MIN_MEMO_DURATION_SECONDS).min(audio_duration);
        }
    }

    (start_sec, end_sec - start_sec)
}

fn moved_memo_position(
    duration_sec: f64,
    start_offset_sec: f64,
    pitch_offset_midi: i32,
    pointer_sec: f64,
    pointer_pitch_midi: i32,
    audio_duration: f64,
) -> (f64, i32) {
    let start_sec =
        (pointer_sec - start_offset_sec).clamp(0.0, (audio_duration - duration_sec).max(0.0));
    let pitch_midi = (pointer_pitch_midi - pitch_offset_midi).clamp(0, 127);
    (start_sec, pitch_midi)
}

fn x_to_time(x: f32, view_start: f64, view_end: f64, rect: egui::Rect) -> f64 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    view_start + (view_end - view_start) * t
}

fn time_to_x(seconds: f64, view_start: f64, view_duration: f64, rect: egui::Rect) -> f32 {
    rect.left() + ((seconds - view_start) / view_duration) as f32 * rect.width()
}

fn y_to_pitch(y: f32, state: &AppState, rect: egui::Rect) -> i32 {
    let pitch_view = state.pitch_view();
    let t = ((rect.bottom() - y) / rect.height()).clamp(0.0, 0.999_999);
    (pitch_view.min_midi_note + (t * pitch_view.pitch_count() as f32).floor() as usize) as i32
}

fn draw_pitch_scrollbar(painter: &egui::Painter, rect: egui::Rect, state: &AppState) {
    painter.rect_filled(
        rect,
        999.0,
        Color32::from_rgba_premultiplied(255, 255, 255, 24),
    );
    painter.rect_stroke(
        rect,
        999.0,
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 48)),
        egui::StrokeKind::Inside,
    );

    let thumb = pitch_scrollbar_thumb_rect(rect, state);
    painter.rect_filled(thumb, 4.0, Color32::from_rgb(90, 168, 204));
}

fn pitch_scrollbar_thumb_rect(rect: egui::Rect, state: &AppState) -> egui::Rect {
    let full_view = state.full_pitch_view();
    let visible_view = state.pitch_view();
    let total = full_view.pitch_count().max(1) as f32;
    let visible = visible_view.pitch_count() as f32;
    let top_ratio = (full_view
        .max_midi_note
        .saturating_sub(visible_view.max_midi_note) as f32
        / total)
        .clamp(0.0, 1.0);
    let height_ratio = (visible / total).clamp(0.0, 1.0);
    let thumb_top = egui::lerp(rect.top()..=rect.bottom(), top_ratio);
    let thumb_bottom = egui::lerp(
        rect.top()..=rect.bottom(),
        (top_ratio + height_ratio).min(1.0),
    );
    egui::Rect::from_min_max(
        egui::pos2(rect.left(), thumb_top),
        egui::pos2(rect.right(), thumb_bottom.max(thumb_top + 2.0)),
    )
}

fn time_view_start_for_thumb_position(
    duration: f64,
    view_duration: f64,
    pointer_t: f64,
    thumb_offset_ratio: f64,
) -> f64 {
    let duration = duration.max(0.001);
    let thumb_ratio = (view_duration / duration).clamp(0.0, 1.0);
    let max_left_ratio = (1.0 - thumb_ratio).max(0.0);
    let left_ratio = (pointer_t.clamp(0.0, 1.0) - thumb_ratio * thumb_offset_ratio.clamp(0.0, 1.0))
        .clamp(0.0, max_left_ratio);
    left_ratio * duration
}

fn time_pan_delta_for_scroll(scroll_delta: f32, view_duration: f64) -> f64 {
    if scroll_delta.abs() <= f32::EPSILON {
        0.0
    } else {
        -scroll_delta.signum() as f64 * view_duration * TIME_PAN_VIEW_FRACTION
    }
}

fn scroll_delta_for_modifiers(raw_scroll_delta: egui::Vec2, modifiers: egui::Modifiers) -> f32 {
    // Shiftを押したホイールは、OSやウィンドウシステムによっては
    // 縦方向ではなく横方向のスクロールとして届く。
    if !modifiers.shift || raw_scroll_delta.y.abs() > f32::EPSILON {
        raw_scroll_delta.y
    } else {
        raw_scroll_delta.x
    }
}

fn pitch_scroll_center_for_thumb_position(
    full_view: crate::app::state::PitchView,
    visible_view: crate::app::state::PitchView,
    pointer_t: f64,
    thumb_offset_ratio: f64,
) -> f64 {
    let total = full_view.pitch_count() as f64;
    let visible = visible_view.pitch_count() as f64;
    let height_ratio = (visible / total).clamp(0.0, 1.0);
    let top_ratio = (pointer_t.clamp(0.0, 1.0) - height_ratio * thumb_offset_ratio.clamp(0.0, 1.0))
        .clamp(0.0, (1.0 - height_ratio).max(0.0));
    let maximum_start = total - visible;
    let minimum_start = full_view.min_midi_note as f64;
    let start = minimum_start + (1.0 - height_ratio - top_ratio) * total;
    debug_assert!((minimum_start..=minimum_start + maximum_start).contains(&start));
    start + visible / 2.0
}

pub(crate) fn layer_color(layer_id: u64) -> Color32 {
    const PALETTE: [Color32; 6] = [
        Color32::from_rgb(72, 202, 228),
        Color32::from_rgb(255, 159, 67),
        Color32::from_rgb(78, 205, 126),
        Color32::from_rgb(255, 105, 180),
        Color32::from_rgb(255, 209, 102),
        Color32::from_rgb(162, 125, 255),
    ];
    layer_id
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| PALETTE.get(index).copied())
        .unwrap_or(Color32::WHITE)
}

fn draw_spectrogram_body(
    painter: &egui::Painter,
    content_rect: egui::Rect,
    state: &AppState,
    view_start: f64,
    view_end: f64,
    cache: &mut SpectrogramCache,
) {
    let Some(track) = &state.track else {
        draw_placeholder_grid(painter, content_rect);
        return;
    };
    let Some(spectrogram) = &track.spectrogram else {
        draw_placeholder_grid(painter, content_rect);
        return;
    };
    if spectrogram.frames == 0 || spectrogram.pitches == 0 {
        draw_placeholder_grid(painter, content_rect);
        return;
    }

    let start_frame = ((view_start / spectrogram.frame_duration_seconds).floor() as usize)
        .min(spectrogram.frames.saturating_sub(1));
    let end_frame = ((view_end / spectrogram.frame_duration_seconds).ceil() as usize)
        .clamp(start_frame + 1, spectrogram.frames);
    let visible_frames = end_frame.saturating_sub(start_frame).max(1);

    let pitch_view = state.pitch_view();
    let first_pitch = pitch_view
        .min_midi_note
        .saturating_sub(spectrogram.min_midi_note)
        .min(spectrogram.pitches.saturating_sub(1));
    let last_pitch_exclusive = pitch_view
        .max_midi_note
        .saturating_add(1)
        .saturating_sub(spectrogram.min_midi_note)
        .min(spectrogram.pitches);
    let pixels_per_point = painter.ctx().pixels_per_point();
    let width_pixels = (content_rect.width() * pixels_per_point).round().max(1.0) as usize;
    let height_pixels = (content_rect.height() * pixels_per_point).round().max(1.0) as usize;
    let key = SpectrogramCacheKey {
        data_address: spectrogram.intensities.as_ptr() as usize,
        view_start_bits: view_start.to_bits(),
        view_end_bits: view_end.to_bits(),
        first_pitch,
        last_pitch_exclusive,
        width_pixels,
        height_pixels,
        gain_bits: state.spectrogram_gain_db.to_bits(),
        emphasis_bits: state
            .project
            .data
            .project_settings
            .fundamental_analysis
            .emphasis
            .to_bits(),
        emphasized_pitch_classes: state
            .project
            .data
            .project_settings
            .fundamental_analysis
            .emphasized_pitch_classes,
        attenuation_bits: state
            .project
            .data
            .project_settings
            .fundamental_analysis
            .unemphasized_pitch_attenuation
            .to_bits(),
        equalizer_gain_bits: state
            .project
            .data
            .project_settings
            .equalizer_gains_db
            .map(f32::to_bits),
    };
    if cache.key.as_ref() != Some(&key) {
        let image = render_spectrogram_image(
            spectrogram,
            state,
            start_frame,
            visible_frames,
            first_pitch,
            last_pitch_exclusive,
            width_pixels,
            height_pixels,
        );
        cache.texture = Some(painter.ctx().load_texture(
            "spectrogram-body",
            image,
            egui::TextureOptions::NEAREST,
        ));
        cache.key = Some(key);
    }
    if let Some(texture) = &cache.texture {
        painter.image(
            texture.id(),
            content_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    draw_pitch_guides(painter, content_rect, pitch_view);
    draw_loop_markers(painter, content_rect, state, view_start, view_end);
}

fn draw_placeholder_grid(painter: &egui::Painter, rect: egui::Rect) {
    let bands = 24;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let y = egui::lerp(rect.bottom()..=rect.top(), t);
        let color = if i % 3 == 0 {
            Color32::from_rgba_premultiplied(114, 173, 196, 36)
        } else {
            Color32::from_rgba_premultiplied(255, 255, 255, 14)
        };
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, color),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_spectrogram_image(
    spectrogram: &crate::analysis::spectrum::SpectrogramData,
    state: &AppState,
    start_frame: usize,
    visible_frames: usize,
    first_pitch: usize,
    last_pitch_exclusive: usize,
    width_pixels: usize,
    height_pixels: usize,
) -> egui::ColorImage {
    let background = Color32::from_rgb(15, 24, 35);
    let mut image = egui::ColorImage::new(
        [width_pixels, height_pixels],
        vec![background; width_pixels * height_pixels],
    );
    let visible_pitches = last_pitch_exclusive.saturating_sub(first_pitch).max(1);
    let columns = drawing_column_count(visible_frames, width_pixels as f32, 1.0);
    let display_gain = 10.0_f32.powf(state.spectrogram_gain_db / 20.0);
    let project_settings = &state.project.data.project_settings;
    let analysis = &project_settings.fundamental_analysis;

    for pitch in first_pitch..last_pitch_exclusive {
        let midi_note = spectrogram.min_midi_note + pitch;
        let is_emphasized_pitch = analysis.emphasized_pitch_classes[midi_note % 12];
        let equalizer_gain = EqualizerSettings {
            gains_db: project_settings.equalizer_gains_db,
        }
        .gain_for_frequency_hz(midi_to_frequency_hz(midi_note));
        let y_start = height_pixels - (pitch - first_pitch + 1) * height_pixels / visible_pitches;
        let y_end = height_pixels - (pitch - first_pitch) * height_pixels / visible_pitches;

        for column in 0..columns {
            let frames = column_frame_range(column, columns, visible_frames);
            // Final display strength is aggregated per column so a peak in a
            // short analysis frame remains visible when zoomed out.
            let intensity = peak_display_strength(frames, |local_frame| {
                let frame_index = start_frame + local_frame;
                let raw = (spectrogram.intensity_at(frame_index, pitch) * display_gain)
                    .clamp(0.0, 1.0)
                    * equalizer_gain;
                let fundamental = if analysis.emphasis > 0.0 {
                    (spectrogram.fundamental_strength_at(frame_index, pitch) * display_gain)
                        .clamp(0.0, 1.0)
                        * equalizer_gain
                } else {
                    0.0
                };
                apply_unemphasized_pitch_attenuation(
                    apply_fundamental_emphasis(
                        raw,
                        fundamental,
                        is_emphasized_pitch,
                        analysis.emphasis,
                    ),
                    is_emphasized_pitch,
                    analysis.unemphasized_pitch_attenuation,
                )
            });
            if intensity <= 0.01 {
                continue;
            }

            let x_start = column * width_pixels / columns;
            let x_end = (column + 1) * width_pixels / columns;
            let color = composite_spectrogram_color(background, intensity);
            for y in y_start..y_end {
                let row = y * width_pixels;
                for x in x_start..x_end {
                    image.pixels[row + x] = color;
                }
            }
        }
    }

    image
}

fn draw_pitch_guides(
    painter: &egui::Painter,
    rect: egui::Rect,
    pitch_view: crate::app::state::PitchView,
) {
    let pitches = pitch_view.pitch_count();
    for index in 0..=pitches {
        if !(pitch_view.min_midi_note + index).is_multiple_of(12) {
            continue;
        }
        let y = egui::lerp(
            rect.bottom()..=rect.top(),
            index as f32 / pitches.max(1) as f32,
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 22)),
        );
    }
}

fn composite_spectrogram_color(background: Color32, intensity: f32) -> Color32 {
    let intensity = intensity.clamp(0.0, 1.0);
    let (r, g, b) = thermal_gradient(intensity);
    let alpha = egui::lerp(20.0..=255.0, intensity) / 255.0;
    Color32::from_rgb(
        egui::lerp(background.r() as f32..=r as f32, alpha) as u8,
        egui::lerp(background.g() as f32..=g as f32, alpha) as u8,
        egui::lerp(background.b() as f32..=b as f32, alpha) as u8,
    )
}

fn draw_loop_markers(
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &AppState,
    page_start: f64,
    page_end: f64,
) {
    let Some((loop_start, loop_end)) = state.selection.normalized() else {
        return;
    };
    let page_duration = (page_end - page_start).max(0.001);

    for seconds in [loop_start, loop_end] {
        if seconds < page_start || seconds > page_end {
            continue;
        }
        let normalized = ((seconds - page_start) / page_duration).clamp(0.0, 1.0) as f32;
        let x = egui::lerp(rect.left()..=rect.right(), normalized);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(2.0, Color32::from_rgb(255, 105, 180)),
        );
    }
}

fn draw_subpixel_playhead(
    painter: &egui::Painter,
    pixels_per_point: f32,
    top: f32,
    bottom: f32,
    x: f32,
) {
    let pixel_width = 1.0 / pixels_per_point.max(1.0);
    for (column_x, alpha) in subpixel_columns(x, pixels_per_point) {
        if alpha <= f32::EPSILON {
            continue;
        }

        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(column_x, top),
                egui::pos2(column_x + pixel_width, bottom),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(255, 209, 102, (255.0 * alpha) as u8),
        );
    }
}

fn subpixel_columns(x: f32, pixels_per_point: f32) -> [(f32, f32); 2] {
    let pixels_per_point = pixels_per_point.max(1.0);
    let physical_x = x * pixels_per_point;
    let physical_left = physical_x.floor();
    let right_alpha = physical_x - physical_left;
    [
        (physical_left / pixels_per_point, 1.0 - right_alpha),
        ((physical_left + 1.0) / pixels_per_point, right_alpha),
    ]
}

fn drawing_column_count(frames: usize, width_points: f32, pixels_per_point: f32) -> usize {
    let physical_columns = (width_points * pixels_per_point).floor().max(1.0) as usize;
    frames.min(physical_columns)
}

fn column_frame_range(column: usize, columns: usize, frames: usize) -> std::ops::Range<usize> {
    column * frames / columns..(column + 1) * frames / columns
}

fn peak_display_strength(
    frames: std::ops::Range<usize>,
    strength_at: impl FnMut(usize) -> f32,
) -> f32 {
    frames.map(strength_at).fold(0.0, f32::max)
}

fn apply_fundamental_emphasis(
    intensity: f32,
    fundamental_strength: f32,
    is_emphasized_pitch: bool,
    emphasis: f32,
) -> f32 {
    let emphasis = (emphasis / 100.0).clamp(0.0, 1.0);
    if emphasis <= f32::EPSILON {
        return intensity;
    }

    let focused_strength = if is_emphasized_pitch {
        fundamental_strength
    } else {
        fundamental_strength * 0.35
    };
    intensity + (focused_strength - intensity) * emphasis
}

fn apply_unemphasized_pitch_attenuation(
    intensity: f32,
    is_emphasized_pitch: bool,
    attenuation: f32,
) -> f32 {
    if is_emphasized_pitch {
        return intensity;
    }

    intensity * (1.0 - (attenuation / 100.0).clamp(0.0, 1.0))
}

fn midi_to_frequency_hz(midi_note: usize) -> f32 {
    440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
}

fn thermal_gradient(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);

    if t < 0.2 {
        lerp_rgb((8, 6, 26), (42, 20, 90), t / 0.2)
    } else if t < 0.4 {
        lerp_rgb((42, 20, 90), (24, 110, 182), (t - 0.2) / 0.2)
    } else if t < 0.6 {
        lerp_rgb((24, 110, 182), (0, 188, 156), (t - 0.4) / 0.2)
    } else if t < 0.8 {
        lerp_rgb((0, 188, 156), (255, 196, 0), (t - 0.6) / 0.2)
    } else {
        lerp_rgb((255, 196, 0), (255, 72, 32), (t - 0.8) / 0.2)
    }
}

fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        egui::lerp(from.0 as f32..=to.0 as f32, t) as u8,
        egui::lerp(from.1 as f32..=to.1 as f32, t) as u8,
        egui::lerp(from.2 as f32..=to.2 as f32, t) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        MemoEdge, apply_fundamental_emphasis, apply_unemphasized_pitch_attenuation,
        column_frame_range, created_memo_bounds, drawing_column_count, find_memo_hit, layer_color,
        memo_geometry, memo_stroke_color, moved_memo_position, peak_display_strength,
        pitch_scroll_center_for_thumb_position, resized_memo_bounds, scroll_delta_for_modifiers,
        subpixel_columns, time_pan_delta_for_scroll, time_view_start_for_thumb_position,
    };
    use crate::app::state::{AppState, PitchView};
    use crate::model::Track;
    use eframe::egui;

    #[test]
    fn downsampling_covers_every_frame_once_and_preserves_brief_peaks() {
        let strengths = [0.0, 0.9, 0.0, 0.2, 0.1, 0.0, 1.0];
        let columns = drawing_column_count(strengths.len(), 3.0, 1.0);
        let mut visited = Vec::new();
        let peaks: Vec<_> = (0..columns)
            .map(|column| {
                peak_display_strength(column_frame_range(column, columns, strengths.len()), |i| {
                    visited.push(i);
                    strengths[i]
                })
            })
            .collect();
        assert_eq!(visited, (0..strengths.len()).collect::<Vec<_>>());
        assert_eq!(peaks, vec![0.9, 0.2, 1.0]);
    }

    #[test]
    fn drawing_resolution_respects_dpi_and_keeps_zoomed_frames_separate() {
        assert_eq!(drawing_column_count(10000, 100.0, 2.0), 200);
        assert_eq!(drawing_column_count(10000, 100.0, 1.0), 100);
        assert_eq!(drawing_column_count(5, 100.0, 2.0), 5);
        assert_eq!(drawing_column_count(5, 0.5, 1.0), 1);
        for frame in 0..5 {
            assert_eq!(column_frame_range(frame, 5, 5), frame..frame + 1);
        }
    }

    #[test]
    fn fundamental_emphasis_is_raw_at_zero_percent() {
        assert_eq!(apply_fundamental_emphasis(0.3, 0.8, true, 0.0), 0.3);
    }

    #[test]
    fn fundamental_emphasis_uses_the_selected_pitch_strength() {
        assert_eq!(apply_fundamental_emphasis(0.3, 0.8, true, 100.0), 0.8);
        assert!((apply_fundamental_emphasis(0.3, 0.8, false, 100.0) - 0.28).abs() < f32::EPSILON);
    }

    #[test]
    fn attenuation_only_dims_unselected_notes() {
        assert_eq!(apply_unemphasized_pitch_attenuation(0.8, true, 75.0), 0.8);
        assert_eq!(apply_unemphasized_pitch_attenuation(0.8, false, 0.0), 0.8);
        assert_eq!(apply_unemphasized_pitch_attenuation(0.8, false, 100.0), 0.0);
        assert!(
            (apply_unemphasized_pitch_attenuation(0.8, false, 75.0) - 0.2).abs() < f32::EPSILON
        );
    }

    #[test]
    fn created_memo_uses_the_dragged_time_range_in_both_directions() {
        assert_eq!(created_memo_bounds(2.0, 5.0, 10.0), (2.0, 3.0));
        assert_eq!(created_memo_bounds(5.0, 2.0, 10.0), (2.0, 3.0));
    }

    #[test]
    fn created_memo_keeps_the_minimum_duration_inside_audio_bounds() {
        let at_end = created_memo_bounds(9.999, 10.0, 10.0);
        assert!((at_end.0 - 9.99).abs() < f64::EPSILON);
        assert!((at_end.1 - 0.01).abs() < f64::EPSILON);

        let at_start = created_memo_bounds(0.001, 0.0, 10.0);
        assert_eq!(at_start, (0.0, 0.01));
        assert_eq!(created_memo_bounds(0.0, 0.0, 0.005), (0.0, 0.005));
    }

    #[test]
    fn time_scrollbar_click_centers_the_visible_range() {
        assert_eq!(
            time_view_start_for_thumb_position(100.0, 20.0, 0.5, 0.5),
            40.0
        );
        assert_eq!(
            time_view_start_for_thumb_position(100.0, 20.0, 0.0, 0.5),
            0.0
        );
        assert_eq!(
            time_view_start_for_thumb_position(100.0, 20.0, 1.0, 0.5),
            80.0
        );
    }

    #[test]
    fn scrollbar_drag_preserves_the_grabbed_thumb_offset() {
        assert_eq!(
            time_view_start_for_thumb_position(100.0, 20.0, 0.5, 0.25),
            45.0
        );

        let full = PitchView {
            min_midi_note: 24,
            max_midi_note: 108,
        };
        let visible = PitchView {
            min_midi_note: 44,
            max_midi_note: 64,
        };
        let centered = pitch_scroll_center_for_thumb_position(full, visible, 0.5, 0.5);
        let grabbed_near_top = pitch_scroll_center_for_thumb_position(full, visible, 0.5, 0.25);
        assert!(grabbed_near_top < centered);
    }

    #[test]
    fn shift_scroll_pans_by_a_fraction_of_the_visible_duration() {
        assert_eq!(time_pan_delta_for_scroll(1.0, 20.0), -3.0);
        assert_eq!(time_pan_delta_for_scroll(-1.0, 20.0), 3.0);
        assert_eq!(time_pan_delta_for_scroll(0.0, 20.0), 0.0);
    }

    #[test]
    fn shift_scroll_uses_horizontal_delta_when_the_platform_maps_it_there() {
        assert_eq!(
            scroll_delta_for_modifiers(egui::vec2(0.0, 12.0), egui::Modifiers::SHIFT),
            12.0
        );
        assert_eq!(
            scroll_delta_for_modifiers(egui::vec2(-12.0, 0.0), egui::Modifiers::SHIFT),
            -12.0
        );
        assert_eq!(
            scroll_delta_for_modifiers(egui::Vec2::ZERO, egui::Modifiers::SHIFT),
            0.0
        );
    }

    #[test]
    fn subpixel_playhead_distributes_brightness_between_adjacent_columns() {
        let columns = subpixel_columns(100.25, 1.0);

        assert_eq!(columns[0], (100.0, 0.75));
        assert_eq!(columns[1], (101.0, 0.25));
    }

    #[test]
    fn subpixel_playhead_uses_physical_pixel_columns_on_high_dpi_displays() {
        let columns = subpixel_columns(100.25, 2.0);

        assert_eq!(columns[0], (100.0, 0.5));
        assert_eq!(columns[1], (100.5, 0.5));
    }

    #[test]
    fn memo_resize_preserves_the_opposite_edge_and_minimum_duration() {
        assert_eq!(
            resized_memo_bounds(MemoEdge::Start, 2.0, 1.0, 2.75, 10.0),
            (2.75, 0.25)
        );
        let (start, duration) = resized_memo_bounds(MemoEdge::End, 2.0, 1.0, 1.0, 10.0);
        assert_eq!(start, 2.0);
        assert!((duration - 0.01).abs() < f64::EPSILON);
        assert_eq!(
            resized_memo_bounds(MemoEdge::End, 2.0, 1.0, 20.0, 10.0),
            (2.0, 8.0)
        );
    }

    #[test]
    fn moving_a_memo_preserves_grab_offset_and_stays_in_bounds() {
        assert_eq!(moved_memo_position(0.5, 0.2, 3, 4.0, 70, 10.0), (3.8, 67));
        assert_eq!(moved_memo_position(0.5, 0.2, 3, -1.0, -10, 10.0), (0.0, 0));
        assert_eq!(
            moved_memo_position(0.5, 0.2, 3, 20.0, 140, 10.0),
            (9.5, 127)
        );
    }

    #[test]
    fn selected_layer_wins_when_memos_overlap() {
        let mut state = AppState {
            track: Some(Track {
                duration_seconds: 10.0,
                ..Track::default()
            }),
            ..AppState::default()
        };
        let selected_layer = state.project.editing.selected_layer_id.unwrap();
        state.project.add_memo(selected_layer, 1.0, 1.0, 60);
        let front_layer = state.project.data.layers[1].id;
        state.project.add_memo(front_layer, 1.0, 1.0, 60);
        state.project.editing.selected_layer_id = Some(selected_layer);
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 100.0));
        let geometry = memo_geometry(1.0, 1.0, 60, &state, rect, 0.0, 10.0).unwrap();

        let hit = find_memo_hit(geometry.rect.center(), &state, rect, 0.0, 10.0).unwrap();

        assert_eq!(hit.selected.layer_id, selected_layer);
    }

    #[test]
    fn layer_palette_uses_white_after_the_sixth_id() {
        assert_ne!(layer_color(1), egui::Color32::WHITE);
        assert_eq!(layer_color(7), egui::Color32::WHITE);
    }

    #[test]
    fn memo_outline_uses_the_layer_opacity() {
        let base = egui::Color32::from_rgb(10, 20, 30);

        assert_eq!(memo_stroke_color(base, false, 96).a(), 96);
        assert_eq!(memo_stroke_color(base, true, 96).a(), 96);
    }
}
