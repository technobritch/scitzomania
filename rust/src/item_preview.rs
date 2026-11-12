use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::{
    BoxMesh, CapsuleMesh, CylinderMesh, ISubViewportContainer, Mesh, MeshInstance3D, Node3D,
    SphereMesh, StandardMaterial3D, SubViewportContainer,
};
use godot::prelude::*;
use godot::classes::PackedScene;
use crate::model_util::meshes_bounds;

/// Простая форма для предмета, у которого нет своего файла меша.
pub fn primitive_mesh(shape: &str) -> Gd<Mesh> {
    match shape {
        "sphere" => SphereMesh::new_gd().upcast::<Mesh>(),
        "capsule" => CapsuleMesh::new_gd().upcast::<Mesh>(),
        "cylinder" => CylinderMesh::new_gd().upcast::<Mesh>(),
        _ => BoxMesh::new_gd().upcast::<Mesh>(),
    }
}

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

    fn ready(&mut self) {
        // Без stretch внутренний вьюпорт не подгоняется под размер контейнера
        self.base_mut().set_stretch(true);
        self.base_mut().set_custom_minimum_size(Vector2::new(220.0, 220.0));
        self.base_mut().set_h_size_flags(SizeFlags::EXPAND_FILL);
        self.base_mut().set_v_size_flags(SizeFlags::EXPAND_FILL);
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
    }

    fn process(&mut self, delta: f64) {
        if !self.rotating {
            return;
        }

        self.time += delta as f32;

        if let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") {
            pivot.rotate_y(delta as f32 * 1.0);

            let bob = (self.time * 1.8).sin() * 0.12;
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

    /// Показать предмет или спрятать (без изменения раскладки)
    #[func]
    pub fn show_item(&mut self, visible: bool) {
        let color = if visible {
            Color::from_rgba(1.0, 1.0, 1.0, 1.0)
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.0)
        };
        self.base_mut().set_modulate(color);
    }

    #[func]
    pub fn set_color(&mut self, color: Color) {
        let Some(mut mesh_instance) = self
            .base()
            .try_get_node_as::<MeshInstance3D>("SubViewport/Pivot/MeshHolder")
        else {
            return;
        };

        let mut material = StandardMaterial3D::new_gd();
        material.set_albedo(color);
        mesh_instance.set_material_override(&material);
    }

    /// Убирает ранее показанную 3D-модель и возвращает обычный MeshHolder
    fn clear_model(&mut self) {
        if let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") {
            if let Some(mut old) = pivot.try_get_node_as::<Node3D>("ModelRoot") {
                pivot.remove_child(&old);
                old.queue_free();
            }
        }
        if let Some(mut holder) = self
            .base()
            .try_get_node_as::<MeshInstance3D>("SubViewport/Pivot/MeshHolder")
        {
            holder.set_visible(true);
        }
    }

    /// Показывает целую 3D-модель (glb/gltf) вместо одного меша.
    pub fn set_scene(&mut self, scene: Gd<PackedScene>) -> bool {
        let Some(mut pivot) = self.base().try_get_node_as::<Node3D>("SubViewport/Pivot") else {
            return false;
        };
        self.clear_model();

        let Some(instance) = scene.instantiate() else {
            return false;
        };

        let mut holder = Node3D::new_alloc();
        holder.set_name("ModelRoot");
        pivot.add_child(&holder);
        holder.add_child(&instance);

        if let Some(mut mesh_holder) = self
            .base()
            .try_get_node_as::<MeshInstance3D>("SubViewport/Pivot/MeshHolder")
        {
            mesh_holder.set_visible(false);
        }

        // Подгоняем размер и центрируем по границам всех мешей модели
        if let Some((min, max)) = meshes_bounds(&holder) {
            let size = max - min;
            let max_dim = size.x.max(size.y).max(size.z);
            let scale = if max_dim > 0.0001 { 2.0 / max_dim } else { 1.0 };
            let center = (min + max) * 0.5;
            holder.set_scale(Vector3::new(scale, scale, scale));
            holder.set_position(-center * scale);
        }
        true
    }

    #[func]
    pub fn set_mesh(&mut self, mesh: Gd<Mesh>) {
        self.clear_model();
        let Some(mut mesh_instance) = self
            .base()
            .try_get_node_as::<MeshInstance3D>("SubViewport/Pivot/MeshHolder")
        else {
            return;
        };

        let aabb = mesh.get_aabb();
        let size = aabb.size;
        let max_dim = size.x.max(size.y).max(size.z);
        let scale = if max_dim > 0.0001 { 2.0 / max_dim } else { 1.0 };
        let center = aabb.position + aabb.size * 0.5;

        mesh_instance.set_mesh(&mesh);
        mesh_instance.set_position(-center * scale);
        mesh_instance.set_scale(Vector3::new(scale, scale, scale));
    }
}
