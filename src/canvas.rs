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
}

pub fn draw_grid(ui: &mut egui::Ui, canvas_state: &CanvasState, canvas_rect: egui::Rect) {
    println!("draw_grid");
    let painter = ui.painter_at(canvas_rect);

    // 基准网格间距（画布坐标系中的单位）
    let base_grid_size = 50.0;

    // 计算当前缩放下的网格像素大小
    let grid_pixels = base_grid_size * canvas_state.scale;

    // 计算网格级别
    let level_f = -(grid_pixels / 50.0).log2();
    // let level_f_offset = level_f + 0.5;
    let level = level_f.floor() as i32;
    // println!("level_f: {:?}", level_f);
    // println!("level: {:?}", level);
    // let level = level_f.floor() as i32;

    // 计算两个相邻级别的网格大小
    let grid_size_1 = base_grid_size * 2.0_f32.powi(level);
    let grid_size_2 = base_grid_size * 2.0_f32.powi(level + 1);

    // 计算两个级别的透明度
    let t = level_f.fract().abs();
    let alpha_1 = ((1.0 - t) * 255.0) as u8;
    let alpha_2 = (t * 255.0) as u8;

    // 定义网格颜色
    let grid_color_1 = egui::Color32::from_rgba_unmultiplied(100, 100, 100, alpha_1);
    let grid_color_2 = egui::Color32::from_rgba_unmultiplied(100, 100, 100, alpha_2);

    // 计算可见区域的边界（画布坐标）
    let min_canvas = canvas_state.to_canvas(canvas_rect.min);
    let max_canvas = canvas_state.to_canvas(canvas_rect.max);

    // 绘制第一级网格
    let x_start_1 = (min_canvas.x / grid_size_1).floor() as i32;
    let x_end_1 = (max_canvas.x / grid_size_1).ceil() as i32;
    let y_start_1 = (min_canvas.y / grid_size_1).floor() as i32;
    let y_end_1 = (max_canvas.y / grid_size_1).ceil() as i32;

    for x in x_start_1..=x_end_1 {
        let x_pos = x as f32 * grid_size_1;
        let p1 = canvas_state.to_screen(egui::Pos2::new(x_pos, min_canvas.y));
        let p2 = canvas_state.to_screen(egui::Pos2::new(x_pos, max_canvas.y));
        painter.line_segment([p1, p2], (1.0, grid_color_1));
    }
    for y in y_start_1..=y_end_1 {
        let y_pos = y as f32 * grid_size_1;
        let p1 = canvas_state.to_screen(egui::Pos2::new(min_canvas.x, y_pos));
        let p2 = canvas_state.to_screen(egui::Pos2::new(max_canvas.x, y_pos));
        painter.line_segment([p1, p2], (1.0, grid_color_1));
    }

    // 绘制第二级网格
    let x_start_2 = (min_canvas.x / grid_size_2).floor() as i32;
    let x_end_2 = (max_canvas.x / grid_size_2).ceil() as i32;
    let y_start_2 = (min_canvas.y / grid_size_2).floor() as i32;
    let y_end_2 = (max_canvas.y / grid_size_2).ceil() as i32;

    for x in x_start_2..=x_end_2 {
        let x_pos = x as f32 * grid_size_2;
        let p1 = canvas_state.to_screen(egui::Pos2::new(x_pos, min_canvas.y));
        let p2 = canvas_state.to_screen(egui::Pos2::new(x_pos, max_canvas.y));
        painter.line_segment([p1, p2], (1.0, grid_color_2));
    }
    for y in y_start_2..=y_end_2 {
        let y_pos = y as f32 * grid_size_2;
        let p1 = canvas_state.to_screen(egui::Pos2::new(min_canvas.x, y_pos));
        let p2 = canvas_state.to_screen(egui::Pos2::new(max_canvas.x, y_pos));
        painter.line_segment([p1, p2], (1.0, grid_color_2));
    }

    // 画坐标轴
    let axis_color = egui::Color32::RED;
    let origin = canvas_state.to_screen(egui::Pos2::ZERO);
    let x_axis_end = canvas_state.to_screen(egui::Pos2::new(1000.0, 0.0));
    let y_axis_end = canvas_state.to_screen(egui::Pos2::new(0.0, 1000.0));
    painter.line_segment([origin, x_axis_end], (2.0, axis_color));
    painter.line_segment([origin, y_axis_end], (2.0, axis_color));

    // 画一条线
    let line_start = canvas_state.to_screen(egui::Pos2::new(0.0, 0.0));
    let line_end = canvas_state.to_screen(egui::Pos2::new(1000.0, 1000.0));
    painter.line_segment([line_start, line_end], (2.0, egui::Color32::GREEN));

    // 画一个圆
    let circle_center = canvas_state.to_screen(egui::Pos2::new(500.0, 500.0));

    // 画一个矩形
    let rect = egui::Rect::from_min_max(egui::Pos2::new(-500.0, -500.0), egui::Pos2::new(50.0, 50.0));
    let rect = canvas_state.to_screen_rect(rect);
    painter.rect(
        rect,
        egui::Rounding::same(5.0),
        egui::Color32::BLUE,
        egui::Stroke::new(2.0, egui::Color32::GREEN),
    );

    // 将画布坐标系中的半径转换为屏幕坐标系中的半径
    let circle_radius = 100.0 * canvas_state.scale;
    painter.circle(
        circle_center,
        circle_radius,
        egui::Color32::RED,
        egui::Stroke::new(2.0, egui::Color32::GREEN),
    );
}
