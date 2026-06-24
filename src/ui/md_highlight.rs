//! 编辑态 Markdown 源码语法高亮：用于 `TextEdit::layouter`。
//!
//! 轻量手写高亮（不依赖 syntect），覆盖最常见记号：
//! - 行级：标题 `#..`、列表 `- * + `、有序 `1. `、引用 `> `
//! - 行内：`[[双链]]`（本项目核心，醒目高亮）、行内代码 `` `..` ``、`**粗体**`、`*斜体*`
//!
//! 不变量：输出的 [`LayoutJob`] 必须**逐字节覆盖**输入文本（egui 要求 galley 文本与源完全一致），
//! 否则 TextEdit 会 panic。下方所有分支都保证"先冲刷未着色的普通段，再追加着色段"。

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Stroke};

/// 高亮配色（按明暗主题取色）。
pub struct MdColors {
    pub base: Color32,
    pub heading: Color32,
    pub link: Color32,
    pub code: Color32,
    pub emph: Color32,
    pub marker: Color32,
}

impl MdColors {
    pub fn from_visuals(v: &egui::Visuals) -> Self {
        let strong = v.strong_text_color();
        Self {
            base: v.text_color(),
            heading: strong,
            link: Color32::from_rgb(0x4f, 0xa3, 0xff),
            code: if v.dark_mode {
                Color32::from_rgb(0xe0, 0xa0, 0x70)
            } else {
                Color32::from_rgb(0xa0, 0x55, 0x10)
            },
            emph: strong,
            marker: v.weak_text_color(),
        }
    }
}

fn fmt(font: &FontId, color: Color32, italics: bool, underline: bool) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color,
        italics,
        underline: if underline {
            Stroke::new(1.0, color)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    }
}

/// 标题级别（1..=6），需 `#` 后紧跟空格；否则 None。
fn heading_level(s: &str) -> Option<usize> {
    let hashes = s.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && s[hashes..].starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

/// 列表/引用行首记号的字节长度（含尾随空格）；否则 None。
fn line_marker_len(s: &str) -> Option<usize> {
    for p in ["- ", "* ", "+ ", "> "] {
        if let Some(rest) = s.strip_prefix(p) {
            return Some(s.len() - rest.len());
        }
    }
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && s[digits..].starts_with(". ") {
        return Some(digits + 2);
    }
    None
}

/// 追加一段行内文本，识别 `[[..]]` / `` `..` `` / `**..**` / `*..*`，其余按 base 着色。
fn append_inline(job: &mut LayoutJob, s: &str, prop: &FontId, mono: &FontId, c: &MdColors) {
    let mut plain_start = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        let rem = &s[i..];
        // (结束字节偏移, 字体, 颜色, 斜体, 下划线)
        let token: Option<(usize, &FontId, Color32, bool, bool)> =
            if let Some(rel) = rem.strip_prefix("[[").and_then(|r| r.find("]]")) {
                Some((i + 2 + rel + 2, prop, c.link, false, true))
            } else if let Some(r) = rem.strip_prefix('`') {
                r.find('`')
                    .map(|rel| (i + 1 + rel + 1, mono, c.code, false, false))
            } else if let Some(r) = rem.strip_prefix("**") {
                r.find("**")
                    .map(|rel| (i + 2 + rel + 2, prop, c.emph, false, false))
            } else if let Some(r) = rem.strip_prefix('*') {
                r.find('*')
                    .map(|rel| (i + 1 + rel + 1, prop, c.emph, true, false))
            } else {
                None
            };

        if let Some((end, font, color, italics, underline)) = token {
            if i > plain_start {
                job.append(&s[plain_start..i], 0.0, fmt(prop, c.base, false, false));
            }
            job.append(&s[i..end], 0.0, fmt(font, color, italics, underline));
            i = end;
            plain_start = i;
        } else {
            i += rem.chars().next().map_or(1, |ch| ch.len_utf8());
        }
    }
    if s.len() > plain_start {
        job.append(&s[plain_start..], 0.0, fmt(prop, c.base, false, false));
    }
}

/// 把 Markdown 源码布局成带语法高亮的 [`LayoutJob`]。
pub fn layout(text: &str, font_size: f32, wrap_width: f32, c: &MdColors) -> LayoutJob {
    let prop = FontId::new(font_size, FontFamily::Proportional);
    let mono = FontId::new(font_size, FontFamily::Monospace);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    for line in text.split_inclusive('\n') {
        let (content, nl) = match line.strip_suffix('\n') {
            Some(c) => (c, "\n"),
            None => (line, ""),
        };

        let trimmed = content.trim_start();
        let indent_len = content.len() - trimmed.len();
        if indent_len > 0 {
            job.append(
                &content[..indent_len],
                0.0,
                fmt(&prop, c.base, false, false),
            );
        }

        if heading_level(trimmed).is_some() {
            job.append(trimmed, 0.0, fmt(&prop, c.heading, false, false));
        } else if let Some(mlen) = line_marker_len(trimmed) {
            job.append(&trimmed[..mlen], 0.0, fmt(&prop, c.marker, false, false));
            append_inline(&mut job, &trimmed[mlen..], &prop, &mono, c);
        } else {
            append_inline(&mut job, trimmed, &prop, &mono, c);
        }

        if !nl.is_empty() {
            job.append(nl, 0.0, fmt(&prop, c.base, false, false));
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 高亮后的 LayoutJob.text 必须与源完全一致（逐字节覆盖），否则 TextEdit 会 panic。
    fn assert_covers(src: &str) {
        let c = MdColors {
            base: Color32::WHITE,
            heading: Color32::RED,
            link: Color32::BLUE,
            code: Color32::GREEN,
            emph: Color32::YELLOW,
            marker: Color32::GRAY,
        };
        let job = layout(src, 14.0, 200.0, &c);
        assert_eq!(job.text, src, "LayoutJob 必须逐字节覆盖源文本");
    }

    #[test]
    fn covers_all_constructs_including_chinese() {
        assert_covers("");
        assert_covers("纯文本一行");
        assert_covers("# 标题\n\n正文 **粗体** 与 *斜体* 与 `代码`。\n");
        assert_covers("- 列表项 [[双链]]\n1. 有序项\n> 引用\n");
        assert_covers("关联 [[知识图谱]] 和 [[第二大脑]] 🚀");
        assert_covers("未闭合 [[ 和 ` 和 ** 和 *");
        assert_covers("嵌套 **粗 *斜* 体** 与 `代码 [[非链接]]`");
    }
}
