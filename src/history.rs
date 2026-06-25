//! 撤销/重做历史栈（运行态，**不持久化**）。
//!
//! 历史模型 = **整图快照**：[`Graph`] 已 `derive(Clone + Serialize + Deserialize)`，且其内部
//! `StableGraph` 删点/删边后 `NodeIndex`/`EdgeIndex` 保持稳定（AGENTS.md §3.3），所以"克隆整张图"
//! 天然就是一份语义完整、索引稳定的快照——撤销一次删除时，节点连同其边复活且索引不变。
//!
//! 设计取舍：
//! - **只快照 [`Graph`]，不含 [`crate::canvas::CanvasState`] 的 `transform`**：平移/缩放是视图态、
//!   不是数据变更，撤销不回滚视图（spec）。
//! - **不进序列化**：本结构由 [`crate::app::CognitheonApp`] 以 `#[serde(skip)]` 持有，`.cnt` /
//!   eframe storage 格式零变更，旧档照常 `load`（AGENTS.md §7）。
//! - **共享与注入**：内部是 `Arc<Mutex<…>>`，按 §3.1"全局态 = 被多处 clone 的同一个 Arc、构造函数
//!   注入逐层下传"的范式，把同一个 `History` 句柄下发给所有会改图的组件（主线程 / 输入状态机 /
//!   节点 widget），保证它们写同一个真源。它**不是** `Resource<T>` 三大 SSOT 之一，而是协调 SSOT
//!   写入的薄封装。
//!
//! 关键接缝（[`History::mutate`]）：所有"会改变图数据"的写入都经它打快照——**先在 `with_resource`
//! 写闭包之外单独 `read_resource` 克隆出 before 压入 undo、清空 redo，再 `with_resource` 跑变更**。
//! 严守 §3.1：克隆必须在写闭包外单独 `read`，**绝不在写闭包内重入 `read_resource` 同资源**（自死锁）。

use std::sync::{Arc, Mutex};

use crate::graph::graph_impl::Graph;
use crate::resource::GraphResource;

/// 历史栈默认容量（最多保留多少步可撤销）。超出后丢弃最旧的一步。
pub const DEFAULT_CAP: usize = 200;

/// 历史栈内部状态：两条整图快照栈。
#[derive(Debug)]
struct HistoryInner {
    /// 可撤销的过去快照（栈顶 = 最近一次变更之前的图）。
    undo: Vec<Graph>,
    /// 可重做的未来快照（栈顶 = 最近一次撤销掉的图）。
    redo: Vec<Graph>,
    /// 容量上限：`undo` 超过该长度时从底部丢弃最旧的。
    cap: usize,
    /// 暂存的"变更前"快照（拖拽 / 编辑这类**跨多帧**的操作用）：在操作开始时 `stage` 进来，
    /// 操作结束时若图确有改变才 `commit`（压入 undo），否则 `discard`。保证一次拖拽 = 一个撤销
    /// 单元、且原地未动的点击不产生空撤销项。
    staged: Option<Graph>,
}

impl HistoryInner {
    fn new(cap: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            cap: cap.max(1),
            staged: None,
        }
    }

    /// 压入一份"变更前"快照到 undo 栈，并清空 redo（产生新分支）。超容量从底部丢弃。
    fn push_undo(&mut self, snapshot: Graph) {
        self.undo.push(snapshot);
        self.redo.clear();
        if self.undo.len() > self.cap {
            // 丢弃最旧的一步，保持容量。
            let overflow = self.undo.len() - self.cap;
            self.undo.drain(0..overflow);
        }
    }
}

/// 撤销/重做历史。可廉价 `clone`（仅克隆内部 `Arc`，共享同一真源），用于构造函数注入。
#[derive(Clone, Debug)]
pub struct History {
    inner: Arc<Mutex<HistoryInner>>,
}

impl Default for History {
    fn default() -> Self {
        Self::with_cap(DEFAULT_CAP)
    }
}

impl History {
    /// 以指定容量新建一个空历史。
    pub fn with_cap(cap: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HistoryInner::new(cap))),
        }
    }

    /// 是否有可撤销的步骤。
    pub fn can_undo(&self) -> bool {
        !self.inner.lock().unwrap().undo.is_empty()
    }

    /// 是否有可重做的步骤。
    pub fn can_redo(&self) -> bool {
        !self.inner.lock().unwrap().redo.is_empty()
    }

    /// 暂存一份"变更前"快照，用于**跨多帧**的操作（拖拽 / 编辑）。在操作开始那一帧调用一次，
    /// 后续每帧的写入不再各自打快照；操作结束时由 [`History::commit_staged_if_changed`]（图确有
    /// 改变才提交）或 [`History::discard_staged`] 收尾。重复 `stage`（已有暂存时）会被忽略，避免
    /// 一次拖拽里多次开始把中间态当起点。
    ///
    /// `before` 由调用方在 `with_resource` 写闭包**之外**单独 `read_resource` 克隆（§3.1）。
    pub fn stage(&self, before: Graph) {
        let mut inner = self.inner.lock().unwrap();
        if inner.staged.is_none() {
            inner.staged = Some(before);
        }
    }

    /// 把暂存快照按"图数据是否真的变了"提交或丢弃：变了则压入 undo（一个撤销单元），
    /// 没变则 [`discard_staged`](Self::discard_staged)。避免"双击进入编辑但什么也没改就退出"
    /// 之类的空操作产生一个无意义撤销项。
    ///
    /// "是否变了"以**持久化数据**（[`Graph`] 的 serde 形状）为准——只比较真正会进 `.cnt` 的字段
    /// （`edge_type` / `graph` 拓扑与权重），`selected` / `editing_node` 是 `#[serde(skip)]` 运行态、
    /// 天然不参与比较。比较经 `serde_json` 在退出编辑那一帧做一次（用户节奏、非每帧），开销可忽略。
    ///
    /// `current` 由调用方在 `with_resource` 写闭包**之外**单独 `read_resource` 克隆（§3.1）。
    pub fn commit_staged_if_changed(&self, current: &Graph) {
        let mut inner = self.inner.lock().unwrap();
        let Some(before) = inner.staged.take() else {
            return;
        };
        if graph_data_differs(&before, current) {
            inner.push_undo(before);
        }
        // 未变：before 已被 take 丢弃，等价于 discard。
    }

    /// 丢弃暂存快照（操作被取消 / 图实际未变，如原地点击未拖动）。
    pub fn discard_staged(&self) {
        self.inner.lock().unwrap().staged = None;
    }

    /// 仅压入一份"变更前"快照（不跑变更）。用于"进入编辑 / 开始拖拽前"的预快照：把后续一连串
    /// `with_resource` 写入（编辑期文本改动 + 退出时的 resolve、或整段拖拽位移）**合并成一个撤销
    /// 单元**——它们不再各自 [`History::mutate`] 叠加快照。
    ///
    /// `before` 由调用方在 `with_resource` 写闭包**之外**单独 `read_resource` 克隆（§3.1）。
    pub fn record(&self, before: Graph) {
        self.inner.lock().unwrap().push_undo(before);
    }

    /// 关键接缝：在改图**之前**打一次快照，然后执行改图闭包。
    ///
    /// 调用顺序严格遵守 §3.1：
    /// 1. 在写闭包**之外**单独 `read_resource` 克隆出 `before`（绝不在写闭包内重入同资源读）；
    /// 2. 把 `before` 压入 undo、清空 redo；
    /// 3. 再 `with_resource` 跑真正的变更 `f`，返回其结果。
    ///
    /// 用于"一次性"图变更（建点 / 删点 / 删边 / 连边 / layout 等），每次一个独立撤销单元。
    pub fn mutate<R>(&self, graph_resource: &GraphResource, f: impl FnOnce(&mut Graph) -> R) -> R {
        // 1. 写闭包之外单独 read，克隆出"变更前"整图（§3.1：绝不在写闭包内重入读同资源）。
        let before = graph_resource.read_resource(|g| g.clone());
        // 2. 压栈 + 清空 redo。
        self.record(before);
        // 3. 跑真正的变更。
        graph_resource.with_resource(f)
    }

    /// 撤销一步：把当前图压入 redo，从 undo 弹出上一份快照整体替换回 [`GraphResource`]。
    ///
    /// 替换方式与 `app.rs` 的 Load 路径同构（整体换图）。替换整图后必须由调用方清运行态
    /// （`selected` / `editing_node`），见 [`replace_graph_in_place`]。返回是否真的撤销了一步。
    pub fn undo(&self, graph_resource: &GraphResource) -> bool {
        // 先在写闭包外取出要弹出的快照（同时把当前图克隆压 redo），避免在 with_resource 内重入读。
        let snapshot = {
            let mut inner = self.inner.lock().unwrap();
            match inner.undo.pop() {
                Some(prev) => prev,
                None => return false,
            }
        };
        // 当前图（撤销前）克隆压入 redo。在 with_resource 之外单独 read（§3.1）。
        let current = graph_resource.read_resource(|g| g.clone());
        self.inner.lock().unwrap().redo.push(current);
        replace_graph_in_place(graph_resource, snapshot);
        true
    }

    /// 重做一步：与 [`History::undo`] 对称——把当前图压入 undo，从 redo 弹出快照整体替换。
    /// 返回是否真的重做了一步。
    pub fn redo(&self, graph_resource: &GraphResource) -> bool {
        let snapshot = {
            let mut inner = self.inner.lock().unwrap();
            match inner.redo.pop() {
                Some(next) => next,
                None => return false,
            }
        };
        let current = graph_resource.read_resource(|g| g.clone());
        self.inner.lock().unwrap().undo.push(current);
        replace_graph_in_place(graph_resource, snapshot);
        true
    }
}

/// 把一份整图快照整体写回 [`GraphResource`]（与 Load 替换同构），并清空所有持旧 `NodeIndex` 的
/// 运行态——`selected` / `editing_node`。
///
/// 必要性（AGENTS.md §3.3）：替换整图后，原先选中/正在编辑的 `NodeIndex` 可能在新快照里已不存在，
/// 悬空索引若被后续渲染/命中以 `.unwrap()` 读取会 panic。节点几何（observer 写入 `ctx` temp data 的
/// `NodeRenderInfo`）由 `render_graph` 按新图重写；本函数的所有触发点（快捷键截获在 `app.rs::ui`
/// 顶部、Edit 菜单在 top panel）都**早于**画布 CentralPanel 的渲染，故替换发生在本帧几何读取之前，
/// 不会有"读到旧索引几何"的窗口。
/// 两张图的**持久化数据**是否不同（用于"暂存快照是否值得提交"判定）。
///
/// 以 `serde_json` 序列化后比较：只看真正进 `.cnt` 的字段（`edge_type` + `graph` 拓扑/权重），
/// `#[serde(skip)]` 的运行态（`selected` / `editing_node`）不参与。序列化失败（理论上不会）时
/// 保守地判为"不同"，宁可多留一个撤销项也不丢用户的改动。
fn graph_data_differs(a: &Graph, b: &Graph) -> bool {
    match (serde_json::to_string(a), serde_json::to_string(b)) {
        (Ok(sa), Ok(sb)) => sa != sb,
        _ => true,
    }
}

fn replace_graph_in_place(graph_resource: &GraphResource, snapshot: Graph) {
    graph_resource.with_resource(|g| {
        *g = snapshot;
        // 清运行态：避免悬空索引（§3.3）。这两个字段是 `#[serde(skip)]` 运行态，不随快照保存，
        // 故快照里它们已是默认值；这里再显式清一次，语义清晰、且兼容未来快照可能携带它们。
        g.selected.clear();
        g.editing_node = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::node::Node;

    fn node(text: &str) -> Node {
        Node {
            id: 0,
            position: egui::pos2(0.0, 0.0),
            text: text.to_owned(),
            note: String::new(),
            aliases: Vec::new(),
        }
    }

    fn graph_with(n: usize) -> GraphResource {
        let res = GraphResource::default();
        res.with_resource(|g| {
            for i in 0..n {
                g.add_node(node(&format!("n{i}")));
            }
        });
        res
    }

    fn node_count(res: &GraphResource) -> usize {
        res.read_resource(|g| g.graph.node_count())
    }

    #[test]
    fn empty_history_cannot_undo_or_redo() {
        let h = History::default();
        let res = graph_with(0);
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert!(!h.undo(&res), "空历史 undo 应无效");
        assert!(!h.redo(&res), "空历史 redo 应无效");
    }

    #[test]
    fn mutate_then_undo_restores_previous_graph() {
        let h = History::default();
        let res = graph_with(1);
        assert_eq!(node_count(&res), 1);

        // 一次变更：加一个节点。
        h.mutate(&res, |g| {
            g.add_node(node("added"));
        });
        assert_eq!(node_count(&res), 2);
        assert!(h.can_undo());
        assert!(!h.can_redo());

        // 撤销：回到 1 个节点。
        assert!(h.undo(&res));
        assert_eq!(node_count(&res), 1);
        assert!(!h.can_undo());
        assert!(h.can_redo());
    }

    #[test]
    fn redo_reapplies_undone_change() {
        let h = History::default();
        let res = graph_with(1);
        h.mutate(&res, |g| {
            g.add_node(node("added"));
        });
        assert!(h.undo(&res));
        assert_eq!(node_count(&res), 1);

        assert!(h.redo(&res));
        assert_eq!(node_count(&res), 2, "redo 应重新应用变更");
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn new_mutation_clears_redo_branch() {
        let h = History::default();
        let res = graph_with(1);
        h.mutate(&res, |g| {
            g.add_node(node("a"));
        });
        assert!(h.undo(&res));
        assert!(h.can_redo(), "撤销后应有 redo");

        // 撤销后再做新变更：redo 分支应被清空。
        h.mutate(&res, |g| {
            g.add_node(node("b"));
        });
        assert!(!h.can_redo(), "新变更应清空 redo 分支");
        assert!(h.can_undo());
    }

    #[test]
    fn undo_restores_deleted_node_with_stable_index() {
        let h = History::default();
        let res = graph_with(0);
        // 建两点一边，记录 b 的索引。
        let (a, b) = res.with_resource(|g| {
            let a = g.add_node(node("a"));
            let b = g.add_node(node("b"));
            g.add_edge(crate::graph::edge::Edge::new(
                a,
                b,
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
                crate::resource::CanvasStateResource::default(),
            ));
            (a, b)
        });
        assert_eq!(res.read_resource(|g| g.graph.edge_count()), 1);

        // 删除 b（连同其边）——经 history 打快照。
        h.mutate(&res, |g| g.remove_node(b));
        assert_eq!(node_count(&res), 1);
        assert_eq!(res.read_resource(|g| g.graph.edge_count()), 0);

        // 撤销：b 连同边复活，且 NodeIndex 不变（StableGraph 快照保索引稳定）。
        assert!(h.undo(&res));
        assert_eq!(node_count(&res), 2);
        assert_eq!(res.read_resource(|g| g.graph.edge_count()), 1, "边应复活");
        assert!(res.read_resource(|g| g.get_node(b).is_some()), "b 应复活");
        assert!(res.read_resource(|g| g.get_node(a).is_some()));
        assert!(res.read_resource(|g| g.edge_exists(a, b)), "a->b 边应复活");
    }

    #[test]
    fn cap_evicts_oldest_snapshots() {
        let h = History::with_cap(3);
        let res = graph_with(0);
        // 5 次变更，cap=3：只保留最近 3 步可撤销。
        for i in 0..5 {
            h.mutate(&res, |g| {
                g.add_node(node(&format!("n{i}")));
            });
        }
        assert_eq!(node_count(&res), 5);

        // 最多撤销 3 步（cap 限制）。
        let mut undone = 0;
        while h.undo(&res) {
            undone += 1;
            assert!(undone <= 3, "撤销步数不应超过 cap");
        }
        assert_eq!(undone, 3, "cap=3 应恰好能撤销 3 步");
        assert_eq!(
            node_count(&res),
            2,
            "最旧 2 步被丢弃，回退到第 3 步前的 2 个节点"
        );
    }

    #[test]
    fn record_then_undo_uses_presnapshot() {
        // record（预快照）语义：把一连串后续写入合并为一个撤销单元。
        let h = History::default();
        let res = graph_with(1);

        // 模拟"进入编辑/拖拽前"预快照：克隆 before（写闭包外 read），record。
        let before = res.read_resource(|g| g.clone());
        h.record(before);

        // 后续多次 with_resource 写入（编辑期 / 拖拽期），不再各自打快照。
        res.with_resource(|g| {
            g.add_node(node("x"));
        });
        res.with_resource(|g| {
            g.add_node(node("y"));
        });
        assert_eq!(node_count(&res), 3);

        // 一次撤销回到预快照那一刻（1 个节点），证明多次写入是一个撤销单元。
        assert!(h.undo(&res));
        assert_eq!(node_count(&res), 1);
    }

    #[test]
    fn undo_clears_selection_and_editing_runtime_state() {
        let h = History::default();
        let res = graph_with(2);
        let idx = res.read_resource(|g| g.graph.node_indices().next().unwrap());

        h.mutate(&res, |g| {
            g.add_node(node("added"));
        });
        // 在新图上设置运行态（选中 + 编辑），它们持有旧索引。
        res.with_resource(|g| {
            g.select_node(idx);
            g.set_editing_node(Some(idx));
        });

        // 撤销后运行态应被清空（避免悬空索引，§3.3）。
        assert!(h.undo(&res));
        assert!(res.read_resource(|g| g.get_selected_nodes().is_empty()));
        assert!(res.read_resource(|g| g.get_editing_node().is_none()));
    }
}
