use egui::{Color32, Id, Stroke, pos2};
use game_interface::types::{
    emoticons::{EmoticonType, EnumCount},
    render::character::{IntoEnumIterator, TeeEye},
};
use math::math::{
    PI, length, normalize_pre_length,
    vector::{dvec2, vec2},
};
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use client_ui_utils::{render_emoticon_for_ui, render_tee_for_ui, rotate};

use super::user_data::{EmoteWheelEvent, UserData};

fn normalized_angle_diff(angle: f32, center: f32) -> f32 {
    let diff = angle - center;
    diff.sin().atan2(diff.cos())
}

fn is_mouse_in_wheel_segment(
    mouse: dvec2,
    center: vec2,
    segment_center: vec2,
    inner_radius: f32,
    outer_radius: f32,
    entry_count: usize,
) -> bool {
    let mouse_dir = mouse - dvec2::new(center.x as f64, center.y as f64);
    let mouse_len = length(&mouse_dir) as f32;

    if mouse_len < inner_radius || mouse_len > outer_radius {
        return false;
    }

    let mouse_angle = (mouse_dir.y as f32).atan2(mouse_dir.x as f32);
    let segment_angle = segment_center.y.atan2(segment_center.x);
    let segment_half_angle = PI / entry_count as f32;

    normalized_angle_diff(mouse_angle, segment_angle).abs() <= segment_half_angle
}

fn paint_wheel_segment(
    ui: &egui::Ui,
    center: vec2,
    segment_center: vec2,
    inner_radius: f32,
    outer_radius: f32,
    entry_count: usize,
    color: Color32,
) {
    let segment_angle = segment_center.y.atan2(segment_center.x);
    let half_angle = PI / entry_count as f32;
    let steps = 16;

    for i in 0..steps {
        let angle0 = segment_angle - half_angle + 2.0 * half_angle * i as f32 / steps as f32;
        let angle1 = segment_angle - half_angle + 2.0 * half_angle * (i + 1) as f32 / steps as f32;

        let point_at = |angle: f32, radius: f32| {
            pos2(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        };

        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                point_at(angle0, inner_radius),
                point_at(angle0, outer_radius),
                point_at(angle1, outer_radius),
                point_at(angle1, inner_radius),
            ],
            color,
            Stroke::NONE,
        ));
    }
}

/// not required
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let rect = ui.ctx().content_rect();

    let width_scale = rect.width() / pipe.user_data.canvas_handle.canvas_width() as f32;

    let radius = |percentage: f32| {
        (percentage / 100.0 * pipe.user_data.canvas_handle.canvas_height() as f32) * width_scale
    };

    let color = Color32::from_black_alpha(100);
    let hover_color = Color32::from_rgba_unmultiplied(180, 205, 255, 15);

    let inner_stroke_size = radius(15.0);
    let inner_start = radius(5.0);

    let outer_radius = radius(35.0);
    let outer_stroke_size = outer_radius - (inner_stroke_size + inner_start);
    let outer_start = inner_start + inner_stroke_size;
    let outer_center = outer_start + outer_stroke_size / 2.0;
    let outer_end = outer_start + outer_stroke_size;

    ui.painter()
        .circle_filled(rect.center(), outer_radius, color);

    let inner_center = inner_stroke_size / 2.0 + inner_start;
    let inner_end = inner_stroke_size + inner_start;

    ui.painter().circle_stroke(
        rect.center(),
        radius(5.0),
        Stroke::new(inner_stroke_size, color),
    );

    ui_state.add_blur_circle(rect.center(), outer_radius);

    let mouse = &mut *pipe.user_data.mouse;

    // render emoticons in a radius around the outer circle
    let mut pos = vec2::new(0.0, outer_center);
    let center = rect.center();
    let center = vec2::new(center.x, center.y);

    let mouse_dir = dvec2::new(mouse.x, mouse.y) - dvec2::new(center.x as f64, center.y as f64);
    let mouse_len = length(&mouse_dir);
    if mouse_len > outer_radius as f64 {
        let center = dvec2::new(center.x as f64, center.y as f64);
        let mouse_dir = normalize_pre_length(&mouse_dir, mouse_len);
        mouse.x = center.x + mouse_dir.x * outer_radius as f64;
        mouse.y = center.y + mouse_dir.y * outer_radius as f64;
    }

    // rotate a bit so oop emote is on the very right
    let start_rot = |pos: &mut vec2| {
        rotate(
            &vec2::default(),
            -2.0 * 5.0 / EmoticonType::COUNT as f32 * PI,
            std::slice::from_mut(pos),
        )
    };
    start_rot(&mut pos);
    for emote in EmoticonType::iter() {
        let rot = |pos: &mut vec2, scale: f32| {
            rotate(
                &vec2::default(),
                scale * 2.0 / EmoticonType::COUNT as f32 * PI,
                std::slice::from_mut(pos),
            )
        };

        rot(&mut pos, 1.0);

        let center = center + pos;
        let size = radius(10.0);
        let selected = is_mouse_in_wheel_segment(
            dvec2::new(mouse.x, mouse.y),
            center - pos,
            pos,
            outer_start,
            outer_end,
            EmoticonType::COUNT,
        );
        if selected {
            paint_wheel_segment(
                ui,
                center - pos,
                pos,
                outer_start,
                outer_end,
                EmoticonType::COUNT,
                hover_color,
            );
            pipe.user_data
                .events
                .push(EmoteWheelEvent::EmoticonSelected(emote));
        }
        let val = if selected {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-emoticons-{}", emote as usize)),
                1.5,
                0.15,
            )
        } else {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-emoticons-{}", emote as usize)),
                1.0,
                0.15,
            )
        };
        render_emoticon_for_ui(
            pipe.user_data.stream_handle,
            pipe.user_data.canvas_handle,
            pipe.user_data.emoticons_container,
            ui,
            ui_state,
            rect,
            None,
            pipe.user_data.emoticon,
            center,
            size * val,
            emote,
        );
    }

    // render tees in a radius around the inner circle
    let mut pos = vec2::new(0.0, inner_center);
    let center = rect.center();
    let center = vec2::new(center.x, center.y);

    // rotate a bit so normal eyes are on the very right
    let start_rot = |pos: &mut vec2| {
        rotate(
            &vec2::default(),
            -3.0 / TeeEye::COUNT as f32 * PI,
            std::slice::from_mut(pos),
        )
    };
    start_rot(&mut pos);
    for eye in TeeEye::iter().rev() {
        let rot = |pos: &mut vec2, scale: f32| {
            rotate(
                &vec2::default(),
                -scale * 2.0 / TeeEye::COUNT as f32 * PI,
                std::slice::from_mut(pos),
            )
        };
        rot(&mut pos, 1.0);

        let center = center + pos;
        let size = radius(10.0);
        let selected = is_mouse_in_wheel_segment(
            dvec2::new(mouse.x, mouse.y),
            center - pos,
            pos,
            inner_start,
            inner_end,
            TeeEye::COUNT,
        );
        if selected {
            paint_wheel_segment(
                ui,
                center - pos,
                pos,
                inner_start,
                inner_end,
                TeeEye::COUNT,
                hover_color,
            );
            pipe.user_data
                .events
                .push(EmoteWheelEvent::EyeSelected(eye));
        }
        let val = if selected {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-eyes-{}", eye as usize)),
                1.5,
                0.15,
            )
        } else {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-eyes-{}", eye as usize)),
                1.0,
                0.15,
            )
        };
        render_tee_for_ui(
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui,
            ui_state,
            rect,
            None,
            pipe.user_data.skin,
            pipe.user_data.skin_info.as_ref(),
            center,
            size * val,
            eye,
        );
    }

    ui_state.add_glass_elipse(
        egui::pos2(mouse.x as f32, mouse.y as f32),
        egui::vec2(75.0, 75.0),
        2.2,
        Color32::from_rgba_unmultiplied(200, 200, 255, 255),
    );
}
