use egui::PointerButton;

#[derive(Debug, Clone, Default)]
pub struct ButtonState {
    primary: bool,
    secondary: bool,
    middle: bool,
    extra1: bool,
    extra2: bool,
}

impl ButtonState {
    pub fn new() -> Self {
        Self {
            primary: false,
            secondary: false,
            middle: false,
            extra1: false,
            extra2: false,
        }
    }

    pub fn get(&self, button: PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.primary,
            PointerButton::Secondary => self.secondary,
            PointerButton::Middle => self.middle,
            PointerButton::Extra1 => self.extra1,
            PointerButton::Extra2 => self.extra2,
        }
    }

    pub fn set(&mut self, button: PointerButton, value: bool) {
        match button {
            PointerButton::Primary => self.primary = value,
            PointerButton::Secondary => self.secondary = value,
            PointerButton::Middle => self.middle = value,
            PointerButton::Extra1 => self.extra1 = value,
            PointerButton::Extra2 => self.extra2 = value,
        }
    }
}
