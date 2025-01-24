use egui::{Sense, Ui, Widget};

use crate::canvas::CanvasState;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
}

pub struct NodeWidget<'a> {
    pub selected: bool,
    pub data: Node,
    pub canvas_state: &'a CanvasState,
}

fn node_widget(ui: &mut egui::Ui, data: &Node, canvas_state: &CanvasState) -> egui::Response {
    // let canvas_state = self.canvas_state;

    let text = format!("{}", data.text);
    let galley =
        ui.painter()
            .layout_no_wrap(text.clone(), egui::FontId::default(), egui::Color32::RED);
    let text_size = galley.size();

    let desired_size = text_size + egui::vec2(20.0, 10.0);

    let screen_pos = canvas_state.to_screen(data.position);

    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

    if ui.is_rect_visible(rect) {
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        // println!("screen_pos: {:?}", screen_pos);
        // 根据文本大小创建矩形区域
        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        // 绘制边框
        ui.painter().rect(
            rect,
            egui::Rounding::same(5.0),
            egui::Color32::from_rgba_premultiplied(50, 50, 50, 200),
            egui::Stroke::new(3.0, egui::Color32::GREEN), // 将线宽从20.0改为1.0
        );

        // 根据rect计算文本位置，使得文本居中
        let text_pos = rect.center();
        // 绘制文本
        ui.painter().text(
            text_pos,
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::default(),
            egui::Color32::RED,
        );
    }
    // ui.allocate_ui(desired_size, |ui| {

    response
    // })
    // .inner
}

impl<'a> Widget for NodeWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let canvas_state = self.canvas_state;
        let text = format!("{}", self.data.text);
        let font_size = 20.0 * canvas_state.scale; // 你可以调整这个数值
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley =
            ui.painter()
                .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();

        let desired_size = text_size + egui::vec2(20.0, 10.0) * canvas_state.scale;

        let screen_pos = canvas_state.to_screen(self.data.position);

        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        // if ui.is_rect_visible(rect) {
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        // println!("screen_pos: {:?}", screen_pos);
        // 根据文本大小创建矩形区域
        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        // 绘制边框
        ui.painter().rect(
            rect,
            egui::Rounding::same(5.0 * canvas_state.scale),
            egui::Color32::from_rgba_premultiplied(50, 50, 50, 200),
            egui::Stroke::new(3.0 * canvas_state.scale, egui::Color32::GREEN), // 将线宽从20.0改为1.0
        );

        // 根据rect计算文本位置，使得文本居中
        let text_pos = rect.center();
        // 绘制文本
        ui.painter().text(
            text_pos,
            egui::Align2::CENTER_CENTER,
            text,
            font.clone(),
            egui::Color32::RED,
        );
        // }
        // ui.allocate_ui(desired_size, |ui| {

        response
    }
}

pub fn render_graph(
    graph: &petgraph::stable_graph::StableGraph<Node, ()>,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    canvas_state: &CanvasState,
) {
    for idx in graph.node_indices() {
        let node = graph.node_weight(idx).unwrap();
        println!("node: {}", node.id);
        // Put the node id into the ui
        let pos = node.position;
        let screen_pos = canvas_state.to_screen(pos);

        // 在屏幕上指定位置放置label控件

        // ui.allocate_ui(egui::Vec2::new(100.0, 100.0), |ui| {
        //     ui.add(NodeWidget {
        //         selected: false,
        //         data: node.clone(),
        //         canvas_state,
        //     });
        // });
        ui.add(NodeWidget {
            selected: false,
            data: node.clone(),
            canvas_state,
        });

        // node_widget(ui, &node, canvas_state);
    }
}
