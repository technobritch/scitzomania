use godot::classes::input::MouseMode;
use godot::classes::{Control, INode, Input, InputEvent, InputMap, Node};
use godot::prelude::*;
use crate::dialogue_box::DialogueBox;

/// Четыре фазы дня по порядку
pub const PHASE_NAMES: [&str; 4] = ["morning", "day", "evening", "night"];

/// true — пока курсор мыши свободен (Esc), мир (NPC) замирает
const FREEZE_WHEN_MOUSE_VISIBLE: bool = true;

/// Единое состояние мира: флаги и счётчики (ключевые выборы игрока),
/// номер дня, фаза дня и язык условий, который читают диалоги, NPC и всё остальное.
///
/// Время здесь не течёт само: фазу двигают ключевые события через advance_phase().
#[derive(GodotClass)]
#[class(base=Node)]
pub struct GameState {
    base: Base<Node>,
    flags: Dictionary<Variant, Variant>,
    /// Номер дня, с 1
    day: i32,
    /// Индекс фазы: 0 утро, 1 день, 2 вечер, 3 ночь
    phase: i32,
    /// Растёт при любом изменении состояния — NPC по нему узнают, что пора пересчитать поведение
    version: u64,
}

#[godot_api]
impl INode for GameState {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            flags: Dictionary::new(),
            day: 1,
            phase: 0,
            version: 0,
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        // Отладка: клавиша, привязанная к действию debug_next_phase, листает фазы
        if InputMap::singleton().has_action("debug_next_phase")
            && event.is_action_pressed("debug_next_phase")
        {
            self.advance_phase();
        }
    }
}

#[godot_api]
impl GameState {
    /// Фаза или день изменились
    #[signal]
    fn phase_changed(day: i64, phase: i64);

    // ---------- флаги и счётчики (как раньше) ----------

    #[func]
    fn set_flag(&mut self, name: GString, value: Variant) {
        self.flags.set(&name.to_variant(), &value);
        self.version += 1;
    }

    #[func]
    fn get_flag(&self, name: GString) -> Variant {
        self.flags.get(&name.to_variant()).unwrap_or(Variant::nil())
    }

    #[func]
    fn has_flag(&self, name: GString) -> bool {
        self.flags.get(&name.to_variant()).is_some()
    }

    #[func]
    fn set_bool_flag(&mut self, name: GString, value: bool) {
        self.flags.set(&name.to_variant(), &value.to_variant());
        self.version += 1;
    }

    /// Переключить булев флаг на противоположный (нет флага — считается false)
    #[func]
    pub fn toggle_bool_flag(&mut self, name: GString) {
        let current = self.get_bool_flag(name.clone(), false);
        self.set_bool_flag(name, !current);
    }

    #[func]
    fn get_bool_flag(&self, name: GString, default_value: bool) -> bool {
        self.flags
            .get(&name.to_variant())
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(default_value)
    }

    #[func]
    fn increment_counter(&mut self, name: GString, by: i64) {
        let current = self.get_counter(name.clone());
        self.flags.set(&name.to_variant(), &(current + by).to_variant());
        self.version += 1;
    }

    #[func]
    fn get_counter(&self, name: GString) -> i64 {
        self.flags
            .get(&name.to_variant())
            .and_then(|v| v.try_to::<i64>().ok())
            .unwrap_or(0)
    }

    // ---------- день и фазы ----------

    pub fn day(&self) -> i32 {
        self.day
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn phase_name(&self) -> &'static str {
        PHASE_NAMES[(self.phase.rem_euclid(4)) as usize]
    }

    #[func]
    fn get_day(&self) -> i64 {
        self.day as i64
    }

    #[func]
    fn get_phase(&self) -> i64 {
        self.phase as i64
    }

    #[func]
    fn get_phase_name(&self) -> GString {
        GString::from(self.phase_name())
    }

    /// Сдвинуть время на следующую фазу (утро → день → вечер → ночь → утро следующего дня).
    /// Вызывается ключевыми событиями: концом диалога, сном, сюжетной сценой.
    #[func]
    fn advance_phase(&mut self) {
        self.phase += 1;
        if self.phase >= 4 {
            self.phase = 0;
            self.day += 1;
        }
        self.after_time_change();
    }

    /// Перейти сразу в нужные день и фазу (0 утро, 1 день, 2 вечер, 3 ночь)
    #[func]
    fn set_phase(&mut self, day: i64, phase: i64) {
        self.day = (day as i32).max(1);
        self.phase = (phase as i32).clamp(0, 3);
        self.after_time_change();
    }

    fn after_time_change(&mut self) {
        self.version += 1;
        godot_print!("[Мир] День {}, {}", self.day, self.phase_name());
        let (d, p) = (self.day as i64, self.phase as i64);
        self.base_mut()
            .emit_signal("phase_changed", &[d.to_variant(), p.to_variant()]);
    }

    // ---------- пауза мира ----------

    /// Открыто ли что-то, на время чего мир должен замирать: инвентарь, магазин, диалог, меню
    #[func]
    pub fn is_ui_blocking(&self) -> bool {
        if let Some(scene) = self.base().get_tree().get_current_scene() {
            let open = |name: &str| {
                scene
                    .try_get_node_as::<Control>(name)
                    .map(|c| c.is_visible())
                    .unwrap_or(false)
            };
            if open("%inventory") || open("%ShopUI") {
                return true;
            }

            let dialogue_open = scene
                .try_get_node_as::<DialogueBox>("%DialogueBox")
                .map(|d| d.bind().is_active())
                .unwrap_or(false);
            if dialogue_open {
                return true;
            }
        }

        FREEZE_WHEN_MOUSE_VISIBLE && Input::singleton().get_mouse_mode() == MouseMode::VISIBLE
    }

    // ---------- язык условий ----------

    /// Проверка условия. Примеры:
    ///   "flag:helped_guard"            флаг истинен (можно и просто "helped_guard")
    ///   "!flag:guard_angry"            флаг не истинен
    ///   "day>=2", "day==3"             номер дня
    ///   "phase==evening", "phase>=day" фаза (morning < day < evening < night)
    ///   "counter:trust>=3"             счётчик
    ///   "a && b", "a || b"             И, ИЛИ (&& сильнее ||)
    #[func]
    fn check(&self, condition: GString) -> bool {
        self.eval(&condition.to_string())
    }

    pub fn eval(&self, condition: &str) -> bool {
        let condition = condition.trim();
        if condition.is_empty() {
            return true;
        }
        condition
            .split("||")
            .any(|or_part| or_part.split("&&").all(|atom| self.eval_atom(atom.trim())))
    }

    fn eval_atom(&self, atom: &str) -> bool {
        if atom.is_empty() {
            return true;
        }
        if let Some(rest) = atom.strip_prefix('!') {
            if !rest.starts_with('=') {
                return !self.eval_atom(rest.trim());
            }
        }

        if let Some((lhs, op, rhs)) = split_cmp(atom) {
            let (lhs, rhs) = (lhs.trim(), rhs.trim());

            if lhs == "day" {
                return match rhs.parse::<i64>() {
                    Ok(n) => cmp_num(self.day as i64, op, n),
                    Err(_) => false,
                };
            }
            if lhs == "phase" {
                return match phase_index(rhs) {
                    Some(n) => cmp_num(self.phase as i64, op, n),
                    None => {
                        godot_print!("[Условие] неизвестная фаза '{}' в '{}'", rhs, atom);
                        false
                    }
                };
            }
            if let Some(name) = lhs.strip_prefix("counter:") {
                return match rhs.parse::<i64>() {
                    Ok(n) => cmp_num(self.get_counter(GString::from(name)), op, n),
                    Err(_) => false,
                };
            }
            godot_print!("[Условие] непонятное условие: '{}'", atom);
            return false;
        }

        let name = atom.strip_prefix("flag:").unwrap_or(atom);
        self.get_bool_flag(GString::from(name), false)
    }
}

/// Делит "day>=2" на ("day", ">=", "2")
fn split_cmp(atom: &str) -> Option<(&str, &'static str, &str)> {
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(i) = atom.find(op) {
            return Some((&atom[..i], op, &atom[i + op.len()..]));
        }
    }
    None
}

fn cmp_num(a: i64, op: &str, b: i64) -> bool {
    match op {
        "==" => a == b,
        "!=" => a != b,
        ">=" => a >= b,
        "<=" => a <= b,
        ">" => a > b,
        "<" => a < b,
        _ => false,
    }
}

/// "evening" -> 2; число тоже принимается
fn phase_index(s: &str) -> Option<i64> {
    PHASE_NAMES
        .iter()
        .position(|n| *n == s)
        .map(|i| i as i64)
        .or_else(|| s.parse::<i64>().ok())
}
