use egui::{Pos2, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntersectDirection {
    Left,
    Right,
    Top,
    Bottom,
}

pub fn widget_screen_pos(window_pos: Pos2, widget_rect: Rect) -> Pos2 {
    let vec = window_pos - widget_rect.min;
    Pos2::new(vec.x, vec.y)
}

pub fn intersect_rect_simple(
    canvas_rect: Rect,
    canvas_pos: Pos2,
) -> Option<(Pos2, IntersectDirection)> {
    let c = canvas_rect.center();
    let dx = canvas_pos.x - c.x;
    let dy = canvas_pos.y - c.y;

    let mut dir = IntersectDirection::Left;

    // 若方向向量都为 0，无法判定方向
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return None;
    }

    let mut t_candidates = Vec::new();

    // 计算与“左右边”相交的 t_x
    if dx > 0.0 {
        // 会先撞到 rect.max.x
        let t = (canvas_rect.max.x - c.x) / dx;
        if t >= 0.0 {
            let y_on_edge = c.y + t * dy;
            if y_on_edge >= canvas_rect.min.y && y_on_edge <= canvas_rect.max.y {
                t_candidates.push((t, Pos2::new(canvas_rect.max.x, y_on_edge)));
            }
        }
    } else if dx < 0.0 {
        // 会先撞到 rect.min.x
        let t = (canvas_rect.min.x - c.x) / dx;
        if t >= 0.0 {
            let y_on_edge = c.y + t * dy;
            if y_on_edge >= canvas_rect.min.y && y_on_edge <= canvas_rect.max.y {
                t_candidates.push((t, Pos2::new(canvas_rect.min.x, y_on_edge)));
            }
        }
    }

    // 计算与“上下边”相交的 t_y
    if dy > 0.0 {
        // 会先撞到 rect.max.y
        let t = (canvas_rect.max.y - c.y) / dy;
        if t >= 0.0 {
            let x_on_edge = c.x + t * dx;
            if x_on_edge >= canvas_rect.min.x && x_on_edge <= canvas_rect.max.x {
                t_candidates.push((t, Pos2::new(x_on_edge, canvas_rect.max.y)));
            }
        }
    } else if dy < 0.0 {
        // 会先撞到 rect.min.y
        let t = (canvas_rect.min.y - c.y) / dy;
        if t >= 0.0 {
            let x_on_edge = c.x + t * dx;
            if x_on_edge >= canvas_rect.min.x && x_on_edge <= canvas_rect.max.x {
                t_candidates.push((t, Pos2::new(x_on_edge, canvas_rect.min.y)));
            }
        }
    }

    // 取最小 t，并获取方向
    let min_t = t_candidates
        .into_iter()
        .min_by(|(t1, _), (t2, _)| t1.partial_cmp(t2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, pos)| pos);

    if let Some(pos) = min_t {
        if pos.x == canvas_rect.max.x {
            dir = IntersectDirection::Right;
        } else if pos.x == canvas_rect.min.x {
            dir = IntersectDirection::Left;
        } else if pos.y == canvas_rect.max.y {
            dir = IntersectDirection::Bottom;
        } else if pos.y == canvas_rect.min.y {
            dir = IntersectDirection::Top;
        }
        Some((pos, dir))
    } else {
        None
    }
}

pub fn intersect_rect_with_pos(
    canvas_rect: Rect,
    src_canvas_pos: Pos2,
    dst_canvas_pos: Pos2,
) -> Option<(Pos2, IntersectDirection)> {
    let dx = dst_canvas_pos.x - src_canvas_pos.x;
    let dy = dst_canvas_pos.y - src_canvas_pos.y;

    let mut dir = IntersectDirection::Left;
    // 若方向向量都为 0，无法判定方向
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return None;
    }

    let mut t_candidates = Vec::new();

    // 计算与“左右边”相交的 t_x
    if dx > 0.0 {
        // 会先撞到 rect.max.x
        let t = (canvas_rect.max.x - src_canvas_pos.x) / dx;
        if t >= 0.0 {
            let y_on_edge = src_canvas_pos.y + t * dy;
            if y_on_edge >= canvas_rect.min.y && y_on_edge <= canvas_rect.max.y {
                t_candidates.push((t, Pos2::new(canvas_rect.max.x, y_on_edge)));
            }
        }
    } else if dx < 0.0 {
        // 会先撞到 rect.min.x
        let t = (canvas_rect.min.x - src_canvas_pos.x) / dx;
        if t >= 0.0 {
            let y_on_edge = src_canvas_pos.y + t * dy;
            if y_on_edge >= canvas_rect.min.y && y_on_edge <= canvas_rect.max.y {
                t_candidates.push((t, Pos2::new(canvas_rect.min.x, y_on_edge)));
            }
        }
    }

    // 计算与“上下边”相交的 t_y
    if dy > 0.0 {
        // 会先撞到 rect.max.y
        let t = (canvas_rect.max.y - src_canvas_pos.y) / dy;
        if t >= 0.0 {
            let x_on_edge = src_canvas_pos.x + t * dx;
            if x_on_edge >= canvas_rect.min.x && x_on_edge <= canvas_rect.max.x {
                t_candidates.push((t, Pos2::new(x_on_edge, canvas_rect.max.y)));
            }
        }
    } else if dy < 0.0 {
        // 会先撞到 rect.min.y
        let t = (canvas_rect.min.y - src_canvas_pos.y) / dy;
        if t >= 0.0 {
            let x_on_edge = src_canvas_pos.x + t * dx;
            if x_on_edge >= canvas_rect.min.x && x_on_edge <= canvas_rect.max.x {
                t_candidates.push((t, Pos2::new(x_on_edge, canvas_rect.min.y)));
            }
        }
    }

    // 取最小 t，并获取方向
    let min_t = t_candidates
        .into_iter()
        .min_by(|(t1, _), (t2, _)| t1.partial_cmp(t2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, pos)| pos);

    if let Some(pos) = min_t {
        if pos.x == canvas_rect.max.x {
            dir = IntersectDirection::Right;
        } else if pos.x == canvas_rect.min.x {
            dir = IntersectDirection::Left;
        } else if pos.y == canvas_rect.max.y {
            dir = IntersectDirection::Bottom;
        } else if pos.y == canvas_rect.min.y {
            dir = IntersectDirection::Top;
        }
        Some((pos, dir))
    } else {
        None
    }
}

// 计算两点连线的垂向偏移方向
pub fn edge_offset_direction(source_canvas_pos: Pos2, target_canvas_pos: Pos2) -> Vec2 {
    let direction = (target_canvas_pos - source_canvas_pos).normalized();

    // 计算垂直向量

    direction.rot90()
}
