//! Production item framing and song holder tests. Authored source-only; UNRUN.

use super::*;
use crate::codec::item_stack_seralizer::ItemStackSerializer;
use pumpkin_data::{item::Item, item_stack::ItemStack, jukebox_song::JukeboxSong};

#[test]
fn all_song_references_preserve_following_item_component_and_round_trip() {
    for (id, song) in JukeboxSong::all().iter().enumerate() {
        let stack = ItemStack::new_with_component(
            1,
            &Item::MUSIC_DISC_11,
            vec![
                (
                    DataComponent::JukeboxPlayable,
                    Some(JukeboxPlayableImpl { song: *song }.to_dyn()),
                ),
                (
                    DataComponent::Damage,
                    Some(DamageImpl { damage: 300 }.to_dyn()),
                ),
            ],
        );
        let mut bytes = Vec::new();
        ItemStackSerializer(Cow::Borrowed(&stack))
            .write_with_version(&mut bytes, &JavaMinecraftVersion::V_26_3)
            .unwrap();
        let mut reader = bytes.as_slice();
        assert_eq!(reader.get_var_int().unwrap().0, 1);
        assert_eq!(
            reader.get_var_int().unwrap().0,
            i32::from(Item::MUSIC_DISC_11.id)
        );
        assert_eq!(reader.get_var_int().unwrap().0, 2);
        assert_eq!(reader.get_var_int().unwrap().0, 0);
        assert_eq!(
            reader.get_var_int().unwrap().0,
            i32::from(DataComponent::JukeboxPlayable.to_id())
        );
        assert_eq!(reader[0], u8::try_from(id + 1).unwrap());
        let decoded = deserialize(DataComponent::JukeboxPlayable, &mut reader).unwrap();
        assert_eq!(decoded.write_data(), NbtTag::String(song.asset_id().into()));
        assert_eq!(
            reader.get_var_int().unwrap().0,
            i32::from(DataComponent::Damage.to_id())
        );
        assert_eq!(
            deserialize(DataComponent::Damage, &mut reader)
                .unwrap()
                .write_data(),
            NbtTag::Int(300)
        );
        assert!(reader.is_empty());
        let mut rewritten = Vec::new();
        serialize(
            DataComponent::JukeboxPlayable,
            decoded.as_ref(),
            &mut rewritten,
        )
        .unwrap();
        assert_eq!(rewritten, [u8::try_from(id + 1).unwrap()]);
    }
}

#[test]
fn direct_unknown_negative_and_truncated_holders_error_without_synthetic_song() {
    for holder in [0, -1, i32::MIN, 23, i32::MAX] {
        let mut bytes = Vec::new();
        bytes.write_var_int(&VarInt(holder)).unwrap();
        let marker_end = bytes.len();
        bytes.extend_from_slice(&[0x5a, 0xa5]);
        let mut reader = bytes.as_slice();
        assert!(matches!(
            deserialize(DataComponent::JukeboxPlayable, &mut reader),
            Err(ReadingError::Message(_))
        ));
        assert_eq!(reader, &bytes[marker_end..]);
    }
    for bytes in [
        &[][..],
        &[0x80][..],
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00][..],
    ] {
        assert!(deserialize(DataComponent::JukeboxPlayable, &mut &bytes[..]).is_err());
    }
    // A complete official direct payload is valid upstream but outside this
    // bounded reference projection: marker 0, sound reference, text, float, VarInt.
    let mut direct = vec![0];
    direct.write_var_int(&VarInt(1)).unwrap();
    direct
        .write_nbt(NbtTag::String("Direct song".into()))
        .unwrap();
    direct.extend_from_slice(&71.0_f32.to_be_bytes());
    direct.write_var_int(&VarInt(11)).unwrap();
    let mut reader = direct.as_slice();
    assert!(deserialize(DataComponent::JukeboxPlayable, &mut reader).is_err());
    assert_eq!(reader, &direct[1..]);
}
