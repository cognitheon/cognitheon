#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Node {
    pub id: u64,
    pub position: egui::Pos2,
    pub text: String,
    pub note: String,
    /// 节点别名（PKM aliases，同 Obsidian）：`[[别名]]` 与 `[[text]]` 同样寻址到本节点。
    ///
    /// 寻址收口在 [`crate::wikilink::find_by_title`]（text 优先、alias 次之），故别名在
    /// resolve_links / 读模式跳转 / 自动补全 / 反向链接 / 命令面板搜索全链路一致生效。
    ///
    /// `#[serde(default)]`：旧 `.cnt` / eframe storage（无 `aliases` 字段）反序列化为空 `Vec`，
    /// 照常 load 不崩（AGENTS.md §7 向后兼容，故**不** bump `SCHEMA_VERSION`）。
    #[serde(default)]
    pub aliases: Vec<String>,
    // pub render_info: Option<NodeRenderInfo>,
}
