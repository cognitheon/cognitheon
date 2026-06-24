# AGENTS.md — cognitheon

> 本文件是本项目**跨工具的工程准则单一来源**。任何 AI 代理（Claude Code / 其它）在本仓库工作前必读。
> Claude Code / OMC 专属的执行细则见 `CLAUDE.md`，它只补充、不重复本文件。

## 1. 项目是什么

cognitheon 是一个基于 **egui / eframe** 的 Rust **节点图编辑器**：在可无限缩放/平移的画布上创建、连接、操纵节点；支持 **Bezier 曲线**与**直线**两类边、框选/多选/拖拽、GPU 粒子特效；图可持久化为 `.cnt`（serde_json）文件，并经 eframe storage 自动保存会话。

**双 target**：同一份代码既编译为 **native 桌面应用**（`cargo run`），又编译为 **wasm / web** 应用（`trunk serve`，经 GitHub Pages 部署）。

**演进方向（代码里已埋的预留意图，尚未接线）**：
- `typetag 0.2` 已在依赖中声明但全仓无任何 `#[typetag]` —— 为未来**多态序列化**（trait object 化的边/锚点）预留。
- `src/gpu_render/bezier/mod.rs` 是空壳 —— **GPU 加速贝塞尔**的预留位；当前贝塞尔完全走 egui CPU painter。
- 边多态当前是 `enum EdgeType + match` 硬编码；`src/ui/edge_trait.rs` 的 trait 方案是**被废弃的草稿**，不是现成扩展点。

**身份说明**：项目、crate、应用结构体已统一命名为 `cognitheon` / `CognitheonApp`（本仓库由 `eframe_template` 模板演化而来，已完成改名；`fill_template.sh` / `fill_template.ps1` 是一次性模板初始化脚本，现已失效，勿再运行）。

## 2. 技术栈

- **语言 / 工具链**：Rust 1.96（edition 2021）。`rust-toolchain` 钉死 `channel = "1.96"`（刻意不带 patch 版本；egui/eframe 0.34 要求 Rust ≥ 1.92），自带 `rustfmt`/`clippy` + `wasm32-unknown-unknown` target。
- **UI / 框架**：egui / eframe `0.34`（`default-features = false`，显式启用 `accesskit` / `default_fonts` / `persistence` / `wayland` / `wgpu`）。**渲染后端写死 wgpu（禁用 glow）**，粒子系统强依赖此，切后端会编译断。注意 eframe 0.34 的 `App` 必需方法是 `fn ui(&mut self, &mut egui::Ui, &mut Frame)`（旧 `update` 已 deprecated）；面板用 `egui::Panel::top/bottom(...).show_inside(ui, …)`、菜单用 `egui::MenuBar::new().ui(…)`、关菜单用 `ui.close()`。
- **图数据**：`petgraph 0.8`（feature `serde-1`）。关键用 `stable_graph::StableGraph<Node, Edge>`——删点/删边后 `NodeIndex`/`EdgeIndex` 保持稳定，这是用索引做长期句柄的前提。
- **GPU**：`wgpu 29`（经 `eframe::egui_wgpu` 重导出，版本随 eframe 绑定）+ `bytemuck`（粒子 POD 数据）+ `assets/particle.wgsl`（粒子着色器）。
- **序列化**：`serde` / `serde_json`——`.cnt` 文件存档 + eframe storage 持久化；egui 开 `serde` feature 让 `Pos2`/`Rect`/`Vec2`/`TSTransform` 可序列化。
- **native 专属**：`env_logger`（日志）、`tokio`（文件 IO 的 `block_on`）、`rfd 0.17`（异步文件对话框）。⚠️ `tokio` 的 `net` 特性依赖 `mio`，**无法编译进 wasm32**——故 `tokio` 必须置于 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`，且 `CognitheonApp.runtime` 字段与 File 菜单 Save/Load 代码整体 `#[cfg(not(target_arch = "wasm32"))]` 门控。新增 native-only 依赖务必同此处理，否则 wasm/CI 编译断。
- **粒子随机**：`rand 0.10`（注意 `random_range` 已移到 `rand::RngExt` trait）。
- **wasm 专属**：`wasm-bindgen-futures`、`web-sys`、`getrandom 0.4`（需 `wasm_js` 后端，由 `.cargo/config.toml` 注入 `--cfg getrandom_backend="wasm_js"`）。
- **web 打包**：Trunk（`Trunk.toml` 设 `filehash = false`，产物固定名 `cognitheon.js` / `cognitheon_bg.wasm`，与 `assets/sw.js` 预缓存列表对应）。

## 3. 架构铁律（违反即返工）

> 这五条是本仓库的核心不变量。每条都附"违反后果"——它们不是风格偏好，是会导致死锁 / 坐标错位 / 运行 panic / CI 挂的硬约束。

### 3.1 统一资源合约：SSOT + `Arc<RwLock>` 闭包访问
所有**跨组件共享的可变状态只有三个真源**：`GraphResource` / `CanvasStateResource` / `ParticleSystemResource`，均为 `Resource<T> = Arc<RwLock<T>>` 的类型别名（`src/resource/`）。
- 访问**只能**经 `read_resource(|t| ...)`（只读）/ `with_resource(|t| ...)`（可写）两个闭包 API。**闭包作用域 = 锁作用域。**
- **禁止**持锁跨闭包；**禁止**在同一资源的闭包内再取同一把锁（`RwLock` 用 `.unwrap()` 解锁，重入即自死锁，锁中毒会二次 panic）。
- **没有全局单例**（`src/global.rs` 是空文件）。"全局态" = 被多处 `clone` 的同一个 `Arc`，一律靠**构造函数注入**逐层下传。`clone` 只克隆 Arc，共享同一真源。
- 违反后果：死锁卡死整个 UI 线程，或锁中毒后连环 panic。

### 3.2 坐标系单一变换源
- **数据层一律存画布坐标（canvas space）**：`Node.position`、所有 `Anchor.canvas_pos`、`NodeRenderInfo.canvas_rect`。**屏幕坐标永不进图、永不持久化。**
- 画布 ↔ 屏幕变换**只走** `CanvasState.transform`（`egui::emath::TSTransform`）的 `to_screen` / `to_canvas`（= `transform` / `transform.inverse()`）。`CanvasState` 是坐标变换的**唯一权威**。
- `CanvasState.offset` / `scale` 是**死字段**（变换全走 `transform`），禁止使用。
- 推论：拖拽 delta 必须 `÷ transform.scaling` 才是画布位移；绘制时半径/字号/箭头长度必须 `× scaling`；缩放须用锚点保持公式 `transform = transform * from_translation(p) * from_scaling(δ) * from_translation(-p)`（`p = transform.inverse() * mouse`），并 `scaling.clamp(0.1, 100.0)`。
- 违反后果：缩放/平移后元素全局错位，或拖动速度不跟手。

### 3.3 索引即句柄 + 节点几何 observer 旁路
- 上层（Widget / observer / selection）**只用 `petgraph` 的 `NodeIndex`/`EdgeIndex` 作引用**，靠 `StableGraph` 保证删除后索引稳定；**从不跨帧持有 `&Node`/`&Edge`**。
- `Node.id` / `Edge.id`（由 `CanvasState` 的 `AtomicU64` 计数器分配）是**另一套业务 id**，与拓扑索引解耦、**不可混用**。`ctx.data` 的 key 是 `node_index.index()`，不是 `node.id`。
- **节点几何不存进图**：`NodeWidget` 渲染末尾算出 `canvas_rect` → 经 `Vec<Arc<dyn NodeObserver>>`（本项目**唯一落地的 trait 多态点**）→ `NodeRenderObserver` 写入 egui `ctx` temp data（key = `Id::new(node_index.index().to_string())`）→ `EdgeWidget` / `hit_test` / `helpers` 反读。
- 这是节点与边/命中之间**唯一的几何耦合通道**，存在"依赖上一帧"的**一帧延迟**（`render_graph` 故意**先画边后画点**；本帧新增节点当帧 `hit_test` / 连边可能落空）。`helpers` 缺信息时 `.unwrap()` 会 panic。
- 违反后果：边短暂消失/错位，或读不到 render_info 时 panic。

### 3.4 输入是显式有限状态机
- 交互由 `src/input/state_manager.rs` 的 `InputStateManager` **唯一驱动**；`current_state`（`InputState` 枚举）任意时刻唯一。
- `update()` 每帧四步：更新 context → 判定 target → 处理一次性事件（状态**转换**）→ 处理持续事件（状态**内更新**）。一次性事件管转换、持续事件管更新，两条管线分离。
- 不变量：`is_busy()`（非 `Idle`）即占用输入（缩放/滚动只在 `Idle` 生效）；处理 `pointer.delta` 前必须 `handles_mouse_motion()` 为真；每个拖拽态在 `button_release` 必须 `finalize` 或回 `Idle`，不得泄漏到下一帧；`Escape` 能从任意态强制回 `Idle`。
- **改输入逻辑只改 `state_manager.rs`**。`src/ui/canvas/{input,input_detector,input_handlers}.rs` 与 `src/input/{events,detectors}.rs` 是**已被取代的旧实现/空壳**，禁止在其上加逻辑（见 §5 死代码雷区）。
- 违反后果：手势冲突、状态泄漏到下一帧、鼠标移动被吞。

### 3.5 GPU 渲染三段式契约（egui ↔ wgpu 桥接）
自定义 wgpu paint **必须三件套齐全**，否则 `callback_resources.get().unwrap()` 运行即 panic：
1. **启动期**在 `src/app.rs` 用 `cc.wgpu_render_state` 把资源 `insert` 进 `rs.renderer.write().callback_resources`（按类型做 key）；
2. **运行时** `ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, MyCallback))`；
3. 实现 `CallbackTrait::prepare`（CPU 更新 + upload GPU，拿 `&Queue`）与 `paint`（录 `render_pass`）。
- GPU 数据结构（如 `Particle`）必须 `#[repr(C)]` + `bytemuck::Pod`，内存布局与对应 `.wgsl` 的 struct **严格一致**（含 `_pad` 对齐）。
- 违反后果：资源未注册或类型/布局不匹配 → 运行即崩或渲染全错。

## 4. 工程方法约束（强制，违反即返工）

1. **职责 subagent 级隔离**：设计/架构、开发、测试/审查、研究等职责**永远由独立 subagent 承担**；**严禁同一 agent 既写代码又自审**。作者（authoring）与审查（review/verify）永远分属不同 lane、不同上下文。
2. **禁止打补丁式修复**：发现 bug/缺陷**必须从整体架构出发、从架构层面解决**，修复方案需说明它如何契合 §3 的架构铁律。严禁局部 hack / 临时补丁掩盖根因——尤其禁止绕过 `Resource` 直接 `.0.read()`、手写 `*scale+offset` 代替 `TSTransform`、在死代码区（§5）加逻辑、用 `println!` 当调试手段。
3. **勤查资料 + 最佳实践**：egui / eframe `0.34`、wgpu `29`、petgraph `0.8` 的 API **跨版本易变**，遇到不确定的用法**先查官方文档**（docs.rs 对应版本）再写。持续对齐 egui 生态最佳实践。
4. **双 target 可调试（native + wasm）**：见 §7。新调试输出**统一用 `log` 宏**（`log::debug!/info!/error!`），不要用 `println!`——`println!` 在 wasm 上不进浏览器 console。任何平台分支用 `#[cfg(target_arch = "wasm32")]` / `cfg!(...)`，native-only 路径（tokio 文件 IO、`rfd`、`env_logger`）与 wasm 路径都要顾及。
5. **远离死代码区**：本仓库有历史遗留的双实现/空壳（§5）。新代码一律走"当前生效"的实现；除非任务明确要求清理，否则只记录边界、不就地扩建。

## 5. 目录结构与关键路径

**当前生效的主干**：
- `src/resource/` —— 共享状态原语 `Resource<T> = Arc<RwLock<T>>` + 三个类型别名。所有跨组件状态的并发入口。
- `src/canvas.rs` —— `CanvasState`：坐标变换（`TSTransform`）与 `node/edge` 全局 id（`AtomicU64`）的**唯一权威**。
- `src/graph/` —— 图数据模型（SSOT）。`graph_impl.rs` 是核心 `Graph`（封装 `StableGraph`）+ 全部增删改查；其自由函数 `render_graph()` 是**每帧渲染入口**（先所有 `EdgeWidget` 后所有 `NodeWidget`，并给每个节点挂 `NodeRenderObserver`）。
- `src/input/state_manager.rs` —— **交互的唯一驱动**（输入状态机）。
- `src/ui/canvas/widget_impl.rs` —— `CanvasWidget` 主渲染**每帧编排**：`allocate(Sense::drag())` → `draw_grid` → `input_manager.update` → `temp_edge` → `render_graph` → `draw_particle_system`。**绘制顺序即 z-order**（网格最底、粒子最顶）。
- `src/ui/` —— 节点/边 widget。`edge.rs` 是边渲染分发中枢（按 `graph.edge_type` match 到 `BezierWidget`/`LineWidget`）；`node.rs` 渲染末尾发布 `NodeRenderInfo` 给 observers。
- `src/gpu_render/particle/` —— GPU 粒子旁路（CPU 模拟 + wgpu storage buffer，`PointList` 拓扑，shader 内手算 NDC 投影）。
- `src/geometry/mod.rs` —— 纯画布坐标几何工具（射线求交贴合节点边框、多重边偏移）。
- `src/colors.rs` —— 主题相关颜色（按 `egui::Theme` Light/Dark）。
- `src/app.rs` —— 应用外壳 `CognitheonApp`（eframe::App）+ 持久化 + 顶/底面板（含 EdgeType 下拉、调试 HUD）。
- `src/main.rs` —— 双 target 入口（native `run_native` / wasm `WebRunner`）。

**死代码 / 雷区（勿在其上加逻辑）**：
- 旧输入系统：`src/ui/canvas/{input,input_detector,input_handlers}.rs` + `src/input/{events,detectors}.rs`（已被 `state_manager.rs` 取代，主路径不调用）。
- `CanvasState.offset` / `scale` 死字段（变换走 `transform`）。
- `src/ui/edge_trait.rs`（被废弃的 trait 草稿，整文件注释）。
- `src/graph/anchor.rs` 顶层 `enum Anchor`（未派生 serde、几乎未用；真正生效的是 `LineAnchor` / `BezierAnchor`）。
- `Node.render_info` 字段（已注释，几何改走 observer + temp data）；`EdgeRenderInfo`（已定义、完全未用）。
- `InputState` 里 `Zooming` / `DraggingControlPoint` / `MovingSelection` 三个枚举分支已登记但**无进入路径**（占位）。

## 6. 扩展约束（怎么加东西）

- **新增一种边类型**：成本高且分散（是 `enum EdgeType + match`，非 trait）。须同改：① `src/graph/edge.rs` 的 `EdgeType` 枚举 + `Edge` 新增几何字段；② `src/ui/edge.rs` 的 `update_*_edge` 与渲染 match 两分支；③ `src/ui/temp_edge.rs` 的 match；④ `src/app.rs` 顶栏 `ComboBox` 选项；⑤ 新建 `XxxWidget`。注意 `edge_type` 是 `graph` 级**全局**字段（所有边同型），且每条 `Edge` 同时冗余存 `bezier_edge` + `line_edge` 两份几何、每帧被 `EdgeWidget` 重算回写。
- **新增 GPU 渲染效果**：必须复刻 §3.5 三段式，且资源在 `app.rs` 启动期 `insert`。`src/gpu_render/bezier/` 是空壳、不能照抄；可参考 `particle/` 完整实现。
- **新增交互模式**：① 在 `InputState` 加枚举分支 + 所需字段；② 若响应鼠标移动，登记进 `handles_mouse_motion()` 白名单（必要时 `is_dragging`）；③ 在 `handle_*_button_press`/`double_click` 里按 `InputTarget` 加进入该状态的 `transition_to`；④ 在 `handle_mouse_motion` 加状态内更新分支；⑤ 在 `handle_*_button_release` 加收尾/finalize；⑥ 在 `handle_state_specific_updates` 加每帧绘制/光标；⑦ 确认 `Escape` 能回 `Idle`（已自动覆盖）。优先复用上面三个占位状态。

## 7. 开发工作流与"验证才算完成"

### 环境准备
```bash
rustup target add wasm32-unknown-unknown   # rust-toolchain 已声明，rustup 通常自动装
cargo install trunk                         # 或下载预编译二进制
# Nix 用户：nix develop（flake.nix 提供 rust + trunk + GUI/wayland/x11 库）
```

### 日常命令
```bash
# native
cargo run                       # 调试运行桌面窗口
RUST_LOG=debug cargo run        # 带日志（env_logger 读 RUST_LOG，输出 stderr）
cargo run --release             # release（隐藏 Windows 控制台）

# wasm / web
trunk serve                     # 本地热重载（默认 http://127.0.0.1:8080）
trunk serve --open              # 同上并打开浏览器
trunk build --release           # web 发布构建到 dist/

# lint 修复
cargo fmt --all
cargo clippy --fix
```

### 提交前必跑（一键本地 CI）
```bash
bash check.sh
```
`check.sh` 等价于：① `cargo check --workspace --all-targets`（native）② `cargo check --workspace --all-features --lib --target wasm32-unknown-unknown`（wasm）③ `cargo fmt --all -- --check` ④ `cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::all` ⑤ `cargo test --workspace --all-targets --all-features` ⑥ `cargo test --workspace --doc` ⑦ `trunk build`。
CI（`.github/workflows/rust.yml`）全局 `RUSTFLAGS = -D warnings`，**任何 warning 都会让 CI 挂**。

### Definition of Done（完成判据）
1. 本地 `bash check.sh` **双 target 全绿**（native + wasm32 都过、零 warning）。
2. 交互/渲染类改动（egui 逻辑难以无头单测）：必须 `cargo run` **与** `trunk serve` 各跑一遍**目测确认**，以截图 / 底部状态栏读数为证——**不得仅凭"编译通过"声称完成**。
3. 触及数据结构/序列化的改动：旧 `.cnt` 存档与 eframe storage 能 `load` 不崩（持久化兼容）。
4. 由**独立的** verifier / code-reviewer 收集证据、给出结论；**不得由实现者自审**（见 §4.1）。

> **数据合约现状**（非承诺）：`.cnt` 是 `serde_json` 直接序列化整个 `CognitheonApp`，无 schema 版本号。`#[serde(default)]` 让"新增字段"对旧存档向后兼容，但**删字段 / 改 `EdgeType` 或 `Anchor` 枚举会破坏旧存档**。当前无 migration 机制——改数据结构时须自行评估兼容性。
