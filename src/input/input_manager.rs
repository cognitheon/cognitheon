use egui::{Id, Pos2};
use petgraph::graph::NodeIndex;

use crate::{
    globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource},
    graph::render_info::NodeRenderInfo,
};

use super::{detectors::*, input_state::InputState};

pub struct InputStateManager {
    current_state: InputState,
    graph_resource: GraphResource,
    canvas_state_resource: CanvasStateResource,
}

impl InputStateManager {
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            current_state: InputState::Idle,
            graph_resource,
            canvas_state_resource,
        }
    }

    pub fn get_current_state(&self) -> &InputState {
        &self.current_state
    }

    pub fn transition_to(&mut self, new_state: InputState) {
        // 可以添加从当前状态到新状态的退出/进入逻辑
        self.current_state = new_state;
    }

    pub fn handle_input(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        match &self.current_state {
            InputState::Idle => self.handle_idle_state(ui, response),
            InputState::Panning { last_cursor_pos } => {
                self.handle_panning_state(ui, *last_cursor_pos)
            }
            // InputState::DraggingNode {
            //     node_index,
            //     start_pos,
            // } => self.handle_dragging_node_state(ui, response, *node_index, *start_pos),
            // 处理其他状态...
            _ => {}
        }
    }

    fn handle_idle_state(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        // 处理当前状态下的输入，并根据条件转换到其他状态

        // 例如：空格+拖动开始平移
        if ui.input(|i| {
            i.key_down(egui::Key::Space) && i.pointer.button_pressed(egui::PointerButton::Primary)
        }) {
            if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                self.transition_to(InputState::Panning {
                    last_cursor_pos: cursor_pos,
                });
                return;
            }
        }

        // 例如：检测点击节点并开始拖动
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                // 检测鼠标是否点击在节点上
                if let Some(node_index) = self.hit_test_node(ui, cursor_pos) {
                    self.transition_to(InputState::DraggingNode {
                        node_index,
                        start_pos: cursor_pos,
                    });
                    return;
                }

                // 开始框选
                self.transition_to(InputState::Selecting {
                    start_pos: cursor_pos,
                    current_pos: cursor_pos,
                });
                return;
            }
        }

        // 其他可能的状态转换...
    }

    fn handle_panning_state(&mut self, ui: &mut egui::Ui, last_cursor_pos: Pos2) {
        // 处理平移状态逻辑

        // 设置光标样式
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        // 获取当前光标位置
        if let Some(current_pos) = ui.input(|i| i.pointer.hover_pos()) {
            // 计算偏移量并应用到画布
            let delta = current_pos - last_cursor_pos;
            self.canvas_state_resource.with_canvas_state(|state| {
                state.transform.translation += delta;
            });

            // 更新状态中的上一次光标位置
            self.transition_to(InputState::Panning {
                last_cursor_pos: current_pos,
            });
        }

        // 检测退出平移状态的条件
        if ui.input(|i| {
            i.pointer.button_released(egui::PointerButton::Primary)
                || i.key_released(egui::Key::Space)
        }) {
            self.transition_to(InputState::Idle);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
    }

    pub fn hit_test_node(&self, ui: &mut egui::Ui, screen_pos: egui::Pos2) -> Option<NodeIndex> {
        self.graph_resource.read_graph(|graph| {
            graph.graph.node_indices().find(|&node_index| {
                let node_render_info: Option<NodeRenderInfo> = ui
                    .ctx()
                    .data(|d| d.get_temp(Id::new(node_index.index().to_string())));
                // println!("node_render_info: {:?}", node_render_info);
                if let Some(node_render_info) = node_render_info {
                    let node_screen_rect =
                        self.canvas_state_resource
                            .read_canvas_state(|canvas_state| {
                                canvas_state.to_screen_rect(node_render_info.canvas_rect)
                            });
                    if node_screen_rect.contains(screen_pos) {
                        return true;
                    }
                }
                false
            })
        })
    }
}

pub fn calc_input(ui: &mut egui::Ui) -> InputState {
    // let last_input: Option<InputState> = ui.data(|d| d.get_temp(Id::NULL));
    let mut input_state = InputState::Idle;

    let cursor_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    if detect_drag_canvas(ui) {
        input_state = InputState::Panning {
            last_cursor_pos: cursor_pos,
        };
    } else if detect_select_node(ui) {
        input_state = InputState::Selecting {
            start_pos: cursor_pos,
            current_pos: cursor_pos,
        };
    }

    input_state
}

pub fn hit_test_nodes(ui: &mut egui::Ui) -> Option<NodeIndex> {
    None
}
