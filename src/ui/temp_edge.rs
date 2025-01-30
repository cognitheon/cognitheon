use egui::*;
use petgraph::graph::EdgeIndex;

use crate::{
    global::{CanvasStateResource, GraphResource},
    graph::edge::{TempEdge, TempEdgeTarget},
};

use super::bezier::{Anchor, BezierWidget};

pub struct TempEdgeWidget {
    pub edge: TempEdge,
}

impl TempEdgeWidget {
    pub fn new(edge: TempEdge) -> Self {
        Self { edge }
    }

    pub fn get_target_anchor(&self, ui: &mut egui::Ui) -> Anchor {
        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        match self.edge.target {
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
        println!("TempEdgeWidget::ui");
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        let screen_rect = graph_resource.read_graph(|graph| {
            let source_node = graph.get_node(self.edge.source).unwrap();
            source_node
                .render_info
                .as_ref()
                .unwrap()
                .screen_rect
                .clone()
        });

        let source_anchor = Anchor::new_smooth(screen_rect.center());
        // println!("source_anchor: {:?}", source_anchor.pos);
        let target_anchor = self.get_target_anchor(ui);
        let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

        ui.add(BezierWidget::new(
            vec![source_anchor, target_anchor],
            EdgeIndex::new(0),
        ));
        response
    }
}
