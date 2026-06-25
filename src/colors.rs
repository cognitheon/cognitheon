// 为亮色/暗色主题设置颜色

/// 关键词过滤 Dim 模式下"淡出"图元的 alpha 缩放系数（0~1）。
///
/// 不匹配的节点 / 边把自身颜色的 alpha 乘以它后渲染（仍可见但明显变淡），与命中节点形成
/// 强对比。集中为常量便于统一调参，渲染端只调 [`dim`]。
pub const DIM_ALPHA_FACTOR: f32 = 0.18;

/// 把一个颜色按 [`DIM_ALPHA_FACTOR`] 降低 alpha，用于 Dim 模式淡出不匹配的节点 / 边。
///
/// egui `Color32` 是**预乘 alpha**（premultiplied）：缩放 alpha 必须连同 RGB 一起按同一系数
/// 缩放，否则颜色会失真发亮。`Color32::gamma_multiply` 正是"对预乘色整体乘一个 0~1 因子"
/// （linofy 后逐分量相乘）的标准做法，故直接用它，避免手算预乘分量的坑。
pub fn dim(color: egui::Color32) -> egui::Color32 {
    color.gamma_multiply(DIM_ALPHA_FACTOR)
}

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

/// 结构着色（按度数编码节点枢纽程度）的色阶：归一度数比 `ratio`（0~1）→ 冷→暖渐变色。
///
/// 把不可见的图结构（节点度数 / 枢纽程度）映射到外框颜色：
/// - `ratio == 0`（最低度数档）→ 冷色（蓝青）；
/// - `ratio == 1`（最高度数 = 全图枢纽）→ 暖色（橙红）；
/// - 中间线性插值（在 sRGB 分量上插，足够直观、无需 OKLab）。
///
/// `ratio` 由调用方按 `degree / max_degree` 算好并 `clamp(0,1)`（`max_degree == 0` 时全传 0、
/// 走最低档，避免除零）。**孤立节点（度数 0）不走本函数**——另用 [`structure_isolated`]
/// 的弱化色 + 虚线，区别于"低度但非孤立"。两主题统一色相、仅按明暗微调饱和度/亮度以保证可读。
pub fn structure_degree(theme: egui::Theme, ratio: f32) -> egui::Color32 {
    let t = ratio.clamp(0.0, 1.0);
    // 冷端（低度）与暖端（高度）的 RGB 锚点；暗色主题整体提亮以在深色卡片/网格上可辨。
    let (cold, warm) = if theme == egui::Theme::Light {
        ([30u8, 110u8, 180u8], [210u8, 70u8, 40u8])
    } else {
        ([70u8, 150u8, 220u8], [240u8, 120u8, 60u8])
    };
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        lerp(cold[0], warm[0]),
        lerp(cold[1], warm[1]),
        lerp(cold[2], warm[2]),
        220,
    )
}

/// 结构着色下**孤立节点（度数 0）**的外框色——弱化的中性灰（配合虚线绘制），明确区别于
/// "低度但有连接"的冷色：度数 0 = 图里的盲点，刻意画得最不显眼。两主题分浅/深。
pub fn structure_isolated(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_premultiplied(150, 150, 155, 150)
    } else {
        egui::Color32::from_rgba_premultiplied(120, 120, 125, 150)
    }
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

/// 标签 `#tag` 高亮色：编辑态正文里的 `#标签` 用此色渲染（仿 `[[双链]]` 醒目样式），
/// 提示「这是一个横切分类标签」。
///
/// 刻意区别于双链高亮蓝（[`crate::ui::md_highlight::MdColors::link`]）、悬空红
/// （[`wikilink_dangling`]）与行内代码橙（`MdColors::code`）——取**绿松石 / 青绿**色相：
/// 暗色主题用偏亮的青绿、亮色主题用更深的孔雀绿以保证在浅底上的对比度。
/// **v1 标签不进图**，此高亮纯为编辑态视觉提示（标签的检索由面板 + 搜索 + 本高亮覆盖）。
pub fn tag(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgb(0x0f, 0x82, 0x6e)
    } else {
        egui::Color32::from_rgb(0x3a, 0xd6, 0xb4)
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

/// 小地图（minimap）背景半透明垫底——让缩略图脱离画布网格背景而成一块清晰浮层。两主题分浅/深底。
pub fn minimap_bg(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgba_unmultiplied(245, 245, 247, 225)
    } else {
        egui::Color32::from_rgba_unmultiplied(28, 28, 30, 220)
    }
}

/// 小地图外框描边——比背景略亮的中性灰，勾勒浮层边界。两主题分浅/深。
pub fn minimap_frame(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgb(170, 170, 175)
    } else {
        egui::Color32::from_rgb(90, 90, 95)
    }
}

/// 小地图内单个节点的点色——与节点边框同色系的蓝，缩略可辨。两主题分浅/深以保证对比度。
pub fn minimap_node(theme: egui::Theme) -> egui::Color32 {
    if theme == egui::Theme::Light {
        egui::Color32::from_rgb(60, 100, 160)
    } else {
        egui::Color32::from_rgb(150, 180, 220)
    }
}

/// 小地图视口框描边——醒目橙，区别于节点点的蓝。两主题统一。
pub fn minimap_viewport_stroke(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(230, 130, 60, 230)
}

/// 小地图视口框极淡填充——提示"当前视野范围"但不挡住底下的节点点。两主题统一。
pub fn minimap_viewport_fill(_theme: egui::Theme) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(230, 130, 60, 28)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 结构着色色阶：ratio 端点 = 冷/暖锚点；clamp 越界；中点在两端之间单调过渡。
    #[test]
    fn structure_degree_endpoints_and_clamp() {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let cold = structure_degree(theme, 0.0);
            let warm = structure_degree(theme, 1.0);
            // 暖端比冷端更"红"、更不"蓝"（冷→暖 = 蓝青→橙红）。
            assert!(warm.r() > cold.r(), "暖端红分量应更大");
            assert!(warm.b() < cold.b(), "暖端蓝分量应更小");
            // clamp：越界 ratio 退化到端点。
            assert_eq!(structure_degree(theme, -1.0), cold, "负 ratio 截到冷端");
            assert_eq!(structure_degree(theme, 2.0), warm, "超界 ratio 截到暖端");
            // 中点红分量落在两端之间（单调插值）。
            let mid = structure_degree(theme, 0.5);
            assert!(mid.r() > cold.r() && mid.r() < warm.r(), "中点红分量居中");
        }
    }
}
