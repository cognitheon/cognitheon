use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Id, Widget};
use petgraph::graph::NodeIndex;

use crate::{
    canvas::CanvasState,
    graph::{
        edge::{TempEdge, TempEdgeTarget},
        node::Node,
        Graph,
    },
};

use super::{helpers::draw_grid, temp_edge::TempEdgeWidget};

pub struct CanvasWidget<'a> {
    pub canvas_state: &'a mut CanvasState,
    pub graph: &'a mut Graph,
    global_node_id: AtomicU64,
}

impl<'a> CanvasWidget<'a> {
    pub fn new(canvas_state: &'a mut CanvasState, graph: &'a mut Graph) -> Self {
        Self {
            canvas_state,
            graph,
            global_node_id: AtomicU64::new(0),
        }
    }

    pub fn hit_test(&self, pos: egui::Pos2) -> Option<NodeIndex> {
        self.graph.graph.node_indices().find_map(|node_index| {
            let node = self.graph.get_node(node_index).unwrap();
            if node.render_info.is_some()
                && node.render_info.as_ref().unwrap().screen_rect.contains(pos)
            {
                Some(node_index)
            } else {
                None
            }
        })
    }

    pub fn configure_actions(&mut self, ui: &mut egui::Ui, canvas_response: &egui::Response) {
        // =====================
        // 1. 处理缩放 (鼠标滚轮)
        // =====================
        // if canvas_response.hovered() {
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

            // 计算鼠标指针相对于画布原点的偏移
            let mouse_canvas_pos = (mouse_pos - self.canvas_state.offset) / self.canvas_state.scale;

            // 保存旧的缩放值
            // let old_scale = self.canvas_state.scale;

            // 更新缩放值
            self.canvas_state.scale *= zoom_delta;
            self.canvas_state.scale = self.canvas_state.scale.clamp(0.1, 100.0);

            // 计算新的偏移量，保持鼠标位置不变
            self.canvas_state.offset = mouse_pos - (mouse_canvas_pos * self.canvas_state.scale);
        }
        // }

        // =====================
        // 2. 处理平移 (拖拽)
        // =====================
        // if canvas_response.dragged() {
        // 只有按住空格键且用鼠标左键时，才允许拖拽
        if ui.input(|i| i.key_down(egui::Key::Space)) {
            // 设置鼠标指针为手型
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                // drag_delta() 表示本次帧被拖拽的增量
                let drag_delta = canvas_response.drag_delta();
                self.canvas_state.offset += drag_delta;
            }
        }
        // }

        // if canvas_response.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta != egui::Vec2::ZERO {
            self.canvas_state.offset += scroll_delta;
        }
        // }

        // 处理双击
        if canvas_response.hovered() {
            if ui.input(|i: &egui::InputState| {
                i.key_pressed(egui::Key::Escape)
                    || i.pointer.button_clicked(egui::PointerButton::Primary)
            }) {
                self.graph.set_selected_node(None);
                self.graph.set_editing_node(None);
            }

            if ui.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            }) {
                if let Some(screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    // 将屏幕坐标转换为画布坐标
                    let canvas_pos = self.canvas_state.to_canvas(screen_pos);
                    let node = Node {
                        id: self.global_node_id.fetch_add(1, Ordering::Relaxed),
                        position: canvas_pos,
                        text: String::new(),
                        note: String::new(),
                        render_info: None,
                    };
                    let node_index = self.graph.add_node(node.clone());
                    self.graph.set_selected_node(Some(node_index));
                    self.graph.set_editing_node(Some(node_index));
                    // self.editing_text = Some((canvas_pos, String::new()));
                }
                println!("double clicked");
            }
        }

        // 处理右键拖动
        if ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary)) {
            let mut graph: Graph = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<Graph>(Id::new("graph")))
                .unwrap();
            let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
            let canvas_pos = self.canvas_state.to_canvas(mouse_pos);
            if let Some(node_index) = self.hit_test(mouse_pos) {
                // 创建临时边
                let temp_edge = TempEdge {
                    source: node_index,
                    target: TempEdgeTarget::Point(canvas_pos),
                };
                graph.set_creating_edge(Some(temp_edge));
            }
        }

        if canvas_response.drag_started() {}
    }
}

impl<'a> Widget for CanvasWidget<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = ui.available_size();
        let (canvas_rect, canvas_response) =
            ui.allocate_exact_size(desired_size, egui::Sense::drag());

        // println!("desired_size: {:?}", desired_size);
        // println!("canvas_rect: {:?}", canvas_rect);

        draw_grid(ui, self.canvas_state, canvas_rect);
        // ui.add(BsplineWidget::new(
        //     vec![
        //         Pos2::new(0.0, 0.0),
        //         Pos2::new(100.0, 100.0),
        //         Pos2::new(100.0, 200.0),
        //         Pos2::new(300.0, 300.0),
        //         Pos2::new(100.0, 400.0),
        //     ],
        //     canvas_rect,
        //     self.canvas_state,
        // ));

        let creating_edge = self.graph.get_creating_edge();
        if let Some(edge) = creating_edge {
            ui.add(TempEdgeWidget::new(edge));
        }

        // ui.add(BezierWidget::new(
        //     vec![
        //         // self.graph.get_creating_edge().unwrap().source.,
        //         Anchor::new_smooth(Pos2::new(100.0, 200.0)),
        //     ],
        //     self.canvas_state,
        //     EdgeIndex::new(0),
        // ));

        crate::graph::render_graph(&mut self.graph, ui, &mut self.canvas_state);
        self.configure_actions(ui, &canvas_response);

        canvas_response
    }
}
