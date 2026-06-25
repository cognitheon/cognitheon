//! 小地图 / 鸟瞰图（minimap）—— 右下角常驻的**只读展示性投影** + 点击平移交互。
//!
//! ## 这是什么
//! 把全图所有节点位置等比缩到右下角一个固定大小的小矩形里，每个节点画成一个小点/方块，
//! 再叠一个半透明矩形框表示"当前主画布看得见的范围"。在 minimap 内点击即把主画布平移到
//! 对应位置（**缩放不变**）。空图（无节点）时整体隐藏不画。
//!
//! ## 与 AGENTS.md §3.2 坐标系铁律的关系（最大风险点，务必看懂）
//! `CanvasState.transform` 是**画布 ↔ 屏幕**变换的唯一权威。minimap 用的是**另一套独立映射**
//! `mini()`（画布坐标 → minimap 局部屏幕坐标），它：
//! - **只用于 minimap 自身绘制**，绝不回写 `Node.position`、绝不持久化、绝不污染
//!   `CanvasState.transform`；
//! - 节点点位**直接读 `Node.position`**（画布坐标 §3.2），不走 observer / temp data，
//!   故无"依赖上一帧"的一帧延迟。
//!
//! 唯一会改 `CanvasState` 的地方是**点击平移**：把点击的 minimap 局部坐标经 `mini()` 的**逆**
//! 反投影回画布坐标，再**只改 `transform.translation`** 让该画布点落到主视口中心
//! （`translation = viewport_center - scaling * canvas_point`，**`scaling` 不变**）。
//! 死字段 `offset` / `scale`（§5）一概不碰。
//!
//! ## 投影公式
//! 设 `bbox` = 所有节点的画布坐标包围盒（[`crate::wikilink::minimap_bbox`]，退化时下方做
//! `max(1.0)` 兜底），`content` = minimap 内可用于绘制节点的矩形区域，
//! `s = min(content.w / bbox_w, content.h / bbox_h)`（等比，取较小者），并把缩略图在 content
//! 内**居中**（`off = content.center - s * bbox.center`）：
//! - 正投影 `mini(p)   = s * p + off`
//! - 逆投影 `unmini(m) = (m - off) / s`
//!
//! ## 交互门控（§3.4，不与画布状态机争输入）
//! minimap 用独立的 `egui::Area`（`Order::Foreground`）+ `allocate_rect(frame_rect, Sense::click())`
//! 占满外框读自身点击（复刻命令面板 / links_panel 的浮层范式）。**但**这并不足以挡住画布状态机：
//! egui 0.34 的 `Response::contains_pointer()` 只在另一层 widget**完全覆盖整块画布**时才把画布移
//! 出命中表，200x140 的 minimap 不覆盖整块画布，故同帧画布状态机仍会把这次点击当 `Canvas` 处理
//! （清选区 / 入框选）。真正的「点 minimap 只平移、不动选区」靠**状态机侧门控**实现：
//! `state_manager.rs` 的画布门控用 `Context::layer_id_at(指针位置)` 取**顶层 layer**——指针落在
//! minimap / 命令面板 / 帮助浮层等任何 `Order::Foreground` 的 `Area` 上时顶层非 Background，状态
//! 机当帧不把它当画布点击；而节点 / 空白画布属 egui 注册的整窗 `Background` Area，顶层 = Background
//! → 照常放行，故不误伤节点点击 / 框选 / 建点（详见 `state_manager.rs::update`）。
//!
//! **关键**：`layer_id_at` 取的是 Area 的状态 rect（`pivot_pos + 测量 size`），故 minimap 必须用
//! `allocate_rect`（推进 cursor、把 Area 测量尺寸撑到 frame_rect）而非裸 `interact`（纯命中、不推进
//! cursor、Area 测得近零尺寸）——否则指针落在 minimap 上时 `layer_id_at` 反而命中底下的 Background
//! 层、门控失效、点击穿透到画布。该不变量有无头实证守卫（见 `state_manager.rs` 的 `tests`）。
//! v1 只做"点击平移"；视口框拖动暂不做（取舍见模块末尾说明）。

use egui::{Id, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::colors;
use crate::resource::{CanvasStateResource, GraphResource};
use crate::wikilink::minimap_bbox;

/// minimap 外框尺寸（屏幕像素，含内边距）。
const MINIMAP_SIZE: Vec2 = Vec2::new(200.0, 140.0);
/// minimap 锚到画布右下角时离边缘的留白。
const MINIMAP_MARGIN: Vec2 = Vec2::new(12.0, 12.0);
/// minimap 内框（节点投影区）相对外框的内边距。
const MINIMAP_PADDING: f32 = 8.0;
/// 单个节点在 minimap 内画成的小方块半边长（屏幕像素）。
const NODE_DOT_HALF: f32 = 1.6;

/// minimap 的画布坐标 → minimap 局部屏幕坐标的**独立等比投影**（§3.2：与
/// `CanvasState.transform` 完全解耦，仅用于 minimap 自身绘制 / 交互反投影）。
#[derive(Clone, Copy, Debug)]
struct MiniProjection {
    /// 等比缩放系数 `s`。
    scale: f32,
    /// 平移偏移 `off`，使 `bbox` 中心落到 `content` 中心（居中）。
    offset: Vec2,
}

impl MiniProjection {
    /// 由全图包围盒 `bbox`（画布坐标）与 minimap 节点投影区 `content`（屏幕坐标）构造。
    ///
    /// 退化兜底（§3.2 重中之重）：单节点 / 共线时 `bbox` 宽或高为 0，用 `max(1.0)` 兜底，
    /// 避免 `s = content / 0 = Inf`、进而 NaN 污染后续所有坐标。
    fn new(bbox: Rect, content: Rect) -> Self {
        let bw = bbox.width().max(1.0);
        let bh = bbox.height().max(1.0);
        let scale = (content.width() / bw).min(content.height() / bh);
        // off = content.center - s * bbox.center（把包围盒中心对齐到 content 中心）。
        let offset = content.center().to_vec2() - scale * bbox.center().to_vec2();
        Self { scale, offset }
    }

    /// 正投影：画布坐标 → minimap 局部屏幕坐标。
    fn project(&self, canvas: Pos2) -> Pos2 {
        (self.scale * canvas.to_vec2() + self.offset).to_pos2()
    }

    /// 逆投影：minimap 局部屏幕坐标 → 画布坐标。`scale` 已 `max(1.0)` 兜底保证非 0。
    fn unproject(&self, mini: Pos2) -> Pos2 {
        ((mini.to_vec2() - self.offset) / self.scale).to_pos2()
    }
}

/// 渲染 minimap 并处理点击平移。**必须在 `render_graph` 之后调用**（z-order 在节点之上，§3.5）。
///
/// 参数 `canvas_screen_rect` = 主画布在屏幕上的实际区域（= `CanvasWidget` 分配到的
/// `screen_rect`，由 `app.rs` 从画布 `Response.rect` 取得）。minimap 锚到它的右下角，并据它
/// 反算"当前主视口对应的画布矩形"以画视口框。
///
/// §3.1：读节点位置经 [`GraphResource::read_resource`]、写 `transform` 经
/// [`CanvasStateResource::with_resource`]，闭包即锁作用域、不重入、不跨闭包持锁。
pub fn show_minimap(
    ctx: &egui::Context,
    canvas_screen_rect: Rect,
    graph_resource: &GraphResource,
    canvas_state_resource: &CanvasStateResource,
) {
    // 空图（无节点 → 无包围盒）：整体隐藏，不画。直接读 Node.position 算包围盒（§3.2，
    // 不依赖 observer / temp data，避一帧延迟）。
    let Some(bbox) = graph_resource.read_resource(minimap_bbox) else {
        return;
    };

    // minimap 外框：锚到画布区域右下角，向内留白。尺寸恒为 MINIMAP_SIZE（min = max - 固定尺寸），
    // 故宽高恒正、不会退化成负尺寸矩形；画布即便很小也只是外框探出画布左/上边，不影响绘制与命中。
    let max = canvas_screen_rect.max - MINIMAP_MARGIN;
    let min = max - MINIMAP_SIZE;
    let frame_rect = Rect::from_min_max(min, max);

    // 节点投影区 = 外框去内边距。
    let content = frame_rect.shrink(MINIMAP_PADDING);
    let proj = MiniProjection::new(bbox, content);

    // 主题色：集中到 colors.rs（与全项目 Light/Dark 取色一致，§5 不就地散落颜色）。
    let theme = ctx.theme();
    let bg = colors::minimap_bg(theme);
    let frame_stroke = Stroke::new(1.0, colors::minimap_frame(theme));
    let node_color = colors::minimap_node(theme);
    // 视口框：半透明描边 + 极淡填充，醒目但不挡住节点点。
    let viewport_stroke = Stroke::new(1.5, colors::minimap_viewport_stroke(theme));
    let viewport_fill = colors::minimap_viewport_fill(theme);

    // 节点点位（直接读 Node.position → project）。批量收集成 Shape 一次性 add（§性能，仿
    // draw_grid 批量绘制），大图每帧 N 点不逐个调用 painter。
    let node_screen_pts: Vec<Pos2> = graph_resource.read_resource(|g| {
        g.graph
            .node_indices()
            .map(|i| proj.project(g.graph[i].position))
            .collect()
    });

    // 当前主视口对应的画布矩形：主画布 screen_rect 经 to_canvas_rect 反算（§3.2 唯一权威），
    // 再经 mini() 投影成 minimap 内的小框。clamp 到 content 内，避免视口超出 minimap 边界时
    // 框画到外框之外。
    let viewport_canvas_rect =
        canvas_state_resource.read_resource(|cs| cs.to_canvas_rect(canvas_screen_rect));
    let vp_min = proj.project(viewport_canvas_rect.min);
    let vp_max = proj.project(viewport_canvas_rect.max);
    let viewport_mini_rect = Rect::from_min_max(vp_min, vp_max).intersect(frame_rect);

    // 独立 Area（Foreground，复刻命令面板浮层范式）：消费自身指针交互、不穿透到画布状态机。
    // movable(false) + 固定 fixed_pos 到外框左上角；allocate_rect 占满外框作为点击区域。
    let mut clicked_canvas: Option<Pos2> = None;
    egui::Area::new(Id::new("minimap_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(frame_rect.min)
        .movable(false)
        .interactable(true)
        .show(ctx, |ui| {
            // 限制绘制 / 命中在外框内。
            ui.set_clip_rect(frame_rect);

            // 用 allocate_rect 占满外框：既拿点击 Response，又让该 Area 的**测量尺寸**等于
            // frame_rect。这是状态机侧 `layer_id_at` 门控生效的前提——`layer_id_at` 取的是 Area
            // 的状态 rect（pivot_pos + 测量 size），若只 `interact`（纯命中、不推进 cursor）则 Area
            // 测得近零尺寸，指针落在 minimap 上时 `layer_id_at` 反而命中底下的 Background 层、门控
            // 失效（点击穿透）。先 allocate 占位再画，保证 minimap 区域被识别为 Foreground Area。
            let resp = ui.allocate_rect(frame_rect, Sense::click());
            if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    clicked_canvas = Some(proj.unproject(pos));
                }
            }

            let painter = ui.painter();

            // 背景 + 外框。
            painter.rect(
                frame_rect,
                egui::CornerRadius::same(4),
                bg,
                frame_stroke,
                StrokeKind::Inside,
            );

            // 批量画节点点（小方块）。
            if !node_screen_pts.is_empty() {
                let dot = Vec2::splat(NODE_DOT_HALF);
                let shapes: Vec<egui::Shape> = node_screen_pts
                    .iter()
                    .map(|p| {
                        egui::Shape::rect_filled(
                            Rect::from_min_max(*p - dot, *p + dot),
                            egui::CornerRadius::ZERO,
                            node_color,
                        )
                    })
                    .collect();
                painter.add(egui::Shape::Vec(shapes));
            }

            // 视口框（半透明填充 + 描边）。
            if viewport_mini_rect.width() > 0.0 && viewport_mini_rect.height() > 0.0 {
                painter.rect(
                    viewport_mini_rect,
                    egui::CornerRadius::ZERO,
                    viewport_fill,
                    viewport_stroke,
                    StrokeKind::Inside,
                );
            }
        });

    // 点击平移（§3.2 合规）：让点击对应的画布点落到主视口中心，**只改 transform.translation、
    // scaling 不变**（与 focus_node 同型公式 translation = center - scaling * canvas_point，
    // 但居中基准 rect 不同：focus_node 用 ctx.content_rect().center()，此处用 canvas_screen_rect
    // .center()——后者与上方视口框反算用的 canvas_screen_rect 同基准，保证「点哪儿就把哪儿挪到
    // 视口框中心」自洽）。
    if let Some(canvas_point) = clicked_canvas {
        let center = canvas_screen_rect.center();
        canvas_state_resource.with_resource(|cs| {
            let s = cs.transform.scaling;
            cs.transform.translation = center.to_vec2() - s * canvas_point.to_vec2();
        });
        log::debug!("minimap click -> pan to canvas {canvas_point:?} (scaling unchanged)");
    }
}
