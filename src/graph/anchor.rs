use egui::Vec2;

pub enum Anchor {
    Line(LineAnchor),
    Bezier(BezierAnchor),
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LineAnchor {
    pub canvas_pos: egui::Pos2,
}

impl LineAnchor {
    pub fn new(canvas_pos: egui::Pos2) -> Self {
        Self { canvas_pos }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]

pub struct BezierAnchor {
    pub canvas_pos: egui::Pos2,
    pub handle_in_canvas_pos: egui::Pos2,
    pub handle_out_canvas_pos: egui::Pos2,
    pub is_smooth: bool,
}

impl BezierAnchor {
    pub fn new_smooth(canvas_pos: egui::Pos2) -> Self {
        let handle_offset = Vec2::new(30.0, 0.0); // 默认水平对称
        Self {
            canvas_pos,
            handle_in_canvas_pos: canvas_pos - handle_offset,
            handle_out_canvas_pos: canvas_pos + handle_offset,
            is_smooth: true,
        }
    }

    // 创建尖锐锚点（控制柄各自独立）
    pub fn new_sharp(canvas_pos: egui::Pos2) -> Self {
        let handle_offset_in = Vec2::new(-30.0, 0.0); // 默认水平
        let handle_offset_out = Vec2::new(30.0, 0.0); // 默认水平
        Self {
            canvas_pos,
            handle_in_canvas_pos: canvas_pos + handle_offset_in,
            handle_out_canvas_pos: canvas_pos + handle_offset_out,
            is_smooth: false,
        }
    }

    pub fn with_handles(mut self, handle_in: egui::Pos2, handle_out: egui::Pos2) -> Self {
        self.handle_in_canvas_pos = handle_in;
        self.handle_out_canvas_pos = handle_out;
        self
    }

    // 强制设为平滑锚点，并更新控制柄为对称状态
    pub fn set_smooth(&mut self) {
        self.is_smooth = true;
        self.enforce_smooth();
    }

    // 强制设为尖锐锚点
    pub fn set_sharp(&mut self) {
        self.is_smooth = false;
    }

    // 强制更新控制柄为对称状态，保持平滑
    pub fn enforce_smooth(&mut self) {
        let in_vec = self.canvas_pos - self.handle_in_canvas_pos;
        self.handle_out_canvas_pos = self.canvas_pos + in_vec;
    }
}
