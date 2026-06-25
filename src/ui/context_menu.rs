//! 右键上下文菜单的**跨层请求总线**（隐式状态总线约定，与 `FOCUS_REQUEST_KEY` / 命令面板同范式）。
//!
//! ## 为什么走 temp-data 总线而非 egui 原生 `Response::context_menu`
//! 右键在本项目被输入状态机（`state_manager.rs`）**独占**用于「连边手势」（右键拖拽 节点→节点/空白）。
//! egui 原生 `context_menu` 会自己监听 secondary 事件弹菜单，与状态机争抢 secondary，导致连边手势
//! 与菜单互相打架、且难以做"右键单击弹菜单 / 右键拖拽连边"的消歧。
//!
//! 故改为：**单击/拖拽消歧只在 `state_manager.rs` 的 secondary release 单点决策**（AGENTS.md §3.4：
//! 一次性事件管转换，决策集中一处）。判为"右键单击"时，状态机把一个 [`ContextMenuRequest`]
//! 写入 egui `ctx` temp data；由 [`crate::app::CognitheonApp::ui`] 在画布渲染**之后**读取并用
//! `egui::Popup::new(id, ctx, anchor, layer_id)` 自管弹出菜单（与命令面板 / focus 请求同款
//! temp-data 总线，更可控、不与状态机争 secondary）。菜单本身是 egui 即时 UI，**不是**
//! `InputState` 枚举分支。
//!
//! ## 菜单项 → 状态机的“进入编辑/新建”请求总线（同范式反向通道）
//! 菜单是 `app.rs` 的即时 UI，拿不到 `&mut InputStateManager`，**不能**直接 `set_editing_node`：
//! 那样设置的 `editing_node` 不被输入状态机持有，下一帧 `state_manager.update()` 的 `Idle` 分支
//! 会无条件 `set_editing_node(None)` 把它清掉，编辑框永不出现（且从未 `stage` 快照、编辑不可撤销）。
//! 故「编辑标题」「在此新建节点」也走 temp-data 请求总线写 [`EditNodeRequest`] / [`CreateNodeRequest`]，
//! 由 `state_manager.update()`（输入唯一驱动，§3.4）消费时执行与 `handle_double_click` 同款三件套
//! （`stage_edit_snapshot` + select + `set_editing_node` + `transition_to(EditingNode)`），编辑态由
//! 状态机持有、退出时经 `resolve_on_exit_edit` 提交为可撤销单元。
//!
//! ## 跨帧失效容错（AGENTS.md §3.3）
//! 菜单可能跨多帧打开，其间目标 `NodeIndex` / `EdgeIndex` 可能已被删除而悬空。菜单回调里**必须**
//! 用 `get_node` / `get_edge` / `edge_endpoints` 做 `Option` 容错、**绝不 `.unwrap()`**；目标失效时
//! 菜单相应项灰显或菜单整体关闭，不 panic。

use egui::{Id, Pos2};
use petgraph::graph::{EdgeIndex, NodeIndex};

/// 右键上下文菜单请求的 temp-data key（隐式状态总线）。状态机判为"右键单击"那一帧写入，
/// `app.rs` 渲染期消费——读到即打开菜单，菜单关闭时清除。
pub const CONTEXT_MENU_REQUEST_KEY: &str = "context_menu_request";

/// 右键单击命中的目标（决定菜单内容）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuTarget {
    /// 右键点在某节点上。
    Node(NodeIndex),
    /// 右键点在某条边上。
    Edge(EdgeIndex),
    /// 右键点在空白画布上。
    Canvas,
}

/// 一次右键单击产生的上下文菜单请求：命中目标 + 弹出位置（屏幕坐标）。
///
/// `screen_pos` 是右键单击时的指针屏幕坐标，菜单用 `egui::Popup::new(id, ctx, anchor, layer_id)`
/// 以此为锚点弹出。
#[derive(Clone, Copy, Debug)]
pub struct ContextMenuRequest {
    pub target: ContextMenuTarget,
    pub screen_pos: Pos2,
}

/// 写入一个上下文菜单请求（状态机在判定右键单击那一帧调用）。
pub fn request_context_menu(ctx: &egui::Context, request: ContextMenuRequest) {
    ctx.data_mut(|d| d.insert_temp(Id::new(CONTEXT_MENU_REQUEST_KEY), request));
}

/// 「编辑标题」菜单项 → 状态机的“进入编辑”请求 temp-data key（隐式状态总线，反向通道）。
/// `app.rs` 菜单项写入、`state_manager.update()` 在一次性事件附近消费后清除。
pub const EDIT_NODE_REQUEST_KEY: &str = "edit_node_request";

/// 「在此新建节点」菜单项 → 状态机的“新建并进入编辑”请求 temp-data key（隐式状态总线）。
pub const CREATE_NODE_REQUEST_KEY: &str = "create_node_request";

/// 请求让状态机把某节点带入编辑态（与双击节点同语义）。`app.rs` 的「编辑标题」菜单项写入。
#[derive(Clone, Copy, Debug)]
pub struct EditNodeRequest {
    pub node: NodeIndex,
}

/// 请求让状态机在指定画布坐标新建节点并立即进入编辑（与双击空白建点同语义）。
/// `app.rs` 的「在此新建节点」菜单项写入；位置存**画布坐标**（§3.2：屏幕坐标永不进图）。
#[derive(Clone, Copy, Debug)]
pub struct CreateNodeRequest {
    pub canvas_pos: Pos2,
}

/// 写入一个「进入编辑」请求（菜单项点击那一帧调用，不直接 `set_editing_node`，详见模块文档）。
pub fn request_edit_node(ctx: &egui::Context, request: EditNodeRequest) {
    ctx.data_mut(|d| d.insert_temp(Id::new(EDIT_NODE_REQUEST_KEY), request));
}

/// 写入一个「新建并进入编辑」请求（菜单项点击那一帧调用，建点+编辑由状态机做成单一撤销单元）。
pub fn request_create_node(ctx: &egui::Context, request: CreateNodeRequest) {
    ctx.data_mut(|d| d.insert_temp(Id::new(CREATE_NODE_REQUEST_KEY), request));
}
