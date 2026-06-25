use std::sync::Arc;

use crate::graph::node::Node;
use crate::history::History;
use crate::resource::{CanvasStateResource, GraphResource};
use crate::ui::bezier::BezierEdge;
use crate::ui::edge::EdgeWidget;
use crate::ui::line_edge::LineEdge;
use crate::ui::node_render_observer::NodeRenderObserver;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};

use crate::ui::node::NodeWidget;

use super::edge::{Edge, EdgeType};
use super::selection::GraphSelection;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Graph {
    pub edge_type: EdgeType,
    pub graph: petgraph::stable_graph::StableGraph<Node, Edge>,
    #[serde(skip)]
    pub selected: GraphSelection,
    #[serde(skip)]
    pub editing_node: Option<NodeIndex>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            edge_type: EdgeType::Line,
            graph: petgraph::stable_graph::StableGraph::new(),
            selected: GraphSelection::None,
            editing_node: None,
        }
    }
}

impl Graph {
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        self.graph.add_node(node)
    }

    pub fn add_node_with_edge(
        &mut self,
        node: Node,
        src_node_index: NodeIndex,
        canvas_state_resource: CanvasStateResource,
    ) -> NodeIndex {
        let node = node.clone();
        let dst_node_index = self.add_node(node.clone());
        let src_node = self.get_node(src_node_index).unwrap();

        self.add_edge(Edge::new(
            src_node_index,
            dst_node_index,
            src_node.position,
            node.clone().position,
            canvas_state_resource,
        ));
        dst_node_index
    }

    pub fn get_node(&self, node_index: NodeIndex) -> Option<&Node> {
        self.graph.node_weight(node_index)
    }

    pub fn get_node_mut(&mut self, node_index: NodeIndex) -> Option<&mut Node> {
        self.graph.node_weight_mut(node_index)
    }

    pub fn get_selected_nodes(&self) -> Vec<NodeIndex> {
        match &self.selected {
            GraphSelection::Node(nodes) => nodes.clone(),
            _ => vec![],
        }
    }

    /// 当前选中的边（选区类型非 Edge 时返回空）。与 [`Self::get_selected_nodes`] 对称，
    /// 供右侧面板按"恰好选中一条边"展示标签编辑用。
    pub fn get_selected_edges(&self) -> Vec<EdgeIndex> {
        match &self.selected {
            GraphSelection::Edge(edges) => edges.clone(),
            _ => vec![],
        }
    }

    pub fn is_node_selected(&self, node_index: NodeIndex) -> bool {
        match &self.selected {
            GraphSelection::Node(nodes) => nodes.contains(&node_index),
            _ => false,
        }
    }

    pub fn select_node(&mut self, node_index: NodeIndex) {
        match &mut self.selected {
            GraphSelection::Node(nodes) => nodes.push(node_index),
            _ => self.selected = GraphSelection::Node(vec![node_index]),
        }
    }

    pub fn select_nodes(&mut self, nodes: Vec<NodeIndex>) {
        match &mut self.selected {
            GraphSelection::Node(selected_nodes) => selected_nodes.extend(nodes),
            _ => self.selected = GraphSelection::Node(nodes),
        }
    }

    pub fn get_editing_node(&self) -> Option<NodeIndex> {
        self.editing_node
    }

    pub fn set_editing_node(&mut self, node_index: Option<NodeIndex>) {
        self.editing_node = node_index;
    }

    pub fn remove_node(&mut self, node_index: NodeIndex) {
        let result = self.graph.remove_node(node_index);
        log::debug!("remove_node result: {result:?}");
        // self.selected_nodes.clear();
        self.editing_node = None;
    }

    /// 把当前选区（节点或边）整体从图里删除，返回实际删除的元素数。
    ///
    /// 纯图层、无副作用之外只动 `self.graph` / `self.selected` / `self.editing_node`，
    /// 可无头单测（仿 wikilink / layout 的 `#[cfg(test)]` 风格）。UI（Edit 菜单 / 右键删除）
    /// 只需把它包进 `History::mutate` 即可成为一个可撤销单元。
    ///
    /// 语义（AGENTS.md §3.3）：
    /// - 节点选区：逐个 `remove_node`——`StableGraph::remove_node` 会**连带删除**该节点的所有
    ///   邻接边，故无需手动清边；删除后 `EdgeIndex` 对其余边保持稳定。
    /// - 边选区：逐个 `remove_edge`，只删边、不动端点节点。
    /// - 删除后**必须清空 `selected`**：选区里残留的索引已悬空，留着会被后续渲染/命中以
    ///   `.unwrap()` 读取而 panic（§3.3）。`editing_node` 由 `remove_node` 顺带清空，这里再兜底清一次。
    pub fn remove_selected(&mut self) -> usize {
        let removed = match std::mem::take(&mut self.selected) {
            GraphSelection::Node(nodes) => {
                let mut n = 0;
                for node_index in nodes {
                    // 容错：选区里可能有已被其它操作删掉的悬空索引——remove_node 对不存在的
                    // 索引返回 None、不 panic（§3.3）。只对真正删掉的计数。
                    if self.graph.remove_node(node_index).is_some() {
                        n += 1;
                    }
                }
                n
            }
            GraphSelection::Edge(edges) => {
                let mut n = 0;
                for edge_index in edges {
                    if self.graph.remove_edge(edge_index).is_some() {
                        n += 1;
                    }
                }
                n
            }
            GraphSelection::None => 0,
        };
        // 选区已被 take 置为 None（清空），这里再显式收口 editing（删点路径已清，删边路径未触及）。
        self.editing_node = None;
        removed
    }

    /// 全选所有节点（替换当前选区为"全部节点"的节点选区）。纯图层，可无头单测。
    ///
    /// 右键空白菜单「全选」复用它。空图时选区被置为空的节点选区（语义等价于无选中）。
    pub fn select_all_nodes(&mut self) {
        let all: Vec<NodeIndex> = self.graph.node_indices().collect();
        self.selected = GraphSelection::Node(all);
    }
}

impl Graph {
    pub fn select_edge(&mut self, edge_index: EdgeIndex) {
        match &mut self.selected {
            GraphSelection::Edge(edges) => edges.push(edge_index),
            _ => self.selected = GraphSelection::Edge(vec![edge_index]),
        }
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.graph.add_edge(edge.source, edge.target, edge);
    }

    pub fn get_edge(&self, edge_index: EdgeIndex) -> Option<&Edge> {
        self.graph.edge_weight(edge_index)
    }

    pub fn remove_edge(&mut self, edge_index: EdgeIndex) {
        self.graph.remove_edge(edge_index);
    }

    pub fn update_bezier_edge(&mut self, edge_index: EdgeIndex, bezier_edge: BezierEdge) {
        let edge = self.graph.edge_weight_mut(edge_index).unwrap();
        edge.bezier_edge = bezier_edge;
    }

    pub fn update_line_edge(&mut self, edge_index: EdgeIndex, line_edge: LineEdge) {
        let edge = self.graph.edge_weight_mut(edge_index).unwrap();
        edge.line_edge = line_edge;
    }

    /// 写回一条边的标签文本（空 → `None`，归一为"无标签"）。§3.3 失效容错：边已被删则静默跳过、
    /// 不 panic（不同于上面两个 `unwrap` 的几何回写——标签编辑入口可能持有失效选区）。
    pub fn update_edge_text(&mut self, edge_index: EdgeIndex, text: Option<String>) {
        if let Some(edge) = self.graph.edge_weight_mut(edge_index) {
            edge.text = text;
        }
    }

    pub fn edge_exists(&self, src_node_index: NodeIndex, dst_node_index: NodeIndex) -> bool {
        self.graph.contains_edge(src_node_index, dst_node_index)
    }

    /// 删除 `source` 发出的**过时** wikilink 自动边（[`EdgeOrigin::Wiki`]）：目标不在 `keep` 集
    /// 里的那些。返回删除的条数。
    ///
    /// 仅触碰 `Wiki` 边——手画的 [`EdgeOrigin::Manual`] 边永不被删。这是 `resolve_links` 把
    /// "出链 wiki 边"做成 note 幂等投影的差量删除半步；命中 `keep` 的 wiki 边原地保留，
    /// 其 `EdgeIndex` 不抖动。
    pub fn remove_stale_wiki_edges_from(&mut self, source: NodeIndex, keep: &[NodeIndex]) -> usize {
        use crate::graph::edge::EdgeOrigin;
        // 先收集再删，避免在遍历期间结构性修改。EdgeIndex 在 StableGraph 下删边后保持稳定。
        let to_remove: Vec<EdgeIndex> = self
            .graph
            .edges_directed(source, petgraph::Direction::Outgoing)
            .filter(|e| e.weight().origin == EdgeOrigin::Wiki && !keep.contains(&e.target()))
            .map(|e| e.id())
            .collect();
        for eidx in &to_remove {
            self.graph.remove_edge(*eidx);
        }
        to_remove.len()
    }

    pub fn edge_count_undirected(&self, node1_index: NodeIndex, node2_index: NodeIndex) -> usize {
        self.graph
            .edge_references()
            .filter(|edge| {
                edge.source() == node1_index && edge.target() == node2_index
                    || edge.source() == node2_index && edge.target() == node1_index
            })
            .count()
    }
}

impl Graph {
    pub fn reset(&mut self) {
        self.graph = petgraph::stable_graph::StableGraph::new();
        self.selected = GraphSelection::None;
        self.editing_node = None;
    }
}

pub fn render_graph(
    ui: &mut egui::Ui,
    graph_resource: GraphResource,
    canvas_state_resource: CanvasStateResource,
    history: History,
) {
    // 过滤可见度判定集中在此一处（§3.3 / §3.5）：算一张"本帧哪些节点可见"的快照写入 temp
    // data，下面的 EdgeWidget / NodeWidget 与 hit_test 反读，避免每个 widget 各自重算 O(N)
    // 全文搜索。可见集复用现成 wikilink::search；query 空 → 全部可见。§3.1：图只读经
    // read_resource 闭包（作用域 = 锁作用域），闭包内不再取同一锁。
    graph_resource.read_resource(|graph| {
        crate::graph::filter::publish_filter_visibility(ui.ctx(), graph);
    });

    let node_indices = graph_resource
        .read_resource(|graph| graph.graph.node_indices().collect::<Vec<NodeIndex>>());

    // println!("node_indices: {:?}", node_indices.len());

    let edge_indices = graph_resource
        .read_resource(|graph| graph.graph.edge_indices().collect::<Vec<EdgeIndex>>());

    for edge_index in edge_indices {
        ui.add(EdgeWidget {
            edge_index,
            graph_resource: graph_resource.clone(),
            canvas_state_resource: canvas_state_resource.clone(),
        });
    }

    for node_index in node_indices {
        // println!("node: {}", node_index.index());
        // Put the node id into the ui

        let mut node_widget = NodeWidget::new(
            node_index,
            graph_resource.clone(),
            canvas_state_resource.clone(),
            history.clone(),
        );
        node_widget.add_observer(Arc::new(NodeRenderObserver::new(ui.ctx().clone())));
        ui.add(node_widget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::node::Node;
    use crate::resource::CanvasStateResource;

    fn node(text: &str) -> Node {
        Node {
            id: 0,
            position: egui::pos2(0.0, 0.0),
            text: text.to_owned(),
            note: String::new(),
            aliases: Vec::new(),
        }
    }

    fn edge(g: &Graph, src: NodeIndex, dst: NodeIndex) -> Edge {
        let sp = g.get_node(src).map(|n| n.position).unwrap_or_default();
        let dp = g.get_node(dst).map(|n| n.position).unwrap_or_default();
        Edge::new(src, dst, sp, dp, CanvasStateResource::default())
    }

    /// 删除节点选区：连同邻接边一并删除，选区清空，返回删除节点数。
    #[test]
    fn remove_selected_nodes_drops_adjacent_edges_and_clears_selection() {
        let mut g = Graph::default();
        let a = g.add_node(node("a"));
        let b = g.add_node(node("b"));
        let c = g.add_node(node("c"));
        let e_ab = edge(&g, a, b);
        let e_bc = edge(&g, b, c);
        g.add_edge(e_ab);
        g.add_edge(e_bc);
        assert_eq!(g.graph.node_count(), 3);
        assert_eq!(g.graph.edge_count(), 2);

        // 选中 a 和 b 两个节点。
        g.selected = GraphSelection::Node(vec![a, b]);
        let removed = g.remove_selected();

        assert_eq!(removed, 2, "应删除 2 个节点");
        assert_eq!(g.graph.node_count(), 1, "只剩 c");
        // a-b 边随 a/b 删除；b-c 边随 b 删除——两条边全没了。
        assert_eq!(g.graph.edge_count(), 0, "邻接边应随节点一并删除");
        assert!(g.get_node(c).is_some(), "c 应保留");
        assert!(
            matches!(g.selected, GraphSelection::None),
            "删除后选区必须清空（防悬空索引）"
        );
    }

    /// 删除边选区：只删边、保留端点节点，选区清空，返回删除边数。
    #[test]
    fn remove_selected_edges_keeps_nodes_and_clears_selection() {
        let mut g = Graph::default();
        let a = g.add_node(node("a"));
        let b = g.add_node(node("b"));
        let c = g.add_node(node("c"));
        let e_ab_w = edge(&g, a, b);
        let e_bc_w = edge(&g, b, c);
        g.add_edge(e_ab_w);
        g.add_edge(e_bc_w);
        let e_ab = g.graph.find_edge(a, b).unwrap();
        let e_bc = g.graph.find_edge(b, c).unwrap();

        g.selected = GraphSelection::Edge(vec![e_ab, e_bc]);
        let removed = g.remove_selected();

        assert_eq!(removed, 2, "应删除 2 条边");
        assert_eq!(g.graph.edge_count(), 0, "两条边都删掉");
        assert_eq!(g.graph.node_count(), 3, "端点节点不动");
        assert!(matches!(g.selected, GraphSelection::None));
    }

    /// 空选区删除：无操作，返回 0，不 panic。
    #[test]
    fn remove_selected_none_is_noop() {
        let mut g = Graph::default();
        g.add_node(node("a"));
        assert_eq!(g.remove_selected(), 0);
        assert_eq!(g.graph.node_count(), 1);
    }

    /// 选区含悬空索引（已被删的节点）时容错：跳过、只对真正删掉的计数，不 panic（§3.3）。
    #[test]
    fn remove_selected_tolerates_stale_indices() {
        let mut g = Graph::default();
        let a = g.add_node(node("a"));
        let b = g.add_node(node("b"));
        // 先删 b，使 b 成为悬空索引。
        g.remove_node(b);
        // 选区里同时含活索引 a 与悬空索引 b。
        g.selected = GraphSelection::Node(vec![a, b]);
        let removed = g.remove_selected();
        assert_eq!(removed, 1, "只有 a 真正被删，悬空索引被跳过");
        assert_eq!(g.graph.node_count(), 0);
        assert!(matches!(g.selected, GraphSelection::None));
    }

    /// 全选：选区变为含全部节点的节点选区。
    #[test]
    fn select_all_nodes_selects_every_node() {
        let mut g = Graph::default();
        let a = g.add_node(node("a"));
        let b = g.add_node(node("b"));
        g.select_all_nodes();
        match &g.selected {
            GraphSelection::Node(ns) => {
                assert_eq!(ns.len(), 2);
                assert!(ns.contains(&a) && ns.contains(&b));
            }
            _ => panic!("全选后应为节点选区"),
        }
    }
}
