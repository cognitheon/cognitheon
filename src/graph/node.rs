use crate::globals::canvas_state_resource::CanvasStateResource;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
    pub render_info: Option<NodeRenderInfo>,
}

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug)]
pub struct NodeRenderInfo {
    pub canvas_rect: egui::Rect,
}

impl NodeRenderInfo {
    pub fn canvas_center(&self) -> egui::Pos2 {
        self.canvas_rect.center()
    }

    pub fn screen_rect(&self, canvas_state: &CanvasStateResource) -> egui::Rect {
        canvas_state.read_canvas_state(|canvas_state| canvas_state.to_screen_rect(self.canvas_rect))
    }
}
