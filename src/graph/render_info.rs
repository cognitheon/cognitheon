use crate::resource::CanvasStateResource;

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug)]
pub struct NodeRenderInfo {
    pub canvas_rect: egui::Rect,
}

impl NodeRenderInfo {
    pub fn canvas_center(&self) -> egui::Pos2 {
        self.canvas_rect.center()
    }

    pub fn screen_rect(&self, canvas_state: &CanvasStateResource) -> egui::Rect {
        canvas_state.read_resource(|canvas_state| canvas_state.to_screen_rect(self.canvas_rect))
    }
}

pub struct EdgeRenderInfo {
    pub canvas_rect: egui::Rect,
}
