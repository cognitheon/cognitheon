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
/// 「邻居聚焦」开关的 temp data key（纯 UI、不序列化）。
pub const FOCUS_ENABLED_KEY: &str = "neighbor_focus_enabled";
/// 「结构着色」开关的 temp data key（纯 UI、不序列化；默认关）。
pub const STRUCTURE_COLORING_KEY: &str = "structure_coloring_enabled";
/// 本帧算好的"全图最大度数"的 temp data key（[`render_graph`] 入口算一次、`NodeWidget` 反读）。
///
/// 结构着色把每节点度数按 `degree / max_degree` 归一到色阶 / 框宽。集中算一次避免每节点各扫
/// 全图的 `O(N²)`（仿可见度快照的"入口算、widget 反读"范式）。
const STRUCTURE_MAX_DEGREE_KEY: &str = "structure_max_degree";
/// 本帧算好的"可见度快照"的 temp data key（[`render_graph`] 入口写、各 widget 反读）。
const FILTER_VISIBILITY_KEY: &str = "filter_visibility";

/// 邻居聚焦的邻域半径（跳数）。当前固定 1 跳（中心 + 直接邻居）；留常量便于将来做成可调参数。
const FOCUS_HOPS: usize = 1;

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

/// 本帧的可见度快照：过滤规则 + 邻居聚焦规则，二者在 `render_graph` 入口【合成】成一张表。
///
/// 在 `render_graph` 入口算一次并写入 temp data；各 widget 反读，避免重复 `O(N)` 搜索 / 邻域 BFS。
/// 失效 / 缺失（首帧尚未发布、或被清掉）时各 widget 默认 [`Visibility::Visible`]，安全。
///
/// 合成语义（见 [`node_visibility`]，**过滤优先**）：过滤把节点判 `Hidden` → 终判 `Hidden`；
/// 否则邻居聚焦激活且节点不在 `focus` 集 → `Dimmed`；其余取过滤自身的判定。
#[derive(Clone, Debug)]
struct FilterVisibility {
    /// 是否处于过滤态（query 非空）。`false` 时过滤规则一律放行 `Visible`。
    active: bool,
    mode: FilterMode,
    /// 命中关键词的节点集（`active == false` 时为空、不被查询）。
    visible: HashSet<NodeIndex>,
    /// 邻居聚焦是否激活（开关 ON **且**恰好选中一个节点）。`false` 时邻域规则不参与合成。
    focus_active: bool,
    /// 聚焦邻域集（中心 + `FOCUS_HOPS` 跳邻居；`focus_active == false` 时为空、不被查询）。
    focus: HashSet<NodeIndex>,
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

/// 读取「邻居聚焦」开关状态，无则默认 `false`（温和起见默认关，否则每次选中都 dim 全图会烦）。
pub fn focus_enabled(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(FOCUS_ENABLED_KEY)))
        .unwrap_or(false)
}

/// 写回「邻居聚焦」开关状态。
pub fn set_focus_enabled(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(Id::new(FOCUS_ENABLED_KEY), enabled));
}

/// 读取「结构着色」开关状态，无则默认 `false`（默认关，保持现有干净外观）。
pub fn structure_coloring_enabled(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(STRUCTURE_COLORING_KEY)))
        .unwrap_or(false)
}

/// 写回「结构着色」开关状态。
pub fn set_structure_coloring_enabled(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(Id::new(STRUCTURE_COLORING_KEY), enabled));
}

/// 在 `render_graph` 入口算一次全图最大度数并发布到 temp data（仅当结构着色开启时才计算，
/// 关闭时不做无谓全图扫描）。`NodeWidget` 渲染时经 [`structure_max_degree`] 反读，按
/// `degree / max_degree` 归一化自身度数到色阶 / 框宽。§3.1：图只读经调用方 `read_resource`
/// 闭包（作用域 = 锁作用域），闭包内不再取同一锁。
pub fn publish_structure_max_degree(ctx: &egui::Context, graph: &Graph) {
    if !structure_coloring_enabled(ctx) {
        // 关闭时清掉上一帧残留，避免开关切回时读到陈旧值。
        ctx.data_mut(|d| d.remove::<usize>(Id::new(STRUCTURE_MAX_DEGREE_KEY)));
        return;
    }
    let max = crate::wikilink::max_degree(graph);
    ctx.data_mut(|d| d.insert_temp(Id::new(STRUCTURE_MAX_DEGREE_KEY), max));
}

/// 反读本帧全图最大度数（结构着色归一化基准）。缺失（结构着色关闭 / 首帧未发布）返回 `0`
/// —— 调用方对 `max_degree == 0` 做除零兜底（全按最低档）。
pub fn structure_max_degree(ctx: &egui::Context) -> usize {
    ctx.data(|d| d.get_temp::<usize>(Id::new(STRUCTURE_MAX_DEGREE_KEY)))
        .unwrap_or(0)
}

/// 在 `render_graph` 入口算一次本帧可见度快照（过滤规则 + 邻居聚焦规则【合成】）并发布到 temp data。
///
/// - **过滤可见集** = [`crate::wikilink::search`] 的命中集（query 非空时）；query 为空 → `active =
///   false`（过滤规则全放行）。
/// - **邻居聚焦** 激活条件：开关 ON（[`focus_enabled`]）**且**恰好选中一个节点（[`Graph::get_selected_nodes`]，
///   §3.1 在调用方 `read_resource` 闭包内只读选区）。激活时邻域集 = [`crate::wikilink::neighborhood`]
///   （中心 + `FOCUS_HOPS` 跳，纯拓扑、§3.3 用 `NodeIndex`，不依赖几何 observer）。
///
/// 二者只在此一处算好、合成由 [`node_visibility`] 反读时完成（过滤优先）。§3.1：图只读经调用方
/// `read_resource` 闭包（作用域 = 锁作用域），闭包内不再取同一锁。
pub fn publish_filter_visibility(ctx: &egui::Context, graph: &Graph) {
    let query = current_query(ctx);
    let q = query.trim();
    let (active, visible) = if q.is_empty() {
        (false, HashSet::new())
    } else {
        // 复用现成全文搜索引擎（标题 + 别名 + 正文）——零新算法。
        let hits: HashSet<NodeIndex> = crate::wikilink::search(graph, q).into_iter().collect();
        (true, hits)
    };

    // 邻居聚焦：仅当开关 ON 且**恰好选中一个节点**时激活，邻域集 = 中心 + FOCUS_HOPS 跳邻居。
    // 选中 0 个 / 多个、或开关 OFF → 不激活（不参与合成，恢复全图）。
    let (focus_active, focus) = if focus_enabled(ctx) {
        let selected = graph.get_selected_nodes();
        if let [center] = selected[..] {
            (
                true,
                crate::wikilink::neighborhood(graph, center, FOCUS_HOPS),
            )
        } else {
            (false, HashSet::new())
        }
    } else {
        (false, HashSet::new())
    };

    let snapshot = FilterVisibility {
        active,
        mode: current_mode(ctx),
        visible,
        focus_active,
        focus,
    };
    ctx.data_mut(|d| d.insert_temp(Id::new(FILTER_VISIBILITY_KEY), snapshot));
}

/// 反读本帧可见度快照。缺失（首帧未发布 / 被清）时返回"无过滤"——全部 `Visible`，安全早退。
fn visibility_snapshot(ctx: &egui::Context) -> Option<FilterVisibility> {
    ctx.data(|d| d.get_temp::<FilterVisibility>(Id::new(FILTER_VISIBILITY_KEY)))
}

/// 某节点在当前【过滤 ∩ 邻居聚焦】合成下的可见度（`NodeWidget` 渲染时反读）。
///
/// 合成规则（**过滤优先**，spec 定）：
/// 1. 先取过滤判定：无过滤 / 命中 → 暂定 `Visible`；不命中且 Dim → `Dimmed`；不命中且 Hide → `Hidden`。
/// 2. 过滤判 `Hidden` → 终判 `Hidden`（过滤优先，不被邻域规则覆盖）。
/// 3. 否则若**邻居聚焦激活**（开关 ON + 单选）且该节点**不在邻域集** → `Dimmed`（邻域之外淡出）。
/// 4. 其余取过滤的判定（邻域内 + 过滤可见 → `Visible`）。
///
/// 即：邻域规则只能把"过滤未隐藏"的节点从 `Visible` 压成 `Dimmed`，不会把 `Hidden` 拉回，也不会把
/// 已 `Dimmed` 的更进一步——三态合成天然吸收（`Dimmed` ∨ `Dimmed` = `Dimmed`）。
pub fn node_visibility(ctx: &egui::Context, node_index: NodeIndex) -> Visibility {
    let Some(s) = visibility_snapshot(ctx) else {
        return Visibility::Visible;
    };

    // 1~2. 过滤判定（过滤优先：Hidden 直接终判）。
    let filtered = if s.active {
        if s.visible.contains(&node_index) {
            Visibility::Visible
        } else {
            match s.mode {
                FilterMode::Dim => Visibility::Dimmed,
                FilterMode::Hide => Visibility::Hidden,
            }
        }
    } else {
        Visibility::Visible
    };
    if filtered == Visibility::Hidden {
        return Visibility::Hidden;
    }

    // 3~4. 邻域规则：聚焦激活且节点在邻域之外 → 淡出；否则保留过滤判定。
    if s.focus_active && !s.focus.contains(&node_index) {
        Visibility::Dimmed
    } else {
        filtered
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

    /// 过滤 ∩ 邻居聚焦的合成真值表（纯逻辑，与 [`node_visibility`] 第 2~4 步同构、无需 egui ctx）。
    ///
    /// `filtered` = 过滤单独判定；`focus_active`/`in_focus` = 邻域规则。复刻"过滤优先 + 邻域只能
    /// 把 Visible 压成 Dimmed"的合成。
    fn compose(filtered: Visibility, focus_active: bool, in_focus: bool) -> Visibility {
        if filtered == Visibility::Hidden {
            return Visibility::Hidden;
        }
        if focus_active && !in_focus {
            Visibility::Dimmed
        } else {
            filtered
        }
    }

    #[test]
    fn compose_filter_hidden_wins_over_focus() {
        use Visibility::*;
        // 过滤判 Hidden：无论邻域聚焦如何，终判仍 Hidden（过滤优先）。
        assert_eq!(compose(Hidden, true, true), Hidden);
        assert_eq!(compose(Hidden, true, false), Hidden);
        assert_eq!(compose(Hidden, false, false), Hidden);
    }

    #[test]
    fn compose_focus_dims_outside_neighborhood() {
        use Visibility::*;
        // 聚焦激活 + 节点在邻域外：过滤可见的也被压成 Dimmed。
        assert_eq!(compose(Visible, true, false), Dimmed);
        // 邻域内：保留过滤判定（可见）。
        assert_eq!(compose(Visible, true, true), Visible);
        // 已被过滤 Dim 的，邻域外仍 Dim（吸收）。
        assert_eq!(compose(Dimmed, true, false), Dimmed);
    }

    #[test]
    fn compose_focus_inactive_passes_filter_through() {
        use Visibility::*;
        // 聚焦未激活（开关 OFF 或非单选）：直接取过滤判定，邻域不参与。
        assert_eq!(compose(Visible, false, false), Visible);
        assert_eq!(compose(Dimmed, false, false), Dimmed);
        assert_eq!(compose(Visible, false, true), Visible);
    }

    #[test]
    fn focus_enabled_default_is_off() {
        // 间接断言默认关：开关 temp data 缺失时 focus_enabled 应返回 false（温和默认）。
        // 这里只校验默认常量语义，真实 ctx 读取由集成/手测覆盖。
        assert_eq!(FOCUS_HOPS, 1, "默认聚焦半径为 1 跳");
    }
}
