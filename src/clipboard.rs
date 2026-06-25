//! 节点子图复制 / 粘贴（剪贴板序列化片段）——纯函数层。
//!
//! 这是「框选多节点 Ctrl+C 复制、Ctrl+V 在鼠标处粘贴副本」的数据层：把一段选区导出成一份
//! **自包含、与运行实例解耦**的 JSON 片段（[`copy_subgraph`]），再在任意时刻 / 任意实例把它
//! 重新长回图上（[`paste_subgraph`]）。状态机接线（Ctrl+C/V 事件、系统剪贴板、history 可撤销）
//! 在 `src/input/state_manager.rs`，本文件只管纯粹的「图 ↔ JSON」转换，便于 `#[cfg(test)]` 无头单测。
//!
//! ## 架构契合（AGENTS.md）
//!
//! - **§3.3 索引即句柄、剪贴板绝不存 `NodeIndex`**：导出的边用**选区内局部下标**（`0..n`，按选区
//!   顺序）引用两端，而非全局 `NodeIndex`——`NodeIndex` 不跨实例 / 不跨时间稳定，存进剪贴板就会在
//!   粘贴到另一张图 / 另一次会话时指向错误节点。局部下标在「同一份片段内」自洽，粘贴时映射到新分配的
//!   `NodeIndex`。
//! - **§3.2 数据层存画布坐标**：导出存**相对选区包围盒左上角的偏移** `rel_pos`（画布空间），粘贴时
//!   `position = paste_at + rel_pos`（`paste_at` 也是画布坐标）——整组保持相对布局、并整体平移到鼠标处。
//! - **§3.1 锁作用域**：本文件是纯函数，不持锁；`copy_subgraph` 取 `&Graph`、`paste_subgraph` 取
//!   `&mut Graph`，由调用方在 `read_resource` / `with_resource`（经 `history.mutate`）闭包内调用，
//!   闭包作用域 = 锁作用域。`paste_subgraph` 内 `canvas.new_node_id()` 取的是 **CanvasState** 的锁，
//!   与 Graph 锁是不同资源，不构成同资源重入。
//! - **§6 未新增 `EdgeType`**；§7 剪贴板 JSON 是**独立格式**、不进 `.cnt`、不影响持久化布局。
//!
//! ## 复制范围决策（已定）：节点 + 仅 `Manual` 边，丢弃 `Wiki` 边与跨界边
//!
//! - **只复制两端都在选区内的边**：跨界边（仅一端在选区）语义不明（另一端不在副本里，重建会悬空），
//!   明确丢弃。
//! - **只复制 [`EdgeOrigin::Manual`] 边、不复制 [`EdgeOrigin::Wiki`] 边**：Wiki 边是 `note` 正文
//!   `[[标题]]` 的幂等投影（note = SSOT），由 `wikilink::resolve_links` 在退出编辑时重建。复制时把
//!   它当数据搬运会与「note 是唯一真源」打架——粘贴后下次编辑退出，`resolve_links` 会按副本节点的
//!   `note` 自然投影出 Wiki 边，比硬搬一份更契合架构。故复制阶段只搬手画的 `Manual` 边。

use egui::{Pos2, Vec2};
use petgraph::graph::NodeIndex;

use crate::graph::edge::EdgeOrigin;
use crate::graph::graph_impl::Graph;
use crate::graph::node::Node;
use crate::resource::CanvasStateResource;

/// 剪贴板片段里的一个节点：携带节点的全部用户数据 + **相对选区包围盒左上角的偏移**。
///
/// 不存 `id`（粘贴时由 `CanvasState` 重新分配）、不存绝对 `position`（存 `rel_pos` 以便整体平移）、
/// 不存 `NodeIndex`（§3.3）。字段都加 `#[serde(default)]`：未来片段格式演进时旧片段仍可解析。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
struct ClipNode {
    /// 节点标题（`Node::text`）。
    #[serde(default)]
    text: String,
    /// 节点正文（`Node::note`，note = SSOT）。
    #[serde(default)]
    note: String,
    /// 节点别名（`Node::aliases`）。
    #[serde(default)]
    aliases: Vec<String>,
    /// 相对选区包围盒左上角的画布空间偏移（§3.2）。粘贴时 `position = paste_at + rel_pos`。
    #[serde(default)]
    rel_pos: Vec2,
}

/// 剪贴板片段里的一条边：用**选区内局部下标**引用两端（§3.3，绝不存 `NodeIndex`）。
///
/// `src_local` / `dst_local` 是 `nodes` 数组里的下标（`0..nodes.len()`）。只导出 `Manual` 边且
/// 两端都在选区内，故粘贴时两个下标必然有效（除非片段被外部篡改——解析时再做越界容错）。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
struct ClipEdge {
    /// 源节点在 `nodes` 数组里的局部下标。
    #[serde(default)]
    src_local: usize,
    /// 目标节点在 `nodes` 数组里的局部下标。
    #[serde(default)]
    dst_local: usize,
    /// 边标签文本（`Edge::text`）。
    #[serde(default)]
    text: Option<String>,
}

/// 一份完整的剪贴板片段（复制 / 粘贴的载荷）。是与运行实例解耦的自包含单元。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
struct ClipFragment {
    #[serde(default)]
    nodes: Vec<ClipNode>,
    #[serde(default)]
    edges: Vec<ClipEdge>,
}

/// 把选区 `sel` 内的子图导出为一份自包含 JSON 片段。
///
/// - **节点**：按 `sel` 给定顺序导出（去重、跳过已失效索引），坐标存 `rel_pos = position - bbox_min`
///   （`bbox_min` = 选区所有节点 position 的逐分量最小值，即包围盒左上角）。
/// - **边**：只导出**两端都在选区内**且 `origin == Manual` 的边（§ 模块文档的范围决策），用局部下标
///   引用两端。跨界边、`Wiki` 边一律丢弃。
///
/// 空选 / 无有效节点 → 返回 `{"nodes":[],"edges":[]}`（合法空片段，粘贴时得空 `Vec`）。序列化
/// 理论上不会失败（全是 POD + String）；万一失败返回空片段字符串而非 panic。
pub fn copy_subgraph(graph: &Graph, sel: &[NodeIndex]) -> String {
    use petgraph::visit::{EdgeRef, IntoEdgeReferences};

    // 1. 选区去重 + 过滤失效索引，固定一个「局部下标顺序」（NodeIndex -> 局部下标）。
    //    用 Vec 保序 + HashMap 查下标，与 layout.rs 的 index_of 同款（§3.3 局部下标）。
    let mut local_order: Vec<NodeIndex> = Vec::with_capacity(sel.len());
    let mut local_of: std::collections::HashMap<NodeIndex, usize> =
        std::collections::HashMap::with_capacity(sel.len());
    for &idx in sel {
        if local_of.contains_key(&idx) {
            continue; // 去重：同一节点在选区里只出现一次。
        }
        if graph.get_node(idx).is_none() {
            continue; // 容错：跳过已被删的悬空索引（§3.3），不 panic。
        }
        local_of.insert(idx, local_order.len());
        local_order.push(idx);
    }

    // 2. 计算选区包围盒左上角（逐分量最小 position）。空选区时 bbox 无意义，留空片段。
    if local_order.is_empty() {
        return serde_json::to_string(&ClipFragment::default()).unwrap_or_else(|_| {
            // 静态空片段不可能序列化失败，这里只是为了不 unwrap panic 的兜底。
            String::from(r#"{"nodes":[],"edges":[]}"#)
        });
    }
    let mut bbox_min = Pos2::new(f32::INFINITY, f32::INFINITY);
    for &idx in &local_order {
        let p = graph.get_node(idx).map(|n| n.position).unwrap_or_default();
        bbox_min.x = bbox_min.x.min(p.x);
        bbox_min.y = bbox_min.y.min(p.y);
    }

    // 3. 导出节点（rel_pos = position - bbox_min）。
    let nodes: Vec<ClipNode> = local_order
        .iter()
        .map(|&idx| {
            let node = graph.get_node(idx).expect("已在上面过滤掉失效索引");
            ClipNode {
                text: node.text.clone(),
                note: node.note.clone(),
                aliases: node.aliases.clone(),
                rel_pos: node.position - bbox_min,
            }
        })
        .collect();

    // 4. 导出边：仅 Manual 边、且两端都在选区内（用局部下标引用）。跨界 / Wiki 边丢弃。
    let edges: Vec<ClipEdge> = graph
        .graph
        .edge_references()
        .filter(|e| e.weight().origin == EdgeOrigin::Manual)
        .filter_map(|e| {
            let src_local = *local_of.get(&e.source())?;
            let dst_local = *local_of.get(&e.target())?;
            Some(ClipEdge {
                src_local,
                dst_local,
                text: e.weight().text.clone(),
            })
        })
        .collect();

    let fragment = ClipFragment { nodes, edges };
    serde_json::to_string(&fragment).unwrap_or_else(|_| String::from(r#"{"nodes":[],"edges":[]}"#))
}

/// 把一份 JSON 片段粘贴到图上：重分配 id、整体平移到 `paste_at`、按局部下标重建 Manual 边。
///
/// - **每个节点**：经 `canvas.new_node_id()` 分配全新 `id`（§3.3 业务 id 与拓扑索引解耦），
///   `position = paste_at + rel_pos`（画布坐标，§3.2），`text/note/aliases` 原样搬运 → `add_node`
///   得到新 `NodeIndex`，按局部下标记入映射表。
/// - **每条边**：用局部下标查映射表得两端新 `NodeIndex`，构造一条 `Manual` 边（`Edge::new`）加入图。
///   边的 `id` 同样经 `CanvasState` 新分配（在 `Edge::new` 内部）。
///
/// 返回**新建节点的 `NodeIndex` 列表**（按片段里节点顺序），供调用方粘贴后整体选中。
///
/// 容错（§3.3）：JSON 解析失败 → 返回空 `Vec`、不 panic、不改图；边的局部下标越界 → 跳过该边
/// （不 panic），其余照常重建。
pub fn paste_subgraph(
    graph: &mut Graph,
    canvas: &CanvasStateResource,
    json: &str,
    paste_at: Pos2,
) -> Vec<NodeIndex> {
    // 1. 解析片段（坏 JSON → 空 Vec，不改图、不 panic）。
    let fragment: ClipFragment = match serde_json::from_str(json) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("paste_subgraph: 剪贴板内容不是合法片段 JSON，忽略：{e}");
            return Vec::new();
        }
    };

    // 2. 重建节点：新 id + 整体平移，记录「局部下标 -> 新 NodeIndex」。
    let mut new_indices: Vec<NodeIndex> = Vec::with_capacity(fragment.nodes.len());
    for clip_node in &fragment.nodes {
        // new_node_id 取的是 CanvasState 的锁（与 Graph 锁不同资源，§3.1 不构成同资源重入）。
        let new_id = canvas.read_resource(|cs| cs.new_node_id());
        let node = Node {
            id: new_id,
            position: paste_at + clip_node.rel_pos,
            text: clip_node.text.clone(),
            note: clip_node.note.clone(),
            aliases: clip_node.aliases.clone(),
        };
        new_indices.push(graph.add_node(node));
    }

    // 3. 重建 Manual 边：用局部下标映射到新 NodeIndex，越界则跳过（容错，不 panic）。
    for clip_edge in &fragment.edges {
        let (Some(&src), Some(&dst)) = (
            new_indices.get(clip_edge.src_local),
            new_indices.get(clip_edge.dst_local),
        ) else {
            log::warn!(
                "paste_subgraph: 边局部下标越界（src={}, dst={}, n={}），跳过该边",
                clip_edge.src_local,
                clip_edge.dst_local,
                new_indices.len()
            );
            continue;
        };
        let (src_pos, dst_pos) = (
            graph.get_node(src).map(|n| n.position).unwrap_or_default(),
            graph.get_node(dst).map(|n| n.position).unwrap_or_default(),
        );
        let mut edge = crate::graph::edge::Edge::new(src, dst, src_pos, dst_pos, canvas.clone());
        // 保留原边标签（Edge::new 默认 text=None）。
        edge.text = clip_edge.text.clone();
        graph.add_edge(edge);
    }

    new_indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::{Edge, EdgeOrigin};
    use crate::graph::node::Node;
    use crate::resource::CanvasStateResource;

    fn node_at(text: &str, x: f32, y: f32) -> Node {
        Node {
            id: 0,
            position: egui::pos2(x, y),
            text: text.to_owned(),
            note: format!("note-{text}"),
            aliases: vec![format!("alias-{text}")],
        }
    }

    /// 与生产一致：节点 id 从 `canvas.new_node_id()` 计数器分配（复刻 state_manager 建点路径）。
    /// 用于需要「原节点 id 真实、与粘贴新分配 id 同源」的测试（验证 id 不重叠）。
    fn node_via_canvas(canvas: &CanvasStateResource, text: &str, x: f32, y: f32) -> Node {
        Node {
            id: canvas.read_resource(|cs| cs.new_node_id()),
            position: egui::pos2(x, y),
            text: text.to_owned(),
            note: format!("note-{text}"),
            aliases: vec![format!("alias-{text}")],
        }
    }

    /// 在 `g` 上加一条 Manual 边（默认 origin）。
    fn add_manual_edge(
        g: &mut Graph,
        src: NodeIndex,
        dst: NodeIndex,
        canvas: &CanvasStateResource,
    ) {
        let sp = g.get_node(src).map(|n| n.position).unwrap_or_default();
        let dp = g.get_node(dst).map(|n| n.position).unwrap_or_default();
        g.add_edge(Edge::new(src, dst, sp, dp, canvas.clone()));
    }

    /// 在 `g` 上加一条 Wiki 边。
    fn add_wiki_edge(g: &mut Graph, src: NodeIndex, dst: NodeIndex, canvas: &CanvasStateResource) {
        let sp = g.get_node(src).map(|n| n.position).unwrap_or_default();
        let dp = g.get_node(dst).map(|n| n.position).unwrap_or_default();
        g.add_edge(Edge::new_wiki(src, dst, sp, dp, canvas.clone()));
    }

    fn count_edges(g: &Graph) -> usize {
        g.graph.edge_count()
    }

    /// copy → paste round-trip：节点数 / Manual 边数 / 相对布局保持、id 全新、跨界边被丢。
    #[test]
    fn copy_paste_roundtrip_preserves_nodes_manual_edges_and_relative_layout() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        // 三个选区内节点 a(10,10) b(40,30) c(10,60)，一个选区外节点 d(200,200)。
        // id 经 canvas 计数器分配（与生产同源），以验证粘贴新分配的 id 与原 id 不重叠。
        let a = g.add_node(node_via_canvas(&canvas, "a", 10.0, 10.0));
        let b = g.add_node(node_via_canvas(&canvas, "b", 40.0, 30.0));
        let c = g.add_node(node_via_canvas(&canvas, "c", 10.0, 60.0));
        let d = g.add_node(node_via_canvas(&canvas, "d", 200.0, 200.0));
        // 内部 Manual 边 a->b、b->c；跨界 Manual 边 c->d（d 不在选区）；内部 Wiki 边 a->c。
        add_manual_edge(&mut g, a, b, &canvas);
        add_manual_edge(&mut g, b, c, &canvas);
        add_manual_edge(&mut g, c, d, &canvas);
        add_wiki_edge(&mut g, a, c, &canvas);

        let sel = vec![a, b, c];
        let json = copy_subgraph(&g, &sel);

        // 粘贴到 (1000, 1000)。bbox_min = (10,10)，故 a 落在 paste_at + (0,0)。
        let paste_at = egui::pos2(1000.0, 1000.0);
        let before_nodes = g.graph.node_count();
        let before_edges = count_edges(&g);
        let new_ids = paste_subgraph(&mut g, &canvas, &json, paste_at);

        // 3 个新节点。
        assert_eq!(new_ids.len(), 3, "应粘贴 3 个节点");
        assert_eq!(g.graph.node_count(), before_nodes + 3);
        // 内部 Manual 边 2 条（a->b, b->c）；跨界 c->d 丢、Wiki a->c 丢。
        assert_eq!(
            count_edges(&g),
            before_edges + 2,
            "只重建 2 条内部 Manual 边（跨界边 + Wiki 边都被丢）"
        );

        // 相对布局保持：新 a/b/c 相对位移 = 原 a/b/c 相对位移。
        let na = g.get_node(new_ids[0]).unwrap();
        let nb = g.get_node(new_ids[1]).unwrap();
        let nc = g.get_node(new_ids[2]).unwrap();
        assert_eq!(
            na.position,
            egui::pos2(1000.0, 1000.0),
            "a 落在 paste_at（bbox 原点）"
        );
        assert_eq!(
            nb.position,
            egui::pos2(1030.0, 1020.0),
            "b 保持相对 a 的 (30,20)"
        );
        assert_eq!(
            nc.position,
            egui::pos2(1000.0, 1050.0),
            "c 保持相对 a 的 (0,50)"
        );

        // id 全新：新节点 id 与原节点 id 不重叠。
        let new_id_set: std::collections::HashSet<u64> =
            new_ids.iter().map(|&i| g.get_node(i).unwrap().id).collect();
        let old_id_set: std::collections::HashSet<u64> = [a, b, c, d]
            .iter()
            .map(|&i| g.get_node(i).unwrap().id)
            .collect();
        assert!(
            new_id_set.is_disjoint(&old_id_set),
            "粘贴节点 id 必须全新，与原节点不重叠"
        );

        // 用户数据原样搬运。
        assert_eq!(na.text, "a");
        assert_eq!(na.note, "note-a");
        assert_eq!(na.aliases, vec!["alias-a".to_string()]);

        // 新边连的是新节点（拓扑保持）：新 a->b、新 b->c 存在。
        assert!(g.edge_exists(new_ids[0], new_ids[1]), "新 a->b 应存在");
        assert!(g.edge_exists(new_ids[1], new_ids[2]), "新 b->c 应存在");
    }

    /// 粘贴重建的边都是 Manual origin（即便原图里掺了 Wiki 边，Wiki 不被复制）。
    #[test]
    fn pasted_edges_are_all_manual() {
        use petgraph::visit::{EdgeRef, IntoEdgeReferences};
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        let a = g.add_node(node_at("a", 0.0, 0.0));
        let b = g.add_node(node_at("b", 50.0, 0.0));
        add_manual_edge(&mut g, a, b, &canvas);

        let json = copy_subgraph(&g, &[a, b]);
        let new_ids = paste_subgraph(&mut g, &canvas, &json, egui::pos2(500.0, 500.0));

        // 新建边（连新节点）必为 Manual。
        let pasted_edges: Vec<_> = g
            .graph
            .edge_references()
            .filter(|e| new_ids.contains(&e.source()) && new_ids.contains(&e.target()))
            .collect();
        assert_eq!(pasted_edges.len(), 1);
        assert_eq!(pasted_edges[0].weight().origin, EdgeOrigin::Manual);
    }

    /// 空选：copy 得合法空片段，paste 得空 Vec、不改图。
    #[test]
    fn empty_selection_yields_empty_fragment_and_no_paste() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        g.add_node(node_at("a", 0.0, 0.0));

        let json = copy_subgraph(&g, &[]);
        let before = g.graph.node_count();
        let new_ids = paste_subgraph(&mut g, &canvas, &json, egui::pos2(0.0, 0.0));
        assert!(new_ids.is_empty(), "空片段粘贴应得空 Vec");
        assert_eq!(g.graph.node_count(), before, "空粘贴不改图");
    }

    /// 坏 JSON：paste 返回空 Vec、不 panic、不改图。
    #[test]
    fn bad_json_is_tolerated() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        g.add_node(node_at("a", 0.0, 0.0));
        let before = g.graph.node_count();

        for bad in ["", "not json", "{", r#"{"nodes": 42}"#, "[]"] {
            let new_ids = paste_subgraph(&mut g, &canvas, bad, egui::pos2(0.0, 0.0));
            assert!(new_ids.is_empty(), "坏 JSON {bad:?} 应得空 Vec");
        }
        assert_eq!(g.graph.node_count(), before, "坏 JSON 不改图");
    }

    /// 单节点无边：round-trip 得 1 个新节点、0 条边。
    #[test]
    fn single_node_no_edge_roundtrip() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        let a = g.add_node(node_at("solo", 7.0, 9.0));

        let json = copy_subgraph(&g, &[a]);
        let new_ids = paste_subgraph(&mut g, &canvas, &json, egui::pos2(100.0, 200.0));
        assert_eq!(new_ids.len(), 1);
        assert_eq!(count_edges(&g), 0, "单节点无边");
        // 单节点 bbox 原点 = 自身，故落在 paste_at。
        assert_eq!(
            g.get_node(new_ids[0]).unwrap().position,
            egui::pos2(100.0, 200.0)
        );
    }

    /// 越界局部下标的边被跳过、不 panic（防外部篡改片段）。
    #[test]
    fn out_of_bounds_local_index_edge_is_skipped() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        // 手搓一份只含 1 个节点、但边引用下标 5 的非法片段。
        let json = r#"{"nodes":[{"text":"a","note":"","aliases":[],"rel_pos":[0.0,0.0]}],"edges":[{"src_local":0,"dst_local":5,"text":null}]}"#;
        let new_ids = paste_subgraph(&mut g, &canvas, json, egui::pos2(0.0, 0.0));
        assert_eq!(new_ids.len(), 1, "1 个节点照常粘贴");
        assert_eq!(count_edges(&g), 0, "越界边被跳过，不 panic");
    }

    /// 选区里含重复 / 失效索引时去重并跳过失效，不影响导出。
    #[test]
    fn copy_dedups_and_skips_stale_indices() {
        let canvas = CanvasStateResource::default();
        let mut g = Graph::default();
        let a = g.add_node(node_at("a", 0.0, 0.0));
        let b = g.add_node(node_at("b", 30.0, 0.0));
        g.remove_node(b); // b 成为悬空索引。
        add_manual_edge(&mut g, a, a, &canvas); // 自环（两端都是 a）便于验证去重后仍能重建。

        // 选区含重复 a 和悬空 b。
        let json = copy_subgraph(&g, &[a, a, b]);
        let new_ids = paste_subgraph(&mut g, &canvas, &json, egui::pos2(500.0, 500.0));
        assert_eq!(new_ids.len(), 1, "去重 a、跳过悬空 b，只导出 1 个节点");
    }
}
