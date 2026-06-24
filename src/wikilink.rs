//! Wikilink / 双链引擎（模型层，纯逻辑、不依赖 GUI，可无头测试）。
//!
//! "第二大脑"的核心差异化：在节点正文里写 `[[标题]]` 即自动在图上长出连接——
//! 找到同名节点就连过去，没有就新建一个再连。配套提供反向链接（谁链接到我）与全文搜索。
//!
//! 这一层只操作 [`Graph`] 数据与 id 分配（经 [`CanvasStateResource`]），渲染/交互留给 UI 层。

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

/// 按精确标题查找第一个匹配的节点。
pub fn find_by_title(graph: &Graph, title: &str) -> Option<NodeIndex> {
    graph
        .graph
        .node_indices()
        .find(|&i| graph.graph[i].text == title)
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
        let matches: Vec<NodeIndex> = graph
            .graph
            .node_indices()
            .filter(|&idx| graph.graph[idx].text == title)
            .collect();

        let target = if let Some(&first) = matches.first() {
            if matches.len() > 1 {
                log::warn!(
                    "wikilink: 标题 \"{title}\" 匹配到 {} 个节点，连接到第一个（其余忽略）",
                    matches.len()
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
/// 找出所有正文里出现 `[[target 的标题]]` 的节点，并附上包含该链接的上下文行。
/// 以**当前标题**匹配（target 改名后旧链接不再命中，与 Obsidian 一致）；空标题节点无反链。
pub fn backlinks_with_context(graph: &Graph, target: NodeIndex) -> Vec<Backlink> {
    let target_title = match graph.get_node(target) {
        Some(n) if !n.text.is_empty() => n.text.clone(),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for src_idx in graph.graph.node_indices() {
        if src_idx == target {
            continue;
        }
        let src = &graph.graph[src_idx];
        let contexts: Vec<String> = src
            .note
            .lines()
            .filter(|line| parse_links(line).iter().any(|t| t == &target_title))
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

/// 全文搜索：标题或正文包含 `query`（**大小写不敏感**的子串匹配）的节点，按节点索引顺序返回。
pub fn search(graph: &Graph, query: &str) -> Vec<NodeIndex> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_lowercase();
    graph
        .graph
        .node_indices()
        .filter(|&i| {
            let n = &graph.graph[i];
            n.text.to_lowercase().contains(&q) || n.note.to_lowercase().contains(&q)
        })
        .collect()
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
}
