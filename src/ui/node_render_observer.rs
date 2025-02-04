use egui::{Context, Id};
use petgraph::graph::NodeIndex;

use crate::graph::node::NodeRenderInfo;
use crate::graph::node_observer::NodeObserver;

/// 负责更新 context.data 中对应节点渲染信息的观察者
pub struct NodeRenderObserver {
    /// 这里持有一个 egui::Context 的引用，用以访问 data_mut 并写入临时数据
    pub ctx: Context,
}

impl NodeRenderObserver {
    pub fn new(ctx: Context) -> Self {
        Self { ctx }
    }
}

impl NodeObserver for NodeRenderObserver {
    fn on_node_changed(&self, node_index: NodeIndex, render_info: NodeRenderInfo) {
        // 当节点发生变化时，将渲染信息写入 egui context 的临时数据里
        self.ctx.data_mut(|d| {
            d.insert_temp(Id::new(node_index.index().to_string()), render_info);
        });
        // 也可以在此处做更多处理，例如发出事件、更新日志等
    }
}
