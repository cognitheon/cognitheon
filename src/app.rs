use std::sync::{Arc, RwLock};

use egui::Id;

use crate::canvas::CanvasState;
use crate::global::{CanvasStateResource, GraphResource};
use crate::graph::Graph;
use crate::ui::canvas::CanvasWidget;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    canvas_resource: CanvasStateResource,
    graph_resource: GraphResource,
    #[serde(skip)]
    canvas_widget: CanvasWidget,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let graph_resource = GraphResource::default();
        let canvas_resource = CanvasStateResource::default();
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            canvas_resource: canvas_resource.clone(),
            graph_resource: graph_resource.clone(),
            canvas_widget: CanvasWidget::new(graph_resource.clone(), canvas_resource.clone()),
        }
    }
}

fn setup_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "source_hans_sans".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/SourceHanSansSC-Regular.otf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "source_hans_sans".to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "source_hans_sans".to_owned());

    // 在插入字体后添加调试输出
    println!(
        "Font data size: {:?} bytes",
        fonts.font_data["source_hans_sans"].font.len()
    );

    // Tell egui to use these fonts:
    ctx.set_fonts(fonts);
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }
        setup_font(&cc.egui_ctx);
        let graph = Graph::default();
        let graph_resource = GraphResource(Arc::new(RwLock::new(graph)));
        let canvas_state_resource =
            CanvasStateResource(Arc::new(RwLock::new(CanvasState::default())));

        cc.egui_ctx.data_mut(|data| {
            data.insert_persisted(Id::NULL, graph_resource);
            data.insert_temp(Id::NULL, canvas_state_resource);
        });

        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }

    // pub fn get_graph(ctx: &egui::Context) -> &Graph {
    //     ctx.data(|data| {
    //         let app = data
    //             .get_persisted::<TemplateApp>(eframe::APP_KEY.into())
    //             .unwrap();
    //         &app.graph
    //     })
    // }
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
                // 获取全局主题
                // let theme = ui.ctx().theme();
                // println!("theme: {:?}", theme);

                if ui.button("test").clicked() {
                    println!("test");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(&mut self.canvas_widget);
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                    current_zoom(ui);
                    current_offset(ui);
                });
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

fn current_zoom(ui: &mut egui::Ui) {
    // 获取当前缩放
    let canvas_state_resource: CanvasStateResource =
        ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
    canvas_state_resource.read_canvas_state(|canvas_state| {
        ui.label(format!("zoom: {:.2}", canvas_state.transform.scaling));
    });
    // let zoom = ui.input(|i| i.zoom_delta());
    // ui.label(format!("zoom: {}", canvas_state.scale));
}

fn current_offset(ui: &mut egui::Ui) {
    let canvas_state_resource: CanvasStateResource =
        ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
    canvas_state_resource.read_canvas_state(|canvas_state| {
        ui.label(format!("offset: {:?}", canvas_state.transform.translation));
    });
}
