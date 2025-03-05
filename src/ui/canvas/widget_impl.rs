use egui::{Color32, Id, Pos2, Stroke, Widget};

use crate::ui::{helpers::draw_dashed_line_with_offset, temp_edge::TempEdgeWidget};

use super::{data::CanvasWidget, helpers::draw_grid};

impl Widget for &mut CanvasWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // self.pre_render_actions(ui);
        let desired_size = ui.available_size();
        let (screen_rect, canvas_response) =
            ui.allocate_exact_size(desired_size, egui::Sense::drag());

        // println!("desired_size: {:?}", desired_size);

        self.canvas_state_resource.read_resource(|canvas_state| {
            draw_grid(ui, canvas_state, screen_rect);
        });

        self.input_manager.update(ui, &canvas_response);

        if let Some(edge) = self.temp_edge.as_ref() {
            // println!("temp_edge target: {:?}", edge.target);
            ui.add(TempEdgeWidget {
                temp_edge: edge,
                graph_resource: self.graph_resource.clone(),
                canvas_state_resource: self.canvas_state_resource.clone(),
            });
        }

        // self.graph_resource.with_resource(|graph| {
        crate::graph::graph_impl::render_graph(
            ui,
            self.graph_resource.clone(),
            self.canvas_state_resource.clone(),
        );
        // });

        let offset: f32 = ui
            .data(|d| d.get_temp(Id::new("animation_offset")))
            .unwrap_or(0.0);

        draw_dashed_line_with_offset(
            ui.painter(),
            Pos2::new(100.0, 100.0),
            Pos2::new(300.0, 100.0),
            Stroke::new(2.0, Color32::ORANGE),
            10.0,
            5.0,
            offset,
        );

        // self.update_selected_nodes();
        // self.draw_particle_system(ui, screen_rect);
        self.input_manager.draw_particle_system(ui, screen_rect);
        // self.post_render_actions(ui, &canvas_response);

        canvas_response
    }
}
