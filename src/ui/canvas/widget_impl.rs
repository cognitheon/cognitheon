use egui::Widget;

use crate::ui::temp_edge::TempEdgeWidget;

use super::{data::CanvasWidget, helpers::draw_grid};

impl Widget for &mut CanvasWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.pre_render_actions(ui);
        let desired_size = ui.available_size();
        let (screen_rect, canvas_response) =
            ui.allocate_exact_size(desired_size, egui::Sense::drag());

        // println!("desired_size: {:?}", desired_size);

        self.canvas_state_resource
            .read_canvas_state(|canvas_state| {
                draw_grid(ui, canvas_state, screen_rect);
            });

        if let Some(edge) = self.temp_edge.as_ref() {
            // println!("temp_edge target: {:?}", edge.target);
            ui.add(TempEdgeWidget {
                temp_edge: edge,
                graph_resource: self.graph_resource.clone(),
                canvas_state_resource: self.canvas_state_resource.clone(),
            });
        }

        // graph_resource.read_graph(|graph| {
        crate::graph::graph_impl::render_graph(
            ui,
            self.graph_resource.clone(),
            self.canvas_state_resource.clone(),
        );
        // });

        self.post_render_actions(ui, &canvas_response);

        canvas_response
    }
}
