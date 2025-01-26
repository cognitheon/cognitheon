use crate::canvas::CanvasState;
use crate::graph::Graph;
use egui::{Sense, Widget};
use petgraph::graph::NodeIndex;

use crate::colors::{node_background, node_border, node_border_selected};

pub struct NodeWidget<'a> {
    pub node_index: NodeIndex,
    pub graph: &'a mut Graph,
    pub canvas_state: &'a mut CanvasState,
}

impl<'a> NodeWidget<'a> {
    pub fn setup_actions(&mut self, response: &egui::Response, ui: &mut egui::Ui) {
        // 检测单击事件
        if response.clicked() {
            if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
                println!("node clicked: {:?}", self.node_index);
                self.graph.set_selected_node(Some(self.node_index));
            }
        }

        // 处理键盘按键
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.graph.set_selected_node(None);
            self.graph.set_editing_node(None);
        }

        if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            if self.graph.get_selected_node() == Some(self.node_index)
                && !self.graph.get_editing_node().is_some()
            {
                println!("node deleted: {:?}", self.node_index);
                self.graph.remove_node(self.node_index);
            }
        }

        // Ctrl + Enter
        if ui.input(|i| {
            i.key_pressed(egui::Key::Enter) && i.modifiers.contains(egui::Modifiers::CTRL)
        }) && self.graph.get_editing_node() == Some(self.node_index)
        {
            println!("node enter: {:?}", self.node_index);
            self.graph.set_editing_node(None);
        }

        if response.dragged() && self.graph.get_editing_node() != Some(self.node_index) {
            self.graph.set_editing_node(None);
            println!("node dragged: {:?}", self.node_index);
            let drag_delta = response.drag_delta() / (self.canvas_state.scale);
            let node = self.graph.get_node_mut(self.node_index).unwrap();
            node.position += drag_delta;
        }

        if response.double_clicked() {
            println!("node double clicked: {:?}", self.node_index);
            self.graph.set_editing_node(Some(self.node_index));
        }
    }
}

impl<'a> Widget for NodeWidget<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        // 对缩放比例进行区间化
        let scale_level = (self.canvas_state.scale * 10.0).ceil() / 10.0;
        // let node = self.graph.get_node_mut(self.node_id).unwrap();

        let text = {
            let node = self.graph.get_node(self.node_index).unwrap();
            format!("{}", node.text)
        };
        let font_size = 20.0 * scale_level as f32; // 你可以调整这个数值
                                                   // let font_size = 20.0;
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();
        // let text_size = egui::Vec2::new(100.0, 100.0);

        let min_width = 60.0 * self.canvas_state.scale;
        // let min_height = 40.0 * self.canvas_state.scale;

        let desired_size = egui::vec2(
            (text_size.x + 20.0 * self.canvas_state.scale).max(min_width),
            (text_size.y + 10.0 * self.canvas_state.scale),
        );

        let screen_pos = {
            let node = self.graph.get_node(self.node_index).unwrap();
            self.canvas_state.to_screen(node.position)
        };

        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        self.setup_actions(&response, ui);

        let selected_rect = rect.expand(5.0 * self.canvas_state.scale);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            // let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
            // println!("screen_pos: {:?}", screen_pos);
            // 根据文本大小创建矩形区域

            // let stroke_width = 3.0 * canvas_state.scale;
            let stroke_width = 1.0;

            if self.graph.get_selected_node() == Some(self.node_index) {
                painter.rect(
                    selected_rect,
                    egui::Rounding::same(5.0),
                    egui::Color32::TRANSPARENT,
                    egui::Stroke::new(2.0, node_border_selected(ui.ctx().theme())), // 将线宽从20.0改为1.0
                );
            }
            // 绘制边框
            painter.rect(
                rect,
                egui::Rounding::same(5.0),
                node_background(ui.ctx().theme()),
                egui::Stroke::new(stroke_width, node_border(ui.ctx().theme())), // 将线宽从20.0改为1.0
            );

            // 根据rect计算文本位置，使得文本居中
            let text_pos = rect.center();

            if self.graph.get_editing_node() == Some(self.node_index) {
                let mut text = {
                    let node = self.graph.get_node(self.node_index).unwrap();
                    format!("{}", node.text)
                };
                // let mut response = ui.text_edit_singleline(&mut text);
                let edit_response = ui.put(
                    rect,
                    egui::TextEdit::multiline(&mut text)
                        // .min_size(egui::vec2(min_width, min_height))
                        .desired_rows(1)
                        .font(font)
                        .text_color(egui::Color32::RED)
                        .background_color(node_background(ui.ctx().theme()))
                        .margin(egui::vec2(10.0, 5.0) * self.canvas_state.scale)
                        .horizontal_align(egui::Align::Center),
                );

                if edit_response.lost_focus() {
                    self.graph.set_editing_node(None);
                }

                edit_response.request_focus();
                let node = self.graph.get_node_mut(self.node_index).unwrap();
                node.text = text;
            } else {
                // 绘制文本
                painter.text(
                    text_pos,
                    egui::Align2::CENTER_CENTER,
                    text,
                    font.clone(),
                    egui::Color32::RED,
                );
            }
        };

        response

        // let response = ui.label("text");
        // response
    }
}
