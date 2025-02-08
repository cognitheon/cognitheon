use egui::Key;

pub fn space_pressed(ui: &mut egui::Ui) -> bool {
    ui.input(|i| i.key_pressed(Key::Space))
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
