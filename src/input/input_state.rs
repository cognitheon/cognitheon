// 在 src/input/input_state.rs 中

use egui::Pos2;
use petgraph::graph::{EdgeIndex, NodeIndex};

#[derive(Debug, Clone, PartialEq)]
pub enum InputState {
    /// 空闲状态 - 系统等待新的输入
    Idle,

    /// 平移状态 - 用户正在平移画布
    Panning { last_cursor_pos: Pos2 },

    /// 缩放状态 - 用户正在缩放画布
    Zooming { center: Pos2, start_scale: f32 },

    /// 节点拖动状态 - 用户正在拖动一个节点
    DraggingNode {
        node_index: NodeIndex,
        start_pos: Pos2,
        // 用于多选拖动
        is_selection_drag: bool,
        selected_indices: Vec<NodeIndex>,
    },

    /// 节点编辑状态 - 用户正在编辑节点文本
    EditingNode { node_index: NodeIndex },

    /// 创建边状态 - 用户正在从源节点创建一条边
    CreatingEdge {
        source_node: NodeIndex,
        current_cursor_pos: Pos2,
    },

    /// 拖拽控制点状态 - 用户正在调整贝塞尔曲线的控制点
    DraggingControlPoint {
        edge_index: EdgeIndex,
        point_index: usize,
        start_pos: Pos2,
    },

    /// 框选状态 - 用户正在通过拖动框选节点
    Selecting {
        start_pos: Pos2,
        current_pos: Pos2,
        /// 是否添加到现有选择
        add_to_selection: bool,
    },

    /// 移动画布中选中内容状态
    MovingSelection {
        start_pos: Pos2,
        nodes: Vec<NodeIndex>,
    },
}

impl InputState {
    /// 返回该状态是否为"忙"状态 - 会阻止其他输入的处理
    pub fn is_busy(&self) -> bool {
        !matches!(self, InputState::Idle)
    }

    /// 返回该状态是否会处理持续性的鼠标移动
    pub fn handles_mouse_motion(&self) -> bool {
        matches!(
            self,
            InputState::Panning { .. }
                | InputState::DraggingNode { .. }
                | InputState::CreatingEdge { .. }
                | InputState::DraggingControlPoint { .. }
                | InputState::Selecting { .. }
                | InputState::MovingSelection { .. }
        )
    }

    /// 返回状态是否涉及拖动操作
    pub fn is_dragging(&self) -> bool {
        matches!(
            self,
            InputState::DraggingNode { .. }
                | InputState::DraggingControlPoint { .. }
                | InputState::Selecting { .. }
                | InputState::MovingSelection { .. }
        )
    }
}
