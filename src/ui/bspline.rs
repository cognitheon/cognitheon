use egui::{Color32, Id, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget};

use crate::canvas::CanvasState;

pub struct BsplineWidget<'a> {
    pub control_points: Vec<Pos2>,
    pub canvas_rect: Rect,
    pub canvas_state: &'a mut CanvasState,
}

impl<'a> BsplineWidget<'a> {
    pub fn new(
        control_points: Vec<Pos2>,
        canvas_rect: Rect,
        canvas_state: &'a mut CanvasState,
    ) -> Self {
        Self {
            control_points,
            canvas_rect,
            canvas_state,
        }
    }

    fn desired_size(&self) -> Vec2 {
        // 用控制点计算边界
        let min = self
            .control_points
            .iter()
            .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap())
            .unwrap();
        let max = self
            .control_points
            .iter()
            .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap())
            .unwrap();
        // self.canvas_state
        //     .to_screen_vec2(Vec2::new(max.x - min.x, max.y - min.y))
        Vec2::new(max.x - min.x, max.y - min.y)
    }
}

impl<'a> Widget for BsplineWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = self.desired_size();
        // println!("desired_size: {:?}", desired_size);
        let screen_pos = self.canvas_state.to_screen(self.control_points[0]);
        let rect = Rect::from_min_size(screen_pos, desired_size);
        let response = ui.allocate_rect(rect, Sense::drag());

        if ui.is_rect_visible(rect) {
            // println!("rect: {:?}", rect);
            let painter = ui.painter();
            let stroke = Stroke::new(4.0, Color32::RED);
            for (i, &pos) in self.control_points.iter().enumerate() {
                let center = rect.min + Vec2::new(pos.x, pos.y);
                let color = Color32::RED;
                painter.circle_filled(center, 4.0, color);

                if i > 0 {
                    let prev = rect.min
                        + Vec2::new(self.control_points[i - 1].x, self.control_points[i - 1].y);
                    painter.line_segment([prev, center], stroke);
                }
            }

            // 生成B样条曲线
            if self.control_points.len() >= 4 {
                let degree = 3;
                let knots: Vec<f32> = (0..=self.control_points.len() + degree)
                    .map(|i| i as f32)
                    .collect();

                let t_range = knots[degree]..=knots[self.control_points.len()];
                let steps = 100;
                let mut path = Vec::with_capacity(steps);

                for step in 0..=steps {
                    let t = t_range.start()
                        + (t_range.end() - t_range.start()) * (step as f32 / steps as f32);
                    if let Some(point) = de_boor_algorithm(&self.control_points, t, degree, &knots)
                    {
                        path.push(self.canvas_state.to_screen(point));
                    }
                }

                painter.add(Shape::line(path, stroke));
            }
        }
        response
    }
}

pub fn de_boor_algorithm(
    control_points: &[Pos2],
    t: f32,
    degree: usize,
    knots: &[f32],
) -> Option<Pos2> {
    let n = control_points.len();
    if n <= degree || knots.len() != n + degree + 1 {
        return None;
    }

    // 找到t所在的节点区间
    let k = knots.partition_point(|&x| x <= t).saturating_sub(1);
    let k = k.clamp(degree, n - 1);

    let mut points = control_points[(k - degree)..=k]
        .iter()
        .map(|p| Vec2::new(p.x, p.y))
        .collect::<Vec<_>>();

    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = k - degree + j;
            let alpha = (t - knots[i]) / (knots[i + degree + 1 - r] - knots[i]);
            points[j] = points[j - 1] + (points[j] - points[j - 1]) * alpha;
        }
    }

    Some(Pos2::new(points[degree].x, points[degree].y))
}
