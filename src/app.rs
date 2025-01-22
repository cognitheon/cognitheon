/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    canvas_state: CanvasState,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct CanvasState {
    offset: egui::Vec2,
    scale: f32,
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
    fn to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        // 假设：先缩放，再平移
        // 你也可以根据需求进行其它顺序或加上中心点等修正
        canvas_pos * self.scale + self.offset
    }

    /// 将"屏幕坐标"转换回"画布坐标"（如需在鼠标点击时计算画布内的点）
    fn to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        (screen_pos - self.offset) / self.scale
    }
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            canvas_state: CanvasState::default(),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);

                if ui.button("test").clicked() {
                    println!("test");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });

            ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            let desired_size = ui.available_size();
            let (canvas_rect, canvas_response) =
                ui.allocate_exact_size(desired_size, egui::Sense::drag());

            // =====================
            // 1. 处理缩放 (鼠标滚轮)
            // =====================
            if canvas_response.hovered() {
                let zoom_delta = ui.input(|i| i.zoom_delta());
                if zoom_delta != 1.0 {
                    let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

                    // 计算鼠标指针相对于画布原点的偏移
                    let mouse_canvas_pos =
                        (mouse_pos - self.canvas_state.offset) / self.canvas_state.scale;

                    // 保存旧的缩放值
                    let old_scale = self.canvas_state.scale;

                    // 更新缩放值
                    self.canvas_state.scale *= zoom_delta;
                    self.canvas_state.scale = self.canvas_state.scale.clamp(0.01, 1000.0);

                    // 计算新的偏移量，保持鼠标位置不变
                    self.canvas_state.offset =
                        mouse_pos - (mouse_canvas_pos * self.canvas_state.scale);
                }
            }

            // =====================
            // 2. 处理平移 (拖拽)
            // =====================
            if canvas_response.dragged() {
                // drag_delta() 表示本次帧被拖拽的增量
                let drag_delta = canvas_response.drag_delta();
                self.canvas_state.offset += drag_delta;
            }

            if canvas_response.hovered() {
                let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
                if scroll_delta != egui::Vec2::ZERO {
                    self.canvas_state.offset += scroll_delta;
                }
            }

            // =====================
            // 3. 使用 painter 在画布上绘制
            // =====================
            // let painter = ui.painter_at(canvas_rect);
            draw_grid(ui, &self.canvas_state, canvas_rect);

            // // 下面仅作示例：绘制一个 0..100 区域内的网格和一条线
            // // 根据画布变换，将它映射到屏幕
            // let grid_color = egui::Color32::from_gray(100);
            // for x in (0..=100).step_by(10) {
            //     let p1 = self.canvas_state.to_screen(egui::Pos2::new(x as f32, 0.0));
            //     let p2 = self
            //         .canvas_state
            //         .to_screen(egui::Pos2::new(x as f32, 100.0));
            //     painter.line_segment([p1, p2], (1.0, grid_color));
            // }
            // for y in (0..=100).step_by(10) {
            //     let p1 = self.canvas_state.to_screen(egui::Pos2::new(0.0, y as f32));
            //     let p2 = self
            //         .canvas_state
            //         .to_screen(egui::Pos2::new(100.0, y as f32));
            //     painter.line_segment([p1, p2], (1.0, grid_color));
            // }

            // // 画一条斜线
            // let start = self.canvas_state.to_screen(egui::Pos2::new(0.0, 0.0));
            // let end = self.canvas_state.to_screen(egui::Pos2::new(100.0, 100.0));
            // painter.line_segment([start, end], (2.0, egui::Color32::RED));

            // // =====================
            // // 4. 在画布上放置一个按钮 (示意)
            // // =====================
            // // 假设我们想让按钮位于画布坐标 (50, 50) 左上角，尺寸 80x30
            // let canvas_button_pos = egui::Pos2::new(50.0, 50.0);
            // let button_size = egui::Vec2::new(80.0, 30.0);

            // // 先把它转换成屏幕坐标矩形
            // let screen_top_left = self.canvas_state.to_screen(canvas_button_pos);
            // let screen_rect = egui::Rect::from_min_size(screen_top_left, button_size);

            // // 在该矩形内部放置一个 UI
            // let button_resp = ui.allocate_ui_at_rect(screen_rect, |ui| {
            //     if ui.button("Canvas Button").clicked() {
            //         // 在这里处理按钮点击事件
            //         println!("Canvas Button clicked");
            //     }
            // });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}

fn draw_grid(ui: &mut egui::Ui, canvas_state: &CanvasState, canvas_rect: egui::Rect) {
    let painter = ui.painter_at(canvas_rect);

    // 基准网格间距（画布坐标系中的单位）
    let base_grid_size = 50.0;

    // 计算当前缩放下的网格像素大小
    let grid_pixels = base_grid_size * canvas_state.scale;

    // 计算网格级别
    let level_f = -(grid_pixels / 50.0).log2();
    // let level_f_offset = level_f + 0.5;
    let level = level_f.floor() as i32;
    println!("level_f: {:?}", level_f);
    println!("level: {:?}", level);
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
    // 将画布坐标系中的半径转换为屏幕坐标系中的半径
    let circle_radius = 100.0 * canvas_state.scale;
    painter.circle(
        circle_center,
        circle_radius,
        egui::Color32::RED,
        egui::Stroke::new(2.0, egui::Color32::GREEN),
    );
}
