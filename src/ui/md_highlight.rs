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
    /// 悬空双链色：`[[X]]` 中 X 当前在图里无对应标题节点时用此色（区别于 `link`）。
    pub dangling: Color32,
    /// 标签 `#tag` 高亮色（仿 `[[双链]]` 醒目样式，区别于 link 蓝 / dangling 红 / code 橙）。
    pub tag: Color32,
    pub code: Color32,
    pub emph: Color32,
    pub marker: Color32,
}

impl MdColors {
    pub fn from_visuals(v: &egui::Visuals) -> Self {
        let strong = v.strong_text_color();
        let theme = if v.dark_mode {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        Self {
            base: v.text_color(),
            heading: strong,
            link: Color32::from_rgb(0x4f, 0xa3, 0xff),
            dangling: crate::colors::wikilink_dangling(theme),
            tag: crate::colors::tag(theme),
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

/// 字符是否能构成标签 `#tag` 正文（与 `wikilink::is_tag_char` 同口径：字母 / 数字 / CJK / `_` / `-` / `/`）。
///
/// 高亮端必须与 `wikilink::parse_tags` 的解析判据一致，否则会出现「高亮了但搜不到」或反之的错位。
fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/'
}

/// 若 `rem` 以 `#标签` 开头（`#` 紧跟标签字符），返回标签**整体**（含前导 `#`）的字节长度；否则 None。
///
/// 与 `wikilink::parse_tags` 判据一致：`#` 后须紧跟 [`is_tag_char`]（空白 → markdown 标题，不匹配），
/// 标签吃到首个非标签字符为止。返回长度仅依赖 char 边界（`#` 1 字节 + 各标签 char 的 `len_utf8`），
/// 故切片落点恒在 char 边界，逐字节覆盖不变量安全。
fn tag_token_len(rem: &str) -> Option<usize> {
    let after_hash = rem.strip_prefix('#')?;
    let mut len = 0usize; // 标签正文（不含 `#`）的字节长度
    for ch in after_hash.chars() {
        if is_tag_char(ch) {
            len += ch.len_utf8();
        } else {
            break;
        }
    }
    // `#` 后无标签字符（标题 `# ` / 孤立 `#` / `#标点`）→ 不是标签。
    if len == 0 {
        None
    } else {
        Some(1 + len) // 1 = `#`
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
///
/// `is_known(title)` 判定 `[[title]]` 的目标标题当前是否已在图中存在（trim 后比较）：存在用 `c.link`
/// 高亮、不存在用 `c.dangling` 悬空色。谓词由调用方在取图锁的闭包外预计算后按值传入，
/// **本函数不接触任何共享资源 / 图锁**（§3.1：layouter 闭包内绝不取锁）。
fn append_inline(
    job: &mut LayoutJob,
    s: &str,
    prop: &FontId,
    mono: &FontId,
    c: &MdColors,
    is_known: &dyn Fn(&str) -> bool,
) {
    let mut plain_start = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        let rem = &s[i..];
        // (结束字节偏移, 字体, 颜色, 斜体, 下划线)
        let token: Option<(usize, &FontId, Color32, bool, bool)> =
            if let Some(rel) = rem.strip_prefix("[[").and_then(|r| r.find("]]")) {
                // 双链：标题原文 = `[[` 与 `]]` 之间（rel 是 `]]` 相对 `rem[2..]` 的字节偏移，
                // 均落在 ASCII `[`/`]` 边界上，故对中文标题安全、不切多字节）。按 wikilink
                // 的提取语义 trim 首尾空白后判定是否存在，选高亮色 / 悬空色——着色判定不改动
                // append 的 range，逐字节覆盖不变量不受影响。
                let title = rem[2..2 + rel].trim();
                let color = if is_known(title) { c.link } else { c.dangling };
                Some((i + 2 + rel + 2, prop, color, false, true))
            } else if let Some(tlen) = tag_token_len(rem) {
                // 标签 `#tag`：整体（含 `#`）着 tag 色、加下划线（仿 [[双链]] 醒目样式）。
                // tag_token_len 的端点恒在 char 边界（见其文档），逐字节覆盖不变量不受影响；
                // 与标题 `# ` 互斥（heading 行不进 append_inline，且 `# ` 这里也因后跟空白返回 None）。
                Some((i + tlen, prop, c.tag, false, true))
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
///
/// `is_known(title)`：判定 `[[title]]` 目标标题是否已存在于图中（存在高亮、不存在悬空色）。
/// 调用方须在取图锁的闭包外预计算（如把"已存在标题集合" `HashSet<String>` 按值 capture），
/// **本函数纯逻辑、不取任何锁**，可在 egui 布局期的 `TextEdit::layouter` 闭包内安全调用。
pub fn layout(
    text: &str,
    font_size: f32,
    wrap_width: f32,
    c: &MdColors,
    is_known: &dyn Fn(&str) -> bool,
) -> LayoutJob {
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
            append_inline(&mut job, &trimmed[mlen..], &prop, &mono, c, is_known);
        } else {
            append_inline(&mut job, trimmed, &prop, &mono, c, is_known);
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

    const DANGLING: Color32 = Color32::from_rgb(0xff, 0x00, 0xff); // 测试用悬空色（区别于 link 蓝）
    const TAG: Color32 = Color32::from_rgb(0x00, 0xff, 0xff); // 测试用标签色（区别于其它）

    fn test_colors() -> MdColors {
        MdColors {
            base: Color32::WHITE,
            heading: Color32::RED,
            link: Color32::BLUE,
            dangling: DANGLING,
            tag: TAG,
            code: Color32::GREEN,
            emph: Color32::YELLOW,
            marker: Color32::GRAY,
        }
    }

    /// 高亮后的 LayoutJob.text 必须与源完全一致（逐字节覆盖），否则 TextEdit 会 panic。
    /// 对"全部标题已知"与"全部标题悬空"两种谓词都断言——两条着色分支都不得破坏覆盖。
    fn assert_covers(src: &str) {
        let c = test_colors();
        for is_known in [&(|_: &str| true) as &dyn Fn(&str) -> bool, &|_| false] {
            let job = layout(src, 14.0, 200.0, &c, is_known);
            assert_eq!(job.text, src, "LayoutJob 必须逐字节覆盖源文本");
        }
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
        // 悬空 / 存在混合、中文标题：两种谓词下都逐字节覆盖
        assert_covers("混合 [[已存在]] 与 [[缺失]] 收尾");
        // 标签 #tag：英文 / 中文 / 层级 / 标点终止 / 孤立 # / 与标题互斥，均须逐字节覆盖
        assert_covers("见 #rust 这里");
        assert_covers("#知识图谱，很重要");
        assert_covers("层级 #a/b/c 与 #foo_bar-baz");
        assert_covers("混合 #tag 和 [[双链]] 与 `代码` 收尾");
        assert_covers("孤立 # 号 与 #! 非标签 与行尾 #");
        assert_covers("- 列表里的 #tag 标签项\n> 引用里 #quote 标签");
    }

    /// 找到 `job` 中正好覆盖子串 `needle` 的那一段的颜色（按字节区间命中）。
    fn color_of_substr(job: &LayoutJob, needle: &str) -> Color32 {
        let start = job.text.find(needle).expect("needle 应出现在源文本里");
        let end = start + needle.len();
        job.sections
            .iter()
            .find(|s| s.byte_range.start <= start && s.byte_range.end >= end)
            .map(|s| s.format.color)
            .expect("应有一段覆盖该子串")
    }

    #[test]
    fn dangling_vs_known_wikilink_color() {
        let c = test_colors();
        // 只有 "已存在" 已知；"缺失" 与中文 "知识图谱" 视为悬空。
        let known = |t: &str| t == "已存在";
        let src = "看 [[已存在]] 和 [[缺失]] 与 [[知识图谱]]";
        let job = layout(src, 14.0, 200.0, &c, &known);

        assert_eq!(
            color_of_substr(&job, "[[已存在]]"),
            c.link,
            "已存在 → 高亮色"
        );
        assert_eq!(color_of_substr(&job, "[[缺失]]"), DANGLING, "缺失 → 悬空色");
        assert_eq!(
            color_of_substr(&job, "[[知识图谱]]"),
            DANGLING,
            "缺失中文标题 → 悬空色"
        );
    }

    #[test]
    fn tag_highlight_color_and_boundaries() {
        let c = test_colors();
        let known = |_: &str| true;
        // 英文标签整体（含 `#`）着 tag 色。
        let job = layout("见 #rust 这里", 14.0, 200.0, &c, &known);
        assert_eq!(color_of_substr(&job, "#rust"), TAG, "#rust → 标签色");
        // 中文标签，标点终止：`#知识图谱` 着色、其后逗号不在标签内。
        let job2 = layout("#知识图谱，很重要", 14.0, 200.0, &c, &known);
        assert_eq!(
            color_of_substr(&job2, "#知识图谱"),
            TAG,
            "中文标签 → 标签色"
        );
        // 层级标签。
        let job3 = layout("项目 #a/b/c 末尾", 14.0, 200.0, &c, &known);
        assert_eq!(color_of_substr(&job3, "#a/b/c"), TAG, "层级标签 → 标签色");
    }

    #[test]
    fn heading_not_treated_as_tag() {
        let c = test_colors();
        let known = |_: &str| true;
        // `# 标题`（`#` 后空格）走 heading 分支着 heading 色，不是标签色。
        let job = layout("# 标题", 14.0, 200.0, &c, &known);
        assert_eq!(
            color_of_substr(&job, "# 标题"),
            c.heading,
            "# 标题 → heading 色，非标签"
        );
        // 行内孤立 `#`（后接空白）不着标签色——整行无标签段，按 base 覆盖。
        let job2 = layout("单独 # 号", 14.0, 200.0, &c, &known);
        assert_eq!(job2.text, "单独 # 号", "逐字节覆盖");
        assert_eq!(color_of_substr(&job2, "#"), c.base, "孤立 # → base，非标签");
    }

    #[test]
    fn wikilink_title_trimmed_before_lookup() {
        let c = test_colors();
        // 谓词按 trim 后标题匹配（与 wikilink::parse_links 的提取语义一致）。
        let known = |t: &str| t == "A";
        let job = layout("x [[ A ]] y", 14.0, 200.0, &c, &known);
        assert_eq!(
            color_of_substr(&job, "[[ A ]]"),
            c.link,
            "首尾空白应被 trim 后再判定存在"
        );
    }
}
