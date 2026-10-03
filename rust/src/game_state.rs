use godot::classes::{Node, INode};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node)]
struct GameState {
    base: Base<Node>,
    flags: Dictionary<Variant, Variant>,
}

#[godot_api]
impl INode for GameState {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            flags: Dictionary::new(),
        }
    }
}

#[godot_api]
impl GameState {
    #[func]
    fn set_flag(&mut self, name: GString, value: Variant) {
        self.flags.set(&name.to_variant(), &value);
    }

    #[func]
    fn get_flag(&self, name: GString) -> Variant {
        self.flags.get(&name.to_variant()).unwrap_or(Variant::nil())
    }

    #[func]
    fn has_flag(&self, name: GString) -> bool {
        self.flags.get(&name.to_variant()).is_some()
    }

    #[func]
    fn set_bool_flag(&mut self, name: GString, value: bool) {
        self.flags.set(&name.to_variant(), &value.to_variant());
    }

    #[func]
    fn get_bool_flag(&self, name: GString, default_value: bool) -> bool {
        self.flags
            .get(&name.to_variant())
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(default_value)
    }

    #[func]
    fn increment_counter(&mut self, name: GString, by: i64) {
        let current = self.get_counter(name.clone());
        self.flags.set(&name.to_variant(), &(current + by).to_variant());
    }

    #[func]
    fn get_counter(&self, name: GString) -> i64 {
        self.flags
            .get(&name.to_variant())
            .and_then(|v| v.try_to::<i64>().ok())
            .unwrap_or(0)
    }
}
