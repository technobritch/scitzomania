use godot::classes::{Control, IControl, Label, InputEvent, InputEventMouseButton, Node};
use godot::global::MouseButton;
use godot::prelude::*;
use crate::item_slot::ItemSlot;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct ItemChip {
    base: Base<Control>,
    item_slot_path: NodePath,
}

#[godot_api]
impl IControl for ItemChip {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            item_slot_path: NodePath::default(),
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mb) = event.try_cast::<InputEventMouseButton>() {
            if mb.is_pressed() && mb.get_button_index() == MouseButton::LEFT {
                self.notify_selected();
            }
        }
    }

    fn get_drag_data(&mut self, _at_position: Vector2) -> Variant {
        let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&self.item_slot_path) else {
            return Variant::nil();
        };
        let Some(item_id) = item_slot.bind().item_id() else {
            return Variant::nil();
        };

        let mut preview = Label::new_alloc();
        preview.set_text(&item_id);
        self.base_mut().set_drag_preview(&preview);

        let mut dict = Dictionary::<Variant, Variant>::new();
        dict.set("item_id", &item_id.to_variant());
        dict.set("source_path", &self.item_slot_path.to_variant());
        dict.set("origin", &"hiding_slot".to_variant());
        Variant::from(dict)
    }
}

#[godot_api]
impl ItemChip {
    pub fn set_item_slot_path(&mut self, path: NodePath) {
        self.item_slot_path = path;
        self.refresh_label();
    }

    fn refresh_label(&mut self) {
        let count = self.base().get_child_count();
        for i in (0..count).rev() {
            if let Some(mut child) = self.base().get_child(i) {
                child.queue_free();
            }
        }

        if let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&self.item_slot_path) {
            if let Some(item_id) = item_slot.bind().item_id() {
                let mut label = Label::new_alloc();
                label.set_text(&item_id);
                self.base_mut().add_child(&label);
            }
        }
    }

    fn notify_selected(&mut self) {
        let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&self.item_slot_path) else { return };
        let Some(item_id) = item_slot.bind().item_id() else { return };

        if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
            inventory.call("select_item", &[item_id.to_variant(), self.item_slot_path.to_variant()]);
        }
    }
}
