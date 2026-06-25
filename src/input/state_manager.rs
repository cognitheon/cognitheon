// 在 src/input/state_manager.rs 中

use std::collections::HashMap;

use crate::{
    gpu_render::particle::particle_callback::ParticleCallback,
    graph::edge_hit::{get_edge_hit_info, point_polyline_distance, set_hovered_edge},
    graph::render_info::NodeRenderInfo,
    history::History,
    input::{events::InputTarget, input_state::InputState},
    resource::{CanvasStateResource, GraphResource},
};

use eframe::egui_wgpu;
use egui::*;
use petgraph::graph::{EdgeIndex, NodeIndex};

use super::button_state::ButtonState;

/// 右键单击 vs 右键拖拽的位移阈值（屏幕像素）。
///
/// 右键 press 后进入 `PendingSecondary`：指针位移**超过**此阈值判为"拖拽"（升级为连边手势），
/// 否则在 release 时判为"单击"（弹上下文菜单）。屏幕像素阈值与画布缩放无关——它衡量的是用户
/// 手抖的容差，而非画布距离（§3.2 区分屏幕/画布坐标）。取值参考 egui 默认拖拽起判阈（约 6px）。
const SECONDARY_DRAG_THRESHOLD: f32 = 6.0;

/// 存储输入处理所需的上下文数据
#[derive(Debug)]
pub struct InputContext {
    /// 画布资源
    pub canvas_state_resource: CanvasStateResource,

    /// 图形资源
    pub graph_resource: GraphResource,

    /// 撤销/重做历史（运行态，构造函数注入；所有会改图数据的写入经它打快照）。
    pub history: History,

    /// 当前鼠标位置（屏幕坐标）
    pub current_mouse_pos: Pos2,

    /// 前一帧的鼠标位置
    pub prev_mouse_pos: Pos2,

    /// 当前按键修饰符
    pub modifiers: Modifiers,

    /// 当前按下的鼠标按钮
    pub pressed_buttons: ButtonState,

    /// 当前按下的键
    pub pressed_keys: HashMap<Key, bool>,

    /// 从上一帧到当前帧的时间（秒）
    pub delta_time: f32,
}

impl InputContext {
    pub fn new(
        graph_resource: GraphResource,
        canvas_state_resource: CanvasStateResource,
        history: History,
    ) -> Self {
        Self {
            canvas_state_resource,
            graph_resource,
            history,
            current_mouse_pos: Pos2::ZERO,
            prev_mouse_pos: Pos2::ZERO,
            modifiers: Modifiers::NONE,
            pressed_buttons: ButtonState::new(),
            pressed_keys: HashMap::new(),
            delta_time: 0.0,
        }
    }

    /// 更新上下文中的每帧数据
    pub fn update(&mut self, ui: &mut egui::Ui) {
        ui.input(|i| {
            self.prev_mouse_pos = self.current_mouse_pos;
            self.current_mouse_pos = i.pointer.hover_pos().unwrap_or(self.current_mouse_pos);
            self.modifiers = i.modifiers;
            self.delta_time = i.stable_dt;

            // 更新按键状态
            for key in Key::ALL {
                self.pressed_keys.insert(*key, i.key_down(*key));
            }

            // 更新按钮状态
            self.pressed_buttons.set(
                PointerButton::Primary,
                i.pointer.button_down(PointerButton::Primary),
            );
            self.pressed_buttons.set(
                PointerButton::Secondary,
                i.pointer.button_down(PointerButton::Secondary),
            );
            self.pressed_buttons.set(
                PointerButton::Middle,
                i.pointer.button_down(PointerButton::Middle),
            );
            self.pressed_buttons.set(
                PointerButton::Extra1,
                i.pointer.button_down(PointerButton::Extra1),
            );
            self.pressed_buttons.set(
                PointerButton::Extra2,
                i.pointer.button_down(PointerButton::Extra2),
            );
        });
    }

    /// 检查鼠标是否在节点上
    pub fn hit_test_node(&self, ui: &egui::Ui, screen_pos: Pos2) -> Option<NodeIndex> {
        // 使用你现有的命中测试逻辑
        self.graph_resource.read_resource(|graph| {
            graph.graph.node_indices().find(|&node_index| {
                let node_render_info: Option<NodeRenderInfo> = ui
                    .ctx()
                    .data(|d| d.get_temp(Id::new(node_index.index().to_string())));

                if let Some(node_render_info) = node_render_info {
                    let node_screen_rect =
                        self.canvas_state_resource.read_resource(|canvas_state| {
                            canvas_state.to_screen_rect(node_render_info.canvas_rect)
                        });

                    return node_screen_rect.contains(screen_pos);
                }
                false
            })
        })
    }

    /// 检查鼠标是否命中某条边，返回最近且在阈值内的 `EdgeIndex`。
    ///
    /// 反读 `EdgeWidget` 每帧发布到 temp data 的画布采样折线（§3.3 边几何旁路，承认一帧延迟：
    /// 本帧新建的边当帧未发布、命中落空，下帧即可命中），做**画布空间**的点到折线距离测试。
    /// 距离阈值随缩放换算到画布空间 = `屏幕阈值 / scaling`（§3.2）；多条边命中时取最近一条。
    /// 缺采样（未发布/点数不足）的边早退跳过，不 panic（§3.3）。
    pub fn hit_test_edge(&self, ui: &egui::Ui, screen_pos: Pos2) -> Option<EdgeIndex> {
        /// 边命中的屏幕像素阈值（光标离边描线多少像素内算命中）。
        const HIT_THRESHOLD_SCREEN: f32 = 8.0;

        let scaling = self
            .canvas_state_resource
            .read_resource(|cs| cs.transform.scaling);
        // 阈值换算到画布空间：屏幕 px / scaling（§3.2）。scaling 已 clamp 到 [0.1, 100]，不为 0。
        let threshold_canvas = HIT_THRESHOLD_SCREEN / scaling;
        let canvas_pos = self
            .canvas_state_resource
            .read_resource(|cs| cs.to_canvas(screen_pos));

        let edge_indices = self
            .graph_resource
            .read_resource(|graph| graph.graph.edge_indices().collect::<Vec<EdgeIndex>>());

        let mut best: Option<(EdgeIndex, f32)> = None;
        for edge_index in edge_indices {
            let Some(hit_info) = get_edge_hit_info(ui.ctx(), edge_index) else {
                continue; // 该边几何尚未发布（一帧延迟）——跳过，不 panic。
            };
            let dist = point_polyline_distance(canvas_pos, &hit_info.canvas_samples);
            if dist <= threshold_canvas {
                match best {
                    Some((_, best_dist)) if best_dist <= dist => {}
                    _ => best = Some((edge_index, dist)),
                }
            }
        }
        best.map(|(idx, _)| idx)
    }

    /// 将屏幕坐标转换为画布坐标
    pub fn screen_to_canvas(&self, screen_pos: Pos2) -> Pos2 {
        self.canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_canvas(screen_pos))
    }

    /// 将画布坐标转换为屏幕坐标
    pub fn canvas_to_screen(&self, canvas_pos: Pos2) -> Pos2 {
        self.canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(canvas_pos))
    }
}

/// 输入状态管理器
#[derive(Debug)]
pub struct InputStateManager {
    /// 当前输入状态
    pub current_state: InputState,

    /// 输入上下文
    pub context: InputContext,

    /// 上一次记录的输入目标
    pub last_target: Option<InputTarget>,

    /// 上一帧的 `editing_node`，用于检测"退出编辑"的边沿（`Some(x) -> 非 x`）。
    /// 退出编辑是 wikilink `resolve_links` 的统一触发时机（§3.4：输入逻辑只在本文件驱动）。
    prev_editing_node: Option<NodeIndex>,
}

impl InputStateManager {
    pub fn new(
        graph_resource: GraphResource,
        canvas_state_resource: CanvasStateResource,
        history: History,
    ) -> Self {
        Self {
            current_state: InputState::Idle,
            context: InputContext::new(graph_resource, canvas_state_resource, history),
            last_target: None,
            prev_editing_node: None,
        }
    }

    /// 转换到新状态
    pub fn transition_to(&mut self, new_state: InputState) {
        // 可以在这里添加状态转换的日志或验证
        log::debug!(
            "Input state transition: {:?} -> {:?}",
            self.current_state,
            new_state
        );
        self.current_state = new_state;
    }

    /// 每帧更新输入状态
    pub fn update(&mut self, ui: &mut egui::Ui, canvas_response: &egui::Response) {
        // 更新上下文
        self.context.update(ui);

        // 区域门控：指针是否落在 canvas 区域内（其它面板如右侧链接面板在外）。
        // 用 canvas 的 drag response 判定——节点卡片在 canvas rect 内、`contains_pointer()`
        // 仍为 true，故节点点击/拖拽不受影响；只排除 canvas rect 外（右侧面板等）的事件。
        let pointer_in_canvas = canvas_response.contains_pointer();

        // 处理输入事件，获取当前输入目标
        let target = self.determine_target(ui);
        self.last_target = Some(target.clone());

        // 首先处理一次性事件，这些可能导致状态转换
        self.handle_one_shot_events(ui, &target, pointer_in_canvas);

        // 消费来自右键菜单的「进入编辑 / 新建并编辑」请求（跨层 temp-data 总线，§3.4 输入唯一驱动）：
        // 菜单是 app.rs 即时 UI 拿不到 &mut self，只写请求；进编辑/建点的状态转换集中在此处理，
        // 与双击同款三件套，避免菜单直接 set_editing_node 被 Idle 分支清掉。
        self.handle_context_menu_requests(ui);

        // 然后根据当前状态处理持续性事件
        self.handle_continuous_events(ui, &target);

        // 处理状态特定的每帧逻辑
        self.handle_state_specific_updates(ui);

        // 退出编辑边沿检测：把"editing_node 从 Some(x) 变为非 x"作为 resolve 的统一触发时机，
        // 收口所有退出路径（Idle 每帧清空、Escape、点击空白/其它节点、Ctrl+Enter）。
        // 只在真正退出那一帧对刚退出的节点 resolve 一次，不每帧重复。
        self.resolve_on_exit_edit();
    }

    /// 检测 `editing_node` 的退出边沿并对刚退出的节点触发 wikilink 投影。
    ///
    /// `prev` 与当前帧的 `editing_node` 比较：若 `prev = Some(x)` 且当前不再是 `x`
    /// （变成 `None` 或换到了别的节点），说明 `x` 退出了编辑——此时把 `x` 正文里的
    /// `[[标题]]` 幂等投影到图上（先清旧 wiki 边再按当前正文重建）。
    fn resolve_on_exit_edit(&mut self) {
        let current = self
            .context
            .graph_resource
            .read_resource(|g| g.get_editing_node());

        if let Some(exited) = self.prev_editing_node {
            if current != Some(exited) {
                self.context.graph_resource.with_resource(|graph| {
                    crate::wikilink::resolve_links(
                        graph,
                        &self.context.canvas_state_resource,
                        exited,
                    );
                });

                // 退出编辑 = 一个撤销单元的收尾：把进入编辑时暂存的快照按"图数据是否真变了"
                // 提交（编辑期文本改动 + 本次 resolve 一起算一步）或丢弃（双击进编辑没改就退出）。
                // current 在写闭包外单独 read 克隆（§3.1：绝不在写闭包内重入读同资源）。
                let after = self.context.graph_resource.read_resource(|g| g.clone());
                self.context.history.commit_staged_if_changed(&after);
            }
        }

        self.prev_editing_node = current;
    }

    /// 处理可能触发状态转换的一次性事件
    ///
    /// `pointer_in_canvas`：指针是否落在 canvas 区域内。**进入**新交互态的 press / double-click
    /// 必须发生在 canvas 内，否则（如点右侧面板）不分发，避免误清选中、误入框选。
    /// release 与键盘事件不门控——拖拽中把指针移出 canvas 再释放，仍须正常 finalize 收尾，
    /// 不得让状态机泄漏在 Dragging/Selecting。
    fn handle_one_shot_events(
        &mut self,
        ui: &mut egui::Ui,
        target: &InputTarget,
        pointer_in_canvas: bool,
    ) {
        // 检查鼠标点击（进入交互态，须落在 canvas 内）
        if pointer_in_canvas && ui.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
            self.handle_primary_button_press(ui, target);
        }

        if pointer_in_canvas && ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            self.handle_secondary_button_press(ui, target);
        }

        // 检查鼠标释放（收尾，不门控——拖拽出界释放也要正常结束）
        if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
            self.handle_primary_button_release(ui, target);
        }

        if ui.input(|i| i.pointer.button_released(PointerButton::Secondary)) {
            self.handle_secondary_button_release(ui, target);
        }

        // 检查键盘按键
        if ui.input(|i| i.key_pressed(Key::Space)) {
            self.handle_space_key_press();
        }
        if ui.input(|i| i.key_released(Key::Space)) {
            self.handle_space_key_release();
        }

        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.handle_escape_key();
        }

        if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            self.handle_delete_key();
        }

        // 检查双击（进入编辑/建点，须落在 canvas 内）
        if pointer_in_canvas
            && ui.input(|i| i.pointer.button_double_clicked(PointerButton::Primary))
        {
            self.handle_double_click(ui, target);
        }
    }

    /// 消费右键菜单写入的「进入编辑 / 新建并编辑」请求（temp-data 反向总线，§3.4 输入唯一驱动）。
    ///
    /// 菜单项（`app.rs` 即时 UI）拿不到 `&mut InputStateManager`，故只把意图写入 `ctx` temp data；
    /// 真正的状态转换在此一次性消费——与 `handle_double_click` **完全同款**三件套，保证编辑态由状态机
    /// 持有、不被 `Idle` 分支清掉，且经 `stage_edit_snapshot` 在退出时 `resolve_on_exit_edit` 提交为
    /// 可撤销单元。读到请求即 `remove`（消费一次）。
    ///
    /// 失效容错（§3.3）：`EditNodeRequest` 的目标节点可能在菜单跨帧打开期间被删——`get_node` 为
    /// `None` 则**丢弃请求、不转换**（绝不 `.unwrap()` / 不 panic）。
    fn handle_context_menu_requests(&mut self, ui: &mut egui::Ui) {
        use crate::ui::context_menu::{
            CreateNodeRequest, EditNodeRequest, CREATE_NODE_REQUEST_KEY, EDIT_NODE_REQUEST_KEY,
        };

        // 「编辑标题」：让目标节点进入编辑态（与双击节点同语义）。
        // 用 get_temp + remove 消费（egui 的 remove_temp 要求 T: Default，与 show_context_menu 同款），
        // 读到即移除（消费一次）。
        let edit_id = Id::new(EDIT_NODE_REQUEST_KEY);
        let edit_req: Option<EditNodeRequest> = ui.ctx().data(|d| d.get_temp(edit_id));
        if let Some(req) = edit_req {
            ui.ctx().data_mut(|d| d.remove::<EditNodeRequest>(edit_id));
            let node_index = req.node;
            // 容错：目标节点是否仍存在（跨帧打开期间可能已被删，§3.3）。失效则丢弃请求不转换。
            let exists = self
                .context
                .graph_resource
                .read_resource(|g| g.get_node(node_index).is_some());
            if exists {
                // 与 handle_double_click(Node) 同款三件套：先 stage（写闭包外 read 克隆，§3.1），
                // 再 select + set_editing_node，最后 transition_to(EditingNode)。
                self.stage_edit_snapshot();
                self.context.graph_resource.with_resource(|graph| {
                    graph.selected.clear();
                    graph.select_node(node_index);
                    graph.set_editing_node(Some(node_index));
                });
                self.transition_to(InputState::EditingNode { node_index });
            }
        }

        // 「在此新建节点」：在给定画布坐标新建节点并立即进入编辑（与双击空白建点同语义，create+edit
        // 为单一撤销单元）。同样用 get_temp + remove 消费（remove_temp 要求 T: Default）。
        let create_id = Id::new(CREATE_NODE_REQUEST_KEY);
        let create_req: Option<CreateNodeRequest> = ui.ctx().data(|d| d.get_temp(create_id));
        if let Some(req) = create_req {
            ui.ctx()
                .data_mut(|d| d.remove::<CreateNodeRequest>(create_id));
            // 与 handle_double_click(Canvas) 同款：stage_edit_snapshot 合并“建点 + 编辑期改动 +
            // 退出 resolve”为一个撤销单元；new_node_id + add_node + select + set_editing 在同一写闭包内
            // 一气呵成（§3.1：clone 的 new_node_id 在写闭包外读出）。
            self.stage_edit_snapshot();

            let new_node_id = self
                .context
                .canvas_state_resource
                .read_resource(|cs| cs.new_node_id());
            let node = crate::graph::node::Node {
                id: new_node_id,
                position: req.canvas_pos,
                text: String::new(),
                note: String::new(),
                aliases: Vec::new(),
            };
            let node_index = self.context.graph_resource.with_resource(|graph| {
                let idx = graph.add_node(node);
                graph.selected.clear();
                graph.select_node(idx);
                graph.set_editing_node(Some(idx));
                idx
            });

            self.transition_to(InputState::EditingNode { node_index });
        }
    }

    /// 处理持续性事件
    fn handle_continuous_events(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 处理鼠标移动
        if self.current_state.handles_mouse_motion() {
            let delta = ui.input(|i: &egui::InputState| i.pointer.delta());
            if delta != Vec2::ZERO {
                log::trace!("delta: {:?}", delta);
                self.handle_mouse_motion(ui, delta, target);
            }
        }

        // 处理滚动：指针悬在节点卡片上时，把滚动让给卡片内部 ScrollArea，画布不平移（避免里外都滚）
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta != Vec2::ZERO
            && self
                .context
                .hit_test_node(ui, self.context.current_mouse_pos)
                .is_none()
        {
            self.handle_scroll(scroll_delta);
        }

        // 处理缩放
        let zoom_delta = ui.input(|i: &egui::InputState| i.zoom_delta());
        if zoom_delta != 1.0 {
            self.handle_zoom(zoom_delta);
        }
    }

    /// 处理状态特定的更新逻辑
    fn handle_state_specific_updates(&mut self, ui: &mut egui::Ui) {
        match &self.current_state {
            InputState::Idle => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                self.context.graph_resource.with_resource(|graph| {
                    graph.set_editing_node(None);
                });
            }
            InputState::Panning {
                last_cursor_pos: _,
                dragging: _,
            } => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            InputState::Selecting {
                start_pos,
                current_pos,
                add_to_selection,
            } => {
                let start = *start_pos;
                let current = *current_pos;
                let add = *add_to_selection;

                // 实时更新选择范围内的节点
                self.update_selection_preview(start, current, add);

                // 绘制选择框
                self.draw_selection_rect(ui, start, current);
            }
            InputState::CreatingEdge {
                source_node,
                current_cursor_pos,
            } => {
                // 绘制临时边
                self.draw_temp_edge(ui, *source_node, *current_cursor_pos);
            }
            // 其他状态的特定更新...
            _ => {}
        }
    }

    /// 确定当前鼠标位置的输入目标
    fn determine_target(&self, ui: &egui::Ui) -> InputTarget {
        let cursor_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO);

        // 首先检查节点（优先级最高，短路）。命中节点时清空 hover 边（节点压在边之上）。
        if let Some(node_index) = self.context.hit_test_node(ui, cursor_pos) {
            set_hovered_edge(ui.ctx(), None);
            return InputTarget::Node(node_index);
        }

        // 边命中：性能短路——只在 Idle 态且节点已优先未命中时才遍历边（O(边数×采样)/帧）。
        // 非 Idle（拖拽/框选/连边中）不需要边目标，跳过遍历同时清空 hover 高亮。
        if matches!(self.current_state, InputState::Idle) {
            if let Some(edge_index) = self.context.hit_test_edge(ui, cursor_pos) {
                set_hovered_edge(ui.ctx(), Some(edge_index));
                return InputTarget::Edge(edge_index);
            }
        }
        // 未命中边（或非 Idle）：清空 hover 高亮，避免上一帧的高亮残留。
        set_hovered_edge(ui.ctx(), None);

        // 默认为画布
        InputTarget::Canvas
    }

    // 下面是各种事件处理器...

    fn handle_primary_button_press(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        match target {
            InputTarget::Node(node_index) => {
                if matches!(
                    self.current_state,
                    InputState::EditingNode { node_index: _ }
                ) {
                    return;
                }
                // 点击节点 - 开始拖动或选择
                let shift_pressed = ui.input(|i| i.modifiers.shift);

                // 检查节点是否已经在选中状态
                let node_already_selected = self
                    .context
                    .graph_resource
                    .read_resource(|graph| graph.is_node_selected(*node_index));

                if shift_pressed {
                    // 添加到现有选择
                    self.context.graph_resource.with_resource(|graph| {
                        graph.select_node(*node_index);
                    });
                } else if node_already_selected {
                    // 如果点击的是已选中的节点，不改变选择状态，准备拖动所有选中的节点
                    let selected_nodes = self
                        .context
                        .graph_resource
                        .read_resource(|graph| graph.get_selected_nodes());

                    // 进入拖拽前暂存一份快照（拖拽 = 一个撤销单元；§3.1：写闭包外单独 read 克隆）。
                    // release 时若节点确有位移则 commit，否则 discard（原地点击不产生空撤销项）。
                    self.stage_drag_snapshot();
                    self.transition_to(InputState::DraggingNode {
                        node_index: *node_index,
                        start_pos: self.context.current_mouse_pos,
                        is_selection_drag: true,
                        selected_indices: selected_nodes,
                    });
                } else {
                    // 新的选择
                    self.context.graph_resource.with_resource(|graph| {
                        graph.selected.clear();
                        graph.select_node(*node_index);
                    });

                    // 进入拖拽前暂存一份快照（同上）。
                    self.stage_drag_snapshot();
                    // 开始拖动节点
                    self.transition_to(InputState::DraggingNode {
                        node_index: *node_index,
                        start_pos: self.context.current_mouse_pos,
                        is_selection_drag: false,
                        selected_indices: vec![*node_index],
                    });
                }
            }
            InputTarget::Edge(edge_index) => {
                // 点击边 → 选中（选中真源唯一走 GraphSelection::Edge，§5 不碰 bezier 失效字段）。
                // 编辑节点中点边：先退出编辑（与点 Canvas 一致），不改选中。
                if matches!(
                    self.current_state,
                    InputState::EditingNode { node_index: _ }
                ) {
                    self.transition_to(InputState::Idle);
                } else {
                    let shift_pressed = ui.input(|i| i.modifiers.shift);
                    self.context.graph_resource.with_resource(|graph| {
                        if shift_pressed {
                            // Shift 追加多选：已是边选区则去重追加，否则新建边选区（替换异类选中）。
                            match &mut graph.selected {
                                crate::graph::selection::GraphSelection::Edge(edges) => {
                                    if !edges.contains(edge_index) {
                                        edges.push(*edge_index);
                                    }
                                }
                                _ => graph.select_edge(*edge_index),
                            }
                        } else {
                            // 普通点击：替换为仅含本边的选区。
                            graph.selected.clear();
                            graph.select_edge(*edge_index);
                        }
                    });
                }
                // 边目前不进入拖拽态，停留 Idle。
            }
            InputTarget::Canvas => {
                // self.context.graph_resource.with_resource(|graph| {
                //     graph.selected.clear();
                // });
                match &self.current_state {
                    InputState::EditingNode { node_index: _ } => {
                        self.transition_to(InputState::Idle);
                    }
                    _ => {
                        // 点击空白区域 - 开始框选
                        let shift_pressed = ui.input(|i: &egui::InputState| i.modifiers.shift);
                        let space_pressed = ui.input(|i: &egui::InputState| i.key_down(Key::Space));

                        if !shift_pressed {
                            // 清除现有选择
                            self.context.graph_resource.with_resource(|graph| {
                                graph.selected.clear();
                            });
                        }

                        if space_pressed {
                            self.transition_to(InputState::Panning {
                                last_cursor_pos: self.context.current_mouse_pos,
                                dragging: true,
                            });
                        } else {
                            self.transition_to(InputState::Selecting {
                                start_pos: self.context.current_mouse_pos,
                                current_pos: self.context.current_mouse_pos,
                                add_to_selection: shift_pressed,
                            });
                        }
                    }
                }
            }
            // 处理其他目标...
            _ => {}
        }

        self.context
            .pressed_buttons
            .set(PointerButton::Primary, true);
    }

    fn handle_secondary_button_press(&mut self, _ui: &mut egui::Ui, target: &InputTarget) {
        // 只在空闲态响应右键按下。
        if !matches!(self.current_state, InputState::Idle) {
            return;
        }

        // 右键单击 vs 右键拖拽消歧（§3.4 核心难点）：按下**不立即**进 CreatingEdge，而是先进
        // PendingSecondary 暂存命中目标与按下坐标。后续由 motion（超阈值→连边）/ release
        // （未超阈值→弹上下文菜单）单点决策，避免右键单击被当成空连边手势。
        self.transition_to(InputState::PendingSecondary {
            target: target.clone(),
            start_pos: self.context.current_mouse_pos,
        });

        self.context
            .pressed_buttons
            .set(PointerButton::Secondary, true);
    }

    fn handle_primary_button_release(&mut self, _ui: &mut egui::Ui, _target: &InputTarget) {
        match &self.current_state {
            InputState::Panning {
                last_cursor_pos,
                dragging,
            } => {
                if *dragging {
                    self.transition_to(InputState::Panning {
                        last_cursor_pos: *last_cursor_pos,
                        dragging: false,
                    });
                } else {
                    self.transition_to(InputState::Idle);
                }
            }
            InputState::DraggingNode { .. } => {
                // 结束节点拖动：把进入拖拽时暂存的快照按"是否真的移动了"提交或丢弃。
                self.finalize_drag_snapshot();
                self.transition_to(InputState::Idle);
            }
            InputState::Selecting {
                start_pos,
                current_pos,
                add_to_selection,
            } => {
                // 结束选择
                self.finalize_selection(*start_pos, *current_pos, *add_to_selection);
                self.transition_to(InputState::Idle);
            }
            // 处理其他状态...
            _ => {}
        }

        self.context
            .pressed_buttons
            .set(PointerButton::Primary, false);
    }

    fn handle_secondary_button_release(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 右键释放的**唯一**消歧/收尾决策点（§3.4：一次性事件管转换，集中一处）：
        // - PendingSecondary：从未超阈值 → 判为"右键单击" → 写上下文菜单请求（自管弹出）。
        // - CreatingEdge：右键拖拽已升级为连边手势 → 按落点连边 / 建点+连边。
        match self.current_state {
            InputState::PendingSecondary {
                target: ref menu_target,
                start_pos,
            } => {
                // 二次确认位移仍在阈值内（motion 已会把超阈值的升级为 CreatingEdge，到这里基本必然
                // 是单击；保险起见再判一次，超阈值则当作未连成的拖拽，不弹菜单）。
                let moved = (self.context.current_mouse_pos - start_pos).length();
                if moved <= SECONDARY_DRAG_THRESHOLD {
                    use crate::ui::context_menu::{
                        request_context_menu, ContextMenuRequest, ContextMenuTarget,
                    };
                    // 右键命中节点但该节点不在当前选区 → 先 clear+select 为单选（与主流编辑器右键语义
                    // 一致，§3.4 状态机内改动）。避免「删除选中 N 个」的作用对象与右键命中节点分裂导致误删。
                    if let InputTarget::Node(node_index) = menu_target {
                        let node_index = *node_index;
                        let already_selected = self
                            .context
                            .graph_resource
                            .read_resource(|g| g.is_node_selected(node_index));
                        if !already_selected {
                            self.context.graph_resource.with_resource(|g| {
                                g.selected.clear();
                                g.select_node(node_index);
                            });
                        }
                    }
                    // 命中目标 → 菜单类型。菜单弹出位置 = 按下时的屏幕坐标（菜单锚点更直觉）。
                    let menu = match menu_target {
                        InputTarget::Node(node_index) => ContextMenuTarget::Node(*node_index),
                        InputTarget::Edge(edge_index) => ContextMenuTarget::Edge(*edge_index),
                        // 画布及其它（ControlPoint/UI 当前不会出现在 secondary 命中里）一律按空白处理。
                        _ => ContextMenuTarget::Canvas,
                    };
                    request_context_menu(
                        ui.ctx(),
                        ContextMenuRequest {
                            target: menu,
                            screen_pos: start_pos,
                        },
                    );
                }
                self.transition_to(InputState::Idle);
            }
            InputState::CreatingEdge {
                source_node,
                current_cursor_pos: _,
            } => {
                match target {
                    InputTarget::Node(target_node) => {
                        // 创建边到目标节点
                        if source_node != *target_node {
                            self.create_edge(source_node, *target_node);
                        }
                    }
                    InputTarget::Canvas => {
                        // 在鼠标位置创建新节点，然后连接
                        let canvas_pos = self
                            .context
                            .screen_to_canvas(self.context.current_mouse_pos);
                        self.create_node_with_edge(source_node, canvas_pos);
                    }
                    // 处理其他目标...
                    _ => {}
                }

                // 回到空闲状态
                self.transition_to(InputState::Idle);
            }
            _ => {}
        }

        self.context
            .pressed_buttons
            .set(PointerButton::Secondary, false);
    }

    fn handle_mouse_motion(&mut self, _ui: &mut egui::Ui, delta: Vec2, _target: &InputTarget) {
        // if self.context.pressed_buttons.get(PointerButton::Primary) {
        //     println!("draw_particle_system");
        //     self.draw_particle_system(ui, ui.available_rect_before_wrap());
        // }

        match &self.current_state {
            InputState::PendingSecondary { target, start_pos } => {
                // 右键拖拽超阈值 → 升级为连边手势。位移以屏幕坐标算（阈值是屏幕像素，与缩放无关）。
                let moved = (self.context.current_mouse_pos - *start_pos).length();
                if moved > SECONDARY_DRAG_THRESHOLD {
                    // 只有从节点出发的右键拖拽才连边（拖到节点/空白由 release 收尾）；从空白 / 边
                    // 出发的右键拖拽无连边语义，直接回 Idle（不弹菜单、不连边）。
                    if let InputTarget::Node(node_index) = target {
                        let source = *node_index;
                        self.transition_to(InputState::CreatingEdge {
                            source_node: source,
                            current_cursor_pos: self.context.current_mouse_pos,
                        });
                    } else {
                        self.transition_to(InputState::Idle);
                    }
                }
            }
            InputState::Panning {
                last_cursor_pos: _,
                dragging,
            } => {
                if *dragging {
                    // 更新平移
                    self.context.canvas_state_resource.with_resource(|state| {
                        state.transform.translation += delta;
                    });

                    // 更新状态，保持平移
                    self.transition_to(InputState::Panning {
                        last_cursor_pos: self.context.current_mouse_pos,
                        dragging: true,
                    });
                }
            }
            InputState::DraggingNode {
                node_index,
                start_pos: _,
                is_selection_drag,
                selected_indices,
            } => {
                // 计算缩放调整后的增量
                let scaled_delta = delta
                    / self
                        .context
                        .canvas_state_resource
                        .read_resource(|s| s.transform.scaling);

                if *is_selection_drag {
                    // 移动所有选中的节点
                    for &idx in selected_indices {
                        self.context.graph_resource.with_resource(|graph| {
                            if let Some(node) = graph.get_node_mut(idx) {
                                node.position += scaled_delta;
                            }
                        });
                    }
                } else {
                    // 只移动当前节点
                    self.context.graph_resource.with_resource(|graph| {
                        if let Some(node) = graph.get_node_mut(*node_index) {
                            node.position += scaled_delta;
                        }
                    });
                }
            }
            InputState::Selecting {
                start_pos,
                current_pos: _,
                add_to_selection,
            } => {
                // 更新选择框的当前位置
                self.transition_to(InputState::Selecting {
                    start_pos: *start_pos,
                    current_pos: self.context.current_mouse_pos,
                    add_to_selection: *add_to_selection,
                });
            }
            InputState::CreatingEdge {
                source_node,
                current_cursor_pos: _,
            } => {
                // 更新临时边的终点
                self.transition_to(InputState::CreatingEdge {
                    source_node: *source_node,
                    current_cursor_pos: self.context.current_mouse_pos,
                });
            }
            // 处理其他状态...
            _ => {}
        }
    }

    fn handle_space_key_press(&mut self) {
        if matches!(self.current_state, InputState::Idle) {
            self.transition_to(InputState::Panning {
                last_cursor_pos: self.context.current_mouse_pos,
                dragging: false,
            });
        }
    }
    fn handle_space_key_release(&mut self) {
        if matches!(
            self.current_state,
            InputState::Panning {
                last_cursor_pos: _,
                dragging: _
            }
        ) {
            self.transition_to(InputState::Idle);
        }
    }

    fn handle_escape_key(&mut self) {
        // Escape 收口两件正交的事，拆开处理，避免“仅在非 Idle 才清选区”的旧短路（§3.4 不变量：
        // Escape 能从任意态回 Idle）：
        // 1) 中止进行中的手势 —— 仅当非 Idle 时回 Idle（保留“Escape 打断拖拽/框选/连边”原语义）。
        // 2) 清选区 + 退出编辑 —— 始终执行（含 Idle 态）。否则点边/点节点落定后停在 Idle，
        //    Esc 永远清不掉高亮（选区/editing 均为运行态 #[serde(skip)]，非图变更，不打快照、不影响 undo）。
        if !matches!(self.current_state, InputState::Idle) {
            // 拖拽中途被 Escape 打断：节点位移已落到图上（不回滚），把暂存快照按是否真移动了提交/丢弃，
            // 避免暂存快照泄漏到下一次操作（§3.4：每个拖拽态收尾不得泄漏）。
            if matches!(self.current_state, InputState::DraggingNode { .. }) {
                self.finalize_drag_snapshot();
            }
            self.transition_to(InputState::Idle);
        }

        // 清除选择 / 退出编辑（始终执行，含 Idle 态）。
        self.context.graph_resource.with_resource(|graph| {
            graph.selected.clear();
            graph.set_editing_node(None);
        });
    }

    fn handle_delete_key(&mut self) {
        if matches!(
            self.current_state,
            InputState::EditingNode { node_index: _ }
        ) {
            return;
        }

        use crate::graph::selection::GraphSelection;

        // 在写闭包外读出选区是否为空（§3.1：读锁闭包结束即释放，不与后续写闭包重入）。
        // 空选不打快照，避免空撤销项。
        let empty = self
            .context
            .graph_resource
            .read_resource(|graph| match &graph.selected {
                GraphSelection::Node(ns) => ns.is_empty(),
                GraphSelection::Edge(es) => es.is_empty(),
                GraphSelection::None => true,
            });
        if empty {
            return;
        }

        // 删除选区（节点连带邻接边 / 边只删边）——经 history 打一次快照可撤销（§3.3 索引稳定，
        // Ctrl+Z 整组复活）。remove_selected 是纯图层方法，内部已清空 selected（防悬空索引）。
        self.context
            .history
            .mutate(&self.context.graph_resource, |graph| {
                graph.remove_selected();
            });
    }

    fn handle_double_click(&mut self, _ui: &mut egui::Ui, target: &InputTarget) {
        match target {
            InputTarget::Node(node_index) => {
                // 进入编辑前暂存快照：把"整段编辑期文本改动 + 退出触发的 resolve"合并为一个撤销
                // 单元（spec）。退出编辑由 resolve_on_exit_edit 统一 commit/discard（§3.4 收口）。
                self.stage_edit_snapshot();
                // 双击节点开始编辑
                self.context.graph_resource.with_resource(|graph| {
                    graph.set_editing_node(Some(*node_index));
                });
                self.transition_to(InputState::EditingNode {
                    node_index: *node_index,
                });
            }
            InputTarget::Canvas => {
                // 双击画布创建新节点。进入编辑前暂存快照：把"建点 + 编辑期改动 + 退出 resolve"
                // 合并为一个撤销单元——一次撤销既删掉新建的空节点也撤销其后续编辑。
                self.stage_edit_snapshot();

                let canvas_pos = self
                    .context
                    .screen_to_canvas(self.context.current_mouse_pos);
                let new_node_id = self
                    .context
                    .canvas_state_resource
                    .read_resource(|cs| cs.new_node_id());

                let node = crate::graph::node::Node {
                    id: new_node_id,
                    position: canvas_pos,
                    text: String::new(),
                    note: String::new(),
                    aliases: Vec::new(),
                };

                let node_index = self.context.graph_resource.with_resource(|graph| {
                    let idx = graph.add_node(node);
                    graph.select_node(idx);
                    graph.set_editing_node(Some(idx));
                    idx
                });

                self.transition_to(InputState::EditingNode { node_index });
            }
            // 处理其他目标...
            _ => {}
        }
    }

    fn handle_scroll(&mut self, delta: Vec2) {
        // 如果没有在执行其他操作，则平移画布
        if matches!(self.current_state, InputState::Idle) {
            self.context.canvas_state_resource.with_resource(|state| {
                state.transform.translation += delta;
            });
        }
    }

    fn handle_zoom(&mut self, delta: f32) {
        // 如果没有在执行其他操作，则缩放画布
        if matches!(self.current_state, InputState::Idle) {
            let mouse_pos = self.context.current_mouse_pos;

            self.context.canvas_state_resource.with_resource(|state| {
                let scaling = state.transform.scaling;
                if scaling <= 0.1 && delta < 1.0 || scaling >= 100.0 && delta > 1.0 {
                    return;
                }
                let pointer_in_layer = state.transform.inverse() * mouse_pos;

                // 缩放，保持鼠标下方的点不变
                state.transform = state.transform
                    * egui::emath::TSTransform::from_translation(pointer_in_layer.to_vec2())
                    * egui::emath::TSTransform::from_scaling(delta)
                    * egui::emath::TSTransform::from_translation(-pointer_in_layer.to_vec2());

                // 最终scaling截断
                state.transform.scaling = state.transform.scaling.clamp(0.1, 100.0);
            });
        }
    }

    // 辅助方法

    /// 进入节点拖拽前暂存一份"变更前"整图快照（撤销/重做）。
    ///
    /// 在写闭包**之外**单独 `read_resource` 克隆 before（§3.1：绝不在写闭包内重入读同资源）。
    /// `stage` 自身幂等（已有暂存时忽略），故同一次拖拽里即便重复调用也只记一份起点。
    fn stage_drag_snapshot(&self) {
        let before = self.context.graph_resource.read_resource(|g| g.clone());
        self.context.history.stage(before);
    }

    /// 进入编辑前暂存一份"变更前"整图快照（编辑期改动 + 退出 resolve 合并为一个撤销单元）。
    /// 与 [`Self::stage_drag_snapshot`] 同构，单独抽出以表意。
    fn stage_edit_snapshot(&self) {
        let before = self.context.graph_resource.read_resource(|g| g.clone());
        self.context.history.stage(before);
    }

    /// 节点拖拽结束时收尾暂存快照：图数据确有改变（节点真的移动了）则提交为一个撤销单元，
    /// 否则丢弃（原地点击未拖动不产生空撤销项）。current 在写闭包外单独 read 克隆（§3.1）。
    fn finalize_drag_snapshot(&self) {
        let after = self.context.graph_resource.read_resource(|g| g.clone());
        self.context.history.commit_staged_if_changed(&after);
    }

    fn update_selection_preview(
        &mut self,
        start_pos: Pos2,
        current_pos: Pos2,
        add_to_selection: bool,
    ) {
        // 转换为画布坐标
        let start_canvas = self.context.screen_to_canvas(start_pos);
        let current_canvas = self.context.screen_to_canvas(current_pos);

        // 计算选择矩形
        let min_x = start_canvas.x.min(current_canvas.x);
        let min_y = start_canvas.y.min(current_canvas.y);
        let max_x = start_canvas.x.max(current_canvas.x);
        let max_y = start_canvas.y.max(current_canvas.y);

        let selection_rect =
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y));

        // 找出在矩形内的节点
        self.context.graph_resource.with_resource(|graph| {
            // 首先找出新选中的节点
            let new_selected: Vec<NodeIndex> = graph
                .graph
                .node_indices()
                .filter(|&idx| {
                    if let Some(node) = graph.get_node(idx) {
                        selection_rect.contains(node.position)
                    } else {
                        false
                    }
                })
                .collect();

            // 根据是否添加到现有选择处理
            if add_to_selection {
                if let crate::graph::selection::GraphSelection::Node(ref mut selected) =
                    graph.selected
                {
                    // 将新选中的节点添加到现有选择中
                    for &idx in &new_selected {
                        if !selected.contains(&idx) {
                            selected.push(idx);
                        }
                    }
                } else {
                    graph.selected = crate::graph::selection::GraphSelection::Node(new_selected);
                }
            } else {
                // 替换现有选择
                graph.selected = crate::graph::selection::GraphSelection::Node(new_selected);
            }
        });
    }

    fn finalize_selection(&mut self, start_pos: Pos2, current_pos: Pos2, add_to_selection: bool) {
        // 最终确认选择，这里可以与预览相同或添加额外逻辑
        self.update_selection_preview(start_pos, current_pos, add_to_selection);
    }

    fn draw_selection_rect(&self, ui: &mut egui::Ui, start_pos: Pos2, current_pos: Pos2) {
        let rect = egui::Rect::from_two_pos(start_pos, current_pos);
        let painter = ui.painter();

        // 绘制虚线选择框
        let offset: f32 = ui
            .ctx()
            .data(|d| d.get_temp(Id::new("animation_offset")))
            .unwrap_or(0.0);
        crate::ui::helpers::draw_dashed_rect_with_offset(
            painter,
            rect,
            egui::Stroke::new(1.0, egui::Color32::ORANGE),
            10.0,
            5.0,
            offset,
        );

        // // 绘制半透明填充
        // painter.rect_filled(
        //     rect,
        //     0.0,
        //     egui::Color32::from_rgba_premultiplied(100, 100, 255, 40),
        // );
    }

    fn draw_temp_edge(&self, ui: &mut egui::Ui, source_node: NodeIndex, target_pos: Pos2) {
        // 获取源节点的位置
        let render_info: Option<NodeRenderInfo> = ui
            .ctx()
            .data(|d| d.get_temp(Id::new(source_node.index().to_string())));
        if let Some(render_info) = render_info {
            let source_pos = render_info.canvas_rect.center();
            // 转换为屏幕坐标
            let source_screen = self.context.canvas_to_screen(source_pos);
            // 绘制临时边
            let painter = ui.painter();
            painter.line_segment(
                [source_screen, target_pos],
                egui::Stroke::new(2.0, egui::Color32::YELLOW),
            );

            // 绘制箭头
            let dir = (target_pos - source_screen).normalized();
            let arrow_len = 10.0;
            let arrow_angle = 30.0 * std::f32::consts::PI / 180.0;

            let left = target_pos
                - arrow_len
                    * egui::vec2(
                        dir.x * arrow_angle.cos() - dir.y * arrow_angle.sin(),
                        dir.x * arrow_angle.sin() + dir.y * arrow_angle.cos(),
                    );

            let right = target_pos
                - arrow_len
                    * egui::vec2(
                        dir.x * arrow_angle.cos() + dir.y * arrow_angle.sin(),
                        -dir.x * arrow_angle.sin() + dir.y * arrow_angle.cos(),
                    );

            painter.line_segment(
                [target_pos, left],
                egui::Stroke::new(2.0, egui::Color32::YELLOW),
            );

            painter.line_segment(
                [target_pos, right],
                egui::Stroke::new(2.0, egui::Color32::YELLOW),
            );
        }
        let _source_pos = self
            .context
            .graph_resource
            .read_resource(|graph| graph.get_node(source_node).map(|node| node.position))
            .unwrap_or(Pos2::ZERO);
    }

    fn create_edge(&mut self, source: NodeIndex, target: NodeIndex) {
        // 检查边是否已存在
        let edge_exists = self
            .context
            .graph_resource
            .read_resource(|graph| graph.edge_exists(source, target));

        if !edge_exists {
            // 获取源节点和目标节点的位置
            let (source_pos, target_pos) = self.context.graph_resource.read_resource(|graph| {
                (
                    graph
                        .get_node(source)
                        .map(|n| n.position)
                        .unwrap_or_default(),
                    graph
                        .get_node(target)
                        .map(|n| n.position)
                        .unwrap_or_default(),
                )
            });

            // 创建新边
            let edge = crate::graph::edge::Edge::new(
                source,
                target,
                source_pos,
                target_pos,
                self.context.canvas_state_resource.clone(),
            );

            // 添加到图中——经 history 打一次快照（连边是一个独立撤销单元）。
            self.context
                .history
                .mutate(&self.context.graph_resource, |graph| {
                    graph.add_edge(edge);
                });
        }
    }

    fn create_node_with_edge(&mut self, source: NodeIndex, canvas_pos: Pos2) {
        // 创建新节点
        let new_node_id = self
            .context
            .canvas_state_resource
            .read_resource(|cs| cs.new_node_id());

        let node = crate::graph::node::Node {
            id: new_node_id,
            position: canvas_pos,
            text: String::new(),
            note: String::new(),
            aliases: Vec::new(),
        };

        // 添加节点并创建边——经 history 打一次快照（建点+连边是一个独立撤销单元）。
        self.context
            .history
            .mutate(&self.context.graph_resource, |graph| {
                graph.add_node_with_edge(node, source, self.context.canvas_state_resource.clone());
            });
    }

    pub fn draw_particle_system(&self, ui: &mut egui::Ui, screen_rect: egui::Rect) {
        // 获取当前帧的时间间隔
        // println!("screen_rect: {:?}", screen_rect);

        let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        let offset_pos = screen_rect.min;
        let mouse_pos = Pos2::new(mouse_pos.x - offset_pos.x, mouse_pos.y - offset_pos.y);
        // println!("mouse_pos: {:?}", mouse_pos);
        if self.context.pressed_buttons.get(PointerButton::Primary) {
            self.draw_particle_system_with_mouse_pos(ui, mouse_pos, screen_rect);
        }
    }

    fn draw_particle_system_with_mouse_pos(
        &self,
        ui: &mut egui::Ui,
        mouse_pos: Pos2,
        screen_rect: egui::Rect,
    ) {
        let dt = ui.ctx().input(|i| i.stable_dt);
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            screen_rect,
            ParticleCallback::new([mouse_pos.x, mouse_pos.y], dt, screen_rect),
        ));
    }
}
