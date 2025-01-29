use egui::*;
use petgraph::graph::EdgeIndex;

use crate::{
    canvas::CanvasState,
    graph::{
        edge::{TempEdge, TempEdgeTarget},
        Graph,
    },
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
        let graph: Graph = ui.ctx().data(|d| d.get_temp(Id::new("graph"))).unwrap();
        match self.edge.target {
            TempEdgeTarget::Node(node_index) => {
                let node = graph.get_node(node_index).unwrap();
                let rect = node.render_info.as_ref().unwrap().screen_rect;
                Anchor::new_smooth(rect.center())
            }
            TempEdgeTarget::Point(point) => Anchor::new_smooth(point),
            TempEdgeTarget::None => Anchor::new_smooth(Pos2::ZERO),
        }
    }
}

impl Widget for TempEdgeWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let graph: Graph = ui.ctx().data(|d| d.get_temp(Id::new("graph"))).unwrap();
        let source_node = graph.get_node(self.edge.source).unwrap();
        let rect = source_node.render_info.as_ref().unwrap().screen_rect;
        let source_anchor = Anchor::new_smooth(rect.center());
        let target_anchor = self.get_target_anchor(ui);
        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        let mut canvas_state: CanvasState = ui
            .ctx()
            .data(|d| d.get_temp(Id::new("canvas_state")))
            .unwrap();

        ui.add(BezierWidget::new(
            vec![source_anchor, target_anchor],
            &mut canvas_state,
            EdgeIndex::new(0),
        ));
        response
    }
}
