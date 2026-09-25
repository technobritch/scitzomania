use godot::classes::{Panel, IPanel, PopupMenu, InputEvent, InputEventMouseButton, Label, StyleBoxFlat, Node};
use godot::global::MouseButton;
use godot::prelude::*;
use crate::hiding_slot::HidingSlot;

#[derive(GodotClass)]
#[class(base=Panel)]
pub struct ItemSlot {
    base: Base<Panel>,
    item_id: Option<GString>,
}

#[godot_api]
impl IPanel for ItemSlot {
    fn init(base: Base<Panel>) -> Self {
        Self { base, item_id: None }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mb) = event.try_cast::<InputEventMouseButton>() {
            if !mb.is_pressed() {
                return;
            }
            match mb.get_button_index() {
                MouseButton::LEFT => {
                    if self.item_id.is_some() {
                        self.notify_selected();
                    }
                }
                MouseButton::RIGHT => {
                    if self.item_id.is_some() {
                        let pos = mb.get_position();
                        self.show_context_menu(pos);
                    }
                }
                _ => {}
            }
        }
    }

    fn get_drag_data(&mut self, _at_position: Vector2) -> Variant {
        let Some(item_id) = self.item_id.clone() else {
            return Variant::nil();
        };

        let mut preview = Label::new_alloc();
        preview.set_text(&item_id);
        self.base_mut().set_drag_preview(&preview);

        let mut dict = Dictionary::new();
        dict.set("item_id", item_id.to_variant());
        dict.set("item_id", &item_id.to_variant());
        dict.set("source_path", &self.base().get_path().to_variant());
        Variant::from(dict)
    }

    fn can_drop_data(&self, _at_position: Vector2, data: Variant) -> bool {
        data.try_to::<Dictionary<Variant, Variant>>().is_ok()
    }

    fn drop_data(&mut self, _at_position: Vector2, data: Variant) {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else { return };
        let Some(item_id_variant) = dict.get("item_id") else { return };
        let Some(source_path_variant) = dict.get("source_path") else { return };

        let item_id = item_id_variant.to::<GString>();
        let source_path = source_path_variant.to::<NodePath>();
        let my_item = self.item_id.clone();

        if let Some(mut src) = self.base().try_get_node_as::<ItemSlot>(&source_path) {
            if src.instance_id() == self.base().instance_id() {
                return;
            }
            src.bind_mut().set_item(my_item);
        } else if let Some(mut src) = self.base().try_get_node_as::<HidingSlot>(&source_path) {
            src.bind_mut().clear_item();
        }

        self.set_item(Some(item_id));
    }
}

#[godot_api]
impl ItemSlot {
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

    pub fn set_highlighted(&mut self, on: bool) {
        if on {
            let mut style = StyleBoxFlat::new_gd();
            style.set_bg_color(Color::from_rgb(0.15, 0.15, 0.15));
            style.set_border_width_all(3);
            style.set_border_color(Color::from_rgb(0.2, 0.9, 0.2));
            self.base_mut().add_theme_stylebox_override("panel", &style);
        } else {
            self.base_mut().remove_theme_stylebox_override("panel");
        }
    }

    fn notify_selected(&mut self) {
        let Some(item_id) = self.item_id.clone() else { return };
        if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
            let path = self.base().get_path();
            inventory.call("select_item", &[item_id.to_variant(), path.to_variant()]);
        }
    }

    fn show_context_menu(&mut self, at_position: Vector2) {
        let mut menu = PopupMenu::new_alloc();
        menu.add_item("Взаимодействовать");
        menu.add_item("Изучить");
        menu.add_item("Объединить");

        let callable = self.base().callable("on_menu_id_pressed");
        menu.connect("id_pressed", &callable);

        let hide_callable = menu.callable("queue_free");
        menu.connect("popup_hide", &hide_callable);

        self.base_mut().add_child(&menu);

        let global_pos = self.base().get_global_position() + at_position;
        menu.set_position(Vector2i::new(global_pos.x as i32, global_pos.y as i32));
        menu.popup();
    }

    #[func]
    fn on_menu_id_pressed(&mut self, id: i64) {
        let Some(item_id) = self.item_id.clone() else { return };

        match id {
            0 => godot_print!("Взаимодействовать: {}", item_id),
            1 => self.notify_selected(),
            2 => {
                if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
                    let path = self.base().get_path();
                    inventory.call("start_combine", &[item_id.to_variant(), path.to_variant()]);
                }
            }
            _ => {}
        }
    }
}
