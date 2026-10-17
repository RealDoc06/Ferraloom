use crate::{CompoundTag, ListTag, NbtString, ParseError, Tag, compare_nbt, parse_prefix};
use std::fmt;

/// Port of `NbtPathArgument`'s six node types.
#[derive(Clone, Debug)]
enum Node {
    Child(NbtString),
    Index(i32),
    All,
    MatchElement(CompoundTag),
    MatchObject(NbtString, CompoundTag),
    MatchRoot(CompoundTag),
}
#[derive(Clone, Debug)]
enum Step {
    Key(NbtString),
    Index(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    Syntax(ParseError),
    NothingFound(String),
    ExpectedList(Tag),
    InvalidIndex(i32),
    TooDeep,
}
impl PathError {
    pub fn cursor(&self) -> i32 {
        if let Self::Syntax(e) = self {
            e.cursor as i32
        } else {
            -1
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::Syntax(e) => e.message.clone(),
            Self::NothingFound(p) => format!("Found no elements matching {p}"),
            Self::ExpectedList(t) => format!("Expected a list: got {t}"),
            Self::InvalidIndex(i) => format!("Invalid list index: {i}"),
            Self::TooDeep => "Resulting NBT too deeply nested".into(),
        }
    }
}
impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}
impl std::error::Error for PathError {}

/// Port of `NbtPathArgument.NbtPath`. Mutations retain vanilla's partial changes on failure.
#[derive(Clone, Debug)]
pub struct NbtPath {
    original: String,
    nodes: Vec<Node>,
    ends: Vec<usize>,
}
impl NbtPath {
    pub fn parse(input: &str) -> Result<Self, PathError> {
        let text: Vec<u16> = input.encode_utf16().collect();
        let mut pos = 0;
        let mut nodes = Vec::new();
        let mut ends = Vec::new();
        while pos < text.len() && text[pos] != 32 {
            let node = match text[pos] {
                34 | 39 => {
                    let name = read_name(&text, &mut pos)?;
                    object_node(input, &text, &mut pos, name)?
                }
                91 => {
                    pos += 1;
                    if text.get(pos) == Some(&123) {
                        let c = read_pattern(input, &mut pos)?;
                        expect(&text, &mut pos, ']')?;
                        Node::MatchElement(c)
                    } else if text.get(pos) == Some(&93) {
                        pos += 1;
                        Node::All
                    } else {
                        let start = pos;
                        while text
                            .get(pos)
                            .is_some_and(|c| matches!(*c, 45 | 46 | 48..=57))
                        {
                            pos += 1;
                        }
                        if pos == start {
                            return Err(syntax(pos, "Expected integer"));
                        }
                        let digits = String::from_utf16_lossy(&text[start..pos]);
                        let index = digits
                            .parse::<i32>()
                            .map_err(|_| syntax(start, format!("Invalid integer '{digits}'")))?;
                        expect(&text, &mut pos, ']')?;
                        Node::Index(index)
                    }
                }
                123 => {
                    if !nodes.is_empty() {
                        return Err(syntax(pos, "Invalid NBT path element"));
                    }
                    Node::MatchRoot(read_pattern(input, &mut pos)?)
                }
                _ => {
                    let start = pos;
                    while text
                        .get(pos)
                        .is_some_and(|c| !matches!(*c, 32 | 34 | 39 | 91 | 93 | 46 | 123 | 125))
                    {
                        pos += 1;
                    }
                    if start == pos {
                        return Err(syntax(pos, "Invalid NBT path element"));
                    }
                    let name = NbtString::from_utf16(text[start..pos].to_vec());
                    object_node(input, &text, &mut pos, name)?
                }
            };
            nodes.push(node);
            ends.push(pos);
            if text.get(pos).is_some_and(|c| !matches!(*c, 32 | 91 | 123)) {
                expect(&text, &mut pos, '.')?;
            }
        }
        Ok(Self {
            original: String::from_utf16_lossy(&text[..pos]),
            nodes,
            ends,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.original
    }
    fn not_found(&self, index: usize) -> PathError {
        PathError::NothingFound(String::from_utf16_lossy(
            &self
                .original
                .encode_utf16()
                .take(self.ends[index])
                .collect::<Vec<_>>(),
        ))
    }
    pub fn get(&self, root: &Tag) -> Result<Vec<Tag>, PathError> {
        let mut values = vec![root.clone()];
        for (i, node) in self.nodes.iter().enumerate() {
            values = values
                .iter()
                .flat_map(|parent| {
                    select(node, parent)
                        .into_iter()
                        .filter_map(|step| child(parent, &step))
                })
                .collect();
            if values.is_empty() {
                return Err(self.not_found(i));
            }
        }
        Ok(values)
    }
    pub fn count_matching(&self, root: &Tag) -> usize {
        self.get(root).map_or(0, |v| v.len())
    }
    fn targets(
        &self,
        root: &mut Tag,
        count: usize,
        last_default: &Tag,
        create: bool,
    ) -> Result<Vec<Vec<Step>>, PathError> {
        let mut paths = vec![Vec::new()];
        for i in 0..count {
            let mut next = Vec::new();
            let preferred = self
                .nodes
                .get(i + 1)
                .map(preferred)
                .unwrap_or_else(|| last_default.clone());
            for path in paths {
                let steps = with_mut(root, &path, &mut |parent| {
                    if create {
                        ensure(&self.nodes[i], parent, &preferred)
                    } else {
                        select(&self.nodes[i], parent)
                    }
                })
                .unwrap_or_default();
                for step in steps {
                    let mut path = path.clone();
                    if let Some(step) = step {
                        path.push(step)
                    }
                    next.push(path)
                }
            }
            if next.is_empty() && i + 1 < self.nodes.len() {
                return Err(self.not_found(i));
            }
            paths = next;
        }
        Ok(paths)
    }
    pub fn get_or_create(&self, root: &mut Tag, value: &Tag) -> Result<Vec<Tag>, PathError> {
        let paths = self.targets(root, self.nodes.len(), value, true)?;
        Ok(paths.iter().filter_map(|path| at(root, path)).collect())
    }
    pub fn set(&self, root: &mut Tag, value: &Tag) -> Result<usize, PathError> {
        if Self::is_too_deep(value, self.nodes.len()) {
            return Err(PathError::TooDeep);
        }
        let value = &value.copy();
        let Some(last) = self.nodes.last() else {
            return Ok(0);
        };
        let paths = self.targets(root, self.nodes.len() - 1, &preferred(last), true)?;
        Ok(paths
            .iter()
            .map(|path| {
                with_mut(root, path, &mut |parent| set_node(last, parent, value)).unwrap_or(0)
            })
            .sum())
    }
    pub fn insert(&self, index: i32, root: &mut Tag, values: &[Tag]) -> Result<usize, PathError> {
        if values
            .iter()
            .any(|v| Self::is_too_deep(v, self.nodes.len()))
        {
            return Err(PathError::TooDeep);
        }
        let values = values.iter().map(Tag::copy).collect::<Vec<_>>();
        let paths = self.targets(root, self.nodes.len(), &Tag::List(ListTag::default()), true)?;
        let mut changed = 0;
        for path in paths {
            let result = with_mut(root, &path, &mut |parent| {
                let Some(len) = parent.collection_len() else {
                    return Err(PathError::ExpectedList(parent.clone()));
                };
                let mut actual = if index < 0 {
                    len as i64 + i64::from(index) + 1
                } else {
                    i64::from(index)
                };
                let mut modified = false;
                for value in &values {
                    let compatible = matches!(parent, Tag::List(_)) || value.as_double().is_some();
                    if !compatible {
                        continue;
                    }
                    if actual < 0 || actual > parent.collection_len().unwrap() as i64 {
                        return Err(PathError::InvalidIndex(actual as i32));
                    }
                    if parent.collection_insert(actual as usize, value) {
                        actual += 1;
                        modified = true;
                    }
                }
                Ok(usize::from(modified))
            })
            .unwrap_or(Ok(0));
            changed += result?;
        }
        Ok(changed)
    }
    pub fn remove(&self, root: &mut Tag) -> usize {
        let Some(last) = self.nodes.last() else {
            return 0;
        };
        let Ok(paths) = self.targets(root, self.nodes.len() - 1, &Tag::End, false) else {
            return 0;
        };
        paths
            .iter()
            .map(|path| with_mut(root, path, &mut |parent| remove_node(last, parent)).unwrap_or(0))
            .sum()
    }
    pub fn is_too_deep(tag: &Tag, depth: usize) -> bool {
        depth >= 512
            || match tag {
                Tag::Compound(c) => c.values().any(|v| Self::is_too_deep(v, depth + 1)),
                Tag::List(v) => v.iter().any(|v| Self::is_too_deep(v, depth + 1)),
                _ => false,
            }
    }
}
impl fmt::Display for NbtPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.original)
    }
}
fn syntax(cursor: usize, message: impl Into<String>) -> PathError {
    PathError::Syntax(ParseError {
        cursor,
        message: message.into(),
    })
}
fn expect(text: &[u16], pos: &mut usize, c: char) -> Result<(), PathError> {
    if text.get(*pos) == Some(&(c as u16)) {
        *pos += 1;
        Ok(())
    } else {
        Err(syntax(*pos, format!("Expected '{c}'")))
    }
}
fn read_name(text: &[u16], pos: &mut usize) -> Result<NbtString, PathError> {
    let quote = text[*pos];
    *pos += 1;
    let mut units = Vec::new();
    while let Some(&c) = text.get(*pos) {
        *pos += 1;
        if c == quote {
            return Ok(NbtString::from_utf16(units));
        }
        if c == 92 {
            let Some(&e) = text.get(*pos) else { break };
            *pos += 1;
            if e != quote && e != 92 {
                return Err(syntax(
                    *pos - 1,
                    format!(
                        "Invalid escape sequence '\\{}' in quoted string",
                        char::from_u32(u32::from(e)).unwrap_or('\u{fffd}')
                    ),
                ));
            }
            units.push(e)
        } else {
            units.push(c)
        }
    }
    Err(syntax(*pos, "Unclosed quoted string"))
}
fn object_node(
    input: &str,
    text: &[u16],
    pos: &mut usize,
    name: NbtString,
) -> Result<Node, PathError> {
    if name.is_empty() {
        return Err(syntax(*pos, "Invalid NBT path element"));
    }
    if text.get(*pos) == Some(&123) {
        Ok(Node::MatchObject(name, read_pattern(input, pos)?))
    } else {
        Ok(Node::Child(name))
    }
}
fn read_pattern(input: &str, pos: &mut usize) -> Result<CompoundTag, PathError> {
    let prefix = input.encode_utf16().take(*pos).collect::<Vec<_>>();
    let byte = String::from_utf16_lossy(&prefix).len();
    let (tag, used) = parse_prefix(&input[byte..]).map_err(|mut e| {
        e.cursor += *pos;
        PathError::Syntax(e)
    })?;
    *pos += used;
    if let Tag::Compound(c) = tag {
        Ok(c)
    } else {
        Err(syntax(*pos, "Expected compound tag"))
    }
}
fn preferred(node: &Node) -> Tag {
    match node {
        Node::All | Node::Index(_) | Node::MatchElement(_) => Tag::List(ListTag::default()),
        _ => Tag::Compound(CompoundTag::new()),
    }
}
fn matches(pattern: &CompoundTag, tag: Option<&Tag>) -> bool {
    compare_nbt(Some(&Tag::Compound(pattern.copy())), tag, true)
}
fn actual(index: i32, len: usize) -> Option<usize> {
    let n = if index < 0 {
        len as i64 + i64::from(index)
    } else {
        i64::from(index)
    };
    if n >= 0 && (n as usize) < len {
        Some(n as usize)
    } else {
        None
    }
}
// None denotes the current tag, used by a root predicate.
fn select(node: &Node, parent: &Tag) -> Vec<Option<Step>> {
    match node {
        Node::Child(name) => {
            if parent.as_compound().and_then(|c| c.get_key(name)).is_some() {
                vec![Some(Step::Key(name.clone()))]
            } else {
                vec![]
            }
        }
        Node::MatchObject(name, pattern) => {
            if matches(pattern, parent.as_compound().and_then(|c| c.get_key(name))) {
                vec![Some(Step::Key(name.clone()))]
            } else {
                vec![]
            }
        }
        Node::MatchRoot(pattern) => {
            if matches(pattern, Some(parent)) {
                vec![None]
            } else {
                vec![]
            }
        }
        Node::Index(index) => parent
            .collection_len()
            .and_then(|len| actual(*index, len))
            .map(|i| vec![Some(Step::Index(i))])
            .unwrap_or_default(),
        Node::All => (0..parent.collection_len().unwrap_or(0))
            .map(|i| Some(Step::Index(i)))
            .collect(),
        Node::MatchElement(pattern) => {
            if let Tag::List(v) = parent {
                v.iter()
                    .enumerate()
                    .filter(|(_, tag)| matches(pattern, Some(tag)))
                    .map(|(i, _)| Some(Step::Index(i)))
                    .collect()
            } else {
                vec![]
            }
        }
    }
}
fn ensure(node: &Node, parent: &mut Tag, value: &Tag) -> Vec<Option<Step>> {
    match node {
        Node::Child(name) => {
            if let Tag::Compound(c) = parent
                && c.get_key(name).is_none()
            {
                c.insert(name.clone(), value.copy());
            }
        }
        Node::MatchObject(name, pattern) => {
            if let Tag::Compound(c) = parent
                && c.get_key(name).is_none()
            {
                c.insert(name.clone(), Tag::Compound(pattern.copy()));
            }
        }
        Node::All => {
            if parent.collection_len() == Some(0) {
                parent.collection_insert(0, value);
            }
        }
        Node::MatchElement(pattern) => {
            if let Tag::List(v) = parent
                && !v.iter().any(|tag| matches(pattern, Some(tag)))
            {
                v.push(Tag::Compound(pattern.copy()));
            }
        }
        _ => {}
    }
    select(node, parent)
}
fn child(parent: &Tag, step: &Option<Step>) -> Option<Tag> {
    match step {
        None => Some(parent.clone()),
        Some(Step::Key(k)) => parent.as_compound()?.get_key(k).cloned(),
        Some(Step::Index(i)) => parent.collection_get(*i),
    }
}
fn at(root: &Tag, path: &[Step]) -> Option<Tag> {
    let mut value = root.clone();
    for step in path {
        value = child(&value, &Some(step.clone()))?;
    }
    Some(value)
}
fn with_mut<T>(root: &mut Tag, path: &[Step], f: &mut impl FnMut(&mut Tag) -> T) -> Option<T> {
    let Some((first, rest)) = path.split_first() else {
        return Some(f(root));
    };
    match first {
        Step::Key(k) => {
            let Tag::Compound(c) = root else { return None };
            with_mut(c.get_mut_key(k)?, rest, f)
        }
        Step::Index(i) => {
            if let Tag::List(v) = root {
                return with_mut(v.get_mut(*i)?, rest, f);
            }
            let mut value = root.collection_get(*i)?;
            let result = with_mut(&mut value, rest, f);
            root.collection_set(*i, &value);
            result
        }
    }
}
fn set_node(node: &Node, parent: &mut Tag, value: &Tag) -> usize {
    match node {
        Node::Child(name) => {
            if let Tag::Compound(c) = parent {
                let previous = c.insert(name.clone(), value.clone());
                usize::from(previous.as_ref() != Some(value))
            } else {
                0
            }
        }
        Node::MatchRoot(_) => 0,
        Node::All => {
            let Some(size) = parent.collection_len() else {
                return 0;
            };
            if size == 0 {
                parent.collection_insert(0, value);
                return 1;
            }
            let count = (0..size)
                .filter(|i| parent.collection_get(*i).as_ref() != Some(value))
                .count();
            if count == 0 {
                return 0;
            }
            parent.collection_clear();
            if !parent.collection_insert(0, value) {
                return 0;
            }
            for i in 1..size {
                parent.collection_insert(i, value);
            }
            count
        }
        Node::MatchElement(_) if matches!(parent,Tag::List(v) if v.is_empty()) => {
            parent.collection_insert(0, value);
            1
        }
        _ => {
            let steps = select(node, parent);
            let mut changed = 0;
            for step in steps {
                if child(parent, &step).as_ref() == Some(value) {
                    continue;
                }
                match step {
                    Some(Step::Key(k)) => {
                        if let Tag::Compound(c) = parent {
                            c.insert(k, value.clone());
                            changed += 1;
                        }
                    }
                    Some(Step::Index(i)) => changed += usize::from(parent.collection_set(i, value)),
                    None => {}
                }
            }
            changed
        }
    }
}
fn remove_node(node: &Node, parent: &mut Tag) -> usize {
    let steps = select(node, parent);
    let mut count = 0;
    for step in steps.into_iter().rev() {
        match step {
            Some(Step::Key(k)) => {
                if let Tag::Compound(c) = parent {
                    c.remove_key(&k);
                    count += 1;
                }
            }
            Some(Step::Index(i)) => {
                parent.collection_remove(i);
                count += 1
            }
            None => {}
        }
    }
    count
}
