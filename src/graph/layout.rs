//! 力导向自动布局（模型层，纯逻辑、不依赖 GUI，可无头测试）。
//!
//! "一键整理画布"：把图按连接关系自然铺开——相连的靠近、不相连的分散、尽量少重叠。
//! 实现经典 Fruchterman–Reingold（弹簧-斥力 + 降温），无新依赖、纯 Rust 自实现。
//!
//! 设计为**纯函数** [`force_directed_layout`]：输入节点集合 + 边列表 + 初始位置，输出新位置 map，
//! 与 egui / [`crate::resource::Resource`] 完全解耦，便于单测。UI 层经 [`layout_graph`] 把它接到
//! [`crate::graph::graph_impl::Graph`]（读拓扑、写回 `Node.position`，全程画布坐标，AGENTS.md §3.2）。

use std::collections::HashMap;

use petgraph::graph::NodeIndex;

use crate::graph::graph_impl::Graph;

/// 力导向布局参数。`Default` 给出对典型笔记图（数十节点）观感不错的一组取值。
#[derive(Debug, Clone, Copy)]
pub struct LayoutParams {
    /// 理想边长（画布坐标单位）。两端相连节点会被弹簧拉向此距离。
    pub ideal_length: f32,
    /// 迭代步数上限（防止节点很多时卡死 UI）。
    pub iterations: usize,
    /// 初始温度（单步最大位移），随迭代线性降温到 0。
    pub initial_temperature: f32,
    /// 重叠/重合时用于打散的最小距离下限（避免除零、避免坍缩成一点）。
    pub min_distance: f32,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            ideal_length: 220.0,
            iterations: 300,
            initial_temperature: 320.0,
            min_distance: 1.0,
        }
    }
}

/// 纯函数力导向布局：给定节点、无向边（以下标对表示）与初始位置，返回每个节点的新位置。
///
/// - `nodes`：参与布局的节点索引（顺序即下标，`positions`/边引用都以这个下标系。
/// - `edges`：边的 `(a, b)`，`a`/`b` 是 `nodes` 里的下标（非 `NodeIndex`），自环与越界对被忽略。
/// - `positions`：与 `nodes` 等长的初始位置；若某项是 NaN/Inf 会被替换为一个确定性散布点。
///
/// 算法：Fruchterman–Reingold——所有节点对之间斥力 `k^2 / d`，每条边引力 `d^2 / k`
/// （`k` = 理想边长），合力按温度钳幅后落位，温度逐步线性降到 0（cooling schedule）。
/// 重合点用确定性微扰打散，保证不会全部坍缩到同一点，且输出永不含 NaN/Inf。
pub fn force_directed_layout(
    nodes: &[NodeIndex],
    edges: &[(usize, usize)],
    positions: &[egui::Pos2],
    params: LayoutParams,
) -> Vec<egui::Pos2> {
    let n = nodes.len();
    debug_assert_eq!(n, positions.len(), "positions 必须与 nodes 等长");

    // 边界：空图 / 单节点直接原样返回（位置仍做一次 sanitize，杜绝 NaN/Inf 流出）。
    if n <= 1 {
        return positions.iter().map(|p| sanitize(*p, 0)).collect();
    }

    let k = params.ideal_length.max(1.0);
    let min_dist = params.min_distance.max(1e-3);

    // 初始位置 sanitize：NaN/Inf 或与他点重合的点，用确定性散布点替换，避免初始即坍缩。
    let mut pos: Vec<egui::Pos2> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| sanitize(*p, i))
        .collect();
    scatter_coincident(&mut pos, k);

    let mut temperature = params.initial_temperature.max(0.0);
    let cooling = if params.iterations > 0 {
        params.initial_temperature / params.iterations as f32
    } else {
        0.0
    };

    let mut disp = vec![egui::Vec2::ZERO; n];

    for _ in 0..params.iterations {
        for d in disp.iter_mut() {
            *d = egui::Vec2::ZERO;
        }

        // 斥力：每个无序节点对一次，对称累加（O(n^2)）。
        for i in 0..n {
            for j in (i + 1)..n {
                let mut delta = pos[i] - pos[j];
                let mut dist = delta.length();
                if dist < min_dist {
                    // 重合：用确定性方向打散，避免除零与坍缩。
                    delta = deterministic_dir(i, j);
                    dist = min_dist;
                }
                let force = (k * k) / dist;
                let dir = delta / dist;
                disp[i] += dir * force;
                disp[j] -= dir * force;
            }
        }

        // 引力：每条边把两端拉近。
        for &(a, b) in edges {
            if a == b || a >= n || b >= n {
                continue;
            }
            let mut delta = pos[a] - pos[b];
            let mut dist = delta.length();
            if dist < min_dist {
                delta = deterministic_dir(a, b);
                dist = min_dist;
            }
            let force = (dist * dist) / k;
            let dir = delta / dist;
            disp[a] -= dir * force;
            disp[b] += dir * force;
        }

        // 按温度钳幅后落位（cooling schedule：温度线性降到 0）。
        for i in 0..n {
            let d = disp[i];
            let len = d.length();
            if len > 1e-6 {
                let capped = d / len * len.min(temperature);
                pos[i] += capped;
            }
        }

        temperature = (temperature - cooling).max(0.0);
    }

    // 收尾 sanitize：任何残留 NaN/Inf（理论不该出现）兜底替换。
    pos.iter()
        .enumerate()
        .map(|(i, p)| sanitize(*p, i))
        .collect()
}

/// 把单点的非有限坐标替换为一个确定性散布点（按下标排到一圈上），保证输出有限且不全重合。
fn sanitize(p: egui::Pos2, i: usize) -> egui::Pos2 {
    if p.x.is_finite() && p.y.is_finite() {
        p
    } else {
        deterministic_point(i)
    }
}

/// 确定性散布点：按下标沿黄金角螺旋排开，无随机、跨平台一致（wasm/native 同结果）。
fn deterministic_point(i: usize) -> egui::Pos2 {
    let angle = i as f32 * 2.399_963_2; // 黄金角（弧度）
    let radius = 10.0 * (i as f32 + 1.0).sqrt();
    egui::pos2(radius * angle.cos(), radius * angle.sin())
}

/// 两个重合点之间的确定性"分离方向"（单位向量），避免除零，且左右点得到相反方向。
fn deterministic_dir(i: usize, j: usize) -> egui::Vec2 {
    let angle = (i.wrapping_mul(31).wrapping_add(j) as f32) * 2.399_963_2;
    egui::vec2(angle.cos(), angle.sin())
}

/// 把彼此重合的初始点沿确定性方向轻微错开，避免第一轮迭代时大量节点零距离。
fn scatter_coincident(pos: &mut [egui::Pos2], k: f32) {
    let n = pos.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if (pos[i] - pos[j]).length() < 1e-3 {
                let off = deterministic_dir(i, j) * (k * 0.5);
                pos[j] += off;
            }
        }
    }
}

/// 把力导向布局接到 [`Graph`]：读拓扑（`node_indices` / `edge_references`），跑纯函数，
/// 把结果写回每个 `Node.position`（画布坐标，AGENTS.md §3.2）。返回受影响的节点数。
///
/// 调用方应在 `with_resource` 闭包内调用（持写锁、单一真源，AGENTS.md §3.1）。
pub fn layout_graph(graph: &mut Graph, params: LayoutParams) -> usize {
    use petgraph::visit::{EdgeRef, IntoEdgeReferences};

    let nodes: Vec<NodeIndex> = graph.graph.node_indices().collect();
    if nodes.len() <= 1 {
        return 0;
    }

    // NodeIndex -> 下标，供边引用与位置数组对齐。
    let index_of: HashMap<NodeIndex, usize> =
        nodes.iter().enumerate().map(|(i, &idx)| (idx, i)).collect();

    let positions: Vec<egui::Pos2> = nodes.iter().map(|&idx| graph.graph[idx].position).collect();

    let edges: Vec<(usize, usize)> = graph
        .graph
        .edge_references()
        .filter_map(|e| {
            let a = index_of.get(&e.source())?;
            let b = index_of.get(&e.target())?;
            Some((*a, *b))
        })
        .collect();

    let new_positions = force_directed_layout(&nodes, &edges, &positions, params);

    for (idx, new_pos) in nodes.iter().zip(new_positions) {
        if let Some(node) = graph.graph.node_weight_mut(*idx) {
            node.position = new_pos;
        }
    }

    nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasState;
    use crate::graph::edge::Edge;
    use crate::graph::node::Node;
    use crate::resource::CanvasStateResource;

    fn pos_finite(p: egui::Pos2) -> bool {
        p.x.is_finite() && p.y.is_finite()
    }

    #[test]
    fn empty_graph_does_not_panic() {
        let out = force_directed_layout(&[], &[], &[], LayoutParams::default());
        assert!(out.is_empty());
    }

    #[test]
    fn single_node_returns_same_finite_position() {
        let nodes = vec![NodeIndex::new(0)];
        let positions = vec![egui::pos2(5.0, 7.0)];
        let out = force_directed_layout(&nodes, &[], &positions, LayoutParams::default());
        assert_eq!(out.len(), 1);
        assert!(pos_finite(out[0]));
        assert_eq!(out[0], egui::pos2(5.0, 7.0));
    }

    #[test]
    fn two_connected_nodes_converge_to_ideal_length() {
        // 两个相连节点初始离得很远，布局后距离应趋近理想边长。
        let nodes = vec![NodeIndex::new(0), NodeIndex::new(1)];
        let edges = vec![(0usize, 1usize)];
        let positions = vec![egui::pos2(-500.0, 0.0), egui::pos2(500.0, 0.0)];
        let params = LayoutParams::default();
        let out = force_directed_layout(&nodes, &edges, &positions, params);

        let dist = (out[0] - out[1]).length();
        // 趋近理想边长（容许斥力/引力平衡点附近的偏差）。
        assert!(
            (dist - params.ideal_length).abs() < params.ideal_length * 0.5,
            "相连节点距离 {dist} 应趋近理想边长 {}",
            params.ideal_length
        );
        assert!(pos_finite(out[0]) && pos_finite(out[1]));
    }

    #[test]
    fn isolated_nodes_do_not_collapse_to_one_point() {
        // 多个孤立节点（无边）全部初始重合到原点：斥力应把它们分散开。
        let nodes: Vec<NodeIndex> = (0..6).map(NodeIndex::new).collect();
        let positions = vec![egui::pos2(0.0, 0.0); 6];
        let out = force_directed_layout(&nodes, &[], &positions, LayoutParams::default());

        // 任意两点距离都应明显 > 0（没有坍缩成一点）。
        for i in 0..out.len() {
            assert!(pos_finite(out[i]));
            for j in (i + 1)..out.len() {
                let d = (out[i] - out[j]).length();
                assert!(d > 1.0, "孤立节点 {i} 与 {j} 不应坍缩重合（dist={d}）");
            }
        }
    }

    #[test]
    fn connected_pair_closer_than_disconnected_pair() {
        // 三角拓扑：0-1 相连、2 孤立。布局后相连两点应比孤立点彼此更近。
        let nodes: Vec<NodeIndex> = (0..3).map(NodeIndex::new).collect();
        let edges = vec![(0usize, 1usize)];
        let positions = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(10.0, 0.0),
            egui::pos2(-10.0, 5.0),
        ];
        let out = force_directed_layout(&nodes, &edges, &positions, LayoutParams::default());
        let connected = (out[0] - out[1]).length();
        let to_isolated_0 = (out[0] - out[2]).length();
        let to_isolated_1 = (out[1] - out[2]).length();
        assert!(
            connected < to_isolated_0 && connected < to_isolated_1,
            "相连对距离 {connected} 应小于到孤立点的距离 ({to_isolated_0}, {to_isolated_1})"
        );
    }

    #[test]
    fn nan_inf_inputs_are_sanitized() {
        let nodes = vec![NodeIndex::new(0), NodeIndex::new(1), NodeIndex::new(2)];
        let positions = vec![
            egui::pos2(f32::NAN, 0.0),
            egui::pos2(0.0, f32::INFINITY),
            egui::pos2(1.0, 2.0),
        ];
        let out = force_directed_layout(&nodes, &[], &positions, LayoutParams::default());
        for p in &out {
            assert!(pos_finite(*p), "输出不得含 NaN/Inf：{p:?}");
        }
    }

    #[test]
    fn zero_iterations_only_sanitizes() {
        let params = LayoutParams {
            iterations: 0,
            ..LayoutParams::default()
        };
        let nodes = vec![NodeIndex::new(0), NodeIndex::new(1)];
        let positions = vec![egui::pos2(100.0, 100.0), egui::pos2(200.0, 50.0)];
        let out = force_directed_layout(&nodes, &[], &positions, params);
        assert_eq!(out, positions, "0 迭代应原样返回（仅 sanitize）");
    }

    fn graph_with_chain(titles: &[&str]) -> (Graph, CanvasStateResource) {
        let canvas = CanvasStateResource::new(CanvasState::default());
        let mut g = Graph::default();
        let mut prev: Option<NodeIndex> = None;
        for t in titles {
            let id = canvas.read_resource(|c| c.new_node_id());
            let idx = g.add_node(Node {
                id,
                position: egui::pos2(0.0, 0.0),
                text: (*t).to_owned(),
                note: String::new(),
                aliases: Vec::new(),
            });
            if let Some(p) = prev {
                let pp = g.get_node(p).unwrap().position;
                let cp = g.get_node(idx).unwrap().position;
                g.add_edge(Edge::new(p, idx, pp, cp, canvas.clone()));
            }
            prev = Some(idx);
        }
        (g, canvas)
    }

    #[test]
    fn layout_graph_spreads_chain_and_keeps_finite() {
        // 链式 A-B-C-D 全部初始在原点：布局后应被铺开（不重合）且坐标有限。
        let (mut g, _c) = graph_with_chain(&["A", "B", "C", "D"]);
        let affected = layout_graph(&mut g, LayoutParams::default());
        assert_eq!(affected, 4);

        let positions: Vec<egui::Pos2> = g
            .graph
            .node_indices()
            .map(|i| g.graph[i].position)
            .collect();
        for (i, p) in positions.iter().enumerate() {
            assert!(pos_finite(*p), "节点 {i} 坐标应有限");
            for (j, q) in positions.iter().enumerate() {
                if i != j {
                    assert!((*p - *q).length() > 1.0, "节点 {i}/{j} 不应重合");
                }
            }
        }
    }

    #[test]
    fn layout_graph_empty_and_single_no_panic() {
        let mut empty = Graph::default();
        assert_eq!(layout_graph(&mut empty, LayoutParams::default()), 0);

        let (mut single, _c) = graph_with_chain(&["only"]);
        assert_eq!(layout_graph(&mut single, LayoutParams::default()), 0);
    }
}
