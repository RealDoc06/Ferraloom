use super::*;
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
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
