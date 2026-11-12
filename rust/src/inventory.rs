use godot::classes::{Control, IControl, Input, InputEvent, GridContainer, Node, Label, Mesh, PackedScene};
use godot::classes::input::MouseMode;
use godot::prelude::*;
use crate::item_slot::ItemSlot;
use crate::hiding_slot::HidingSlot;
use crate::item_preview::{primitive_mesh, ItemPreview};
use crate::item_database::{self, ItemSize};

#[derive(GodotClass)]
#[class(base=Control)]
pub struct Inventory {
    base: Base<Control>,
    selected_slot: Option<NodePath>,
    selected_hiding_slot: Option<NodePath>,
    combine_source: Option<(GString, NodePath)>,
    summary_timer: f32,
}

#[godot_api]
impl IControl for Inventory {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            selected_slot: None,
            selected_hiding_slot: None,
            combine_source: None,
            summary_timer: 1.0,
        }
    }

    fn ready(&mut self) {
        let required = [
            "%Player",
            "%slots",
            "%BodySlots",
            "%SuspiciousItemsLabel",
            "%HiddenCountLabel",
            "%OverallDetectionLabel",
            "%SelectedItemPreview",
            "%SelectedItemName",
            "%SelectedItemDescription",
            "%SelectedItemClass",
            "%SelectedItemSize",
            "%SelectedItemLocation",
            "%SelectedItemDetection",
        ];
        for name in required {
            if self.base().try_get_node_as::<Node>(name).is_none() {
                godot_print!("[инвентарь] НЕ НАЙДЕН узел {}", name);
            }
        }

        self.clear_preview_panel();
    }

    fn process(&mut self, delta: f64) {
        if !self.base().is_visible() {
            self.summary_timer = 1.0;
            return;
        }

        self.summary_timer += delta as f32;
        if self.summary_timer >= 0.25 {
            self.summary_timer = 0.0;
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

    /// Подбор с количеством (стопки, например сигареты)
    #[func]
    fn add_item_amount(&mut self, item_id: GString, amount: i64) {
        self.add_item(&item_id, amount.max(1) as u32);
    }

    fn place_item_in_first_empty_slot(&mut self, item_id: GString) -> bool {
        self.add_item(&item_id, 1)
    }

    fn all_slots(&self) -> Vec<Gd<ItemSlot>> {
        let mut out = Vec::new();
        let Some(slots) = self.base().try_get_node_as::<GridContainer>("%slots") else {
            return out;
        };
        for i in 0..slots.get_child_count() {
            if let Some(child) = slots.get_child(i) {
                if let Ok(slot) = child.try_cast::<ItemSlot>() {
                    out.push(slot);
                }
            }
        }
        out
    }

    fn is_stackable(item_id: &str) -> bool {
        item_database::load_item(item_id)
            .map(|d| d.stackable)
            .unwrap_or(false)
    }

    /// Сколько штук предмета у игрока всего (во всех ячейках, в том числе спрятанных)
    pub fn count_item(&self, item_id: &str) -> u32 {
        let mut total = 0;
        for slot in self.all_slots() {
            let b = slot.bind();
            if b.item_id().map(|i| i.to_string() == item_id).unwrap_or(false) {
                total += b.count();
            }
        }
        total
    }

    /// Поместится ли amount штук предмета
    pub fn can_add_item(&self, item_id: &str, amount: u32) -> bool {
        let slots = self.all_slots();
        let empty = slots.iter().filter(|s| !s.bind().has_item()).count() as u32;
        if Self::is_stackable(item_id) {
            let has_stack = slots
                .iter()
                .any(|s| s.bind().item_id().map(|i| i.to_string() == item_id).unwrap_or(false));
            has_stack || empty >= 1
        } else {
            empty >= amount
        }
    }

    /// Кладёт предмет в инвентарь: стопируемые добавляются в существующую стопку
    pub fn add_item(&mut self, item_id: &GString, amount: u32) -> bool {
        let mut slots = self.all_slots();
        let mut ok = false;

        if Self::is_stackable(&item_id.to_string()) {
            // 1) стопка, которая не спрятана на теле
            for slot in slots.iter_mut() {
                let (same, hidden, count) = {
                    let b = slot.bind();
                    (
                        b.item_id().map(|i| i == *item_id).unwrap_or(false),
                        b.hidden_at().is_some(),
                        b.count(),
                    )
                };
                if same && !hidden {
                    slot.bind_mut().set_count(count + amount);
                    ok = true;
                    break;
                }
            }
            // 2) новая стопка в пустой ячейке
            if !ok {
                for slot in slots.iter_mut() {
                    if !slot.bind().has_item() {
                        slot.bind_mut().set_item_with_count(Some(item_id.clone()), amount);
                        ok = true;
                        break;
                    }
                }
            }
            // 3) крайний случай: только спрятанная стопка
            if !ok {
                for slot in slots.iter_mut() {
                    let (same, count) = {
                        let b = slot.bind();
                        (b.item_id().map(|i| i == *item_id).unwrap_or(false), b.count())
                    };
                    if same {
                        slot.bind_mut().set_count(count + amount);
                        ok = true;
                        break;
                    }
                }
            }
        } else {
            let mut remaining = amount;
            for slot in slots.iter_mut() {
                if remaining == 0 {
                    break;
                }
                if !slot.bind().has_item() {
                    slot.bind_mut().set_item(Some(item_id.clone()));
                    remaining -= 1;
                }
            }
            ok = remaining == 0;
        }

        self.base_mut().call_deferred("refresh_selection", &[]);
        ok
    }

    /// Забирает amount штук предмета (сначала из тех, что не спрятаны). false - если не хватает
    pub fn remove_item(&mut self, item_id: &str, amount: u32) -> bool {
        if self.count_item(item_id) < amount {
            return false;
        }

        let mut remaining = amount;
        for take_hidden in [false, true] {
            for mut slot in self.all_slots() {
                if remaining == 0 {
                    break;
                }
                let (same, hidden, count) = {
                    let b = slot.bind();
                    (
                        b.item_id().map(|i| i.to_string() == item_id).unwrap_or(false),
                        b.hidden_at().is_some(),
                        b.count(),
                    )
                };
                if !same || hidden != take_hidden {
                    continue;
                }
                let take = count.min(remaining);
                slot.bind_mut().set_count(count - take);
                remaining -= take;
            }
        }

        self.base_mut().call_deferred("refresh_selection", &[]);
        true
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

    /// Перечитать выбранный предмет после перетаскивания / "Достать"
    #[func]
    fn refresh_selection(&mut self) {
        let Some(slot_path) = self.selected_slot.clone() else {
            return;
        };

        let (item_id, hiding_path) = match self.base().try_get_node_as::<ItemSlot>(&slot_path) {
            Some(slot) => {
                let b = slot.bind();
                (b.item_id(), b.hidden_at())
            }
            None => (None, None),
        };

        if let Some(prev) = self.selected_hiding_slot.take() {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&prev) {
                hs.bind_mut().set_highlighted(false);
            }
        }

        let Some(item_id) = item_id else {
            if let Some(mut slot) = self.base().try_get_node_as::<ItemSlot>(&slot_path) {
                slot.bind_mut().set_highlighted(false);
            }
            self.selected_slot = None;
            self.clear_preview_panel();
            return;
        };

        if let Some(h) = hiding_path.clone() {
            if let Some(mut hs) = self.base().try_get_node_as::<HidingSlot>(&h) {
                hs.bind_mut().set_highlighted(true);
            }
        }
        self.selected_hiding_slot = hiding_path.clone();

        self.update_preview_panel(&item_id, hiding_path);
    }

    fn clear_preview_panel(&mut self) {
        if let Some(mut preview) = self.base().try_get_node_as::<ItemPreview>("%SelectedItemPreview") {
            preview.bind_mut().show_item(false);
            preview.bind_mut().set_rotating(false);
        }

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemName") {
            label.set_text("Выберите предмет");
        }
        for name in [
            "%SelectedItemDescription",
            "%SelectedItemClass",
            "%SelectedItemSize",
            "%SelectedItemLocation",
            "%SelectedItemDetection",
        ] {
            if let Some(mut label) = self.base().try_get_node_as::<Label>(name) {
                label.set_text("");
            }
        }
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
            godot_print!("[инвентарь] не удалось загрузить items/{}.json", item_id);
            return;
        };

        // Порядок: целая 3D-модель -> файл меша -> простая форма из поля "shape"
        let scene = if data.scene_path.is_empty() {
            None
        } else {
            try_load::<PackedScene>(&data.scene_path).ok()
        };
        let loaded_mesh = if data.mesh_path.is_empty() {
            None
        } else {
            try_load::<Mesh>(&data.mesh_path).ok()
        };
        let color = Color::from_html(&data.color).unwrap_or(Color::from_rgb(0.85, 0.85, 0.85));

        if let Some(mut preview) = self.base().try_get_node_as::<ItemPreview>("%SelectedItemPreview") {
            let mut shown = false;
            if let Some(scene) = scene {
                shown = preview.bind_mut().set_scene(scene);
            }
            if !shown {
                if !data.scene_path.is_empty() {
                    godot_print!("[инвентарь] не удалось показать модель {}", data.scene_path);
                }
                let mesh = loaded_mesh.unwrap_or_else(|| primitive_mesh(&data.shape.to_string()));
                preview.bind_mut().set_mesh(mesh);
                preview.bind_mut().set_color(color);
            }
            preview.bind_mut().show_item(true);
            preview.bind_mut().set_rotating(true);
        }

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemName") {
            label.set_text(&data.name);
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemDescription") {
            label.set_text(&data.description);
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemClass") {
            label.set_text(&GString::from(format!("Класс: {}", data.item_class).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemSize") {
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

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemLocation") {
            label.set_text(&GString::from(format!("Где: {}", location_text).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SelectedItemDetection") {
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

        if let Some(mut label) = self.base().try_get_node_as::<Label>("%SuspiciousItemsLabel") {
            label.set_text(&GString::from(format!("Подозрительные предметы: {}", suspicious_text).as_str()));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%HiddenCountLabel") {
            label.set_text(&GString::from(
                format!("Спрятано: {} {}", hidden_count, plural_items(hidden_count)).as_str(),
            ));
        }
        if let Some(mut label) = self.base().try_get_node_as::<Label>("%OverallDetectionLabel") {
            label.set_text(&GString::from(format!("Общая обнаруживаемость: {}", overall_risk).as_str()));
        }
    }
}

fn plural_items(n: i32) -> &'static str {
    let m10 = n % 10;
    let m100 = n % 100;
    if m10 == 1 && m100 != 11 {
        "предмет"
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        "предмета"
    } else {
        "предметов"
    }
}
