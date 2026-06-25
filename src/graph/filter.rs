//! 关键词过滤的"可见度判定"旁路（渲染层 + egui temp data，不进 SSOT / 不序列化）。
//!
//! 大图降噪：输入关键词 → 不匹配的节点（及其相连边）**淡出（Dim）或隐藏（Hide）**，匹配
//! 节点正常显示；清空关键词恢复全部可见。两模式可切换。
//!
//! ## 架构定位（AGENTS.md §3.3 / §3.5）
//! - **匹配集复用现成引擎**：可见集 = [`crate::wikilink::search`]（标题 + 别名 + 正文，大小写
//!   不敏感、已有测试）的命中集；query 为空时**全部可见**（不过滤）。零新算法。
//! - **判定集中在一处**：[`publish_filter_visibility`] 在 `render_graph` 入口算一次"哪些节点
//!   可见"，写入 egui temp data；`NodeWidget` / `EdgeWidget` / `hit_test_edge` 反读
//!   [`node_visibility`] / [`edge_visibility`]，避免每个 widget 各自重算 `O(N)` 搜索。
//! - **纯 UI 状态**：过滤 query 与模式存 temp data（key 仿命令面板风格），**不进序列化**、
//!   不碰 SSOT / 状态机。与未来「邻居聚焦」叠加时只需扩展这里的可见集计算，渲染端不动。
//!
//! ## 三态可见度
//! - `Visible`：在可见集内（或无过滤）——正常渲染。
//! - `Dimmed`：不在可见集且模式为 Dim——降低 alpha 淡出（节点照常渲染、几何照常写 observer，
//!   **不触 §3.3 隐藏耦合**，是更安全的默认主路径）。
//! - `Hidden`：不在可见集且模式为 Hide——节点不画卡片、不写几何；相连边整条跳过（见
//!   [`edge_visibility`]，从源头避开 `node_rect_center` 等对缺失几何的 `.unwrap()` panic）。

use std::collections::HashSet;

use egui::Id;
use petgraph::graph::NodeIndex;

use crate::graph::graph_impl::Graph;

/// 过滤关键词的 temp data key（纯 UI、不序列化；仿命令面板 `command_palette_query` 风格）。
pub const FILTER_QUERY_KEY: &str = "filter_query";
/// 过滤模式（Dim / Hide）的 temp data key。
pub const FILTER_MODE_KEY: &str = "filter_mode";
/// 本帧算好的"过滤可见度快照"的 temp data key（[`render_graph`] 入口写、各 widget 反读）。
const FILTER_VISIBILITY_KEY: &str = "filter_visibility";

/// 过滤模式：不匹配节点淡出还是彻底隐藏。Dim 为默认（更安全，不碰 §3.3 几何耦合）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FilterMode {
    /// 淡出：不匹配节点 / 边降低 alpha，仍渲染、仍写几何。
    #[default]
    Dim,
    /// 隐藏：不匹配节点不画、不写几何；相连边整条跳过。
    Hide,
}

/// 单个图元（节点或边）在当前过滤下的可见度。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// 正常渲染（命中 / 无过滤）。
    Visible,
    /// 淡出（Dim 模式下的不匹配项）。
    Dimmed,
    /// 隐藏（Hide 模式下的不匹配项）。
    Hidden,
}

/// 本帧的过滤可见度快照：可见节点集 + 当前模式。`active = false` 表示无过滤（全部可见）。
///
/// 在 `render_graph` 入口算一次并写入 temp data；各 widget 反读，避免重复 `O(N)` 搜索。
/// 失效 / 缺失（首帧尚未发布、或被清掉）时各 widget 默认 [`Visibility::Visible`]，安全。
#[derive(Clone, Debug)]
struct FilterVisibility {
    /// 是否处于过滤态（query 非空）。`false` 时所有图元一律 `Visible`。
    active: bool,
    mode: FilterMode,
    /// 命中关键词的节点集（`active == false` 时为空、不被查询）。
    visible: HashSet<NodeIndex>,
}

/// 读取当前过滤 query（trim 后），无则空串。
pub fn current_query(ctx: &egui::Context) -> String {
    ctx.data(|d| d.get_temp::<String>(Id::new(FILTER_QUERY_KEY)))
        .unwrap_or_default()
}

/// 读取当前过滤模式，无则默认 [`FilterMode::Dim`]。
pub fn current_mode(ctx: &egui::Context) -> FilterMode {
    ctx.data(|d| d.get_temp::<FilterMode>(Id::new(FILTER_MODE_KEY)))
        .unwrap_or_default()
}

/// 写回过滤 query（空串归一为"无过滤"，但仍存空串以保留输入框文本一致性）。
pub fn set_query(ctx: &egui::Context, query: String) {
    ctx.data_mut(|d| d.insert_temp(Id::new(FILTER_QUERY_KEY), query));
}

/// 写回过滤模式。
pub fn set_mode(ctx: &egui::Context, mode: FilterMode) {
    ctx.data_mut(|d| d.insert_temp(Id::new(FILTER_MODE_KEY), mode));
}

/// 在 `render_graph` 入口算一次本帧可见度快照并发布到 temp data。
///
/// 可见集 = [`crate::wikilink::search`] 的命中集（query 非空时）；query 为空 → `active = false`
/// （全部可见，不查询）。§3.1：图只读经传入闭包外的 `read_resource`（调用方持锁作用域内）。
pub fn publish_filter_visibility(ctx: &egui::Context, graph: &Graph) {
    let query = current_query(ctx);
    let q = query.trim();
    let snapshot = if q.is_empty() {
        FilterVisibility {
            active: false,
            mode: current_mode(ctx),
            visible: HashSet::new(),
        }
    } else {
        // 复用现成全文搜索引擎（标题 + 别名 + 正文）——零新算法。
        let hits: HashSet<NodeIndex> = crate::wikilink::search(graph, q).into_iter().collect();
        FilterVisibility {
            active: true,
            mode: current_mode(ctx),
            visible: hits,
        }
    };
    ctx.data_mut(|d| d.insert_temp(Id::new(FILTER_VISIBILITY_KEY), snapshot));
}

/// 反读本帧可见度快照。缺失（首帧未发布 / 被清）时返回"无过滤"——全部 `Visible`，安全早退。
fn visibility_snapshot(ctx: &egui::Context) -> Option<FilterVisibility> {
    ctx.data(|d| d.get_temp::<FilterVisibility>(Id::new(FILTER_VISIBILITY_KEY)))
}

/// 某节点在当前过滤下的可见度（`NodeWidget` 渲染时反读）。
///
/// 无过滤 / 命中 → `Visible`；不命中且 Dim → `Dimmed`；不命中且 Hide → `Hidden`。
pub fn node_visibility(ctx: &egui::Context, node_index: NodeIndex) -> Visibility {
    match visibility_snapshot(ctx) {
        Some(s) if s.active => {
            if s.visible.contains(&node_index) {
                Visibility::Visible
            } else {
                match s.mode {
                    FilterMode::Dim => Visibility::Dimmed,
                    FilterMode::Hide => Visibility::Hidden,
                }
            }
        }
        // 无快照或非过滤态：全部可见。
        _ => Visibility::Visible,
    }
}

/// 一条边在当前过滤下的可见度，由其两端节点可见度合成（`EdgeWidget` / `hit_test_edge` 反读）。
///
/// 规则（与"不匹配节点连同其相连边一起降噪"一致）：
/// - 两端**任一** `Hidden` → 边 `Hidden`（Hide 模式：整条边跳过、不画也不可命中，从源头避开
///   对隐藏节点缺失几何的 `node_rect_center` 等 `.unwrap()` panic，§3.3）。
/// - 否则两端**任一** `Dimmed`（且无 Hidden）→ 边 `Dimmed`（Dim 模式：边随之淡出）。
/// - 两端都 `Visible` → 边 `Visible`。
///
/// 端点索引由调用方从图里取（§3.3 用 `NodeIndex` 句柄，不依赖 observer 几何）；端点失效
/// （已被删）的容错由调用方处理，本函数只做三态合成。
pub fn edge_visibility(ctx: &egui::Context, source: NodeIndex, target: NodeIndex) -> Visibility {
    let a = node_visibility(ctx, source);
    let b = node_visibility(ctx, target);
    if a == Visibility::Hidden || b == Visibility::Hidden {
        Visibility::Hidden
    } else if a == Visibility::Dimmed || b == Visibility::Dimmed {
        Visibility::Dimmed
    } else {
        Visibility::Visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三态合成：Hidden 优先于 Dimmed 优先于 Visible（纯逻辑，无需 egui ctx）。
    /// 这里直接验证合成规则的真值表（合成逻辑与 [`edge_visibility`] 同构）。
    fn combine(a: Visibility, b: Visibility) -> Visibility {
        if a == Visibility::Hidden || b == Visibility::Hidden {
            Visibility::Hidden
        } else if a == Visibility::Dimmed || b == Visibility::Dimmed {
            Visibility::Dimmed
        } else {
            Visibility::Visible
        }
    }

    #[test]
    fn edge_combine_hidden_dominates() {
        use Visibility::*;
        assert_eq!(combine(Hidden, Visible), Hidden);
        assert_eq!(combine(Visible, Hidden), Hidden);
        assert_eq!(combine(Hidden, Dimmed), Hidden);
        assert_eq!(combine(Dimmed, Hidden), Hidden);
    }

    #[test]
    fn edge_combine_dimmed_when_no_hidden() {
        use Visibility::*;
        assert_eq!(combine(Dimmed, Visible), Dimmed);
        assert_eq!(combine(Visible, Dimmed), Dimmed);
        assert_eq!(combine(Dimmed, Dimmed), Dimmed);
    }

    #[test]
    fn edge_combine_visible_when_both_visible() {
        use Visibility::*;
        assert_eq!(combine(Visible, Visible), Visible);
    }

    #[test]
    fn filter_mode_default_is_dim() {
        assert_eq!(FilterMode::default(), FilterMode::Dim);
    }
}
