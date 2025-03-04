use egui::{emath::TSTransform, Color32, Id, Pos2, Rect, Response, Stroke};

use crate::{canvas::CanvasState, ui::helpers::draw_dashed_rect_with_offset};

use super::{
    data::CanvasWidget,
    input::{is_input_busy, make_input_busy, make_input_idle},
};

impl CanvasWidget {
    pub fn handle_pan(&mut self, ui: &mut egui::Ui) {
        // 处理拖拽平移
        if CanvasWidget::space_pressed(ui) {
            // 设置鼠标指针为手型
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            // println!("handle_pan");
            make_input_busy(ui);

            if CanvasWidget::primary_button_down(ui) {
                self.canvas_state_resource
                    .with_canvas_state(|canvas_state| {
                        // drag_delta() 表示本次帧被拖拽的增量

                        let drag_delta = ui.input(|i| i.pointer.delta());

                        canvas_state.transform.translation += drag_delta;
                    });
            }
        }

        // 处理滚动平移
        if let Some(scroll_delta) = CanvasWidget::scrolling(ui) {
            self.canvas_state_resource
                .with_canvas_state(|canvas_state| {
                    canvas_state.transform.translation += scroll_delta;
                });
            make_input_busy(ui);
        }

        if CanvasWidget::space_released(ui) {
            make_input_idle(ui);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
    }

    pub fn handle_scale(&mut self, ui: &mut egui::Ui) {
        // 处理缩放
        if CanvasWidget::zooming(ui) {
            // 计算鼠标指针相对于画布原点的偏移
            self.canvas_state_resource
                .with_canvas_state(|canvas_state: &mut CanvasState| {
                    let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
                    // let mouse_canvas_pos = (mouse_pos - canvas_state.offset) / canvas_state.scale;
                    // // 保存旧的缩放值
                    // // let old_scale = self.canvas_state.scale;

                    // // 更新缩放值
                    // let mut scale = canvas_state.scale;
                    // scale *= zoom_delta;
                    // scale = scale.clamp(0.1, 100.0);
                    // canvas_state.scale = scale;

                    // // 计算新的偏移量，保持鼠标位置不变
                    // // let mut offset = canvas_state.offset;
                    // let offset = mouse_pos - (mouse_canvas_pos * scale);
                    // canvas_state.offset = offset;

                    let pointer_in_layer = canvas_state.transform.inverse() * mouse_pos;
                    let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
                    let pan_delta = ui.ctx().input(|i| i.smooth_scroll_delta);

                    // 缩放的最小值为0.1，最大值为100.0
                    if canvas_state.transform.scaling < 0.1 {
                        return;
                    }
                    if canvas_state.transform.scaling > 100.0 {
                        return;
                    }

                    // Zoom in on pointer:
                    canvas_state.transform = canvas_state.transform
                        * TSTransform::from_translation(pointer_in_layer.to_vec2())
                        * TSTransform::from_scaling(zoom_delta)
                        * TSTransform::from_translation(-pointer_in_layer.to_vec2());

                    // Pan:
                    canvas_state.transform =
                        TSTransform::from_translation(pan_delta) * canvas_state.transform;
                });
            make_input_busy(ui);
        }
    }

    /// 处理拖拽选择
    pub fn handle_drag_select(&mut self, ui: &mut egui::Ui, canvas_response: &Response) {
        if is_input_busy(ui) {
            return;
        }

        if CanvasWidget::drag_select(ui, canvas_response) {
            self.graph_resource.with_graph(|graph| {
                graph.set_editing_node(None);
            });

            if self.drag_select_range.is_none() {
                let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
                self.drag_select_range = Some([mouse_pos, mouse_pos]);
            } else {
                let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
                self.drag_select_range = Some([self.drag_select_range.unwrap()[0], mouse_pos]);

                let painter = ui.painter();

                let screen_rect = Rect::from_min_max(
                    self.drag_select_range.unwrap()[0],
                    self.drag_select_range.unwrap()[1],
                );
                let offset: f32 = ui
                    .data(|d| d.get_temp(Id::new("animation_offset")))
                    .unwrap_or(0.0);
                draw_dashed_rect_with_offset(
                    painter,
                    screen_rect,
                    Stroke::new(1.0, Color32::ORANGE),
                    10.0,
                    5.0,
                    offset,
                );
                // painter.rect_stroke(
                //     rect,
                //     egui::Rounding::ZERO,
                //     egui::Stroke::new(1.0, egui::Color32::ORANGE),
                // );
            }
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.drag_select_range = None;
        }
    }

    pub fn handle_escape(&mut self, ui: &mut egui::Ui, canvas_response: &Response) {
        if CanvasWidget::escape(ui, canvas_response) {
            self.drag_select_range = None;
            self.graph_resource.with_graph(|graph| {
                graph.selected.clear();
                graph.set_editing_node(None);
            });
        }
    }

    pub fn handle_tab(&mut self, ui: &mut egui::Ui) {
        if self.tab_pressed(ui) {
            let selected_node_indices = self
                .graph_resource
                .read_graph(|graph| graph.get_selected_nodes());

            let src_node_canvas_pos_vec = selected_node_indices
                .iter()
                .map(|index| {
                    self.canvas_state_resource
                        .read_canvas_state(|canvas_state| {
                            canvas_state.to_canvas(
                                self.graph_resource
                                    .read_graph(|graph| graph.get_node(*index).unwrap().position),
                            )
                        })
                })
                .collect::<Vec<Pos2>>();
        }
    }
}
