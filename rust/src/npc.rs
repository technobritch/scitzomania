use std::collections::HashMap;

use godot::classes::file_access::ModeFlags;
use godot::classes::{
    AnimationPlayer, CharacterBody3D, CollisionObject3D, FileAccess, ICharacterBody3D, Json, Node,
    PhysicsBody3D, PhysicsRayQueryParameters3D, ProjectSettings,
};
use godot::prelude::*;
use crate::game_state::GameState;
use crate::room_point::RoomPoint;

/// Скорость поворота NPC (рад/сек)
const TURN_SPEED: f32 = 6.0;
/// true — в Output печатается, что делает NPC (для отладки)
const DEBUG_NPC: bool = true;

/// Разгон и торможение (м/с²)
const ACCEL: f32 = 6.0;
/// В этом радиусе у цели NPC замедляется
const SLOW_RADIUS: f32 = 1.2;
/// Как часто проверяем, продвигается ли NPC (сек)
const STUCK_CHECK: f32 = 0.5;
/// Прошёл меньше этого за проверку (м) — значит застрял
const STUCK_MIN_MOVE: f32 = 0.12;
/// Сколько секунд обходит препятствие боком
const SIDESTEP_TIME: f32 = 0.7;
/// Если не может добраться столько секунд — ставим на место
const STUCK_TELEPORT: f32 = 8.0;
/// Если цель недостижима по навигации столько секунд (например, NPC оказался вне навмеша) — ставим на место
const UNREACHABLE_TELEPORT: f32 = 3.0;
/// «Усы»: лучи, которыми NPC щупает окружение впереди
const WHISKER_HEIGHT: f32 = 0.6;
const WHISKER_LEN: f32 = 1.1;
/// Угол боковых усов (радианы, ~34°)
const WHISKER_ANGLE: f32 = 0.6;
/// Если отодвинули от своего места дальше (м) — возвращается
const RETURN_DISTANCE: f32 = 0.7;
/// Пауза между попытками вернуться (сек)
const RETURN_COOLDOWN: f32 = 2.0;
/// Ключи правил в JSON: четыре фазы и "any" (любая фаза)
const RULE_KEYS: [&str; 5] = ["morning", "day", "evening", "night", "any"];

#[derive(Clone, Copy, PartialEq)]
enum NpcState {
    /// Идёт к контрольной точке
    Moving,
    /// Стоит на своём месте в комнате
    Waiting,
}

/// Что NPC делает в фазе: стоит в комнате `room` и играет `anim`
#[derive(Clone, PartialEq)]
struct Rule {
    room: String,
    anim: String,
    /// true — не идти пешком, а сразу появиться на месте
    teleport: bool,
}

/// Переопределение: если выполнено условие, его правила побеждают базовые.
/// Из подходящих побеждает то, у которого больше priority.
struct Override {
    priority: i32,
    cond: String,
    rules: HashMap<String, Rule>,
}

struct Schedule {
    default: HashMap<String, Rule>,
    overrides: Vec<Override>,
}

fn variant_to_i32(v: &Variant) -> i32 {
    // JSON-числа Godot читает как float
    if let Ok(f) = v.try_to::<f64>() {
        f as i32
    } else if let Ok(i) = v.try_to::<i64>() {
        i as i32
    } else {
        0
    }
}

/// Читает правила по ключам morning/day/evening/night/any из словаря
fn parse_rules(d: &Dictionary<Variant, Variant>) -> HashMap<String, Rule> {
    let mut out = HashMap::new();
    for key in RULE_KEYS {
        let Some(v) = d.get(key) else { continue };
        let Ok(rd) = v.try_to::<Dictionary<Variant, Variant>>() else { continue };

        let room = rd.get("room").map(|v| v.to::<GString>().to_string()).unwrap_or_default();
        if room.is_empty() {
            continue;
        }
        let anim = rd.get("anim").map(|v| v.to::<GString>().to_string()).unwrap_or_default();
        let teleport = rd
            .get("teleport")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);

        out.insert(key.to_string(), Rule { room, anim, teleport });
    }
    out
}

/// Читает res://npcs/<npc_id>.json
fn load_schedule(npc_id: &str) -> Option<Schedule> {
    let path = format!("res://npcs/{}.json", npc_id);
    let Some(file) = FileAccess::open(&GString::from(path.as_str()), ModeFlags::READ) else {
        godot_print!("[Npc] не удалось открыть файл поведения: {}", path);
        return None;
    };
    let text = file.get_as_text();

    let parsed = Json::parse_string(&text);
    let Ok(root) = parsed.try_to::<Dictionary<Variant, Variant>>() else {
        godot_print!("[Npc] неверный формат JSON: {}", path);
        return None;
    };

    let default = root
        .get("default")
        .and_then(|v| v.try_to::<Dictionary<Variant, Variant>>().ok())
        .map(|d| parse_rules(&d))
        .unwrap_or_default();

    let mut overrides = Vec::new();
    if let Some(v) = root.get("overrides") {
        if let Ok(arr) = v.try_to::<Array<Variant>>() {
            for i in 0..arr.len() {
                let Some(item) = arr.get(i) else { continue };
                let Ok(d) = item.try_to::<Dictionary<Variant, Variant>>() else { continue };

                let priority = d.get("priority").map(|v| variant_to_i32(&v)).unwrap_or(0);
                let cond = d.get("if").map(|v| v.to::<GString>().to_string()).unwrap_or_default();
                let rules = parse_rules(&d);
                if rules.is_empty() {
                    continue;
                }
                overrides.push(Override { priority, cond, rules });
            }
        }
    }
    overrides.sort_by(|a, b| b.priority.cmp(&a.priority));

    Some(Schedule { default, overrides })
}

/// NPC, поведение которого определяется состоянием мира (GameState): день, фаза и флаги.
/// Файл res://npcs/<npc_id>.json: базовое поведение по фазам + переопределения по условиям.
/// Путь между комнатами он строит сам через NavigationAgent3D.
///
/// Структура сцены:
///   Npc (CharacterBody3D, начало координат у ног!)
///   ├─ CollisionShape3D       (капсула, сдвинутая вверх на пол-высоты)
///   ├─ NavigationAgent3D
///   ├─ Model                  (модель, при желании с AnimationPlayer)
///   └─ DialogueTrigger        (для разговора)
#[derive(GodotClass)]
#[class(base=CharacterBody3D)]
pub struct Npc {
    base: Base<CharacterBody3D>,

    /// Имя NPC: файл res://npcs/<npc_id>.json и личное место в комнате (Marker3D с таким же именем)
    #[export]
    npc_id: GString,
    /// Скорость ходьбы, м/с
    #[export]
    speed: f32,
    /// Путь до AnimationPlayer внутри сцены NPC
    #[export]
    animation_player: NodePath,
    /// Анимация, когда стоит без особого дела (и пока ведёт диалог)
    #[export]
    idle_anim: GString,
    /// Анимация ходьбы
    #[export]
    walk_anim: GString,
    /// Булев флаг GameState, который переключается после каждого разговора с этим NPC
    /// (чтобы по нему в JSON можно было отправлять NPC в другую комнату). Пусто — не переключать
    #[export]
    talk_toggle_flag: GString,

    schedule: Option<Schedule>,
    schedule_loaded: bool,
    active_rule: Option<Rule>,
    last_version: u64,
    snap: bool,

    state: NpcState,
    current_room: String,
    target_room: String,
    stop_anim: String,
    final_yaw: f32,

    talking: bool,
    talk_frames: u32,
    look_yaw: f32,

    frames: u32,
    current_anim: String,
    warned_no_state: bool,

    // Умное движение
    target_pos: Vector3,
    stop_pos: Vector3,
    has_stop: bool,
    check_timer: f32,
    last_check_pos: Vector3,
    stuck_time: f32,
    stuck_total: f32,
    sidestep_timer: f32,
    sidestep_sign: f32,
    return_cooldown: f32,
    unreachable_time: f32,
    player_ignored: bool,
}

#[godot_api]
impl ICharacterBody3D for Npc {
    fn init(base: Base<CharacterBody3D>) -> Self {
        Self {
            base,
            npc_id: GString::new(),
            speed: 1.6,
            animation_player: NodePath::from("Model/AnimationPlayer"),
            idle_anim: GString::from("idle"),
            walk_anim: GString::from("walk"),
            talk_toggle_flag: GString::new(),
            schedule: None,
            schedule_loaded: false,
            active_rule: None,
            last_version: u64::MAX,
            snap: true,
            state: NpcState::Waiting,
            current_room: String::new(),
            target_room: String::new(),
            stop_anim: String::new(),
            final_yaw: 0.0,
            talking: false,
            talk_frames: 0,
            look_yaw: 0.0,
            frames: 0,
            current_anim: String::new(),
            warned_no_state: false,
            target_pos: Vector3::ZERO,
            stop_pos: Vector3::ZERO,
            has_stop: false,
            check_timer: 0.0,
            last_check_pos: Vector3::ZERO,
            stuck_time: 0.0,
            stuck_total: 0.0,
            sidestep_timer: 0.0,
            sidestep_sign: 1.0,
            return_cooldown: 0.0,
            unreachable_time: 0.0,
            player_ignored: false,
        }
    }

    fn ready(&mut self) {
        // Слои столкновений настраиваем сами, чтобы NPC не толкал свой же дочерний триггер диалога:
        // тело NPC — невидимо для лучей и других тел (слой 0), само сталкивается только с миром (слой 1);
        // дочерние тела (триггер диалога) — на слое 2 и ни с чем не сталкиваются сами.
        self.base_mut().set_collision_layer(0);
        self.base_mut().set_collision_mask(1);
        let children = self.base().get_children();
        for child in children.iter_shared() {
            if let Ok(mut body) = child.try_cast::<CollisionObject3D>() {
                body.set_collision_layer(2);
                body.set_collision_mask(0);
            }
        }

        let Some(mut agent) = self.nav_agent() else {
            godot_print!("[Npc] '{}': нет узла NavigationAgent3D", self.npc_id);
            return;
        };
        agent.set("path_desired_distance", &0.3f32.to_variant());
        agent.set("target_desired_distance", &0.3f32.to_variant());
    }

    fn physics_process(&mut self, delta: f64) {
        let delta = delta as f32;
        self.frames = self.frames.saturating_add(1);

        // Тело игрока не должно физически толкать NPC (но NPC видит его «усами» и обходит)
        if !self.player_ignored && self.frames > 5 {
            self.ignore_player();
        }

        let mut velocity = self.base().get_velocity();

        if !self.base().is_on_floor() {
            let gravity = ProjectSettings::singleton()
                .get_setting("physics/3d/default_gravity")
                .try_to::<f32>()
                .unwrap_or(9.8);
            velocity.y -= gravity * delta;
        } else {
            velocity.y = 0.0;
        }

        let Some(mut state) = self.find_state() else {
            if !self.warned_no_state {
                godot_print!(
                    "[Npc] '{}': не найден GameState (автозагрузка с именем GameStateGlobal) — NPC стоит на месте",
                    self.npc_id
                );
                self.warned_no_state = true;
            }
            velocity.x = 0.0;
            velocity.z = 0.0;
            self.base_mut().set_velocity(velocity);
            self.base_mut().move_and_slide();
            return;
        };

        let (version, phase, blocked) = {
            let s = state.bind();
            (s.version(), s.phase_name().to_string(), s.is_ui_blocking())
        };

        // Диалог закончился, когда окна закрылись (с небольшой задержкой на их открытие)
        if self.talking {
            self.talk_frames += 1;
            if self.talk_frames > 5 && !blocked {
                self.talking = false;
                // Разговор окончен: переключаем флаг, и NPC пойдёт в другую комнату
                if !self.talk_toggle_flag.is_empty() {
                    state.bind_mut().toggle_bool_flag(self.talk_toggle_flag.clone());
                }
            }
        }

        // Пока открыто меню, диалог, магазин или идёт разговор — NPC замирает на месте
        let frozen = self.talking || blocked;

        // Состояние мира изменилось (фаза, флаг) — пересчитываем, что NPC должен делать
        if !frozen && self.frames > 3 && version != self.last_version {
            self.refresh_rule(&state, &phase);
            self.last_version = version;
            self.snap = false;
        }

        let mut walking = false;

        if frozen {
            velocity.x = 0.0;
            velocity.z = 0.0;
            self.reset_stuck();
            if self.talking {
                self.face_yaw(self.look_yaw, delta);
            }
        } else {
            match self.state {
                NpcState::Moving => {
                    let previous = velocity;
                    if self.step_along_path(delta, &mut velocity) {
                        walking = true;
                        // Плавный разгон вместо мгновенного старта
                        velocity.x = approach(previous.x, velocity.x, ACCEL * delta);
                        velocity.z = approach(previous.z, velocity.z, ACCEL * delta);
                        self.update_stuck(delta);
                    } else {
                        velocity.x = 0.0;
                        velocity.z = 0.0;
                        self.state = NpcState::Waiting;
                        self.current_room = self.target_room.clone();
                        if DEBUG_NPC {
                            godot_print!("[Npc] '{}': дошёл до '{}'", self.npc_id, self.current_room);
                        }
                    }
                }
                NpcState::Waiting => {
                    velocity.x = 0.0;
                    velocity.z = 0.0;
                    self.face_yaw(self.final_yaw, delta);
                    self.return_cooldown = (self.return_cooldown - delta).max(0.0);

                    // Если игрок оттеснил NPC от его места — он сам возвращается
                    if self.has_stop && !self.current_room.is_empty() && self.return_cooldown <= 0.0 {
                        let pos = self.base().get_global_position();
                        let off = Vector3::new(pos.x - self.stop_pos.x, 0.0, pos.z - self.stop_pos.z).length();
                        if off > RETURN_DISTANCE {
                            self.return_cooldown = RETURN_COOLDOWN;
                            let room = self.current_room.clone();
                            let anim = self.stop_anim.clone();
                            if DEBUG_NPC {
                                godot_print!("[Npc] '{}': оттеснили от места, возвращается в '{}'", self.npc_id, room);
                            }
                            self.travel_to(&room, &anim);
                        }
                    }
                }
            }
        }

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();

        if DEBUG_NPC && self.frames % 60 == 0 {
            godot_print!(
                "[Npc] '{}': {} | позиция {:?} | на полу: {} | заморожен: {}",
                self.npc_id,
                if self.state == NpcState::Moving { "идёт" } else { "стоит" },
                self.base().get_global_position(),
                self.base().is_on_floor(),
                frozen
            );
        }

        let idle = self.idle_anim.to_string();
        let anim = if walking {
            self.walk_anim.to_string()
        } else if frozen || self.stop_anim.is_empty() {
            idle
        } else {
            self.stop_anim.clone()
        };
        self.play_anim(&anim);
    }
}

#[godot_api]
impl Npc {
    /// Вызывается, когда игрок начинает разговор: NPC останавливается и поворачивается к игроку.
    /// Продолжит путь сам, когда диалог/магазин закроются.
    #[func]
    pub fn begin_dialogue(&mut self, player_position: Vector3) {
        let pos = self.base().get_global_position();
        let d = player_position - pos;
        self.look_yaw = (-d.x).atan2(-d.z);
        self.talking = true;
        self.talk_frames = 0;
    }

    #[func]
    pub fn is_moving(&self) -> bool {
        self.state == NpcState::Moving
    }

    /// NavigationAgent3D берём как обычный узел и управляем им через call/set
    fn nav_agent(&self) -> Option<Gd<Node>> {
        self.base().try_get_node_as::<Node>("NavigationAgent3D")
    }

    fn find_state(&self) -> Option<Gd<GameState>> {
        let root = self.base().get_tree().get_root()?;
        root.try_get_node_as::<GameState>("GameStateGlobal")
    }

    /// Какое правило сейчас действует: самое приоритетное переопределение с истинным условием,
    /// иначе базовое поведение для этой фазы
    fn resolve_rule(&self, state: &GameState, phase: &str) -> Option<Rule> {
        let schedule = self.schedule.as_ref()?;

        for ov in &schedule.overrides {
            if !state.eval(&ov.cond) {
                continue;
            }
            if let Some(rule) = ov.rules.get(phase).or_else(|| ov.rules.get("any")) {
                return Some(rule.clone());
            }
        }

        schedule
            .default
            .get(phase)
            .or_else(|| schedule.default.get("any"))
            .cloned()
    }

    /// Пересчитывает правило и при смене отправляет NPC в нужную комнату
    fn refresh_rule(&mut self, state: &Gd<GameState>, phase: &str) {
        if !self.schedule_loaded {
            self.schedule = load_schedule(&self.npc_id.to_string());
            self.schedule_loaded = true;
        }

        let rule = {
            let s = state.bind();
            self.resolve_rule(&s, phase)
        };
        let Some(rule) = rule else { return };

        if self.active_rule.as_ref() == Some(&rule) {
            return;
        }

        if DEBUG_NPC {
            godot_print!(
                "[Npc] '{}': правило -> комната '{}', анимация '{}', {}",
                self.npc_id,
                rule.room,
                rule.anim,
                if self.snap || rule.teleport { "ставим сразу" } else { "идёт пешком" }
            );
        }

        if self.snap || rule.teleport {
            // Начало игры или явный телепорт: сразу на месте, без долгой дороги
            self.place_at_room(&rule.room, &rule.anim);
        } else {
            self.start_rule(&rule);
        }
        self.active_rule = Some(rule);
    }

    /// Идти в комнату из правила (если уже там — просто сменить анимацию)
    fn start_rule(&mut self, rule: &Rule) {
        if self.current_room == rule.room && self.state == NpcState::Waiting {
            self.stop_anim = rule.anim.clone();
            return;
        }
        self.travel_to(&rule.room, &rule.anim);
    }

    /// Мгновенно поставить NPC на его место в комнате
    fn place_at_room(&mut self, room: &str, anim: &str) {
        self.stop_anim = anim.to_string();
        if self.current_room == room && self.state == NpcState::Waiting {
            return;
        }
        let Some((pos, yaw)) = self.find_stop(room) else {
            godot_print!("[Npc] '{}': комната '{}' не найдена", self.npc_id, room);
            return;
        };
        self.base_mut().set_global_position(pos);
        self.base_mut().set_rotation(Vector3::new(0.0, yaw, 0.0));
        self.base_mut().set_velocity(Vector3::ZERO);
        self.final_yaw = yaw;
        self.current_room = room.to_string();
        self.state = NpcState::Waiting;
        self.stop_pos = pos;
        self.target_pos = pos;
        self.has_stop = true;

        if DEBUG_NPC {
            godot_print!("[Npc] '{}': поставлен в '{}' на {:?}", self.npc_id, room, pos);
        }
    }

    /// Находит место в комнате и просит NavigationAgent3D проложить путь
    fn travel_to(&mut self, room_id: &str, anim: &str) -> bool {
        self.stop_anim = anim.to_string();

        let Some((pos, yaw)) = self.find_stop(room_id) else {
            godot_print!(
                "[Npc] '{}': комната '{}' не найдена (нет RoomPoint с таким room_id)",
                self.npc_id,
                room_id
            );
            return false;
        };

        self.final_yaw = yaw;

        let Some(mut agent) = self.nav_agent() else {
            return false;
        };
        agent.call("set_target_position", &[pos.to_variant()]);
        self.target_pos = pos;
        self.stop_pos = pos;
        self.has_stop = true;
        self.reset_stuck();

        if DEBUG_NPC {
            godot_print!(
                "[Npc] '{}': идёт в '{}', цель {:?}, сейчас {:?}",
                self.npc_id,
                room_id,
                pos,
                self.base().get_global_position()
            );
        }

        self.target_room = room_id.to_string();
        self.current_room = String::new();
        self.state = NpcState::Moving;
        true
    }

    /// Ищет RoomPoint с нужным room_id и возвращает личное место этого NPC в нём
    fn find_stop(&self, room_id: &str) -> Option<(Vector3, f32)> {
        let tree = self.base().get_tree();
        let npc_id = self.npc_id.to_string();

        for node in tree.get_nodes_in_group("room_point").iter_shared() {
            if let Ok(room) = node.try_cast::<RoomPoint>() {
                let room = room.bind();
                if room.room_id.to_string() == room_id {
                    return Some(room.stop_for(&npc_id));
                }
            }
        }
        None
    }

    /// Один шаг по проложенному пути. true — ещё идёт, false — дошёл
    fn step_along_path(&mut self, delta: f32, velocity: &mut Vector3) -> bool {
        let Some(mut agent) = self.nav_agent() else {
            return false;
        };

        let finished = agent
            .call("is_navigation_finished", &[])
            .try_to::<bool>()
            .unwrap_or(true);
        if finished {
            return false;
        }

        let pos = self.base().get_global_position();
        let next = agent
            .call("get_next_path_position", &[])
            .try_to::<Vector3>()
            .unwrap_or(pos);
        let mut dir = next - pos;
        dir.y = 0.0;
        let dir = dir.normalized_or_zero();

        if dir == Vector3::ZERO {
            return true;
        }

        // Обход препятствия: если застрял, ненадолго шагает в сторону
        let mut steer = dir;
        if self.sidestep_timer > 0.0 {
            self.sidestep_timer = (self.sidestep_timer - delta).max(0.0);
            let side = Vector3::new(dir.z, 0.0, -dir.x) * self.sidestep_sign;
            steer = (dir * 0.5 + side * 0.9).normalized_or_zero();
        }

        // У цели замедляется, чтобы встать мягко
        let to_target =
            Vector3::new(self.target_pos.x - pos.x, 0.0, self.target_pos.z - pos.z).length();
        // Окружение: «усы» щупают стены и предметы впереди и отводят траекторию
        if to_target > 0.6 {
            steer = self.avoid_obstacles(steer);
        }

        let slow = (to_target / SLOW_RADIUS).clamp(0.35, 1.0);
        let speed = self.speed * slow;

        velocity.x = steer.x * speed;
        velocity.z = steer.z * speed;

        // В Godot «вперёд» — это -Z
        let yaw = (-steer.x).atan2(-steer.z);
        self.face_yaw(yaw, delta);
        true
    }

    /// Ставит NPC на его место в целевой комнате (последняя мера, если путь невозможен)
    fn recover_to_target(&mut self, reason: &str) {
        let room = self.target_room.clone();
        let anim = self.stop_anim.clone();
        if DEBUG_NPC {
            godot_print!("[Npc] '{}': {} — ставим в '{}'", self.npc_id, reason, room);
        }
        self.current_room = String::new();
        self.place_at_room(&room, &anim);
        self.reset_stuck();
    }

    /// NPC перестаёт физически сталкиваться с игроком, чтобы тот не мог затолкать его за стены
    fn ignore_player(&mut self) {
        self.player_ignored = true;
        let player = self
            .base()
            .get_tree()
            .get_current_scene()
            .and_then(|scene| scene.try_get_node_as::<PhysicsBody3D>("Player"));
        if let Some(player) = player {
            let node: Gd<Node> = player.upcast();
            self.base_mut().add_collision_exception_with(&node);
        } else if DEBUG_NPC {
            godot_print!("[Npc] '{}': узел Player не найден в сцене", self.npc_id);
        }
    }

    /// Луч в мир (слой 1: стены, предметы, игрок). Возвращает (расстояние, нормаль), если во что-то упёрся
    fn probe(&self, from: Vector3, to: Vector3) -> Option<(f32, Vector3)> {
        let world = self.base().get_world_3d()?;
        let mut space = world.get_direct_space_state()?;
        let mut query = PhysicsRayQueryParameters3D::create(from, to)?;
        query.set_collision_mask(1);
        let result = space.intersect_ray(&query);
        if result.is_empty() {
            return None;
        }
        let hit = result.get("position")?.try_to::<Vector3>().ok()?;
        let normal = result.get("normal")?.try_to::<Vector3>().ok()?;
        Some(((hit - from).length(), normal))
    }

    /// Огибание препятствий: три луча (прямо и под углом влево и вправо).
    /// Упёрся в стену — скользит вдоль неё; в лоб — идёт туда, где свободнее; сбоку близко — отходит
    fn avoid_obstacles(&self, dir: Vector3) -> Vector3 {
        let origin = self.base().get_global_position() + Vector3::new(0.0, WHISKER_HEIGHT, 0.0);
        let left_dir = rotate_y(dir, WHISKER_ANGLE);
        let right_dir = rotate_y(dir, -WHISKER_ANGLE);

        let center = self.probe(origin, origin + dir * WHISKER_LEN);
        let left = self.probe(origin, origin + left_dir * WHISKER_LEN);
        let right = self.probe(origin, origin + right_dir * WHISKER_LEN);

        if center.is_none() && left.is_none() && right.is_none() {
            return dir;
        }

        let closeness = |d: f32| 1.0 - (d / WHISKER_LEN).clamp(0.0, 1.0);
        let mut steer = dir;

        if let Some((d, normal)) = center {
            let n = Vector3::new(normal.x, 0.0, normal.z).normalized_or_zero();
            steer = steer + n * (1.0 + 2.0 * closeness(d));

            // Почти в лоб: выбираем сторону, где луч уходит дальше
            if n.dot(dir) < -0.7 {
                let left_free = left.map(|(d, _)| d).unwrap_or(WHISKER_LEN * 2.0);
                let right_free = right.map(|(d, _)| d).unwrap_or(WHISKER_LEN * 2.0);
                let side = if left_free >= right_free { left_dir } else { right_dir };
                steer = steer + side * 2.0;
            }
        }
        if let Some((d, _)) = left {
            steer = steer + right_dir * (closeness(d) * 1.5);
        }
        if let Some((d, _)) = right {
            steer = steer + left_dir * (closeness(d) * 1.5);
        }

        let steer = steer.normalized_or_zero();
        if steer == Vector3::ZERO {
            dir
        } else {
            steer
        }
    }

    fn reset_stuck(&mut self) {
        self.check_timer = 0.0;
        self.last_check_pos = self.base().get_global_position();
        self.stuck_time = 0.0;
        self.stuck_total = 0.0;
        self.sidestep_timer = 0.0;
        self.unreachable_time = 0.0;
    }

    /// Если NPC идёт, но не продвигается (упёрся в игрока, стену), то обходит боком,
    /// заново прокладывает путь, а если совсем не выходит — ставится на место
    fn update_stuck(&mut self, delta: f32) {
        self.check_timer += delta;
        if self.check_timer < STUCK_CHECK {
            return;
        }
        self.check_timer = 0.0;

        // Навигация говорит, что цель недостижима (NPC вне навмеша, проход закрыт) — не мучаемся долго
        let reachable = match self.nav_agent() {
            Some(mut agent) => agent
                .call("is_target_reachable", &[])
                .try_to::<bool>()
                .unwrap_or(true),
            None => true,
        };
        if reachable {
            self.unreachable_time = 0.0;
        } else {
            self.unreachable_time += STUCK_CHECK;
            if self.unreachable_time >= UNREACHABLE_TELEPORT {
                self.recover_to_target("цель недостижима по навигации");
                return;
            }
        }

        let pos = self.base().get_global_position();
        let moved = Vector3::new(pos.x - self.last_check_pos.x, 0.0, pos.z - self.last_check_pos.z)
            .length();
        self.last_check_pos = pos;

        if moved >= STUCK_MIN_MOVE {
            self.stuck_time = 0.0;
            self.stuck_total = 0.0;
            return;
        }

        self.stuck_time += STUCK_CHECK;
        self.stuck_total += STUCK_CHECK;

        if self.stuck_total >= STUCK_TELEPORT {
            self.recover_to_target("долго не может дойти");
            return;
        }

        if self.stuck_time >= 1.0 {
            self.stuck_time = 0.0;
            self.sidestep_timer = SIDESTEP_TIME;
            self.sidestep_sign = -self.sidestep_sign;

            let target = self.target_pos;
            if let Some(mut agent) = self.nav_agent() {
                agent.call("set_target_position", &[target.to_variant()]);
            }
            if DEBUG_NPC {
                godot_print!("[Npc] '{}': застрял, обходит и прокладывает путь заново", self.npc_id);
            }
        }
    }

    fn face_yaw(&mut self, target: f32, delta: f32) {
        let current = self.base().get_rotation().y;
        let new_yaw = turn_toward(current, target, TURN_SPEED * delta);
        self.base_mut().set_rotation(Vector3::new(0.0, new_yaw, 0.0));
    }

    fn play_anim(&mut self, name: &str) {
        if name.is_empty() || name == self.current_anim {
            return;
        }

        let player = self
            .base()
            .try_get_node_as::<AnimationPlayer>(&self.animation_player);
        let Some(mut player) = player else {
            return;
        };

        player.play_ex().name(&StringName::from(name)).done();
        self.current_anim = name.to_string();
    }
}

/// Поворачивает горизонтальный вектор вокруг оси Y на angle радиан
fn rotate_y(v: Vector3, angle: f32) -> Vector3 {
    let (s, c) = angle.sin_cos();
    Vector3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}

/// Приближает значение к цели не больше чем на step
fn approach(from: f32, to: f32, step: f32) -> f32 {
    if (to - from).abs() <= step {
        to
    } else {
        from + (to - from).signum() * step
    }
}

/// Поворот угла к цели по кратчайшему пути, не больше max_step за раз
fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut diff = (target - current).rem_euclid(tau);
    if diff > std::f32::consts::PI {
        diff -= tau;
    }
    if diff.abs() <= max_step {
        target
    } else {
        current + diff.signum() * max_step
    }
}
