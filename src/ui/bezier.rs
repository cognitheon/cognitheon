use egui::*;

use crate::global::CanvasStateResource;

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
        let handle_offset = Vec2::new(100.0, 0.0); // 默认水平对称
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
        Self {
            source_anchor: source,
            target_anchor: target,
            control_anchors: vec![],
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
    fn ui(mut self, ui: &mut Ui) -> Response {
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

    fn apply_actions(&mut self, response: &egui::Response, ui: &mut Ui) {
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // println!("apply_actions: {:?}", self.dragging);

        // 获取鼠标位置（屏幕坐标）
        let screen_pos = match ui.ctx().input(|i| i.pointer.interact_pos()) {
            Some(p) => p,
            None => return,
        };
        // 转换为世界坐标
        let world_pos = canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_canvas(screen_pos));
        // println!("world_pos: {:?}", world_pos);
        if response.drag_started() {
            println!("drag begin");
            if let Some((drag_type, _)) = self.hit_test(world_pos, ui) {
                println!("drag begin: {:?}", drag_type);
                self.dragging = Some(drag_type);
            }
        }

        // 处理拖动开始
        // if response.clicked() {
        //     if let Some((drag_type, _)) = self.hit_test(world_pos) {
        //         println!("drag begin: {:?}", drag_type);
        //         self.dragging = Some(drag_type);
        //     }
        // }

        // 处理持续拖动
        if response.dragged() {
            println!("dragging: {:?}", self.dragging);
            if let Some(drag_type) = &self.dragging {
                println!("drag_type: {:?}", drag_type);
                match drag_type {
                    DragType::Anchor(index) => self.drag_anchor(*index, ui),
                    DragType::HandleIn(index) => self.drag_handle_in(*index, ui),
                    DragType::HandleOut(index) => self.drag_handle_out(*index, ui),
                }
            }
        }

        if response.drag_stopped() {
            println!("dragging released");
            self.dragging = None;
        }

        // 处理拖动结束
        // if response.clicked() {
        //     println!("dragging released");
        //     self.dragging = None;
        // }
    }

    /// 检测鼠标位置命中的元素
    fn hit_test(&self, world_pos: Pos2, ui: &mut Ui) -> Option<(DragType, usize)> {
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        let hit_radius: f32 =
            10.0 * canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);

        // 优先检测控制柄
        for (i, anchor) in self.edge.control_anchors.iter().enumerate() {
            if (world_pos - anchor.handle_in_canvas_pos).length() < hit_radius {
                return Some((DragType::HandleIn(i), i));
            }
            if (world_pos - anchor.handle_out_canvas_pos).length() < hit_radius {
                return Some((DragType::HandleOut(i), i));
            }
        }

        // 检测锚点
        for (i, anchor) in self.edge.control_anchors.iter().enumerate() {
            let offset = world_pos - anchor.canvas_pos;
            if offset.length() < hit_radius {
                println!("hit anchor: {:?}", i);
                return Some((DragType::Anchor(i), i));
            }
        }

        None
    }

    fn drag_anchor(&mut self, index: usize, ui: &mut Ui) {
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // println!("drag_anchor: {:?}", index);
        let anchor = &mut self.edge.control_anchors[index];
        let delta = ui.input(|i| i.pointer.delta())
            / canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);
        println!("delta: {:?}", delta);
        anchor.canvas_pos += delta;
        anchor.handle_in_canvas_pos += delta;
        anchor.handle_out_canvas_pos += delta;
        println!("anchor: {:?}", anchor.canvas_pos);
        // 如果锚点是平滑状态，需要强制更新控制柄
        if anchor.is_smooth {
            anchor.enforce_smooth();
        }
    }

    fn drag_handle_in(&mut self, index: usize, ui: &mut Ui) {
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        let anchor = &mut self.edge.control_anchors[index];
        let delta = ui.input(|i| i.pointer.delta())
            / canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);
        anchor.handle_in_canvas_pos += delta;

        if anchor.is_smooth {
            let mirror_delta =
                anchor.canvas_pos - (anchor.handle_in_canvas_pos - anchor.canvas_pos);
            anchor.handle_out_canvas_pos = mirror_delta;
        }
    }

    fn drag_handle_out(&mut self, index: usize, ui: &mut Ui) {
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        let anchor = &mut self.edge.control_anchors[index];
        let delta = ui.input(|i| i.pointer.delta())
            / canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);
        anchor.handle_out_canvas_pos += delta;

        if anchor.is_smooth {
            let mirror_delta =
                anchor.canvas_pos - (anchor.handle_out_canvas_pos - anchor.canvas_pos);
            anchor.handle_in_canvas_pos = mirror_delta;
        }
    }

    fn draw_bezier(&self, ui: &mut Ui) {
        println!("BezierWidget::draw_bezier");
        let painter = ui.painter();
        // let canvas_state_resource: CanvasStateResource =
        //     ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // 绘制所有锚点和控制柄
        for anchor in &self.edge.control_anchors {
            // let (screen_pos, screen_handle_in, screen_handle_out, radius) = canvas_state_resource
            //     .read_canvas_state(|canvas_state| {
            //         (
            //             canvas_state.to_screen(anchor.pos),
            //             canvas_state.to_screen(anchor.handle_in),
            //             canvas_state.to_screen(anchor.handle_out),
            //             3.0 * canvas_state.scale,
            //         )
            //     });
            // println!("screen_pos: {:?}", screen_pos);
            let radius = 3.0
                * self
                    .canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.transform.scaling);

            // let screen_pos = anchor.pos;
            // let screen_handle_in = anchor.handle_in;
            // let screen_handle_out = anchor.handle_out;
            // let radius =
            //     3.0 * canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);

            let color = if anchor.selected {
                egui::Color32::GOLD
            } else {
                egui::Color32::from_rgba_premultiplied(150, 150, 10, 200)
            };
            let circle_screen_pos = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_screen(anchor.canvas_pos));
            // 绘制锚点
            painter.circle(circle_screen_pos, radius, color, (1.0, egui::Color32::GOLD));

            // 绘制控制柄线
            painter.line_segment(
                [anchor.canvas_pos, anchor.handle_in_canvas_pos],
                (3.0, egui::Color32::LIGHT_BLUE),
            );
            painter.line_segment(
                [anchor.canvas_pos, anchor.handle_out_canvas_pos],
                (3.0, egui::Color32::LIGHT_RED),
            );

            // 绘制控制柄点
            painter.circle(
                anchor.handle_in_canvas_pos,
                radius,
                egui::Color32::BLUE,
                (3.0, egui::Color32::LIGHT_BLUE),
            );
            painter.circle(
                anchor.handle_out_canvas_pos,
                radius,
                egui::Color32::RED,
                (3.0, egui::Color32::LIGHT_RED),
            );
        }

        // 绘制贝塞尔曲线路径
        // 将所有锚点合并
        let full_anchors = std::iter::once(&self.edge.source_anchor)
            .chain(self.edge.control_anchors.iter())
            .chain(std::iter::once(&self.edge.target_anchor))
            .collect::<Vec<_>>();

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

        // 绘制包围框
        let bounding_rect = self.bounding_rect(100);
        let screen_rect = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_screen_rect(bounding_rect));
        painter.rect(
            screen_rect,
            0.0,
            egui::Color32::TRANSPARENT,
            Stroke::new(1.0, egui::Color32::ORANGE),
        );
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
