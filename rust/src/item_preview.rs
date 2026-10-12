use godot::classes::{SubViewportContainer, ISubViewportContainer, Node3D, MeshInstance3D, Mesh};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=SubViewportContainer)]
pub struct ItemPreview {
    base: Base<SubViewportContainer>,
    rotating: bool,
    time: f32,
}

#[godot_api]
impl ISubViewportContainer for ItemPreview {
    fn init(base: Base<SubViewportContainer>) -> Self {
        Self { base, rotating: false, time: 0.0 }
    }

    fn process(&mut self, delta: f64) {
        if !self.rotating {
            return;
        }

        self.time += delta as f32;

        if let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") {
            pivot.rotate_y(delta as f32 * 1.0);

            let bob = (self.time * 1.8).sin() * 0.08;
            let mut pos = pivot.get_position();
            pos.y = bob;
            pivot.set_position(pos);
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
