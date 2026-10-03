
use godot::classes::{SubViewportContainer, ISubViewportContainer, Node3D, MeshInstance3D, Mesh, InputEvent, InputEventMouseButton, InputEventMouseMotion};
use godot::global::MouseButton;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=SubViewportContainer)]
pub struct ItemPreview {
    base: Base<SubViewportContainer>,
    rotating: bool,
    dragging: bool,
}

#[godot_api]
impl ISubViewportContainer for ItemPreview {
    fn init(base: Base<SubViewportContainer>) -> Self {
        Self { base, rotating: false, dragging: false }
    }

    fn process(&mut self, delta: f64) {
        if self.rotating && !self.dragging {
            if let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") {
                pivot.rotate_y(delta as f32 * 0.6);
            }
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mb) = event.clone().try_cast::<InputEventMouseButton>() {
            if mb.get_button_index() == MouseButton::LEFT {
                self.dragging = mb.is_pressed();
            }
        }

        if self.dragging {
            if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
                let rel = motion.get_relative();
                if let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") {
                    pivot.rotate_y(rel.x * 0.01);
                    pivot.rotate_x(rel.y * 0.01);
                }
            }
        }
    }
}

#[godot_api]
impl ItemPreview {
    #[func]
    pub fn set_rotating(&mut self, value: bool) {
        self.rotating = value;
    }

    #[func]
    pub fn set_mesh(&mut self, mesh: Gd<Mesh>) {
        let Some(mut mesh_instance) = self
            .base()
            .try_get_node_as::<MeshInstance3D>("SubViewport/Pivot/MeshHolder")
        else {
            return;
        };

        let aabb = mesh.get_aabb();
        let size = aabb.size;
        let max_dim = size.x.max(size.y).max(size.z);
        let scale = if max_dim > 0.0001 { 1.0 / max_dim } else { 1.0 };
        let center = aabb.position + aabb.size * 0.5;

        mesh_instance.set_mesh(&mesh);
        mesh_instance.set_position(-center * scale);
        mesh_instance.set_scale(Vector3::new(scale, scale, scale));
    }
}
