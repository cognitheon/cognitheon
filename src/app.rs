use std::sync::Arc;

use egui::{Align, ComboBox, Layout, RichText};

use crate::globals::{
    canvas_state_resource::CanvasStateResource, graph_resource::GraphResource,
    particle_system_resource::ParticleSystemResource,
};
// use crate::globals::{CanvasStateResource, GraphResource};
use crate::graph::edge::EdgeType;
use crate::particle::particle_system::ParticleSystem;
use crate::ui::canvas::CanvasWidget;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    // edge_type: EdgeType,
    canvas_resource: CanvasStateResource,
    graph_resource: GraphResource,
    #[serde(skip)]
    canvas_widget: CanvasWidget,
    #[serde(skip)]
    particle_system: Option<ParticleSystemResource>,
}

// impl Debug for TemplateApp {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:?}", self.graph_resource)?;
//         write!(f, "{:?}", self.canvas_resource)
//     }
// }

impl Default for TemplateApp {
    fn default() -> Self {
        let graph_resource = GraphResource::default();
        let canvas_resource = CanvasStateResource::default();
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            // edge_type: EdgeType::Line,
            canvas_resource: canvas_resource.clone(),
            graph_resource: graph_resource.clone(),
            canvas_widget: CanvasWidget::new(graph_resource.clone(), canvas_resource.clone()),
            particle_system: None,
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
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }
        setup_font(&cc.egui_ctx);

        let mut app = if let Some(storage) = cc.storage {
            println!("load");
            let mut app: TemplateApp =
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
            app.canvas_widget =
                CanvasWidget::new(app.graph_resource.clone(), app.canvas_resource.clone());
            println!("app: {:?}", app);
            app
        } else {
            Default::default()
        };

        let wgpu_render_state = cc.wgpu_render_state.as_ref();
        if let Some(rs) = wgpu_render_state {
            let device = &rs.device;

            // 构造我们的粒子系统
            let particle_system = ParticleSystem::new(
                device,
                rs.target_format,
                2000, // 最大粒子数
                10,   // 每帧生成多少粒子
                2.0,  // 粒子最大生命（秒）
                10.0, // 粒子最大速度
            );

            let particle_system_resource = ParticleSystemResource::new(particle_system);
            // println!("particle_system: {:?}", particle_system);

            // 注册到资源里，这样在回调里可以获取到
            rs.renderer
                .write()
                .callback_resources
                .insert::<ParticleSystemResource>(particle_system_resource.clone());

            app.particle_system = Some(particle_system_resource.clone());
        }

        app
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
        println!("save");
        // println!("self: {:?}", self);
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // println!(
        //     "update: {:?}",
        //     self.graph_resource.0.read().unwrap().graph.node_count()
        // );

        if let Some(particle_system_resource) = &self.particle_system {
            particle_system_resource.read_particle_system(|particle_system| {
                println!(
                    "particle_system: {:?}",
                    particle_system
                        .particles
                        .iter()
                        .filter(|p| p.life > 0.0)
                        .count()
                );
            });
        }
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

                let mut edge_type = self
                    .graph_resource
                    .read_graph(|graph| graph.edge_type.clone());
                ComboBox::from_label("Edge Type")
                    .selected_text(format!("{:?}", edge_type))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut edge_type, EdgeType::Bezier, "Bezier")
                            .clicked()
                        {
                            self.graph_resource
                                .with_graph(|graph| graph.edge_type = EdgeType::Bezier);
                        }
                        if ui
                            .selectable_value(&mut edge_type, EdgeType::Line, "Line")
                            .clicked()
                        {
                            self.graph_resource
                                .with_graph(|graph| graph.edge_type = EdgeType::Line);
                        }
                    });
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                    current_zoom(ui, &self.canvas_resource);
                    current_offset(ui, &self.canvas_resource);
                });
                ui.end_row();
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::LEFT)
                        .with_main_justify(true)
                        .with_cross_justify(true),
                    |ui| {
                        // 两端对齐
                        powered_by_egui_and_eframe(ui);
                        // ui.add_space();
                        // ui.label("test");
                        ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                            let label = ui.label(
                                RichText::new("⚠ Debug build ⚠")
                                    .small()
                                    .color(ui.visuals().warn_fg_color),
                            );
                            label.on_hover_text("egui was compiled with debug assertions enabled.");
                        });
                    },
                );
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().outer_margin(egui::Margin::same(3.0)))
            .show(ctx, |ui| {
                ui.add(&mut self.canvas_widget);
            });

        // if let Some(rs) = ctx..as_ref() {
        //     rs.renderer.write().callback_resources.clear();
        // }
        ctx.request_repaint();
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

fn current_zoom(ui: &mut egui::Ui, canvas_state_resource: &CanvasStateResource) {
    // 获取当前缩放
    canvas_state_resource.read_canvas_state(|canvas_state| {
        ui.label(format!("zoom: {:.2}", canvas_state.transform.scaling));
    });
    // let zoom = ui.input(|i| i.zoom_delta());
    // ui.label(format!("zoom: {}", canvas_state.scale));
}

fn current_offset(ui: &mut egui::Ui, canvas_state_resource: &CanvasStateResource) {
    canvas_state_resource.read_canvas_state(|canvas_state| {
        ui.label(format!("offset: {:?}", canvas_state.transform.translation));
    });
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
