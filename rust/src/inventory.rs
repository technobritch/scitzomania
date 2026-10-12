use godot::classes::{Control, IControl, Input, InputEvent, GridContainer, Node, Label, Mesh};
use godot::classes::input::MouseMode;
use godot::prelude::*;
use crate::item_slot::ItemSlot;
use crate::hiding_slot::HidingSlot;
use crate::item_preview::ItemPreview;
use crate::item_database::{self, ItemSize};

#[derive(GodotClass)]
#[class(base=Control)]
struct Inventory {
    base: Base<Control>,
    selected_slot: Option<NodePath>,
    selected_hiding_slot: Option<NodePath>,
    combine_source: Option<(GString, NodePath)>,
}

#[godot_api]
impl IControl for Inventory {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            selected_slot: None,
            selected_hiding_slot: None,
            combine_source: None,
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh_risk_summary();
        }
    }

    fn unhandled_input(&mut self, _event: Gd<InputEvent>) {
        if Input::singleton().is_action_just_pressed("toggle_inventory") {
            let Some(mut player) = self.base().try_get_node_as::<Node>("%Player") else {
                return;
            };

            let player_on_floor = player.call("is_on_floor_now", &[]).try_to::<bool>().unwrap_or(true);
            let dialogue_active = player.call("is_dialogue_active", &[]).try_to::<bool>().unwrap_or(false);

            if !player_on_floor || dialogue_active {
                return;
            }

            let visible = self.base().is_visible();
            let new_visible = !visible;
            self.base_mut().set_visible(new_visible);

            if new_visible {
                Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
            } else {
                Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
            }
        }
    }
}

#[godot_api]
impl Inventory {
    #[func]
    fn add_item_icon(&mut self, item_id: GString) {
        self.place_item_in_first_empty_slot(item_id);
    }

    fn place_item_in_first_empty_slot(&mut self, item_id: GString) -> bool {
        let Some(slots) = self.base().try_get_node_as::<GridContainer>("%slots") else {
            return false;
        };

        let count = slots.get_child_count();
        for i in 0..count {
            if let Some(child) = slots.get_child(i) {
                if let Ok(mut slot) = child.try_cast::<ItemSlot>() {
                    if !slot.bind().has_item() {
                        slot.bind_mut().set_item(Some(item_id));
                        return true;
                    }
                }
            }
        }
        false
    }

    #[func]
    fn select_item(&mut self, item_id: GString, slot_path: NodePath) {
        if let Some((source_id, source_path)) = self.combine_source.clone() {
            self.combine_source = None;

            if source_path == slot_path {
                godot_print!("Нельзя объединить предмет с самим собой");
                return;
            }

            if let Some(source_data) = item_database::load_item(&source_id.to_string()) {
                let matched = source_data
                    .combinations
                    .iter()
                    .find(|c| c.with.to_string() == item_id.to_string())
                    .map(|c| c.result.clone());

                if let Some(result_id) = matched {
                    if let Some(mut s1) = self.base().try_get_node_as::<ItemSlot>(&source_path) {
                        s1.bind_mut().clear_item();
                    }
                    if let Some(mut s2) = self.base().try_get_node_as::<ItemSlot>(&slot_path) {
                        s2.bind_mut().clear_item();
                    }
                    self.place_item_in_first_empty_slot(result_id);
                    godot_print!("Предметы объединены!");
                } else {
                    godot_print!("Эти предметы нельзя объединить");
                }
            }
            return;
        }

        if let Some(prev_path) = self.selected_slot.clone() {
            if let Some(mut prev_slot) = self.base().try_get_node_as::<ItemSlot>(&prev_path) {
                prev_slot.bind_mut().set_highlighted(false);
            }
        }
        if let Some(prev_hiding) = self.selected_hiding_slot.clone() {
            if let Some(mut prev_hs) = self.base().try_get_node_as::<HidingSlot>(&prev_hiding) {
                prev_hs.bind_mut().set_highlighted(false);
            }
        }

        let mut hiding_path: Option<NodePath> = None;
        if let Some(mut slot) = self.base().try_get_node_as::<ItemSlot>(&slot_path) {
            slot.bind_mut().set_highlighted(true);
            hiding_path = slot.bind().hidden_at();
        }
        if let Some(h) = hiding_path.clone() {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                hs.bind_mut().set_highlighted(true);
            }
        }

        self.selected_slot = Some(slot_path);
        self.selected_hiding_slot = hiding_path.clone();

        self.update_preview_panel(&item_id, hiding_path);
    }

    #[func]
    fn start_combine(&mut self, item_id: GString, slot_path: NodePath) {
        self.combine_source = Some((item_id, slot_path));
        godot_print!("Выберите второй предмет для объединения");
    }

    fn size_text(size: &ItemSize) -> &'static str {
        match size {
            ItemSize::Small => "Маленький",
            ItemSize::Medium => "Средний",
            ItemSize::Large => "Большой",
        }
    }

    fn update_preview_panel(&mut self, item_id: &GString, hiding_path: Option<NodePath>) {
        let Some(data) = item_database::load_item(&item_id.to_string()) else {
            return;
        };

        if let Ok(mesh) = try_load::<Mesh>(&data.mesh_path) {
            if let Some(mut preview) = self.base().try_get_node_as::<ItemPreview>("%selected_item_preview") {
                preview.bind_mut().set_mesh(mesh);
                preview.bind_mut().set_rotating(true);
            }
        }

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_name") {
            label.set_text(&data.name);
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_description") {
            label.set_text(&data.description);
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_class") {
            label.set_text(&GString::from(format!("Класс: {}", data.item_class).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_size") {
            label.set_text(&GString::from(format!("Размер: {}", Self::size_text(&data.size)).as_str()));
        }

        let (location_text, detection_text) = match hiding_path {
            Some(h) => {
                if let Some(hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                    let b = hs.bind();
                    (
                        b.display_name.to_string(),
                        format!("{:.0}% заметность", b.detection_chance * 100.0),
                    )
                } else {
                    ("В сумке".to_string(), "—".to_string())
                }
            }
            None => ("В сумке".to_string(), "—".to_string()),
        };

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_location") {
            label.set_text(&GString::from(format!("Где: {}", location_text).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%selected_item_detection") {
            label.set_text(&GString::from(detection_text.as_str()));
        }
    }

    fn refresh_risk_summary(&mut self) {
        let Some(body_slots) = self.base().try_get_node_as::<Node>("%BodySlots") else {
            return;
        };

        let mut suspicious_names: Vec<String> = Vec::new();
        let mut hidden_count = 0;
        let mut detection_sum = 0.0f32;
        let mut detection_count = 0;

        let count = body_slots.get_child_count();
        for i in 0..count {
            let Some(child) = body_slots.get_child(i) else { continue };
            let Ok(hiding_slot) = child.try_cast::<HidingSlot>() else { continue };
            let b = hiding_slot.bind();
            let occupants = b.occupant_paths();
            let detection = b.detection_chance;
            drop(b);

            for occ in occupants {
                hidden_count += 1;
                detection_sum += detection;
                detection_count += 1;

                if let Some(item_slot) = self.base().try_get_node_as::<ItemSlot>(&occ) {
                    if let Some(item_id) = item_slot.bind().item_id() {
                        if let Some(data) = item_database::load_item(&item_id.to_string()) {
                            if data.item_class.to_string() != "I" {
                                suspicious_names.push(data.name.to_string());
                            }
                        }
                    }
                }
            }
        }

        let suspicious_text = if suspicious_names.is_empty() {
            "нет".to_string()
        } else {
            suspicious_names.join(", ")
        };

        let overall_risk = if detection_count == 0 {
            "—".to_string()
        } else {
            let avg = detection_sum / detection_count as f32;
            if avg < 0.4 {
                "Низкая".to_string()
            } else if avg < 0.65 {
                "Средняя".to_string()
            } else {
                "Высокая".to_string()
            }
        };

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%suspicious_items_label") {
            label.set_text(&GString::from(format!("Подозрительные предметы: {}", suspicious_text).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%hidden_count_label") {
            label.set_text(&GString::from(format!("Спрятано: {} предметов", hidden_count).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%overall_detection_label") {
            label.set_text(&GString::from(format!("Общая обнаруживаемость: {}", overall_risk).as_str()));
        }
    }
}
