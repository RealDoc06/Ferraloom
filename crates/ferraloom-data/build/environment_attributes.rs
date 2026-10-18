use serde_json::Value;
use std::fmt::Write;
/// Registry-order facts extracted from the native immutable objects/codecs.
pub(super) fn generate(out: &mut String, rows: &Value, registry: &Value) {
    let entries = super::ordered_entries(&registry["entries"]);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), entries.len());
    writeln!(
        out,
        "static ENVIRONMENT_ATTRIBUTES: [EnvironmentAttributeData; {}] = [",
        rows.len()
    )
    .unwrap();
    for (row, (name, id)) in rows.iter().zip(entries) {
        assert_eq!(row["id"].as_u64(), Some(id));
        assert_eq!(row["name"].as_str(), Some(name));
        write!(out,"EnvironmentAttributeData {{ attribute_type:registries::attribute_type::Id({}),field:{:?},default_json:{:?},",row["type"],row["field"].as_str().unwrap(),row["default"].to_string()).unwrap();
        for key in [
            "positional",
            "spatially_interpolated",
            "full_resolution_biomes",
            "syncable",
        ] {
            write!(out, "{key}:{},", row[key].as_bool().unwrap()).unwrap();
        }
        out.push_str("modifiers:&[");
        for name in row["modifiers"].as_array().unwrap() {
            write!(out, "{:?},", name.as_str().unwrap()).unwrap();
        }
        out.push_str("],");
        for key in ["float_min", "float_max"] {
            if row[key].is_null() {
                write!(out, "{key}:None,").unwrap();
            } else {
                write!(out, "{key}:Some({}),", super::float(&row[key])).unwrap();
            }
        }
        out.push_str("},\n");
    }
    out.push_str("];\n");
}
