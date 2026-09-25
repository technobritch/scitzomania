use godot::classes::{Control, IControl, Label, FileAccess, Json, VBoxContainer};
use godot::classes::file_access::ModeFlags;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct DialogueBox {
    base: Base<Control>,
    nodes: Dictionary<Variant, Variant>,
    current_id: GString,
    choices: Vec<(GString, GString)>,
    in_choice_mode: bool,
    selected_index: i32,
}

#[godot_api]
impl IControl for DialogueBox {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            nodes: Dictionary::new(),
            current_id: GString::new(),
            choices: Vec::new(),
            in_choice_mode: false,
            selected_index: 0,
        }
    }
}

#[godot_api]
impl DialogueBox {
    pub fn is_active(&self) -> bool {
        self.base().is_visible()
    }

    #[func]
    pub fn start_dialogue(&mut self, path: GString) {
        let Some(file) = FileAccess::open(&path, ModeFlags::READ) else {
            godot_print!("Не удалось открыть файл диалога: {}", path);
            return;
        };
        let text = file.get_as_text();

        let parsed = Json::parse_string(&text);
        let Ok(root) = parsed.try_to::<Dictionary<Variant, Variant>>() else {
            godot_print!("Неверный формат JSON в файле: {}", path);
            return;
        };

        let Some(start_variant) = root.get("start") else {
            godot_print!("В диалоге нет поля 'start': {}", path);
            return;
        };
        let Some(nodes_variant) = root.get("nodes") else {
            godot_print!("В диалоге нет поля 'nodes': {}", path);
            return;
        };
        let Ok(nodes) = nodes_variant.try_to::<Dictionary<Variant, Variant>>() else {
            return;
        };

        self.nodes = nodes;
        let start_id = start_variant.to::<GString>();

        self.base_mut().set_visible(true);
        self.show_node(start_id);
    }

    fn show_node(&mut self, id: GString) {
        self.current_id = id.clone();

        let Some(node_variant) = self.nodes.get(&id) else {
            self.close();
            return;
        };
        let Ok(node_dict) = node_variant.try_to::<Dictionary<Variant, Variant>>() else {
            self.close();
            return;
        };

        let speaker = node_dict.get("speaker").map(|v| v.to::<GString>()).unwrap_or_default();
        let text = node_dict.get("text").map(|v| v.to::<GString>()).unwrap_or_default();

        if let Some(mut speaker_label) = self
            .base()
            .try_get_node_as::<Label>("Background/VBoxContainer/SpeakerLabel")
        {
            speaker_label.set_text(&speaker);
        }
        if let Some(mut text_label) = self
            .base()
            .try_get_node_as::<Label>("Background/VBoxContainer/TextLabel")
        {
            text_label.set_text(&text);
        }

        self.choices.clear();
        self.selected_index = 0;

        if let Some(choices_variant) = node_dict.get("choices") {
            if let Ok(arr) = choices_variant.try_to::<Array<Variant>>() {
                for i in 0..arr.len() {
                    if let Some(item) = arr.get(i) {
                        if let Ok(choice_dict) = item.try_to::<Dictionary<Variant, Variant>>() {
                            let choice_text = choice_dict.get("text").map(|v| v.to::<GString>()).unwrap_or_default();
                            let next = choice_dict.get("next").map(|v| v.to::<GString>()).unwrap_or_default();
                            self.choices.push((choice_text, next));
                        }
                    }
                }
            }
        }

        self.in_choice_mode = !self.choices.is_empty();
        self.render_choices();
    }

    fn render_choices(&mut self) {
        let Some(mut container) = self
            .base()
            .try_get_node_as::<VBoxContainer>("Background/VBoxContainer/ChoicesContainer")
        else {
            return;
        };

        let count = container.get_child_count();
        for i in (0..count).rev() {
            if let Some(mut child) = container.get_child(i) {
                child.queue_free();
            }
        }

        if !self.in_choice_mode {
            container.set_visible(false);
            return;
        }

        container.set_visible(true);

        for (text, _) in self.choices.clone() {
            let mut label = Label::new_alloc();
            label.set_text(&text);
            container.add_child(&label);
        }

        self.update_highlight();
    }

    fn update_highlight(&mut self) {
        let Some(container) = self
            .base()
            .try_get_node_as::<VBoxContainer>("Background/VBoxContainer/ChoicesContainer")
        else {
            return;
        };

        let count = container.get_child_count();
        for i in 0..count {
            if let Some(child) = container.get_child(i) {
                if let Ok(mut label) = child.try_cast::<Label>() {
                    if i == self.selected_index {
                        label.add_theme_color_override("font_color", Color::from_rgb(1.0, 0.85, 0.0));
                    } else {
                        label.add_theme_color_override("font_color", Color::from_rgb(1.0, 1.0, 1.0));
                    }
                }
            }
        }
    }

    #[func]
    pub fn move_selection(&mut self, delta: i32) {
        if !self.in_choice_mode || self.choices.is_empty() {
            return;
        }
        let len = self.choices.len() as i32;
        self.selected_index = ((self.selected_index + delta) % len + len) % len;
        self.update_highlight();
    }

    #[func]
    pub fn confirm(&mut self) {
        if self.in_choice_mode {
            if let Some((_, next)) = self.choices.get(self.selected_index as usize) {
                let next = next.clone();
                if next.is_empty() {
                    self.close();
                } else {
                    self.show_node(next);
                }
            }
            return;
        }

        let next_id = self
            .nodes
            .get(&self.current_id)
            .and_then(|v| v.try_to::<Dictionary<Variant, Variant>>().ok())
            .and_then(|d| d.get("next"))
            .map(|v| v.to::<GString>());

        match next_id {
            Some(id) if !id.is_empty() => self.show_node(id),
            _ => self.close(),
        }
    }

    #[func]
    pub fn close(&mut self) {
        self.base_mut().set_visible(false);
    }
}
