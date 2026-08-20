use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::app::state::AppState;

const LOOP_HANDLE_HIT_RADIUS: f32 = 10.0;
const LOOP_TIMELINE_DRAG_ID: &str = "loop-timeline-drag";
const LOOP_HANDLE_SIZE: Vec2 = Vec2::new(12.0, 14.0);

#[derive(Debug, Default, Clone, Copy)]
pub struct TimelineActions {
    pub selection_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimelineDragMode {
    Create,
    AdjustStart,
    AdjustEnd,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, left_offset: f32) -> TimelineActions {
    let mut actions = TimelineActions::default();
    let desired_size = Vec2::new(ui.available_width(), 34.0);
    let (full_rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(full_rect);
    let view_start = state.current_view_start_seconds();
    let view_end = state.current_view_end_seconds();
    let view_duration = (view_end - view_start).max(0.001);

    let timeline_rect = egui::Rect::from_min_max(
        egui::pos2(full_rect.left() + left_offset, full_rect.top() + 8.0),
        egui::pos2(full_rect.right() - 8.0, full_rect.bottom() - 8.0),
    );
    let response = ui.interact(
        timeline_rect,
        ui.id().with("loop-range-timeline"),
        Sense::click_and_drag(),
    );

    painter.rect_filled(full_rect, 0.0, Color32::TRANSPARENT);
    painter.rect_filled(
        timeline_rect,
        999.0,
        Color32::from_rgba_premultiplied(255, 255, 255, 18),
    );
    painter.rect_stroke(
        timeline_rect,
        999.0,
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 42)),
        egui::StrokeKind::Inside,
    );

    let drag_id = ui.id().with(LOOP_TIMELINE_DRAG_ID);
    let mut drag_mode = ui
        .ctx()
        .data(|data| data.get_temp::<TimelineDragMode>(drag_id));

    if let Some((start, end)) = state.selection.normalized() {
        if start >= view_start && end <= view_end {
            let start_x = time_to_x(start, view_start, view_duration, timeline_rect);
            let end_x = time_to_x(end, view_start, view_duration, timeline_rect);
            let selection_rect = egui::Rect::from_min_max(
                egui::pos2(start_x, timeline_rect.top()),
                egui::pos2(end_x, timeline_rect.bottom()),
            );
            painter.rect_filled(
                selection_rect,
                999.0,
                Color32::from_rgba_premultiplied(80, 180, 255, 84),
            );
            draw_loop_handle(&painter, start_x, timeline_rect.center().y, HandleDirection::Start);
            draw_loop_handle(&painter, end_x, timeline_rect.center().y, HandleDirection::End);
        }
    }

    if response.drag_started() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            drag_mode = Some(pick_drag_mode(pointer_pos.x, state, timeline_rect, view_start, view_duration));
            ui.ctx().data_mut(|data| {
                if let Some(mode) = drag_mode {
                    data.insert_temp(drag_id, mode);
                }
            });
            let seconds = x_to_time(pointer_pos.x, view_start, view_duration, timeline_rect);
            match drag_mode.unwrap_or(TimelineDragMode::Create) {
                TimelineDragMode::Create => state.selection.set_range(seconds, seconds),
                TimelineDragMode::AdjustStart => state.selection.start_seconds = Some(seconds),
                TimelineDragMode::AdjustEnd => state.selection.end_seconds = Some(seconds),
            }
            actions.selection_changed = true;
        }
    }

    if response.dragged() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let seconds = x_to_time(pointer_pos.x, view_start, view_duration, timeline_rect);
            match drag_mode.unwrap_or(TimelineDragMode::Create) {
                TimelineDragMode::Create => {
                    let start = state.selection.start_seconds.unwrap_or(seconds);
                    state.selection.set_range(start, seconds);
                }
                TimelineDragMode::AdjustStart => {
                    state.selection.start_seconds = Some(seconds);
                }
                TimelineDragMode::AdjustEnd => {
                    state.selection.end_seconds = Some(seconds);
                }
            }
            actions.selection_changed = true;
        }
    }

    if response.drag_stopped() {
        ui.ctx().data_mut(|data| data.remove::<TimelineDragMode>(drag_id));
        if state.selection.normalized().is_none() {
            state.selection.clear();
        }
        actions.selection_changed = true;
    }

    painter.text(
        timeline_rect.left_center() + egui::vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        "Loop Range",
        FontId::proportional(12.0),
        Color32::from_rgb(210, 224, 235),
    );

    painter.text(
        timeline_rect.right_center() + egui::vec2(-6.0, 0.0),
        Align2::RIGHT_CENTER,
        format!("{:.0} - {:.0}s", view_start, view_end),
        FontId::proportional(12.0),
        Color32::from_rgb(176, 192, 205),
    );

    actions
}

fn time_to_x(seconds: f64, page_start: f64, page_duration: f64, rect: egui::Rect) -> f32 {
    let t = ((seconds - page_start) / page_duration).clamp(0.0, 1.0) as f32;
    egui::lerp(rect.left()..=rect.right(), t)
}

fn x_to_time(x: f32, page_start: f64, page_duration: f64, rect: egui::Rect) -> f64 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    page_start + page_duration * t
}

fn pick_drag_mode(
    pointer_x: f32,
    state: &AppState,
    rect: egui::Rect,
    page_start: f64,
    page_duration: f64,
) -> TimelineDragMode {
    let Some((start, end)) = state.selection.normalized() else {
        return TimelineDragMode::Create;
    };
    if start < page_start || end > page_start + page_duration {
        return TimelineDragMode::Create;
    }

    let start_x = time_to_x(start, page_start, page_duration, rect);
    let end_x = time_to_x(end, page_start, page_duration, rect);
    let start_distance = (pointer_x - start_x).abs();
    let end_distance = (pointer_x - end_x).abs();

    if start_distance <= LOOP_HANDLE_HIT_RADIUS && start_distance <= end_distance {
        TimelineDragMode::AdjustStart
    } else if end_distance <= LOOP_HANDLE_HIT_RADIUS {
        TimelineDragMode::AdjustEnd
    } else {
        TimelineDragMode::Create
    }
}

#[derive(Debug, Clone, Copy)]
enum HandleDirection {
    Start,
    End,
}

fn draw_loop_handle(painter: &egui::Painter, x: f32, center_y: f32, direction: HandleDirection) {
    let half_w = LOOP_HANDLE_SIZE.x * 0.5;
    let half_h = LOOP_HANDLE_SIZE.y * 0.5;
    let points = match direction {
        HandleDirection::Start => vec![
            egui::pos2(x - half_w, center_y - half_h),
            egui::pos2(x + half_w, center_y),
            egui::pos2(x - half_w, center_y + half_h),
        ],
        HandleDirection::End => vec![
            egui::pos2(x + half_w, center_y - half_h),
            egui::pos2(x - half_w, center_y),
            egui::pos2(x + half_w, center_y + half_h),
        ],
    };

    painter.add(egui::Shape::convex_polygon(
        points,
        Color32::from_rgb(255, 105, 180),
        Stroke::new(1.0, Color32::from_rgb(255, 182, 220)),
    ));
}
