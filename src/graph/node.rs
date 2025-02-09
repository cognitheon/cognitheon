#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
    // pub render_info: Option<NodeRenderInfo>,
}
