use super::*;
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn text(s: &str) -> String {
    String::from_utf8(unhex(s)).unwrap()
}
fn vectors(kind: &str) -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../tests/java/vectors.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(move |p| p[0] == kind)
}

#[test]
fn vanilla_snbt_and_binary_vectors() {
    for p in vectors("parse") {
        let input = text(p[1]);
        let tag = parse(&input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        assert_eq!(tag.to_string(), text(p[2]), "compact {input:?}");
        assert_eq!(pretty(&tag), text(p[4]), "pretty {input:?}");
        assert_eq!(to_text(&tag, "", true), text(p[5]), "text {input:?}");
        let bytes = unhex(p[3]);
        let decoded =
            NbtIo::read_any_tag(bytes.as_slice(), &mut NbtAccounter::unlimited()).unwrap();
        assert_eq!(decoded, tag, "read {input:?}");
        let mut actual = Vec::new();
        NbtIo::write_any_tag(&mut actual, &tag).unwrap();
        assert_eq!(actual, bytes, "write {input:?}");
    }
}
#[test]
fn vanilla_snbt_errors() {
    let mut errors = Vec::new();
    for p in vectors("error") {
        let input = text(p[1]);
        let expected = ParseError {
            cursor: p[2].parse().unwrap(),
            message: text(p[3]),
        };
        let actual = parse(&input).unwrap_err();
        if actual != expected {
            errors.push(format!("{input:?}: got {actual:?}; expected {expected:?}"));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
#[test]
fn vanilla_compression_vectors() {
    for p in vectors("compressed") {
        let compression = match p[1] {
            "gzip" => Compression::Gzip,
            "zlib" => Compression::Zlib,
            _ => Compression::None,
        };
        let bytes = unhex(p[2]);
        let compound = NbtIo::read_compound(
            bytes.as_slice(),
            compression,
            &mut NbtAccounter::unlimited(),
        )
        .unwrap();
        assert_eq!(compound.to_string(), text(p[3]));
        let mut actual = Vec::new();
        NbtIo::write_compound(&mut actual, compression, &compound).unwrap();
        assert_eq!(
            NbtIo::read_compound(
                actual.as_slice(),
                compression,
                &mut NbtAccounter::unlimited()
            )
            .unwrap(),
            compound
        );
    }
}
#[test]
fn vanilla_partial_matching_vectors() {
    for p in vectors("compare") {
        let a = parse(&text(p[1])).unwrap();
        let b = parse(&text(p[2])).unwrap();
        assert_eq!(
            compare_nbt(Some(&a), Some(&b), p[3] == "true"),
            p[4] == "true",
            "{p:?}"
        );
    }
}
fn path_root() -> Tag {
    parse("{a:[{id:1,x:2},{id:2,x:3}],n:[I;1,2],v:4}").unwrap()
}
fn path_operation(
    path: &NbtPath,
    op: &str,
    value: &str,
    root: &mut Tag,
) -> Result<String, PathError> {
    match op {
        "get" => path.get(root).map(|tags| {
            format!(
                "[{}]",
                tags.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        "set" => path
            .set(root, &parse(value).unwrap())
            .map(|n| n.to_string()),
        "insert" => path
            .insert(-1, root, &[parse(value).unwrap()])
            .map(|n| n.to_string()),
        _ => Ok(path.remove(root).to_string()),
    }
}
#[test]
fn vanilla_path_operations() {
    for p in vectors("path") {
        let input = text(p[1]);
        let path = NbtPath::parse(&input).unwrap();
        let mut root = path_root();
        assert_eq!(
            path_operation(&path, p[2], &text(p[3]), &mut root).unwrap(),
            text(p[4]),
            "{input}"
        );
        assert_eq!(root.to_string(), text(p[5]), "{input}");
    }
    for p in vectors("patherror") {
        let input = text(p[1]);
        let mut root = path_root();
        let result = NbtPath::parse(&input)
            .and_then(|path| path_operation(&path, p[2], &text(p[3]), &mut root));
        let error = result.unwrap_err();
        assert_eq!(error.cursor(), p[4].parse::<i32>().unwrap(), "{input}");
        assert_eq!(error.message(), text(p[5]), "{input}");
    }
}

#[test]
fn vanilla_allocation_and_depth_limits() {
    for p in vectors("account") {
        let bytes = unhex(p[1]);
        let mut a = NbtAccounter::new(p[2].parse().unwrap(), p[3].parse().unwrap());
        let result = NbtIo::read_any_tag(bytes.as_slice(), &mut a);
        assert_eq!(a.usage(), p[4].parse::<i64>().unwrap(), "{p:?}");
        assert_eq!(a.depth(), p[5].parse::<usize>().unwrap(), "{p:?}");
        if p[6] == "ok" {
            assert_eq!(result.unwrap().to_string(), text(p[7]), "{p:?}");
        } else {
            assert_eq!(result.unwrap_err().to_string(), text(p[7]), "{p:?}");
        }
    }
}
#[test]
fn vanilla_modified_utf8() {
    for p in vectors("utf") {
        let bytes = unhex(p[1]);
        let result = NbtIo::read_utf(&mut bytes.as_slice());
        if p[2] == "ok" {
            let units = unhex(p[3])
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect::<Vec<_>>();
            assert_eq!(result.unwrap().as_utf16(), units, "{p:?}");
        } else {
            assert_eq!(result.unwrap_err().to_string(), text(p[3]), "{p:?}");
        }
    }
    for p in vectors("fallback") {
        let value = Tag::String("a".repeat(p[1].parse().unwrap()).into());
        let mut strict = Vec::new();
        let result = NbtIo::write_any_tag(&mut strict, &value);
        assert_eq!(strict.len(), p[2].parse::<usize>().unwrap());
        if p[3].is_empty() {
            result.unwrap()
        } else {
            assert_eq!(result.unwrap_err().to_string(), text(p[3]));
        }
        let mut fallback = Vec::new();
        NbtIo::write(
            &mut fallback,
            Compression::None,
            &RootFormat::Named(NbtString::default()),
            &value,
        )
        .unwrap();
        assert_eq!(fallback.len(), p[4].parse::<usize>().unwrap());
        assert!(fallback.starts_with(&unhex(p[5])));
    }
}
#[test]
fn vanilla_numeric_conversions() {
    for p in vectors("numeric") {
        let value = match text(p[1]).as_str() {
            "-Infinityd" => Tag::Double(f64::NEG_INFINITY),
            "NaNf" => Tag::Float(f32::NAN),
            s => parse(s).unwrap(),
        };
        assert_eq!(value.as_byte().unwrap(), p[2].parse::<i8>().unwrap());
        assert_eq!(value.as_short().unwrap(), p[3].parse::<i16>().unwrap());
        assert_eq!(value.as_int().unwrap(), p[4].parse::<i32>().unwrap());
        assert_eq!(value.as_long().unwrap(), p[5].parse::<i64>().unwrap());
        assert_eq!(value.as_boolean().unwrap(), p[6] == "true");
    }
}
#[test]
fn vanilla_text_folding() {
    for p in vectors("text") {
        let tag = parse(&text(p[1])).unwrap();
        assert_eq!(to_text(&tag, "", true), text(p[2]));
    }
}

#[test]
fn vanilla_floating_point_printers() {
    for p in vectors("float") {
        let bits = u32::from_str_radix(p[1], 16).unwrap();
        let value = Tag::Float(f32::from_bits(bits));
        assert_eq!(value.to_string(), text(p[2]), "float {bits:x}");
    }
    for p in vectors("double") {
        let bits = u64::from_str_radix(p[1], 16).unwrap();
        let value = Tag::Double(f64::from_bits(bits));
        assert_eq!(value.to_string(), text(p[2]), "double {bits:x}");
    }
}

#[test]
fn vanilla_hash_map_iteration() {
    let mut c = CompoundTag::new();
    for p in vectors("map") {
        let key = text(p[2]);
        match p[1] {
            "put" => {
                c.insert(key, Tag::Int(1));
            }
            "remove" => {
                c.remove(&key);
            }
            "copy" => {
                c = c.copy();
            }
            _ => unreachable!(),
        }
        let tag = Tag::Compound(c.clone());
        let mut bytes = Vec::new();
        NbtIo::write_any_tag(&mut bytes, &tag).unwrap();
        assert_eq!(bytes, unhex(p[3]), "{} {}", p[1], text(p[2]));
        assert_eq!(to_text(&tag, "", false), text(p[4]));
    }
}
fn utf16(s: &str) -> Vec<u16> {
    unhex(s)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect()
}
#[test]
fn vanilla_surrogate_strings() {
    for p in vectors("surrogate") {
        let tag = parse(&text(p[1])).unwrap();
        assert_eq!(tag.to_snbt_utf16().as_utf16(), utf16(p[2]));
        assert_eq!(pretty_utf16(&tag).as_utf16(), utf16(p[3]));
        assert_eq!(to_text_utf16(&tag, "", true).as_utf16(), utf16(p[4]));
        let mut bytes = Vec::new();
        NbtIo::write_any_tag(&mut bytes, &tag).unwrap();
        assert_eq!(bytes, unhex(p[5]));
    }
}
#[test]
fn vanilla_path_edge_cases() {
    let mut failures = Vec::new();
    for p in vectors("scenario") {
        let input = text(p[1]);
        let path = text(p[2]);
        let mut root = parse(&input).unwrap();
        let value = text(p[5]);
        let result = NbtPath::parse(&path).and_then(|path| {
            if p[3] == "insert" {
                let Tag::List(values) = parse(&value).unwrap() else {
                    unreachable!()
                };
                path.insert(p[4].parse().unwrap(), &mut root, &values)
                    .map(|n| n.to_string())
            } else {
                path_operation(&path, p[3], &value, &mut root)
            }
        });
        let actual = match result {
            Ok(result) => ("ok".to_string(), result),
            Err(error) => (error.cursor().to_string(), error.message()),
        };
        if actual != (p[6].to_string(), text(p[7])) || root.to_string() != text(p[8]) {
            failures.push(format!(
                "{input} {path} {}: got {actual:?} {root}; expected ({}, {:?}) {}",
                p[3],
                p[6],
                text(p[7]),
                text(p[8])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn vanilla_named_roots_and_list_wrappers() {
    for p in vectors("named") {
        let bytes = unhex(p[1]);
        let (_, tag) = NbtIo::read(
            bytes.as_slice(),
            Compression::None,
            true,
            &mut NbtAccounter::unlimited(),
        )
        .unwrap();
        assert_eq!(tag.to_string(), text(p[2]));
    }
    for p in vectors("binary") {
        let bytes = unhex(p[1]);
        let tag = NbtIo::read_any_tag(bytes.as_slice(), &mut NbtAccounter::unlimited()).unwrap();
        assert_eq!(tag.to_string(), text(p[2]));
        let mut rewritten = Vec::new();
        NbtIo::write_any_tag(&mut rewritten, &tag).unwrap();
        assert_eq!(
            NbtIo::read_any_tag(rewritten.as_slice(), &mut NbtAccounter::unlimited()).unwrap(),
            tag
        );
    }
}
