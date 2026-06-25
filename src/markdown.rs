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

    /// 解析一个 YAML flow 序列 `[...]` 的内部（不含中括号）成元素列表（**测试用**）。
    ///
    /// 必修项 4：旧实现以裸 `, ` 硬切，遇到含逗号的引号化元素（`"a, b"` 或注入了 `: # ,` 的原标题
    /// 别名）会切错、削弱往返保真测试的可信度。这里按 YAML 双引号字符串规则正确地状态机扫描：
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

    /// 一个极小的 frontmatter 解析器（仅测试用）：拆出 `---` 块与正文，回读 id/position/aliases，
    /// 验证 node_to_markdown 的输出可被结构化读回（往返保真）。aliases 经 [`parse_flow_seq`]
    /// 正确处理含逗号 / 转义的引号化元素（必修项 4）。
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
}
