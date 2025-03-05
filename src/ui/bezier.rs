use egui::{CursorIcon, Id, PointerButton, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Widget};

use crate::{graph::anchor::BezierAnchor, resource::CanvasStateResource};

use super::helpers::draw_dashed_rect_with_offset;

// #[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
// pub struct Anchor {
//     pub canvas_pos: egui::Pos2,            // 锚点坐标
//     pub handle_in_canvas_pos: egui::Pos2,  // 进入方向控制柄
//     pub handle_out_canvas_pos: egui::Pos2, // 退出方向控制柄
//     pub is_smooth: bool,                   // 是否平滑锚点（控制柄对称）
//     pub selected: bool,                    // 是否被选中
// }

// impl Anchor {
//     // 创建平滑锚点（自动生成对称控制柄）
//     pub fn new_smooth(canvas_pos: egui::Pos2) -> Self {
//         let handle_offset = Vec2::new(30.0, 0.0); // 默认水平对称
//         Self {
//             canvas_pos,
//             handle_in_canvas_pos: canvas_pos - handle_offset,
//             handle_out_canvas_pos: canvas_pos + handle_offset,
//             is_smooth: true,
//             selected: false,
//         }
//     }

//     // 创建尖锐锚点（控制柄各自独立）
//     pub fn new_sharp(canvas_pos: egui::Pos2) -> Self {
//         let handle_offset_in = Vec2::new(-30.0, 0.0); // 默认水平
//         let handle_offset_out = Vec2::new(30.0, 0.0); // 默认水平
//         Self {
//             canvas_pos,
//             handle_in_canvas_pos: canvas_pos + handle_offset_in,
//             handle_out_canvas_pos: canvas_pos + handle_offset_out,
//             is_smooth: false,
//             selected: false,
//         }
//     }

//     pub fn with_handles(mut self, handle_in: egui::Pos2, handle_out: egui::Pos2) -> Self {
//         self.handle_in_canvas_pos = handle_in;
//         self.handle_out_canvas_pos = handle_out;
//         self
//     }

//     // 强制设为平滑锚点，并更新控制柄为对称状态
//     pub fn set_smooth(&mut self) {
//         self.is_smooth = true;
//         self.enforce_smooth();
//     }

//     // 强制设为尖锐锚点
//     pub fn set_sharp(&mut self) {
//         self.is_smooth = false;
//     }

//     // 强制更新控制柄为对称状态，保持平滑
//     pub fn enforce_smooth(&mut self) {
//         let in_vec = self.canvas_pos - self.handle_in_canvas_pos;
//         self.handle_out_canvas_pos = self.canvas_pos + in_vec;
//     }
// }

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BezierEdge {
    pub source_anchor: BezierAnchor,
    pub target_anchor: BezierAnchor,
    pub control_anchors: Vec<BezierAnchor>, // 控制锚点，可以有多个
}

impl BezierEdge {
    pub fn new(source_anchor: BezierAnchor, target_anchor: BezierAnchor) -> Self {
        Self {
            source_anchor,
            target_anchor,
            control_anchors: Vec::new(),
        }
    }

    pub fn with_control_anchors(mut self, control_anchors: Vec<BezierAnchor>) -> Self {
        self.control_anchors = control_anchors;
        self
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum DragType {
    None,
    Anchor,
    HandleIn,
    HandleOut,
    MoveBezier, // 拖拽整个贝塞尔曲线
}

#[derive(Clone, Debug)]
pub struct BezierWidget {
    pub edge: BezierEdge,
    pub canvas_state_resource: CanvasStateResource,
    pub dragging: DragType,
    pub dragging_anchor_index: Option<usize>, // 拖拽锚点索引
}

impl BezierWidget {
    pub fn new(edge: BezierEdge, canvas_state_resource: CanvasStateResource) -> Self {
        BezierWidget {
            edge,
            canvas_state_resource,
            dragging: DragType::None,
            dragging_anchor_index: None,
        }
    }

    pub fn bounding_rect(&self, samples: usize) -> Rect {
        // 收集曲线上的所有离散采样点
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;

        // 将源、目标以及中间控制锚点拼接在一起
        let full_anchors = std::iter::once(&self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter())
            .chain(std::iter::once(&self.edge.target_anchor))
            .collect::<Vec<_>>();

        if full_anchors.len() >= 2 {
            for i in 0..full_anchors.len() - 1 {
                let p0 = full_anchors[i].canvas_pos;
                let p1 = full_anchors[i].handle_out_canvas_pos;
                let p2 = full_anchors[i + 1].handle_in_canvas_pos;
                let p3 = full_anchors[i + 1].canvas_pos;

                // 在 [0, 1] 区间内均匀采样
                for step in 0..=samples {
                    let t = step as f32 / samples as f32;
                    let point = cubic_bezier(p0, p1, p2, p3, t);

                    min_x = min_x.min(point.x);
                    max_x = max_x.max(point.x);
                    min_y = min_y.min(point.y);
                    max_y = max_y.max(point.y);
                }
            }
        }

        let min_pos = egui::pos2(min_x, min_y);
        let max_pos = egui::pos2(max_x, max_y);
        Rect::from_min_max(min_pos, max_pos)
    }

    /// 检测鼠标位置命中的元素
    fn hit_test(&mut self, world_pos: Pos2) -> DragType {
        let hit_radius: f32 = 10.0
            * self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.transform.scaling);

        let full_anchors = std::iter::once(&self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter())
            .chain(std::iter::once(&self.edge.target_anchor))
            .collect::<Vec<_>>();

        // 检测控制柄 (HandleIn)
        for (i, anchor) in full_anchors.iter().enumerate() {
            // 跳过首锚点
            if i == 0 {
                continue;
            }
            let offset = world_pos - anchor.handle_in_canvas_pos;
            if offset.length() < hit_radius {
                self.dragging_anchor_index = Some(i);
                return DragType::HandleIn;
            }
        }
        // 检测控制柄 (HandleOut)
        for (i, anchor) in full_anchors.iter().enumerate() {
            // 跳过尾锚点
            if i == full_anchors.len() - 1 {
                continue;
            }

            let offset = world_pos - anchor.handle_out_canvas_pos;
            if offset.length() < hit_radius {
                self.dragging_anchor_index = Some(i);
                return DragType::HandleOut;
            }
        }

        // 检测锚点
        for (i, anchor) in full_anchors.iter().enumerate() {
            if i == 0 || i == full_anchors.len() - 1 {
                continue;
            }
            let offset = world_pos - anchor.canvas_pos;
            if offset.length() < hit_radius {
                self.dragging_anchor_index = Some(i);
                return DragType::Anchor;
            }
        }

        // 贝塞尔曲线路径检测 (包围盒粗略检测 + 采样点精细检测)
        let bounding_rect = self.bounding_rect(100); // 采样 100 个点用于包围盒计算
        if bounding_rect.contains(world_pos) {
            // 如果鼠标位置在包围盒内，进行更精确的采样点检测
            let mut curve_points = Vec::new();
            for i in 0..full_anchors.len() - 1 {
                let start_anchor = &full_anchors[i];
                let end_anchor = &full_anchors[i + 1];
                for t in 0..=100 {
                    let t = t as f32 / 100.0;
                    let point = cubic_bezier(
                        start_anchor.canvas_pos,
                        start_anchor.handle_out_canvas_pos,
                        end_anchor.handle_in_canvas_pos,
                        end_anchor.canvas_pos,
                        t,
                    );
                    curve_points.push(point);
                }
            }

            for point in &curve_points {
                if (world_pos - *point).length() < hit_radius {
                    return DragType::MoveBezier; // 命中贝塞尔曲线路径
                }
            }
        }

        DragType::None // 没有命中任何元素
    }

    pub fn apply_actions(&mut self, response: &egui::Response, ui: &mut Ui) {
        // println!("hover pos: {:?}", response.hover_pos());
        let mouse_canvas_pos = self.canvas_state_resource.read_resource(|canvas_state| {
            canvas_state.to_canvas(response.hover_pos().unwrap_or_default())
        });

        // 拖拽锚点
        if self.dragging == DragType::Anchor {
            if let Some(index) = self.dragging_anchor_index {
                self.drag_anchor(index, ui);
                if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
                    self.dragging = DragType::None;
                    self.dragging_anchor_index = None;
                }
                ui.ctx().request_repaint(); // 持续重绘
                return; // 提前返回，避免其他交互干扰
            }
        }
        // 拖拽控制柄 (HandleIn)
        if self.dragging == DragType::HandleIn {
            if let Some(index) = self.dragging_anchor_index {
                self.drag_handle_in(index, ui);
                if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
                    self.dragging = DragType::None;
                    self.dragging_anchor_index = None;
                }
                ui.ctx().request_repaint(); // 持续重绘
                return; // 提前返回，避免其他交互干扰
            }
        }
        // 拖拽控制柄 (HandleOut)
        if self.dragging == DragType::HandleOut {
            if let Some(index) = self.dragging_anchor_index {
                self.drag_handle_out(index, ui);
                if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
                    self.dragging = DragType::None;
                    self.dragging_anchor_index = None;
                }
                ui.ctx().request_repaint(); // 持续重绘
                return; // 提前返回，避免其他交互干扰
            }
        }
        // 拖拽贝塞尔曲线
        if self.dragging == DragType::MoveBezier {
            self.drag_bezier(ui);
            if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) {
                self.dragging = DragType::None;
            }
            ui.ctx().request_repaint(); // 持续重绘
            return; // 提前返回，避免其他交互干扰
        }

        if response.hovered() {
            // println!("response.hovered()");
            let drag_type = self.hit_test(mouse_canvas_pos);
            // println!("drag_type: {:?}", drag_type);
            match drag_type {
                DragType::Anchor
                | DragType::HandleIn
                | DragType::HandleOut
                | DragType::MoveBezier => {
                    // 根据不同的 DragType 设置不同的鼠标样式 (可选)
                    ui.output_mut(|o| o.cursor_icon = CursorIcon::Grab);
                }
                DragType::None => {
                    // 恢复默认鼠标样式 (可选)
                    // ui.output_mut(|o| o.cursor_icon = CursorIcon::Default);
                }
            }

            if response.double_clicked() {
                ui.ctx().request_repaint();
            }
            if response.drag_started() && drag_type != DragType::None {
                self.dragging = drag_type; // 开始拖拽
                ui.ctx().request_repaint();
            }
        }
    }

    fn drag_bezier(&mut self, ui: &mut Ui) {
        let delta = ui.input(|i| i.pointer.delta())
            / self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.transform.scaling);
        self.edge.source_anchor.canvas_pos += delta;
        self.edge.target_anchor.canvas_pos += delta;
        // 拖拽控制锚点
        for anchor in &mut self.edge.control_anchors {
            anchor.canvas_pos += delta;
        }
        // 拖拽控制柄
        self.edge.source_anchor.handle_in_canvas_pos += delta;
        self.edge.source_anchor.handle_out_canvas_pos += delta;
        self.edge.target_anchor.handle_in_canvas_pos += delta;
        self.edge.target_anchor.handle_out_canvas_pos += delta;
        // 控制锚点的控制柄也需要拖拽
        for anchor in &mut self.edge.control_anchors {
            anchor.handle_in_canvas_pos += delta;
            anchor.handle_out_canvas_pos += delta;
        }
    }

    fn drag_anchor(&mut self, index: usize, ui: &mut Ui) {
        let full_anchors = std::iter::once(&mut self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter_mut())
            .chain(std::iter::once(&mut self.edge.target_anchor));
        let mut all_anchors: Vec<&mut BezierAnchor> = full_anchors.collect();

        let anchor = &mut all_anchors[index];

        let delta = ui.input(|i| i.pointer.delta())
            / self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.transform.scaling);
        anchor.canvas_pos += delta;
        anchor.handle_in_canvas_pos += delta;
        anchor.handle_out_canvas_pos += delta;
        // println!("anchor: {:?}", anchor.canvas_pos);
        // 如果锚点是平滑状态，需要强制更新控制柄
        if anchor.is_smooth {
            anchor.enforce_smooth();
        }
    }

    fn drag_handle_in(&mut self, index: usize, ui: &mut Ui) {
        // println!("drag_handle_in: {:?}", index);
        let full_anchors = std::iter::once(&mut self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter_mut())
            .chain(std::iter::once(&mut self.edge.target_anchor));
        let mut all_anchors: Vec<&mut BezierAnchor> = full_anchors.collect();
        let anchor = &mut all_anchors[index];

        let delta = ui.input(|i| i.pointer.delta())
            / self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.transform.scaling);
        anchor.handle_in_canvas_pos += delta;

        if anchor.is_smooth {
            let mirror_delta =
                anchor.canvas_pos - (anchor.handle_in_canvas_pos - anchor.canvas_pos);
            anchor.handle_out_canvas_pos = mirror_delta;
        }
    }

    fn drag_handle_out(&mut self, index: usize, ui: &mut Ui) {
        // println!("drag_handle_out: {:?}", index);
        let full_anchors = std::iter::once(&mut self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter_mut())
            .chain(std::iter::once(&mut self.edge.target_anchor));
        let mut all_anchors: Vec<&mut BezierAnchor> = full_anchors.collect();
        let anchor = &mut all_anchors[index];

        let delta = ui.input(|i| i.pointer.delta())
            / self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.transform.scaling);
        anchor.handle_out_canvas_pos += delta;

        if anchor.is_smooth {
            let mirror_delta =
                anchor.canvas_pos - (anchor.handle_out_canvas_pos - anchor.canvas_pos);
            anchor.handle_in_canvas_pos = mirror_delta;
        }
    }

    fn draw_bezier(&self, ui: &mut Ui) {
        // println!("BezierWidget::draw_bezier");
        let painter = ui.painter();
        // let canvas_state_resource: CanvasStateResource =
        //     ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        // 将所有锚点合并
        let full_anchors = std::iter::once(&self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter())
            .chain(std::iter::once(&self.edge.target_anchor))
            .collect::<Vec<_>>();

        // 绘制所有锚点和控制柄
        for (i, anchor) in full_anchors.iter().enumerate() {
            let radius = 3.0
                * self
                    .canvas_state_resource
                    .read_resource(|canvas_state| canvas_state.transform.scaling);

            // let color = if anchor.selected {
            //     egui::Color32::GOLD
            // } else {
            //     egui::Color32::from_rgba_premultiplied(150, 150, 10, 200)
            // };
            let color = egui::Color32::GOLD;
            let circle_screen_pos = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_screen(anchor.canvas_pos));

            if i != 0 && i != full_anchors.len() - 1 {
                // 绘制锚点
                painter.circle(circle_screen_pos, radius, color, (1.0, egui::Color32::GOLD));
            }

            let handle_in_screen_pos = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_screen(anchor.handle_in_canvas_pos));
            let handle_out_screen_pos = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_screen(anchor.handle_out_canvas_pos));

            // 绘制控制柄线

            // 首点没有入控制柄
            if i != 0 {
                painter.line_segment(
                    [circle_screen_pos, handle_in_screen_pos],
                    (3.0, egui::Color32::LIGHT_BLUE),
                );
            }
            // 末点没有出控制柄
            if i != full_anchors.len() - 1 {
                painter.line_segment(
                    [circle_screen_pos, handle_out_screen_pos],
                    (3.0, egui::Color32::LIGHT_RED),
                );
            }

            // 绘制控制柄点
            if i != 0 {
                painter.circle(
                    handle_in_screen_pos,
                    radius,
                    egui::Color32::BLUE,
                    (3.0, egui::Color32::LIGHT_BLUE),
                );
            }
            if i != full_anchors.len() - 1 {
                painter.circle(
                    handle_out_screen_pos,
                    radius,
                    egui::Color32::RED,
                    (3.0, egui::Color32::LIGHT_RED),
                );
            }
        }

        // 绘制贝塞尔曲线路径

        if full_anchors.len() >= 2 {
            let mut path = Vec::new();
            for i in 0..full_anchors.len() - 1 {
                let canvas_start = full_anchors[i].canvas_pos;
                let screen_start = self
                    .canvas_state_resource
                    .read_resource(|canvas_state| canvas_state.to_screen(canvas_start));
                let canvas_end = full_anchors[i + 1].canvas_pos;
                let screen_end = self
                    .canvas_state_resource
                    .read_resource(|canvas_state| canvas_state.to_screen(canvas_end));
                let screen_cp1 = self.canvas_state_resource.read_resource(|canvas_state| {
                    canvas_state.to_screen(full_anchors[i].handle_out_canvas_pos)
                });
                let screen_cp2 = self.canvas_state_resource.read_resource(|canvas_state| {
                    canvas_state.to_screen(full_anchors[i + 1].handle_in_canvas_pos)
                });

                // 细分三次贝塞尔曲线为线段
                for t in 0..=100 {
                    let t = t as f32 / 100.0;
                    let point = cubic_bezier(screen_start, screen_cp1, screen_cp2, screen_end, t);
                    path.push(point);
                }
            }
            painter.add(Shape::line(path, Stroke::new(2.0, egui::Color32::GRAY)));
        }

        self.draw_arrow(painter);
        // self.draw_bounding_rect(painter);
    }

    // fn draw_bounding_rect(&self, painter: &egui::Painter) {
    //     // 绘制包围框
    //     let bounding_rect = self.bounding_rect(100);
    //     let screen_rect = self
    //         .canvas_state_resource
    //         .read_resource(|canvas_state| canvas_state.to_screen_rect(bounding_rect));
    //     painter.rect(
    //         screen_rect,
    //         0.0,
    //         egui::Color32::TRANSPARENT,
    //         Stroke::new(1.0, egui::Color32::ORANGE),
    //     );
    // }

    fn draw_arrow(&self, painter: &egui::Painter) {
        let scale = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.transform.scaling);

        // 箭头的原始长度（例如 10.0），再乘以缩放系数
        let arrow_length = 10.0 * scale;

        // 同样，你也可以让线条粗细随缩放变化
        let arrow_stroke_width = 2.0;

        // 3. 在整条曲线最后端画箭头
        //    由于 full_anchors 最后一项就是 target_anchor，让我们直接取它使用
        let target_anchor = &self.edge.target_anchor;
        let screen_end = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen(target_anchor.canvas_pos));

        // 这里假设你希望使用 (p3 - p2) （即 target_anchor.canvas_pos - target_anchor.handle_in_canvas_pos）
        // 来表示 t=1 处贝塞尔曲线的切线方向
        // 如果你想沿 handle_out，也可以换成 (target_anchor.handle_out_canvas_pos - target_anchor.canvas_pos)
        let world_dir = target_anchor.handle_in_canvas_pos - target_anchor.canvas_pos;
        // 避免极端情况
        if world_dir.length() > f32::EPSILON {
            let screen_dir_start = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_screen(target_anchor.canvas_pos));
            let screen_dir_end = self.canvas_state_resource.read_resource(|canvas_state| {
                canvas_state.to_screen(target_anchor.canvas_pos - world_dir)
            });
            let dir = screen_dir_end - screen_dir_start;

            // 箭头长度
            // let arrow_length = 10.0;

            // 旋转向量函数
            fn rotate(v: egui::Vec2, angle_rad: f32) -> egui::Vec2 {
                let (sin, cos) = angle_rad.sin_cos();
                egui::Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
            }

            // 箭头两条短线与切线的夹角（可自己调整角度）
            let angle = 30_f32.to_radians();
            let dir_norm = dir.normalized();
            let left_dir = rotate(dir_norm, angle) * arrow_length;
            let right_dir = rotate(dir_norm, -angle) * arrow_length;

            let arrow_stroke = (arrow_stroke_width, egui::Color32::GRAY);

            // 在目标端画两条短线
            painter.line_segment([screen_end, screen_end - left_dir], arrow_stroke);
            painter.line_segment([screen_end, screen_end - right_dir], arrow_stroke);
        }
    }

    pub fn draw_bounding_rect(&mut self, ui: &mut Ui) {
        let painter = ui.painter();
        let bounding_rect = self.bounding_rect(100);
        let screen_rect = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen_rect(bounding_rect));

        // 这里记录 offset 并且在每帧累加
        // 比如让它每秒增加 120 像素，“速度”可以自己调
        // let delta_time = ui.input(|i| i.stable_dt).min(0.1); // 稳定的一帧时间
        // let speed = 120.0; // 像素/秒

        // 每帧更新 offset
        let offset: f32 = ui
            .ctx()
            .data(|d| d.get_temp(Id::new("animation_offset")))
            .unwrap_or(0.0);

        // println!("offset: {:?}", offset);
        draw_dashed_rect_with_offset(
            painter,
            screen_rect,
            Stroke::new(1.0, egui::Color32::ORANGE),
            10.0,
            10.0,
            offset,
        );
        // painter.rect(
        //     screen_rect,
        //     0.0,
        //     egui::Color32::TRANSPARENT,
        //     Stroke::new(1.0, egui::Color32::ORANGE),
        // );
    }
}

impl Widget for &mut BezierWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        // println!("BezierWidget::ui: selected: {:?}", self.edge.selected);
        // let canvas_state_resource: CanvasStateResource =
        //     ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // println!(
        //     "anchors: {:?}",
        //     self.anchors.iter().map(|a| a.pos).collect::<Vec<_>>()
        // );
        // let (pos, desired_size) = self.desired_size();
        // let rect = Rect::from_min_size(pos, desired_size);
        let bounding_rect = self.bounding_rect(100);
        let screen_rect = self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.to_screen_rect(bounding_rect));

        let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

        self.draw_bezier(ui);
        self.draw_bounding_rect(ui);
        // ui.painter().rect(
        //     response.rect,
        //     0.0,
        //     egui::Color32::TRANSPARENT,
        //     Stroke::new(1.0, egui::Color32::ORANGE),
        // );
        self.apply_actions(&response, ui);
        response
    }
}

fn cubic_bezier(
    p0: egui::Pos2,
    p1: egui::Pos2,
    p2: egui::Pos2,
    p3: egui::Pos2,
    t: f32,
) -> egui::Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let u = 1.0 - t;
    let u2 = u * u;
    let u3 = u2 * u;
    egui::pos2(
        u3 * p0.x + 3.0 * u2 * t * p1.x + 3.0 * u * t2 * p2.x + t3 * p3.x,
        u3 * p0.y + 3.0 * u2 * t * p1.y + 3.0 * u * t2 * p2.y + t3 * p3.y,
    )
}
