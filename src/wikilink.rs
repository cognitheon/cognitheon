//! Wikilink / 双链引擎（模型层，纯逻辑、不依赖 GUI，可无头测试）。
//!
//! "第二大脑"的核心差异化：在节点正文里写 `[[标题]]` 即自动在图上长出连接——
//! 找到同名节点就连过去，没有就新建一个再连。配套提供反向链接（谁链接到我）与全文搜索。
//!
//! 这一层只操作 [`Graph`] 数据与 id 分配（经 [`CanvasStateResource`]），渲染/交互留给 UI 层。

use std::collections::{HashSet, VecDeque};

use petgraph::graph::NodeIndex;

use crate::graph::edge::Edge;
use crate::graph::graph_impl::Graph;
use crate::graph::node::Node;
use crate::resource::CanvasStateResource;

/// 从文本中按出现顺序提取所有 `[[标题]]` 引用（去重、去首尾空白、跳过空标题）。
///
/// 以 `&str::find` 在 UTF-8 上扫描 `[[`/`]]`（均为 ASCII，不会切到中文多字节中间），故对中文标题安全。
pub fn parse_links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        match after.find("]]") {
            Some(end) => {
                let title = after[..end].trim();
                // 跳过空标题与嵌套残留（如 `[[a[[b]]]]` 会提取出 "a[[b"）
                if !title.is_empty() && !title.contains("[[") && !out.iter().any(|t| t == title) {
                    out.push(title.to_owned());
                }
                rest = &after[end + 2..];
            }
            // 有 `[[` 但没有闭合 `]]` —— 停止扫描
            None => break,
        }
    }
    out
}

/// 判定字符是否能构成标签（`#tag`）正文：字母 / 数字 / CJK / `_` / `-` / `/`（层级分隔）。
///
/// 与 markdown 标题 `# ` 的判据**天然互斥**：标题要求 `#` 后紧跟空格（见 `md_highlight::heading_level`），
/// 而本谓词对空白返回 `false`，故 `# 标题` 不会被当成标签、`#tag` 才是标签。
/// `/` 入集是为了 `#a/b` 这类层级标签（同 Obsidian/Logseq）；中文等非 ASCII 字母靠 `is_alphabetic` 纳入。
fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/'
}

/// 从文本中按出现顺序提取所有 `#标签` 引用（去重、保留出现顺序）。
///
/// **v1：标签不进图**——仅作正文派生的检索维度（搜索 / 面板 / 编辑态高亮实时 parse），不建节点/边、
/// 不改 `Node`/`Edge`/序列化。
///
/// 识别规则（与 markdown 标题消歧、对中文安全）：
/// - `#` **紧跟标签字符**（[`is_tag_char`]：字母 / 数字 / CJK / `_` / `-` / `/`）= 标签起点；
///   `#` 后是**空白**则是 markdown 标题（`# 标题`），**不**当标签——与 `md_highlight::heading_level`
///   的「`#` 后跟空格」判据互斥、保持一致。
/// - 标签正文一直吃到**首个非标签字符**（空白或标点）为止；故 `#知识图谱，` 提取出 `知识图谱`
///   （中文逗号是终止边界）、`#rust.` 提取出 `rust`。
/// - 连续多个 `#`（如 `##`、`###`）：`#` 不是标签字符，故 `##tag` 的首个 `#` 后紧跟 `#`（非标签字符），
///   不构成标签；`# ` / `## ` 是标题，也不构成。`#` 单独出现（后接空白/EOF/标点）不构成标签。
/// - 去重并保留首次出现顺序（同 [`parse_links`]）。
///
/// 经 `char_indices` 在 UTF-8 上扫描（`#` 是 ASCII、不会切进多字节中段），对中文标签安全。
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut iter = text.char_indices().peekable();
    while let Some((i, ch)) = iter.next() {
        if ch != '#' {
            continue;
        }
        // `#` 后必须紧跟标签字符才算标签起点（否则是标题 `# `、连续 `##`、或孤立 `#`）。
        let tag_start = i + 1; // `#` 是 ASCII，占 1 字节
        match iter.peek() {
            Some(&(_, next)) if is_tag_char(next) => {}
            _ => continue,
        }
        // 从 tag_start 吃到首个非标签字符为止。逐字符推进 iter，使外层循环从标签末尾继续，
        // 避免把标签正文里的字符（如 `a/b` 的 `/`）误当新一轮扫描起点。
        let mut tag_end = tag_start;
        while let Some(&(j, c)) = iter.peek() {
            if is_tag_char(c) {
                tag_end = j + c.len_utf8();
                iter.next();
            } else {
                break;
            }
        }
        // tag_start / tag_end 均来自 char 边界（`#` 后一字节 + char_indices 偏移），切片安全。
        let tag = std::str::from_utf8(&bytes[tag_start..tag_end])
            .expect("tag 切片落在 char 边界")
            .to_owned();
        if !out.iter().any(|t| t == &tag) {
            out.push(tag);
        }
    }
    out
}

/// 列出正文里含 `#tag` 标签的所有节点（标签精确相等，**大小写敏感**，按节点索引顺序）。
///
/// **v1 标签检索维度**（标签不进图）：遍历节点、对其 `note` 实时 [`parse_tags`]，含 `tag` 即命中。
/// `tag` 传入时去掉前导 `#`（调用方若带 `#` 需自行 `trim_start_matches('#')`）。纯函数、可无头单测。
/// 大小写敏感（同 Obsidian 标签默认；标题/正文的子串搜索 [`search`] 才大小写不敏感）。
pub fn nodes_with_tag(graph: &Graph, tag: &str) -> Vec<NodeIndex> {
    let tag = tag.trim_start_matches('#');
    if tag.is_empty() {
        return Vec::new();
    }
    graph
        .graph
        .node_indices()
        .filter(|&i| parse_tags(&graph.graph[i].note).iter().any(|t| t == tag))
        .collect()
}

/// 节点是否以 `title` 命名：标题精确相等，**或**别名集里有精确相等项。
///
/// 别名（`Node.aliases`）与 `text` 一视同仁地参与寻址（PKM aliases，同 Obsidian）。
/// 这是别名寻址的**唯一判定点**——`find_by_title` / `resolve_links` 的目标匹配 /
/// `backlinks_with_context` 全部经它，单点改造即全链路一致。
fn node_matches_title(node: &Node, title: &str) -> bool {
    node.text == title || node.aliases.iter().any(|a| a == title)
}

/// 按标题查找第一个匹配的节点：**text 优先、alias 次之**。
///
/// 先扫 `text == title` 的首个；无则扫别名命中（`aliases.contains(title)`）的首个。
/// 保持"取第一个"的歧义语义（多个同名/同别名只连第一个）。寻址在此收口——
/// resolve_links / 读模式跳转 / 自动补全 / 反向链接全经它，故别名全链路生效。
pub fn find_by_title(graph: &Graph, title: &str) -> Option<NodeIndex> {
    graph
        .graph
        .node_indices()
        .find(|&i| graph.graph[i].text == title)
        .or_else(|| {
            graph
                .graph
                .node_indices()
                .find(|&i| graph.graph[i].aliases.iter().any(|a| a == title))
        })
}

/// 按标题查找**所有**匹配的节点（`find_by_title` 取第一个的全量版本），用于歧义消歧 UI。
///
/// 匹配口径与 [`find_by_title`] / 歧义统计完全一致：**text 命中 ∪ alias 命中**。
/// 排序保证"第一个"语义一致——**text 命中在前、仅 alias 命中在后**，各组内按节点索引升序；
/// 同一节点既 text 命中又 alias 命中只计一次（归入 text 组、不重复）。故返回的 `[0]` 与
/// `find_by_title` 选出的"已连接目标"恒为同一节点。
///
/// 返回 `Vec` 长度即歧义计数：`0` = 未创建、`1` = 唯一无歧义、`> 1` = 同名歧义（面板据此提示）。
pub fn find_all_by_title(graph: &Graph, title: &str) -> Vec<NodeIndex> {
    let mut out: Vec<NodeIndex> = Vec::new();
    // 先收 text 精确命中（与 find_by_title 的优先级一致），保持索引升序。
    for i in graph.graph.node_indices() {
        if graph.graph[i].text == title {
            out.push(i);
        }
    }
    // 再收"仅 alias 命中、且未在 text 组里"的节点，追加在后。
    for i in graph.graph.node_indices() {
        if graph.graph[i].text != title && graph.graph[i].aliases.iter().any(|a| a == title) {
            out.push(i);
        }
    }
    out
}

/// [`resolve_links`] 的结果：本次新建的节点与新建的边数。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolveOutcome {
    pub created_nodes: Vec<NodeIndex>,
    pub created_edges: usize,
    /// 正文里出现、但图中存在多个同名节点的标题（连到了第一个，其余被忽略）——歧义警示。
    pub ambiguous: Vec<String>,
}

/// 把 `source` 节点正文里的 `[[标题]]` 幂等投影到图上（wiki 自动边 = note 的投影）。
///
/// 算法是一次**差量同步**（而非粗暴的"全删全建"，以保持未变链接的 `EdgeIndex` 稳定）：
/// 1. 解析正文 `[[标题]]` → 解析出每个目标节点（同名则复用、缺失则在 source 附近新建）；
/// 2. **删除过时**：删掉 `source` 发出、但目标已不在当前期望集里的 wiki 边（`EdgeOrigin::Wiki`）；
/// 3. **补齐缺失**：为期望集里尚无边的目标新建 wiki 边。
///
/// 手画的 `EdgeOrigin::Manual` 边**绝不被触碰**。`created_edges` 只计本次**真正新建**的 wiki 边。
///
/// 幂等性：对同一 note 连续调用两次，节点集与 wiki 边集一致，第二次 `created_nodes` /
/// `created_edges` 均为 0、不重复建边、不抖动 `EdgeIndex`。
pub fn resolve_links(
    graph: &mut Graph,
    canvas: &CanvasStateResource,
    source: NodeIndex,
) -> ResolveOutcome {
    let mut outcome = ResolveOutcome::default();

    let (body, source_pos) = match graph.get_node(source) {
        Some(n) => (n.note.clone(), n.position),
        None => return outcome,
    };

    // 1. 解析正文每个标题为目标节点（缺失则新建），得到本次正文期望连到的目标集合。
    let mut desired: Vec<NodeIndex> = Vec::new();
    for (i, title) in parse_links(&body).into_iter().enumerate() {
        // 经 find_by_title 收口寻址（text 优先、alias 次之），别名也能复用既有节点。
        let target = if let Some(first) = find_by_title(graph, &title) {
            // 歧义判定与寻址同口径：统计所有"标题或别名命中"的节点；多于一个则警示
            // （仍连到 find_by_title 选出的第一个）。
            let match_count = graph
                .graph
                .node_indices()
                .filter(|&idx| node_matches_title(&graph.graph[idx], &title))
                .count();
            if match_count > 1 {
                log::warn!(
                    "wikilink: 标题 \"{title}\" 匹配到 {match_count} 个节点，连接到第一个（其余忽略）"
                );
                outcome.ambiguous.push(title.clone());
            }
            first
        } else {
            let id = canvas.read_resource(|c| c.new_node_id());
            // 在 source 右侧错落排开，避免新节点叠在一起
            let pos = egui::pos2(source_pos.x + 180.0, source_pos.y + (i as f32) * 90.0);
            let idx = graph.add_node(Node {
                id,
                position: pos,
                text: title,
                note: String::new(),
                aliases: Vec::new(),
            });
            outcome.created_nodes.push(idx);
            idx
        };

        if target != source && !desired.contains(&target) {
            desired.push(target);
        }
    }

    // 2. 删除过时的 wiki 边：source 发出、目标不在期望集里的那些。
    graph.remove_stale_wiki_edges_from(source, &desired);

    // 3. 补齐缺失：期望目标尚无边（手画或 wiki）则建一条 wiki 边。
    for target in desired {
        if !graph.edge_exists(source, target) {
            let target_pos = graph.get_node(target).map_or(source_pos, |n| n.position);
            graph.add_edge(Edge::new_wiki(
                source,
                target,
                source_pos,
                target_pos,
                canvas.clone(),
            ));
            outcome.created_edges += 1;
        }
    }

    outcome
}

/// 一条反向链接：源节点 + 其正文里提及本节点（`[[标题]]`）的上下文行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    pub source: NodeIndex,
    pub title: String,
    /// 源节点正文里包含 `[[target 标题]]` 的那些行（已 trim）。
    pub contexts: Vec<String>,
}

/// 基于正文文本的反向链接（"被谁引用 + 原话"）——PKM "linked references" 的数据源。
///
/// 找出所有正文里出现 `[[target 的标题]]` **或** `[[target 的任一别名]]` 的节点，并附上
/// 包含该链接的上下文行。以**当前标题/别名**匹配（target 改名后旧链接不再命中，与 Obsidian
/// 一致）；标题与别名全为空的节点无反链。
pub fn backlinks_with_context(graph: &Graph, target: NodeIndex) -> Vec<Backlink> {
    // target 的全部寻址名：标题 ∪ 别名（去空）。任一被 [[…]] 引用即算反链。
    let target_names: Vec<String> = match graph.get_node(target) {
        Some(n) => {
            let mut names = Vec::new();
            if !n.text.is_empty() {
                names.push(n.text.clone());
            }
            names.extend(n.aliases.iter().filter(|a| !a.is_empty()).cloned());
            names
        }
        None => return Vec::new(),
    };
    if target_names.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for src_idx in graph.graph.node_indices() {
        if src_idx == target {
            continue;
        }
        let src = &graph.graph[src_idx];
        let contexts: Vec<String> = src
            .note
            .lines()
            .filter(|line| {
                parse_links(line)
                    .iter()
                    .any(|t| target_names.iter().any(|name| name == t))
            })
            .map(|line| line.trim().to_owned())
            .collect();
        if !contexts.is_empty() {
            out.push(Backlink {
                source: src_idx,
                title: src.text.clone(),
                contexts,
            });
        }
    }
    out
}

/// 反向链接：所有有边指向 `target` 的节点（incoming）。
pub fn backlinks(graph: &Graph, target: NodeIndex) -> Vec<NodeIndex> {
    let mut seen = Vec::new();
    for src in graph
        .graph
        .neighbors_directed(target, petgraph::Direction::Incoming)
    {
        if !seen.contains(&src) {
            seen.push(src);
        }
    }
    seen
}

/// 从正文里取一行摘要（命令面板/搜索结果用）。
///
/// `query` 非空：取**首个（大小写不敏感）包含 `query` 的行**（trim 后），让用户一眼看到命中所在；
/// 没有命中行（例如只命中了标题）或 `query` 为空：回退首个非空行。
/// 结果按 **char** 截断到 80 个字符（中文安全，绝不裸 byte slice）。空正文返回空串。
pub fn snippet_around(note: &str, query: &str) -> String {
    let q = query.trim().to_lowercase();
    let pick = if q.is_empty() {
        None
    } else {
        note.lines()
            .map(str::trim)
            .find(|l| l.to_lowercase().contains(&q))
    };
    let line = pick
        .or_else(|| note.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    line.chars().take(80).collect()
}

/// 孤立节点：图中**没有任何边连接**的节点（无向度数为 0），按节点索引顺序返回。
///
/// 大图卫生 / 发现盲点的实用工具（同 Obsidian 图谱「孤立笔记」过滤）：列出既无出边也无入边、
/// 既不被 `[[…]]` 引用也未手画连接的节点。判定用 `edges_directed(Incoming/Outgoing)`——
/// 比 `neighbors_undirected` 语义更显式，且对**自环**正确：自环边同时出现在该节点的入边与出边里，
/// 故自带自环的节点**不算孤立**（它有边）。空图返回空集。
pub fn orphan_nodes(graph: &Graph) -> Vec<NodeIndex> {
    graph
        .graph
        .node_indices()
        .filter(|&idx| {
            graph
                .graph
                .edges_directed(idx, petgraph::Direction::Incoming)
                .next()
                .is_none()
                && graph
                    .graph
                    .edges_directed(idx, petgraph::Direction::Outgoing)
                    .next()
                    .is_none()
        })
        .collect()
}

/// 节点的**无向度数**：该节点的边关联次数（出边数 + 入边数），纯拓扑。
///
/// "结构着色"（按枢纽程度编码节点外观）的数据源——度数越高越是图的枢纽。判定与
/// [`orphan_nodes`] **同口径**走 `edges_directed(Incoming/Outgoing)`（而非 `neighbors_undirected`：
/// 后者的 `skip_start` 会把自环去重为 1 个邻居、低估自环节点的边数），§3.3 用 `NodeIndex` 句柄、
/// 不依赖几何 observer、无一帧延迟。
///
/// 语义（= 标准图论"边关联度数"）：
/// - 孤立节点（无任何边）→ `0`。
/// - 链端 → `1`；链中 → `2`；星心（连 n 个叶）→ `n`。
/// - **自环**：一条自环边同时出现在该节点的入边与出边里，故对度数贡献 **2**——与"无向图里
///   自环度数计 2"的标准约定一致，也与 [`orphan_nodes`] 的自环判定（入/出任一非空即有边）自洽。
/// - 多重边（两节点间多条边）逐条计数。
/// - `node` 已失效（被删）→ `0`（`edges_directed` 对悬空索引产出空迭代器、不 panic）。
pub fn node_degree(graph: &Graph, node: NodeIndex) -> usize {
    graph
        .graph
        .edges_directed(node, petgraph::Direction::Outgoing)
        .count()
        + graph
            .graph
            .edges_directed(node, petgraph::Direction::Incoming)
            .count()
}

/// 全图**最大无向度数**（归一化基准）。空图 / 全孤立图返回 `0`。
///
/// "结构着色"把每个节点的度数按 `degree / max_degree` 归一到色阶 / 框宽；调用方须对
/// `max_degree == 0` 做除零兜底（全按最低档）。集中算一次（在 `render_graph` 入口经 temp data
/// 下发），避免每节点各扫全图的 `O(N²)`。纯函数、可无头单测。
pub fn max_degree(graph: &Graph) -> usize {
    graph
        .graph
        .node_indices()
        .map(|idx| node_degree(graph, idx))
        .max()
        .unwrap_or(0)
}

/// 以 `center` 为中心、半径 `hops` 跳的**邻域节点集**（含 `center` 自身），纯拓扑、不依赖几何。
///
/// "邻居聚焦"的数据源（Obsidian 局部图谱同理）：选中一个节点时，把它 + `hops` 跳内可达的邻居高亮、
/// 其余淡出。判定走**无向邻接**（`StableGraph::neighbors_undirected`：出/入边一视同仁），与
/// [`orphan_nodes`] / [`backlinks`] 同口径用 `NodeIndex` 句柄（§3.3），无一帧延迟、无几何 `.unwrap()`
/// panic 风险（比隐藏耦合安全）。
///
/// 语义：
/// - `hops == 0`：只含 `center` 自身（退化，供"只聚焦中心"用）。
/// - `hops == 1`：`center` + 直接邻居（默认聚焦半径）。
/// - `hops >= 2`：逐层 BFS 扩展，每个节点只计**最短跳数**、不重复。
/// - 孤立节点（无任何边）的邻域恒为其自身。
/// - `center` 已失效（被删）则返回空集——`neighbors_undirected` 对不存在的索引产出空迭代器、不 panic。
/// - **自环**：自环邻居即 `center`，已被 `center` 自身覆盖，不致重复或漏算。
///
/// 复杂度 `O(邻域内边数)`：BFS 只遍历被访问节点的邻接，不扫全图。
pub fn neighborhood(graph: &Graph, center: NodeIndex, hops: usize) -> HashSet<NodeIndex> {
    let mut visited: HashSet<NodeIndex> = HashSet::new();
    // center 失效（不在图中）时直接返回空集，避免把悬空索引当中心。
    if graph.get_node(center).is_none() {
        return visited;
    }
    visited.insert(center);
    // (节点, 已用跳数) 的 BFS 队列；按最短跳数分层扩展，达到 hops 即止。
    let mut frontier: VecDeque<(NodeIndex, usize)> = VecDeque::new();
    frontier.push_back((center, 0));
    while let Some((node, depth)) = frontier.pop_front() {
        if depth >= hops {
            continue;
        }
        for nb in graph.graph.neighbors_undirected(node) {
            // HashSet 去重：多重边 / 已访问节点只入队一次，自环邻居即 center 已在集内。
            if visited.insert(nb) {
                frontier.push_back((nb, depth + 1));
            }
        }
    }
    visited
}

/// 全文搜索：标题、**别名**或正文包含 `query`（**大小写不敏感**的子串匹配）的节点，
/// 按节点索引顺序返回。
///
/// **标签检索分支**：`query` 以 `#` 开头且其后是合法标签名（如 `#rust`）时，转为**标签精确匹配**
/// ——经 [`nodes_with_tag`] 列出正文含该 `#tag` 的节点（大小写敏感、精确相等，而非子串）。这让命令面板
/// 输 `#rust` 直接命中带该标签的笔记，是标签作为「横切分类」检索维度的入口（标签不进图，实时 parse）。
/// 退化：仅一个 `#`（后无合法标签名）落回普通子串搜索（按 `#` 子串匹配，行为同旧版）。
///
/// 纳入别名：`[[别名]]`（或别名本身的关键词）在命令面板能搜到对应节点——与别名寻址语义一致。
pub fn search(graph: &Graph, query: &str) -> Vec<NodeIndex> {
    if query.is_empty() {
        return Vec::new();
    }
    // 标签分支：`#tag`（紧跟合法标签名）→ 标签精确匹配（大小写敏感）。
    if let Some(rest) = query.strip_prefix('#') {
        let tag: String = rest.chars().take_while(|&c| is_tag_char(c)).collect();
        if !tag.is_empty() {
            return nodes_with_tag(graph, &tag);
        }
    }
    let q = query.to_lowercase();
    graph
        .graph
        .node_indices()
        .filter(|&i| {
            let n = &graph.graph[i];
            n.text.to_lowercase().contains(&q)
                || n.note.to_lowercase().contains(&q)
                || n.aliases.iter().any(|a| a.to_lowercase().contains(&q))
        })
        .collect()
}

/// 所有节点位置（画布坐标，AGENTS.md §3.2）的最小包围盒。空图返回 `None`。
///
/// 纯函数、不依赖 GUI / observer（直接读 `Node.position`，避一帧延迟）：是 minimap
/// 等比投影与 `app.rs::zoom_to_fit` 的共享几何基元，便于无头单测（空图 `None`、单节点、
/// 共线、多点）。**仅算包围盒**——退化（单节点 / 共线 → width 或 height = 0）的最小尺寸
/// 兜底留给调用方（minimap 投影、zoom_to_fit 各自 `max(1.0)`），本函数保持"如实反映几何"。
pub fn minimap_bbox(graph: &Graph) -> Option<egui::Rect> {
    let mut it = graph.graph.node_indices().map(|i| graph.graph[i].position);
    let first = it.next()?;
    let mut rect = egui::Rect::from_min_max(first, first);
    for p in it {
        rect = rect.union(egui::Rect::from_min_max(p, p));
    }
    Some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasState;

    fn graph_with(titles: &[&str]) -> (Graph, CanvasStateResource) {
        let canvas = CanvasStateResource::new(CanvasState::default());
        let mut g = Graph::default();
        for t in titles {
            let id = canvas.read_resource(|c| c.new_node_id());
            g.add_node(Node {
                id,
                position: egui::pos2(0.0, 0.0),
                text: (*t).to_owned(),
                note: String::new(),
                aliases: Vec::new(),
            });
        }
        (g, canvas)
    }

    #[test]
    fn parse_basic_dedup_trim_chinese() {
        assert_eq!(
            parse_links("see [[Alpha]] and [[Beta]]"),
            vec!["Alpha", "Beta"]
        );
        // 去重 + 去空白
        assert_eq!(parse_links("[[ A ]] x [[A]]"), vec!["A"]);
        // 中文标题
        assert_eq!(
            parse_links("关联 [[知识图谱]] 与 [[第二大脑]]"),
            vec!["知识图谱", "第二大脑"]
        );
        // 空标题 / 未闭合 / 嵌套残留
        assert_eq!(parse_links("[[]] [[unclosed"), Vec::<String>::new());
        assert_eq!(parse_links("[[a[[b]]]]"), Vec::<String>::new());
        assert!(parse_links("no links here").is_empty());
    }

    // ===== 标签 #tag 解析（parse_tags）—— 与 markdown 标题消歧、中文终止边界 =====

    #[test]
    fn parse_tags_english_and_chinese() {
        // 句中标签、英文、中文。
        assert_eq!(parse_tags("见 #rust 这里"), vec!["rust"]);
        assert_eq!(parse_tags("关于 #知识图谱 的笔记"), vec!["知识图谱"]);
        // 多个标签，保留出现顺序。
        assert_eq!(
            parse_tags("#rust 与 #wasm 和 #egui"),
            vec!["rust", "wasm", "egui"]
        );
        // 数字 / 下划线 / 连字符。
        assert_eq!(
            parse_tags("#v2 #foo_bar #foo-bar"),
            vec!["v2", "foo_bar", "foo-bar"]
        );
    }

    #[test]
    fn parse_tags_line_start_tag_vs_heading() {
        // 行首 `#tag`（无空格）= 标签。
        assert_eq!(parse_tags("#rust 行首标签"), vec!["rust"]);
        // `# 标题`（`#` 后空格）= markdown 标题，不是标签。
        assert!(parse_tags("# 标题").is_empty());
        assert!(parse_tags("# 这是中文标题").is_empty());
        // `## 二级标题` / `### 三级` 同理（`#` 后紧跟 `#` 非标签字符 → 不构成标签；后续是空格→标题）。
        assert!(parse_tags("## 二级标题").is_empty());
        assert!(parse_tags("### 三级标题").is_empty());
    }

    #[test]
    fn parse_tags_punctuation_terminates() {
        // 中文标点终止：`#知识图谱，` 的标签是 `知识图谱`（逗号不入标签）。
        assert_eq!(parse_tags("#知识图谱，很重要"), vec!["知识图谱"]);
        // 英文标点 / 括号 / 句号终止。
        assert_eq!(parse_tags("用 #rust. 写"), vec!["rust"]);
        assert_eq!(parse_tags("(#tag)"), vec!["tag"]);
        assert_eq!(parse_tags("#tag, #other"), vec!["tag", "other"]);
    }

    #[test]
    fn parse_tags_hierarchy_slash() {
        // 层级标签 `#a/b`：`/` 入标签字符集。
        assert_eq!(
            parse_tags("#projects/cognitheon"),
            vec!["projects/cognitheon"]
        );
        assert_eq!(parse_tags("#a/b/c 末尾"), vec!["a/b/c"]);
    }

    #[test]
    fn parse_tags_dedup_and_lone_hash() {
        // 去重、保留首次顺序。
        assert_eq!(parse_tags("#rust #rust #wasm #rust"), vec!["rust", "wasm"]);
        // 孤立 `#`（后接空白 / EOF / 标点）不构成标签。
        assert!(parse_tags("单独 # 号").is_empty());
        assert!(parse_tags("行尾 #").is_empty());
        assert!(parse_tags("#! 非标签").is_empty());
        assert!(parse_tags("没有标签的文本").is_empty());
    }

    #[test]
    fn parse_tags_chinese_terminates_at_space_and_eof() {
        // 中文标签在空白终止；行尾标签吃到 EOF。
        assert_eq!(parse_tags("#第二大脑 是核心"), vec!["第二大脑"]);
        assert_eq!(parse_tags("末尾 #机器学习"), vec!["机器学习"]);
    }

    // ===== 标签检索 nodes_with_tag / search(#tag) =====

    #[test]
    fn nodes_with_tag_matches_body_tags() {
        let (mut g, _c) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        g.get_node_mut(a).unwrap().note = "学习 #rust 与 #wasm".to_owned();
        g.get_node_mut(b).unwrap().note = "只有 #rust".to_owned();
        g.get_node_mut(c).unwrap().note = "无标签".to_owned();

        assert_eq!(nodes_with_tag(&g, "rust"), vec![a, b], "rust 命中 A、B");
        assert_eq!(nodes_with_tag(&g, "wasm"), vec![a], "wasm 仅命中 A");
        // 带前导 `#` 也应被剥离后精确匹配。
        assert_eq!(nodes_with_tag(&g, "#rust"), vec![a, b]);
        // 不存在 / 空标签。
        assert!(nodes_with_tag(&g, "python").is_empty());
        assert!(nodes_with_tag(&g, "").is_empty());
        assert!(nodes_with_tag(&g, "#").is_empty());
    }

    #[test]
    fn nodes_with_tag_chinese_and_case_sensitive() {
        let (mut g, _c) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.get_node_mut(a).unwrap().note = "#知识图谱 的笔记".to_owned();
        g.get_node_mut(b).unwrap().note = "#Rust 大写".to_owned();
        assert_eq!(nodes_with_tag(&g, "知识图谱"), vec![a]);
        // 大小写敏感：`rust` 不命中 `#Rust`。
        assert!(nodes_with_tag(&g, "rust").is_empty());
        assert_eq!(nodes_with_tag(&g, "Rust"), vec![b]);
    }

    #[test]
    fn search_tag_branch_exact_match() {
        let (mut g, _c) = graph_with(&["有标签", "无标签", "标题含rust"]);
        let tagged = find_by_title(&g, "有标签").unwrap();
        g.get_node_mut(tagged).unwrap().note = "见 #rust 这里".to_owned();
        // `#rust` 走标签分支：只命中正文含 #rust 的节点（不因标题含 "rust" 字样命中）。
        assert_eq!(
            search(&g, "#rust"),
            vec![tagged],
            "标签搜索精确命中带标签节点"
        );
        // 普通子串搜索 "rust"：标题含 rust 的节点也命中（验证两条分支区别）。
        let by_title = find_by_title(&g, "标题含rust").unwrap();
        let hits = search(&g, "rust");
        assert!(hits.contains(&tagged) && hits.contains(&by_title));
        // 仅 `#`（无合法标签名）落回普通子串搜索：正文含 `#` 字面量的节点命中（不走标签分支、不 panic）。
        assert_eq!(
            search(&g, "#"),
            vec![tagged],
            "孤立 # 落回子串搜索，命中正文含 # 的节点"
        );
    }

    #[test]
    fn resolve_creates_missing_target_and_edge() {
        let (mut g, canvas) = graph_with(&["源"]);
        let src = find_by_title(&g, "源").unwrap();
        g.get_node_mut(src).unwrap().note = "指向 [[新节点]]".to_owned();

        let out = resolve_links(&mut g, &canvas, src);
        assert_eq!(out.created_nodes.len(), 1, "应新建 1 个目标节点");
        assert_eq!(out.created_edges, 1, "应新建 1 条边");
        assert_eq!(g.graph.node_count(), 2);
        assert!(find_by_title(&g, "新节点").is_some());
    }

    #[test]
    fn resolve_reuses_existing_target_no_new_node() {
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        g.get_node_mut(a).unwrap().note = "[[B]]".to_owned();

        let out = resolve_links(&mut g, &canvas, a);
        assert!(out.created_nodes.is_empty(), "B 已存在，不应新建节点");
        assert_eq!(out.created_edges, 1);
        assert_eq!(g.graph.node_count(), 2);
    }

    #[test]
    fn resolve_is_idempotent_and_skips_self_link() {
        let (mut g, canvas) = graph_with(&["A"]);
        let a = find_by_title(&g, "A").unwrap();
        // 自指 [[A]] 应被跳过；[[B]] 第一次建、第二次幂等
        g.get_node_mut(a).unwrap().note = "[[A]] [[B]]".to_owned();

        let first = resolve_links(&mut g, &canvas, a);
        assert_eq!(first.created_nodes.len(), 1); // 只建 B
        assert_eq!(first.created_edges, 1); // 只连 A->B（A->A 跳过）

        let second = resolve_links(&mut g, &canvas, a);
        assert_eq!(
            second,
            ResolveOutcome::default(),
            "重复解析应无新增（幂等）"
        );
    }

    #[test]
    fn backlinks_lists_incoming() {
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.get_node_mut(a).unwrap().note = "[[B]]".to_owned();
        resolve_links(&mut g, &canvas, a);
        assert_eq!(backlinks(&g, b), vec![a], "B 的反向链接应是 A");
        assert!(backlinks(&g, a).is_empty());
    }

    #[test]
    fn backlinks_with_context_returns_source_and_lines() {
        let (mut g, canvas) = graph_with(&["目标"]);
        let target = find_by_title(&g, "目标").unwrap();
        let id = canvas.read_resource(|c| c.new_node_id());
        let src = g.add_node(Node {
            id,
            position: egui::pos2(0.0, 0.0),
            text: "源笔记".to_owned(),
            note: "无关的一行\n这里提到 [[目标]]，很重要\n又一行写了 [[目标]] 再次".to_owned(),
            aliases: Vec::new(),
        });

        let bls = backlinks_with_context(&g, target);
        assert_eq!(bls.len(), 1, "应有一个来源");
        assert_eq!(bls[0].source, src);
        assert_eq!(bls[0].title, "源笔记");
        assert_eq!(bls[0].contexts.len(), 2, "两行都提到了 [[目标]]");
        assert!(bls[0].contexts[0].contains("很重要"));
        // 空标题节点无反链；自身不计入
        assert!(backlinks_with_context(&g, src).is_empty());
    }

    #[test]
    fn search_matches_title_and_body() {
        let (mut g, _canvas) = graph_with(&["标题里有关键词", "别的"]);
        let other = find_by_title(&g, "别的").unwrap();
        g.get_node_mut(other).unwrap().note = "正文里也有关键词".to_owned();
        let hits = search(&g, "关键词");
        assert_eq!(hits.len(), 2, "标题命中 + 正文命中");
        assert!(search(&g, "不存在").is_empty());
        assert!(search(&g, "").is_empty());
    }

    #[test]
    fn search_is_case_insensitive() {
        let (g, _c) = graph_with(&["Hello World"]);
        assert_eq!(search(&g, "hello").len(), 1);
        assert_eq!(search(&g, "WORLD").len(), 1);
    }

    #[test]
    fn orphan_nodes_empty_graph_is_empty() {
        let (g, _c) = graph_with(&[]);
        assert!(orphan_nodes(&g).is_empty(), "空图无孤立节点");
    }

    #[test]
    fn orphan_nodes_all_isolated() {
        // 三个互不相连的节点 → 全部孤立，按索引顺序返回。
        let (g, _c) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        assert_eq!(orphan_nodes(&g), vec![a, b, c]);
    }

    #[test]
    fn orphan_nodes_chain_has_none() {
        // A -> B -> C 链：每个节点都有边，无孤立。
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        g.add_edge(Edge::new(
            b,
            c,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert!(orphan_nodes(&g).is_empty(), "链式结构无孤立节点");
    }

    #[test]
    fn orphan_nodes_incoming_only_not_orphan() {
        // 只有入边的节点也不算孤立（出/入任一非空即有边）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        // A 有出边、B 有入边，二者都不孤立。
        assert!(orphan_nodes(&g).is_empty());
    }

    #[test]
    fn orphan_nodes_self_loop_is_not_orphan() {
        // 自环：边同时是入边与出边，故有边 → 不算孤立（语义：孤立 = 无任何边）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            a,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(orphan_nodes(&g), vec![b], "仅自环的 A 不孤立，B 才孤立");
    }

    #[test]
    fn orphan_nodes_appears_after_edge_removed() {
        // 删边后端点恢复孤立。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert!(orphan_nodes(&g).is_empty());

        let eidx = g.graph.edge_indices().next().unwrap();
        g.graph.remove_edge(eidx);
        assert_eq!(orphan_nodes(&g), vec![a, b], "删边后两端都回到孤立");
    }

    // ===== 节点度数 node_degree / max_degree（结构着色数据源）=====

    #[test]
    fn node_degree_isolated_is_zero() {
        // 孤立节点（无任何边）度数为 0；空图 max_degree 为 0。
        let (g_empty, _c0) = graph_with(&[]);
        assert_eq!(max_degree(&g_empty), 0, "空图最大度数为 0");

        let (g, _c) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        assert_eq!(node_degree(&g, a), 0);
        assert_eq!(max_degree(&g), 0, "全孤立图最大度数为 0");
    }

    #[test]
    fn node_degree_chain_ends_and_middle() {
        // 链 A - B - C：端点度数 1、链中度数 2，max_degree = 2。
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        g.add_edge(Edge::new(
            b,
            c,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(node_degree(&g, a), 1, "链端度数 1");
        assert_eq!(node_degree(&g, c), 1, "链端度数 1");
        assert_eq!(node_degree(&g, b), 2, "链中度数 2");
        assert_eq!(max_degree(&g), 2);
    }

    #[test]
    fn node_degree_star_center_is_leaf_count() {
        // 星形：中心连 3 个叶 → 中心度数 3、各叶度数 1，max_degree = 3。
        let (mut g, canvas) = graph_with(&["C", "L1", "L2", "L3"]);
        let center = find_by_title(&g, "C").unwrap();
        for leaf in ["L1", "L2", "L3"] {
            let l = find_by_title(&g, leaf).unwrap();
            g.add_edge(Edge::new(
                center,
                l,
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
                canvas.clone(),
            ));
        }
        assert_eq!(node_degree(&g, center), 3, "星心度数 = 叶数");
        for leaf in ["L1", "L2", "L3"] {
            let l = find_by_title(&g, leaf).unwrap();
            assert_eq!(node_degree(&g, l), 1, "叶节点度数 1");
        }
        assert_eq!(max_degree(&g), 3);
    }

    #[test]
    fn node_degree_self_loop_counts_two() {
        // 自环：neighbors_undirected 把自环边产出两次 → 度数 +2（无向图自环计 2 的标准约定）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            a,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(node_degree(&g, a), 2, "一条自环对度数贡献 2");
        assert_eq!(node_degree(&g, b), 0, "B 仍孤立");
        assert_eq!(max_degree(&g), 2);
    }

    #[test]
    fn node_degree_multi_edge_counts_each() {
        // 多重边：A、B 间两条边 → 两端度数各 2（逐条计数）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        for _ in 0..2 {
            g.add_edge(Edge::new(
                a,
                b,
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
                canvas.clone(),
            ));
        }
        assert_eq!(node_degree(&g, a), 2, "多重边逐条计数");
        assert_eq!(node_degree(&g, b), 2);
        assert_eq!(max_degree(&g), 2);
    }

    #[test]
    fn node_degree_stale_index_is_zero() {
        // 已删节点（悬空索引）度数为 0、不 panic（§3.3 容错）。
        let (mut g, _c) = graph_with(&["A"]);
        let a = find_by_title(&g, "A").unwrap();
        g.remove_node(a);
        assert_eq!(node_degree(&g, a), 0);
    }

    // ===== 邻域聚焦 neighborhood =====

    #[test]
    fn neighborhood_isolated_node_is_self_only() {
        // 孤立节点（无任何边）：任意跳数邻域恒为自身。
        let (g, _c) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        assert_eq!(neighborhood(&g, a, 1), HashSet::from([a]));
        assert_eq!(neighborhood(&g, a, 3), HashSet::from([a]));
    }

    #[test]
    fn neighborhood_zero_hops_is_self_only() {
        // hops == 0：即使有邻居也只含中心自身（退化分支）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(neighborhood(&g, a, 0), HashSet::from([a]));
    }

    #[test]
    fn neighborhood_one_hop_chain() {
        // 链 A - B - C：1 跳邻域 = 中心 + 直接邻居（无向，方向无关）。
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        g.add_edge(Edge::new(
            b,
            c,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        // B 居中：1 跳含 A、B、C。
        assert_eq!(neighborhood(&g, b, 1), HashSet::from([a, b, c]));
        // A 在端点：1 跳只含 A、B（C 是 2 跳）。
        assert_eq!(neighborhood(&g, a, 1), HashSet::from([a, b]));
    }

    #[test]
    fn neighborhood_two_hops_reaches_further() {
        // 链 A - B - C - D：从 A 出发 2 跳含 A、B、C（D 是 3 跳，不含）。
        let (mut g, canvas) = graph_with(&["A", "B", "C", "D"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        let d = find_by_title(&g, "D").unwrap();
        for (s, t) in [(a, b), (b, c), (c, d)] {
            g.add_edge(Edge::new(
                s,
                t,
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
                canvas.clone(),
            ));
        }
        assert_eq!(neighborhood(&g, a, 2), HashSet::from([a, b, c]));
        assert_eq!(neighborhood(&g, a, 3), HashSet::from([a, b, c, d]));
    }

    #[test]
    fn neighborhood_direction_agnostic() {
        // 有向边 A -> B：无向邻域里 B 的 1 跳仍含 A（入边也算邻居）。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(neighborhood(&g, b, 1), HashSet::from([a, b]));
    }

    #[test]
    fn neighborhood_cycle_no_duplicates_and_terminates() {
        // 环 A - B - C - A：BFS 经 HashSet 去重，不因环死循环。
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();
        for (s, t) in [(a, b), (b, c), (c, a)] {
            g.add_edge(Edge::new(
                s,
                t,
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
                canvas.clone(),
            ));
        }
        // 1 跳从 A：A 的直接邻居是 B、C（经 A-B 与 C-A），全员到齐。
        assert_eq!(neighborhood(&g, a, 1), HashSet::from([a, b, c]));
        // 大跳数也不超过整环节点集，且能终止。
        assert_eq!(neighborhood(&g, a, 10), HashSet::from([a, b, c]));
    }

    #[test]
    fn neighborhood_self_loop_is_self() {
        // 自环 A-A：A 的邻居即自身，邻域 = {A}，不重复也不漏。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        g.add_edge(Edge::new(
            a,
            a,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        assert_eq!(neighborhood(&g, a, 1), HashSet::from([a]));
        assert_eq!(neighborhood(&g, a, 2), HashSet::from([a]));
    }

    #[test]
    fn neighborhood_stale_center_is_empty() {
        // 中心已被删（悬空索引）：返回空集、不 panic（§3.3 容错）。
        let (mut g, _c) = graph_with(&["A"]);
        let a = find_by_title(&g, "A").unwrap();
        g.remove_node(a);
        assert!(neighborhood(&g, a, 1).is_empty());
    }

    #[test]
    fn orphan_nodes_wiki_edge_makes_target_non_orphan() {
        // wiki 自动边与手画边一视同仁：被 [[…]] 连上的目标不再孤立。
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        g.get_node_mut(a).unwrap().note = "[[B]]".to_owned();
        resolve_links(&mut g, &canvas, a);
        assert!(orphan_nodes(&g).is_empty(), "wiki 边连接后无孤立节点");
    }

    #[test]
    fn snippet_prefers_matching_line() {
        let note = "第一行无关\n这里有关键词在中间\n第三行";
        // 命中行优先（大小写不敏感）
        assert_eq!(snippet_around(note, "关键词"), "这里有关键词在中间");
        assert_eq!(
            snippet_around("alpha\nBETA here\ngamma", "beta"),
            "BETA here"
        );
    }

    #[test]
    fn snippet_falls_back_to_first_nonempty_line() {
        // 无命中行（只命中标题等场景）→ 回退首个非空行
        assert_eq!(snippet_around("\n  \n首行内容\n次行", "不存在"), "首行内容");
        // 空 query → 回退首个非空行
        assert_eq!(snippet_around("\n首个非空\n", ""), "首个非空");
    }

    #[test]
    fn snippet_empty_note_is_empty() {
        assert_eq!(snippet_around("", "x"), "");
        assert_eq!(snippet_around("   \n  ", "x"), "");
    }

    #[test]
    fn snippet_truncates_by_char_not_byte() {
        // 全中文 100 字：char 截断到 80，不在多字节中间切断（不 panic）
        let line: String = "字".repeat(100);
        let out = snippet_around(&line, "字");
        assert_eq!(out.chars().count(), 80);
        assert_eq!(out, "字".repeat(80));
    }

    /// 收集 `source` 当前的出边来源标记，用于断言 wiki/manual 边集。
    fn out_edges(g: &Graph, source: NodeIndex) -> Vec<crate::graph::edge::EdgeOrigin> {
        g.graph
            .edges_directed(source, petgraph::Direction::Outgoing)
            .map(|e| e.weight().origin)
            .collect()
    }

    #[test]
    fn resolve_removes_stale_wiki_edge_when_link_dropped() {
        use crate::graph::edge::EdgeOrigin;
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();

        // 第一次：A 正文链向 B 与 C，建两条 wiki 边
        g.get_node_mut(a).unwrap().note = "[[B]] [[C]]".to_owned();
        resolve_links(&mut g, &canvas, a);
        assert_eq!(g.graph.edges(a).count(), 2, "应有两条出边");
        assert!(out_edges(&g, a).iter().all(|o| *o == EdgeOrigin::Wiki));

        // 删掉 [[C]]，只留 [[B]]：再 resolve 后过时的 A->C wiki 边应消失
        g.get_node_mut(a).unwrap().note = "[[B]]".to_owned();
        resolve_links(&mut g, &canvas, a);
        assert_eq!(g.graph.edges(a).count(), 1, "过时 wiki 边应被删除");
        assert!(g.edge_exists(a, b), "A->B 应保留");
        let c = find_by_title(&g, "C").unwrap();
        assert!(!g.edge_exists(a, c), "A->C 应已删除");
    }

    #[test]
    fn resolve_does_not_remove_manual_edges() {
        use crate::graph::edge::{Edge, EdgeOrigin};
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        let c = find_by_title(&g, "C").unwrap();

        // 用户手画 A->B（Manual），且 A 正文链向 C（Wiki）
        g.add_edge(Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        ));
        g.get_node_mut(a).unwrap().note = "[[C]]".to_owned();
        resolve_links(&mut g, &canvas, a);
        assert!(g.edge_exists(a, c), "wiki 边 A->C 应建立");

        // 清空正文再 resolve：手画 A->B 必须保留，wiki A->C 必须消失
        g.get_node_mut(a).unwrap().note = String::new();
        resolve_links(&mut g, &canvas, a);
        let origins = out_edges(&g, a);
        assert_eq!(origins, vec![EdgeOrigin::Manual], "只剩手画的 A->B");
        assert!(g.edge_exists(a, b), "手画边 A->B 永不被删");
        assert!(!g.edge_exists(a, c), "wiki 边 A->C 应被删");
    }

    #[test]
    fn resolve_wiki_edge_set_is_idempotent() {
        let (mut g, canvas) = graph_with(&["A", "B", "C"]);
        let a = find_by_title(&g, "A").unwrap();
        g.get_node_mut(a).unwrap().note = "[[B]] [[C]]".to_owned();

        resolve_links(&mut g, &canvas, a);
        let edges_after_first = g.graph.edge_count();

        // 连续两次：边集不抖动、不重复建
        let second = resolve_links(&mut g, &canvas, a);
        assert_eq!(second.created_edges, 0, "第二次不应新建边");
        assert!(second.created_nodes.is_empty(), "第二次不应新建节点");
        assert_eq!(
            g.graph.edge_count(),
            edges_after_first,
            "wiki 边集应稳定（幂等）"
        );
        assert_eq!(g.graph.edges(a).count(), 2);
    }

    #[test]
    fn old_archive_edge_defaults_manual_and_survives_resolve() {
        use crate::graph::edge::{Edge, EdgeOrigin};
        let (mut g, canvas) = graph_with(&["A", "B"]);
        let a = find_by_title(&g, "A").unwrap();
        let b = find_by_title(&g, "B").unwrap();

        // 构造一条真实的边并序列化，再从 JSON 中剥掉 `origin` 字段，模拟旧 .cnt（无该字段）。
        // 用真实 serde 形状（而非手写 JSON）避免脆弱性。
        let edge = Edge::new(
            a,
            b,
            egui::pos2(0.0, 0.0),
            egui::pos2(0.0, 0.0),
            canvas.clone(),
        );
        let mut value: serde_json::Value = serde_json::to_value(&edge).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("origin")
            .expect("新边序列化应含 origin 字段");

        // 旧档（无 origin）反序列化应默认 Manual
        let old_edge: Edge = serde_json::from_value(value).expect("旧档边应能反序列化");
        assert_eq!(
            old_edge.origin,
            EdgeOrigin::Manual,
            "缺 origin 字段应默认 Manual"
        );

        // 旧档里的边落到图上后，resolve 不应误删它
        g.add_edge(old_edge);
        g.get_node_mut(a).unwrap().note = String::new();
        resolve_links(&mut g, &canvas, a);
        assert!(g.edge_exists(a, b), "旧档手画边永不被 resolve 删除");
    }

    #[test]
    fn resolve_flags_ambiguous_duplicate_titles() {
        let (mut g, canvas) = graph_with(&["Dup", "Dup", "源"]);
        let src = find_by_title(&g, "源").unwrap();
        g.get_node_mut(src).unwrap().note = "[[Dup]]".to_owned();
        let out = resolve_links(&mut g, &canvas, src);
        assert!(out.created_nodes.is_empty(), "Dup 已存在不该新建");
        assert_eq!(out.ambiguous, vec!["Dup".to_string()], "同名应被标记为歧义");
        assert_eq!(out.created_edges, 1, "仍应连到第一个 Dup");
    }

    // ===== 节点别名 aliases =====

    /// 设置节点别名（逗号分隔语义已在 UI/headless 层处理，这里直接给 Vec）。
    fn set_aliases(g: &mut Graph, idx: NodeIndex, aliases: &[&str]) {
        g.get_node_mut(idx).unwrap().aliases = aliases.iter().map(|s| (*s).to_owned()).collect();
    }

    #[test]
    fn find_by_title_resolves_alias() {
        let (mut g, _c) = graph_with(&["机器学习"]);
        let ml = find_by_title(&g, "机器学习").unwrap();
        set_aliases(&mut g, ml, &["ML", "machine learning"]);
        // 别名命中同一节点
        assert_eq!(find_by_title(&g, "ML"), Some(ml));
        assert_eq!(find_by_title(&g, "machine learning"), Some(ml));
        // 标题仍命中
        assert_eq!(find_by_title(&g, "机器学习"), Some(ml));
        // 无关词不命中
        assert!(find_by_title(&g, "深度学习").is_none());
    }

    #[test]
    fn find_all_by_title_no_match_is_empty() {
        // 空结果：图中无该标题/别名 → 长度 0（面板视作"未创建"）。
        let (g, _c) = graph_with(&["A", "B"]);
        assert!(find_all_by_title(&g, "不存在").is_empty());
        // 空图
        let (empty, _c0) = graph_with(&[]);
        assert!(find_all_by_title(&empty, "任何").is_empty());
    }

    #[test]
    fn find_all_by_title_single_match() {
        // 无歧义：唯一命中 → 长度 1，且与 find_by_title 选出的同一节点。
        let (g, _c) = graph_with(&["唯一", "别的"]);
        let only = find_by_title(&g, "唯一").unwrap();
        assert_eq!(find_all_by_title(&g, "唯一"), vec![only]);
    }

    #[test]
    fn find_all_by_title_two_same_text() {
        // 两个同名（text 相同）→ 长度 2，索引升序，首个 = find_by_title 的连接目标。
        let (g, _c) = graph_with(&["Dup", "Dup", "其它"]);
        let all = find_all_by_title(&g, "Dup");
        assert_eq!(all.len(), 2, "两个同名节点应全部返回");
        // 索引升序：先建的在前
        assert!(all[0].index() < all[1].index());
        // 首个与 find_by_title（"取第一个"）一致
        assert_eq!(all[0], find_by_title(&g, "Dup").unwrap());
    }

    #[test]
    fn find_all_by_title_text_then_alias_ordering() {
        // text 命中 + alias 命中混合：A.text=="K"、B.alias 含 "K" → 返回 [A, B]（text 在前）。
        let (mut g, _c) = graph_with(&["K", "B"]);
        let a = find_by_title(&g, "K").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        set_aliases(&mut g, b, &["K"]);
        let all = find_all_by_title(&g, "K");
        assert_eq!(all, vec![a, b], "text 命中在前、alias 命中在后");
        // 首个与 find_by_title（text 优先）一致
        assert_eq!(all[0], find_by_title(&g, "K").unwrap());
    }

    #[test]
    fn find_all_by_title_alias_only_match() {
        // 仅别名命中：节点标题不同、别名含查询词 → 命中该节点（长度 1）。
        let (mut g, _c) = graph_with(&["机器学习", "无关"]);
        let ml = find_by_title(&g, "机器学习").unwrap();
        set_aliases(&mut g, ml, &["ML"]);
        assert_eq!(find_all_by_title(&g, "ML"), vec![ml]);
    }

    #[test]
    fn find_all_by_title_same_node_text_and_alias_counted_once() {
        // 同一节点的 text 与某别名都等于查询词（退化情形）→ 只计一次，不重复。
        let (mut g, _c) = graph_with(&["X"]);
        let x = find_by_title(&g, "X").unwrap();
        set_aliases(&mut g, x, &["X"]); // 别名与标题同字
        assert_eq!(find_all_by_title(&g, "X"), vec![x], "同节点命中只计一次");
    }

    #[test]
    fn find_by_title_prefers_text_over_alias() {
        // 节点 A 的标题是 "X"；节点 B 的别名也是 "X"。text 优先 → 命中 A（先建的）。
        let (mut g, _c) = graph_with(&["X", "B"]);
        let a = find_by_title(&g, "X").unwrap();
        let b = find_by_title(&g, "B").unwrap();
        set_aliases(&mut g, b, &["X"]);
        assert_eq!(
            find_by_title(&g, "X"),
            Some(a),
            "标题精确匹配优先于别名匹配"
        );
    }

    #[test]
    fn resolve_links_via_alias_reuses_node_no_new_node() {
        let (mut g, canvas) = graph_with(&["目标", "源"]);
        let target = find_by_title(&g, "目标").unwrap();
        let src = find_by_title(&g, "源").unwrap();
        set_aliases(&mut g, target, &["alias-of-target"]);

        g.get_node_mut(src).unwrap().note = "[[alias-of-target]]".to_owned();
        let out = resolve_links(&mut g, &canvas, src);
        assert!(out.created_nodes.is_empty(), "别名命中既有节点，不应新建");
        assert_eq!(out.created_edges, 1, "应连一条 src->target 的 wiki 边");
        assert!(g.edge_exists(src, target), "边应指向别名所属节点");
    }

    #[test]
    fn alias_and_text_to_same_node_is_idempotent_single_edge() {
        // 幂等关键用例：note 同时含 [[标题]] 与 [[别名]] 指向同一节点 →
        // 只连一条边，连续两次 resolve 边集一致（desired 以 NodeIndex 去重）。
        let (mut g, canvas) = graph_with(&["目标", "源"]);
        let target = find_by_title(&g, "目标").unwrap();
        let src = find_by_title(&g, "源").unwrap();
        set_aliases(&mut g, target, &["TGT"]);

        g.get_node_mut(src).unwrap().note = "见 [[目标]] 又见 [[TGT]]".to_owned();
        let first = resolve_links(&mut g, &canvas, src);
        assert!(first.created_nodes.is_empty(), "目标已存在，不应新建节点");
        assert_eq!(
            first.created_edges, 1,
            "[[目标]] 与 [[TGT]] 同指一节点，只应连一条边"
        );
        assert_eq!(g.graph.edges(src).count(), 1, "src 只有一条出边");

        // 第二次 resolve：幂等，无新增、边集不抖动
        let edges_before = g.graph.edge_count();
        let second = resolve_links(&mut g, &canvas, src);
        assert_eq!(second.created_edges, 0, "第二次不应新建边（幂等）");
        assert!(second.created_nodes.is_empty(), "第二次不应新建节点");
        assert_eq!(g.graph.edge_count(), edges_before, "边集应稳定");
        assert_eq!(g.graph.edges(src).count(), 1, "src 仍只有一条出边");
    }

    #[test]
    fn backlinks_with_context_matches_via_alias() {
        let (mut g, _c) = graph_with(&["目标", "源"]);
        let target = find_by_title(&g, "目标").unwrap();
        let src = find_by_title(&g, "源").unwrap();
        set_aliases(&mut g, target, &["别名甲"]);
        // 源正文只用别名引用目标
        g.get_node_mut(src).unwrap().note = "这里引用了 [[别名甲]]，很重要".to_owned();

        let bls = backlinks_with_context(&g, target);
        assert_eq!(bls.len(), 1, "经别名也应识别为反向链接");
        assert_eq!(bls[0].source, src);
        assert!(bls[0].contexts[0].contains("很重要"));
    }

    #[test]
    fn search_matches_alias() {
        let (mut g, _c) = graph_with(&["机器学习", "无关"]);
        let ml = find_by_title(&g, "机器学习").unwrap();
        set_aliases(&mut g, ml, &["ML"]);
        // 别名命中（大小写不敏感）
        assert_eq!(search(&g, "ml").len(), 1, "搜别名应命中该节点");
        assert_eq!(search(&g, "ML"), vec![ml]);
        // 标题/正文仍各自命中
        assert_eq!(search(&g, "机器").len(), 1);
    }

    #[test]
    fn old_archive_node_without_aliases_field_loads_empty() {
        // 旧 .cnt（无 aliases 字段）兼容验证：仿 EdgeOrigin old_archive 范式——
        // 序列化真实 Node 再剥掉 `aliases` 字段，反序列化应默认空 Vec、不崩。
        let (g, _c) = graph_with(&["A"]);
        let a = find_by_title(&g, "A").unwrap();
        let node = g.get_node(a).unwrap().clone();

        let mut value: serde_json::Value = serde_json::to_value(&node).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("aliases")
            .expect("新节点序列化应含 aliases 字段");

        let old_node: Node = serde_json::from_value(value).expect("旧档节点应能反序列化");
        assert!(
            old_node.aliases.is_empty(),
            "缺 aliases 字段应默认空 Vec（向后兼容）"
        );
        assert_eq!(old_node.text, "A", "其余字段应保真");
    }

    /// 以给定画布坐标建图（节点标题取序号），供 minimap_bbox 的几何单测使用。
    fn graph_with_positions(points: &[(f32, f32)]) -> Graph {
        let canvas = CanvasStateResource::new(CanvasState::default());
        let mut g = Graph::default();
        for (i, (x, y)) in points.iter().enumerate() {
            let id = canvas.read_resource(|c| c.new_node_id());
            g.add_node(Node {
                id,
                position: egui::pos2(*x, *y),
                text: i.to_string(),
                note: String::new(),
                aliases: Vec::new(),
            });
        }
        g
    }

    #[test]
    fn minimap_bbox_empty_is_none() {
        let g = graph_with_positions(&[]);
        assert!(minimap_bbox(&g).is_none(), "空图无包围盒");
    }

    #[test]
    fn minimap_bbox_single_node_is_degenerate_point() {
        let g = graph_with_positions(&[(12.0, -7.0)]);
        let bbox = minimap_bbox(&g).expect("单节点应有包围盒");
        assert_eq!(bbox.min, egui::pos2(12.0, -7.0));
        assert_eq!(bbox.max, egui::pos2(12.0, -7.0));
        // 退化为点：宽高均 0（最小尺寸兜底由调用方负责，本函数如实反映几何）。
        assert_eq!(bbox.width(), 0.0);
        assert_eq!(bbox.height(), 0.0);
    }

    #[test]
    fn minimap_bbox_collinear_has_zero_extent_on_one_axis() {
        // 水平共线：y 恒定 → 高度 0、宽度非 0。
        let gh = graph_with_positions(&[(0.0, 5.0), (10.0, 5.0), (-4.0, 5.0)]);
        let bh = minimap_bbox(&gh).unwrap();
        assert_eq!(bh.min, egui::pos2(-4.0, 5.0));
        assert_eq!(bh.max, egui::pos2(10.0, 5.0));
        assert_eq!(bh.height(), 0.0);
        assert_eq!(bh.width(), 14.0);

        // 垂直共线：x 恒定 → 宽度 0、高度非 0。
        let gv = graph_with_positions(&[(3.0, 0.0), (3.0, 20.0), (3.0, -6.0)]);
        let bv = minimap_bbox(&gv).unwrap();
        assert_eq!(bv.width(), 0.0);
        assert_eq!(bv.height(), 26.0);
    }

    #[test]
    fn minimap_bbox_multi_node_union() {
        let g = graph_with_positions(&[(-10.0, -20.0), (30.0, 5.0), (0.0, 40.0), (15.0, -25.0)]);
        let bbox = minimap_bbox(&g).unwrap();
        assert_eq!(bbox.min, egui::pos2(-10.0, -25.0));
        assert_eq!(bbox.max, egui::pos2(30.0, 40.0));
    }
}
