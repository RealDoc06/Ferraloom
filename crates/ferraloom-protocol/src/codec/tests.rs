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
