use godot::prelude::*;

struct MyExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MyExtension {}
mod item_icon;
mod player;
mod inventory;
mod item_slot;
mod dialogue_trigger;
mod dialogue_box;
mod item_preview;
mod item_database;
mod pickupable;
mod hiding_slot;
mod model_util;
mod game_state;
mod shop_ui;
