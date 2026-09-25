use godot::classes::{
    CharacterBody3D, ICharacterBody3D, Camera3D, Input, ProjectSettings,
    InputEvent, InputEventMouseMotion, PhysicsRayQueryParameters3D, Node, ColorRect,
};
use godot::classes::input::MouseMode;
use godot::prelude::*;
use crate::dialogue_box::DialogueBox;
use crate::dialogue_trigger::DialogueTrigger;
use crate::pickupable::Pickupable;

#[derive(GodotClass)]
#[class(base=CharacterBody3D)]
struct Player {
    base: Base<CharacterBody3D>,
    speed: f32,
    jump_velocity: f32,
    mouse_sensitivity: f32,
    captured: bool,
}

#[godot_api]
impl ICharacterBody3D for Player {
    fn init(base: Base<CharacterBody3D>) -> Self {
        Self {
            base,
            speed: 5.0,
            jump_velocity: 4.5,
            mouse_sensitivity: 0.003,
            captured: false,
        }
    }

    fn ready(&mut self) {}

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if !self.captured {
            Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
            self.captured = true;
        }

        if self.is_dialogue_active() {
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
        if self.is_dialogue_active() {
            self.base_mut().set_velocity(Vector3::ZERO);
            self.base_mut().move_and_slide();
            self.update_crosshair();
            return;
        }

        let mut velocity = self.base().get_velocity();
        let delta = delta as f32;

        if !self.base().is_on_floor() {
            let gravity = ProjectSettings::singleton()
                .get_setting("physics/3d/default_gravity")
                .try_to::<f32>()
                .unwrap_or(9.8);
            velocity.y -= gravity * delta;
        }

        let input = Input::singleton();

        if input.is_action_just_pressed("ui_accept") && self.base().is_on_floor() {
            velocity.y = self.jump_velocity;
        }

        let input_dir = input.get_vector("ui_left", "ui_right", "ui_up", "ui_down");
        let basis = self.base().get_transform().basis;
        let direction = (basis * Vector3::new(input_dir.x, 0.0, input_dir.y)).normalized_or_zero();

        if direction != Vector3::ZERO {
            velocity.x = direction.x * self.speed;
            velocity.z = direction.z * self.speed;
        } else {
            velocity.x = move_toward(velocity.x, 0.0, self.speed);
            velocity.z = move_toward(velocity.z, 0.0, self.speed);
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
        self.base()
            .try_get_node_as::<DialogueBox>("%DialogueBox")
            .map(|d| d.bind().is_active())
            .unwrap_or(false)
    }

    /// Кастует луч вперёд от камеры и возвращает любой узел, в который попал луч
    fn raycast_hit(&self) -> Option<Gd<Node>> {
        let camera = self.base().try_get_node_as::<Camera3D>("Camera3D")?;

        let transform = camera.get_global_transform();
        let from = transform.origin;
        let forward = transform.basis * Vector3::new(0.0, 0.0, -1.0);
        let to = from + forward * 3.0;

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
        let looking_at_item = self.raycast_item().is_some();
        let looking_at_dialogue = self.raycast_dialogue().is_some();

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
            let item_id = node
                .clone()
                .try_cast::<Pickupable>()
                .ok()
                .map(|p| p.bind().item_id.clone())
                .unwrap_or_else(|| GString::from("cube"));

            let mut node = node;
            node.queue_free();

            if let Some(mut inventory) = self.base().try_get_node_as::<Node>("%inventory") {
                inventory.call("add_item_icon", &[item_id.to_variant()]);
            }
            return;
        }

        if let Some(node) = self.raycast_dialogue() {
            if let Ok(trigger) = node.try_cast::<DialogueTrigger>() {
                let path = trigger.bind().dialogue_path.clone();

                if let Some(mut dialogue_box) = self.base().try_get_node_as::<DialogueBox>("%DialogueBox") {
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
