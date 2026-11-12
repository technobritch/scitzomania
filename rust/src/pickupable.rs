use godot::classes::{
    BoxShape3D, CollisionShape3D, ConvexPolygonShape3D, IStaticBody3D, MeshInstance3D, Node,
    Node3D, StaticBody3D,
};
use godot::prelude::*;
use crate::item_database;
use crate::model_util::collect_meshes;

/// Насколько зона попадания больше самой модели (1.0 = точно по форме)
const AIM_ASSIST: f32 = 1.15;
/// Минимальная толщина зоны попадания по любой оси, в метрах
const MIN_THICKNESS: f32 = 0.06;

#[derive(GodotClass)]
#[class(tool, base=StaticBody3D)]
pub struct Pickupable {
    base: Base<StaticBody3D>,
    #[export]
    pub item_id: GString,
    /// Сколько штук даёт подбор (для сигарет: пачка из нескольких)
    #[export]
    pub amount: i32,
}

#[godot_api]
impl IStaticBody3D for Pickupable {
    fn init(base: Base<StaticBody3D>) -> Self {
        Self {
            base,
            item_id: GString::from("cube"),
            amount: 1,
        }
    }

    fn ready(&mut self) {
        // Предмет всегда в группе "item" - руками добавлять не нужно
        self.base_mut().add_to_group("item");

        let id = self.item_id.to_string();
        let me: Gd<Node> = self.to_gd().upcast::<Node>();

        let mut meshes: Vec<Gd<MeshInstance3D>> = Vec::new();
        collect_meshes(&me, &mut meshes);

        // Запоминаем, какая 3D-модель стоит у этого предмета в мире,
        // чтобы показать её же в превью инвентаря
        if let Some(path) = find_model_path(&me) {
            item_database::register_model_path(&id, &path.to_string());
        }

        // Если коллизия уже есть (сделана руками) - ничего не трогаем
        let has_shape = me
            .get_children()
            .iter_shared()
            .any(|c| c.try_cast::<CollisionShape3D>().is_ok());
        if has_shape {
            godot_print!("[Pickupable] '{}': коллизия уже есть, мешей в модели: {}", id, meshes.len());
            return;
        }

        // Строим зону попадания по реальной форме каждого меша модели
        let me3d: Gd<Node3D> = self.to_gd().upcast::<Node3D>();
        let inv = me3d.get_global_transform().affine_inverse();
        let mut built = 0;
        let mut zone_min = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut zone_max = Vector3::new(f32::MIN, f32::MIN, f32::MIN);

        for mi in &meshes {
            let Some(mesh) = mi.get_mesh() else {
                continue;
            };
            if !mi.is_visible_in_tree() {
                continue;
            }
            let xf = inv * mi.get_global_transform();

            // Выпуклая оболочка меша; если не получилось - 8 углов его границ
            let local_points: Vec<Vector3> = match mesh.create_convex_shape() {
                Some(hull) => hull.get_points().as_slice().to_vec(),
                None => {
                    let aabb = mesh.get_aabb();
                    (0..8)
                        .map(|i| {
                            aabb.position
                                + Vector3::new(
                                    if i & 1 != 0 { aabb.size.x } else { 0.0 },
                                    if i & 2 != 0 { aabb.size.y } else { 0.0 },
                                    if i & 4 != 0 { aabb.size.z } else { 0.0 },
                                )
                        })
                        .collect()
                }
            };
            if local_points.is_empty() {
                continue;
            }

            let in_item_space: Vec<Vector3> = local_points.iter().map(|p| xf * *p).collect();
            let points = inflate(&in_item_space);
            for p in &points {
                zone_min = Vector3::new(zone_min.x.min(p.x), zone_min.y.min(p.y), zone_min.z.min(p.z));
                zone_max = Vector3::new(zone_max.x.max(p.x), zone_max.y.max(p.y), zone_max.z.max(p.z));
            }

            let mut shape = ConvexPolygonShape3D::new_gd();
            shape.set_points(&PackedVector3Array::from(points));

            let mut collision = CollisionShape3D::new_alloc();
            collision.set_shape(&shape);
            self.base_mut().add_child(&collision);
            built += 1;
        }

        if built == 0 {
            godot_print!(
                "[Pickupable] '{}': внутри не найдено ни одного меша! Модель должна лежать ВНУТРИ этого узла. Ставлю коробку 0.4 по умолчанию",
                id
            );
            let mut shape = BoxShape3D::new_gd();
            shape.set_size(Vector3::new(0.4, 0.4, 0.4));
            let mut collision = CollisionShape3D::new_alloc();
            collision.set_shape(&shape);
            self.base_mut().add_child(&collision);
            return;
        }

        let size = zone_max - zone_min;
        godot_print!(
            "[Pickupable] '{}': зона попадания по форме модели, частей: {}, размер {:.2} x {:.2} x {:.2} м",
            id,
            built,
            size.x,
            size.y,
            size.z
        );
    }
}

/// Чуть раздувает облако точек вокруг его центра и не даёт ему быть тоньше MIN_THICKNESS
fn inflate(points: &[Vector3]) -> Vec<Vector3> {
    let mut min = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vector3::new(f32::MIN, f32::MIN, f32::MIN);
    for p in points {
        min = Vector3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Vector3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    let center = (min + max) * 0.5;
    let size = max - min;

    let factor = |side: f32| -> f32 {
        if side > 0.0001 {
            AIM_ASSIST.max(MIN_THICKNESS / side)
        } else {
            1.0
        }
    };
    let (fx, fy, fz) = (factor(size.x), factor(size.y), factor(size.z));

    points
        .iter()
        .map(|p| {
            Vector3::new(
                center.x + (p.x - center.x) * fx,
                center.y + (p.y - center.y) * fy,
                center.z + (p.z - center.z) * fz,
            )
        })
        .collect()
}

fn find_model_path(node: &Gd<Node>) -> Option<GString> {
    for child in node.get_children().iter_shared() {
        let path = child.get_scene_file_path();
        if !path.is_empty() {
            return Some(path);
        }
        if let Some(p) = find_model_path(&child) {
            return Some(p);
        }
    }
    None
}
