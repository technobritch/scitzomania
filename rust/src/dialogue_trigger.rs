
use godot::classes::{StaticBody3D, IStaticBody3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=StaticBody3D)]
pub struct DialogueTrigger {
    base: Base<StaticBody3D>,
    #[export]
    pub dialogue_path: GString,
}

#[godot_api]
impl IStaticBody3D for DialogueTrigger {
    fn init(base: Base<StaticBody3D>) -> Self {
        Self {
            base,
            dialogue_path: GString::from("res://dialogues/example.json"),
        }
    }
}
