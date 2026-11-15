use godot::classes::{IMarker3D, Marker3D, Node3D};
use godot::prelude::*;

/// Контрольная точка комнаты. Ставится на карте в центре комнаты.
///
/// Внутри можно создать дочерние Marker3D с именем = npc_id нужного NPC:
/// это индивидуальное место, где именно этот NPC останавливается в комнате
/// (и куда он смотрит: он повернётся в сторону -Z маркера).
/// Если для NPC маркера нет, он остановится в самой контрольной точке.
#[derive(GodotClass)]
#[class(base=Marker3D)]
pub struct RoomPoint {
    base: Base<Marker3D>,
    /// Название комнаты, по которому её находят NPC ("cafeteria", "ward_a" ...)
    #[export]
    pub room_id: GString,
}

#[godot_api]
impl IMarker3D for RoomPoint {
    fn init(base: Base<Marker3D>) -> Self {
        Self {
            base,
            room_id: GString::new(),
        }
    }

    fn ready(&mut self) {
        // Все комнаты в одной группе, чтобы NPC могли их найти без путей
        self.base_mut().add_to_group("room_point");
    }
}

#[godot_api]
impl RoomPoint {
    /// Где в этой комнате должен встать NPC с таким id: (мировая позиция, поворот по Y)
    pub fn stop_for(&self, npc_id: &str) -> (Vector3, f32) {
        if !npc_id.is_empty() {
            if let Some(spot) = self.base().try_get_node_as::<Node3D>(npc_id) {
                return (spot.get_global_position(), spot.get_global_rotation().y);
            }
        }
        (
            self.base().get_global_position(),
            self.base().get_global_rotation().y,
        )
    }
}
