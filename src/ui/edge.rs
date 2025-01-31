use egui::*;
use petgraph::graph::{EdgeIndex, NodeIndex};

use crate::{
    global::{CanvasStateResource, GraphResource},
    graph::{edge::EdgeType, node::NodeRenderInfo},
};

use super::{
    bezier::{Anchor, BezierEdge, BezierWidget},
    line_edge::{LineEdge, LineWidget},
};

pub struct EdgeWidget {
    pub edge_index: EdgeIndex,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
}

impl EdgeWidget {
    // 由节点中心点和目标点计算出在节点边框上的点
    fn get_source_pos(&self, ui: &egui::Ui, node_center: Pos2, canvas_target: Pos2) -> Pos2 {
        let (src, dst) = self
            .graph_resource
            .read_graph(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        let src_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(src.index().to_string())))
            .unwrap();
        let src_node_rect = src_node_render_info.canvas_rect;
        let src_node_canvas_center = src_node_rect.center();

        // 计算出在节点边框上的点

        // 1. 计算斜率
        let slope = canvas_target - src_node_canvas_center;
        // 2. 计算在节点边框上的点
        let source_pos = src_node_canvas_center + slope * src_node_rect.width() / 2.0;
        source_pos
    }

    // 根据实时的节点位置更新贝塞尔曲线锚点信息
    fn update_bezier_edge(&self, ui: &egui::Ui) {
        // 获取首尾节点
        let (src, dst) = self
            .graph_resource
            .read_graph(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        // 获取首尾节点中心点
        let src_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(src.index().to_string())))
            .unwrap();
        let src_node_canvas_center = src_node_render_info.canvas_center();

        let dst_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(dst.index().to_string())))
            .unwrap();
        let dst_node_canvas_center = dst_node_render_info.canvas_center();

        // 获取已有贝塞尔曲线控制点锚点
        let bezier_edge = self
            .graph_resource
            .read_graph(|graph| graph.get_edge(self.edge_index).unwrap().bezier_edge.clone());
        let control_anchors = bezier_edge.control_anchors;

        let mut new_bezier_edge = BezierEdge::new(
            Anchor::new_smooth(src_node_canvas_center),
            Anchor::new_smooth(dst_node_canvas_center),
        );
        new_bezier_edge.update_control_anchors(control_anchors);

        self.graph_resource.with_graph(|graph| {
            graph.update_bezier_edge(self.edge_index, new_bezier_edge);
        });
    }

    fn update_line_edge(&self, ui: &egui::Ui) {
        // 获取首尾节点
        let (src, dst) = self
            .graph_resource
            .read_graph(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        // 获取首尾节点中心点
        let src_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(src.index().to_string())))
            .unwrap();
        let src_node_canvas_center = src_node_render_info.canvas_center();

        let dst_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(dst.index().to_string())))
            .unwrap();
        let dst_node_canvas_center = dst_node_render_info.canvas_center();

        let new_line_edge = LineEdge::new(
            Anchor::new_smooth(src_node_canvas_center),
            Anchor::new_smooth(dst_node_canvas_center),
        );
        self.graph_resource.with_graph(|graph| {
            graph.update_line_edge(self.edge_index, new_line_edge);
        });
    }
}

impl<'a> Widget for EdgeWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.update_bezier_edge(ui);
        self.update_line_edge(ui);
        // println!("TempEdgeWidget::ui");

        let edge_type = self
            .graph_resource
            .read_graph(|graph| graph.edge_type.clone());
        let response = match edge_type {
            EdgeType::Bezier => {
                let bezier_edge = self.graph_resource.read_graph(|graph| {
                    graph.get_edge(self.edge_index).unwrap().bezier_edge.clone()
                });
                ui.add(BezierWidget::new(
                    bezier_edge.clone(),
                    self.canvas_state_resource,
                    None,
                ))
            }
            EdgeType::Line => {
                let line_edge = self
                    .graph_resource
                    .read_graph(|graph| graph.get_edge(self.edge_index).unwrap().line_edge.clone());
                ui.add(LineWidget::new(
                    line_edge.clone(),
                    self.canvas_state_resource,
                ))
            }
        };

        // ui.add(BezierWidget::new(
        //     vec![source_anchor, target_anchor],
        //     EdgeIndex::new(0),
        // ));
        response
    }
}
