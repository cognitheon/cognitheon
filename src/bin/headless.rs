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
//!   get <index>                 打印该节点 id/pos/title/body
//!   rm <index>                  删除节点
//!   count                       打印 `nodes <n> edges <m>`
//!   dump                        打印全部节点与边
//!   save <path>                 存为带版本号的 JSON，返回 `ok bytes <n>`
//!   load <path>                 从文件加载（兼容旧 .cnt），返回 `ok nodes <n> edges <m>`
//!   reset                       清空
//!   help                        打印命令
//!   quit | exit                 退出

use std::io::{self, BufRead, Write};

use cognitheon::canvas::CanvasState;
use cognitheon::graph::graph_impl::Graph;
use cognitheon::graph::node::Node;
use cognitheon::persistence;
use petgraph::graph::NodeIndex;

struct Session {
    graph: Graph,
    canvas: CanvasState,
}

impl Session {
    fn new() -> Self {
        Self {
            graph: Graph::default(),
            canvas: CanvasState::default(),
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
                "commands: new <x> <y> [title] | title <i> <t> | body <i> <b> | get <i>",
                "          rm <i> | count | dump | save <path> | load <path> | reset | quit",
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
            let id = s.canvas.new_node_id();
            let idx = s.graph.add_node(Node {
                id,
                position: egui::pos2(x, y),
                text: title,
                note: String::new(),
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

        "get" => {
            let idx = parse_index(rest)?;
            let node = s.graph.get_node(idx).ok_or("no such node")?;
            Ok(Some(vec![
                format!("id {}", node.id),
                format!("pos {} {}", node.position.x, node.position.y),
                format!("title {}", escape(&node.text)),
                format!("body {}", escape(&node.note)),
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
            let data = persistence::save_string(&s.graph, Some(&s.canvas))
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
            s.canvas = canvas;
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
