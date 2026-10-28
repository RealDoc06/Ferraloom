//! Replay real repository bootstrap dumps. Scalar and description values in
//! this fixture are already valid; the server separately replays the complete
//! metadata schema with its canonical protocol codecs.
use super::*;
use crate::datapack::metadata::{PackCompatibility, PackMetadata};
use serde_json::{Value, json};
use std::io::Write;
struct FixtureCodecs;
impl MetadataCodecs for FixtureCodecs {
    type Description = Value;
    fn description(&self, value: &Value) -> Result<Value, Error> {
        assert!(
            value.is_string() || value.get("translate").is_some(),
            "validated fixture description"
        );
        Ok(value.clone())
    }
    fn integer(&self, value: &Value) -> Result<i32, Error> {
        value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| Error::InvalidData("fixture integer".into()))
    }
}
struct FixtureDirectory(PathBuf);
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(input: &Value) -> FixtureDirectory {
    let directory = std::env::temp_dir().join(format!(
        "ferraloom-repository-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    for pack in input["packs"].as_array().unwrap() {
        let path = directory.join(pack["name"].as_str().unwrap());
        match pack["kind"].as_str().unwrap() {
            "symlink" => {
                std::os::unix::fs::symlink(directory.join(pack["target"].as_str().unwrap()), path)
                    .unwrap()
            }
            "ignored" => fs::write(path, "ignored").unwrap(),
            "zip" => {
                let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);
                zip.start_file("pack.mcmeta", options).unwrap();
                zip.write_all(pack["metadata"].to_string().as_bytes())
                    .unwrap();
                for (name, content) in pack["files"].as_object().unwrap() {
                    zip.start_file(name, options).unwrap();
                    zip.write_all(content.as_str().unwrap().as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            "directory" => {
                fs::create_dir_all(&path).unwrap();
                fs::write(path.join("pack.mcmeta"), pack["metadata"].to_string()).unwrap();
                for (name, content) in pack["files"].as_object().unwrap() {
                    let file = path.join(name);
                    fs::create_dir_all(file.parent().unwrap()).unwrap();
                    fs::write(file, content.as_str().unwrap()).unwrap();
                }
                if let Some(target) = pack.get("nested_link") {
                    std::os::unix::fs::symlink(
                        directory.join(target.as_str().unwrap()),
                        path.join("linked"),
                    )
                    .unwrap();
                }
            }
            _ => unreachable!("fixture kind"),
        }
    }
    FixtureDirectory(directory)
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn compatibility<D>(metadata: &PackMetadata<D>) -> &'static str {
    match metadata.compatibility {
        PackCompatibility::TooOld => "TOO_OLD",
        PackCompatibility::TooNew => "TOO_NEW",
        PackCompatibility::Unknown => "UNKNOWN",
        PackCompatibility::Compatible => "COMPATIBLE",
    }
}
#[test]
fn native_repository_bootstrap_selection_metadata_overlays_and_resource_stacks() {
    let input: Value = serde_json::from_str(include_str!(
        "../../../tests/java/datapacks/repository-input.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/java/datapacks/repository.json"
    ))
    .unwrap();
    assert_eq!(expected.as_array().unwrap().len(), 12);
    let directory = fixture(&input);
    let jar =
        std::env::var("FERRALOOM_VANILLA_JAR").expect("native resources on the wrapper builder");
    for (case, row) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.as_array().unwrap())
    {
        let mut repository =
            PackRepository::discover(Path::new(&jar), &directory.0, None, &FixtureCodecs).unwrap();
        let initial = WorldDataConfiguration {
            data_packs: Arc::new(DataPackConfig {
                enabled: strings(&case["enabled"]),
                disabled: strings(&case["disabled"]),
            }),
            enabled_features: FeatureFlags::from_names(strings(&case["features"])).unwrap(),
        };
        let configuration = repository
            .configure(
                &initial,
                case["init"].as_bool().unwrap(),
                case["safe"].as_bool().unwrap(),
            )
            .unwrap();
        let resources = repository.open_selected().unwrap();
        let available=repository.available().map(|pack|json!({"id":pack.id,"source":format!("{:?}",pack.source),"required":pack.required,"fixed":pack.fixed_position,"position":match pack.default_position {Position::Bottom=>"BOTTOM",Position::Top=>"TOP"},"compatibility":compatibility(&pack.metadata),"description":pack.metadata.description,"features":pack.metadata.features.names(),"overlays":pack.metadata.overlays})).collect::<Vec<_>>();
        let id = "test:function/ordered.mcfunction".parse().unwrap();
        let resource = |resource: super::super::Resource<'_>| {
            json!([
                resource.source,
                String::from_utf8(resource.bytes.to_vec()).unwrap()
            ])
        };
        let listed = resources
            .list("function/", "")
            .into_iter()
            .map(|id| {
                let resource = resources.get(&id).unwrap();
                json!([
                    id.to_string(),
                    resource.source,
                    String::from_utf8(resource.bytes.to_vec()).unwrap()
                ])
            })
            .collect::<Vec<_>>();
        let mut observed = json!({"name":case["name"],"available":available,"selected":repository.selected_ids(),"features":configuration.enabled_features.names(),"resource":resources.get(&id).map(resource),"stack":resources.stack(&id).into_iter().map(resource).collect::<Vec<_>>(),"listed":listed});
        // Gson omits null object members in the native bootstrap dump.
        if resources.get(&id).is_none() {
            observed.as_object_mut().unwrap().remove("resource");
        }
        let mut expected = row.clone();
        let saved = expected.as_object_mut().unwrap().remove("encoded").unwrap();
        assert_eq!(observed, expected, "{}", case["name"]);
        let mut encoded = ferraloom_nbt::CompoundTag::new();
        configuration.write_nbt(&mut encoded);
        assert_eq!(
            encoded,
            ferraloom_nbt::parse_compound(saved.as_str().unwrap()).unwrap(),
            "{}",
            case["name"]
        );
    }
}
