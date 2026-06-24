//! 持久化层：带 schema 版本号的文档格式 + 向后兼容旧 `.cnt` 的加载与迁移。
//!
//! 设计要点：
//! - 保存用 [`DocumentRef`]（持引用，零拷贝）——`CanvasState` 含 `AtomicU64` 不可 `Clone`，
//!   故不能先构造一个 owned 文档再序列化，只能序列化引用。
//! - 加载用 [`Document`]（owned）。先按是否含顶层 `graph` 键判别新/旧格式，再分别解析
//!   （历史上 `.cnt` 是整个 `CognitheonApp` 的 serde_json，顶层是 `graph_resource`/`canvas_resource`）。
//! - [`SCHEMA_VERSION`] 是当前格式版本；[`migrate`] 是未来结构演进的迁移接缝——任何会改变
//!   `Graph`/`Node`/`Edge` 序列化布局的演进都应 `SCHEMA_VERSION += 1` 并在此加一条升级分支。
//! - 比当前程序更新的档案会被 [`load`] 显式拒绝（[`LoadError::TooNew`]），而非静默读出半损坏的图。

use std::sync::atomic::Ordering;

use crate::canvas::CanvasState;
use crate::graph::graph_impl::Graph;

/// 当前持久化 schema 版本。
pub const SCHEMA_VERSION: u32 = 1;

/// 保存用的文档视图（持引用，避免 clone）。
#[derive(serde::Serialize)]
struct DocumentRef<'a> {
    version: u32,
    graph: &'a Graph,
    #[serde(skip_serializing_if = "Option::is_none")]
    canvas: Option<&'a CanvasState>,
}

/// 加载得到的文档（owned）。
#[derive(serde::Deserialize, Debug)]
pub struct Document {
    /// 读出来的 schema 版本；旧档（无版本字段）记为 0，经 [`migrate`] 后贴成 [`SCHEMA_VERSION`]。
    #[serde(default)]
    pub version: u32,
    pub graph: Graph,
    /// 画布视图（变换 + id 计数器）。可选——纯数据导出可以不带，见 [`Document::into_parts`]。
    #[serde(default)]
    pub canvas: Option<CanvasState>,
}

impl Document {
    /// 拆成 `(图, 画布)`。
    ///
    /// 若文档未携带画布（纯数据导出 / 手写档），合成一个默认画布，并把 id 计数器推到现有
    /// 节点、边 id 的最大值 +1，避免后续 `new_node_id()` / `new_edge_id()` 与已加载的 id 撞车。
    pub fn into_parts(self) -> (Graph, CanvasState) {
        let graph = self.graph;
        let canvas = self.canvas.unwrap_or_else(|| {
            let cs = CanvasState::default();
            if let Some(max_id) = graph.graph.node_weights().map(|n| n.id).max() {
                cs.global_node_id
                    .store(max_id.saturating_add(1), Ordering::Relaxed);
            }
            if let Some(max_id) = graph.graph.edge_weights().map(|e| e.id).max() {
                cs.global_edge_id
                    .store(max_id.saturating_add(1), Ordering::Relaxed);
            }
            cs
        });
        (graph, canvas)
    }
}

/// 加载错误：要么是底层 JSON 解析失败，要么是档案版本比本程序支持的更新。
#[derive(Debug)]
pub enum LoadError {
    /// JSON 解析失败（畸形数据 / 字段不匹配）。
    Parse(serde_json::Error),
    /// 档案 schema 版本高于本程序支持的版本。
    TooNew { found: u32, supported: u32 },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Parse(e) => write!(f, "解析失败: {e}"),
            LoadError::TooNew { found, supported } => write!(
                f,
                "档案 schema 版本 {found} 高于本程序支持的 {supported}，请升级 cognitheon",
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Parse(e) => Some(e),
            LoadError::TooNew { .. } => None,
        }
    }
}

/// 把图（及可选画布视图）序列化为带版本号的、人类可读的 JSON 字符串（开放格式，便于 git/外部工具读取）。
pub fn save_string(
    graph: &Graph,
    canvas: Option<&CanvasState>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&DocumentRef {
        version: SCHEMA_VERSION,
        graph,
        canvas,
    })
}

/// 从字节加载文档：先按是否含顶层 `graph` 键判别新/旧格式，分别解析，再统一校验版本并迁移。
///
/// 判别后**只**报对应格式解析器的错误（不会把一个损坏的新档误报成"缺少 graph_resource"）。
pub fn load(data: &[u8]) -> Result<Document, LoadError> {
    let value: serde_json::Value = serde_json::from_slice(data).map_err(LoadError::Parse)?;

    // 新版：{ "version": N, "graph": {...}, "canvas": ... }。旧 `.cnt` 顶层是 `graph_resource`，无 `graph`。
    if value.get("graph").is_some() {
        let doc: Document = serde_json::from_value(value).map_err(LoadError::Parse)?;
        return finish(doc);
    }

    // 旧版：整个 CognitheonApp 的 serde_json，含 `graph_resource` / `canvas_resource`
    // （`Resource<T>` 序列化透传内层 T，故这两个字段就是 `Graph` / `CanvasState` 的 JSON）。
    #[derive(serde::Deserialize)]
    struct Legacy {
        graph_resource: Graph,
        #[serde(default)]
        canvas_resource: Option<CanvasState>,
    }
    let legacy: Legacy = serde_json::from_value(value).map_err(LoadError::Parse)?;
    finish(Document {
        version: 0,
        graph: legacy.graph_resource,
        canvas: legacy.canvas_resource,
    })
}

/// 校验版本（拒绝过新档）后跑迁移。
fn finish(doc: Document) -> Result<Document, LoadError> {
    if doc.version > SCHEMA_VERSION {
        return Err(LoadError::TooNew {
            found: doc.version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(migrate(doc))
}

/// 迁移接缝：把任意旧版本文档升级到当前 [`SCHEMA_VERSION`]。
///
/// 目前只有 v1，迁移即贴版本号。未来新增字段/改枚举时，在这里按 `doc.version` 逐级转换：
/// ```ignore
/// while doc.version < SCHEMA_VERSION {
///     match doc.version {
///         0 | 1 => { /* v1 -> v2 的字段补默认/重命名 */ doc.version = 2; }
///         _ => break,
///     }
/// }
/// ```
fn migrate(mut doc: Document) -> Document {
    doc.version = SCHEMA_VERSION;
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::Edge;
    use crate::graph::node::Node;
    use crate::resource::CanvasStateResource;

    fn node(id: u64, x: f32, y: f32, title: &str, body: &str) -> Node {
        Node {
            id,
            position: egui::pos2(x, y),
            text: title.to_owned(),
            note: body.to_owned(),
        }
    }

    fn sample_graph() -> Graph {
        let mut g = Graph::default();
        g.add_node(node(1, 10.0, 20.0, "A", "正文 A"));
        g.add_node(node(2, 30.0, 40.0, "B", ""));
        g
    }

    #[test]
    fn save_emits_version_and_roundtrips() {
        let g = sample_graph();
        let s = save_string(&g, None).unwrap();
        assert!(s.contains("\"version\""), "导出必须带 version 字段");
        let doc = load(s.as_bytes()).unwrap();
        assert_eq!(doc.version, SCHEMA_VERSION);
        assert_eq!(doc.graph.graph.node_count(), 2);
        let titles: Vec<String> = doc
            .graph
            .graph
            .node_weights()
            .map(|n| n.text.clone())
            .collect();
        assert!(titles.contains(&"A".to_string()) && titles.contains(&"B".to_string()));
    }

    #[test]
    fn empty_graph_roundtrips() {
        let s = save_string(&Graph::default(), None).unwrap();
        let doc = load(s.as_bytes()).unwrap();
        assert_eq!(doc.graph.graph.node_count(), 0);
        assert_eq!(doc.graph.graph.edge_count(), 0);
    }

    #[test]
    fn graph_with_edge_roundtrips() {
        let mut g = sample_graph();
        let idxs: Vec<_> = g.graph.node_indices().collect();
        let canvas = CanvasStateResource::new(CanvasState::default());
        g.add_edge(Edge::new(
            idxs[0],
            idxs[1],
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 1.0),
            canvas,
        ));
        let s = save_string(&g, None).unwrap();
        let doc = load(s.as_bytes()).unwrap();
        assert_eq!(doc.graph.graph.edge_count(), 1, "边应往返保真");
    }

    #[test]
    fn canvas_counters_roundtrip() {
        let g = sample_graph();
        let canvas = CanvasState::default();
        let _ = canvas.new_node_id(); // 0
        let _ = canvas.new_node_id(); // 1，计数器推进到 2
        let s = save_string(&g, Some(&canvas)).unwrap();
        let (_, restored) = load(s.as_bytes()).unwrap().into_parts();
        assert_eq!(restored.new_node_id(), 2, "id 计数器应保真");
    }

    #[test]
    fn into_parts_synthesizes_safe_counter_when_canvas_absent() {
        // 节点 id 为 5 和 9，但文档不带 canvas → 合成画布计数器应推到 10。
        let mut g = Graph::default();
        g.add_node(node(5, 0.0, 0.0, "a", ""));
        g.add_node(node(9, 0.0, 0.0, "b", ""));
        let s = save_string(&g, None).unwrap(); // canvas = None
        let (_, canvas) = load(s.as_bytes()).unwrap().into_parts();
        assert_eq!(canvas.new_node_id(), 10, "应推进到 max(id)+1，避免撞车");
    }

    #[test]
    fn loads_legacy_cnt_without_version() {
        // 构造一个旧 .cnt 形态：{ "label": ..., "graph_resource": <Graph>, "canvas_resource": null }
        let g = sample_graph();
        let graph_json = serde_json::to_string(&g).unwrap();
        let legacy = format!(
            "{{\"label\":\"hi\",\"graph_resource\":{graph_json},\"canvas_resource\":null}}"
        );
        let doc = load(legacy.as_bytes()).unwrap();
        assert_eq!(doc.graph.graph.node_count(), 2, "旧档应可加载");
        assert_eq!(
            doc.version, SCHEMA_VERSION,
            "旧档加载后应被迁移贴上当前版本"
        );
    }

    #[test]
    fn rejects_too_new_version() {
        let g = sample_graph();
        let bumped =
            save_string(&g, None)
                .unwrap()
                .replacen("\"version\": 1", "\"version\": 999", 1);
        match load(bumped.as_bytes()) {
            Err(LoadError::TooNew { found, .. }) => assert_eq!(found, 999),
            other => panic!("应拒绝过新版本, 实际: {other:?}"),
        }
    }

    #[test]
    fn load_rejects_garbage() {
        assert!(load(b"not json at all").is_err());
    }
}
