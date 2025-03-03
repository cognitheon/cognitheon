// 在 src/input/events.rs 中

use egui::{Key, Modifiers, PointerButton, Pos2, Vec2};
use petgraph::graph::{EdgeIndex, NodeIndex};

#[derive(Debug, Clone)]
pub enum InputEvent {
    /// 一次性事件 - 只触发一次
    OneShot(OneShotEvent),

    /// 持续性事件 - 每帧都可能触发
    Continuous(ContinuousEvent),
}

#[derive(Debug, Clone)]
pub enum OneShotEvent {
    /// 鼠标按下
    MouseDown {
        button: PointerButton,
        pos: Pos2,
        modifiers: Modifiers,
    },

    /// 鼠标释放
    MouseUp {
        button: PointerButton,
        pos: Pos2,
        modifiers: Modifiers,
    },

    /// 鼠标点击（按下后释放）
    Click {
        button: PointerButton,
        pos: Pos2,
        modifiers: Modifiers,
    },

    /// 双击
    DoubleClick {
        button: PointerButton,
        pos: Pos2,
        modifiers: Modifiers,
    },

    /// 按键按下
    KeyDown { key: Key, modifiers: Modifiers },

    /// 按键释放
    KeyUp { key: Key, modifiers: Modifiers },
}

#[derive(Debug, Clone)]
pub enum ContinuousEvent {
    /// 鼠标移动
    MouseMove { pos: Pos2, delta: Vec2 },

    /// 鼠标拖动（按下按钮时的移动）
    Drag {
        button: PointerButton,
        pos: Pos2,
        delta: Vec2,
        modifiers: Modifiers,
    },

    /// 滚轮滚动
    Scroll { delta: Vec2 },

    /// 缩放（捏合或滚轮+Ctrl）
    Zoom { delta: f32, center: Pos2 },
}

/// 输入事件的目标
#[derive(Debug, Clone, PartialEq)]
pub enum InputTarget {
    /// 画布背景
    Canvas,

    /// 特定节点
    Node(NodeIndex),

    /// 特定边
    Edge(EdgeIndex),

    /// 贝塞尔曲线控制点
    ControlPoint {
        edge_index: EdgeIndex,
        point_index: usize,
    },

    /// 其他UI元素
    UI,
}
