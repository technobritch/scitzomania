use godot::classes::{FileAccess, Json};
use godot::classes::file_access::ModeFlags;
use godot::prelude::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static MODEL_PATHS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Запоминает, какая 3D-модель стоит у предмета с таким id в мире.
pub fn register_model_path(id: &str, path: &str) {
    let map = MODEL_PATHS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut m) = map.lock() {
        m.insert(id.to_string(), path.to_string());
    }
}

fn registered_model_path(id: &str) -> Option<String> {
    let map = MODEL_PATHS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock().ok().and_then(|m| m.get(id).cloned())
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemSize {
    Small,
    Medium,
    Large,
}

impl ItemSize {
    pub fn from_str(s: &str) -> ItemSize {
        match s {
            "medium" => ItemSize::Medium,
            "large" => ItemSize::Large,
            _ => ItemSize::Small,
        }
    }

    pub fn units(&self) -> i32 {
        match self {
            ItemSize::Small => 1,
            ItemSize::Medium => 2,
            ItemSize::Large => 4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemCombination {
    pub with: GString,
    pub result: GString,
}

#[derive(Clone, Debug)]
pub struct ItemData {
    #[allow(dead_code)]
    pub id: GString,
    pub name: GString,
    pub description: GString,
    pub mesh_path: GString,
    pub scene_path: GString,
    pub shape: GString,
    pub color: GString,
    pub size: ItemSize,
    pub item_class: GString,
    /// Можно ли складывать несколько штук в одну ячейку (сигареты)
    pub stackable: bool,
    pub combinations: Vec<ItemCombination>,
}


pub fn load_item(id: &str) -> Option<ItemData> {
    let path = format!("res://items/{}.json", id);
    let file = FileAccess::open(&GString::from(path.as_str()), ModeFlags::READ)?;
    let text = file.get_as_text();

    let parsed = Json::parse_string(&text);
    let root = parsed.try_to::<Dictionary<Variant, Variant>>().ok()?;

    let name = root.get("name").map(|v| v.to::<GString>()).unwrap_or_default();
    let description = root.get("description").map(|v| v.to::<GString>()).unwrap_or_default();
    let mesh_path = root.get("mesh").map(|v| v.to::<GString>()).unwrap_or_default();
    let scene_from_json = root.get("scene").map(|v| v.to::<GString>()).unwrap_or_default();
    let scene_path = if scene_from_json.is_empty() {
        registered_model_path(id)
            .map(|p| GString::from(p.as_str()))
            .unwrap_or_default()
    } else {
        scene_from_json
    };
    let shape = root.get("shape").map(|v| v.to::<GString>()).unwrap_or_default();
    let color = root.get("color").map(|v| v.to::<GString>()).unwrap_or_default();
    let size_str = root.get("size").map(|v| v.to::<GString>()).unwrap_or_default();
    let item_class = root.get("class").map(|v| v.to::<GString>()).unwrap_or_default();

    let stackable = root
        .get("stackable")
        .and_then(|v| v.try_to::<bool>().ok())
        .unwrap_or(false);

    let mut combinations = Vec::new();
    if let Some(combos_variant) = root.get("combinations") {
        if let Ok(arr) = combos_variant.try_to::<Array<Variant>>() {
            for i in 0..arr.len() {
                if let Some(item) = arr.get(i) {
                    if let Ok(dict) = item.try_to::<Dictionary<Variant, Variant>>() {
                        let with = dict.get("with").map(|v| v.to::<GString>()).unwrap_or_default();
                        let result = dict.get("result").map(|v| v.to::<GString>()).unwrap_or_default();
                        combinations.push(ItemCombination { with, result });
                    }
                }
            }
        }
    }

    Some(ItemData {
        id: GString::from(id),
        name,
        description,
        mesh_path,
        scene_path,
        shape,
        color,
        size: ItemSize::from_str(&size_str.to_string()),
        item_class,
        stackable,
        combinations,
    })
}
