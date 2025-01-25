// 为亮色/暗色主题设置颜色

pub fn node_background(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(180, 180, 180, 200)
    } else {
        egui::Color32::from_rgba_premultiplied(50, 50, 50, 200)
    }
}
