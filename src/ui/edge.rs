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
    fn intersect_rect_simple(rect: Rect, p: Pos2) -> Option<Pos2> {
        let c = rect.center();
        let dx = p.x - c.x;
        let dy = p.y - c.y;

        // 若方向向量都为 0，无法判定方向
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            return None;
        }

        let mut t_candidates = Vec::new();

        // 计算与“左右边”相交的 t_x
        if dx > 0.0 {
            // 会先撞到 rect.max.x
            let t = (rect.max.x - c.x) / dx;
            if t >= 0.0 {
                let y_on_edge = c.y + t * dy;
                if y_on_edge >= rect.min.y && y_on_edge <= rect.max.y {
                    t_candidates.push((t, Pos2::new(rect.max.x, y_on_edge)));
                }
            }
        } else if dx < 0.0 {
            // 会先撞到 rect.min.x
            let t = (rect.min.x - c.x) / dx;
            if t >= 0.0 {
                let y_on_edge = c.y + t * dy;
                if y_on_edge >= rect.min.y && y_on_edge <= rect.max.y {
                    t_candidates.push((t, Pos2::new(rect.min.x, y_on_edge)));
                }
            }
        }

        // 计算与“上下边”相交的 t_y
        if dy > 0.0 {
            // 会先撞到 rect.max.y
            let t = (rect.max.y - c.y) / dy;
            if t >= 0.0 {
                let x_on_edge = c.x + t * dx;
                if x_on_edge >= rect.min.x && x_on_edge <= rect.max.x {
                    t_candidates.push((t, Pos2::new(x_on_edge, rect.max.y)));
                }
            }
        } else if dy < 0.0 {
            // 会先撞到 rect.min.y
            let t = (rect.min.y - c.y) / dy;
            if t >= 0.0 {
                let x_on_edge = c.x + t * dx;
                if x_on_edge >= rect.min.x && x_on_edge <= rect.max.x {
                    t_candidates.push((t, Pos2::new(x_on_edge, rect.min.y)));
                }
            }
        }

        // 取最小 t
        t_candidates
            .into_iter()
            .min_by(|(t1, _), (t2, _)| t1.partial_cmp(t2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, pos)| pos)
    }

    // 由节点中心点和目标点计算出在节点边框上的点
    fn get_source_pos(
        &self,
        ui: &egui::Ui,
        source_render_info: NodeRenderInfo,
        target_render_info: NodeRenderInfo,
    ) -> Pos2 {
        // 计算出在节点边框上的点
        let source_node_canvas_center = source_render_info.canvas_center();
        let target_node_canvas_center = target_render_info.canvas_center();
        let source_node_rect = source_render_info.canvas_rect;
        let target_node_rect = target_render_info.canvas_rect;

        // 1. 计算斜率
        let slope = target_node_canvas_center - source_node_canvas_center;
        // 2. 计算在节点边框上的点
        source_node_canvas_center
    }

    fn get_target_pos(
        &self,
        ui: &egui::Ui,
        source_node_canvas_center: Pos2,
        target_node_canvas_center: Pos2,
    ) -> Pos2 {
        let (src, dst) = self
            .graph_resource
            .read_graph(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        let dst_node_render_info: NodeRenderInfo = ui
            .ctx()
            .data(|reader| reader.get_temp(Id::new(dst.index().to_string())))
            .unwrap();
        let dst_node_canvas_center = dst_node_render_info.canvas_center();

        dst_node_canvas_center
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

        let source_canvas_pos = EdgeWidget::intersect_rect_simple(
            src_node_render_info.canvas_rect,
            dst_node_canvas_center,
        );
        let target_canvas_pos = EdgeWidget::intersect_rect_simple(
            dst_node_render_info.canvas_rect,
            src_node_canvas_center,
        );

        // 获取已有贝塞尔曲线控制点锚点
        let bezier_edge = self
            .graph_resource
            .read_graph(|graph| graph.get_edge(self.edge_index).unwrap().bezier_edge.clone());
        let control_anchors = bezier_edge.control_anchors;

        let mut new_bezier_edge = BezierEdge::new(
            Anchor::new_smooth(source_canvas_pos.unwrap()),
            Anchor::new_smooth(target_canvas_pos.unwrap()),
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

        let source_canvas_pos = EdgeWidget::intersect_rect_simple(
            src_node_render_info.canvas_rect,
            dst_node_canvas_center,
        );
        let target_canvas_pos = EdgeWidget::intersect_rect_simple(
            dst_node_render_info.canvas_rect,
            src_node_canvas_center,
        );

        let new_line_edge = LineEdge::new(
            Anchor::new_smooth(source_canvas_pos.unwrap()),
            Anchor::new_smooth(target_canvas_pos.unwrap()),
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
