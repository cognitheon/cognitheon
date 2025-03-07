use egui::*;
use petgraph::graph::EdgeIndex;

use crate::{
    geometry::{edge_offset_direction, intersect_rect_with_pos, IntersectDirection},
    graph::{
        anchor::{BezierAnchor, LineAnchor},
        edge::EdgeType,
        helpers::{get_node_render_info, node_rect_center},
        render_info::NodeRenderInfo,
    },
    resource::{CanvasStateResource, GraphResource},
};

use super::{
    bezier::{BezierEdge, BezierWidget},
    line_edge::{LineEdge, LineWidget},
};

pub struct EdgeWidget {
    pub edge_index: EdgeIndex,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
}

impl EdgeWidget {
    // 根据实时的节点位置更新贝塞尔曲线锚点信息
    fn update_bezier_edge(&self, ui: &egui::Ui) {
        // 获取首尾节点
        let (src_node_index, dst_node_index) = self
            .graph_resource
            .read_resource(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        let edge_count = self
            .graph_resource
            .read_resource(|graph| graph.edge_count_undirected(src_node_index, dst_node_index));
        // println!("edge_count: {:?}", edge_count);

        // 获取首尾节点中心点
        let src_node_render_info = get_node_render_info(src_node_index, ui);

        let dst_node_render_info = get_node_render_info(dst_node_index, ui);

        if src_node_render_info.is_none() || dst_node_render_info.is_none() {
            return;
        }

        let src_node_canvas_center = node_rect_center(src_node_index, ui);
        let dst_node_canvas_center = node_rect_center(dst_node_index, ui);

        let mut src_center = src_node_canvas_center;
        let mut dst_center = dst_node_canvas_center;

        // println!("========================");
        // println!("edge_count: {}", edge_count);
        // println!("src_center: {:?}", src_center);
        // println!("dst_center: {:?}", dst_center);
        if edge_count != 1 {
            let offset_dir = edge_offset_direction(src_node_canvas_center, dst_node_canvas_center);
            // println!("offset_dir: {:?}", offset_dir);
            let offset_amount = 10.0;
            // let edge_dir = (dst_node_canvas_center - src_node_canvas_center).normalized();
            src_center += offset_dir * offset_amount;
            dst_center += offset_dir * offset_amount;
        }

        // println!("src_center: {:?}", src_center);
        // println!("dst_center: {:?}", dst_center);
        // println!("========================");

        let Some((source_canvas_pos, source_dir)) = intersect_rect_with_pos(
            src_node_render_info.unwrap().canvas_rect,
            src_center,
            dst_center,
        ) else {
            return;
        };
        let Some((target_canvas_pos, target_dir)) = intersect_rect_with_pos(
            dst_node_render_info.unwrap().canvas_rect,
            dst_center,
            src_center,
        ) else {
            return;
        };

        let offset_amount = 50.0;

        let handle_offset_source = match source_dir {
            IntersectDirection::Left => Vec2::new(-offset_amount, 0.0),
            IntersectDirection::Right => Vec2::new(offset_amount, 0.0),
            IntersectDirection::Top => Vec2::new(0.0, -offset_amount),
            IntersectDirection::Bottom => Vec2::new(0.0, offset_amount),
        };
        // println!("target_dir: {:?}", target_dir);
        let handle_offset_target = match target_dir {
            IntersectDirection::Left => Vec2::new(-offset_amount, 0.0),
            IntersectDirection::Right => Vec2::new(offset_amount, 0.0),
            IntersectDirection::Top => Vec2::new(0.0, -offset_amount),
            IntersectDirection::Bottom => Vec2::new(0.0, offset_amount),
        };

        let source_anchor = BezierAnchor::new_smooth(source_canvas_pos).with_handles(
            source_canvas_pos + handle_offset_source,
            source_canvas_pos + handle_offset_source,
        );
        let target_anchor = BezierAnchor::new_smooth(target_canvas_pos).with_handles(
            target_canvas_pos + handle_offset_target,
            target_canvas_pos + handle_offset_target,
        );

        // 获取已有贝塞尔曲线控制点锚点
        let bezier_edge = self
            .graph_resource
            .read_resource(|graph| graph.get_edge(self.edge_index).unwrap().bezier_edge.clone());
        let control_anchors = bezier_edge.control_anchors;

        let new_bezier_edge =
            BezierEdge::new(source_anchor, target_anchor).with_control_anchors(control_anchors);

        // let mut new_bezier_edge = BezierEdge::new(
        //     Anchor::new_smooth(source_canvas_pos.unwrap()),
        //     Anchor::new_smooth(target_canvas_pos.unwrap()),
        // );
        // new_bezier_edge.update_control_anchors(control_anchors);

        self.graph_resource.with_resource(|graph| {
            graph.update_bezier_edge(self.edge_index, new_bezier_edge);
        });
    }

    fn update_line_edge(&self, ui: &egui::Ui) {
        // 获取首尾节点
        let (src_node_index, dst_node_index) = self
            .graph_resource
            .read_resource(|graph| graph.graph.edge_endpoints(self.edge_index))
            .unwrap();

        let edge_count = self
            .graph_resource
            .read_resource(|graph| graph.edge_count_undirected(src_node_index, dst_node_index));
        // println!("edge_count: {:?}", edge_count);

        // 获取首尾节点中心点
        let src_node_render_info: Option<NodeRenderInfo> = get_node_render_info(src_node_index, ui);

        let dst_node_render_info: Option<NodeRenderInfo> = get_node_render_info(dst_node_index, ui);
        if src_node_render_info.is_none() || dst_node_render_info.is_none() {
            return;
        }

        let src_node_canvas_center = src_node_render_info.unwrap().canvas_center();
        let dst_node_canvas_center = dst_node_render_info.unwrap().canvas_center();

        let mut src_center = src_node_canvas_center;
        let mut dst_center = dst_node_canvas_center;

        if edge_count != 1 {
            let offset_dir = edge_offset_direction(src_node_canvas_center, dst_node_canvas_center);
            // println!("offset_dir: {:?}", offset_dir);
            let offset_amount = 10.0;
            src_center += offset_dir * offset_amount;
            dst_center += offset_dir * offset_amount;
        }

        let Some((source_canvas_pos, _source_dir)) = intersect_rect_with_pos(
            src_node_render_info.unwrap().canvas_rect,
            src_center,
            dst_center,
        ) else {
            return;
        };
        let Some((target_canvas_pos, _target_dir)) = intersect_rect_with_pos(
            dst_node_render_info.unwrap().canvas_rect,
            dst_center,
            src_center,
        ) else {
            return;
        };

        let new_line_edge = LineEdge::new(
            LineAnchor::new(source_canvas_pos),
            LineAnchor::new(target_canvas_pos),
        );

        self.graph_resource.with_resource(|graph| {
            graph.update_line_edge(self.edge_index, new_line_edge);
        });
    }
}

impl Widget for EdgeWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.update_bezier_edge(ui);
        self.update_line_edge(ui);
        // println!("TempEdgeWidget::ui");

        let edge_type = self
            .graph_resource
            .read_resource(|graph| graph.edge_type.clone());
        let response = match edge_type {
            EdgeType::Bezier => {
                let bezier_edge = self.graph_resource.read_resource(|graph| {
                    graph.get_edge(self.edge_index).unwrap().bezier_edge.clone()
                });
                ui.add(&mut BezierWidget::new(
                    bezier_edge.clone(),
                    self.canvas_state_resource,
                ))
            }
            EdgeType::Line => {
                let line_edge = self.graph_resource.read_resource(|graph| {
                    graph.get_edge(self.edge_index).unwrap().line_edge.clone()
                });
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
