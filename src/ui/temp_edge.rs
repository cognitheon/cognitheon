use egui::*;
use petgraph::graph::{EdgeIndex, NodeIndex};

use crate::{
    global::{CanvasStateResource, GraphResource},
    graph::edge::EdgeType,
    ui::bezier::BezierEdge,
};

use super::bezier::{Anchor, BezierWidget};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TempEdge {
    pub source: NodeIndex,
    pub target: TempEdgeTarget,
    pub edge_type: EdgeType,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum TempEdgeTarget {
    None,
    Node(NodeIndex),
    Point(egui::Pos2),
}

pub struct TempEdgeWidget {
    pub temp_edge: TempEdge,
}

impl TempEdgeWidget {
    pub fn get_target_anchor(&self, ui: &mut egui::Ui) -> Anchor {
        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        let temp_edge: TempEdge = graph_resource.read_graph(|graph| graph.get_temp_edge().unwrap());
        match temp_edge.target {
            TempEdgeTarget::Node(node_index) => {
                let screen_rect = graph_resource.read_graph(|graph| {
                    let node = graph.get_node(node_index).unwrap();
                    node.render_info.as_ref().unwrap().screen_rect
                });

                Anchor::new_smooth(screen_rect.center())
            }
            TempEdgeTarget::Point(point) => Anchor::new_smooth(point),
            TempEdgeTarget::None => Anchor::new_smooth(Pos2::ZERO),
        }
    }
}

impl Widget for TempEdgeWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // println!("TempEdgeWidget::ui");

        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        let temp_edge: TempEdge = graph_resource.read_graph(|graph| graph.get_temp_edge().unwrap());

        let screen_rect = graph_resource.read_graph(|graph| {
            let source_node = graph.get_node(temp_edge.source).unwrap();
            source_node
                .render_info
                .as_ref()
                .unwrap()
                .screen_rect
                .clone()
        });

        let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

        match temp_edge.edge_type {
            EdgeType::Bezier(bezier_edge) => {
                ui.add(BezierWidget::new(
                    bezier_edge,
                    Some(Box::new(move |bezier_edge| {
                        graph_resource.with_graph(|graph| {
                            graph.set_temp_edge(Some(TempEdge {
                                source: temp_edge.source,
                                target: temp_edge.target.clone(),
                                edge_type: EdgeType::Bezier(bezier_edge),
                            }));
                        });
                    }) as Box<dyn Fn(BezierEdge) + '_>),
                ));
            }
            EdgeType::Line => {}
        }

        // ui.add(BezierWidget::new(
        //     vec![source_anchor, target_anchor],
        //     EdgeIndex::new(0),
        // ));
        response
    }
}
