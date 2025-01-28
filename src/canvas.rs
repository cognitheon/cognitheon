#[derive(serde::Deserialize, serde::Serialize)]
pub struct CanvasState {
    pub offset: egui::Vec2,
    pub scale: f32,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            scale: 1.0,
        }
    }
}

impl CanvasState {
    /// 将"画布坐标"转换到"屏幕坐标"
    pub fn to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        // 假设：先缩放，再平移
        // 你也可以根据需求进行其它顺序或加上中心点等修正
        canvas_pos * self.scale + self.offset
    }

    pub fn to_screen_rect(&self, canvas_rect: egui::Rect) -> egui::Rect {
        let min = self.to_screen(canvas_rect.min);
        let max = self.to_screen(canvas_rect.max);
        egui::Rect::from_min_max(min, max)
    }

    /// 将"屏幕坐标"转换回"画布坐标"（如需在鼠标点击时计算画布内的点）
    pub fn to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        (screen_pos - self.offset) / self.scale
    }

    pub fn to_screen_vec2(&self, canvas_pos: egui::Vec2) -> egui::Vec2 {
        canvas_pos * self.scale + self.offset
    }
}
