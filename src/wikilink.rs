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

/// 把 `source` 节点正文里的 `[[标题]]` 落到图上。
///
/// 对每个标题：找到同名节点则复用，否则在 source 附近新建一个；随后确保存在 `source -> target` 的边
/// （已存在则不重复建、不自指）。返回新建的节点与边数，供调用方反馈/测试断言。
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

        if source != target && !graph.edge_exists(source, target) {
            let target_pos = graph.get_node(target).map_or(source_pos, |n| n.position);
            graph.add_edge(Edge::new(
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
