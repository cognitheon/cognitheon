//! Markdown / vault 导出：把图导成一组 Obsidian 兼容的 `.md` 文件（外加手画边旁路 JSON），
//! 与 Obsidian 等 PKM 工具互通。**纯函数风格、零 IO**——所有落盘 / 浏览器下载在 [`crate::app`]
//! 层经 [`crate::io`] 网关完成（§3.1 只读图、导出非图变更不进 history）。
//!
//! ## 为什么 markdown 独立于 `.cnt`（AGENTS.md §7）
//! `.cnt`（[`crate::persistence`]）是 cognitheon 的**唯一真档格式**（带 schema 版本、往返保真整图）。
//! 本模块是**额外的开放导出格式**，不改 `.cnt` 序列化布局、不参与持久化兼容——它牺牲部分保真度
//! （手画边只能按端点标题对引用、重名会歧义）换取与 Obsidian 的天然互通。
//!
//! ## 格式约定（用户拍板）
//! - **每节点一个 `标题.md`**：正文即 `Node.note` **原样**（note = SSOT，绝不改写，§7）。`[[标题]]`
//!   与 Obsidian wikilink 同语法，天然兼容、原样保留即可。
//! - **最小手写 YAML frontmatter**（`---` 包裹，不引入 `serde_yaml` 新依赖）：`id` / `position`
//!   （画布坐标，§3.2）/ `aliases`（非空才写）。值转义见 [`yaml_quote`]。
//! - **Wiki 边不导出文件**：wiki 边本就是 note 里 `[[标题]]` 的幂等投影（见 [`crate::wikilink`]），
//!   随 note 文本天然携带，无需旁路。
//! - **手画 Manual 边写 [`MANUAL_EDGES_FILENAME`] 旁路**：手画边不在任何 note 里，单独用端点
//!   `Node.text` 对引用导出 JSON 保往返。已知局限：端点重名会错连（与 `find_by_title` 同源歧义）。
//!
//! ## ZIP 方案：手写 store-only（无压缩）写入器（[`store_zip`]）
//! wasm 下 vault 导出要把多文件打包成单个下载。**故意不引入 `zip` / `flate2` crate**——它们的
//! 压缩后端（`flate2` 默认 `miniz_oxide` 尚可，但 `zip` 的特性矩阵与 C 后端风险面大）在
//! wasm32-unknown-unknown 上是额外的兼容性负担（AGENTS.md §2 红线：wasm 分支绝不拉 C 依赖）。
//! store-only ZIP 格式简单确定（local file header + 原始字节 + central directory + EOCD），CRC-32
//! 纯 Rust 可实现（[`store_zip::crc32`]）——~150 行无新依赖、native/wasm 同一份代码，最稳。

use crate::graph::edge::EdgeOrigin;
use crate::graph::graph_impl::Graph;
use crate::graph::node::Node;
use crate::resource::CanvasStateResource;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};

/// 手画边旁路文件名（与 .md 同目录 / 同 zip）。下划线前缀 + 明确命名，避免与任何用户节点
/// `标题.md` 撞名（节点标题极不可能叫这个），导入侧据此识别。
pub const MANUAL_EDGES_FILENAME: &str = "_cognitheon_manual_edges.json";

/// 一个待导出的 vault 文件（文件名 + 字节内容）。native 逐个写盘、wasm 打包进 zip 共用此结构。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultFile {
    /// 文件名（已 sanitize + 去重，含扩展名）。
    pub filename: String,
    /// 文件字节内容（`.md` 是 UTF-8 文本、旁路是 JSON 文本，统一按字节看待便于打包）。
    pub bytes: Vec<u8>,
}

// ============================ 单节点 → Markdown ============================

/// 把一个节点渲染成 Obsidian 兼容的 Markdown 文本：手写 YAML frontmatter + 正文 note **原样**。
///
/// `final_stem` 是该节点**最终落盘的 `.md` 文件名（不含扩展名）**——由 [`sanitize_filename`] +
/// [`unique_md_filename`] 去重产出。当它 **≠ `node.text`**（标题含被 sanitize 的非法字符，或重名被
/// 去重改名）时，Obsidian 显示的笔记名变成改名后的串，别处 `[[原标题]]` 无法寻址回来。此时把
/// **原始 `node.text` 注入 aliases**（放最前、与既有 aliases 合并去重），让 Obsidian 用 alias
/// 解析 `[[原标题]]`，**原标题不丢、链接句柄保真**（[`crate::wikilink::find_by_title`] 以 text 优先、
/// alias 次之寻址）。`final_stem == node.text`（无歧义、未改名）则不注入。
///
/// 结构：
/// ```text
/// ---
/// id: 42
/// position: [12.5, -7]
/// aliases: ["原标题", "别名一", "别名二"]   # aliases 为空且未改名才省略
/// ---
///
/// <Node.note 原样，逐字节不动>
/// ```
///
/// 不变量：
/// - **note 绝不改写**（SSOT，§7）：frontmatter 之后直接拼 `node.note`，不做任何转义 / 归一 / trim。
/// - `position` 用画布坐标（§3.2），数字直接写（[`fmt_num`] 把整数值去掉 `.0`、保持紧凑）。
/// - 字符串值（aliases，含注入的原标题）经 [`yaml_quote`] 严谨转义，避免 Obsidian/YAML 解析失败。
/// - `id` 是 `u64` 纯数字、无需引号。
pub fn node_to_markdown(node: &Node, final_stem: &str) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", node.id));
    out.push_str(&format!(
        "position: [{}, {}]\n",
        fmt_num(node.position.x),
        fmt_num(node.position.y)
    ));

    // 标题被 sanitize / 去重改名（最终文件名 stem ≠ 原标题）→ 注入原标题为 alias，保住链接句柄。
    // 合并去重：原标题放最前，再追加既有 aliases 里不与之重复的项，保持顺序稳定。
    // 空标题不注入：空串无法被 find_by_title 寻址（`parse_links` 跳过空标题），注入只会写出无意义的
    // `aliases: [""]`——与 collect_manual_edges 跳过空标题端点同口径（空标题不是可寻址句柄）。
    let aliases: Vec<&str> = {
        let rename_changed_title = final_stem != node.text && !node.text.is_empty();
        let mut v: Vec<&str> = Vec::with_capacity(node.aliases.len() + 1);
        if rename_changed_title {
            v.push(node.text.as_str());
        }
        for a in &node.aliases {
            if !v.contains(&a.as_str()) {
                v.push(a.as_str());
            }
        }
        v
    };
    if !aliases.is_empty() {
        let quoted: Vec<String> = aliases.iter().map(|a| yaml_quote(a)).collect();
        out.push_str(&format!("aliases: [{}]\n", quoted.join(", ")));
    }

    out.push_str("---\n\n");
    // 正文 note 原样拼接（SSOT，绝不改写）。
    out.push_str(&node.note);
    out
}

/// 把 `f32` 坐标格式化成紧凑 YAML 数字：整数值省去 `.0`（`12.0` → `12`），非整数保留默认 `Display`
/// 精度（`12.5` → `12.5`）。避免 frontmatter 里到处是 `.0` 噪声，同时不丢精度。
///
/// 非有限值（NaN / ±Inf）回退为 `0`——画布坐标正常不会出现，但 YAML 无合法表示，兜底避免写出
/// 解析不了的 `inf` / `nan`。
fn fmt_num(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    if v.fract() == 0.0 {
        // 整数值：去掉小数部分（含 -0.0 归一为 0）。
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// 把一个字符串值转义成安全的 YAML 标量。
///
/// 规则：仅当值"可能被 YAML/Obsidian 误解析"时才加双引号包裹——含 YAML 指示符 / 特殊字符
/// （`: # [ ] { } , & * ! | > ' " % @ \`` 反引号 等）、**位于标量起始的流上下文指示符**
/// （首字符 `-` / `?` / `~`，详见下方）、前后空格、或任意非 ASCII（中文等，稳妥起见一律引号化）、
/// 或空串。包裹时按 YAML 双引号字符串规则转义内部 `\` 与 `"`，并把控制字符（`\n` `\t` `\r`）
/// 转义成 YAML 转义序列，避免破坏行结构。
///
/// ## 为什么单独判定"起始指示符"（用真实 serde_yaml 0.9 实测）
/// aliases 写进 flow 序列 `aliases: [v1, v2]`，其中以指示符**起头**的标量会被 YAML 误读：
/// - 首字符 `-` 后跟空格/串尾（`- item` / `-`+串尾）→ 解析为 block-seq 起始 → **整块 frontmatter
///   解析失败**（不止丢 aliases，连 id/position 都读不到）；
/// - 首字符 `?`（无论后随什么）→ 解析为 complex-mapping key（`? item` → `{item: null}`）或直接报错，
///   **静默错读或整块失败**；
/// - 首字符 `~`（即便后随空格/串尾的裸 `~`）→ 解析为 YAML null，**丢掉别名内容**。
///
/// 故这里把"首字符为 `-` / `?` / `~`"一律强制引号化（过度引号化无害，仍往返保真；漏引号化会
/// 损坏整块 frontmatter）。`:` `#` `@` `` ` `` 等其它指示符已在上面的字符表里覆盖。
///
/// 纯 ASCII 且不含上述危险字符的"裸词"（如 `Note1`）直接原样返回，保持 frontmatter 可读。
pub fn yaml_quote(s: &str) -> String {
    // 首字符若是 YAML 标量起始指示符（`-` / `?` / `~`），在 flow 序列里会被误解析为
    // block-seq / complex-mapping / null —— 一律强制引号化（见上方实测说明）。
    let starts_with_flow_indicator = matches!(s.chars().next(), Some('-') | Some('?') | Some('~'));

    let needs_quote = s.is_empty()
        || s.starts_with(' ')
        || s.ends_with(' ')
        || starts_with_flow_indicator
        || s.chars().any(|c| {
            !c.is_ascii()
                || c.is_control()
                || matches!(
                    c,
                    ':' | '#'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | ','
                        | '&'
                        | '*'
                        | '!'
                        | '|'
                        | '>'
                        | '\''
                        | '"'
                        | '%'
                        | '@'
                        | '`'
                        | '\\'
                )
        });

    if !needs_quote {
        return s.to_owned();
    }

    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ============================ Markdown → 单节点（导入侧，纯函数） ============================

/// 解析一个 YAML flow 序列 `[...]` 的内部（不含中括号）成元素列表。
///
/// 与 [`node_to_markdown`] 写出的 aliases 形态**严格往返**（它是 #18 导出的逆运算）。按 YAML 双引号
/// 字符串规则做状态机扫描，正确处理含逗号 / 转义的引号化元素（裸 `, ` 硬切会切错 `"a, b"` 这类值）：
/// - 引号外的 `,` 才是元素分隔符；引号内的 `,` 属于值的一部分；
/// - 引号内 `\"` / `\\` / `\n` / `\t` / `\r` 按 [`yaml_quote`] 的转义反向还原；
/// - 裸词（未引号化）元素 trim 两端空白后原样取用。
///
/// 只覆盖 [`node_to_markdown`] 实际会写出的两种形态（裸词 / 双引号），不是通用 YAML 解析器。
fn parse_flow_seq(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        // 跳过元素前的空白。
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        match chars.peek() {
            None => break,
            Some('"') => {
                // 双引号字符串：扫到配对的未转义 `"`，按转义还原。
                chars.next(); // 吃掉开引号
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('\\') => s.push('\\'),
                            Some('"') => s.push('"'),
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('r') => s.push('\r'),
                            Some(other) => {
                                s.push('\\');
                                s.push(other);
                            }
                            None => s.push('\\'),
                        },
                        other => s.push(other),
                    }
                }
                items.push(s);
                // 吃掉到下一个 `,` 为止的空白与分隔符。
                while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
                    chars.next();
                }
                if matches!(chars.peek(), Some(',')) {
                    chars.next();
                }
            }
            Some(_) => {
                // 裸词：扫到引号外的 `,` 或串尾。
                let mut raw = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ',' {
                        break;
                    }
                    raw.push(c);
                    chars.next();
                }
                items.push(raw.trim().to_owned());
                if matches!(chars.peek(), Some(',')) {
                    chars.next();
                }
            }
        }
    }
    items
}

/// 从 [`node_to_markdown`] 输出里恢复的 frontmatter 元数据（id 解析出来但**导入侧不复用**，见
/// [`markdown_to_node`]）。
#[derive(Debug, Default, PartialEq)]
pub struct Frontmatter {
    /// frontmatter 里的 `position: [x, y]`（画布坐标，§3.2）。无 `---` 块 / 无该行 → `None`。
    pub position: Option<egui::Pos2>,
    /// frontmatter 里的 `aliases: [...]`（经 [`parse_flow_seq`] 还原引号化 / 含逗号元素）。
    pub aliases: Vec<String>,
}

/// 把一篇 `.md` 文本拆成 `(frontmatter, body)`：识别 #18 [`node_to_markdown`] 写出的 `---` 包裹
/// frontmatter，解析 `position` / `aliases`，其余即正文 body。**与导出严格往返**。
///
/// **CRLF 容错（必修项 3）**：真实 Obsidian vault / Windows / `git autocrlf` 来源的 `.md` 用
/// `---\r\n…---\r\n` 行尾。本函数对 frontmatter 区**只识别边界、不改字节**：开闭围栏同时容忍
/// `---\n` 与 `---\r\n`，闭合围栏前的 `\r` 一并吞掉；frontmatter 内的字段行经 [`str::lines`]
/// 解析（它已自动剥除行尾 `\r`，故 `position` / `aliases` 在 CRLF 下也正确解析）。**正文 body 仍逐
/// 字节原样**（不做任何行尾归一，note = SSOT，§7）——只在边界检测处兼容 CRLF，不触碰 body 内容。
///
/// 容错（AGENTS.md §7：来源原样、坏档不中断）：
/// - **无 frontmatter**（外部 Obsidian `.md`，或正文恰好以 `---` 开头但无闭合）→ `Frontmatter::default()`
///   + body = 整个 `content`（不丢正文）。
/// - **坏 frontmatter**（`position` / `aliases` 行格式不符）→ 该字段静默落默认值，**绝不 panic**
///   （`position` 解析失败回退 `None` 让调用方用占位坐标，`aliases` 解析失败回退空）。
///
/// body 不做任何 trim / 归一（note = SSOT，导入也原样保留紧跟 frontmatter 后那一个空行之后的全部内容）。
pub fn parse_frontmatter(content: &str) -> (Frontmatter, String) {
    // frontmatter 必须以 `---` 单独成行起头、并有闭合的 `---` 单独成行（与 node_to_markdown 的
    // `---\n…---\n\n` 对称）。**同时容忍 LF（`---\n`）与 CRLF（`---\r\n`）行尾**（必修项 3：真实
    // Obsidian / Windows / git autocrlf 来源是 CRLF），任一不满足 → 无 frontmatter、整段当正文。
    let rest = if let Some(r) = content.strip_prefix("---\r\n") {
        r
    } else if let Some(r) = content.strip_prefix("---\n") {
        r
    } else {
        return (Frontmatter::default(), content.to_owned());
    };
    // 闭合围栏 `\n---\n`：先找 `\n---\n`，命中后把围栏前可能存在的一个 `\r` 算进 frontmatter 区
    // （CRLF 下闭合行是 `…\r\n---\r\n`，`find("\n---\n")` 会落在 `---` 前的 `\r\n` 的 `\n` 上，
    // 围栏后那行 `---` 的尾随 `\r` 由 strip 区的 body 起始空行剥除处理）。
    let Some(end) = rest.find("\n---\n").or_else(|| rest.find("\n---\r\n")) else {
        // 有起始 `---` 但无闭合：当作纯正文（不把半截 frontmatter 吞掉）。
        return (Frontmatter::default(), content.to_owned());
    };
    // fm_block 截到闭合围栏前的 `\n`（含 CRLF 时该 `\n` 前的 `\r` 留在 block 末尾，由 `.lines()`
    // 解析字段时自动剥除，不影响解析）。
    let fm_block = &rest[..end];
    // 跳过闭合围栏本身：可能是 `\n---\n` 或 `\n---\r\n`，按实际命中的长度推进。
    let after_fence = &rest[end..];
    let body = if let Some(b) = after_fence.strip_prefix("\n---\r\n") {
        b
    } else {
        &after_fence["\n---\n".len()..]
    };
    // 去掉 frontmatter 与正文之间那**一个**空行（node_to_markdown 写的是 `---\n\n<note>`；CRLF
    // 来源是 `---\r\n\r\n<note>`，故同时剥 `\r\n` 与 `\n`）。仅剥这一个空行的行尾，body 其余原样。
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);

    let mut fm = Frontmatter::default();
    for raw_line in fm_block.lines() {
        // CRLF 容错（必修项 3）：`fm_block` 的**最后一行**可能尾带一个 `\r`（闭合围栏是 `\r\n---…`
        // 时该 `\r` 落在 block 末尾、不被 `.lines()` 剥除）。逐行去尾随 `\r` 再解析，使 CRLF 来源的
        // `position` / `aliases` 行的 `]` 仍能被 `strip_suffix` 命中——只归一 frontmatter 区行尾、不动正文。
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(v) = line.strip_prefix("position: [") {
            // `position: [x, y]` → 解析两数；任一坏掉则整体落 None（坏档容错、不 panic）。
            if let Some(inner) = v.strip_suffix(']') {
                let mut it = inner.split(',').map(str::trim);
                if let (Some(xs), Some(ys), None) = (it.next(), it.next(), it.next()) {
                    if let (Ok(x), Ok(y)) = (xs.parse::<f32>(), ys.parse::<f32>()) {
                        // 非有限值（NaN/Inf）视为无效坐标，回退占位（与 fmt_num 写出口径对称）。
                        if x.is_finite() && y.is_finite() {
                            fm.position = Some(egui::pos2(x, y));
                        }
                    }
                }
            }
        } else if let Some(v) = line.strip_prefix("aliases: [") {
            if let Some(inner) = v.strip_suffix(']') {
                fm.aliases = parse_flow_seq(inner);
            }
        }
        // id 行被刻意忽略：导入进既有图时 frontmatter id 可能与现有节点撞，统一由调用方重分配。
    }
    (fm, body.to_owned())
}

/// 把一篇 `.md`（文件名 + 正文）解析成一个 [`Node`]（**纯函数、零 IO、不进图**）。导入流水线的原子步。
///
/// 与 #18 [`node_to_markdown`] 严格往返（**链接保真**而非标题字节保真，见下）：
/// - **`Node.text` = 文件名去 `.md`**：#18 把 `text` 写进文件名；若标题被 `sanitize_filename` 改名，
///   #18 已把原 `text` 注入 frontmatter 的 aliases，故导入后 `[[原标题]]` 经别名仍可寻址
///   （[`crate::wikilink::find_by_title`] text 优先、alias 次之）。
/// - **`Node.note` = 正文 body**（[`parse_frontmatter`] 拆出，原样不改写，SSOT §7）。
/// - **id 不复用 frontmatter 的**：导入进既有图，frontmatter id 可能与现有节点撞——由调用方经
///   `canvas.new_node_id()` 重分配后传入 `id`。
/// - **position**：优先 frontmatter；缺失（外部 md / 坏档）则用调用方给的占位 `fallback_pos`（螺旋散布）。
/// - **aliases**：直接取自 frontmatter（含 #18 改名时注入的原标题），保住链接句柄。
///
/// `filename` 可含或不含 `.md` 扩展（统一 strip）；可含路径前缀（取 `file_name` 末段，防把目录当标题）。
pub fn markdown_to_node(filename: &str, content: &str, id: u64, fallback_pos: egui::Pos2) -> Node {
    let (fm, body) = parse_frontmatter(content);
    Node {
        id,
        position: fm.position.unwrap_or(fallback_pos),
        text: filename_to_title(filename),
        note: body,
        aliases: fm.aliases,
    }
}

/// 把上传的文件名转成节点标题：取末段（去掉任何 `/` `\` 路径前缀，防把目录名拼进标题）后去 `.md`
/// 扩展（大小写不敏感）。与 [`sanitize_filename`] + [`unique_md_filename`] 的导出文件名命名往返
/// （去重后缀 `-2` 等会留在标题里——往返是链接保真而非标题字节保真，见 [`markdown_to_node`]）。
fn filename_to_title(filename: &str) -> String {
    // 取末段：兼容用户上传时可能带的相对路径（vault 子目录），只要文件名本身。
    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();
    // 去 `.md`（大小写不敏感）。其它扩展（理论上不该出现）原样保留为标题。
    if stem.len() >= 3 && stem[stem.len() - 3..].eq_ignore_ascii_case(".md") {
        stem[..stem.len() - 3].to_owned()
    } else {
        stem.to_owned()
    }
}

// ============================ 一批 .md → 图（导入流水线，顺序敏感） ============================

/// 螺旋占位坐标生成器：为 frontmatter 缺坐标的导入节点（外部 Obsidian `.md`）给一个确定性散布点，
/// 避免它们全叠在原点。`i` 是该批次内"缺坐标节点"的序号（从 0 起）。
///
/// 用阿基米德螺旋 `r = step * sqrt(i)`、角度黄金角错开，铺成不重叠的盘状散布（坐标缺失时调用方
/// 还会整批跑一次力导向布局，故这里只需"不重合"即可，无需美观）。
fn spiral_placeholder(i: usize) -> egui::Pos2 {
    const STEP: f32 = 60.0;
    const GOLDEN_ANGLE: f32 = 2.399_963_2; // ~137.5°，黄金角（弧度）
    let r = STEP * (i as f32).sqrt();
    let theta = i as f32 * GOLDEN_ANGLE;
    egui::pos2(r * theta.cos(), r * theta.sin())
}

/// 一批 `.md` 导入的结果统计（供 UI / headless / 日志取证）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    /// 本批真正建出的节点数（每个 `.md` 一个；不含 resolve 为缺失 `[[链接]]` 目标新建的节点）。
    pub imported_nodes: usize,
    /// resolve 阶段为"正文里 `[[链接]]` 但批内/图中无此标题"自动新建的节点数。
    pub resolved_new_nodes: usize,
    /// resolve 阶段新建的 wiki 边数。
    pub resolved_edges: usize,
    /// 从手画边旁路 JSON 成功连上的 Manual 边数。
    pub manual_edges: usize,
    /// 因 frontmatter 缺 `position` 而用螺旋占位坐标的节点数（>0 且导入进**空图**时调用方应跑
    /// 一次力导向布局；导入进非空图则不重排，见 [`crate::app::CognitheonApp`] 的 import 文档）。
    pub placeheld_positions: usize,
    /// **append 语义告警计数（必修项 2）**：本批导入的 `.md` 节点里，其标题（`Node.text`）在导入**前**
    /// 已存在于图中的个数。导入是**迁入外部 vault** 语义——永远**新增节点、绝不合并/覆盖**既有同名节点
    /// （覆盖既有 note/position 对"迁入外部库"不合适）。该计数 >0 时调用方应 `log::warn` 提示：这些
    /// 节点已作为新节点追加、未合并；如需还原 vault 请先 File→New 清空再导入（空图导入无重名、无重复）。
    pub renamed_collisions: usize,
}

/// 把一批 `(filename, content)` 导入**既有图**（顺序敏感的核心流水线，纯图操作、零 IO）。
///
/// **append 语义（必修项 2，已定）**：导入 = **迁入外部 vault**——每个 `.md` 永远 `add_node` 成
/// **新节点**，**绝不**与既有同名节点对账、合并或覆盖（覆盖既有节点的 note/position 是另一种破坏，
/// 对"迁入外部库"不合适）。"导出→编辑→再导入"的往返靠**先 File→New 清空再导入**（空图导入无重名、
/// 无重复）。为消除"静默翻倍"，本函数统计本批与既有同名的节点数（[`ImportOutcome::renamed_collisions`]），
/// 由调用方告警提示用户。
///
/// **导入顺序铁律（AGENTS.md §3.3，防重复建点）**：必须**先把所有 `.md` 都 `add_node`**（拿到
/// `NodeIndex`），**再**对每个新节点跑 [`crate::wikilink::resolve_links`]。否则先 resolve 的节点会
/// 为"尚未导入的同名目标"新建重复节点（重名歧义放大）——先全建点让同批的 `[[标题]]` 能经
/// `find_by_title` 命中本批刚建的节点而非另起炉灶。
///
/// 流程：
/// 1. **过滤旁路**：从批次里分出手画边旁路 [`MANUAL_EDGES_FILENAME`]（按文件名识别），其余按 `.md` 处理。
/// 2. **先全建点**：每个 `.md` 经 [`markdown_to_node`] 建节点（id 由 `canvas.new_node_id()` 重分配，
///    position 优先 frontmatter、缺失用螺旋占位并计数）。
/// 3. **再逐个 resolve**：对**本批新建**的每个节点跑 `resolve_links`，把正文 `[[标题]]` 幂等投影成
///    wiki 边（缺失目标自动建，与手动编辑正文同一路径）。
/// 4. **连手画边旁路**：按端点 `Node.text`（经 `find_by_title`）连 [`EdgeOrigin::Manual`] 边；端点
///    寻址不到则 `log::warn` 跳过（坏旁路不中断整批）。
///
/// 容错：坏 frontmatter 由 [`parse_frontmatter`] 兜底（当纯正文）；坏旁路 JSON 解析失败 `log::warn`
/// 跳过（不中断 .md 导入）。本函数**只改图**——`history.mutate` 包裹 / 坐标缺失后的力导向布局 /
/// id 计数器推 max+1 由调用方（[`crate::app`] / headless）负责，使整批成一个撤销单元（§3.1）。
pub fn import_markdown_batch(
    graph: &mut Graph,
    canvas: &CanvasStateResource,
    files: &[(String, String)],
) -> ImportOutcome {
    use crate::graph::edge::Edge;
    use crate::wikilink::{find_by_title, resolve_links};

    let mut outcome = ImportOutcome::default();

    // append 语义对账（必修项 2）：导入**前**快照既有节点标题集，供后面判定本批新节点是否与既有
    // 同名（同名只统计告警、**仍作为新节点追加**，绝不合并/覆盖）。空标题不入集——它不是
    // find_by_title 的可寻址句柄，不参与重名判定（与 collect_manual_edges 跳过空标题同口径）。
    let preexisting_titles: std::collections::HashSet<String> = graph
        .graph
        .node_weights()
        .map(|n| n.text.clone())
        .filter(|t| !t.is_empty())
        .collect();

    // 1. 分离手画边旁路（按确定文件名识别，取末段比较以兼容带路径上传）与 .md。
    let mut md_files: Vec<&(String, String)> = Vec::new();
    let mut manual_sidecars: Vec<&str> = Vec::new();
    for f in files {
        let name = f.0.rsplit(['/', '\\']).next().unwrap_or(&f.0);
        if name == MANUAL_EDGES_FILENAME {
            manual_sidecars.push(&f.1);
        } else {
            md_files.push(f);
        }
    }

    // 2. 先全建点（顺序铁律：拿到全部 NodeIndex 后再 resolve）。
    let mut new_nodes: Vec<petgraph::graph::NodeIndex> = Vec::with_capacity(md_files.len());
    for (filename, content) in &md_files {
        let id = canvas.read_resource(|c| c.new_node_id());
        // 解析一次 frontmatter，按"有无 position"决定坐标来源：有则用 frontmatter，无则用螺旋占位
        // （按"缺坐标节点序号"递增，占位点彼此不重合）并计数，供调用方据 placeheld_positions>0 跑布局。
        let (fm, body) = parse_frontmatter(content);
        let position = match fm.position {
            Some(p) => p,
            None => {
                let p = spiral_placeholder(outcome.placeheld_positions);
                outcome.placeheld_positions += 1;
                p
            }
        };
        let title = filename_to_title(filename);
        // append 语义对账：标题在导入前已存在 → 计数告警（仍作为新节点追加，不合并）。
        if !title.is_empty() && preexisting_titles.contains(&title) {
            outcome.renamed_collisions += 1;
        }
        let idx = graph.add_node(Node {
            id,
            position,
            text: title,
            note: body,
            aliases: fm.aliases,
        });
        new_nodes.push(idx);
        outcome.imported_nodes += 1;
    }

    // 3. 再逐个 resolve（先全建点之后，本批同名目标已可被 find_by_title 命中、不会重复建）。
    for &idx in &new_nodes {
        let res = resolve_links(graph, canvas, idx);
        outcome.resolved_new_nodes += res.created_nodes.len();
        outcome.resolved_edges += res.created_edges;
    }

    // 4. 连手画边旁路：按端点标题（find_by_title）连 Manual 边。坏 JSON / 寻址不到 → warn 跳过。
    for json in &manual_sidecars {
        match serde_json::from_str::<Vec<ManualEdgeRef>>(json) {
            Ok(refs) => {
                for r in refs {
                    let src = find_by_title(graph, &r.source_title);
                    let dst = find_by_title(graph, &r.target_title);
                    match (src, dst) {
                        (Some(s), Some(t)) => {
                            let sp = graph.get_node(s).map(|n| n.position).unwrap_or_default();
                            let tp = graph.get_node(t).map(|n| n.position).unwrap_or_default();
                            graph.add_edge(Edge::new(s, t, sp, tp, canvas.clone()));
                            outcome.manual_edges += 1;
                        }
                        _ => log::warn!(
                            "import: manual edge endpoint not found (source={:?}, target={:?}), skipped",
                            r.source_title,
                            r.target_title
                        ),
                    }
                }
            }
            Err(e) => log::warn!("import: manual edges sidecar parse failed: {e} (skipped)"),
        }
    }

    outcome
}

// ============================ 文件名 sanitize + 去重 ============================

/// 把节点标题转成单个文件名片段（不含扩展名、不含路径分隔符）：替换文件名非法字符
/// （`/ \ : * ? " < > |` 与控制字符）为 `_`，trim 首尾空白。**空标题回退** `node-{id}`。
///
/// 不负责加 `.md` 扩展，也不负责去重——去重由 [`vault_files`] 统一处理（跨多个节点才有意义）。
pub fn sanitize_filename(title: &str, id: u64) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        format!("node-{id}")
    } else {
        trimmed.to_owned()
    }
}

/// 在一组**已存在**的文件名（小写比较，跨平台大小写不敏感文件系统也安全）里为 `base`（不含扩展）
/// 求一个唯一名：若 `base.md` 未占用直接用；否则追加 `-2` / `-3` … 直到不冲突。返回完整文件名
/// （含 `.md`），并把它登记进 `used`。
///
/// 重名是 [`crate::wikilink::find_by_title`] 已知的歧义源（标题不唯一）；导出侧去重保证**文件不互相
/// 覆盖**（否则 vault 里少文件）。代价：去重后文件名与标题不再一一对应，导入 Obsidian 时显示名仍取
/// frontmatter / 文件名，链接仍按 `[[标题]]` 寻址，去重后缀不影响 wikilink 解析。
fn unique_md_filename(base: &str, used: &mut std::collections::HashSet<String>) -> String {
    let mut candidate = format!("{base}.md");
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = format!("{base}-{n}.md");
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

// ============================ 手画边旁路 ============================

/// 手画边旁路 JSON 的一条记录：端点用 `Node.text`（标题）对引用，**不**用 `NodeIndex`——拓扑索引不
/// 跨文档稳定（导入时图是新建的），标题是跨 vault 唯一可对齐的句柄。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ManualEdgeRef {
    /// 源节点标题（`Node.text`）。
    pub source_title: String,
    /// 目标节点标题（`Node.text`）。
    pub target_title: String,
}

/// 收集图里所有**手画**（[`EdgeOrigin::Manual`]）边，按端点标题导出为 [`ManualEdgeRef`] 列表。
///
/// 只收 Manual 边：Wiki 边是 note 里 `[[标题]]` 的投影（随 .md 文本天然携带），导出会重复。端点节点
/// 失效（理论上不应发生——边在图里必有端点）则跳过该边。返回顺序按 `edge_references` 遍历序。
///
/// **空标题端点跳过 + `log::warn`**：旁路 JSON 以 `Node.text`（标题）作跨 vault 寻址句柄
/// （导入侧经 [`crate::wikilink::find_by_title`] 回连）。空标题既不是 `find_by_title` 可寻址的句柄
/// （`parse_links` / `find_by_title` 对空串永不命中），也与文件名的 `node-{id}` 回退**不同口径**
/// （那只是文件名、未注入节点 text/alias，写进旁路同样寻不回）。故空标题端点的手画边**无法保真导出**
/// —— 与其写一条静默断链的空串引用，不如显式跳过并告警，行为可解释、不污染旁路 JSON。
pub fn collect_manual_edges(graph: &Graph) -> Vec<ManualEdgeRef> {
    graph
        .graph
        .edge_references()
        .filter(|e| e.weight().origin == EdgeOrigin::Manual)
        .filter_map(|e| {
            let source_title = graph.graph.node_weight(e.source()).map(|n| n.text.clone());
            let target_title = graph.graph.node_weight(e.target()).map(|n| n.text.clone());
            match (source_title, target_title) {
                (Some(s), Some(t)) => {
                    // 空标题端点无法被 find_by_title 寻址 → 跳过该边并告警（导出口径一致、可解释）。
                    if s.is_empty() || t.is_empty() {
                        log::warn!(
                            "manual edge skipped: empty-title endpoint not addressable (source={s:?}, target={t:?})"
                        );
                        None
                    } else {
                        Some(ManualEdgeRef {
                            source_title: s,
                            target_title: t,
                        })
                    }
                }
                _ => None,
            }
        })
        .collect()
}

/// 把手画边列表序列化成旁路 JSON 字节（`serde_json` pretty，便于 git/外部工具读）。空列表也照常
/// 写一个 `[]`——调用方据 [`collect_manual_edges`] 是否为空决定是否生成该文件。
pub fn manual_edges_json(edges: &[ManualEdgeRef]) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(edges).map(|mut v| {
        v.push(b'\n');
        v
    })
}

// ============================ vault 文件清单（纯计算） ============================

/// 计算整个 vault 的导出文件清单（**纯函数、零 IO**）：每个节点一个 `标题.md`（sanitize + 去重），
/// 外加（仅当存在手画边时）一个 [`MANUAL_EDGES_FILENAME`] 旁路 JSON。
///
/// 调用方（[`crate::app`]）拿到清单后：native 逐个写盘、wasm 经 [`store_zip::build`] 打包成单个 zip
/// 下载。两 target 行为不对称（写目录 / 下载 zip）属预期（AGENTS.md §2 各自门控）。
///
/// 文件名去重保证**不互相覆盖**（重名标题各得 `-2`/`-3` 后缀）。手画边 JSON 失败仅记日志、跳过该
/// 旁路文件（不阻断 .md 导出）。
pub fn vault_files(graph: &Graph) -> Vec<VaultFile> {
    let mut files = Vec::new();
    let mut used = std::collections::HashSet::new();

    for idx in graph.graph.node_indices() {
        let Some(node) = graph.graph.node_weight(idx) else {
            continue;
        };
        let base = sanitize_filename(&node.text, node.id);
        let filename = unique_md_filename(&base, &mut used);
        // 最终文件名 stem（去 `.md`）—— node_to_markdown 据此判定标题是否被 sanitize/去重改名，
        // 改名则把原标题注入 aliases 保住链接句柄（必修项 2）。
        let stem = filename.strip_suffix(".md").unwrap_or(&filename);
        let bytes = node_to_markdown(node, stem).into_bytes();
        files.push(VaultFile { filename, bytes });
    }

    let manual = collect_manual_edges(graph);
    if !manual.is_empty() {
        match manual_edges_json(&manual) {
            Ok(bytes) => files.push(VaultFile {
                filename: MANUAL_EDGES_FILENAME.to_owned(),
                bytes,
            }),
            Err(e) => log::error!("manual edges json failed: {e}"),
        }
    }

    files
}

// ============================ store-only ZIP 写入器 ============================

/// 手写的最小 **store-only（无压缩）** ZIP 写入器：纯 Rust、无新依赖、native/wasm 同一份实现。
///
/// 见模块文档「ZIP 方案」一节——故意不引入 `zip`/`flate2` crate 以彻底规避 wasm32 兼容/ C 依赖风险
/// （AGENTS.md §2）。store-only 格式确定：每文件一个 local file header + 原始字节；末尾一段 central
/// directory（每文件一条 header）+ end-of-central-directory（EOCD）。压缩方法 0 = stored。
pub mod store_zip {
    /// CRC-32（IEEE 802.3 多项式 `0xEDB88320`，ZIP 用的就是这个）。每次现算查表，避免引入 crc crate。
    ///
    /// 与标准解压器（`unzip` / Obsidian 导入 / OS 内置解压）的 CRC 校验一致——算错会让解压报"损坏"。
    pub fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg(); // 1 -> 0xFFFFFFFF, 0 -> 0
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// 一个待打包条目（zip 内文件名 + 字节）。
    pub struct Entry<'a> {
        pub name: &'a str,
        pub data: &'a [u8],
    }

    // ZIP 结构里反复用到的固定签名（小端写入）。
    const LOCAL_FILE_HEADER_SIG: u32 = 0x0403_4b50;
    const CENTRAL_DIR_HEADER_SIG: u32 = 0x0201_4b50;
    const EOCD_SIG: u32 = 0x0605_4b50;

    /// 最小合法 DOS 日期 1980-01-01：bit 0..5 = day(1)、bit 5..9 = month(1)、bit 9..16 = year-1980(0)。
    /// = `(0 << 9) | (1 << 5) | 1` = `0x0021`。DOS 日期的 month/day 不能为 0，故不写 0。
    const MIN_DOS_DATE: u16 = 0x0021;

    fn push_u16(buf: &mut Vec<u8>, v: u16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn push_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    /// 把若干条目打包成一段完整的 store-only ZIP 字节流（可被标准解压器读取）。
    ///
    /// 布局（严格按 PKWARE APPNOTE 的 store-only 子集）：
    /// 1. 对每个条目：local file header（含 CRC-32、压缩前后大小相等、方法 0）+ 文件名 + 原始字节；
    /// 2. central directory：每个条目一条 central header（记录其 local header 的偏移）；
    /// 3. EOCD：条目数、central directory 大小与起始偏移。
    ///
    /// 时间字段写 0（不暴露导出时刻；ZIP 允许 0 时间）；**日期字段写最小合法 DOS 日期
    /// 1980-01-01**（`0x0021` = year 0/month 1/day 1）——DOS 日期里 month/day 不能为 0，写 0/0 会得到
    /// 非法的 1980-00-00，个别严格读取器告警。版本/标志/磁盘号等取最常见兼容值。
    /// 文件名按 UTF-8 写入并置 general-purpose flag bit 11（语言编码标志），保证含中文标题正确解包。
    pub fn build(entries: &[Entry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        // 记录每个条目的 (CRC, 大小, local header 偏移, 文件名字节)，供 central directory 复用。
        struct Central {
            crc: u32,
            size: u32,
            offset: u32,
            name: Vec<u8>,
        }
        let mut centrals: Vec<Central> = Vec::with_capacity(entries.len());

        for entry in entries {
            let name_bytes = entry.name.as_bytes().to_vec();
            let crc = crc32(entry.data);
            let size = entry.data.len() as u32;
            let offset = out.len() as u32;

            // ---- local file header ----
            push_u32(&mut out, LOCAL_FILE_HEADER_SIG);
            push_u16(&mut out, 20); // version needed to extract (2.0)
            push_u16(&mut out, 0x0800); // general purpose flag: bit 11 = UTF-8 filename
            push_u16(&mut out, 0); // compression method: 0 = stored
            push_u16(&mut out, 0); // last mod file time
            push_u16(&mut out, MIN_DOS_DATE); // last mod file date: 1980-01-01 (最小合法 DOS 日期)
            push_u32(&mut out, crc);
            push_u32(&mut out, size); // compressed size (== uncompressed for stored)
            push_u32(&mut out, size); // uncompressed size
            push_u16(&mut out, name_bytes.len() as u16); // file name length
            push_u16(&mut out, 0); // extra field length
            out.extend_from_slice(&name_bytes);
            out.extend_from_slice(entry.data);

            centrals.push(Central {
                crc,
                size,
                offset,
                name: name_bytes,
            });
        }

        // ---- central directory ----
        let central_start = out.len() as u32;
        for c in &centrals {
            push_u32(&mut out, CENTRAL_DIR_HEADER_SIG);
            push_u16(&mut out, 20); // version made by
            push_u16(&mut out, 20); // version needed to extract
            push_u16(&mut out, 0x0800); // general purpose flag: UTF-8
            push_u16(&mut out, 0); // compression method: stored
            push_u16(&mut out, 0); // last mod file time
            push_u16(&mut out, MIN_DOS_DATE); // last mod file date: 1980-01-01 (最小合法 DOS 日期)
            push_u32(&mut out, c.crc);
            push_u32(&mut out, c.size); // compressed size
            push_u32(&mut out, c.size); // uncompressed size
            push_u16(&mut out, c.name.len() as u16); // file name length
            push_u16(&mut out, 0); // extra field length
            push_u16(&mut out, 0); // file comment length
            push_u16(&mut out, 0); // disk number start
            push_u16(&mut out, 0); // internal file attributes
            push_u32(&mut out, 0); // external file attributes
            push_u32(&mut out, c.offset); // relative offset of local header
            out.extend_from_slice(&c.name);
        }
        let central_size = out.len() as u32 - central_start;

        // ---- end of central directory ----
        push_u32(&mut out, EOCD_SIG);
        push_u16(&mut out, 0); // number of this disk
        push_u16(&mut out, 0); // disk where central directory starts
        push_u16(&mut out, centrals.len() as u16); // central dir records on this disk
        push_u16(&mut out, centrals.len() as u16); // total central dir records
        push_u32(&mut out, central_size); // size of central directory
        push_u32(&mut out, central_start); // offset of start of central directory
        push_u16(&mut out, 0); // comment length
        out
    }
}

/// 把一组 [`VaultFile`] 打包成单个 store-only ZIP 字节流（wasm vault 导出用，复用 native 也可）。
pub fn vault_zip(files: &[VaultFile]) -> Vec<u8> {
    let entries: Vec<store_zip::Entry<'_>> = files
        .iter()
        .map(|f| store_zip::Entry {
            name: &f.filename,
            data: &f.bytes,
        })
        .collect();
    store_zip::build(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, x: f32, y: f32, title: &str, note: &str, aliases: &[&str]) -> Node {
        Node {
            id,
            position: egui::pos2(x, y),
            text: title.to_owned(),
            note: note.to_owned(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ---- node_to_markdown：frontmatter + 正文原样 ----

    /// 把节点导成单文件 markdown 的测试快捷方式：stem = node.text（未改名口径），
    /// 对应"文件名 == 原标题、不必注入 alias"的常态。
    fn md_no_rename(n: &Node) -> String {
        node_to_markdown(n, &n.text)
    }

    #[test]
    fn markdown_has_frontmatter_and_verbatim_body() {
        let n = node(42, 12.0, -7.0, "Title", "正文 [[Other]] 原样\n第二行", &[]);
        let md = md_no_rename(&n);
        assert!(md.starts_with("---\n"), "应以 frontmatter 起头");
        assert!(md.contains("id: 42\n"));
        assert!(md.contains("position: [12, -7]\n"), "整数坐标去 .0");
        assert!(!md.contains("aliases"), "空别名不写 aliases 行");
        // frontmatter 后紧跟空行 + 正文原样（含 [[链接]] 与换行）。
        assert!(
            md.ends_with("正文 [[Other]] 原样\n第二行"),
            "正文必须逐字节原样"
        );
        assert!(md.contains("---\n\n正文"), "frontmatter 与正文间一个空行");
    }

    #[test]
    fn markdown_writes_aliases_when_present() {
        let n = node(1, 0.0, 0.0, "T", "body", &["别名一", "alias2"]);
        let md = md_no_rename(&n);
        // 中文别名引号化、ASCII 简单别名也在引号里（含非 ASCII 的列表项各自判定）。
        assert!(md.contains(r#"aliases: ["别名一", alias2]"#), "实际: {md}");
    }

    #[test]
    fn markdown_fractional_position_preserved() {
        let n = node(1, 12.5, 0.25, "T", "", &[]);
        let md = md_no_rename(&n);
        assert!(
            md.contains("position: [12.5, 0.25]\n"),
            "非整数坐标保留精度"
        );
    }

    #[test]
    fn markdown_does_not_rewrite_note() {
        // note 含特殊字符 / 前后空白 / YAML 样式行——必须逐字节原样，不被转义/trim。
        let note = "  ---\nfake: frontmatter\n#tag [[link]]   ";
        let n = node(1, 0.0, 0.0, "T", note, &[]);
        let md = md_no_rename(&n);
        assert!(md.ends_with(note), "note 必须原样不改写（SSOT）");
    }

    // ---- yaml_quote：转义严谨 ----

    #[test]
    fn yaml_quote_plain_ascii_word_unquoted() {
        assert_eq!(yaml_quote("Note1"), "Note1");
        assert_eq!(yaml_quote("hello-world_2"), "hello-world_2");
    }

    #[test]
    fn yaml_quote_special_chars_quoted() {
        assert_eq!(yaml_quote("a: b"), r#""a: b""#); // 冒号
        assert_eq!(yaml_quote("#tag"), r##""#tag""##); // 井号
        assert_eq!(yaml_quote("[bracket]"), r#""[bracket]""#);
        assert_eq!(yaml_quote("a, b"), r#""a, b""#);
        assert_eq!(yaml_quote(""), r#""""#); // 空串
        assert_eq!(yaml_quote(" leading"), r#"" leading""#); // 前导空格
        assert_eq!(yaml_quote("trailing "), r#""trailing ""#); // 尾随空格
    }

    #[test]
    fn yaml_quote_non_ascii_quoted() {
        assert_eq!(yaml_quote("中文"), r#""中文""#);
        assert_eq!(yaml_quote("café"), r#""café""#);
    }

    #[test]
    fn yaml_quote_escapes_inner_quote_and_backslash() {
        assert_eq!(yaml_quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(yaml_quote(r"a\b"), r#""a\\b""#);
        assert_eq!(yaml_quote("line1\nline2"), r#""line1\nline2""#);
    }

    /// 必修项 1：位于标量起始的流上下文指示符（首字符 `-` / `?` / `~`）必须被引号化。
    /// 否则写进 `aliases: [...]` flow 序列会让整块 frontmatter 解析失败或静默错读
    /// （已用 serde_yaml 0.9 实测，见 yaml_quote 文档）。
    #[test]
    fn yaml_quote_leading_flow_indicators_quoted() {
        // 首字符 `-` 后跟空格/串尾：YAML 误读为 block-seq，整块 frontmatter 崩。
        assert_eq!(yaml_quote("- item"), r#""- item""#);
        assert_eq!(yaml_quote("-"), r#""-""#);
        // 首字符 `?`（无论后随什么）：YAML 误读为 complex-mapping key 或报错。
        assert_eq!(yaml_quote("? item"), r#""? item""#);
        assert_eq!(yaml_quote("?"), r#""?""#);
        // 首字符 `~`：裸 `~` 会被读成 null，丢内容。
        assert_eq!(yaml_quote("~"), r#""~""#);
        assert_eq!(yaml_quote("~x"), r#""~x""#);
        // 即便后续不是 YAML 误读点，首字符仍是指示符 → 一律引号化（过度引号化无害、保真）。
        assert_eq!(yaml_quote("-item"), r#""-item""#);
    }

    /// 非起始位置的 `-` / `?` / `~` 不触发引号化（裸词仍可读）；纯安全 ASCII 词保持裸词。
    #[test]
    fn yaml_quote_non_leading_indicators_stay_plain() {
        assert_eq!(yaml_quote("a-b"), "a-b");
        assert_eq!(yaml_quote("x~y"), "x~y");
        assert_eq!(yaml_quote("hello_world"), "hello_world");
        // 注意：含 `?` 在中间的词仍是裸词（`?` 不在危险字符表、只在首字符触发）。
        assert_eq!(yaml_quote("a?b"), "a?b");
    }

    // ---- sanitize_filename：非法字符 / 空标题回退 ----

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_filename("a/b:c*d?e", 1), "a_b_c_d_e");
        assert_eq!(sanitize_filename(r#"a\b<c>d|e"#, 1), "a_b_c_d_e");
        assert_eq!(sanitize_filename("a\"b", 1), "a_b");
    }

    #[test]
    fn sanitize_empty_title_falls_back_to_node_id() {
        assert_eq!(sanitize_filename("", 7), "node-7");
        assert_eq!(sanitize_filename("   ", 9), "node-9"); // 仅空白
                                                           // `///` 全是非法字符 → 替换为 `___`（非空，不触发回退）。
        assert_eq!(sanitize_filename("///", 3), "___");
    }

    #[test]
    fn sanitize_keeps_unicode_title() {
        assert_eq!(sanitize_filename("知识图谱", 1), "知识图谱");
    }

    // ---- 去重 ----

    #[test]
    fn vault_dedups_colliding_titles() {
        let mut g = Graph::default();
        g.add_node(node(1, 0.0, 0.0, "Dup", "", &[]));
        g.add_node(node(2, 0.0, 0.0, "Dup", "", &[]));
        g.add_node(node(3, 0.0, 0.0, "Dup", "", &[]));
        let files = vault_files(&g);
        let names: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
        assert!(names.contains(&"Dup.md"));
        assert!(names.contains(&"Dup-2.md"));
        assert!(names.contains(&"Dup-3.md"));
        assert_eq!(files.len(), 3, "三个重名节点 → 三个不互相覆盖的文件");
    }

    #[test]
    fn vault_dedup_case_insensitive() {
        let mut g = Graph::default();
        g.add_node(node(1, 0.0, 0.0, "Note", "", &[]));
        g.add_node(node(2, 0.0, 0.0, "note", "", &[]));
        let files = vault_files(&g);
        let lower: Vec<String> = files.iter().map(|f| f.filename.to_lowercase()).collect();
        // 大小写不敏感去重：note.md 与 Note.md 视为冲突，第二个得 -2 后缀。
        assert_eq!(lower.len(), 2);
        assert_ne!(lower[0], lower[1], "大小写差异也应去重避免覆盖");
    }

    // ---- 单节点往返保真（解析 frontmatter + body） ----

    /// 一个极小的 frontmatter 解析器（仅测试用）：拆出 `---` 块与正文，回读 id/position/aliases，
    /// 验证 node_to_markdown 的输出可被结构化读回（往返保真）。aliases 经**正式** [`parse_flow_seq`]
    /// （已从 test-only 提升为模块函数、导入侧 [`parse_frontmatter`] 复用）正确处理含逗号 / 转义的
    /// 引号化元素。本 helper 仍单独回读 `id`（[`parse_frontmatter`] 刻意忽略 id），故保留。
    fn parse_md(md: &str) -> (u64, [f32; 2], Vec<String>, String) {
        let rest = md.strip_prefix("---\n").expect("frontmatter start");
        let end = rest.find("\n---\n").expect("frontmatter end");
        let fm = &rest[..end];
        let body = &rest[end + "\n---\n".len()..];
        let body = body.strip_prefix('\n').unwrap_or(body); // 去掉 frontmatter 后那个空行

        let mut id = 0u64;
        let mut pos = [0.0f32, 0.0];
        let mut aliases = Vec::new();
        for line in fm.lines() {
            if let Some(v) = line.strip_prefix("id: ") {
                id = v.parse().unwrap();
            } else if let Some(v) = line.strip_prefix("position: [") {
                let v = v.strip_suffix(']').unwrap();
                let mut it = v.split(", ");
                pos[0] = it.next().unwrap().parse().unwrap();
                pos[1] = it.next().unwrap().parse().unwrap();
            } else if let Some(v) = line.strip_prefix("aliases: [") {
                let v = v.strip_suffix(']').unwrap();
                aliases = parse_flow_seq(v);
            }
        }
        (id, pos, aliases, body.to_owned())
    }

    /// parse_flow_seq 自检：含逗号的引号化元素不被切错（必修项 4 的可信度修复直接验证）。
    #[test]
    fn parse_flow_seq_handles_quoted_commas_and_escapes() {
        // 引号内逗号属于值的一部分，不是分隔符。
        assert_eq!(
            parse_flow_seq(r#""a, b", c"#),
            vec!["a, b".to_string(), "c".to_string()]
        );
        // 转义还原：\" \\ \n。
        assert_eq!(
            parse_flow_seq(r#""say \"hi\"", "a\\b", "x\ny""#),
            vec![
                "say \"hi\"".to_string(),
                r"a\b".to_string(),
                "x\ny".to_string()
            ]
        );
        // 裸词与引号化混排。
        assert_eq!(
            parse_flow_seq(r#"plain, "中文", "a, b""#),
            vec!["plain".to_string(), "中文".to_string(), "a, b".to_string()]
        );
    }

    #[test]
    fn node_roundtrips_through_markdown() {
        let n = node(99, 3.5, -2.0, "标题", "正文 [[链接]] 内容", &["别名"]);
        let md = md_no_rename(&n);
        let (id, pos, aliases, body) = parse_md(&md);
        assert_eq!(id, 99);
        assert_eq!(pos, [3.5, -2.0]);
        assert_eq!(aliases, vec!["别名".to_string()]);
        assert_eq!(body, "正文 [[链接]] 内容", "正文往返保真");
    }

    /// 必修项 1 往返：含起始指示符 / 逗号的别名，导出后经 parse_md 能原样读回（不丢、不错切）。
    /// 这是"用真实 YAML 规则反解析"对 yaml_quote 起始指示符修复的端到端验证。
    #[test]
    fn aliases_with_leading_indicators_roundtrip() {
        let n = node(
            7,
            0.0,
            0.0,
            "T",
            "",
            &["- item", "? item", "~", "a, b", "中文"],
        );
        let md = md_no_rename(&n);
        let (_, _, aliases, _) = parse_md(&md);
        assert_eq!(
            aliases,
            vec![
                "- item".to_string(),
                "? item".to_string(),
                "~".to_string(),
                "a, b".to_string(),
                "中文".to_string(),
            ],
            "起始指示符 / 含逗号别名往返保真，实际 md=\n{md}"
        );
    }

    // ---- 必修项 2：标题改名 → 注入原标题为 alias ----

    /// 标题含被 sanitize 的非法字符（`a:b` → 文件名 `a_b`），导出后 frontmatter 的 aliases
    /// 应含**原标题** `a:b`，且经引号化能被 parse_md 读回——`[[a:b]]` 仍可寻址回该节点。
    #[test]
    fn rename_injects_original_title_into_aliases() {
        let n = node(1, 0.0, 0.0, "a:b", "body", &[]);
        let stem = sanitize_filename(&n.text, n.id); // "a_b"
        assert_ne!(stem, n.text, "标题被 sanitize 改名，前提成立");
        let md = node_to_markdown(&n, &stem);
        let (_, _, aliases, _) = parse_md(&md);
        assert_eq!(aliases, vec!["a:b".to_string()], "原标题应注入 aliases");
    }

    /// 改名时原标题注入到既有 aliases **最前**、与既有项合并去重。
    #[test]
    fn rename_merges_original_title_with_existing_aliases() {
        let n = node(1, 0.0, 0.0, "a:b", "body", &["既有别名", "a:b"]);
        let stem = sanitize_filename(&n.text, n.id); // "a_b"
        let md = node_to_markdown(&n, &stem);
        let (_, _, aliases, _) = parse_md(&md);
        // 原标题放最前；既有 aliases 里与原标题重复的 "a:b" 去重，不出现两次。
        assert_eq!(
            aliases,
            vec!["a:b".to_string(), "既有别名".to_string()],
            "原标题放最前 + 合并去重"
        );
    }

    /// 文件名 == 原标题（无歧义、未改名）时不注入 alias（无冗余）。
    #[test]
    fn no_rename_does_not_inject_alias() {
        let n = node(1, 0.0, 0.0, "Plain", "body", &[]);
        let md = node_to_markdown(&n, "Plain");
        assert!(!md.contains("aliases"), "未改名不应注入 alias，md=\n{md}");
    }

    /// 空标题节点（单节点导出走 `node-{id}` 回退文件名 → stem ≠ ""）不应把空串注入 aliases：
    /// 空串无法被 find_by_title 寻址，写 `aliases: [""]` 无意义（与必修项 3 跳过空标题端点同口径）。
    #[test]
    fn empty_title_does_not_inject_empty_alias() {
        let n = node(7, 0.0, 0.0, "", "body", &[]);
        let stem = sanitize_filename(&n.text, n.id); // "node-7"
        assert_ne!(stem, n.text, "回退文件名 ≠ 空标题，前提成立");
        let md = node_to_markdown(&n, &stem);
        assert!(
            !md.contains("aliases"),
            "空标题不应注入空串 alias，md=\n{md}"
        );
    }

    /// vault 去重改名（`Dup` → `Dup-2.md`）后，被改名的那篇 .md 的 aliases 应含原标题 `Dup`，
    /// 且能被 parse_md（真实 YAML 规则）读回。首篇（文件名 == 原标题）不注入。
    #[test]
    fn vault_dedup_injects_original_title_alias() {
        let mut g = Graph::default();
        g.add_node(node(1, 0.0, 0.0, "Dup", "first", &[]));
        g.add_node(node(2, 0.0, 0.0, "Dup", "second", &[]));
        let files = vault_files(&g);

        let first = files.iter().find(|f| f.filename == "Dup.md").unwrap();
        let renamed = files.iter().find(|f| f.filename == "Dup-2.md").unwrap();

        let (_, _, first_aliases, _) = parse_md(std::str::from_utf8(&first.bytes).unwrap());
        assert!(
            first_aliases.is_empty(),
            "文件名 == 原标题的首篇不注入 alias"
        );

        let (_, _, renamed_aliases, _) = parse_md(std::str::from_utf8(&renamed.bytes).unwrap());
        assert_eq!(
            renamed_aliases,
            vec!["Dup".to_string()],
            "被去重改名的篇应注入原标题 Dup 为 alias"
        );
    }

    // ---- 手画边旁路 ----

    #[test]
    fn collect_manual_edges_excludes_wiki() {
        use crate::graph::edge::Edge;
        use crate::resource::CanvasStateResource;
        let mut g = Graph::default();
        let a = g.add_node(node(1, 0.0, 0.0, "A", "", &[]));
        let b = g.add_node(node(2, 0.0, 0.0, "B", "", &[]));
        let c = g.add_node(node(3, 0.0, 0.0, "C", "", &[]));
        let cs = CanvasStateResource::default();
        // a->b 手画，b->c wiki。
        g.add_edge(Edge::new(
            a,
            b,
            egui::Pos2::ZERO,
            egui::Pos2::ZERO,
            cs.clone(),
        ));
        g.add_edge(Edge::new_wiki(
            b,
            c,
            egui::Pos2::ZERO,
            egui::Pos2::ZERO,
            cs.clone(),
        ));
        let edges = collect_manual_edges(&g);
        assert_eq!(edges.len(), 1, "只导出手画边");
        assert_eq!(edges[0].source_title, "A");
        assert_eq!(edges[0].target_title, "B");
    }

    /// 必修项 3：端点标题为空的手画边无法被 find_by_title 寻址 → 跳过（不写空串引用），
    /// 有标题端点的手画边正常导出。
    #[test]
    fn collect_manual_edges_skips_empty_title_endpoint() {
        use crate::graph::edge::Edge;
        use crate::resource::CanvasStateResource;
        let mut g = Graph::default();
        let a = g.add_node(node(1, 0.0, 0.0, "A", "", &[]));
        let empty = g.add_node(node(2, 0.0, 0.0, "", "", &[])); // 空标题端点
        let b = g.add_node(node(3, 0.0, 0.0, "B", "", &[]));
        let cs = CanvasStateResource::default();
        // A->empty（含空标题端点，应跳过）、A->B（正常导出）。
        g.add_edge(Edge::new(
            a,
            empty,
            egui::Pos2::ZERO,
            egui::Pos2::ZERO,
            cs.clone(),
        ));
        g.add_edge(Edge::new(
            a,
            b,
            egui::Pos2::ZERO,
            egui::Pos2::ZERO,
            cs.clone(),
        ));
        let edges = collect_manual_edges(&g);
        assert_eq!(edges.len(), 1, "空标题端点的边被跳过，只剩 A->B");
        assert_eq!(edges[0].source_title, "A");
        assert_eq!(edges[0].target_title, "B");
        // 旁路里不应出现任何空串引用。
        assert!(
            edges
                .iter()
                .all(|e| !e.source_title.is_empty() && !e.target_title.is_empty()),
            "导出的边端点引用永不为空串"
        );
    }

    #[test]
    fn manual_edges_json_roundtrips() {
        let edges = vec![ManualEdgeRef {
            source_title: "源".to_owned(),
            target_title: "目标".to_owned(),
        }];
        let bytes = manual_edges_json(&edges).unwrap();
        let back: Vec<ManualEdgeRef> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, edges);
    }

    #[test]
    fn vault_includes_manual_edges_sidecar_only_when_present() {
        // 无边：不生成旁路文件。
        let mut g = Graph::default();
        g.add_node(node(1, 0.0, 0.0, "A", "", &[]));
        let files = vault_files(&g);
        assert!(!files.iter().any(|f| f.filename == MANUAL_EDGES_FILENAME));

        // 加一条手画边：生成旁路文件。
        use crate::graph::edge::Edge;
        use crate::resource::CanvasStateResource;
        let b = g.add_node(node(2, 0.0, 0.0, "B", "", &[]));
        let a = g.graph.node_indices().next().unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::Pos2::ZERO,
            egui::Pos2::ZERO,
            CanvasStateResource::default(),
        ));
        let files = vault_files(&g);
        assert!(files.iter().any(|f| f.filename == MANUAL_EDGES_FILENAME));
    }

    // ---- store-only ZIP 字节结构 ----

    #[test]
    fn crc32_matches_known_vector() {
        // 标准测试向量："123456789" 的 CRC-32 = 0xCBF43926。
        assert_eq!(store_zip::crc32(b"123456789"), 0xCBF4_3926);
        // 空输入 CRC = 0。
        assert_eq!(store_zip::crc32(b""), 0);
    }

    #[test]
    fn zip_has_valid_signatures_and_eocd() {
        let entries = [
            store_zip::Entry {
                name: "a.md",
                data: b"hello",
            },
            store_zip::Entry {
                name: "b.md",
                data: b"world!!",
            },
        ];
        let zip = store_zip::build(&entries);

        // 起头是 local file header 签名 PK\x03\x04（小端 0x04034b50）。
        assert_eq!(
            &zip[0..4],
            &[0x50, 0x4b, 0x03, 0x04],
            "首个 local header 签名"
        );

        // 末尾 22 字节是 EOCD（无注释）：签名 PK\x05\x06 + 记录数等。
        let eocd = &zip[zip.len() - 22..];
        assert_eq!(&eocd[0..4], &[0x50, 0x4b, 0x05, 0x06], "EOCD 签名");
        // total central dir records（偏移 10..12，小端 u16）== 2。
        let total = u16::from_le_bytes([eocd[10], eocd[11]]);
        assert_eq!(total, 2, "EOCD 应记录两个条目");

        // central directory header 签名 PK\x01\x02 出现在流里（每条目一个 → 2 个）。
        let cd_sig = [0x50, 0x4b, 0x01, 0x02];
        let count = zip.windows(4).filter(|w| *w == cd_sig).count();
        assert_eq!(count, 2, "应有两条 central directory header");
    }

    #[test]
    fn zip_stores_file_bytes_uncompressed() {
        // store-only：原始字节应原样出现在 zip 流里（无压缩）。
        let data = b"verbatim payload bytes";
        let zip = store_zip::build(&[store_zip::Entry { name: "f.md", data }]);
        let found = zip.windows(data.len()).any(|w| w == data);
        assert!(found, "stored 模式下原始字节应原样可见");
        // CRC 字段（local header 偏移 14..18）应等于 crc32(data)。
        let crc = u32::from_le_bytes([zip[14], zip[15], zip[16], zip[17]]);
        assert_eq!(crc, store_zip::crc32(data), "local header CRC 应匹配");
    }

    /// 可选 nit：local file header 的 last-mod-date（偏移 12..14）写最小合法 DOS 日期 1980-01-01
    /// （`0x0021`），而非非法的 0/0（1980-00-00）。time（偏移 10..12）仍为 0（不暴露导出时刻）。
    #[test]
    fn zip_uses_minimum_valid_dos_date() {
        let zip = store_zip::build(&[store_zip::Entry {
            name: "f.md",
            data: b"x",
        }]);
        let time = u16::from_le_bytes([zip[10], zip[11]]);
        let date = u16::from_le_bytes([zip[12], zip[13]]);
        assert_eq!(time, 0, "时间字段保持 0");
        assert_eq!(date, 0x0021, "日期字段为最小合法 DOS 日期 1980-01-01");
        // 解码校验：month/day 均 ≥ 1（非法的 0/0 会被严格读取器告警）。
        let day = date & 0x1F;
        let month = (date >> 5) & 0x0F;
        let year = 1980 + (date >> 9);
        assert_eq!((year, month, day), (1980, 1, 1));
    }

    #[test]
    fn vault_zip_packs_all_files() {
        let mut g = Graph::default();
        g.add_node(node(1, 0.0, 0.0, "A", "body A", &[]));
        g.add_node(node(2, 0.0, 0.0, "B", "body B", &[]));
        let files = vault_files(&g);
        let zip = vault_zip(&files);
        // 两个 .md → 两条 central directory header。
        let cd_sig = [0x50, 0x4b, 0x01, 0x02];
        let count = zip.windows(4).filter(|w| *w == cd_sig).count();
        assert_eq!(count, files.len());
    }

    // ============================ #19 导入：markdown_to_node + 导入流水线 ============================

    // ---- parse_frontmatter / markdown_to_node 与 #18 导出往返 ----

    /// #18 导出 → #19 导入往返：text 经文件名 / note / position / aliases 保真。
    #[test]
    fn import_roundtrips_node_to_markdown() {
        let n = node(99, 3.5, -2.0, "标题", "正文 [[链接]] 内容", &["别名"]);
        let md = md_no_rename(&n); // stem == text，未改名口径
                                   // 导出文件名 = sanitize(text) + .md；未含非法字符故 == text。
        let filename = format!("{}.md", sanitize_filename(&n.text, n.id));
        // 导入侧重分配 id（不复用 frontmatter 的 99）；position 缺省占位仅在 frontmatter 无坐标时取用。
        let imported = markdown_to_node(&filename, &md, 7, egui::pos2(1000.0, 1000.0));
        assert_eq!(
            imported.id, 7,
            "id 由调用方重分配，不复用 frontmatter 的 99"
        );
        assert_eq!(imported.text, "标题", "text 经文件名往返");
        assert_eq!(imported.note, "正文 [[链接]] 内容", "note 经正文往返保真");
        assert_eq!(
            imported.position,
            egui::pos2(3.5, -2.0),
            "position 经 frontmatter 恢复"
        );
        assert_eq!(
            imported.aliases,
            vec!["别名".to_string()],
            "aliases 经 frontmatter 恢复"
        );
    }

    /// 标题含非法字符被 #18 sanitize 改名（`a:b` → 文件名 `a_b`，原标题注入 aliases）：
    /// 导入后 text 变成 `a_b`（标题字节不保真），但原标题 `a:b` 在 aliases 里 → `[[a:b]]` 仍可寻址（链接保真）。
    #[test]
    fn import_rename_keeps_original_title_addressable_via_alias() {
        let n = node(1, 0.0, 0.0, "a:b", "body", &[]);
        let stem = sanitize_filename(&n.text, n.id); // "a_b"
        let md = node_to_markdown(&n, &stem);
        let filename = format!("{stem}.md");
        let imported = markdown_to_node(&filename, &md, 5, egui::Pos2::ZERO);
        assert_eq!(
            imported.text, "a_b",
            "改名后 text = sanitize 文件名（标题字节不保真）"
        );
        assert!(
            imported.aliases.contains(&"a:b".to_string()),
            "原标题注入 aliases，[[a:b]] 经别名仍可寻址（链接保真）"
        );
    }

    /// 无 frontmatter 的外部 Obsidian `.md`：正文 = 整个 content、aliases 空、position 用占位 fallback。
    #[test]
    fn import_external_md_without_frontmatter() {
        let content = "# 标题\n\n正文里有 [[别的笔记]] 链接。\n第二段。";
        let imported = markdown_to_node("我的笔记.md", content, 3, egui::pos2(42.0, 7.0));
        assert_eq!(imported.text, "我的笔记", "text 取文件名去 .md");
        assert_eq!(
            imported.note, content,
            "无 frontmatter → 正文 = 整个 content"
        );
        assert!(imported.aliases.is_empty(), "外部 md 无 aliases");
        assert_eq!(
            imported.position,
            egui::pos2(42.0, 7.0),
            "缺坐标 → 用占位 fallback"
        );
    }

    /// 坏 frontmatter 容错：起始 `---` 但 position / aliases 行畸形 → 不 panic、字段落默认、当纯正文不丢。
    #[test]
    fn import_malformed_frontmatter_is_lenient() {
        // 有起始 `---` 但无闭合 `---`：整段当正文。
        let no_close = "---\nid: 1\nposition broken\n正文继续";
        let n1 = markdown_to_node("a.md", no_close, 1, egui::pos2(5.0, 5.0));
        assert_eq!(n1.note, no_close, "无闭合 frontmatter → 整段当正文（不吞）");
        assert_eq!(n1.position, egui::pos2(5.0, 5.0), "无有效坐标 → 占位");

        // 闭合 frontmatter 但 position / aliases 畸形：解析回退默认，不 panic。
        let bad_fields = "---\nposition: [not, numbers]\naliases: [unterminated\n---\n\n正文";
        let n2 = markdown_to_node("b.md", bad_fields, 2, egui::pos2(9.0, 9.0));
        assert_eq!(
            n2.position,
            egui::pos2(9.0, 9.0),
            "坏 position 行 → 占位坐标"
        );
        assert_eq!(n2.note, "正文", "正文仍被正确拆出");
    }

    /// 中文标题 / 正文 / 别名整链路往返（含 frontmatter 引号化中文别名读回）。
    #[test]
    fn import_roundtrips_chinese() {
        let n = node(
            1,
            12.0,
            -7.0,
            "知识图谱",
            "关联 [[第二大脑]] 与 [[ML]]",
            &["KG", "图谱"],
        );
        let md = md_no_rename(&n);
        let filename = format!("{}.md", sanitize_filename(&n.text, n.id));
        let imported = markdown_to_node(&filename, &md, 9, egui::Pos2::ZERO);
        assert_eq!(imported.text, "知识图谱");
        assert_eq!(imported.note, "关联 [[第二大脑]] 与 [[ML]]");
        assert_eq!(imported.aliases, vec!["KG".to_string(), "图谱".to_string()]);
        assert_eq!(imported.position, egui::pos2(12.0, -7.0));
    }

    /// 文件名可带路径前缀（vault 子目录上传）与大小写 `.MD` 扩展：均取末段、去扩展为标题。
    #[test]
    fn import_filename_strips_path_and_extension() {
        let n = markdown_to_node("sub/dir/Note.MD", "body", 1, egui::Pos2::ZERO);
        assert_eq!(n.text, "Note", "取末段 + 去大小写 .MD 扩展");
        let n2 = markdown_to_node("plain", "body", 1, egui::Pos2::ZERO);
        assert_eq!(n2.text, "plain", "无扩展 → 原样为标题");
    }

    // ---- import_markdown_batch：顺序敏感（先全建点再 resolve）+ 旁路 + 容错 ----

    fn fresh() -> (Graph, CanvasStateResource) {
        (Graph::default(), CanvasStateResource::default())
    }

    /// 一篇导出的 .md（text=A，正文 [[B]]）+ 一篇 text=B：先全建点再 resolve 后，A->B 连成 wiki 边，
    /// **不**为 [[B]] 重复建第二个 B（顺序铁律的核心保证）。
    #[test]
    fn import_batch_links_within_batch_no_duplicate_node() {
        let (mut g, canvas) = fresh();
        let files = vec![
            (
                "A.md".to_owned(),
                "---\nposition: [0, 0]\n---\n\n指向 [[B]]".to_owned(),
            ),
            (
                "B.md".to_owned(),
                "---\nposition: [100, 0]\n---\n\nB 的正文".to_owned(),
            ),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.imported_nodes, 2, "两篇 .md → 两个节点");
        assert_eq!(
            out.resolved_new_nodes, 0,
            "[[B]] 命中本批已建的 B，不重复建点"
        );
        assert_eq!(out.resolved_edges, 1, "A->B 一条 wiki 边");
        assert_eq!(g.graph.node_count(), 2, "全图恰好两个节点（无重复）");
        let a = crate::wikilink::find_by_title(&g, "A").unwrap();
        let b = crate::wikilink::find_by_title(&g, "B").unwrap();
        assert!(g.edge_exists(a, b), "A->B wiki 边存在");
    }

    /// 反序若先 resolve B 再建 A 不会发生（流水线强制先全建点）：即便 [[目标]] 在批内靠后，也命中不新建。
    #[test]
    fn import_batch_order_independent_of_file_order() {
        let (mut g, canvas) = fresh();
        // 把"引用方"放前、"被引用方"放后：先全建点保证 resolve 时 B 已在图中。
        let files = vec![
            ("A.md".to_owned(), "指向 [[B]]".to_owned()),
            ("B.md".to_owned(), "body".to_owned()),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.resolved_new_nodes, 0, "顺序无关：B 已先建，不重复");
        assert_eq!(g.graph.node_count(), 2);
    }

    /// 外部 md 引用一个**批内没有**的标题：resolve 阶段自动建缺失目标（与手动编辑正文同一路径）。
    #[test]
    fn import_batch_resolve_creates_missing_target() {
        let (mut g, canvas) = fresh();
        let files = vec![("A.md".to_owned(), "指向 [[不存在的]]".to_owned())];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.imported_nodes, 1);
        assert_eq!(out.resolved_new_nodes, 1, "缺失目标自动建");
        assert_eq!(out.resolved_edges, 1);
        assert!(crate::wikilink::find_by_title(&g, "不存在的").is_some());
    }

    /// 手画边旁路 JSON：按端点标题连 Manual 边。
    #[test]
    fn import_batch_connects_manual_edges_sidecar() {
        let (mut g, canvas) = fresh();
        let sidecar = manual_edges_json(&[ManualEdgeRef {
            source_title: "A".to_owned(),
            target_title: "B".to_owned(),
        }])
        .unwrap();
        let files = vec![
            ("A.md".to_owned(), "body A".to_owned()),
            ("B.md".to_owned(), "body B".to_owned()),
            (
                MANUAL_EDGES_FILENAME.to_owned(),
                String::from_utf8(sidecar).unwrap(),
            ),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.imported_nodes, 2, "旁路 json 不算节点");
        assert_eq!(out.manual_edges, 1, "旁路连一条 Manual 边");
        let a = crate::wikilink::find_by_title(&g, "A").unwrap();
        let b = crate::wikilink::find_by_title(&g, "B").unwrap();
        assert!(g.edge_exists(a, b));
        // 该边是 Manual（不会被后续 resolve 误删）：在 a 的出边里找到指向 b 的那条，断言其 origin。
        let origin = g
            .graph
            .edges_directed(a, petgraph::Direction::Outgoing)
            .find(|e| e.target() == b)
            .map(|e| e.weight().origin);
        assert_eq!(origin, Some(EdgeOrigin::Manual), "旁路边是 Manual");
    }

    /// 坏旁路 JSON 不中断整批：.md 照常导入，旁路解析失败仅跳过（manual_edges=0）。
    #[test]
    fn import_batch_bad_sidecar_does_not_abort() {
        let (mut g, canvas) = fresh();
        let files = vec![
            ("A.md".to_owned(), "body".to_owned()),
            (
                MANUAL_EDGES_FILENAME.to_owned(),
                "{ not valid json".to_owned(),
            ),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.imported_nodes, 1, "坏旁路不影响 .md 导入");
        assert_eq!(out.manual_edges, 0, "坏 json 跳过");
    }

    /// 旁路端点标题在图中找不到：跳过该条、不中断（manual_edges 不计该条）。
    #[test]
    fn import_batch_sidecar_unknown_endpoint_skipped() {
        let (mut g, canvas) = fresh();
        let sidecar = manual_edges_json(&[ManualEdgeRef {
            source_title: "A".to_owned(),
            target_title: "幽灵节点".to_owned(),
        }])
        .unwrap();
        let files = vec![
            ("A.md".to_owned(), "body".to_owned()),
            (
                MANUAL_EDGES_FILENAME.to_owned(),
                String::from_utf8(sidecar).unwrap(),
            ),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.manual_edges, 0, "端点找不到 → 跳过该条");
    }

    /// 全部 frontmatter 缺坐标 → 全用螺旋占位、彼此不重合，placeheld_positions 计数等于节点数。
    #[test]
    fn import_batch_placeholder_positions_distinct() {
        let (mut g, canvas) = fresh();
        let files: Vec<(String, String)> = (0..5)
            .map(|i| (format!("N{i}.md"), format!("body {i}")))
            .collect();
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.placeheld_positions, 5, "5 篇全缺坐标 → 5 个占位");
        // 占位坐标两两不重合（spiral_placeholder 确定性散布）。
        let positions: Vec<egui::Pos2> = g.graph.node_weights().map(|n| n.position).collect();
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                assert_ne!(positions[i], positions[j], "占位坐标应两两不同");
            }
        }
    }

    /// 有坐标的节点不占用螺旋占位序号（placeheld_positions 只数缺坐标的）。
    #[test]
    fn import_batch_counts_only_missing_positions() {
        let (mut g, canvas) = fresh();
        let files = vec![
            (
                "有坐标.md".to_owned(),
                "---\nposition: [3, 4]\n---\n\nbody".to_owned(),
            ),
            ("无坐标.md".to_owned(), "body".to_owned()),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.placeheld_positions, 1, "只有 1 篇缺坐标");
        let with = crate::wikilink::find_by_title(&g, "有坐标").unwrap();
        assert_eq!(
            g.get_node(with).unwrap().position,
            egui::pos2(3.0, 4.0),
            "有坐标的用 frontmatter 坐标"
        );
    }

    /// 导入进**既有图**：与现有同名节点复用（不重复建），id 不与现有撞（由调用方 new_node_id 保证）。
    #[test]
    fn import_batch_into_existing_graph_reuses_existing_title() {
        let (mut g, canvas) = fresh();
        // 既有图已有 B。
        let existing_b = {
            let id = canvas.read_resource(|c| c.new_node_id());
            g.add_node(node(id, 0.0, 0.0, "B", "既有 B", &[]))
        };
        // 导入 A，正文 [[B]] → 应复用既有 B、不新建。
        let files = vec![("A.md".to_owned(), "指向 [[B]]".to_owned())];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.resolved_new_nodes, 0, "复用既有 B");
        assert_eq!(g.graph.node_count(), 2, "A + 既有 B，无重复");
        let a = crate::wikilink::find_by_title(&g, "A").unwrap();
        assert!(g.edge_exists(a, existing_b), "A->既有B");
    }

    /// 空批次：无操作（节点/边均不变），所有计数为 0。
    #[test]
    fn import_batch_empty_is_noop() {
        let (mut g, canvas) = fresh();
        let out = import_markdown_batch(&mut g, &canvas, &[]);
        assert_eq!(out, ImportOutcome::default());
        assert_eq!(g.graph.node_count(), 0);
    }

    // ---- 必修项 3：frontmatter CRLF 容错（真实 Obsidian / Windows / git autocrlf 来源） ----

    /// CRLF（`---\r\n…---\r\n`）frontmatter：position / aliases 必须正确解析（不被当正文吞掉），
    /// 正文 body 逐字节原样（含其自身的 CRLF 行尾，SSOT §7 不归一）。
    #[test]
    fn parse_frontmatter_tolerates_crlf() {
        // 真实 Windows / Obsidian 行尾：每行以 \r\n 结束，含 frontmatter 与正文。
        let content =
            "---\r\nid: 42\r\nposition: [12.5, -7]\r\naliases: [\"别名一\", alias2]\r\n---\r\n\r\n正文第一行\r\n第二行";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(
            fm.position,
            Some(egui::pos2(12.5, -7.0)),
            "CRLF frontmatter 的 position 应正确解析（不被静默丢失）"
        );
        assert_eq!(
            fm.aliases,
            vec!["别名一".to_string(), "alias2".to_string()],
            "CRLF frontmatter 的 aliases 应正确解析"
        );
        // 正文逐字节原样：保留 CRLF 行尾、不归一（note = SSOT）。
        assert_eq!(
            body, "正文第一行\r\n第二行",
            "CRLF 正文逐字节原样（含 \\r\\n，仅剥 frontmatter 后一个空行）"
        );
    }

    /// CRLF 外部 md（无 frontmatter）：整段当正文、不丢，aliases 空、position 用占位 fallback。
    #[test]
    fn markdown_to_node_crlf_external_md() {
        let content = "# 标题\r\n\r\n正文 [[别的笔记]]\r\n第二段";
        let imported = markdown_to_node("CRLF笔记.md", content, 3, egui::pos2(42.0, 7.0));
        assert_eq!(imported.text, "CRLF笔记");
        assert_eq!(
            imported.note, content,
            "无 frontmatter 的 CRLF md → 正文 = 整个 content（逐字节原样）"
        );
        assert!(imported.aliases.is_empty());
        assert_eq!(imported.position, egui::pos2(42.0, 7.0), "缺坐标 → 占位");
    }

    /// CRLF frontmatter 经 markdown_to_node 端到端：position / aliases 进 Node，body 原样。
    #[test]
    fn markdown_to_node_crlf_frontmatter_parses() {
        let content =
            "---\r\nposition: [3, 4]\r\naliases: [\"KG\"]\r\n---\r\n\r\n关联 [[第二大脑]]\r\nbody";
        let n = markdown_to_node("知识图谱.md", content, 9, egui::Pos2::ZERO);
        assert_eq!(n.position, egui::pos2(3.0, 4.0), "CRLF position 进 Node");
        assert_eq!(n.aliases, vec!["KG".to_string()], "CRLF aliases 进 Node");
        assert_eq!(n.note, "关联 [[第二大脑]]\r\nbody", "CRLF body 逐字节原样");
    }

    // ---- 必修项 2：append 语义钉死（导入永远新增、绝不合并；重名告警） ----

    /// ①导入进**空图** → 节点 / 边正确（往返），无重名碰撞。
    #[test]
    fn import_into_empty_graph_no_collision_roundtrip() {
        let (mut g, canvas) = fresh();
        let files = vec![
            (
                "A.md".to_owned(),
                "---\nposition: [0, 0]\n---\n\n指向 [[B]]".to_owned(),
            ),
            (
                "B.md".to_owned(),
                "---\nposition: [100, 0]\n---\n\nB 正文".to_owned(),
            ),
        ];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.imported_nodes, 2);
        assert_eq!(out.renamed_collisions, 0, "空图导入无既有同名节点");
        assert_eq!(g.graph.node_count(), 2, "恰好两个节点");
        let a = crate::wikilink::find_by_title(&g, "A").unwrap();
        let b = crate::wikilink::find_by_title(&g, "B").unwrap();
        assert!(g.edge_exists(a, b), "A->B wiki 边");
    }

    /// ②导入进**非空图（已含同名节点）** → **append**：节点数增加、不合并，且 renamed_collisions 计数命中。
    ///
    /// 这是"导出→编辑→再导入"会静默翻倍的语义钉死：导入一个标题为 `A` 的 `.md` 进已有 `A` 的图，
    /// 不覆盖既有 A，而是**新增**第二个标题 `A` 的节点（与 #18 vault 去重导出语义对偶——往返靠先 New）。
    #[test]
    fn import_into_nonempty_graph_appends_and_warns_on_collision() {
        let (mut g, canvas) = fresh();
        // 既有图已有标题 A（手摆位置 (500, 500)、自己的 note）。
        let existing_a = {
            let id = canvas.read_resource(|c| c.new_node_id());
            g.add_node(node(id, 500.0, 500.0, "A", "既有 A 的 note", &[]))
        };
        let before_count = g.graph.node_count();

        // 导入一篇标题也叫 A 的 .md（带自己的坐标 / note）。
        let files = vec![(
            "A.md".to_owned(),
            "---\nposition: [10, 20]\n---\n\n导入版 A 的 note".to_owned(),
        )];
        let out = import_markdown_batch(&mut g, &canvas, &files);

        // append：新增一个节点（不合并、不覆盖既有 A）。
        assert_eq!(out.imported_nodes, 1);
        assert_eq!(
            out.renamed_collisions, 1,
            "导入节点标题与既有 A 同名 → 命中重名告警计数"
        );
        assert_eq!(
            g.graph.node_count(),
            before_count + 1,
            "append：节点数 +1（绝不合并/覆盖既有同名节点）"
        );

        // 既有 A 的位置 / note 纹丝不动（非破坏）。
        let existing = g.get_node(existing_a).unwrap();
        assert_eq!(
            existing.position,
            egui::pos2(500.0, 500.0),
            "既有 A 位置不动"
        );
        assert_eq!(existing.note, "既有 A 的 note", "既有 A 的 note 不被覆盖");

        // 图中现有两个标题为 A 的节点（既有 + 导入），导入版用自己的坐标。
        let titled_a: Vec<_> = g
            .graph
            .node_indices()
            .filter(|&i| g.graph[i].text == "A")
            .collect();
        assert_eq!(titled_a.len(), 2, "两个同名 A 节点共存（append）");
        assert!(
            titled_a
                .iter()
                .any(|&i| g.graph[i].position == egui::pos2(10.0, 20.0)),
            "导入版 A 用 frontmatter 坐标 (10, 20)"
        );
    }

    /// 导入进非空图但**无同名**：renamed_collisions = 0（重名告警只在真撞名时触发）。
    #[test]
    fn import_into_nonempty_graph_distinct_titles_no_collision() {
        let (mut g, canvas) = fresh();
        let _existing = {
            let id = canvas.read_resource(|c| c.new_node_id());
            g.add_node(node(id, 0.0, 0.0, "既有", "note", &[]))
        };
        let files = vec![("全新标题.md".to_owned(), "body".to_owned())];
        let out = import_markdown_batch(&mut g, &canvas, &files);
        assert_eq!(out.renamed_collisions, 0, "无同名 → 不告警");
        assert_eq!(g.graph.node_count(), 2);
    }
}
