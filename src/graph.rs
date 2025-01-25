use egui::{Sense, Ui, Widget};

use crate::{canvas::CanvasState, colors::node_background};

const COLOR_NODE_BORDER: egui::Color32 = egui::Color32::from_rgba_premultiplied(52, 129, 201, 80);
const COLOR_NODE_BORDER_SELECTED: egui::Color32 =
    egui::Color32::from_rgba_premultiplied(222, 78, 78, 80);

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Graph {
    pub graph: petgraph::stable_graph::StableGraph<Node, ()>,
    pub selected_node: Option<u64>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            graph: petgraph::stable_graph::StableGraph::new(),
            selected_node: None,
        }
    }
}

impl Graph {
    pub fn add_node(&mut self, node: Node) {
        self.graph.add_node(node);
    }

    pub fn get_node(&self, id: u64) -> Option<&Node> {
        self.graph
            .node_indices()
            .find(|idx| self.graph.node_weight(*idx).unwrap().id == id)
            .map(|idx| self.graph.node_weight(idx).unwrap())
    }

    pub fn get_node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.graph
            .node_indices()
            .find(|idx| self.graph.node_weight(*idx).unwrap().id == id)
            .map(|idx| self.graph.node_weight_mut(idx).unwrap())
    }

    pub fn set_node(&mut self, node: Node) {
        self.graph.add_node(node);
    }

    pub fn set_selected_node(&mut self, node_id: Option<u64>) {
        self.selected_node = node_id;
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Edge {
    pub id: u64,
    pub source: u64,
    pub target: u64,
}

pub struct NodeWidget<'a> {
    pub node_id: u64,
    pub graph: &'a mut Graph,
    pub canvas_state: &'a mut CanvasState,
}

impl<'a> Widget for NodeWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // 对缩放比例进行区间化
        let scale_level = (self.canvas_state.scale * 10.0).ceil() / 10.0;
        let node = self.graph.get_node(self.node_id).unwrap();

        let text = format!("{}", node.text);
        let font_size = 20.0 * scale_level as f32; // 你可以调整这个数值
                                                   // let font_size = 20.0;
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();
        // let text_size = egui::Vec2::new(100.0, 100.0);

        let desired_size = text_size + egui::vec2(20.0, 10.0) * self.canvas_state.scale;

        let screen_pos = self.canvas_state.to_screen(node.position);

        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        // let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        // 检测单击事件
        if response.clicked() {
            println!("node clicked");
            self.graph.set_selected_node(Some(self.node_id));
        }

        if response.dragged() {
            println!("node dragged");
            let drag_delta = response.drag_delta();
            let node = self.graph.get_node_mut(self.node_id).unwrap();
            node.position += drag_delta;
        }
        // let rect = egui::Rect::from_min_size(screen_pos, desired_size);
        let selected_rect = rect.expand(5.0 * self.canvas_state.scale);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            // let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
            // println!("screen_pos: {:?}", screen_pos);
            // 根据文本大小创建矩形区域

            // let stroke_width = 3.0 * canvas_state.scale;
            let stroke_width = 1.0;

            if self.graph.selected_node == Some(self.node_id) {
                painter.rect(
                    selected_rect,
                    egui::Rounding::same(5.0),
                    egui::Color32::TRANSPARENT,
                    egui::Stroke::new(2.0, COLOR_NODE_BORDER_SELECTED), // 将线宽从20.0改为1.0
                );
            }
            // 绘制边框
            painter.rect(
                rect,
                egui::Rounding::same(5.0),
                node_background(ui.ctx().theme()),
                egui::Stroke::new(stroke_width, COLOR_NODE_BORDER), // 将线宽从20.0改为1.0
            );

            // 根据rect计算文本位置，使得文本居中
            let text_pos = rect.center();
            // 绘制文本
            painter.text(
                text_pos,
                egui::Align2::CENTER_CENTER,
                text,
                font.clone(),
                egui::Color32::RED,
            );
        };

        response

        // let response = ui.label("text");
        // response
    }
}

pub fn render_graph(
    graph: &mut Graph,
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    canvas_state: &mut CanvasState,
) {
    let node_ids = graph
        .graph
        .node_indices()
        .map(|idx| graph.graph.node_weight(idx).unwrap().id)
        .collect::<Vec<u64>>();

    println!("node_ids: {:?}", node_ids.len());

    for node_id in node_ids {
        // println!("node: {}", node.id);
        // Put the node id into the ui

        // 在屏幕上指定位置放置label控件

        ui.add(NodeWidget {
            node_id,
            graph,
            canvas_state,
        });
    }
}
