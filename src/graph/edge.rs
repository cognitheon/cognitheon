use std::fmt::Display;

use petgraph::graph::NodeIndex;

use crate::{
    resource::CanvasStateResource,
    ui::{bezier::BezierEdge, line_edge::LineEdge},
};

use super::anchor::{BezierAnchor, LineAnchor};

/// 边的来源：区分用户手画的边与 wikilink 自动投影的边。
///
/// wikilink `resolve_links` 的幂等投影**只**管理 [`EdgeOrigin::Wiki`] 边（按当前正文 `[[标题]]`
/// 增删同步），**绝不**触碰 [`EdgeOrigin::Manual`] 边。`Manual` 作为默认值（`#[serde(default)]`）：
/// 旧 `.cnt`（无此字段）反序列化后一律视为手画边，永不被 resolve 误删。
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EdgeOrigin {
    /// 用户手画的边（连边手势创建）。默认值——保证旧存档兼容、永不被自动删除。
    #[default]
    Manual,
    /// wikilink `[[标题]]` 自动投影出的边，由 `resolve_links` 幂等增删管理。
    Wiki,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Edge {
    pub id: u64,
    pub source: NodeIndex,
    pub target: NodeIndex,
    pub text: Option<String>,
    /// 边来源标记。旧存档无此字段时按 [`EdgeOrigin::Manual`] 反序列化（向后兼容）。
    #[serde(default)]
    pub origin: EdgeOrigin,
    pub bezier_edge: BezierEdge,
    pub line_edge: LineEdge,
}

impl Edge {
    pub fn new(
        source: NodeIndex,
        target: NodeIndex,
        source_canvas_pos: egui::Pos2,
        target_canvas_pos: egui::Pos2,
        canvas_state_resource: CanvasStateResource,
    ) -> Self {
        Self::with_origin(
            source,
            target,
            source_canvas_pos,
            target_canvas_pos,
            canvas_state_resource,
            EdgeOrigin::Manual,
        )
    }

    /// 构造一条 wikilink 自动投影边（`EdgeOrigin::Wiki`）。几何与 [`Edge::new`] 一致，
    /// 仅来源标记不同，供 `resolve_links` 幂等管理。
    pub fn new_wiki(
        source: NodeIndex,
        target: NodeIndex,
        source_canvas_pos: egui::Pos2,
        target_canvas_pos: egui::Pos2,
        canvas_state_resource: CanvasStateResource,
    ) -> Self {
        Self::with_origin(
            source,
            target,
            source_canvas_pos,
            target_canvas_pos,
            canvas_state_resource,
            EdgeOrigin::Wiki,
        )
    }

    fn with_origin(
        source: NodeIndex,
        target: NodeIndex,
        source_canvas_pos: egui::Pos2,
        target_canvas_pos: egui::Pos2,
        canvas_state_resource: CanvasStateResource,
        origin: EdgeOrigin,
    ) -> Self {
        let edge_id =
            canvas_state_resource.read_resource(|canvas_state| canvas_state.new_edge_id());
        Self {
            id: edge_id,
            source,
            target,
            text: None,
            origin,
            bezier_edge: BezierEdge::new(
                BezierAnchor::new_smooth(source_canvas_pos),
                BezierAnchor::new_smooth(target_canvas_pos),
            ),

            line_edge: LineEdge::new(
                LineAnchor::new(source_canvas_pos),
                LineAnchor::new(target_canvas_pos),
            ),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum EdgeType {
    Line,
    Bezier,
}

impl Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let edge_type_str = match self {
            EdgeType::Line => "Line",
            EdgeType::Bezier => "Bezier",
        };
        write!(f, "{}", edge_type_str)
    }
}
