#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
    pub render_info: Option<NodeRenderInfo>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct NodeRenderInfo {
    pub canvas_rect: egui::Rect,
}

impl NodeRenderInfo {
    pub fn canvas_center(&self) -> egui::Pos2 {
        self.canvas_rect.center()
    }
}
