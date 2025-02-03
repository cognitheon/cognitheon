// 为亮色/暗色主题设置颜色

pub fn node_border(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(19, 90, 155, 200)
    } else {
        egui::Color32::from_rgba_premultiplied(66, 144, 218, 200)
    }
}

pub fn node_border_selected(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(222, 78, 78, 200)
}

pub fn node_background(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(180, 180, 180, 200)
    } else {
        egui::Color32::from_rgba_premultiplied(70, 70, 70, 200)
    }
}
