//! Focused production codec/admission tests. Authored source-only; UNRUN.

use super::*;
use crate::{
    data_component::DataComponent,
    data_component_impl::{CustomDataImpl, read_data},
    item::Item,
    item_stack::{
        ItemStack,
        admission::{validate_item_stack, validate_runtime_item_stack},
    },
    jukebox_song::JukeboxSong,
};

fn saved_disc(value: NbtTag) -> NbtCompound {
    let mut components = NbtCompound::new();
    components.put("minecraft:jukebox_playable", value);
    let mut item = NbtCompound::new();
    item.put_string("id", "minecraft:music_disc_11".into());
    item.put_compound("components", components);
    item
}

#[test]
fn all_builtin_references_survive_component_item_and_admission_paths() {
    let mut hashes = std::collections::HashSet::new();
    for song in JukeboxSong::all() {
        let canonical = NbtTag::String(song.asset_id().into());
        let decoded = read_data(DataComponent::JukeboxPlayable, &canonical).unwrap();
        assert_eq!(decoded.write_data(), canonical);
        assert!(hashes.insert(decoded.get_hash()));
        // Bytecode-derived HashOps Identifier contract, not an executed Java golden.
        assert_eq!(decoded.get_hash(), get_str_hash(song.asset_id()) as i32);
        let empty_namespace = format!(":{}", song.to_name());
        for name in [song.to_name(), song.asset_id(), empty_namespace.as_str()] {
            let saved = saved_disc(NbtTag::String(name.into()));
            assert_eq!(validate_item_stack(&saved), Ok(()));
            let stack = ItemStack::read_item_stack(&saved).unwrap();
            assert_eq!(validate_runtime_item_stack(&stack), Ok(()));
            assert_eq!(
                stack
                    .get_data_component::<JukeboxPlayableImpl>()
                    .unwrap()
                    .song,
                *song
            );
            let mut written = NbtCompound::new();
            stack.write_item_stack(&mut written);
            assert_eq!(
                written
                    .get_compound("components")
                    .unwrap()
                    .get("minecraft:jukebox_playable"),
                Some(&canonical)
            );
            let reopened = ItemStack::read_item_stack(&written).unwrap();
            assert!(stack.are_equal(&reopened));
            // The lossless guard normalizes its own copy, never caller-owned NBT.
            assert_eq!(
                saved
                    .get_compound("components")
                    .unwrap()
                    .get_string("minecraft:jukebox_playable"),
                Some(name)
            );
        }
    }
}

#[test]
fn all_disc_defaults_and_effective_override_removal_are_independent_of_opaque_data() {
    let discs = [
        &Item::MUSIC_DISC_11,
        &Item::MUSIC_DISC_13,
        &Item::MUSIC_DISC_5,
        &Item::MUSIC_DISC_BLOCKS,
        &Item::MUSIC_DISC_BOUNCE,
        &Item::MUSIC_DISC_CAT,
        &Item::MUSIC_DISC_CHIRP,
        &Item::MUSIC_DISC_CREATOR,
        &Item::MUSIC_DISC_CREATOR_MUSIC_BOX,
        &Item::MUSIC_DISC_FAR,
        &Item::MUSIC_DISC_LAVA_CHICKEN,
        &Item::MUSIC_DISC_MALL,
        &Item::MUSIC_DISC_MELLOHI,
        &Item::MUSIC_DISC_OTHERSIDE,
        &Item::MUSIC_DISC_PIGSTEP,
        &Item::MUSIC_DISC_PRECIPICE,
        &Item::MUSIC_DISC_RELIC,
        &Item::MUSIC_DISC_STAL,
        &Item::MUSIC_DISC_STRAD,
        &Item::MUSIC_DISC_TEARS,
        &Item::MUSIC_DISC_WAIT,
        &Item::MUSIC_DISC_WARD,
    ];
    for (disc, song) in discs.into_iter().zip(JukeboxSong::all()) {
        let stack = ItemStack::new(1, disc);
        assert!(stack.patch.is_empty());
        assert_eq!(
            stack
                .get_data_component::<JukeboxPlayableImpl>()
                .unwrap()
                .song,
            *song
        );
        let mut saved = NbtCompound::new();
        stack.write_item_stack(&mut saved);
        assert!(saved.get_compound("components").is_none());
        let reopened = ItemStack::read_item_stack(&saved).unwrap();
        assert_eq!(
            reopened
                .get_data_component::<JukeboxPlayableImpl>()
                .unwrap()
                .song,
            *song
        );
    }
    let mut stack = ItemStack::new(1, &Item::MUSIC_DISC_11);
    let mut opaque = NbtCompound::new();
    opaque.put_string("minecraft:jukebox_playable", "ward".into());
    let mut nested = NbtCompound::new();
    nested.put_compound("components", opaque.clone());
    opaque.put_compound("nested_item_like_data", nested);
    stack.set_data_component(CustomDataImpl::new(opaque.clone()));
    assert_eq!(
        stack
            .get_data_component::<JukeboxPlayableImpl>()
            .unwrap()
            .song,
        JukeboxSong::Id11
    );
    stack.set_data_component(JukeboxPlayableImpl {
        song: JukeboxSong::Cat,
    });
    assert_eq!(
        stack
            .get_data_component::<JukeboxPlayableImpl>()
            .unwrap()
            .song,
        JukeboxSong::Cat
    );
    stack.remove_data_component(DataComponent::JukeboxPlayable);
    let mut saved = NbtCompound::new();
    stack.write_item_stack(&mut saved);
    assert_eq!(validate_item_stack(&saved), Ok(()));
    assert_eq!(validate_runtime_item_stack(&stack), Ok(()));
    let reopened = ItemStack::read_item_stack(&saved).unwrap();
    assert!(
        reopened
            .get_data_component::<JukeboxPlayableImpl>()
            .is_none()
    );
    assert_eq!(
        reopened
            .get_data_component::<CustomDataImpl>()
            .unwrap()
            .data,
        opaque
    );
    assert!(stack.are_equal(&reopened));
}

#[test]
fn unsupported_references_and_non_string_persistent_values_refuse_without_defaults() {
    let mut old_wrapper = NbtCompound::new();
    old_wrapper.put_string("song", "minecraft:11".into());
    for value in [
        NbtTag::String("custom:11".into()),
        NbtTag::String("minecraft:missing".into()),
        NbtTag::String("Minecraft:11".into()),
        NbtTag::String("::11".into()),
        NbtTag::String("minecraft::11".into()),
        NbtTag::String("".into()),
        NbtTag::Int(1),
        NbtTag::Compound(old_wrapper),
        NbtTag::Compound(NbtCompound::new()),
    ] {
        assert!(read_data(DataComponent::JukeboxPlayable, &value).is_none());
        let saved = saved_disc(value);
        assert!(ItemStack::read_item_stack(&saved).is_none());
        assert!(validate_item_stack(&saved).is_err());
    }
}
