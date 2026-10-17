use super::*;
use ferraloom_util::{BlockPos, Uuid};

fn hex(text: &str) -> Vec<u8> {
    text.trim()
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn vanilla_primitive_bytes() {
    let bytes = hex(include_str!("../../tests/fixtures/primitives.hex"));
    let mut r = Reader::new(&bytes);
    let mut w = Writer::new();
    for expected in [0, 1, 127, 128, 255, i32::MAX, -1, i32::MIN] {
        let actual = r.var_int().unwrap();
        assert_eq!(actual, expected);
        w.var_int(actual);
    }
    for expected in [0, 127, 128, i64::MAX, -1, i64::MIN] {
        let actual = r.var_long().unwrap();
        assert_eq!(actual, expected);
        w.var_long(actual);
    }
    let text = r.string(4).unwrap();
    assert_eq!(text, "a🚀é");
    w.string(&text, 4).unwrap();
    let pos = BlockPos::decode(&mut r).unwrap();
    assert_eq!(pos, BlockPos::new(-33554432, -2048, 33554431));
    w.write(&pos).unwrap();
    let uuid = Uuid::decode(&mut r).unwrap();
    assert_eq!(uuid.to_string(), "01234567-89ab-cdef-fedc-ba9876543210");
    w.write(&uuid).unwrap();
    let bits = BitSet::decode(&mut r).unwrap();
    assert!(bits.get(65));
    w.write(&bits).unwrap();
    let fixed = BitSet::decode_fixed(&mut r, 70).unwrap();
    assert_eq!(fixed, bits);
    fixed.encode_fixed(&mut w, 70).unwrap();
    let nbt = r.nbt(false).unwrap();
    w.nbt(&nbt).unwrap();
    r.finish().unwrap();
    assert_eq!(w.as_slice(), bytes);
}
#[test]
fn vanilla_maximum_utf16_string() {
    let bytes = hex(include_str!("../../tests/fixtures/max_string.hex"));
    let mut r = Reader::new(&bytes);
    let value = r.string(32767).unwrap();
    assert_eq!(value.encode_utf16().count(), 32767);
    let mut w = Writer::new();
    w.string(&value, 32767).unwrap();
    assert_eq!(w.as_slice(), bytes);
    assert!(Reader::new(&bytes).string(32766).is_err());
    assert!(
        Writer::new()
            .string(&(value.into_owned() + "x"), 32767)
            .is_err()
    );
}
#[test]
fn variable_integer_limits_and_java_truncation() {
    assert_eq!(
        Reader::new(&[255, 255, 255, 255, 127]).var_int().unwrap(),
        -1
    );
    assert_eq!(
        Reader::new(&[255, 255, 255, 255, 255, 255, 255, 255, 255, 127])
            .var_long()
            .unwrap(),
        -1
    );
    assert!(matches!(
        Reader::new(&[128; 6]).var_int(),
        Err(Error::VarIntTooBig)
    ));
    assert!(matches!(
        Reader::new(&[128; 11]).var_long(),
        Err(Error::VarLongTooBig)
    ));
    assert!(matches!(
        Reader::new(&[128; 5]).var_int(),
        Err(Error::UnexpectedEof)
    ));
}

#[test]
fn vanilla_malformed_utf8_replacement_groups() {
    for (input, normalized) in [
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_0.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_0.hex"),
        ),
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_1.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_1.hex"),
        ),
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_2.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_2.hex"),
        ),
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_3.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_3.hex"),
        ),
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_4.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_4.hex"),
        ),
        (
            include_str!("../../tests/fixtures/malformed_utf8_input_5.hex"),
            include_str!("../../tests/fixtures/malformed_utf8_normalized_5.hex"),
        ),
    ] {
        let input = hex(input);
        let mut r = Reader::new(&input);
        let text = r.string(16).unwrap();
        let mut w = Writer::new();
        w.string(&text, 16).unwrap();
        assert_eq!(w.as_slice(), hex(normalized));
    }
    assert_eq!(Reader::new(&[3, 0xed, 0xa0, 0x80]).string(1).unwrap(), "�");
}

#[test]
fn vanilla_registry_ids_holders_and_holder_sets() {
    use ferraloom_data::{Block, registries::sound_event};
    struct Context;
    impl RegistryContext for Context {
        fn entry_id(&self, registry: &Identifier, key: &Identifier) -> Option<i32> {
            match registry.as_str() {
                "minecraft:block" => {
                    Block::from_name(key.as_str()).map(|v| i32::from(v.protocol_id()))
                }
                "minecraft:sound_event" => {
                    sound_event::Id::from_name(key.as_str()).map(|v| i32::from(v.protocol_id()))
                }
                _ => None,
            }
        }
        fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
            let id = u16::try_from(id).ok()?;
            let name = match registry.as_str() {
                "minecraft:block" => Block::from_protocol_id(id)?.name(),
                "minecraft:sound_event" => sound_event::Id::from_protocol_id(id)?.name(),
                _ => return None,
            };
            Identifier::parse(name).ok()
        }
        fn has_tag(&self, registry: &Identifier, key: &Identifier) -> bool {
            registry.as_str() == "minecraft:block" && key.as_str() == "test:named"
        }
    }
    #[derive(Debug, PartialEq)]
    struct Sound {
        id: Identifier,
        range: Option<f32>,
    }
    impl Wire for Sound {
        fn decode(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Self {
                id: r.read()?,
                range: r.read()?,
            })
        }
        fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
            w.write(&self.id)?;
            w.write(&self.range)
        }
    }
    let bytes = hex(include_str!("../../tests/fixtures/registry_holders.hex"));
    let block = Identifier::parse("minecraft:block").unwrap();
    let sound = Identifier::parse("minecraft:sound_event").unwrap();
    let mut r = Reader::with_context(&bytes, &Context);
    let mut w = Writer::with_context(&Context);
    let stone = r.registry(&block).unwrap();
    assert_eq!(stone.as_str(), "minecraft:stone");
    w.registry(&block, &stone).unwrap();
    let reference = Holder::<Sound>::decode(&mut r, &sound).unwrap();
    assert_eq!(
        reference,
        Holder::Reference(Identifier::parse("minecraft:ambient.cave").unwrap())
    );
    reference.encode(&mut w, &sound).unwrap();
    let direct = Holder::<Sound>::decode(&mut r, &sound).unwrap();
    assert_eq!(
        direct,
        Holder::Direct(Sound {
            id: Identifier::parse("test:direct").unwrap(),
            range: Some(24.5)
        })
    );
    direct.encode(&mut w, &sound).unwrap();
    for _ in 0..2 {
        let set = HolderSet::decode(&mut r, &block).unwrap();
        set.encode(&mut w, &block).unwrap();
    }
    r.finish().unwrap();
    assert_eq!(w.as_slice(), bytes);
}
