use egui::emath::TSTransform;

use super::{
    data::CanvasWidget,
    input::make_input_busy,
    input_detector::{primary_button_down, scrolling, space_pressed, zooming},
};

impl CanvasWidget {
    pub fn handle_pan(&mut self, ui: &mut egui::Ui) {
        // 处理拖拽平移
        if space_pressed(ui) {
            // 设置鼠标指针为手型
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if primary_button_down(ui) {
                self.canvas_state_resource
                    .with_canvas_state(|canvas_state| {
                        // drag_delta() 表示本次帧被拖拽的增量
                        let drag_delta = ui.input(|i| i.pointer.delta());

                        canvas_state.transform.translation += drag_delta;
                    });
                make_input_busy(ui);
            }
        }

        // 处理滚动平移
        if let Some(scroll_delta) = scrolling(ui) {
            self.canvas_state_resource
                .with_canvas_state(|canvas_state| {
                    canvas_state.transform.translation += scroll_delta;
                });
            make_input_busy(ui);
        }
    }

    pub fn handle_scale(&mut self, ui: &mut egui::Ui) {
        // 处理缩放
        if zooming(ui) {
            // 计算鼠标指针相对于画布原点的偏移
            self.canvas_state_resource
                .with_canvas_state(|canvas_state| {
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
}
