pub fn detect_drag_canvas(ui: &mut egui::Ui) -> bool {
    ui.input(|i| {
        i.key_down(egui::Key::Space) && i.modifiers.is_none() && i.pointer.primary_pressed()
    })
}

pub fn detect_select_node(ui: &mut egui::Ui) -> bool {
    ui.input(|i| {
        i.key_down(egui::Key::Space) && i.modifiers.is_none() && i.pointer.secondary_pressed()
    })
}
