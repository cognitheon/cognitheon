//! cognitheon 无头 stdio CLI —— 不依赖 GUI，按行命令驱动图模型，用于无头测试与脚本化调试。
//!
//! 协议：从 stdin 逐行读命令，结果写 stdout。每条响应以 `ok`/`err`/数据前缀开头，便于脚本断言。
//! 文本字段（title/body）里用字面量 `\n` 表示换行，输出时同样把换行转义回 `\n`，保证一行一记录。
//!
//! 注意：节点句柄 `<index>` 是 petgraph 拓扑索引（`new`/`dump` 返回）。删点后该索引成空洞、
//! 不复用，故 index 序列不连续——脚本不要假设 index 连续或可按相对位置推算。
//!
//! 命令一览（`help` 可在运行时打印）：
//!   new <x> <y> [title...]      新建节点，返回 `node <index> <id>`
//!   title <index> <title...>    设标题（Node.text）
//!   body <index> <body...>      设正文（Node.note），支持 `\n`
//!   alias <index> <names...>    设别名（Node.aliases），逗号分隔、trim、去空
//!   get <index>                 打印该节点 id/pos/title/body/aliases
//!   rm <index>                  删除节点
//!   count                       打印 `nodes <n> edges <m>`
//!   dump                        打印全部节点与边
//!   save <path>                 存为带版本号的 JSON，返回 `ok bytes <n>`
//!   load <path>                 从文件加载（兼容旧 .cnt），返回 `ok nodes <n> edges <m>`
//!   link <i> <j>                建一条 i->j 的裸边（不防自环/重复，区别于 parse 的双链）
//!   parse <i>                   解析节点 i 正文里的 [[标题]]，自动建/连节点（双链）
//!   backlinks <i>               列出指向节点 i 的反向链接（基于边）
//!   refs <i>                    反向引用 + 上下文原话（基于正文 [[标题]] 文本）
//!   search <query>              全文搜索（标题/正文）
//!   find <title>                按精确标题找节点，返回 `node <index>`
//!   orphans                     列出孤立节点（无任何边连接），返回 `orphan <index> <title>` + `ok count <n>`
//!   export-md <index>           打印某节点的 Markdown（frontmatter + note 原样，Obsidian 兼容）
//!   export-md                   （无参）打印整个 vault 的导出文件名清单 + 手画边旁路记录数
//!   reset                       清空
//!   help                        打印命令
//!   quit | exit                 退出

use std::io::{self, BufRead, Write};

use cognitheon::canvas::CanvasState;
use cognitheon::graph::edge::Edge;
use cognitheon::graph::graph_impl::Graph;
use cognitheon::graph::node::Node;
use cognitheon::resource::CanvasStateResource;
use cognitheon::{markdown, persistence, wikilink};
use petgraph::graph::NodeIndex;

struct Session {
    graph: Graph,
    canvas: CanvasStateResource,
}

impl Session {
    fn new() -> Self {
        Self {
            graph: Graph::default(),
            canvas: CanvasStateResource::new(CanvasState::default()),
        }
    }
}

/// 把字面量 `\n` 还原成真正的换行（输入侧）。
fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
}

/// 把换行转义成 `\n`，保证输出一行一记录（输出侧）。
fn escape(s: &str) -> String {
    s.replace('\n', "\\n")
}

/// 解析 CLI 用的节点句柄（petgraph 拓扑索引）。
fn parse_index(tok: &str) -> Result<NodeIndex, String> {
    tok.parse::<usize>()
        .map(NodeIndex::new)
        .map_err(|_| "bad index".to_owned())
}

/// 执行一条命令，返回要打印的若干行；`Ok(None)` 表示请求退出。
fn dispatch(s: &mut Session, line: &str) -> Result<Option<Vec<String>>, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();

    match cmd {
        "" => Ok(Some(vec![])),
        "quit" | "exit" => Ok(None),
        "help" => Ok(Some(
            [
                "commands: new <x> <y> [title] | title <i> <t> | body <i> <b> | alias <i> <names> | get <i>",
                "          rm <i> | count | dump | save <path> | load <path> | reset | quit",
                "          link <i> <j> | parse <i> | backlinks <i> | search <q> | find <title> | orphans",
                "          export-md [<i>]",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        )),

        "new" => {
            let mut it = rest.splitn(3, ' ');
            let x: f32 = it.next().unwrap_or("").parse().map_err(|_| "bad x")?;
            let y: f32 = it.next().unwrap_or("").parse().map_err(|_| "bad y")?;
            let title = unescape(it.next().unwrap_or(""));
            let id = s.canvas.read_resource(|c| c.new_node_id());
            let idx = s.graph.add_node(Node {
                id,
                position: egui::pos2(x, y),
                text: title,
                note: String::new(),
                aliases: Vec::new(),
            });
            Ok(Some(vec![format!("node {} {}", idx.index(), id)]))
        }

        "title" => {
            let mut it = rest.splitn(2, ' ');
            let idx = parse_index(it.next().unwrap_or(""))?;
            let title = unescape(it.next().unwrap_or(""));
            let node = s.graph.get_node_mut(idx).ok_or("no such node")?;
            node.text = title;
            Ok(Some(vec!["ok".into()]))
        }

        "body" => {
            let mut it = rest.splitn(2, ' ');
            let idx = parse_index(it.next().unwrap_or(""))?;
            let body = unescape(it.next().unwrap_or(""));
            let node = s.graph.get_node_mut(idx).ok_or("no such node")?;
            node.note = body;
            Ok(Some(vec!["ok".into()]))
        }

        "alias" => {
            let mut it = rest.splitn(2, ' ');
            let idx = parse_index(it.next().unwrap_or(""))?;
            // 逗号分隔、trim、去空 —— 与编辑态 UI 同口径。
            let aliases: Vec<String> = it
                .next()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let node = s.graph.get_node_mut(idx).ok_or("no such node")?;
            node.aliases = aliases;
            Ok(Some(vec![format!("ok aliases {}", node.aliases.len())]))
        }

        "get" => {
            let idx = parse_index(rest)?;
            let node = s.graph.get_node(idx).ok_or("no such node")?;
            Ok(Some(vec![
                format!("id {}", node.id),
                format!("pos {} {}", node.position.x, node.position.y),
                format!("title {}", escape(&node.text)),
                format!("body {}", escape(&node.note)),
                format!("aliases {}", escape(&node.aliases.join(", "))),
            ]))
        }

        "rm" => {
            let idx = parse_index(rest)?;
            if s.graph.get_node(idx).is_none() {
                return Err("no such node".into());
            }
            s.graph.remove_node(idx);
            Ok(Some(vec!["ok".into()]))
        }

        "count" => Ok(Some(vec![format!(
            "nodes {} edges {}",
            s.graph.graph.node_count(),
            s.graph.graph.edge_count()
        )])),

        "dump" => {
            let mut out = Vec::new();
            for idx in s.graph.graph.node_indices() {
                let n = &s.graph.graph[idx];
                out.push(format!(
                    "node {} id={} pos=({},{}) title={} body_len={}",
                    idx.index(),
                    n.id,
                    n.position.x,
                    n.position.y,
                    escape(&n.text),
                    n.note.chars().count()
                ));
            }
            for eidx in s.graph.graph.edge_indices() {
                if let Some((src, dst)) = s.graph.graph.edge_endpoints(eidx) {
                    out.push(format!(
                        "edge {} {}->{}",
                        eidx.index(),
                        src.index(),
                        dst.index()
                    ));
                }
            }
            Ok(Some(out))
        }

        "save" => {
            if rest.is_empty() {
                return Err("usage: save <path>".into());
            }
            let data = s
                .canvas
                .read_resource(|c| persistence::save_string(&s.graph, Some(c)))
                .map_err(|e| format!("serialize: {e}"))?;
            std::fs::write(rest, &data).map_err(|e| format!("write: {e}"))?;
            Ok(Some(vec![format!("ok bytes {}", data.len())]))
        }

        "load" => {
            if rest.is_empty() {
                return Err("usage: load <path>".into());
            }
            let data = std::fs::read(rest).map_err(|e| format!("read: {e}"))?;
            let doc = persistence::load(&data).map_err(|e| format!("parse: {e}"))?;
            let (graph, canvas) = doc.into_parts();
            s.graph = graph;
            s.canvas = CanvasStateResource::new(canvas);
            Ok(Some(vec![format!(
                "ok nodes {} edges {}",
                s.graph.graph.node_count(),
                s.graph.graph.edge_count()
            )]))
        }

        "reset" => {
            s.graph.reset();
            Ok(Some(vec!["ok".into()]))
        }

        "link" => {
            let mut it = rest.splitn(2, ' ');
            let src = parse_index(it.next().unwrap_or(""))?;
            let dst = parse_index(it.next().unwrap_or(""))?;
            let sp = s.graph.get_node(src).ok_or("no such src")?.position;
            let tp = s.graph.get_node(dst).ok_or("no such dst")?.position;
            s.graph
                .add_edge(Edge::new(src, dst, sp, tp, s.canvas.clone()));
            Ok(Some(vec![format!(
                "ok edges {}",
                s.graph.graph.edge_count()
            )]))
        }

        "parse" => {
            let idx = parse_index(rest)?;
            if s.graph.get_node(idx).is_none() {
                return Err("no such node".into());
            }
            let out = wikilink::resolve_links(&mut s.graph, &s.canvas, idx);
            let mut lines = vec![format!(
                "ok created_nodes {} created_edges {}",
                out.created_nodes.len(),
                out.created_edges
            )];
            for n in out.created_nodes {
                lines.push(format!(
                    "created {} {}",
                    n.index(),
                    escape(&s.graph.graph[n].text)
                ));
            }
            for a in &out.ambiguous {
                lines.push(format!("ambiguous {}", escape(a)));
            }
            Ok(Some(lines))
        }

        "backlinks" => {
            let idx = parse_index(rest)?;
            if s.graph.get_node(idx).is_none() {
                return Err("no such node".into());
            }
            let bs = wikilink::backlinks(&s.graph, idx);
            let mut lines: Vec<String> = bs
                .iter()
                .map(|b| format!("backlink {} {}", b.index(), escape(&s.graph.graph[*b].text)))
                .collect();
            lines.push(format!("ok count {}", bs.len()));
            Ok(Some(lines))
        }

        "refs" => {
            let idx = parse_index(rest)?;
            if s.graph.get_node(idx).is_none() {
                return Err("no such node".into());
            }
            let bls = wikilink::backlinks_with_context(&s.graph, idx);
            let mut lines = Vec::new();
            for bl in &bls {
                lines.push(format!("ref {} {}", bl.source.index(), escape(&bl.title)));
                for ctx in &bl.contexts {
                    lines.push(format!("  ctx {}", escape(ctx)));
                }
            }
            lines.push(format!("ok count {}", bls.len()));
            Ok(Some(lines))
        }

        "search" => {
            if rest.is_empty() {
                return Err("usage: search <query>".into());
            }
            let hits = wikilink::search(&s.graph, rest);
            let mut lines: Vec<String> = hits
                .iter()
                .map(|h| format!("hit {} {}", h.index(), escape(&s.graph.graph[*h].text)))
                .collect();
            lines.push(format!("ok count {}", hits.len()));
            Ok(Some(lines))
        }

        "find" => {
            if rest.is_empty() {
                return Err("usage: find <title>".into());
            }
            match wikilink::find_by_title(&s.graph, rest) {
                Some(idx) => Ok(Some(vec![format!("node {}", idx.index())])),
                None => Err("not found".into()),
            }
        }

        "orphans" => {
            let orphans = wikilink::orphan_nodes(&s.graph);
            let mut lines: Vec<String> = orphans
                .iter()
                .map(|o| format!("orphan {} {}", o.index(), escape(&s.graph.graph[*o].text)))
                .collect();
            lines.push(format!("ok count {}", orphans.len()));
            Ok(Some(lines))
        }

        // export-md：带 index 打印该节点的 Markdown（换行转义保证一行一记录，与 body/get 同口径）；
        // 无参时打印整个 vault 的文件清单（节点 .md + 手画边旁路），供无头验证往返保真 / 文件名去重。
        "export-md" => {
            if rest.is_empty() {
                // vault 模式：打印文件名清单（每行一条 `file <name> <bytes>`）+ 旁路边记录数。
                let files = markdown::vault_files(&s.graph);
                let mut lines: Vec<String> = files
                    .iter()
                    .map(|f| format!("file {} {}", f.filename, f.bytes.len()))
                    .collect();
                let manual = markdown::collect_manual_edges(&s.graph);
                lines.push(format!(
                    "ok files {} manual_edges {}",
                    files.len(),
                    manual.len()
                ));
                Ok(Some(lines))
            } else {
                // 单节点模式：打印该节点的 Markdown（换行转义成 \n，多行 frontmatter+正文压成多条记录行）。
                let idx = parse_index(rest)?;
                let node = s.graph.get_node(idx).ok_or("no such node")?;
                // 单节点无去重上下文：stem = sanitize 后的文件名 base（与 app.rs 单节点导出同口径）。
                let stem = markdown::sanitize_filename(&node.text, node.id);
                let md = markdown::node_to_markdown(node, &stem);
                // 逐行输出（保留 frontmatter 结构可读），最后一条收尾。
                let mut lines: Vec<String> =
                    md.lines().map(|l| format!("md {}", escape(l))).collect();
                lines.push(format!("ok bytes {}", md.len()));
                Ok(Some(lines))
            }
        }

        other => Err(format!("unknown command: {other}")),
    }
}

fn main() {
    let mut session = Session::new();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        match dispatch(&mut session, &line) {
            Ok(None) => break,
            Ok(Some(lines)) => {
                for l in lines {
                    let _ = writeln!(out, "{l}");
                }
            }
            Err(e) => {
                let _ = writeln!(out, "err {e}");
            }
        }
        let _ = out.flush();
    }
}
