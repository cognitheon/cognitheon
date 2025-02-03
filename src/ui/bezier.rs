use egui::*;

use crate::globals::canvas_state_resource::CanvasStateResource;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Anchor {
    pub canvas_pos: egui::Pos2,            // 锚点坐标
    pub handle_in_canvas_pos: egui::Pos2,  // 进入方向控制柄
    pub handle_out_canvas_pos: egui::Pos2, // 退出方向控制柄
    pub is_smooth: bool,                   // 是否平滑锚点（控制柄对称）
    pub selected: bool,                    // 是否被选中
}

impl Anchor {
    // 创建平滑锚点（自动生成对称控制柄）
    pub fn new_smooth(canvas_pos: egui::Pos2) -> Self {
        let handle_offset = Vec2::new(30.0, 0.0); // 默认水平对称
        Self {
            canvas_pos,
            handle_in_canvas_pos: canvas_pos - handle_offset,
            handle_out_canvas_pos: canvas_pos + handle_offset,
            is_smooth: true,
            selected: false,
        }
    }

    // 创建带自定义控制柄的锚点（默认非平滑）
    pub fn with_handles(
        canvas_pos: egui::Pos2,
        handle_in: egui::Pos2,
        handle_out: egui::Pos2,
    ) -> Self {
        Self {
            canvas_pos,
            handle_in_canvas_pos: handle_in,
            handle_out_canvas_pos: handle_out,
            is_smooth: false, // 需要手动调用 set_smooth(true) 启用平滑
            selected: false,
        }
    }

    // 添加设置平滑状态的方法
    pub fn set_smooth(&mut self, is_smooth: bool) {
        self.is_smooth = is_smooth;
        if is_smooth {
            self.enforce_smooth();
        }
    }

    // 强制保持控制柄对称
    pub fn enforce_smooth(&mut self) {
        let delta_in = self.handle_in_canvas_pos - self.canvas_pos;
        let delta_out = self.handle_out_canvas_pos - self.canvas_pos;

        // 如果两个控制柄都非零，取平均值
        if delta_in != Vec2::ZERO && delta_out != Vec2::ZERO {
            let avg_dir = (delta_in.normalized() - delta_out.normalized()) / 2.0;
            self.handle_in_canvas_pos = self.canvas_pos + avg_dir * delta_in.length();
            self.handle_out_canvas_pos = self.canvas_pos - avg_dir * delta_out.length();
        }
        // 否则保持反向
        else {
            self.handle_out_canvas_pos = self.canvas_pos - delta_in;
            self.handle_in_canvas_pos = self.canvas_pos - delta_out;
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BezierEdge {
    pub source_anchor: Anchor,
    pub target_anchor: Anchor,
    pub control_anchors: Vec<Anchor>,
}

impl BezierEdge {
    pub fn new(source: Anchor, target: Anchor) -> Self {
        let base_offset: f32 = 30.0;
        // 根据首尾两个锚点的相对位置自动计算两个锚点的控制柄方向
        let dir = target.canvas_pos - source.canvas_pos;
        // 如果横向距离大于纵向距离，则认为方向为水平。
        // 首点的出向与尾点的入向相反。
        let is_horizontal = dir.x.abs() > dir.y.abs();
        let handle_offset = if is_horizontal {
            if dir.x > 0.0 {
                Vec2::new(base_offset, 0.0)
            } else {
                Vec2::new(-base_offset, 0.0)
            }
        } else {
            if dir.y > 0.0 {
                Vec2::new(0.0, base_offset)
            } else {
                Vec2::new(0.0, -base_offset)
            }
        };
        let source_handle_out = source.canvas_pos + handle_offset;
        let target_handle_in = target.canvas_pos - handle_offset;
        let new_source_anchor =
            Anchor::with_handles(source.canvas_pos, source_handle_out, source_handle_out);
        let new_target_anchor =
            Anchor::with_handles(target.canvas_pos, target_handle_in, target_handle_in);
        Self {
            source_anchor: new_source_anchor,
            target_anchor: new_target_anchor,
            control_anchors: vec![],
        }
    }

    pub fn new_with_anchors(source: Anchor, target: Anchor, control_anchors: Vec<Anchor>) -> Self {
        Self {
            source_anchor: source,
            target_anchor: target,
            control_anchors,
        }
    }

    pub fn update_control_anchors(&mut self, control_anchors: Vec<Anchor>) {
        self.control_anchors = control_anchors;
    }
}

pub struct BezierWidget {
    edge: BezierEdge,
    canvas_state_resource: CanvasStateResource,
    pub dragging: Option<DragType>, // 当前拖动的对象类型（锚点、控制柄）
    pub on_change: Option<Box<dyn Fn(BezierEdge)>>, // 当锚点发生变化时，回调函数
}

#[derive(Debug)]
pub enum DragType {
    Anchor(usize),    // 拖动的锚点索引
    HandleIn(usize),  // 拖动的进入控制柄索引
    HandleOut(usize), // 拖动的退出控制柄索引
}

impl Widget for BezierWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        // let canvas_state_resource: CanvasStateResource =
        //     ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // println!(
        //     "anchors: {:?}",
        //     self.anchors.iter().map(|a| a.pos).collect::<Vec<_>>()
        // );
        let (pos, desired_size) = self.desired_size();
        let rect = Rect::from_min_size(pos, desired_size);
        let screen_rect = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen_rect(rect));

        let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());
        self.draw_bezier(ui);
        // self.apply_actions(&response, ui);
        response
    }
}

impl BezierWidget {
    pub fn new(
        edge: BezierEdge,
        canvas_state_resource: CanvasStateResource,
        on_change: Option<Box<dyn Fn(BezierEdge)>>,
    ) -> Self {
        Self {
            edge,
            canvas_state_resource,
            dragging: None,
            on_change,
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

    fn desired_size(&self) -> (Pos2, Vec2) {
        // 用控制点计算边界，能够将所有控制点都包含在内的外接矩形

        let mut min = self.edge.source_anchor.canvas_pos;
        let mut max = self.edge.target_anchor.canvas_pos;
        for anchor in &self.edge.control_anchors {
            min.x = min.x.min(anchor.canvas_pos.x);
            min.y = min.y.min(anchor.canvas_pos.y);
            max.x = max.x.max(anchor.canvas_pos.x);
            max.y = max.y.max(anchor.canvas_pos.y);
        }
        // let min = self
        //     .anchors
        //     .iter()
        //     .min_by(|a, b| {
        //         a.pos
        //             .x
        //             .partial_cmp(&b.pos.x)
        //             .unwrap()
        //             .then(a.pos.y.partial_cmp(&b.pos.y).unwrap())
        //     })
        //     .unwrap();
        // let max = self
        //     .anchors
        //     .iter()
        //     .max_by(|a, b| {
        //         a.pos
        //             .x
        //             .partial_cmp(&b.pos.x)
        //             .unwrap()
        //             .then(a.pos.y.partial_cmp(&b.pos.y).unwrap())
        //     })
        //     .unwrap();
        (min, Vec2::new(max.x - min.x, max.y - min.y))
    }

    // fn apply_actions(&mut self, response: &egui::Response, ui: &mut Ui) {
    //     // 获取鼠标位置（屏幕坐标）
    //     let mouse_screen_pos = match ui.ctx().input(|i| i.pointer.hover_pos()) {
    //         Some(p) => p,
    //         None => return,
    //     };

    //     // println!("mouse_screen_pos: {:?}", mouse_screen_pos);
    //     // 转换为世界坐标
    //     let mouse_canvas_pos = self
    //         .canvas_state_resource
    //         .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //     // println!("world_pos: {:?}", world_pos);
    //     // println!("response rect: {:?}", response.rect);
    //     if response.drag_started() {
    //         println!("bezier drag begin");
    //         if let Some((drag_type, _)) = self.hit_test(mouse_canvas_pos) {
    //             println!("bezier drag begin: {:?}", drag_type);
    //             self.dragging = Some(drag_type);
    //         }
    //     }

    //     // 处理拖动开始
    //     // if response.clicked() {
    //     //     if let Some((drag_type, _)) = self.hit_test(world_pos) {
    //     //         println!("drag begin: {:?}", drag_type);
    //     //         self.dragging = Some(drag_type);
    //     //     }
    //     // }

    //     // 处理持续拖动
    //     if response.dragged() {
    //         println!("bezier dragging: {:?}", self.dragging);
    //         if let Some(drag_type) = &self.dragging {
    //             println!("drag_type: {:?}", drag_type);
    //             match drag_type {
    //                 DragType::Anchor(index) => self.drag_anchor(*index, ui),
    //                 DragType::HandleIn(index) => self.drag_handle_in(*index, ui),
    //                 DragType::HandleOut(index) => self.drag_handle_out(*index, ui),
    //             }
    //         }
    //     }

    //     if response.drag_stopped() {
    //         println!("dragging released");
    //         self.dragging = None;
    //     }

    //     // 处理拖动结束
    //     // if response.clicked() {
    //     //     println!("dragging released");
    //     //     self.dragging = None;
    //     // }
    // }

    // /// 检测鼠标位置命中的元素
    // fn hit_test(&self, world_pos: Pos2) -> Option<(DragType, usize)> {
    //     let hit_radius: f32 = 20.0
    //         * self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.scale);

    //     let all_anchors = std::iter::once(&self.edge.source_anchor)
    //         .chain(self.edge.control_anchors.iter())
    //         .chain(std::iter::once(&self.edge.target_anchor))
    //         .collect::<Vec<_>>();

    //     // 优先检测控制柄
    //     for (i, anchor) in all_anchors.iter().enumerate() {
    //         if (world_pos - anchor.handle_in_canvas_pos).length() < hit_radius {
    //             return Some((DragType::HandleIn(i), i));
    //         }
    //         if (world_pos - anchor.handle_out_canvas_pos).length() < hit_radius {
    //             return Some((DragType::HandleOut(i), i));
    //         }
    //     }

    //     // 检测锚点
    //     for (i, anchor) in all_anchors.iter().enumerate() {
    //         let offset = world_pos - anchor.canvas_pos;
    //         if offset.length() < hit_radius {
    //             println!("hit anchor: {:?}", i);
    //             return Some((DragType::Anchor(i), i));
    //         }
    //     }

    //     None
    // }

    // fn drag_anchor(&mut self, index: usize, ui: &mut Ui) {
    //     let anchor = &mut self.edge.control_anchors[index];
    //     // println!("drag_anchor: {:?}", index);
    //     let delta = ui.input(|i| i.pointer.delta())
    //         / self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.scale);
    //     println!("delta: {:?}", delta);
    //     anchor.canvas_pos += delta;
    //     anchor.handle_in_canvas_pos += delta;
    //     anchor.handle_out_canvas_pos += delta;
    //     println!("anchor: {:?}", anchor.canvas_pos);
    //     // 如果锚点是平滑状态，需要强制更新控制柄
    //     if anchor.is_smooth {
    //         anchor.enforce_smooth();
    //     }
    // }

    // fn drag_handle_in(&mut self, index: usize, ui: &mut Ui) {
    //     println!("drag_handle_in: {:?}", index);
    //     let anchor = &mut self.edge.control_anchors[index];
    //     let delta = ui.input(|i| i.pointer.delta())
    //         / self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.scale);
    //     anchor.handle_in_canvas_pos += delta;

    //     if anchor.is_smooth {
    //         let mirror_delta =
    //             anchor.canvas_pos - (anchor.handle_in_canvas_pos - anchor.canvas_pos);
    //         anchor.handle_out_canvas_pos = mirror_delta;
    //     }
    // }

    // fn drag_handle_out(&mut self, index: usize, ui: &mut Ui) {
    //     println!("drag_handle_out: {:?}", index);
    //     let anchor = &mut self.edge.control_anchors[index];
    //     let delta = ui.input(|i| i.pointer.delta())
    //         / self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.scale);
    //     anchor.handle_out_canvas_pos += delta;

    //     if anchor.is_smooth {
    //         let mirror_delta =
    //             anchor.canvas_pos - (anchor.handle_out_canvas_pos - anchor.canvas_pos);
    //         anchor.handle_in_canvas_pos = mirror_delta;
    //     }
    // }

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
                    .read_canvas_state(|canvas_state| canvas_state.transform.scaling);

            let color = if anchor.selected {
                egui::Color32::GOLD
            } else {
                egui::Color32::from_rgba_premultiplied(150, 150, 10, 200)
            };
            let circle_screen_pos = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_screen(anchor.canvas_pos));

            if i != 0 && i != full_anchors.len() - 1 {
                // 绘制锚点
                painter.circle(circle_screen_pos, radius, color, (1.0, egui::Color32::GOLD));
            }

            let handle_in_screen_pos =
                self.canvas_state_resource
                    .read_canvas_state(|canvas_state| {
                        canvas_state.to_screen(anchor.handle_in_canvas_pos)
                    });
            let handle_out_screen_pos =
                self.canvas_state_resource
                    .read_canvas_state(|canvas_state| {
                        canvas_state.to_screen(anchor.handle_out_canvas_pos)
                    });

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
                    .read_canvas_state(|canvas_state| canvas_state.to_screen(canvas_start));
                let canvas_end = full_anchors[i + 1].canvas_pos;
                let screen_end = self
                    .canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.to_screen(canvas_end));
                let screen_cp1 = self
                    .canvas_state_resource
                    .read_canvas_state(|canvas_state| {
                        canvas_state.to_screen(full_anchors[i].handle_out_canvas_pos)
                    });
                let screen_cp2 = self
                    .canvas_state_resource
                    .read_canvas_state(|canvas_state| {
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
    //         .read_canvas_state(|canvas_state| canvas_state.to_screen_rect(bounding_rect));
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
            .read_canvas_state(|canvas_state| canvas_state.transform.scaling);

        // 箭头的原始长度（例如 10.0），再乘以缩放系数
        let arrow_length = 10.0 * scale;

        // 同样，你也可以让线条粗细随缩放变化
        let arrow_stroke_width = 2.0;

        // 3. 在整条曲线最后端画箭头
        //    由于 full_anchors 最后一项就是 target_anchor，让我们直接取它使用
        let target_anchor = &self.edge.target_anchor;
        let screen_end = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen(target_anchor.canvas_pos));

        // 这里假设你希望使用 (p3 - p2) （即 target_anchor.canvas_pos - target_anchor.handle_in_canvas_pos）
        // 来表示 t=1 处贝塞尔曲线的切线方向
        // 如果你想沿 handle_out，也可以换成 (target_anchor.handle_out_canvas_pos - target_anchor.canvas_pos)
        let world_dir = target_anchor.handle_in_canvas_pos - target_anchor.canvas_pos;
        // 避免极端情况
        if world_dir.length() > f32::EPSILON {
            let screen_dir_start = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_screen(target_anchor.canvas_pos));
            let screen_dir_end = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| {
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
