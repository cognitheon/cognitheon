use egui::*;
use petgraph::graph::EdgeIndex;

use crate::{
    colors::{edge_hover, edge_label_bg, edge_label_text, edge_selected},
    geometry::{edge_offset_direction, intersect_rect_with_pos, IntersectDirection},
    graph::{
        anchor::{BezierAnchor, LineAnchor},
        edge::EdgeType,
        edge_hit::{get_hovered_edge, publish_edge_hit_info},
        helpers::{get_node_render_info, node_rect_center},
        render_info::NodeRenderInfo,
        selection::GraphSelection,
    },
    resource::{CanvasStateResource, GraphResource},
};

use super::{
    bezier::{cubic_bezier, BezierEdge, BezierWidget},
    line_edge::{LineEdge, LineWidget},
};

/// 沿一条边采样的画布坐标折线（命中测试 + 选中/hover 高亮共用）。
///
/// - Line：两端锚点（2 点）。
/// - Bezier：每段三次曲线细分 `SUBDIV` 段，与渲染细分一致，保证高亮描线贴合实际曲线。
fn edge_canvas_samples(edge_type: &EdgeType, line: &LineEdge, bezier: &BezierEdge) -> Vec<Pos2> {
    const SUBDIV: usize = 100;
    match edge_type {
        EdgeType::Line => vec![line.source.canvas_pos, line.target.canvas_pos],
        EdgeType::Bezier => {
            // 与 BezierWidget::draw_bezier 同构的锚点拼接：source -> control... -> target。
            let full_anchors = std::iter::once(&bezier.source_anchor)
                .chain(bezier.control_anchors.iter())
                .chain(std::iter::once(&bezier.target_anchor))
                .collect::<Vec<_>>();
            let mut samples = Vec::new();
            for i in 0..full_anchors.len().saturating_sub(1) {
                let a = full_anchors[i];
                let b = full_anchors[i + 1];
                for step in 0..=SUBDIV {
                    let t = step as f32 / SUBDIV as f32;
                    samples.push(cubic_bezier(
                        a.canvas_pos,
                        a.handle_out_canvas_pos,
                        b.handle_in_canvas_pos,
                        b.canvas_pos,
                        t,
                    ));
                }
            }
            samples
        }
    }
}

/// 一条边在画布坐标系下的"中点"——边标签的锚定位置（纯函数，可单测）。
///
/// - Line：两端锚点的几何中点。
/// - Bezier：中间那一段三次曲线在 `t = 0.5` 处的采样点（复用 [`cubic_bezier`]，与渲染同构）。
///   多段（含 control_anchors）时取拼接后中间那一段，单段时即整条曲线的中点。
fn edge_canvas_midpoint(edge_type: &EdgeType, line: &LineEdge, bezier: &BezierEdge) -> Pos2 {
    match edge_type {
        EdgeType::Line => {
            ((line.source.canvas_pos.to_vec2() + line.target.canvas_pos.to_vec2()) * 0.5).to_pos2()
        }
        EdgeType::Bezier => {
            let full_anchors = std::iter::once(&bezier.source_anchor)
                .chain(bezier.control_anchors.iter())
                .chain(std::iter::once(&bezier.target_anchor))
                .collect::<Vec<_>>();
            // 段数 = 锚点数 - 1（至少 1 段）。取中间那一段的 t=0.5。
            let seg_count = full_anchors.len().saturating_sub(1).max(1);
            let mid_seg = seg_count / 2;
            let a = full_anchors[mid_seg];
            let b = full_anchors[mid_seg + 1];
            cubic_bezier(
                a.canvas_pos,
                a.handle_out_canvas_pos,
                b.handle_in_canvas_pos,
                b.canvas_pos,
                0.5,
            )
        }
    }
}

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

impl EdgeWidget {
    /// 本边是否在 `GraphSelection::Edge` 选中集中。选中真源唯一走 `GraphSelection::Edge`
    /// （§5：不复用 bezier.rs 里那个已注释失效的 `selected` 字段，避免双套真源）。
    fn is_selected(&self) -> bool {
        self.graph_resource.read_resource(|graph| {
            matches!(&graph.selected, GraphSelection::Edge(edges) if edges.contains(&self.edge_index))
        })
    }

    /// 在已渲染的边之上画一层醒目高亮描线（选中优先于 hover）。
    ///
    /// 走画布采样折线 → 逐点 `to_screen` 折线描边，线宽 `× scaling`（§3.2）。这是渲染态改动、
    /// 只在 edge.rs，与命中采样同一份几何（视觉与命中一致）。
    fn draw_highlight(&self, ui: &egui::Ui, canvas_samples: &[Pos2], color: egui::Color32) {
        if canvas_samples.len() < 2 {
            return;
        }
        let (scaling, screen_pts) = self.canvas_state_resource.read_resource(|cs| {
            (
                cs.transform.scaling,
                canvas_samples
                    .iter()
                    .map(|p| cs.to_screen(*p))
                    .collect::<Vec<_>>(),
            )
        });
        // 比默认边（2.0）更粗，× scaling 跟随缩放。
        let width = 4.0 * scaling;
        ui.painter()
            .add(Shape::line(screen_pts, Stroke::new(width, color)));
    }

    /// 只读渲染边标签：若 `Edge.text` 非空，在边中点附近画文字（带半透明背景小色块保证可读）。
    ///
    /// §3.1：`Edge.text` 在 `read_resource` 闭包内读取（闭包即锁作用域、不重入）。
    /// §3.2：字号与背景内边距 × `transform.scaling`，中点经 `to_screen` 走唯一坐标变换源。
    /// 多重边（平行边）下沿 [`edge_offset_direction`] 错开标签位置，复用既有多重边错开逻辑，避免重叠。
    fn draw_label(&self, ui: &egui::Ui, midpoint: Pos2) {
        let text = self
            .graph_resource
            .read_resource(|graph| graph.get_edge(self.edge_index).and_then(|e| e.text.clone()));
        let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
            return;
        };

        // 多重边错开：与 update_*_edge 同构地按无向多重边数判定，沿垂向偏移方向错开标签锚点，
        // 让平行边的标签不互相重叠（复用 edge_offset_direction）。
        let label_canvas_pos = self
            .graph_resource
            .read_resource(|graph| {
                graph
                    .graph
                    .edge_endpoints(self.edge_index)
                    .map(|(src, dst)| (src, dst, graph.edge_count_undirected(src, dst)))
            })
            .map(|(src, dst, edge_count)| {
                // §3.3：render_info 可能因节点本帧被删而缺失——用 Option 版反读、缺则回退原中点，
                // 绝不 .unwrap() panic。多重边（edge_count > 1）才需要错开。
                let centers = (get_node_render_info(src, ui), get_node_render_info(dst, ui));
                match (edge_count > 1, centers) {
                    (true, (Some(src_info), Some(dst_info))) => {
                        let offset_dir = edge_offset_direction(
                            src_info.canvas_center(),
                            dst_info.canvas_center(),
                        );
                        // 标签错开量比边线错开量（10.0）略大，给文字让出空间。
                        midpoint + offset_dir * 16.0
                    }
                    _ => midpoint,
                }
            })
            .unwrap_or(midpoint);

        let (scaling, screen_pos) = self
            .canvas_state_resource
            .read_resource(|cs| (cs.transform.scaling, cs.to_screen(label_canvas_pos)));

        let theme = ui.ctx().theme();
        let font = egui::FontId::new(13.0 * scaling, egui::FontFamily::Proportional);
        let painter = ui.painter();
        let galley = painter.layout_no_wrap(text, font, edge_label_text(theme));

        // 背景小色块：在文字外加内边距（× scaling）后垫底，圆角随缩放。
        let pad = egui::vec2(4.0, 2.0) * scaling;
        let bg_rect = egui::Rect::from_center_size(screen_pos, galley.size() + pad * 2.0);
        painter.rect_filled(
            bg_rect,
            egui::CornerRadius::same((3.0 * scaling) as u8),
            edge_label_bg(theme),
        );
        // 文字居中绘制（与背景块共心）。galley 颜色已在 layout 时定，传入 placeholder 不影响。
        painter.galley(
            bg_rect.center() - galley.size() / 2.0,
            galley,
            edge_label_text(theme),
        );
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

        // 取本帧最新几何，算出画布采样折线：① 发布到命中旁路供 determine_target 反读
        // （§3.3 边版本旁路，承认一帧延迟）；② 选中/hover 时复用同一份几何画高亮，命中与视觉一致。
        let (line_edge, bezier_edge) = self.graph_resource.read_resource(|graph| {
            let edge = graph.get_edge(self.edge_index).unwrap();
            (edge.line_edge.clone(), edge.bezier_edge.clone())
        });
        let canvas_samples = edge_canvas_samples(&edge_type, &line_edge, &bezier_edge);
        publish_edge_hit_info(ui.ctx(), self.edge_index, canvas_samples.clone());

        // 边标签锚点（画布坐标）：line/bezier 各取中点。在 line_edge/bezier_edge 被下方 match
        // move 进 widget 之前算好。
        let label_midpoint = edge_canvas_midpoint(&edge_type, &line_edge, &bezier_edge);

        let response = match edge_type {
            EdgeType::Bezier => ui.add(&mut BezierWidget::new(
                bezier_edge,
                self.canvas_state_resource.clone(),
            )),
            EdgeType::Line => ui.add(LineWidget::new(
                line_edge,
                self.canvas_state_resource.clone(),
            )),
        };

        // 选中 / hover 高亮：选中优先（更醒目的红），其次 hover（浅蓝）。
        let theme = ui.ctx().theme();
        if self.is_selected() {
            self.draw_highlight(ui, &canvas_samples, edge_selected(theme));
        } else if get_hovered_edge(ui.ctx()) == Some(self.edge_index) {
            self.draw_highlight(ui, &canvas_samples, edge_hover(theme));
        }

        // 边标签只读渲染（画在线/高亮之上）：Edge.text 非空才绘制。
        self.draw_label(ui, label_midpoint);

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::anchor::{BezierAnchor, LineAnchor};

    #[test]
    fn line_midpoint_is_segment_center() {
        let line = LineEdge::new(
            LineAnchor::new(Pos2::new(0.0, 0.0)),
            LineAnchor::new(Pos2::new(10.0, 20.0)),
        );
        // bezier 占位（Line 分支不读它）。
        let bezier = BezierEdge::new(
            BezierAnchor::new_smooth(Pos2::ZERO),
            BezierAnchor::new_smooth(Pos2::ZERO),
        );
        let mid = edge_canvas_midpoint(&EdgeType::Line, &line, &bezier);
        assert_eq!(mid, Pos2::new(5.0, 10.0));
    }

    #[test]
    fn bezier_symmetric_curve_midpoint_on_axis() {
        // 对称三次曲线（端点在 x 轴、控制柄上下对称）→ t=0.5 处 x 居中。
        let source = BezierAnchor::new_smooth(Pos2::new(0.0, 0.0))
            .with_handles(Pos2::new(0.0, 0.0), Pos2::new(0.0, 10.0));
        let target = BezierAnchor::new_smooth(Pos2::new(10.0, 0.0))
            .with_handles(Pos2::new(10.0, 10.0), Pos2::new(10.0, 0.0));
        let bezier = BezierEdge::new(source, target);
        let line = LineEdge::new(LineAnchor::new(Pos2::ZERO), LineAnchor::new(Pos2::ZERO));
        let mid = edge_canvas_midpoint(&EdgeType::Bezier, &line, &bezier);
        assert!((mid.x - 5.0).abs() < 1e-4, "mid.x = {}", mid.x);
    }
}
