use godot::classes::control::MouseFilter;
use godot::classes::{Panel, IPanel, Label, StyleBoxFlat, Node, InputEvent, InputEventMouseButton};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;
use crate::item_database;
use crate::item_slot::ItemSlot;

#[derive(GodotClass)]
#[class(base=Panel)]
pub struct HidingSlot {
    base: Base<Panel>,
    #[export]
    pub capacity_units: i32,
    #[export]
    pub detection_chance: f32,
    #[export]
    pub display_name: GString,
    occupants: Vec<NodePath>,
    highlighted: bool,
}

#[godot_api]
impl IPanel for HidingSlot {
    fn init(base: Base<Panel>) -> Self {
        Self {
            base,
            capacity_units: 2,
            detection_chance: 0.5,
            display_name: GString::from("Тайник"),
            occupants: Vec::new(),
            highlighted: false,
        }
    }

    fn ready(&mut self) {
        self.apply_style();
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mb) = event.try_cast::<InputEventMouseButton>() {
            if mb.is_pressed() && mb.get_button_index() == MouseButton::LEFT {
                self.notify_selected();
            }
        }
    }

    fn can_drop_data(&self, _at_position: Vector2, data: Variant) -> bool {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else {
            return false;
        };
        let Some(item_id_variant) = dict.get("item_id") else {
            return false;
        };
        let item_id = item_id_variant.to::<GString>();
        let source_path = dict.get("source_path").map(|v| v.to::<NodePath>());

        let Some(item_data) = item_database::load_item(&item_id.to_string()) else {
            return false;
        };
        let new_units = item_data.size.units();

        let mut used = 0;
        for occ in &self.occupants {
            if Some(occ.clone()) == source_path {
                continue;
            }
            if let Some(slot) = self.base().try_get_node_as::<ItemSlot>(occ) {
                if let Some(id) = slot.bind().item_id() {
                    if let Some(d) = item_database::load_item(&id.to_string()) {
                        used += d.size.units();
                    }
                }
            }
        }

        used + new_units <= self.capacity_units
    }

    fn drop_data(&mut self, _at_position: Vector2, data: Variant) {
        let Ok(dict) = data.try_to::<Dictionary<Variant, Variant>>() else { return };
        let Some(source_path_variant) = dict.get("source_path") else { return };
        let source_path = source_path_variant.to::<NodePath>();

        if self.occupants.contains(&source_path) {
            return;
        }

        let Some(mut source_slot) = self.base().try_get_node_as::<ItemSlot>(&source_path) else { return };

        let old_hidden = source_slot.bind().hidden_at();
        if let Some(old_hidden) = old_hidden {
            if let Some(mut old_hs) = self.base().try_get_node_as::<HidingSlot>(&old_hidden) {
                old_hs.bind_mut().remove_occupant(&source_path);
            }
        }

        source_slot.bind_mut().set_hidden_link(Some(self.base().get_path()));
        self.occupants.push(source_path);
        self.refresh_badge();
        self.notify_changed();
    }

    fn get_drag_data(&mut self, _at_position: Vector2) -> Variant {
        let Some(occ_path) = self.occupants.last().cloned() else {
            return Variant::nil();
        };
        let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&occ_path) else {
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
        dict.set("source_path", &occ_path.to_variant());
        dict.set("origin", &"hiding_slot".to_variant());
        Variant::from(dict)
    }
}

#[godot_api]
impl HidingSlot {
    pub fn occupant_paths(&self) -> Vec<NodePath> {
        self.occupants.clone()
    }

    pub fn remove_occupant(&mut self, path: &NodePath) {
        self.occupants.retain(|p| p != path);
        self.refresh_badge();
    }

    pub fn replace_occupant(&mut self, old_path: &NodePath, new_path: NodePath) {
        if let Some(pos) = self.occupants.iter().position(|p| p == old_path) {
            self.occupants[pos] = new_path;
        } else {
            self.occupants.push(new_path);
        }
        self.refresh_badge();
    }

    pub fn set_highlighted(&mut self, on: bool) {
        self.highlighted = on;
        self.apply_style();
    }

    fn apply_style(&mut self) {
        let color = if self.detection_chance >= 0.75 {
            Color::from_rgb(0.8, 0.2, 0.2)
        } else if self.detection_chance >= 0.5 {
            Color::from_rgb(0.8, 0.6, 0.1)
        } else {
            Color::from_rgb(0.2, 0.6, 0.2)
        };

        let size = self.base().get_size();
        let radius = (size.x.min(size.y) / 2.0).max(4.0) as i32;

        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(color);
        style.set_corner_radius_all(radius);
        if self.highlighted {
            style.set_border_width_all(3);
            style.set_border_color(Color::from_rgb(1.0, 1.0, 1.0));
        }
        self.base_mut().add_theme_stylebox_override("panel", &style);
    }

    fn refresh_badge(&mut self) {
        let count = self.base().get_child_count();
        for i in (0..count).rev() {
            if let Some(mut child) = self.base().get_child(i) {
                child.queue_free();
            }
        }

        let n = self.occupants.len();
        if n == 0 {
            return;
        }

        let size = self.base().get_size();
        let mut label = Label::new_alloc();
        label.set_text(&GString::from(n.to_string().as_str()));
        label.set_size(size);
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.set_vertical_alignment(VerticalAlignment::CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().add_child(&label);
    }

    fn notify_changed(&self) {
        if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
            inventory.call_deferred("refresh_selection", &[]);
        }
    }

    fn notify_selected(&mut self) {
        let Some(occ_path) = self.occupants.last().cloned() else { return };
        let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&occ_path) else { return };
        let Some(item_id) = item_slot.bind().item_id() else { return };

        if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
            inventory.call_deferred("select_item", &[item_id.to_variant(), occ_path.to_variant()]);
        }
    }
}
