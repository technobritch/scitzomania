use godot::classes::file_access::ModeFlags;
use godot::classes::input::MouseMode;
use godot::classes::{
    Button, Control, FileAccess, IControl, Input, InputEvent, Json, Label, Node, PackedScene,
    VBoxContainer,
};
use godot::prelude::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::inventory::Inventory;
use crate::item_database;

// ---------------------------------------------------------------------------
// Данные магазина (файл shops/<имя>.json)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Entry {
    item: String,
    price: u32,
    /// None = товара неограниченно
    count: Option<u32>,
    /// Позиция доступна, только если этот флаг выставлен
    requires_flag: Option<String>,
    /// Позиция пропадает, если этот флаг выставлен
    unless_flag: Option<String>,
}

#[derive(Clone)]
struct ShopData {
    title: String,
    currency: String,
    stock: Vec<Entry>,
    buys: Vec<Entry>,
}

fn parse_entries(list: Option<Variant>) -> Vec<Entry> {
    let mut out = Vec::new();
    let Some(variant) = list else { return out };
    let Ok(arr) = variant.try_to::<Array<Variant>>() else { return out };

    for i in 0..arr.len() {
        let Some(item) = arr.get(i) else { continue };
        let Ok(d) = item.try_to::<Dictionary<Variant, Variant>>() else { continue };

        let name = d
            .get("item")
            .map(|v| v.to::<GString>().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let price = d
            .get("price")
            .and_then(|v| v.try_to::<f64>().ok())
            .unwrap_or(1.0)
            .max(0.0) as u32;
        let count = d
            .get("count")
            .and_then(|v| v.try_to::<f64>().ok())
            .map(|c| c.max(0.0) as u32);
        let requires_flag = d.get("requires_flag").map(|v| v.to::<GString>().to_string());
        let unless_flag = d.get("unless_flag").map(|v| v.to::<GString>().to_string());

        out.push(Entry {
            item: name,
            price,
            count,
            requires_flag,
            unless_flag,
        });
    }
    out
}

fn load_shop(path: &str) -> Option<ShopData> {
    let file = FileAccess::open(&GString::from(path), ModeFlags::READ)?;
    let text = file.get_as_text();

    let parsed = Json::parse_string(&text);
    let root = parsed.try_to::<Dictionary<Variant, Variant>>().ok()?;

    let title = root
        .get("title")
        .map(|v| v.to::<GString>().to_string())
        .unwrap_or_else(|| "Торговец".to_string());
    let currency = root
        .get("currency")
        .map(|v| v.to::<GString>().to_string())
        .unwrap_or_else(|| "cigarette".to_string());

    Some(ShopData {
        title,
        currency,
        stock: parse_entries(root.get("stock")),
        buys: parse_entries(root.get("buys")),
    })
}

// Сколько единиц товара уже куплено у каждого торговца (живёт, пока запущена игра)
static SOLD: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();

fn sold_count(shop: &str, item: &str) -> u32 {
    let map = SOLD.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .ok()
        .and_then(|m| m.get(&format!("{}|{}", shop, item)).copied())
        .unwrap_or(0)
}

fn add_sold(shop: &str, item: &str, amount: u32) {
    let map = SOLD.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut m) = map.lock() {
        *m.entry(format!("{}|{}", shop, item)).or_insert(0) += amount;
    }
}

fn item_name(id: &str) -> String {
    item_database::load_item(id)
        .map(|d| d.name.to_string())
        .unwrap_or_else(|| id.to_string())
}

struct Row {
    name: String,
    detail: String,
    price: String,
    button: String,
    enabled: bool,
    index: i64,
}

// ---------------------------------------------------------------------------
// Окно магазина
// ---------------------------------------------------------------------------

#[derive(GodotClass)]
#[class(base=Control)]
pub struct ShopUi {
    base: Base<Control>,
    /// Сцена одной строки списка (название, цена, кнопка)
    #[export]
    row_scene_path: GString,
    shop_path: String,
    data: Option<ShopData>,
    status: String,
}

#[godot_api]
impl IControl for ShopUi {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            row_scene_path: GString::from("res://shop_row.tscn"),
            shop_path: String::new(),
            data: None,
            status: String::new(),
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_visible(false);

        if let Some(mut close) = self.base().try_get_node_as::<Button>("%CloseButton") {
            let callback = self.base().callable("on_close_pressed");
            close.connect("pressed", &callback);
        } else {
            godot_print!("[магазин] НЕ НАЙДЕН узел %CloseButton");
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.base().is_visible() && event.is_action_pressed("ui_cancel") {
            self.close_shop();
            // Чтобы игрок не обработал тот же Esc и не отпустил мышь
            if let Some(mut viewport) = self.base().get_viewport() {
                viewport.set_input_as_handled();
            }
        }
    }
}

#[godot_api]
impl ShopUi {
    #[func]
    pub fn open_shop(&mut self, path: GString) {
        let Some(data) = load_shop(&path.to_string()) else {
            godot_print!("[магазин] не удалось загрузить файл магазина: {}", path);
            return;
        };

        self.shop_path = path.to_string();
        self.data = Some(data);
        self.status.clear();

        self.base_mut().set_visible(true);
        Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
        self.refresh();
    }

    #[func]
    pub fn close_shop(&mut self) {
        self.base_mut().set_visible(false);
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
    }

    #[func]
    fn on_close_pressed(&mut self) {
        self.close_shop();
    }

    #[func]
    fn on_buy_pressed(&mut self, index: i64) {
        let Some(data) = self.data.clone() else { return };
        let Some(entry) = data.stock.get(index as usize).cloned() else { return };

        if !self.entry_available(&entry) {
            self.status = "Этого уже нет в продаже".to_string();
            self.refresh();
            return;
        }
        if let Some(limit) = entry.count {
            if sold_count(&self.shop_path, &entry.item) >= limit {
                self.status = "Всё распродано".to_string();
                self.refresh();
                return;
            }
        }

        let Some(mut inventory) = self.base().try_get_node_as::<Inventory>("%inventory") else {
            return;
        };

        if inventory.bind().count_item(&data.currency) < entry.price {
            self.status = format!("Не хватает: {}", item_name(&data.currency).to_lowercase());
            self.refresh();
            return;
        }
        if !inventory.bind().can_add_item(&entry.item, 1) {
            self.status = "В инвентаре нет места".to_string();
            self.refresh();
            return;
        }

        inventory.bind_mut().remove_item(&data.currency, entry.price);
        inventory
            .bind_mut()
            .add_item(&GString::from(entry.item.as_str()), 1);
        add_sold(&self.shop_path, &entry.item, 1);

        self.status = format!("Куплено: {}", item_name(&entry.item));
        self.refresh();
    }

    #[func]
    fn on_sell_pressed(&mut self, index: i64) {
        let Some(data) = self.data.clone() else { return };
        let Some(entry) = data.buys.get(index as usize).cloned() else { return };

        if !self.entry_available(&entry) {
            self.status = "Это сейчас не берут".to_string();
            self.refresh();
            return;
        }

        let Some(mut inventory) = self.base().try_get_node_as::<Inventory>("%inventory") else {
            return;
        };

        if !inventory.bind_mut().remove_item(&entry.item, 1) {
            self.status = "У вас этого нет".to_string();
            self.refresh();
            return;
        }
        if entry.price > 0 {
            inventory
                .bind_mut()
                .add_item(&GString::from(data.currency.as_str()), entry.price);
        }

        self.status = format!("Продано: {}", item_name(&entry.item));
        self.refresh();
    }

    // -----------------------------------------------------------------------

    fn flag(&self, name: &str) -> bool {
        let Some(root) = self.base().get_tree().get_root() else {
            return false;
        };
        let Some(mut state) = root.try_get_node_as::<Node>("GameState") else {
            return false;
        };
        state
            .call(
                "get_bool_flag",
                &[GString::from(name).to_variant(), false.to_variant()],
            )
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// Доступна ли позиция сейчас (по флагам событий)
    fn entry_available(&self, entry: &Entry) -> bool {
        if let Some(flag) = &entry.requires_flag {
            if !self.flag(flag) {
                return false;
            }
        }
        if let Some(flag) = &entry.unless_flag {
            if self.flag(flag) {
                return false;
            }
        }
        true
    }

    fn set_label(&self, node_name: &str, text: &str) {
        if let Some(mut label) = self.base().try_get_node_as::<Label>(node_name) {
            label.set_text(&GString::from(text));
        } else {
            godot_print!("[магазин] НЕ НАЙДЕН узел {}", node_name);
        }
    }

    fn refresh(&mut self) {
        let Some(data) = self.data.clone() else { return };
        let Some(inventory) = self.base().try_get_node_as::<Inventory>("%inventory") else {
            godot_print!("[магазин] НЕ НАЙДЕН узел %inventory");
            return;
        };

        let money = inventory.bind().count_item(&data.currency);

        self.set_label("%ShopTitle", &data.title);
        self.set_label(
            "%ShopCurrency",
            &format!("{}: {}", item_name(&data.currency), money),
        );
        self.set_label("%ShopStatus", &self.status);

        // Колонка "Купить"
        let mut buy_rows = Vec::new();
        for (i, entry) in data.stock.iter().enumerate() {
            if !self.entry_available(entry) {
                continue;
            }
            let left = entry
                .count
                .map(|c| c.saturating_sub(sold_count(&self.shop_path, &entry.item)));
            let sold_out = left == Some(0);

            buy_rows.push(Row {
                name: item_name(&entry.item),
                detail: left.map(|n| format!("в наличии: {}", n)).unwrap_or_default(),
                price: entry.price.to_string(),
                button: if sold_out { "Нет".to_string() } else { "Купить".to_string() },
                enabled: !sold_out && money >= entry.price,
                index: i as i64,
            });
        }
        self.fill_list("%BuyList", buy_rows, "on_buy_pressed");

        // Колонка "Продать": только то, что у игрока есть и что этот торговец берёт
        let mut sell_rows = Vec::new();
        for (i, entry) in data.buys.iter().enumerate() {
            if !self.entry_available(entry) || entry.item == data.currency {
                continue;
            }
            let have = inventory.bind().count_item(&entry.item);
            if have == 0 {
                continue;
            }
            sell_rows.push(Row {
                name: item_name(&entry.item),
                detail: format!("у вас: {}", have),
                price: format!("+{}", entry.price),
                button: "Продать".to_string(),
                enabled: true,
                index: i as i64,
            });
        }
        self.fill_list("%SellList", sell_rows, "on_sell_pressed");
    }

    fn fill_list(&self, list_name: &str, rows: Vec<Row>, method: &str) {
        let Some(mut list) = self.base().try_get_node_as::<VBoxContainer>(list_name) else {
            godot_print!("[магазин] НЕ НАЙДЕН узел {}", list_name);
            return;
        };

        for i in (0..list.get_child_count()).rev() {
            if let Some(mut child) = list.get_child(i) {
                list.remove_child(&child);
                child.queue_free();
            }
        }

        let Ok(scene) = try_load::<PackedScene>(&self.row_scene_path) else {
            godot_print!("[магазин] не найдена сцена строки: {}", self.row_scene_path);
            return;
        };

        for row in rows {
            let Some(node) = scene.instantiate() else { continue };
            list.add_child(&node);

            for (name, text) in [
                ("%NameLabel", row.name.as_str()),
                ("%DetailLabel", row.detail.as_str()),
                ("%PriceLabel", row.price.as_str()),
            ] {
                if let Some(mut label) = node.try_get_node_as::<Label>(name) {
                    label.set_text(&GString::from(text));
                }
            }

            if let Some(mut button) = node.try_get_node_as::<Button>("%ActionButton") {
                button.set_text(&GString::from(row.button.as_str()));
                button.set_disabled(!row.enabled);
                let callback = self
                    .base()
                    .callable(method)
                    .bind(&[row.index.to_variant()]);
                button.connect("pressed", &callback);
            }
        }
    }
}
