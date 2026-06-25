// 在 src/input/input_state.rs 中

use egui::Pos2;
use petgraph::graph::{EdgeIndex, NodeIndex};

#[derive(Debug, Clone, PartialEq)]
pub enum InputState {
    /// 空闲状态 - 系统等待新的输入
    Idle,

    /// 平移状态 - 用户正在平移画布
    Panning {
        last_cursor_pos: Pos2,
        dragging: bool,
    },

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

    /// 右键按下待定状态 —— 区分"右键单击（弹上下文菜单）"与"右键拖拽（连边手势）"。
    ///
    /// 右键 press 时进入此态、记录命中目标与按下屏幕坐标，**不立即**进 `CreatingEdge`。
    /// 随后：指针移动超过位移阈值 → 转 `CreatingEdge`（仅当目标是节点，画临时边连边）；
    /// 右键 release 时若位移仍小于阈值 → 判为单击 → 写"上下文菜单请求"到 temp data。
    /// 消歧只在 `state_manager.rs` 的 secondary release / motion 单点决策（AGENTS.md §3.4）。
    PendingSecondary {
        /// 按下时命中的目标（节点 / 边 / 画布），决定拖拽是否连边、单击弹哪种菜单。
        target: super::events::InputTarget,
        /// 按下时的指针屏幕坐标，用于位移阈值判定与菜单锚点。
        start_pos: Pos2,
    },

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
                | InputState::PendingSecondary { .. }
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
