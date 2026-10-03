use godot::classes::{Panel, IPanel, PopupMenu, InputEvent, InputEventMouseButton, Label, StyleBoxFlat, Node};
use godot::global::MouseButton;
use godot::prelude::*;
use crate::hiding_slot::HidingSlot;

#[derive(GodotClass)]
#[class(base=Panel)]
pub struct ItemSlot {
    base: Base<Panel>,
    item_id: Option<GString>,
    hidden_at: Option<NodePath>,
    highlighted: bool,
}

#[godot_api]
impl IPanel for ItemSlot {
    fn init(base: Base<Panel>) -> Self {
        Self {
            base,
            item_id: None,
            hidden_at: None,
            highlighted: false,
        }
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

        let mut dict = Dictionary::<Variant, Variant>::new();
        dict.set("item_id", &item_id.to_variant());
        dict.set("source_path", &self.base().get_path().to_variant());
        dict.set("origin", &"item_slot".to_variant());
        Variant::from(dict)
    }

    fn can_drop_data(&self, _at_position: Vector2, data: Variant) -> bool {
        data.try_to::<Dictionary<Variant, Variant>>().is_ok()
    }

    fn drop_data(&mut self, _at_position: Vector2, data: Variant) {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else { return };
        let Some(source_path_variant) = dict.get("source_path") else { return };
        let source_path = source_path_variant.to::<NodePath>();
        let origin = dict.get("origin").map(|v| v.to::<GString>()).unwrap_or_default();

        if source_path == self.base().get_path() {
            return;
        }

        let Some(mut source_slot) = self.base().try_get_node_as::<ItemSlot>(&source_path) else { return };

        let dest_item = self.item_id.clone();
        let dest_hidden_at = self.hidden_at.clone();
        let (source_item, source_hidden_at) = {
            let b = source_slot.bind();
            (b.item_id.clone(), b.hidden_at.clone())
        };

        let unhiding = origin.to_string() == "hiding_slot";
        let new_self_hidden = if unhiding { None } else { source_hidden_at.clone() };

        self.set_item_and_link(source_item, new_self_hidden);
        source_slot.bind_mut().set_item_and_link(dest_item, dest_hidden_at.clone());

        if unhiding {
            if let Some(h) = source_hidden_at {
                if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                    hs.bind_mut().remove_occupant(&source_path);
                }
            }
        } else if let Some(h) = source_hidden_at {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                hs.bind_mut().replace_occupant(&source_path, self.base().get_path());
            }
        }

        if let Some(h) = dest_hidden_at {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                hs.bind_mut().replace_occupant(&self.base().get_path(), source_path.clone());
            }
        }
    }
}

#[godot_api]
impl ItemSlot {
    pub fn has_item(&self) -> bool {
        self.item_id.is_some()
    }

    pub fn item_id(&self) -> Option<GString> {
        self.item_id.clone()
    }

    pub fn hidden_at(&self) -> Option<NodePath> {
        self.hidden_at.clone()
    }

    pub fn clear_item(&mut self) {
        self.set_item(None);
    }

    pub fn set_item(&mut self, item_id: Option<GString>) {
        if let Some(old_hidden) = self.hidden_at.take() {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&old_hidden) {
                hs.bind_mut().remove_occupant(&self.base().get_path());
            }
        }
        self.rebuild_label(&item_id);
        self.item_id = item_id;
        self.refresh_border();
    }

    fn set_item_and_link(&mut self, item_id: Option<GString>, hidden_at: Option<NodePath>) {
        self.rebuild_label(&item_id);
        self.item_id = item_id;
        self.hidden_at = hidden_at;
        self.refresh_border();
    }

    pub fn set_hidden_link(&mut self, path: Option<NodePath>) {
        self.hidden_at = path;
        self.refresh_border();
    }

    fn rebuild_label(&mut self, item_id: &Option<GString>) {
        let count = self.base().get_child_count();
        for i in (0..count).rev() {
            if let Some(mut child) = self.base().get_child(i) {
                child.queue_free();
            }
        }

        if let Some(id) = item_id {
            let mut label = Label::new_alloc();
            label.set_text(id);
            self.base_mut().add_child(&label);
        }
    }

    pub fn set_highlighted(&mut self, on: bool) {
        self.highlighted = on;
        self.refresh_border();
    }

    fn refresh_border(&mut self) {
        if self.highlighted {
            let mut style = StyleBoxFlat::new_gd();
            style.set_bg_color(Color::from_rgb(0.15, 0.15, 0.15));
            style.set_border_width_all(3);
            style.set_border_color(Color::from_rgb(0.2, 0.9, 0.2));
            self.base_mut().add_theme_stylebox_override("panel", &style);
        } else if self.hidden_at.is_some() {
            let mut style = StyleBoxFlat::new_gd();
            style.set_bg_color(Color::from_rgb(0.15, 0.15, 0.15));
            style.set_border_width_all(2);
            style.set_border_color(Color::from_rgb(0.3, 0.5, 0.9));
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
        if self.hidden_at.is_some() {
            menu.add_item("Достать");
        }

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
            3 => {
                if let Some(old_hidden) = self.hidden_at.take() {
                    if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&old_hidden) {
                        hs.bind_mut().remove_occupant(&self.base().get_path());
                    }
                }
                self.refresh_border();
                godot_print!("Предмет возвращён в сумку");
            }
            _ => {}
        }
    }
}
