use egui::*;

use crate::global::CanvasStateResource;

use super::bezier::Anchor;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LineEdge {
    pub source: Anchor,
    pub target: Anchor,
}

impl LineEdge {
    pub fn new(source: Anchor, target: Anchor) -> Self {
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
}

impl Widget for LineWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let painter = ui.painter();
        let source_canvas_pos = self.line_edge.source.canvas_pos;
        let target_canvas_pos = self.line_edge.target.canvas_pos;

        let source_screen_pos = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen(source_canvas_pos));
        let target_screen_pos = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen(target_canvas_pos));
        let stroke = Stroke::new(2.0, Color32::GRAY);
        painter.line_segment([source_screen_pos, target_screen_pos], stroke);

        let screen_rect = Rect::from_points(&[source_screen_pos, target_screen_pos]);
        ui.allocate_rect(screen_rect, Sense::click_and_drag())
    }
}
