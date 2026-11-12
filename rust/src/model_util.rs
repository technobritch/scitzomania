use godot::classes::{MeshInstance3D, Node, Node3D};
use godot::prelude::*;

/// Собирает все MeshInstance3D внутри узла (рекурсивно).
pub fn collect_meshes(node: &Gd<Node>, out: &mut Vec<Gd<MeshInstance3D>>) {
    for child in node.get_children().iter_shared() {
        if let Ok(mi) = child.clone().try_cast::<MeshInstance3D>() {
            out.push(mi);
        }
        collect_meshes(&child, out);
    }
}

/// Границы всех мешей внутри root в системе координат root: (min, max).
pub fn meshes_bounds(root: &Gd<Node3D>) -> Option<(Vector3, Vector3)> {
    let mut meshes = Vec::new();
    collect_meshes(&root.clone().upcast::<Node>(), &mut meshes);

    let inv = root.get_global_transform().affine_inverse();
    let mut min = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vector3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;

    for mi in meshes {
        if mi.get_mesh().is_none() {
            continue;
        }
        let aabb = mi.get_aabb();
        let xf = inv * mi.get_global_transform();

        for i in 0..8 {
            let corner = aabb.position
                + Vector3::new(
                    if i & 1 != 0 { aabb.size.x } else { 0.0 },
                    if i & 2 != 0 { aabb.size.y } else { 0.0 },
                    if i & 4 != 0 { aabb.size.z } else { 0.0 },
                );
            let p = xf * corner;
            min = Vector3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vector3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
            any = true;
        }
    }

    if any { Some((min, max)) } else { None }
}
