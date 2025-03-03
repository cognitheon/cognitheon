// 在 src/input/state_manager.rs 中

use std::collections::HashMap;

use crate::{
    globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource},
    graph::render_info::NodeRenderInfo,
    input::{events::InputTarget, input_state::InputState},
};

use egui::*;
use petgraph::graph::{EdgeIndex, NodeIndex};

use super::button_state::ButtonState;

/// 存储输入处理所需的上下文数据
#[derive(Debug)]
pub struct InputContext {
    /// 画布资源
    pub canvas_state_resource: CanvasStateResource,

    /// 图形资源
    pub graph_resource: GraphResource,

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
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            canvas_state_resource,
            graph_resource,
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
        self.graph_resource.read_graph(|graph| {
            graph.graph.node_indices().find(|&node_index| {
                let node_render_info: Option<NodeRenderInfo> = ui
                    .ctx()
                    .data(|d| d.get_temp(Id::new(node_index.index().to_string())));

                if let Some(node_render_info) = node_render_info {
                    let node_screen_rect =
                        self.canvas_state_resource
                            .read_canvas_state(|canvas_state| {
                                canvas_state.to_screen_rect(node_render_info.canvas_rect)
                            });

                    return node_screen_rect.contains(screen_pos);
                }
                false
            })
        })
    }

    /// 将屏幕坐标转换为画布坐标
    pub fn screen_to_canvas(&self, screen_pos: Pos2) -> Pos2 {
        self.canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_canvas(screen_pos))
    }

    /// 将画布坐标转换为屏幕坐标
    pub fn canvas_to_screen(&self, canvas_pos: Pos2) -> Pos2 {
        self.canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen(canvas_pos))
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
}

impl InputStateManager {
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            current_state: InputState::Idle,
            context: InputContext::new(graph_resource, canvas_state_resource),
            last_target: None,
        }
    }

    /// 转换到新状态
    pub fn transition_to(&mut self, new_state: InputState) {
        // 可以在这里添加状态转换的日志或验证
        println!(
            "Input state transition: {:?} -> {:?}",
            self.current_state, new_state
        );
        self.current_state = new_state;
    }

    /// 每帧更新输入状态
    pub fn update(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        // 更新上下文
        self.context.update(ui);

        // 处理输入事件，获取当前输入目标
        let target = self.determine_target(ui);
        self.last_target = Some(target.clone());

        // 首先处理一次性事件，这些可能导致状态转换
        self.handle_one_shot_events(ui, &target);

        // 然后根据当前状态处理持续性事件
        self.handle_continuous_events(ui, &target);

        // 处理状态特定的每帧逻辑
        self.handle_state_specific_updates(ui);
    }

    /// 处理可能触发状态转换的一次性事件
    fn handle_one_shot_events(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 检查鼠标点击
        if ui.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
            self.handle_primary_button_press(ui, target);
        }

        if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            self.handle_secondary_button_press(ui, target);
        }

        // 检查鼠标释放
        if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
            self.handle_primary_button_release(ui, target);
        }

        if ui.input(|i| i.pointer.button_released(PointerButton::Secondary)) {
            self.handle_secondary_button_release(ui, target);
        }

        // 检查键盘按键
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.handle_escape_key();
        }

        if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            self.handle_delete_key();
        }

        // 检查双击
        if ui.input(|i| i.pointer.button_double_clicked(PointerButton::Primary)) {
            self.handle_double_click(ui, target);
        }
    }

    /// 处理持续性事件
    fn handle_continuous_events(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 处理鼠标移动
        if self.current_state.handles_mouse_motion() {
            let delta = ui.input(|i: &egui::InputState| i.pointer.delta());
            if delta != Vec2::ZERO {
                println!("delta: {:?}", delta);
                self.handle_mouse_motion(ui, delta, target);
            }
        }

        // 处理滚动
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta != Vec2::ZERO {
            self.handle_scroll(scroll_delta);
        }

        // 处理缩放
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            self.handle_zoom(zoom_delta);
        }
    }

    /// 处理状态特定的更新逻辑
    fn handle_state_specific_updates(&mut self, ui: &mut egui::Ui) {
        match &self.current_state {
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

        // 首先检查节点（优先级最高）
        if let Some(node_index) = self.context.hit_test_node(ui, cursor_pos) {
            return InputTarget::Node(node_index);
        }

        // 检查边和控制点...
        // （这里可以添加你特定的边和控制点检测逻辑）

        // 默认为画布
        InputTarget::Canvas
    }

    // 下面是各种事件处理器...

    fn handle_primary_button_press(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 目前只处理空闲状态下的点击
        if !matches!(self.current_state, InputState::Idle) {
            return;
        }

        match target {
            InputTarget::Node(node_index) => {
                // 点击节点 - 开始拖动或选择
                let shift_pressed = ui.input(|i| i.modifiers.shift);

                // 检查节点是否已经在选中状态
                let node_already_selected = self
                    .context
                    .graph_resource
                    .with_graph(|graph| graph.is_node_selected(*node_index));

                if shift_pressed {
                    // 添加到现有选择
                    self.context.graph_resource.with_graph(|graph| {
                        graph.select_node(*node_index);
                    });
                } else if node_already_selected {
                    // 如果点击的是已选中的节点，不改变选择状态，准备拖动所有选中的节点
                    let selected_nodes = self
                        .context
                        .graph_resource
                        .with_graph(|graph| graph.get_selected_nodes());

                    self.transition_to(InputState::DraggingNode {
                        node_index: *node_index,
                        start_pos: self.context.current_mouse_pos,
                        is_selection_drag: true,
                        selected_indices: selected_nodes,
                    });
                } else {
                    // 新的选择
                    self.context.graph_resource.with_graph(|graph| {
                        graph.selected.clear();
                        graph.select_node(*node_index);
                    });

                    // 开始拖动节点
                    self.transition_to(InputState::DraggingNode {
                        node_index: *node_index,
                        start_pos: self.context.current_mouse_pos,
                        is_selection_drag: false,
                        selected_indices: vec![*node_index],
                    });
                }
            }
            InputTarget::Canvas => {
                // 点击空白区域 - 开始框选
                let shift_pressed = ui.input(|i| i.modifiers.shift);

                if !shift_pressed {
                    // 清除现有选择
                    self.context.graph_resource.with_graph(|graph| {
                        graph.selected.clear();
                    });
                }

                self.transition_to(InputState::Selecting {
                    start_pos: self.context.current_mouse_pos,
                    current_pos: self.context.current_mouse_pos,
                    add_to_selection: shift_pressed,
                });
            }
            // 处理其他目标...
            _ => {}
        }
    }

    fn handle_secondary_button_press(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        // 目前只处理空闲状态下的右键点击
        if !matches!(self.current_state, InputState::Idle) {
            return;
        }

        match target {
            InputTarget::Node(node_index) => {
                // 开始创建从此节点出发的边
                self.transition_to(InputState::CreatingEdge {
                    source_node: *node_index,
                    current_cursor_pos: self.context.current_mouse_pos,
                });
            }
            // 处理其他目标...
            _ => {}
        }
    }

    fn handle_primary_button_release(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        match &self.current_state {
            InputState::DraggingNode { .. } => {
                // 结束节点拖动
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
    }

    fn handle_secondary_button_release(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        if let InputState::CreatingEdge {
            source_node,
            current_cursor_pos,
        } = self.current_state
        {
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
    }

    fn handle_mouse_motion(&mut self, ui: &mut egui::Ui, delta: Vec2, target: &InputTarget) {
        match &self.current_state {
            InputState::Panning { last_cursor_pos: _ } => {
                // 更新平移
                self.context
                    .canvas_state_resource
                    .with_canvas_state(|state| {
                        state.transform.translation += delta;
                    });

                // 更新状态，保持平移
                self.transition_to(InputState::Panning {
                    last_cursor_pos: self.context.current_mouse_pos,
                });
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
                        .read_canvas_state(|s| s.transform.scaling);

                if *is_selection_drag {
                    // 移动所有选中的节点
                    for &idx in selected_indices {
                        self.context.graph_resource.with_graph(|graph| {
                            if let Some(node) = graph.get_node_mut(idx) {
                                node.position += scaled_delta;
                            }
                        });
                    }
                } else {
                    // 只移动当前节点
                    self.context.graph_resource.with_graph(|graph| {
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

    fn handle_escape_key(&mut self) {
        // 几乎任何状态下，按下Escape都应该回到空闲状态
        if !matches!(self.current_state, InputState::Idle) {
            self.transition_to(InputState::Idle);

            // 清除选择
            self.context.graph_resource.with_graph(|graph| {
                graph.selected.clear();
                graph.set_editing_node(None);
            });
        }
    }

    fn handle_delete_key(&mut self) {
        // 删除选中的节点
        self.context.graph_resource.with_graph(|graph| {
            let nodes_to_remove =
                if let crate::graph::selection::GraphSelection::Node(nodes) = &graph.selected {
                    nodes.clone() // 克隆节点列表
                } else {
                    Vec::new()
                };

            for node_index in nodes_to_remove {
                graph.remove_node(node_index);
            }

            graph.selected.clear();
        });
    }

    fn handle_double_click(&mut self, ui: &mut egui::Ui, target: &InputTarget) {
        match target {
            InputTarget::Node(node_index) => {
                // 双击节点开始编辑
                self.context.graph_resource.with_graph(|graph| {
                    graph.set_editing_node(Some(*node_index));
                });
                self.transition_to(InputState::EditingNode {
                    node_index: *node_index,
                });
            }
            InputTarget::Canvas => {
                // 双击画布创建新节点
                let canvas_pos = self
                    .context
                    .screen_to_canvas(self.context.current_mouse_pos);
                let new_node_id = self
                    .context
                    .canvas_state_resource
                    .read_canvas_state(|cs| cs.new_node_id());

                let node = crate::graph::node::Node {
                    id: new_node_id,
                    position: canvas_pos,
                    text: String::new(),
                    note: String::new(),
                };

                let node_index = self.context.graph_resource.with_graph(|graph| {
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
            self.context
                .canvas_state_resource
                .with_canvas_state(|state| {
                    state.transform.translation += delta;
                });
        }
    }

    fn handle_zoom(&mut self, delta: f32) {
        // 如果没有在执行其他操作，则缩放画布
        if matches!(self.current_state, InputState::Idle) {
            let mouse_pos = self.context.current_mouse_pos;

            self.context
                .canvas_state_resource
                .with_canvas_state(|state| {
                    let pointer_in_layer = state.transform.inverse() * mouse_pos;

                    // 缩放，保持鼠标下方的点不变
                    state.transform = state.transform
                        * egui::emath::TSTransform::from_translation(pointer_in_layer.to_vec2())
                        * egui::emath::TSTransform::from_scaling(delta)
                        * egui::emath::TSTransform::from_translation(-pointer_in_layer.to_vec2());
                });
        }
    }

    // 辅助方法

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
        self.context.graph_resource.with_graph(|graph| {
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
        let source_pos = self
            .context
            .graph_resource
            .read_graph(|graph| graph.get_node(source_node).map(|node| node.position))
            .unwrap_or(Pos2::ZERO);

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

    fn create_edge(&mut self, source: NodeIndex, target: NodeIndex) {
        // 检查边是否已存在
        let edge_exists = self
            .context
            .graph_resource
            .read_graph(|graph| graph.edge_exists(source, target));

        if !edge_exists {
            // 获取源节点和目标节点的位置
            let (source_pos, target_pos) = self.context.graph_resource.read_graph(|graph| {
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

            // 添加到图中
            self.context.graph_resource.with_graph(|graph| {
                graph.add_edge(edge);
            });
        }
    }

    fn create_node_with_edge(&mut self, source: NodeIndex, canvas_pos: Pos2) {
        // 创建新节点
        let new_node_id = self
            .context
            .canvas_state_resource
            .read_canvas_state(|cs| cs.new_node_id());

        let node = crate::graph::node::Node {
            id: new_node_id,
            position: canvas_pos,
            text: String::new(),
            note: String::new(),
        };

        // 添加节点并创建边
        self.context.graph_resource.with_graph(|graph| {
            graph.add_node_with_edge(node, source, self.context.canvas_state_resource.clone());
        });
    }
}
