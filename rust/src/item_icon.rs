use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::light_3d::Param;
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Camera3D, Control, DirectionalLight3D, Label, Mesh, MeshInstance3D, Node, Node3D,
    PackedScene, ResourceLoader, StandardMaterial3D, SubViewport, Texture2D, TextureRect,
};
use godot::prelude::*;
use crate::item_database::{self, ItemData};
use crate::item_preview::primitive_mesh;

/// Разрешение статичного рендера предмета (px)
const THUMB_RES: i32 = 128;
/// Размер превью, которое тащится за курсором при drag-and-drop
const DRAG_PREVIEW_SIZE: f32 = 48.0;

/// Ракурс предмета: поворот вокруг вертикальной оси (радианы). ~1.0 ≈ 57° — вид сбоку
const VIEW_YAW: f32 = 1.0;
/// Радиус сферы, в которую вписывается предмет: чем больше число, тем крупнее иконка.
/// Предмет гарантированно не вылезает за края кадра при любом повороте (до ~1.0 безопасно).
const MODEL_RADIUS: f32 = 0.95;
/// Небольшой наклон предмета к камере (радианы)
const VIEW_TILT: f32 = 0.0;
/// Положение камеры и её наклон вниз (радианы); чем ближе камера, тем крупнее предмет
const CAM_POS: Vector3 = Vector3::new(0.0, 0.55, 1.7);
const CAM_PITCH: f32 = -0.31;

/// Есть ли у предмета описание (JSON), по которому можно построить картинку
pub fn has_icon(item_id: &GString) -> bool {
    item_database::load_item(&item_id.to_string()).is_some()
}

/// Текстовая заглушка: имя предмета из JSON (или сам id, если JSON не найден)
fn fallback_label(item_id: &GString) -> Gd<Label> {
    let name = item_database::load_item(&item_id.to_string())
        .map(|d| d.name)
        .unwrap_or_else(|| item_id.clone());

    let mut label = Label::new_alloc();
    label.set_text(&name);
    label
}

/// "#rrggbb" -> Color (серый, если не разобралось)
fn parse_hex_color(s: &str) -> Color {
    let h = s.trim().trim_start_matches('#');
    let part = |a: usize, b: usize| h.get(a..b).and_then(|p| u8::from_str_radix(p, 16).ok());
    if let (Some(r), Some(g), Some(b)) = (part(0, 2), part(2, 4), part(4, 6)) {
        return Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    }
    Color::from_rgb(0.8, 0.8, 0.8)
}

/// Границы всех мешей внутри узла (локальные трансформы, дерево сцены не нужно)
fn collect_bounds(
    node: &Gd<Node>,
    parent: Transform3D,
    min: &mut Vector3,
    max: &mut Vector3,
    any: &mut bool,
) {
    for child in node.get_children().iter_shared() {
        let mut xf = parent;
        if let Ok(n3) = child.clone().try_cast::<Node3D>() {
            xf = parent * n3.get_transform();
        }

        if let Ok(mi) = child.clone().try_cast::<MeshInstance3D>() {
            if mi.get_mesh().is_some() && mi.is_visible() {
                let aabb = mi.get_aabb();
                for i in 0..8 {
                    let corner = aabb.position
                        + Vector3::new(
                            if i & 1 != 0 { aabb.size.x } else { 0.0 },
                            if i & 2 != 0 { aabb.size.y } else { 0.0 },
                            if i & 4 != 0 { aabb.size.z } else { 0.0 },
                        );
                    let p = xf * corner;
                    *min = Vector3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                    *max = Vector3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                    *any = true;
                }
            }
        }

        collect_bounds(&child, xf, min, max, any);
    }
}

/// Строит модель предмета тем же способом, что и превью справа:
/// 1) 3D-сцена (glb) — из поля "scene" или модели, стоящей в мире;
/// 2) файл меша из поля "mesh";
/// 3) простая форма "shape" нужного цвета.
fn build_model(data: &ItemData) -> Gd<Node3D> {
    let mut holder = Node3D::new_alloc();
    let mut filled = false;

    if !data.scene_path.is_empty() && ResourceLoader::singleton().exists(&data.scene_path) {
        if let Ok(scene) = try_load::<PackedScene>(&data.scene_path) {
            if let Some(instance) = scene.instantiate() {
                holder.add_child(&instance);
                filled = true;
            }
        }
    }

    if !filled && !data.mesh_path.is_empty() && ResourceLoader::singleton().exists(&data.mesh_path) {
        if let Ok(mesh) = try_load::<Mesh>(&data.mesh_path) {
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_mesh(&mesh);
            holder.add_child(&mi);
            filled = true;
        }
    }

    if !filled {
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&primitive_mesh(&data.shape.to_string()));
        if !data.color.is_empty() {
            let mut material = StandardMaterial3D::new_gd();
            material.set_albedo(parse_hex_color(&data.color.to_string()));
            mi.set_material_override(&material);
        }
        holder.add_child(&mi);
    }

    // Центрируем и нормируем размер: описанная сфера предмета = MODEL_RADIUS
    let mut min = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vector3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;
    collect_bounds(&holder.clone().upcast::<Node>(), Transform3D::IDENTITY, &mut min, &mut max, &mut any);

    if any {
        let size = max - min;
        let radius = size.length() * 0.5;
        let scale = if radius > 0.0001 { MODEL_RADIUS / radius } else { 1.0 };
        let center = (min + max) * 0.5;
        holder.set_scale(Vector3::new(scale, scale, scale));
        holder.set_position(-center * scale);
    }

    holder
}

/// Строит TextureRect, внутри которого лежит SubViewport с моделью.
/// Вьюпорт рендерится ОДИН раз (UpdateMode::ONCE) — получается статичная картинка.
fn build_thumbnail(model: &Gd<Node3D>) -> Option<Gd<TextureRect>> {
    let mut viewport = SubViewport::new_alloc();
    viewport.set_size(Vector2i::new(THUMB_RES, THUMB_RES));
    viewport.set_transparent_background(true);
    viewport.set_use_own_world_3d(true);
    viewport.set_handle_input_locally(false);
    viewport.set_update_mode(UpdateMode::ONCE);

    // Камера
    let mut camera = Camera3D::new_alloc();
    camera.set_position(CAM_POS);
    camera.set_rotation(Vector3::new(CAM_PITCH, 0.0, 0.0));
    camera.set_current(true);
    viewport.add_child(&camera);

    // Основной свет (спереди-сверху) и слабый заполняющий (с другой стороны)
    let mut key_light = DirectionalLight3D::new_alloc();
    key_light.set_rotation(Vector3::new(-0.7, 0.5, 0.0));
    viewport.add_child(&key_light);

    let mut fill_light = DirectionalLight3D::new_alloc();
    fill_light.set_rotation(Vector3::new(-0.4, -2.2, 0.0));
    fill_light.set_param(Param::ENERGY, 0.45);
    viewport.add_child(&fill_light);

    // Модель в боковом ракурсе
    let mut pivot = Node3D::new_alloc();
    pivot.set_rotation(Vector3::new(VIEW_TILT, VIEW_YAW, 0.0));
    pivot.add_child(model);
    viewport.add_child(&pivot);

    // Показываем результат рендера как обычную текстуру
    let tex = viewport.get_texture()?;

    let mut rect = TextureRect::new_alloc();
    rect.set_texture(&tex.upcast::<Texture2D>());
    rect.set_expand_mode(ExpandMode::IGNORE_SIZE);
    rect.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    // Клики проходят сквозь картинку к слоту (выбор, правая кнопка, drag-and-drop)
    rect.set_mouse_filter(MouseFilter::IGNORE);
    rect.add_child(&viewport);
    Some(rect)
}

fn make_thumbnail_for(item_id: &GString) -> Option<Gd<TextureRect>> {
    let data = item_database::load_item(&item_id.to_string())?;
    let model = build_model(&data);
    build_thumbnail(&model)
}

/// Визуал предмета для слота: статичный рендер 3D-модели на весь слот
/// либо текстовая заглушка, если описания предмета нет.
pub fn make_item_visual(item_id: &GString) -> Gd<Control> {
    if let Some(mut rect) = make_thumbnail_for(item_id) {
        rect.set_anchors_preset(LayoutPreset::FULL_RECT);
        return rect.upcast::<Control>();
    }
    fallback_label(item_id).upcast::<Control>()
}

/// Превью для перетаскивания: картинка фиксированного размера либо заглушка
pub fn make_drag_preview(item_id: &GString) -> Gd<Control> {
    if let Some(mut rect) = make_thumbnail_for(item_id) {
        rect.set_custom_minimum_size(Vector2::new(DRAG_PREVIEW_SIZE, DRAG_PREVIEW_SIZE));
        return rect.upcast::<Control>();
    }
    fallback_label(item_id).upcast::<Control>()
}
