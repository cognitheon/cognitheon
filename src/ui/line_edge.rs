use egui::*;

use crate::{graph::anchor::LineAnchor, resource::CanvasStateResource};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LineEdge {
    pub source: LineAnchor,
    pub target: LineAnchor,
}

impl LineEdge {
    pub fn new(source: LineAnchor, target: LineAnchor) -> Self {
        Self { source, target }
    }
}

pub struct LineWidget {
    pub line_edge: LineEdge,
    pub canvas_state_resource: CanvasStateResource,
}

impl LineWidget {
    pub fn new(line_edge: LineEdge, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            line_edge,
            canvas_state_resource,
        }
    }

    fn draw_arrow(&self, ui: &mut egui::Ui) {
        let painter = ui.painter();
        let stroke = Stroke::new(2.0, Color32::GRAY);

        // 将画布坐标转换为屏幕坐标
        let source_screen_pos = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(self.line_edge.source.canvas_pos));
        let target_screen_pos = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(self.line_edge.target.canvas_pos));

        // 计算方向向量
        let dir = target_screen_pos - source_screen_pos;
        let len = dir.length();
        if len < f32::EPSILON {
            return; // 避免出现零长度向量的情况
        }
        let dir_norm = dir / len; // 单位化的方向向量

        // 箭头长度（可根据需求调整）
        let arrow_length = 10.0;

        // 向量旋转函数，用于生成箭头的左右两条短线
        fn rotate(v: Vec2, angle_rad: f32) -> Vec2 {
            let (sin, cos) = angle_rad.sin_cos();
            Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
        }

        // 计算箭头两条短线的方向（此处使用 30° 作为示例）
        let left_dir = rotate(dir_norm, 30_f32.to_radians()) * arrow_length;
        let right_dir = rotate(dir_norm, -30_f32.to_radians()) * arrow_length;

        // 在目标端画两条短线
        painter.line_segment([target_screen_pos, target_screen_pos - left_dir], stroke);
        painter.line_segment([target_screen_pos, target_screen_pos - right_dir], stroke);
    }
}

impl Widget for LineWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let painter = ui.painter();
        let source_canvas_pos = self.line_edge.source.canvas_pos;
        let target_canvas_pos = self.line_edge.target.canvas_pos;

        let source_screen_pos = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(source_canvas_pos));
        let target_screen_pos = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(target_canvas_pos));
        let stroke = Stroke::new(2.0, Color32::GRAY);
        painter.line_segment([source_screen_pos, target_screen_pos], stroke);
        self.draw_arrow(ui);

        let screen_rect = Rect::from_points(&[source_screen_pos, target_screen_pos]);
        ui.allocate_rect(screen_rect, Sense::click_and_drag())
    }
}
