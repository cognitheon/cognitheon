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

/// 边的默认描线颜色（未 hover / 未选中）。当前边渲染写死 `Color32::GRAY`，集中到此以便统一。
pub fn edge_default(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::GRAY
}

/// 边在 hover 态的描线颜色——比默认更亮，提示"可点选"。两主题统一用浅蓝。
pub fn edge_hover(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(120, 170, 230, 220)
}

/// 边在选中态的描线颜色——与节点选中同色系（醒目红），保持选中语义视觉一致。
pub fn edge_selected(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(222, 78, 78, 230)
}
