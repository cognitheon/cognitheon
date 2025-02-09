use egui::{Key, Response};

use super::input::is_input_busy;

pub fn space_pressed(ui: &mut egui::Ui) -> bool {
    ui.input(|i| i.key_down(Key::Space))
}

pub fn space_released(ui: &mut egui::Ui) -> bool {
    ui.input(|i| i.key_released(Key::Space))
}

pub fn zooming(ui: &mut egui::Ui) -> bool {
    ui.input(|i| i.zoom_delta() != 1.0)
}

pub fn primary_button_down(ui: &mut egui::Ui) -> bool {
    ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
}

pub fn scrolling(ui: &mut egui::Ui) -> Option<egui::Vec2> {
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
    if scroll_delta != egui::Vec2::ZERO {
        return Some(scroll_delta);
    }
    None
}

pub fn drag_select(ui: &mut egui::Ui, canvas_response: &Response) -> bool {
    if !is_input_busy(ui)
        && canvas_response.hovered()
        && ui.input(|i| {
            i.pointer.button_down(egui::PointerButton::Primary)
                && !i.key_down(egui::Key::Space)
                && i.modifiers.is_none()
        })
    {
        return true;
    } else {
        return false;
    }
    // if canvas_response.dragged_by(egui::PointerButton::Primary)
    //     && !ui.input(|i| i.key_pressed(egui::Key::Space))
    // {
    //     true
    // } else {
    //     false
    // }
}
