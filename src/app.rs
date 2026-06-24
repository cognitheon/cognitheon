use std::sync::Arc;

use egui::{Align, ComboBox, Id, Layout, RichText};
#[cfg(not(target_arch = "wasm32"))]
use rfd::AsyncFileDialog;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::{Builder, Runtime};

use crate::resource::{CanvasStateResource, GraphResource, ParticleSystemResource};
// use crate::globals::{CanvasStateResource, GraphResource};
use crate::gpu_render::particle::particle_system::ParticleSystem;
use crate::graph::edge::EdgeType;
use crate::input::state_manager::InputStateManager;
use crate::ui::canvas::data::CanvasWidget;
use crate::wikilink;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct CognitheonApp {
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
    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    runtime: Runtime,
}

// impl Debug for CognitheonApp {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:?}", self.graph_resource)?;
//         write!(f, "{:?}", self.canvas_resource)
//     }
// }

impl Default for CognitheonApp {
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
            #[cfg(not(target_arch = "wasm32"))]
            runtime: Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        }
    }
}

impl CognitheonApp {
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
            let mut app: CognitheonApp =
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
            app.canvas_widget =
                CanvasWidget::new(app.graph_resource.clone(), app.canvas_resource.clone());
            // println!("app: {:?}", app);
            app
        } else {
            Default::default()
        };
        // let mut app: CognitheonApp = Default::default();

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
    //             .get_persisted::<CognitheonApp>(eframe::APP_KEY.into())
    //             .unwrap();
    //         &app.graph
    //     })
    // }
}

impl CognitheonApp {
    /// 右侧链接面板：选中节点的出链 + 反向引用（含上下文原话），点击跳转聚焦。
    fn show_links_panel(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.heading("链接");
        ui.separator();

        let selected = self
            .graph_resource
            .read_resource(|g| g.get_selected_nodes().first().copied());
        let Some(idx) = selected else {
            ui.weak("选中一个节点，查看它的出链与反向引用。");
            return;
        };

        let (title, outlinks) = self.graph_resource.read_resource(|g| match g.get_node(idx) {
            Some(n) => (n.text.clone(), wikilink::parse_links(&n.note)),
            None => (String::new(), Vec::new()),
        });
        let title_disp = if title.is_empty() {
            "（无标题）"
        } else {
            title.as_str()
        };
        ui.label(RichText::new(title_disp).strong());

        // 出链
        ui.add_space(6.0);
        ui.label(RichText::new("出链").weak().small());
        if outlinks.is_empty() {
            ui.weak("（在正文里用 [[标题]] 建立）");
        }
        for lt in &outlinks {
            let target = self
                .graph_resource
                .read_resource(|g| wikilink::find_by_title(g, lt));
            let text = if target.is_some() {
                format!("→ {lt}")
            } else {
                format!("→ {lt}（未创建）")
            };
            if ui
                .add_enabled(target.is_some(), egui::Button::new(text).frame(false))
                .clicked()
            {
                if let Some(t) = target {
                    self.focus_node(ui.ctx(), t);
                }
            }
        }

        // 反向链接 + 上下文原话
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("反向链接").weak().small());
        let backlinks = self
            .graph_resource
            .read_resource(|g| wikilink::backlinks_with_context(g, idx));
        if backlinks.is_empty() {
            ui.weak("（还没有笔记用 [[…]] 提到它）");
        }
        for bl in &backlinks {
            if ui
                .add(
                    egui::Button::new(RichText::new(format!("↩ {}", bl.title)).strong())
                        .frame(false),
                )
                .clicked()
            {
                self.focus_node(ui.ctx(), bl.source);
            }
            for c in &bl.contexts {
                ui.label(RichText::new(c.as_str()).weak().small());
            }
            ui.add_space(4.0);
        }
    }

    /// 选中并把画布聚焦（居中）到某节点。
    fn focus_node(&self, ctx: &egui::Context, idx: petgraph::graph::NodeIndex) {
        let pos = self.graph_resource.with_resource(|g| {
            g.selected.clear();
            g.select_node(idx);
            g.get_node(idx).map(|n| n.position)
        });
        if let Some(pos) = pos {
            let center = ctx.content_rect().center();
            self.canvas_resource.with_resource(|cs| {
                let s = cs.transform.scaling;
                cs.transform.translation = center.to_vec2() - s * pos.to_vec2();
            });
        }
    }
}

impl eframe::App for CognitheonApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // println!("save");
        // println!("self: {:?}", self);
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let last_offset: f32 = ctx
            .data(|m| m.get_temp(Id::new("animation_offset")))
            .unwrap_or(0.0);

        let delta_time = ctx.input(|i| i.stable_dt).min(0.1); // 稳定的一帧时间
        let speed = 20.0; // 像素/秒

        // 每帧更新 offset
        let new_offset = last_offset - speed * delta_time;
        ctx.data_mut(|m| m.insert_temp(Id::new("animation_offset"), new_offset));
        // println!(
        //     "update: {:?}",
        //     self.graph_resource.0.read().unwrap().graph.node_count()
        // );

        // if let Some(particle_system_resource) = &self.particle_system {
        //     particle_system_resource.read_particle_system(|particle_system| {
        //         println!(
        //             "particle_system: {:?}",
        //             particle_system
        //                 .particles
        //                 .iter()
        //                 .filter(|p| p.life > 0.0)
        //                 .count()
        //         );
        //     });
        // }
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: 文件 Save/Load 走 tokio + rfd，仅 native；web 上不显示 File 菜单。
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.menu_button("File", |ui| {
                        if ui.button("New").clicked() {
                            println!("new");
                            self.graph_resource.with_resource(|graph| graph.reset());
                        }

                        if ui.button("Save").clicked() {
                            ui.close();
                            // 读出图 + 画布，序列化为带 schema 版本号的开放 JSON 文档
                            match self.graph_resource.read_resource(|graph| {
                                self.canvas_resource.read_resource(|canvas| {
                                    crate::persistence::save_string(graph, Some(canvas))
                                })
                            }) {
                                Ok(data) => {
                                    let future = async move {
                                        if let Some(file) = AsyncFileDialog::new()
                                            .add_filter("Cognitheon", &["cnt"])
                                            .set_directory("~")
                                            .save_file()
                                            .await
                                        {
                                            match file.write(data.as_bytes()).await {
                                                Ok(_) => log::info!("save success"),
                                                Err(e) => log::error!("save failed: {e}"),
                                            }
                                        }
                                    };
                                    self.runtime.block_on(future);
                                }
                                Err(e) => log::error!("serialize failed: {e}"),
                            }
                        }

                        if ui.button("Load").clicked() {
                            ui.close();
                            let future = async {
                                match AsyncFileDialog::new()
                                    .add_filter("Cognitheon", &["cnt"])
                                    .set_directory("~")
                                    .pick_file()
                                    .await
                                {
                                    Some(file) => Some(file.read().await),
                                    None => None,
                                }
                            };
                            if let Some(data) = self.runtime.block_on(future) {
                                // 兼容旧 .cnt：persistence::load 会回退解析无版本号的旧格式
                                match crate::persistence::load(&data) {
                                    Ok(doc) => {
                                        let (graph, canvas) = doc.into_parts();
                                        self.graph_resource = GraphResource::new(graph);
                                        self.canvas_resource = CanvasStateResource::new(canvas);
                                        self.canvas_widget = CanvasWidget::new(
                                            self.graph_resource.clone(),
                                            self.canvas_resource.clone(),
                                        );
                                    }
                                    Err(e) => log::error!("load failed: {e}"),
                                }
                            }
                        }

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
                    // egui::Window::new("test").show(ctx, |ui| {
                    //     ui.label("test");
                    // });
                }

                let mut edge_type = self
                    .graph_resource
                    .read_resource(|graph| graph.edge_type.clone());
                ComboBox::from_label("Edge Type")
                    .selected_text(format!("{:?}", edge_type))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut edge_type, EdgeType::Bezier, "Bezier")
                            .clicked()
                        {
                            self.graph_resource
                                .with_resource(|graph| graph.edge_type = EdgeType::Bezier);
                        }
                        if ui
                            .selectable_value(&mut edge_type, EdgeType::Line, "Line")
                            .clicked()
                        {
                            self.graph_resource
                                .with_resource(|graph| graph.edge_type = EdgeType::Line);
                        }
                    });
            });
        });

        egui::Panel::bottom("bottom_panel").show_inside(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                    current_zoom(ui, &self.canvas_resource);
                    current_offset(ui, &self.canvas_resource);
                    current_input_state(ui, &self.canvas_widget.input_manager);
                    current_fps(ui, &self.canvas_widget.input_manager);
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

        // 右侧链接面板：选中节点的出链 + 反向引用（含上下文原话），点击跳转聚焦
        egui::Panel::right("links_panel")
            .default_size(260.0)
            .show_inside(ui, |ui| {
                self.show_links_panel(ui);
            });

        egui::CentralPanel::default()
            // .frame(egui::Frame::default().outer_margin(egui::Margin::same(3.0)))
            .show_inside(ui, |ui| {
                ui.add(&mut self.canvas_widget);

                // egui::Window::new("test")
                //     .default_size(Vec2::new(800.0, 600.0))
                //     .show(ctx, |ui| {
                //         ui.label("test");
                //     });
            });

        // ctx.show_viewport_deferred(
        //     ViewportId::from_hash_of("test"),
        //     ViewportBuilder::default().with_title("testwindow"),
        //     |ctx, _viewport_class| {},
        // );
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
    canvas_state_resource.read_resource(|canvas_state| {
        ui.label(format!("zoom: {:.2}", canvas_state.transform.scaling));
    });
    // let zoom = ui.input(|i| i.zoom_delta());
    // ui.label(format!("zoom: {}", canvas_state.scale));
}

fn current_offset(ui: &mut egui::Ui, canvas_state_resource: &CanvasStateResource) {
    canvas_state_resource.read_resource(|canvas_state| {
        ui.label(format!("offset: {:?}", canvas_state.transform.translation));
    });
}

fn current_input_state(ui: &mut egui::Ui, input_state_manager: &InputStateManager) {
    let input_state = &input_state_manager.current_state;
    ui.label(format!("input_state: {:?}", input_state));
}

fn current_fps(ui: &mut egui::Ui, _input_state_manager: &InputStateManager) {
    let dt = ui.ctx().input(|i| i.stable_dt);
    ui.label(format!("fps: {:?}", 1.0 / dt));
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
