use godot::classes::{Panel, IPanel, Label};
use godot::prelude::*;
use crate::item_database::{self, ItemSize};
use crate::item_slot::ItemSlot;

#[derive(GodotClass)]
#[class(base=Panel)]
pub struct HidingSlot {
    base: Base<Panel>,
    #[export]
    pub capacity: GString, // "small" | "medium" | "large"
    #[export]
    pub detection_chance: f32, // 0.0-1.0, для будущей проверки на обыске
    item_id: Option<GString>,
}

#[godot_api]
impl IPanel for HidingSlot {
    fn init(base: Base<Panel>) -> Self {
        Self {
            base,
            capacity: GString::from("small"),
            detection_chance: 0.5,
            item_id: None,
        }
    }

    fn ready(&mut self) {
        let color = if self.detection_chance >= 0.75 {
            Color::from_rgb(0.8, 0.2, 0.2)
        } else if self.detection_chance >= 0.5 {
            Color::from_rgb(0.8, 0.6, 0.1)
        } else {
            Color::from_rgb(0.2, 0.6, 0.2)
        };
        self.base_mut().set_modulate(color);
    }

    fn can_drop_data(&self, _at_position: Vector2, data: Variant) -> bool {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else {
            return false;
        };
        let Some(item_id_variant) = dict.get("item_id") else {
            return false;
        };
        let item_id = item_id_variant.to::<GString>();

        let Some(item_data) = item_database::load_item(&item_id.to_string()) else {
            return false;
        };

        let max_allowed = ItemSize::from_str(&self.capacity.to_string());
        item_data.size.rank() <= max_allowed.rank()
    }

    fn drop_data(&mut self, _at_position: Vector2, data: Variant) {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else { return };
        let Some(item_id_variant) = dict.get("item_id") else { return };
        let Some(source_path_variant) = dict.get("source_path") else { return };

        let item_id = item_id_variant.to::<GString>();
        let source_path = source_path_variant.to::<NodePath>();

        if let Some(mut src) = self.base().try_get_node_as::<ItemSlot>(&source_path) {
            src.bind_mut().clear_item();
        } else if let Some(mut src) = self.base().try_get_node_as::<HidingSlot>(&source_path) {
            src.bind_mut().clear_item();
        }

        self.set_item(Some(item_id));
    }

    fn get_drag_data(&mut self, _at_position: Vector2) -> Variant {
        let Some(item_id) = self.item_id.clone() else {
            return Variant::nil();
        };

        let mut preview = Label::new_alloc();
        preview.set_text(&item_id);
        self.base_mut().set_drag_preview(&preview);

        let mut dict = Dictionary::<Variant, Variant>::new();
        dict.set("item_id", &item_id.to_variant());
        dict.set("source_path", &self.base().get_path().to_variant());
        Variant::from(dict)
    }
}

#[godot_api]
impl HidingSlot {
    pub fn has_item(&self) -> bool {
        self.item_id.is_some()
    }

    pub fn clear_item(&mut self) {
        self.set_item(None);
    }

    pub fn set_item(&mut self, item_id: Option<GString>) {
        let count = self.base().get_child_count();
        for i in (0..count).rev() {
            if let Some(mut child) = self.base().get_child(i) {
                child.queue_free();
            }
        }

        self.item_id = item_id.clone();

        if let Some(id) = item_id {
            let mut label = Label::new_alloc();
            label.set_text(&id);
            self.base_mut().add_child(&label);
        }
    }
}
