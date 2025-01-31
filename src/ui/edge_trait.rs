use super::bezier::Anchor;

trait EdgeTrait {
    fn get_target_anchor(&self, ui: &mut egui::Ui) -> Anchor;
}
