use godot::classes::{StaticBody3D, IStaticBody3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=StaticBody3D)]
pub struct Pickupable {
    base: Base<StaticBody3D>,
    #[export]
    pub item_id: GString,
}

#[godot_api]
impl IStaticBody3D for Pickupable {
    fn init(base: Base<StaticBody3D>) -> Self {
        Self {
            base,
            item_id: GString::from("cube"),
        }
    }
}
