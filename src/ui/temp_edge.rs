use egui::*;
use petgraph::graph::NodeIndex;

use crate::{
    globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource},
    graph::{edge::EdgeType, node::NodeRenderInfo},
    ui::line_edge::LineWidget,
};

use super::{
    bezier::{Anchor, BezierEdge, BezierWidget},
    line_edge::LineEdge,
};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TempEdge {
    pub source: NodeIndex,
    pub target: TempEdgeTarget,
    pub bezier_edge: BezierEdge,
    pub line_edge: LineEdge,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum TempEdgeTarget {
    None,
    Node(NodeIndex),
    Point(egui::Pos2),
}

pub struct TempEdgeWidget<'a> {
    pub temp_edge: &'a TempEdge,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
}

impl<'a> TempEdgeWidget<'a> {
    pub fn get_target_anchor(&self, ui: &mut egui::Ui) -> Anchor {
        match self.temp_edge.target {
            TempEdgeTarget::Node(node_index) => {
                let node_id = node_index.index().to_string();
                let node_render_info: NodeRenderInfo =
                    ui.ctx().data(|d| d.get_temp(Id::new(node_id))).unwrap();

                Anchor::new_smooth(node_render_info.canvas_center())
            }
            TempEdgeTarget::Point(point) => Anchor::new_smooth(point),
            TempEdgeTarget::None => Anchor::new_smooth(Pos2::ZERO),
        }
    }
}

impl<'a> Widget for TempEdgeWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // println!("TempEdgeWidget::ui");

        // let temp_edge: TempEdge = graph_resource.read_graph(|graph| graph.get_temp_edge().unwrap());

        let node_id = self.temp_edge.source;
        let node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|d| d.get_temp(Id::new(node_id.index().to_string())))
            .unwrap();
        let screen_rect = node_render_info.screen_rect(&self.canvas_state_resource);
        println!("screen_rect: {:?}", screen_rect);
        let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

        let edge_type = self
            .graph_resource
            .read_graph(|graph| graph.edge_type.clone());
        match edge_type {
            EdgeType::Bezier => {
                // 获取节点中心点
                let node_render_info: NodeRenderInfo = ui
                    .ctx()
                    .data(|d| d.get_temp(Id::new(node_id.index().to_string())))
                    .unwrap();
                let node_center = node_render_info.canvas_center();
                println!("node_center: {:?}", node_center);
                // 获取起始锚点
                let source_anchor = &self.temp_edge.bezier_edge.source_anchor;
                // 计算差异
                let delta = node_center - source_anchor.canvas_pos;
                println!("delta: {:?}", delta);
                ui.add(BezierWidget::new(
                    self.temp_edge.bezier_edge.clone(),
                    self.canvas_state_resource,
                    None,
                ));
            }
            EdgeType::Line => {
                ui.add(LineWidget::new(
                    self.temp_edge.line_edge.clone(),
                    self.canvas_state_resource,
                ));
            }
        }

        // ui.add(BezierWidget::new(
        //     vec![source_anchor, target_anchor],
        //     EdgeIndex::new(0),
        // ));
        response
    }
}
