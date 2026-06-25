//! 边的命中几何旁路（与节点几何 observer 旁路 §3.3 同构的"边版本"）。
//!
//! 背景（AGENTS.md §3.3）：节点几何（`NodeRenderInfo.canvas_rect`）由 `NodeWidget` 渲染末尾经
//! observer 写入 egui `ctx` temp data，`hit_test` / `EdgeWidget` 反读。边几何此前**不进总线**——
//! `determine_target` 因此永不产出 `InputTarget::Edge`，边只能随节点被动消失。
//!
//! 本模块补上边版本的旁路：`EdgeWidget` 每帧已按实时节点锚点重算边的采样折线，渲染时把这条
//! **画布坐标**折线（[`EdgeHitInfo`]）写入 temp data；`determine_target` 反读、做点到折线的距离
//! 测试得到命中。
//!
//! ## 与节点旁路一致的"一帧延迟"
//! 与节点几何同样存在依赖上一帧的一帧延迟（`render_graph` 先画边后画点）：本帧新建的边当帧
//! 采样点尚未写入，`determine_target` 读不到 → 当帧命中落空，下一帧即可命中。可接受。
//!
//! ## key 命名空间（避免与节点 key 撞车）
//! 节点用 `Id::new(node_index.index().to_string())`。边若也用纯 index 字符串会与节点 key 撞车
//! （同一个 usize 同时是某节点与某边的 index）。故边 key 走 [`edge_hit_id`]，用带前缀的元组
//! `("edge_hit", edge_index.index())` 构造 `Id`，与节点 key 空间天然隔离。
//!
//! ## 早退不 panic（§3.3）
//! 缺信息（节点几何尚未发布、采样点不足）时**早退**、不写 temp data，也不 `.unwrap()`；
//! `determine_target` 读不到该边的 [`EdgeHitInfo`] 时直接跳过这条边，不 panic。

use egui::{Id, Pos2};
use petgraph::graph::EdgeIndex;

/// 一条边的命中几何：**画布坐标**下的采样折线。
///
/// - Line 边：两个端点锚点（2 个采样点）。
/// - Bezier 边：曲线细分后的采样点折线（与渲染用的细分一致，约 100 段/区间）。
///
/// 命中测试 = 点（画布坐标的光标）到这条折线的最近距离 ≤ 阈值。阈值在画布空间 =
/// `屏幕阈值 / scaling`（§3.2：距离阈值随缩放换算到画布空间）。
#[derive(Clone, Debug)]
pub struct EdgeHitInfo {
    /// 画布坐标系下、沿边走向排列的采样点。至少 2 个才有意义。
    pub canvas_samples: Vec<Pos2>,
}

/// 边命中几何的 temp data key（与节点 key 空间隔离，见模块文档）。
pub fn edge_hit_id(edge_index: EdgeIndex) -> Id {
    Id::new(("edge_hit", edge_index.index()))
}

/// 当前 hover 命中的边的 temp data key（单值，本帧由 `determine_target` 写、`EdgeWidget` 读）。
///
/// 写读同帧：`input_manager.update`（内含 `determine_target`）在 `render_graph` 之前跑，故
/// EdgeWidget 渲染时能读到本帧最新的 hover 边；切换/移开时被覆盖或清空，无一帧延迟。
fn hovered_edge_id() -> Id {
    Id::new("hovered_edge")
}

/// 发布本帧 hover 命中的边（`None` 表示无）。`determine_target` 每帧调用一次（命中或清空）。
pub fn set_hovered_edge(ctx: &egui::Context, edge_index: Option<EdgeIndex>) {
    ctx.data_mut(|d| match edge_index {
        Some(idx) => {
            d.insert_temp(hovered_edge_id(), idx);
        }
        None => {
            d.remove::<EdgeIndex>(hovered_edge_id());
        }
    });
}

/// 反读本帧 hover 命中的边（`EdgeWidget` 渲染时调用）。
pub fn get_hovered_edge(ctx: &egui::Context) -> Option<EdgeIndex> {
    ctx.data(|d| d.get_temp::<EdgeIndex>(hovered_edge_id()))
}

/// 把一条边的采样折线发布到 egui `ctx` temp data（`EdgeWidget` 渲染时调用）。
///
/// 采样点不足 2 个时不发布（早退，§3.3）——这样 `determine_target` 读不到，自然跳过该边。
pub fn publish_edge_hit_info(
    ctx: &egui::Context,
    edge_index: EdgeIndex,
    canvas_samples: Vec<Pos2>,
) {
    if canvas_samples.len() < 2 {
        return;
    }
    ctx.data_mut(|d| {
        d.insert_temp(edge_hit_id(edge_index), EdgeHitInfo { canvas_samples });
    });
}

/// 反读一条边的采样折线（`determine_target` 命中测试时调用）。读不到（未发布/已失效）返回 `None`。
pub fn get_edge_hit_info(ctx: &egui::Context, edge_index: EdgeIndex) -> Option<EdgeHitInfo> {
    ctx.data(|d| d.get_temp::<EdgeHitInfo>(edge_hit_id(edge_index)))
}

/// 点 `p` 到线段 `[a, b]` 的最短距离（纯几何，无副作用，可无头单测）。
///
/// 退化（`a == b`）时退回点到点距离。投影参数 `t` 截断到 `[0, 1]` 取线段上最近点。
pub fn point_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= f32::EPSILON {
        // 退化线段：a 与 b 重合。
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

/// 点 `p` 到折线（采样点序列）的最短距离（纯几何，可无头单测）。
///
/// 折线由相邻采样点连成的线段构成，取所有线段中的最小距离。点数 < 2 时返回 `f32::INFINITY`
/// （视为不可命中），调用方据此跳过。
pub fn point_polyline_distance(p: Pos2, samples: &[Pos2]) -> f32 {
    if samples.len() < 2 {
        return f32::INFINITY;
    }
    let mut min_dist = f32::INFINITY;
    for w in samples.windows(2) {
        let d = point_segment_distance(p, w[0], w[1]);
        if d < min_dist {
            min_dist = d;
        }
    }
    min_dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn point_on_segment_has_zero_distance() {
        let d = point_segment_distance(pos2(5.0, 0.0), pos2(0.0, 0.0), pos2(10.0, 0.0));
        assert!(d.abs() < 1e-5, "线段上的点距离应为 0，得到 {d}");
    }

    #[test]
    fn point_perpendicular_to_segment() {
        // (5, 3) 到水平线段 [(0,0),(10,0)] 的垂距 = 3。
        let d = point_segment_distance(pos2(5.0, 3.0), pos2(0.0, 0.0), pos2(10.0, 0.0));
        assert!((d - 3.0).abs() < 1e-5, "垂距应为 3，得到 {d}");
    }

    #[test]
    fn point_beyond_segment_end_clamps_to_endpoint() {
        // (15, 0) 投影超出端点 (10,0)，应截断到端点 → 距离 5。
        let d = point_segment_distance(pos2(15.0, 0.0), pos2(0.0, 0.0), pos2(10.0, 0.0));
        assert!((d - 5.0).abs() < 1e-5, "应截断到端点，距离 5，得到 {d}");
    }

    #[test]
    fn point_before_segment_start_clamps_to_start() {
        let d = point_segment_distance(pos2(-4.0, 0.0), pos2(0.0, 0.0), pos2(10.0, 0.0));
        assert!((d - 4.0).abs() < 1e-5, "应截断到起点，距离 4，得到 {d}");
    }

    #[test]
    fn degenerate_segment_is_point_distance() {
        let d = point_segment_distance(pos2(3.0, 4.0), pos2(0.0, 0.0), pos2(0.0, 0.0));
        assert!(
            (d - 5.0).abs() < 1e-5,
            "退化线段应退回点到点距离 5，得到 {d}"
        );
    }

    #[test]
    fn polyline_picks_nearest_segment() {
        // L 形折线：(0,0)->(10,0)->(10,10)。点 (10,5) 落在竖直段上 → 距离 0。
        let samples = vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        let d = point_polyline_distance(pos2(10.0, 5.0), &samples);
        assert!(d.abs() < 1e-5, "应命中竖直段，距离 0，得到 {d}");
    }

    #[test]
    fn polyline_distance_to_corner() {
        // 点 (13, -3) 离拐角 (10,0) 最近 → 距离 sqrt(9+9)=4.2426。
        let samples = vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        let d = point_polyline_distance(pos2(13.0, -3.0), &samples);
        assert!(
            (d - (18.0_f32).sqrt()).abs() < 1e-4,
            "应为到拐角的距离，得到 {d}"
        );
    }

    #[test]
    fn polyline_too_few_points_is_infinite() {
        assert_eq!(point_polyline_distance(pos2(0.0, 0.0), &[]), f32::INFINITY);
        assert_eq!(
            point_polyline_distance(pos2(0.0, 0.0), &[pos2(1.0, 1.0)]),
            f32::INFINITY
        );
    }
}
