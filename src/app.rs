use crate::canvas::{draw_grid, CanvasState};
use crate::graph;
use std::sync::atomic::{AtomicU64, Ordering};

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    canvas_state: CanvasState,
    #[serde(skip)]
    editing_text: Option<(egui::Pos2, String)>,
    graph: petgraph::stable_graph::StableGraph<graph::Node, ()>,
    current_node: Option<u32>,
    global_node_id: AtomicU64,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            canvas_state: CanvasState::default(),
            editing_text: None,
            graph: petgraph::stable_graph::StableGraph::new(),
            current_node: None,
            global_node_id: AtomicU64::new(0),
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
        eframe::set_value(storage, eframe::APP_KEY, self);
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
            // ui.heading("eframe template");

            // ui.horizontal(|ui| {
            //     ui.label("Write something: ");
            //     ui.text_edit_singleline(&mut self.label);
            // });

            // ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            // if ui.button("Increment").clicked() {
            //     self.value += 1.0;
            // }

            // ui.separator();

            // ui.add(egui::github_link_file!(
            //     "https://github.com/emilk/eframe_template/blob/main/",
            //     "Source code."
            // ));

            let desired_size = ui.available_size();
            let (canvas_rect, canvas_response) =
                ui.allocate_exact_size(desired_size, egui::Sense::drag());

            if canvas_response.double_clicked() {
                println!("double clicked");
            }

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
                    // let old_scale = self.canvas_state.scale;

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

            // 处理双击
            if canvas_response.hovered() {
                if ui.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                }) {
                    if let Some(screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        // 将屏幕坐标转换为画布坐标
                        let canvas_pos = self.canvas_state.to_canvas(screen_pos);
                        self.editing_text = Some((canvas_pos, String::new()));
                    }
                    println!("double clicked");
                }
            }

            if let Some((canvas_pos, text)) = &mut self.editing_text {
                let screen_pos = self.canvas_state.to_screen(*canvas_pos);
                let text_edit_size = egui::Vec2::new(100.0, 20.0);
                let text_edit_rect = egui::Rect::from_min_size(screen_pos, text_edit_size);
                let builder = egui::UiBuilder::new();
                let text_edit_response =
                    ui.allocate_new_ui(builder.max_rect(text_edit_rect), |ui| {
                        let response = ui.text_edit_singleline(text);
                        response.request_focus();
                        response
                    });

                // 如果按下回车或点击其他地方，结束编辑
                if text_edit_response.inner.lost_focus()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    if !text.is_empty() {
                        println!("Text input finished: {}", text);
                        // 这里可以保存文本到某个集合中
                        self.graph.add_node(graph::Node {
                            id: self.global_node_id.fetch_add(1, Ordering::Relaxed),
                            position: *canvas_pos,
                            text: text.clone(),
                            note: String::new(),
                        });
                    }
                    self.editing_text = None;
                }
            }

            // =====================
            // 3. 使用 painter 在画布上绘制
            // =====================
            // let painter = ui.painter_at(canvas_rect);
            draw_grid(ui, &self.canvas_state, canvas_rect);

            graph::render_graph(&self.graph, ui, ctx, &self.canvas_state);

            // ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            //     powered_by_egui_and_eframe(ui);
            //     egui::warn_if_debug_build(ui);
            //     current_zoom(self, ui);
            // });
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

fn current_zoom(app: &TemplateApp, ui: &mut egui::Ui) {
    // 获取当前缩放
    // let zoom = ui.input(|i| i.zoom_delta());
    ui.label(format!("zoom: {}", app.canvas_state.scale));
}
