//! Data packs (`<world>/datapacks`: folders and zips with `pack.mcmeta`) and
//! the parts of them commands use: `.mcfunction` functions (with macros and
//! line continuation), function tags and predicates.
use crate::zip::Zip;
use horizon_world::nbt::Tag;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// A pack on disk.
pub struct DataPack {
    /// `file/<name>` in `/datapack` and level.dat.
    pub id: String,
    source: Source,
}
enum Source {
    Dir(PathBuf),
    Zip(Box<Zip>),
}
impl DataPack {
    /// Every file path under `prefix` (relative to the pack root).
    fn list(&self, prefix: &str) -> Vec<String> {
        match &self.source {
            Source::Zip(z) => z
                .names()
                .filter(|n| n.starts_with(prefix) && !n.ends_with('/'))
                .map(str::to_owned)
                .collect(),
            Source::Dir(root) => {
                let mut out = Vec::new();
                walk(root, &root.join(prefix), &mut out);
                out
            }
        }
    }
    /// The files of `data/<namespace>/<kind>/` for each kind, in the order
    /// `list("data/")` gives them. A folder pack walks only those folders:
    /// the built-in pack holds about 10,000 other data files.
    fn list_kinds(&self, kinds: &[&str]) -> Vec<String> {
        debug_assert!(kinds.is_sorted(), "kinds in walk order");
        let Source::Dir(root) = &self.source else {
            return self.list("data/");
        };
        let Ok(entries) = std::fs::read_dir(root.join("data")) else {
            return Vec::new();
        };
        let mut namespaces: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        namespaces.sort();
        let mut out = Vec::new();
        for ns in namespaces {
            for kind in kinds {
                walk(root, &ns.join(kind), &mut out);
            }
        }
        out
    }
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match &self.source {
            Source::Zip(z) => z.read(path).ok(),
            Source::Dir(root) => std::fs::read(root.join(path)).ok(),
        }
    }
    /// The `description` of `pack.mcmeta`, as text.
    pub fn description(&self) -> String {
        let Some(bytes) = self.read("pack.mcmeta") else {
            return String::new();
        };
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        match &v["pack"]["description"] {
            serde_json::Value::String(s) => s.clone(),
            other => other["text"].as_str().unwrap_or_default().to_owned(),
        }
    }
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}
impl DataPack {
    /// The built-in `vanilla` pack: the game's own data directory.
    pub fn builtin(root: &Path) -> Self {
        DataPack {
            id: "vanilla".into(),
            source: Source::Dir(root.to_path_buf()),
        }
    }
}
/// The packs in `<world>/datapacks`, sorted by name.
pub fn discover(world: &Path) -> Vec<DataPack> {
    let Ok(entries) = std::fs::read_dir(world.join("datapacks")) else {
        return Vec::new();
    };
    let mut packs = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        if path.is_dir() && path.join("pack.mcmeta").exists() {
            packs.push(DataPack {
                id: format!("file/{name}"),
                source: Source::Dir(path),
            });
        } else if name.ends_with(".zip")
            && let Ok(z) = Zip::open(&path)
            && z.contains("pack.mcmeta")
        {
            packs.push(DataPack {
                id: format!("file/{name}"),
                source: Source::Zip(Box::new(z)),
            });
        }
    }
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

// ---- functions ---------------------------------------------------------------

/// A `.mcfunction`: plain command lines and macro lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub id: String,
    pub lines: Vec<FunctionLine>,
    /// Macro parameter names in order of first use.
    pub parameters: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionLine {
    Command(String),
    /// A `$` line: text with `$(name)` placeholders.
    Macro(String),
}
impl Function {
    pub fn is_macro(&self) -> bool {
        !self.parameters.is_empty()
    }
    /// `MacroFunction.instantiate`: the command lines with the arguments
    /// substituted. The error is the translation key and its arguments.
    pub fn instantiate(
        &self,
        arguments: Option<&Tag>,
    ) -> Result<Vec<String>, (String, Vec<String>)> {
        let mut values: BTreeMap<&str, String> = BTreeMap::new();
        if self.is_macro() {
            let Some(args) = arguments else {
                return Err((
                    "commands.function.error.missing_arguments".into(),
                    vec![self.id.clone()],
                ));
            };
            for p in &self.parameters {
                let Some(v) = args.get(p) else {
                    return Err((
                        "commands.function.error.missing_argument".into(),
                        vec![self.id.clone(), p.clone()],
                    ));
                };
                values.insert(p, stringify(v));
            }
        }
        Ok(self
            .lines
            .iter()
            .map(|l| match l {
                FunctionLine::Command(c) => c.clone(),
                FunctionLine::Macro(t) => substitute(t, &values),
            })
            .collect())
    }
}
/// `MacroFunction.stringify`.
fn stringify(tag: &Tag) -> String {
    match tag {
        Tag::Float(v) => decimal(f64::from(f32::from_bits(*v))),
        Tag::Double(v) => decimal(f64::from_bits(*v)),
        Tag::Byte(v) => v.to_string(),
        Tag::Short(v) => v.to_string(),
        Tag::Long(v) => v.to_string(),
        Tag::String(s) => s.to_string_lossy(),
        other => crate::snbt::to_snbt(other),
    }
}
/// `DecimalFormat("#")` with up to 15 fraction digits.
fn decimal(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    let s = format!("{v:.15}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_owned()
    }
}
fn substitute(template: &str, values: &BTreeMap<&str, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find("$(") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find(')') {
            Some(j) => {
                if let Some(v) = values.get(&after[..j]) {
                    out.push_str(v);
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}
fn macro_parameters(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find("$(") {
        let after = &rest[i + 2..];
        let Some(j) = after.find(')') else {
            return Err(format!("Unterminated macro variable in '{line}'"));
        };
        let name = &after[..j];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("Invalid macro variable name '{name}' in '{line}'"));
        }
        out.push(name.to_owned());
        rest = &after[j + 1..];
    }
    Ok(out)
}
/// Checks one command line as `CommandFunction.parseCommand` does, with the
/// compilation source's permissions; the error is `CommandSyntaxException.getMessage`.
pub type Compile<'a> = &'a dyn Fn(&str) -> Result<(), String>;

/// `CommandFunction.fromLines`: comments, blank lines, `\` continuation and
/// `$` macro lines. Plain command lines are compiled with `compile`; macro
/// lines only when the function is instantiated (`MacroFunction.MacroEntry`).
pub fn parse_function(id: &str, text: &str, compile: Compile<'_>) -> Result<Function, String> {
    let raw: Vec<&str> = text.lines().collect();
    let mut lines = Vec::new();
    let mut parameters: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let number = i + 1;
        let mut line = raw[i].trim().to_owned();
        if line.ends_with('\\') {
            loop {
                i += 1;
                if i == raw.len() {
                    return Err("Line continuation at end of file".into());
                }
                line.pop();
                line.push_str(raw[i].trim());
                if !line.ends_with('\\') {
                    break;
                }
            }
        }
        i += 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('/') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
                .collect();
            return Err(if rest.starts_with('/') {
                format!(
                    "Unknown or invalid command '{line}' on line {number} (if you intended to make a comment, use '#' not '//')"
                )
            } else {
                format!(
                    "Unknown or invalid command '{line}' on line {number} (did you mean '{name}'? Do not use a preceding forwards slash.)"
                )
            });
        }
        if let Some(m) = line.strip_prefix('$') {
            for p in macro_parameters(m)? {
                if !parameters.contains(&p) {
                    parameters.push(p);
                }
            }
            lines.push(FunctionLine::Macro(m.to_owned()));
        } else {
            compile(&line).map_err(|e| format!("Whilst parsing command on line {number}: {e}"))?;
            lines.push(FunctionLine::Command(line));
        }
    }
    Ok(Function {
        id: id.to_owned(),
        lines,
        parameters,
    })
}

/// One entry of a function tag (`TagLoader.EntryWithSource`).
#[derive(Clone, Debug, PartialEq)]
pub struct TagEntry {
    /// `ns:f`, or `#ns:tag` for a tag.
    pub id: String,
    pub required: bool,
    /// The id of the pack whose file listed it.
    pub source: String,
}

/// The data of every enabled pack: functions, function tags, predicates.
#[derive(Default)]
pub struct DataStore {
    pub functions: BTreeMap<String, Function>,
    /// Function tag id -> entries. Only tags whose required references all
    /// resolved are kept (`TagLoader.build`).
    pub function_tags: BTreeMap<String, Vec<TagEntry>>,
    pub predicates: BTreeMap<String, serde_json::Value>,
    /// `item_modifier` files, for `/item modify`.
    pub item_modifiers: BTreeMap<String, serde_json::Value>,
    /// Functions that failed to load (id, message), for logging.
    pub errors: Vec<(String, String)>,
    /// Tags that failed to load (id, missing references as vanilla lists them).
    pub tag_errors: Vec<(String, String)>,
}
/// `ns:path` for a file `data/<ns>/<kind>/<path>.<ext>`.
fn resource_id(file: &str, kind: &str, ext: &str) -> Option<String> {
    let rest = file.strip_prefix("data/")?;
    let (ns, rest) = rest.split_once('/')?;
    let path = rest
        .strip_prefix(kind)?
        .strip_prefix('/')?
        .strip_suffix(ext)?;
    Some(format!("{ns}:{path}"))
}
impl DataStore {
    /// `ServerFunctionLibrary.reload`: every pack's data, functions compiled
    /// with `compile`, then the function tags built.
    pub fn load(packs: &[&DataPack], compile: Compile<'_>) -> Self {
        let mut s = Self::default();
        for pack in packs {
            for f in pack.list_kinds(&["function", "item_modifier", "predicate", "tags/function"]) {
                if let Some(id) = resource_id(&f, "function", ".mcfunction")
                    && let Some(bytes) = pack.read(&f)
                {
                    // Only the highest pack's file counts, whether it loads or not.
                    s.errors.retain(|(e, _)| *e != id);
                    match parse_function(&id, &String::from_utf8_lossy(&bytes), compile) {
                        Ok(func) => {
                            s.functions.insert(id, func);
                        }
                        Err(e) => {
                            s.functions.remove(&id);
                            s.errors.push((id, e));
                        }
                    }
                } else if let Some(id) = resource_id(&f, "tags/function", ".json")
                    && let Some(bytes) = pack.read(&f)
                    && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    let entries = tag_entries(&v, &pack.id);
                    let slot = s.function_tags.entry(id).or_default();
                    if v["replace"].as_bool() == Some(true) {
                        slot.clear();
                    }
                    slot.extend(entries);
                } else if let Some(id) = resource_id(&f, "predicate", ".json")
                    && let Some(bytes) = pack.read(&f)
                    && let Ok(v) = serde_json::from_slice(&bytes)
                {
                    s.predicates.insert(id, v);
                } else if let Some(id) = resource_id(&f, "item_modifier", ".json")
                    && let Some(bytes) = pack.read(&f)
                    && let Ok(v) = serde_json::from_slice(&bytes)
                {
                    s.item_modifiers.insert(id, v);
                }
            }
        }
        s.build_tags();
        s
    }
    /// `TagLoader.build`: tags in dependency order; a tag with a required
    /// reference that is missing (a function that failed to load, a tag that
    /// failed or does not exist) is dropped and logged.
    fn build_tags(&mut self) {
        let mut pending = std::mem::take(&mut self.function_tags);
        let ids: Vec<String> = pending.keys().cloned().collect();
        for id in ids {
            self.build_tag(&id, &mut pending);
        }
    }
    fn build_tag(&mut self, id: &str, pending: &mut BTreeMap<String, Vec<TagEntry>>) {
        // Taken out first: a cycle back to this tag finds it missing.
        let Some(entries) = pending.remove(id) else {
            return;
        };
        for e in &entries {
            if let Some(dep) = e.id.strip_prefix('#') {
                self.build_tag(dep, pending);
            }
        }
        let missing: Vec<String> = entries
            .iter()
            .filter(|e| {
                let found = match e.id.strip_prefix('#') {
                    Some(tag) => self.function_tags.contains_key(tag),
                    None => self.functions.contains_key(&e.id),
                };
                e.required && !found
            })
            // EntryWithSource.toString (a required entry has no `?`).
            .map(|e| format!("{} (from {})", e.id, e.source))
            .collect();
        if missing.is_empty() {
            self.function_tags.insert(id.to_owned(), entries);
        } else {
            self.tag_errors.push((id.to_owned(), missing.join(", ")));
        }
    }
    /// `FunctionManager.getTag`: the functions a tag expands to (nested tags
    /// included, each function once).
    pub fn tag_functions(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.expand(id, &mut out, 0);
        out
    }
    fn expand(&self, id: &str, out: &mut Vec<String>, depth: usize) {
        if depth > 16 {
            return;
        }
        let Some(entries) = self.function_tags.get(id) else {
            return;
        };
        for e in entries {
            match e.id.strip_prefix('#') {
                Some(inner) => self.expand(inner, out, depth + 1),
                None => {
                    if self.functions.contains_key(&e.id) && !out.contains(&e.id) {
                        out.push(e.id.clone());
                    }
                }
            }
        }
    }
}
fn normalize(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}
fn tag_entries(v: &serde_json::Value, source: &str) -> Vec<TagEntry> {
    v["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let (id, required) = match e {
                serde_json::Value::String(s) => (s.as_str(), true),
                serde_json::Value::Object(o) => (
                    o.get("id")?.as_str()?,
                    o.get("required")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                ),
                _ => return None,
            };
            Some(TagEntry {
                id: hash_normalize(id),
                required,
                source: source.to_owned(),
            })
        })
        .collect()
}
fn hash_normalize(s: &str) -> String {
    match s.strip_prefix('#') {
        Some(rest) => format!("#{}", normalize(rest)),
        None => normalize(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::nbt::parse_snbt;

    /// A compilation context that accepts every command.
    const ANY: Compile<'static> = &|_| Ok(());

    #[test]
    fn parses_comments_continuations_and_macros() {
        let f = parse_function(
            "a:b",
            "# a comment\n\nsay hi\n  say \\\n  there  \n$say $(who) is $(n)\n$tell $(who) $(who)\n",
            ANY,
        )
        .unwrap();
        assert_eq!(
            f.lines,
            vec![
                FunctionLine::Command("say hi".into()),
                FunctionLine::Command("say there".into()),
                FunctionLine::Macro("say $(who) is $(n)".into()),
                FunctionLine::Macro("tell $(who) $(who)".into()),
            ]
        );
        assert_eq!(f.parameters, ["who", "n"]);
        assert!(
            parse_function("a:b", "/say hi", ANY)
                .unwrap_err()
                .contains("did you mean 'say'")
        );
        assert!(
            parse_function("a:b", "//x", ANY)
                .unwrap_err()
                .contains("use '#'")
        );
        assert!(parse_function("a:b", "say \\", ANY).is_err());
        assert!(parse_function("a:b", "$say $(bad name)", ANY).is_err());
    }

    #[test]
    fn macros_substitute_vanilla_text() {
        let f =
            parse_function("a:b", "$say $(s) $(i) $(d) $(l) $(b) $(c)\nsay plain", ANY).unwrap();
        let args = parse_snbt("{s:\"hi\",i:5,d:1.5d,l:7L,b:1b,c:{x:1}}")
            .unwrap()
            .0;
        let out = f.instantiate(Some(&args)).unwrap();
        assert_eq!(out, ["say hi 5 1.5 7 1 {x:1}", "say plain"]);
        let args = parse_snbt("{s:\"x\",i:1,d:3.0d,l:1L,b:0b,c:{}}").unwrap().0;
        assert_eq!(f.instantiate(Some(&args)).unwrap()[0], "say x 1 3 1 0 {}");
        assert_eq!(
            f.instantiate(None).unwrap_err().0,
            "commands.function.error.missing_arguments"
        );
        let partial = parse_snbt("{s:\"x\"}").unwrap().0;
        assert_eq!(
            f.instantiate(Some(&partial)).unwrap_err().0,
            "commands.function.error.missing_argument"
        );
        // A plain function needs no arguments.
        let plain = parse_function("a:p", "say 1", ANY).unwrap();
        assert_eq!(plain.instantiate(None).unwrap(), ["say 1"]);
    }

    #[test]
    fn packs_provide_functions_and_tags() {
        let dir = std::env::temp_dir().join(format!("hz-datapack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("datapacks/mypack");
        let write = |p: &str, t: &str| {
            let path = pack.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, t).unwrap();
        };
        write(
            "pack.mcmeta",
            "{\"pack\":{\"description\":\"Hello\",\"pack_format\":1}}",
        );
        write(
            "data/mine/function/a.mcfunction",
            "say a\nfunction mine:sub/b\n",
        );
        write("data/mine/function/sub/b.mcfunction", "say b");
        write(
            "data/minecraft/tags/function/tick.json",
            "{\"values\":[\"mine:a\",\"#mine:more\",{\"id\":\"mine:gone\",\"required\":false}]}",
        );
        write(
            "data/mine/tags/function/more.json",
            "{\"values\":[\"mine:sub/b\",\"mine:a\"]}",
        );
        write(
            "data/mine/predicate/p.json",
            "{\"condition\":\"minecraft:random_chance\",\"chance\":1}",
        );
        let packs = discover(&dir);
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "file/mypack");
        assert_eq!(packs[0].description(), "Hello");
        let store = DataStore::load(&[&packs[0]], ANY);
        assert_eq!(store.functions.len(), 2);
        assert!(store.functions.contains_key("mine:sub/b"));
        assert_eq!(
            store.tag_functions("minecraft:tick"),
            ["mine:a", "mine:sub/b"]
        );
        assert!(store.predicates.contains_key("mine:p"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn functions_that_do_not_compile_drop_out_with_the_tags_needing_them() {
        let dir = std::env::temp_dir().join(format!("hz-datapack-compile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |p: &str, t: &str| {
            let path = dir.join("datapacks").join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, t).unwrap();
        };
        write("a/pack.mcmeta", "{}");
        write("b/pack.mcmeta", "{}");
        // Line 4 is where the continued `op` starts; macro lines wait for instantiation.
        write(
            "a/data/p/function/op.mcfunction",
            "say hi\n# c\n\nop \\\n Bob\n",
        );
        write("a/data/p/function/ok.mcfunction", "say ok\n$op $(who)\n");
        // The higher pack's file is the one compiled, valid below or not.
        write("a/data/p/function/over.mcfunction", "say fine");
        write("b/data/p/function/over.mcfunction", "op Bob");
        write(
            "a/data/minecraft/tags/function/tick.json",
            r##"{"values":["p:ok","p:op"]}"##,
        );
        write(
            "a/data/p/tags/function/nested.json",
            r##"{"values":["#minecraft:tick"]}"##,
        );
        write(
            "b/data/p/tags/function/soft.json",
            r##"{"values":[{"id":"p:op","required":false},{"id":"#p:nested","required":false},"p:ok"]}"##,
        );
        let packs = discover(&dir);
        let refs: Vec<&DataPack> = packs.iter().collect();
        let compile: Compile = &|line| {
            if line.starts_with("op ") {
                Err(
                    "Unknown or incomplete command. See below for error at position 0: <--[HERE]"
                        .into(),
                )
            } else {
                Ok(())
            }
        };
        let store = DataStore::load(&refs, compile);
        assert_eq!(store.functions.keys().collect::<Vec<_>>(), ["p:ok"]);
        assert_eq!(
            store.errors,
            [
                (
                    "p:op".to_owned(),
                    "Whilst parsing command on line 4: Unknown or incomplete command. See below for error at position 0: <--[HERE]".to_owned()
                ),
                (
                    "p:over".to_owned(),
                    "Whilst parsing command on line 1: Unknown or incomplete command. See below for error at position 0: <--[HERE]".to_owned()
                ),
            ]
        );
        assert_eq!(
            store.tag_errors,
            [
                ("minecraft:tick".to_owned(), "p:op (from file/a)".to_owned()),
                (
                    "p:nested".to_owned(),
                    "#minecraft:tick (from file/a)".to_owned()
                ),
            ]
        );
        assert_eq!(store.function_tags.keys().collect::<Vec<_>>(), ["p:soft"]);
        assert_eq!(store.tag_functions("p:soft"), ["p:ok"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_packs_list_the_needed_kinds_in_walk_order() {
        let dir = std::env::temp_dir().join(format!("hz-datapack-kinds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("datapacks/p");
        let files = [
            "data/b/function/z.mcfunction",
            "data/b/function/a/y.mcfunction",
            "data/b/loot_table/l.json",
            "data/b/tags/block/t.json",
            "data/b/tags/function/load.json",
            "data/b/predicate/p.json",
            "data/a/item_modifier/m.json",
            "data/a/function/f.mcfunction",
            "data/a/functions/old.mcfunction",
            "data/a/recipe/r.json",
            "data/a/tags/function/tick.json",
            "data/c/worldgen/noise/n.json",
        ];
        for f in files {
            let path = pack.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "{}").unwrap();
        }
        std::fs::write(pack.join("pack.mcmeta"), "{}").unwrap();
        let p = DataPack::builtin(&pack);
        let kinds = ["function", "item_modifier", "predicate", "tags/function"];
        let wanted: Vec<String> = p
            .list("data/")
            .into_iter()
            .filter(|f| {
                let rest = f.splitn(3, '/').nth(2).unwrap_or_default();
                kinds.iter().any(|k| rest.starts_with(&format!("{k}/")))
            })
            .collect();
        assert_eq!(wanted.len(), 7);
        assert_eq!(p.list_kinds(&kinds), wanted);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
