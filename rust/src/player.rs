use godot::classes::{
    CharacterBody3D, ICharacterBody3D, Camera3D, Input, ProjectSettings,
    InputEvent, InputEventMouseMotion, PhysicsRayQueryParameters3D, Node, ColorRect, Control,
    CollisionShape3D, CapsuleShape3D,
};
use godot::classes::input::MouseMode;
use godot::prelude::*;
use crate::dialogue_box::DialogueBox;
use crate::dialogue_trigger::DialogueTrigger;
use crate::pickupable::Pickupable;
use crate::shop_ui::ShopUi;
use crate::npc::Npc;

/// Как далеко от камеры игрок может взаимодействовать (в метрах)
const INTERACT_DISTANCE: f32 = 4.0;
/// true - в Output печатается, во что попадает луч прицела (для отладки)
const DEBUG_RAY: bool = true;

const WALK_SPEED: f32 = 3.0;
const SPRINT_SPEED: f32 = 6.0;
const CROUCH_SPEED: f32 = 1.5;
const CROUCH_HEIGHT_RATIO: f32 = 0.6; // высота капсулы в приседе относительно обычной
const CROUCH_BLEND_SPEED: f32 = 8.0; // скорость плавного опускания/подъёма камеры (1/сек)
const SPRINT_FOV_BONUS: f32 = 8.0; // на сколько градусов расширяется FOV при беге
const SPRINT_FOV_BLEND_SPEED: f32 = 6.0;

#[derive(GodotClass)]
#[class(base=CharacterBody3D)]
struct Player {
    base: Base<CharacterBody3D>,
    jump_velocity: f32,
    mouse_sensitivity: f32,
    captured: bool,
    last_target: String,

    // Присед
    crouching: bool,
    crouch_blend: f32, // 0.0 — стоим, 1.0 — присели (для плавной камеры)
    stand_height: f32,
    stand_col_y: f32,
    stand_cam_y: f32,

    // Бег
    base_fov: f32,
    sprint_blend: f32,
}

#[godot_api]
impl ICharacterBody3D for Player {
    fn init(base: Base<CharacterBody3D>) -> Self {
        Self {
            base,
            jump_velocity: 4.5,
            mouse_sensitivity: 0.003,
            captured: false,
            last_target: String::new(),
            crouching: false,
            crouch_blend: 0.0,
            stand_height: 2.0,
            stand_col_y: 0.0,
            stand_cam_y: 1.8,
            base_fov: 75.0,
            sprint_blend: 0.0,
        }
    }

    fn ready(&mut self) {
        // Запоминаем исходные параметры капсулы
        if let Some(col) = self.base().try_get_node_as::<CollisionShape3D>("CollisionShape3D") {
            self.stand_col_y = col.get_position().y;
            if let Some(shape) = col.get_shape() {
                if let Ok(capsule) = shape.try_cast::<CapsuleShape3D>() {
                    self.stand_height = capsule.get_height();
                }
            }
        }

        // Запоминаем исходную высоту камеры и FOV
        if let Some(camera) = self.base().try_get_node_as::<Camera3D>("Camera3D") {
            self.stand_cam_y = camera.get_position().y;
            self.base_fov = camera.get_fov();
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.is_inventory_open() || self.is_shop_open() {
            return;
        }

        if !self.captured {
            Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
            self.captured = true;
        }

        if self.is_dialogue_box_active() {
            if event.is_action_pressed("ui_up") {
                if let Some(mut dialogue_box) = self.base().try_get_node_as::<DialogueBox>("%DialogueBox") {
                    dialogue_box.bind_mut().move_selection(-1);
                }
            }
            if event.is_action_pressed("ui_down") {
                if let Some(mut dialogue_box) = self.base().try_get_node_as::<DialogueBox>("%DialogueBox") {
                    dialogue_box.bind_mut().move_selection(1);
                }
            }
            if event.is_action_pressed("interact") || event.is_action_pressed("advance_dialogue") {
                if let Some(mut dialogue_box) = self.base().try_get_node_as::<DialogueBox>("%DialogueBox") {
                    dialogue_box.bind_mut().confirm();
                }
            }
            if event.is_action_pressed("ui_cancel") {
                if let Some(mut dialogue_box) = self.base().try_get_node_as::<DialogueBox>("%DialogueBox") {
                    dialogue_box.bind_mut().close();
                }
            }
            return;
        }

        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            let rel = motion.get_relative();
            let sens = self.mouse_sensitivity;
            self.base_mut().rotate_y(-rel.x * sens);

            if let Some(mut camera) = self.base().try_get_node_as::<Camera3D>("Camera3D") {
                camera.rotate_x(-rel.y * sens);
                let mut rot = camera.get_rotation();
                rot.x = rot.x.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
                camera.set_rotation(rot);
            }
        }

        if event.is_action_pressed("ui_cancel") {
            Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
            self.captured = false;
        }

        if event.is_action_pressed("interact") {
            self.try_interact();
        }
    }

    fn physics_process(&mut self, delta: f64) {
        let delta = delta as f32;

        if self.is_dialogue_active() || self.is_inventory_open() {
            self.base_mut().set_velocity(Vector3::ZERO);
            self.base_mut().move_and_slide();
            self.update_crosshair();
            return;
        }

        let input = Input::singleton();
        let on_floor = self.base().is_on_floor();

        // --- Присед ---
        self.update_crouch(input.is_action_pressed("crouch"));

        let mut velocity = self.base().get_velocity();

        if !on_floor {
            let gravity = ProjectSettings::singleton()
                .get_setting("physics/3d/default_gravity")
                .try_to::<f32>()
                .unwrap_or(9.8);
            velocity.y -= gravity * delta;
        }

        if input.is_action_just_pressed("ui_accept") && on_floor && !self.crouching {
            velocity.y = self.jump_velocity;
        }

        let input_dir = input.get_vector("ui_left", "ui_right", "ui_up", "ui_down");

        // --- Выбор скорости: присед / бег / ходьба ---
        // Бег — только пока движемся вперёд и не в приседе
        let moving_forward = input_dir.y < -0.1;
        let sprinting = input.is_action_pressed("sprint") && moving_forward && !self.crouching;

        let speed = if self.crouching {
            CROUCH_SPEED
        } else if sprinting {
            SPRINT_SPEED
        } else {
            WALK_SPEED
        };

        self.update_camera(delta, sprinting);

        let basis = self.base().get_transform().basis;
        let direction = (basis * Vector3::new(input_dir.x, 0.0, input_dir.y)).normalized_or_zero();

        if direction != Vector3::ZERO {
            velocity.x = direction.x * speed;
            velocity.z = direction.z * speed;
        } else {
            // дельта с запасом, чтобы остановка оставалась мгновенной, как раньше
            velocity.x = move_toward(velocity.x, 0.0, SPRINT_SPEED);
            velocity.z = move_toward(velocity.z, 0.0, SPRINT_SPEED);
        }

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();

        self.update_crosshair();
    }
}

#[godot_api]
impl Player {
    #[func]
    fn is_on_floor_now(&self) -> bool {
        self.base().is_on_floor()
    }

    #[func]
    fn is_dialogue_active(&self) -> bool {
        // Любое окно, которое должно остановить игрока: диалог или магазин
        self.is_dialogue_box_active() || self.is_shop_open()
    }

    fn is_dialogue_box_active(&self) -> bool {
        self.base()
            .try_get_node_as::<DialogueBox>("%DialogueBox")
            .map(|d| d.bind().is_active())
            .unwrap_or(false)
    }

    fn is_shop_open(&self) -> bool {
        self.base()
            .try_get_node_as::<Control>("%ShopUI")
            .map(|c| c.is_visible())
            .unwrap_or(false)
    }

    fn is_inventory_open(&self) -> bool {
        self.base()
            .try_get_node_as::<Control>("%inventory")
            .map(|c| c.is_visible())
            .unwrap_or(false)
    }

    /// Логика приседа: сел — если зажата кнопка и мы на полу; встал — если отпустили и над головой свободно
    fn update_crouch(&mut self, want_crouch: bool) {
        let on_floor = self.base().is_on_floor();

        if want_crouch && !self.crouching && on_floor {
            self.crouching = true;
            let h = self.stand_height * CROUCH_HEIGHT_RATIO;
            self.apply_collision_height(h);
        } else if !want_crouch && self.crouching && self.can_stand_up() {
            self.crouching = false;
            let h = self.stand_height;
            self.apply_collision_height(h);
        }
    }

    /// Меняет высоту капсулы так, чтобы её низ остался на месте (игрок не зависает в воздухе)
    fn apply_collision_height(&mut self, new_height: f32) {
        let full = self.stand_height;
        let base_y = self.stand_col_y;

        if let Some(mut col) = self.base().try_get_node_as::<CollisionShape3D>("CollisionShape3D") {
            if let Some(shape) = col.get_shape() {
                if let Ok(mut capsule) = shape.try_cast::<CapsuleShape3D>() {
                    capsule.set_height(new_height);
                }
            }
            let mut pos = col.get_position();
            pos.y = base_y + (new_height - full) * 0.5;
            col.set_position(pos);
        }
    }

    /// Проверяет лучом вверх, хватает ли места, чтобы встать
    fn can_stand_up(&self) -> bool {
        let origin = self.base().get_global_position();
        let bottom = origin.y + self.stand_col_y - self.stand_height * 0.5;
        let crouch_top = bottom + self.stand_height * CROUCH_HEIGHT_RATIO;
        let full_top = bottom + self.stand_height + 0.05;

        let from = Vector3::new(origin.x, crouch_top - 0.05, origin.z);
        let to = Vector3::new(origin.x, full_top, origin.z);

        let Some(world) = self.base().get_world_3d() else { return true };
        let Some(mut space_state) = world.get_direct_space_state() else { return true };
        let Some(mut query) = PhysicsRayQueryParameters3D::create(from, to) else { return true };

        // Не попадаем лучом в самого игрока
        let mut exclude = Array::<Rid>::new();
        exclude.push(self.base().get_rid());
        query.set_exclude(&exclude);

        space_state.intersect_ray(&query).is_empty()
    }

    /// Плавная высота камеры (присед) и FOV (бег)
    fn update_camera(&mut self, delta: f32, sprinting: bool) {
        let crouch_target = if self.crouching { 1.0 } else { 0.0 };
        self.crouch_blend = move_toward(self.crouch_blend, crouch_target, CROUCH_BLEND_SPEED * delta);

        let sprint_target = if sprinting { 1.0 } else { 0.0 };
        self.sprint_blend = move_toward(self.sprint_blend, sprint_target, SPRINT_FOV_BLEND_SPEED * delta);

        let crouch_drop = self.stand_height * (1.0 - CROUCH_HEIGHT_RATIO);
        let cam_y = self.stand_cam_y - crouch_drop * self.crouch_blend;
        let fov = self.base_fov + SPRINT_FOV_BONUS * self.sprint_blend;

        if let Some(mut camera) = self.base().try_get_node_as::<Camera3D>("Camera3D") {
            let mut pos = camera.get_position();
            pos.y = cam_y;
            camera.set_position(pos);
            camera.set_fov(fov);
        }
    }

    /// Центр нарисованного прицела в координатах экрана (или центр окна, если прицела нет)
    fn aim_screen_point(&self) -> Option<Vector2> {
        if let Some(crosshair) = self.base().try_get_node_as::<ColorRect>("%ColorRect") {
            let rect = crosshair.get_global_rect();
            return Some(rect.position + rect.size * 0.5);
        }
        self.viewport_center()
    }

    fn viewport_center(&self) -> Option<Vector2> {
        let viewport = self.base().get_viewport()?;
        Some(viewport.get_visible_rect().size * 0.5)
    }

    /// Кастует луч вперёд от камеры и возвращает любой узел, в который попал луч
    fn raycast_hit(&self) -> Option<Gd<Node>> {
        let camera = self.base().try_get_node_as::<Camera3D>("Camera3D")?;

        // Луч идёт ровно через ту точку экрана, где нарисован прицел
        let screen = self.aim_screen_point()?;
        let from = camera.project_ray_origin(screen);
        let to = from + camera.project_ray_normal(screen) * INTERACT_DISTANCE;

        let world = self.base().get_world_3d()?;
        let mut space_state = world.get_direct_space_state()?;
        let query = PhysicsRayQueryParameters3D::create(from, to)?;
        let result = space_state.intersect_ray(&query);

        if result.is_empty() {
            return None;
        }

        let collider_variant = result.get("collider")?;
        collider_variant.try_to::<Gd<Node>>().ok()
    }

    fn raycast_item(&self) -> Option<Gd<Node>> {
        let node = self.raycast_hit()?;
        if node.is_in_group("item") {
            Some(node)
        } else {
            None
        }
    }

    fn raycast_dialogue(&self) -> Option<Gd<Node>> {
        let node = self.raycast_hit()?;
        if node.is_in_group("dialogue") {
            Some(node)
        } else {
            None
        }
    }

    /// Красит crosshair в зависимости от того, на что мы смотрим
    fn update_crosshair(&mut self) {
        // Один луч на кадр вместо трёх
        let hit = self.raycast_hit();
        let looking_at_item = hit.as_ref().map(|n| n.is_in_group("item")).unwrap_or(false);
        let looking_at_dialogue = hit.as_ref().map(|n| n.is_in_group("dialogue")).unwrap_or(false);

        if DEBUG_RAY {
            let name = match &hit {
                Some(n) => format!("{} ({})", n.get_name(), n.get_class()),
                None => "ничего".to_string(),
            };
            if name != self.last_target {
                godot_print!(
                    "[луч] {} | в группе item: {} | прицел {:?}, центр окна {:?}",
                    name,
                    looking_at_item,
                    self.aim_screen_point(),
                    self.viewport_center()
                );
                self.last_target = name;
            }
        }

        if let Some(mut crosshair) = self.base().try_get_node_as::<ColorRect>("%ColorRect") {
            if looking_at_item {
                crosshair.set_color(Color::from_rgb(1.0, 0.85, 0.0)); // жёлтый — предмет
            } else if looking_at_dialogue {
                crosshair.set_color(Color::from_rgb(0.3, 0.7, 1.0)); // голубой — диалог
            } else {
                crosshair.set_color(Color::from_rgb(1.0, 1.0, 1.0)); // белый — обычное состояние
            }
        }
    }

    fn try_interact(&mut self) {
        if !self.base().is_on_floor() {
            return;
        }

        if self.is_dialogue_active() {
            return;
        }

        if let Some(node) = self.raycast_item() {
            let (item_id, amount) = node
                .clone()
                .try_cast::<Pickupable>()
                .ok()
                .map(|p| {
                    let b = p.bind();
                    (b.item_id.clone(), b.amount.max(1) as i64)
                })
                .unwrap_or_else(|| (GString::from("cube"), 1));

            let mut node = node;
            node.queue_free();

            if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
                inventory.call(
                    "add_item_amount",
                    &[item_id.to_variant(), amount.to_variant()],
                );
            }
            return;
        }

        if let Some(node) = self.raycast_dialogue() {
            if let Ok(trigger) = node.try_cast::<DialogueTrigger>() {
                let (path, shop) = {
                    let b = trigger.bind();
                    (b.dialogue_path.clone(), b.shop_path.clone())
                };

                // Если разговариваем с NPC — он останавливается и поворачивается к игроку
                let mut owner = trigger.get_parent();
                while let Some(n) = owner {
                    match n.try_cast::<Npc>() {
                        Ok(mut npc) => {
                            let player_pos = self.base().get_global_position();
                            npc.bind_mut().begin_dialogue(player_pos);
                            break;
                        }
                        Err(n) => owner = n.get_parent(),
                    }
                }

                if path.is_empty() && !shop.is_empty() {
                    // Торговец без диалога - сразу открываем магазин
                    if let Some(mut shop_ui) = self.base().try_get_node_as::<ShopUi>("%ShopUI") {
                        shop_ui.bind_mut().open_shop(shop);
                    }
                } else if let Some(mut dialogue_box) =
                    self.base().try_get_node_as::<DialogueBox>("%DialogueBox")
                {
                    dialogue_box.bind_mut().set_shop(shop);
                    dialogue_box.bind_mut().start_dialogue(path);
                }
            }
        }
    }
}

fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
