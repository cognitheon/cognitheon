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

/// 搜索命中态的节点描边色——琥珀 / 金色，刻意区别于"选中红"（`node_border_selected`）与
/// 边 hover 的浅蓝（`edge_hover`），让"当前搜索命中集"在画布上一眼可辨。两主题统一。
pub fn node_border_hit(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(240, 184, 40, 230)
}

pub fn node_background(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(180, 180, 180, 200)
    } else {
        egui::Color32::from_rgba_premultiplied(70, 70, 70, 200)
    }
}

/// 悬空双链色：编辑态正文里指向"当前不存在标题节点"的 `[[X]]` 用此暗红/橙色渲染，
/// 提示"此链接当前指向空"（resolve 会在退出编辑时自动补建该节点，故此为瞬态指示）。
/// 刻意区别于已存在双链的高亮蓝（[`crate::ui::md_highlight::MdColors::link`]）与正文 base，
/// 暗色主题用偏橙的暖红、亮色主题用更深的砖红以保证对比度。
pub fn wikilink_dangling(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgb(0xb0, 0x3a, 0x2e)
    } else {
        egui::Color32::from_rgb(0xe8, 0x7a, 0x5a)
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

/// 边标签文字颜色——比默认灰边线更亮、保证在边线/网格上可读。两主题各取高对比前景色。
pub fn edge_label_text(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgb(40, 40, 40)
    } else {
        egui::Color32::from_rgb(230, 230, 230)
    }
}

/// 边标签背景小色块——半透明垫底，让文字脱离边线/网格背景而清晰可读。两主题分别用浅/深底。
pub fn edge_label_bg(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(235, 235, 235, 215)
    } else {
        egui::Color32::from_rgba_premultiplied(45, 45, 45, 215)
    }
}
