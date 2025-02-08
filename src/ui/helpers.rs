use egui::{Painter, Pos2, Rect, Stroke};

/// 绘制虚线矩形
///
/// - `painter`: 目标 `Painter`
/// - `rect`: 要绘制虚线的矩形区域
/// - `stroke`: 线条的画笔（颜色+宽度）
/// - `dash_length`: 虚线中每个实线段的长度
/// - `gap_length`: 虚线中每个空隙的长度
pub fn draw_dashed_rect(
    painter: &Painter,
    rect: Rect,
    stroke: Stroke,
    dash_length: f32,
    gap_length: f32,
) {
    // 分别针对 rect 的四条边，画虚线
    let top_left = rect.min;
    let top_right = Pos2::new(rect.max.x, rect.min.y);
    let bot_left = Pos2::new(rect.min.x, rect.max.y);
    let bot_right = rect.max;

    // 上边
    draw_dashed_line(
        painter,
        top_left,
        top_right,
        stroke,
        dash_length,
        gap_length,
    );
    // 右边
    draw_dashed_line(
        painter,
        top_right,
        bot_right,
        stroke,
        dash_length,
        gap_length,
    );
    // 下边
    draw_dashed_line(
        painter,
        bot_right,
        bot_left,
        stroke,
        dash_length,
        gap_length,
    );
    // 左边
    draw_dashed_line(painter, bot_left, top_left, stroke, dash_length, gap_length);
}

/// 带动画 offset 的虚线矩形
pub fn draw_dashed_rect_with_offset(
    painter: &Painter,
    rect: Rect,
    stroke: Stroke,
    dash_length: f32,
    gap_length: f32,
    offset: f32,
) {
    let top_left = rect.min;
    let top_right = Pos2::new(rect.max.x, rect.min.y);
    let bot_left = Pos2::new(rect.min.x, rect.max.y);
    let bot_right = rect.max;

    // 上边
    draw_dashed_line_with_offset(
        painter,
        top_left,
        top_right,
        stroke,
        dash_length,
        gap_length,
        offset,
    );
    // 右边
    draw_dashed_line_with_offset(
        painter,
        top_right,
        bot_right,
        stroke,
        dash_length,
        gap_length,
        offset,
    );
    // 下边
    draw_dashed_line_with_offset(
        painter,
        bot_right,
        bot_left,
        stroke,
        dash_length,
        gap_length,
        offset,
    );
    // 左边
    draw_dashed_line_with_offset(
        painter,
        bot_left,
        top_left,
        stroke,
        dash_length,
        gap_length,
        offset,
    );
}

/// 绘制一条从 `start` 到 `end` 的“虚线段”，并带有一个动画相位偏移 `offset`
pub fn draw_dashed_line_with_offset(
    painter: &Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_length: f32,
    gap_length: f32,
    offset: f32, // 额外增加的参数：相位偏移
) {
    let total_vec = end - start;
    let total_length = total_vec.length();

    if total_length <= f32::EPSILON {
        return;
    }

    let direction = total_vec.normalized();
    let pattern_length = dash_length + gap_length;

    // 把 offset 缩放/取模到 [0, pattern_length)，避免越走越大
    let offset = offset.rem_euclid(pattern_length);
    // println!("offset: {:?}", offset);

    // 在“pattern 空间”里，我们想要覆盖区间 [offset, offset + total_length]
    let end_pattern = offset + total_length;

    // 步进循环
    let mut t = offset;
    while t < end_pattern {
        // dash 段的起点、终点（在 pattern 空间坐标）
        let dash_start = t;
        let dash_end = t + dash_length;

        // 下一个循环
        t += pattern_length;

        // 这里要把 dash_start~dash_end 截断到 [offset, end_pattern] 内
        let clipped_dash_start = dash_start.clamp(offset, end_pattern);
        let clipped_dash_end = dash_end.clamp(offset, end_pattern);

        // 只有当这段长度 > 0 时，才需要画
        if clipped_dash_end > clipped_dash_start {
            let start_dist = clipped_dash_start - offset;
            let end_dist = clipped_dash_end - offset;

            // 将这段距离映射回真实世界坐标
            let real_start = start + direction * start_dist;
            let real_end = start + direction * end_dist;

            // 绘制这段线
            painter.line_segment([real_start, real_end], stroke);
        }
    }
}

/// 绘制一条从 `start` 到 `end` 的虚线段
fn draw_dashed_line(
    painter: &Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_length: f32,
    gap_length: f32,
) {
    let total_length_vec = end - start; // 向量
    let total_length = total_length_vec.length();

    // 如果线长度很小，直接画实线或者可自行处理
    if total_length <= f32::EPSILON {
        return;
    }

    let direction = total_length_vec.normalized();

    // 记录当前起点，初始为 start
    let mut current_start = start;
    let mut distance_covered = 0.0;

    while distance_covered < total_length {
        // 本段可用的剩余长度
        let remaining = total_length - distance_covered;

        // 计算本次要画的虚线段长度
        let current_dash_len = dash_length.min(remaining);

        // 终点 = current_start + (direction * current_dash_len)
        let current_end = current_start + direction * current_dash_len;

        // 绘制实线段
        painter.line_segment([current_start, current_end], stroke);

        // 更新已走过的距离：加上 dash_length + gap_length
        distance_covered += dash_length + gap_length;

        // current_start 前进 dash_length + gap_length 后的下一个点
        // （注意要先前进一个 dash_length，再留出一个 gap_length）
        let next_start = current_start + direction * (dash_length + gap_length);
        current_start = next_start;
    }
}
