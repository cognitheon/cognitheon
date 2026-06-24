# CLAUDE.md — cognitheon

> **`AGENTS.md` 是工程准则单一来源，先读它。** 本文件只补充 Claude Code / OMC 专属的执行细节，不重复 AGENTS.md 的架构铁律（§3）与扩展约束（§6）。

## 对话语言
与用户对话默认使用**简体中文**。代码、标识符、技术术语保持原文。

## 职责 subagent 级隔离 —— OMC 角色映射（强制）

AGENTS.md §4.1 要求职责永远 subagent 级隔离、严禁一个 agent 同时做多项。在 Claude Code / OMC 下映射为：

| 职责 | 委派给 | 模型建议 |
|---|---|---|
| 设计 / 架构 | `architect`、`planner` | opus |
| 研究 / 查文档 / 探索代码 | `document-specialist`、`explore`、`general-purpose` | sonnet/haiku |
| 开发 / 实现 | `executor` | 复杂任务 opus，常规 sonnet |
| 测试 / 审查 / 验证 | `code-reviewer`、`verifier` | opus（架构级/大改） |

铁律：
- **实现者绝不自审**：写代码的 `executor` 与审查的 `code-reviewer`/`verifier` 必须是不同 subagent、不同上下文。
- 主线程（orchestrator）负责编排与集成，**不亲自承担上述任一专职职责**（规划/设计类文档与 `CLAUDE.md`/`AGENTS.md`/spec 的直接编写除外）。
- 多个独立任务并行委派（同一消息内多 Agent 调用）。

## 禁止打补丁式修复（强制）

遇到 bug / 失败，先委派 `debugger` / 系统化调试定位**根因**。修复方案必须从整体架构出发、说明如何契合 AGENTS.md §3 的架构铁律；禁止局部 hack / 绕过 / 临时补丁。本项目最常见的 hack 反例：绕过 `Resource` 直接 `.0.read()`、手写 `*scale+offset` 代替 `TSTransform`、在死代码区（AGENTS.md §5）加逻辑、用 `println!` 当调试。

## 勤查资料 + 最佳实践（强制）

egui / eframe `0.34`、wgpu `29`、petgraph `0.8` 的 API **跨版本易变**。不确定的用法 → 先经 `document-specialist` 查官方文档（**docs.rs 对应版本号**，仓库内代码优先参考；其次 Context Hub / `chub`，再 web 兜底）再实现。

## `log` 优先，禁止 `println!` 调试（强制）

新调试输出统一用 `log` 宏（`log::debug!/info!/error!`），**不要新增 `println!`**——`println!` 在 wasm 上不进浏览器 console。日志两端各自接管：native = `env_logger`（`RUST_LOG=debug cargo run`，输出 stderr）；wasm = `eframe::WebLogger`（重定向到浏览器 console）。

## 双 target 可调试（native + wasm，强制）

验证 / 调试经下列手段，**不依赖人工盯屏**：

- **native**：`RUST_LOG=debug cargo run` 看 egui/wgpu/app 日志；底部状态栏（`bottom_panel`）实时显示 `zoom / offset / input_state（状态机当前态）/ fps`，是首选的免日志可观测面板；`main.rs` 的 `on_surface_error` 回调是 wgpu 层错误的唯一可见点。
- **wasm**：`trunk serve`（默认 `http://127.0.0.1:8080`）+ 浏览器 **DevTools Console** 看 `WebLogger` 输出与 panic；启动诊断锚点 = `index.html` 的 `#loading_text` spinner（启动成功由 `main.rs` 移除，不消失即启动失败/卡住）。坑：`assets/sw.js` 缓存优先，开发看到旧 wasm 时用 URL 带 `#dev` 禁用 SW 或 `Ctrl+F5` 强刷；`getrandom` 的 `wasm_js` 后端由 `.cargo/config.toml` 自动注入；canvas id `the_canvas_id` 在 `main.rs` 与 `index.html` 两处硬编码必须一致。
- **隐式状态总线**：调试边/命中异常先查 egui `ctx` temp data 这几个 key——`Id::new(node_index.index().to_string())`→`NodeRenderInfo`、`Id::new("animation_offset")`→`f32`、`Id::new("input_busy")`→`bool`。

构建 / wasm 检查 / 测试用 `run_in_background`，避免阻塞。

## 验证才算完成

声明"完成/修复/通过"前，必须由**独立** `verifier`/`code-reviewer` 收集证据（见 AGENTS.md §7 的 Definition of Done）：本地 `bash check.sh` **双 target 全绿、零 warning**；交互/渲染类改动须 `cargo run` 与 `trunk serve` 各跑一遍目测取证；触及序列化的改动须确认旧 `.cnt` / storage 能 `load` 不崩。失败则继续迭代。
