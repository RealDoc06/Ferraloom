use crate::entity::player::ServerPlayer;
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::component::Persistent;

/// Port of EntitySelectorOptions' NBT snapshot of a ServerPlayer. Storage's
/// DataVersion envelope is absent, while a nonempty selected stack is included.
pub fn player_nbt(player: &ServerPlayer) -> ferraloom_protocol::Result<CompoundTag> {
    let mut tag = player.save();
    tag.remove("DataVersion");
    let selected = player.inventory.selected_item();
    if !selected.is_empty() {
        tag.insert("SelectedItem", selected.to_nbt()?);
    }
    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::GameType;
    use ferraloom_data::Item;
    use ferraloom_nbt::{Tag, compare_nbt};
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{Uuid, Vec3};
    #[derive(serde::Deserialize)]
    struct Vector {
        count: i32,
        pattern: String,
        inverted: bool,
        matches: bool,
    }
    #[test]
    fn selected_stack_and_absent_storage_envelope_match_java_selectors() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../../vanilla/player-nbt-vectors.json")).unwrap();
        for vector in vectors {
            let mut player = ServerPlayer::new(
                Uuid::NIL,
                "SelectorPlayer".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            if vector.count > 0 {
                player.inventory.set_selected_item(ItemStack::new(
                    Item::STONE,
                    vector.count,
                    Default::default(),
                ));
            }
            let actual = Tag::Compound(player_nbt(&player).unwrap());
            let pattern = ferraloom_nbt::parse(&vector.pattern).unwrap();
            assert_eq!(
                compare_nbt(Some(&pattern), Some(&actual), true) != vector.inverted,
                vector.matches,
                "{}, count {}, inverted {}",
                vector.pattern,
                vector.count,
                vector.inverted
            );
        }
    }
}
