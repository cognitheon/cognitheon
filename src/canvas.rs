use std::sync::atomic::{AtomicU64, Ordering};

use egui::emath::TSTransform;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct CanvasState {
    pub offset: egui::Vec2,
    pub scale: f32,
    // 从画布到屏幕的变换矩阵
    pub transform: TSTransform,
    pub global_node_id: AtomicU64,
    pub global_edge_id: AtomicU64,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            scale: 1.0,
            transform: TSTransform::IDENTITY,
            global_node_id: AtomicU64::new(0),
            global_edge_id: AtomicU64::new(0),
        }
    }
}

impl CanvasState {
    pub fn new_node_id(&self) -> u64 {
        self.global_node_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn new_edge_id(&self) -> u64 {
        self.global_edge_id.fetch_add(1, Ordering::Relaxed)
    }

    /// 将"画布坐标"转换到"屏幕坐标"
    pub fn to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        // 假设：先缩放，再平移
        // 你也可以根据需求进行其它顺序或加上中心点等修正
        // canvas_pos * self.scale + self.offset
        // canvas_pos * self.scale + self.offset

        self.transform.mul_pos(canvas_pos)
    }

    pub fn to_screen_rect(&self, canvas_rect: egui::Rect) -> egui::Rect {
        // let min = self.to_screen(canvas_rect.min);
        // let max = self.to_screen(canvas_rect.max);
        // egui::Rect::from_min_max(min, max);

        self.transform.mul_rect(canvas_rect)
    }

    pub fn to_canvas_rect(&self, screen_rect: egui::Rect) -> egui::Rect {
        self.transform.inverse().mul_rect(screen_rect)
    }

    /// 将"屏幕坐标"转换回"画布坐标"（如需在鼠标点击时计算画布内的点）
    pub fn to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        // (screen_pos - self.offset) / self.scale

        let inverse = self.transform.inverse();
        inverse.mul_pos(screen_pos)
    }

    pub fn to_screen_vec2(&self, canvas_pos: egui::Vec2) -> egui::Vec2 {
        // canvas_pos * self.scale + self.offset
        let canvas_pos = egui::Pos2::new(canvas_pos.x, canvas_pos.y);

        let screen_pos = self.transform.mul_pos(canvas_pos);
        egui::Vec2::new(screen_pos.x, screen_pos.y)
    }

    pub fn to_canvas_vec2(&self, screen_pos: egui::Vec2) -> egui::Vec2 {
        // canvas_pos * self.scale + self.offset
        let screen_pos = egui::Pos2::new(screen_pos.x, screen_pos.y);

        let canvas_pos = self.transform.inverse().mul_pos(screen_pos);
        egui::Vec2::new(canvas_pos.x, canvas_pos.y)
    }
}
