use std::{any::Any, collections::BTreeMap, sync::Arc};
use crate::*;

/// Port of ParsedArgument, with a dynamically typed value and its source range.
#[derive(Clone)]
pub struct ParsedArgument { pub range: StringRange, pub value: Arc<dyn Any + Send + Sync> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedCommandNode { pub node: NodeId, pub range: StringRange }

/// Port of CommandContext and its parse-time builder. Sources are shared without requiring Clone.
pub struct CommandContext<S> {
    pub source: Arc<S>,
    pub input: String,
    pub arguments: BTreeMap<String, ParsedArgument>,
    pub root_node: NodeId,
    pub nodes: Vec<ParsedCommandNode>,
    pub range: StringRange,
    pub child: Option<Box<Self>>,
    pub command: Option<Command<S>>,
    pub modifier: Option<RedirectModifier<S>>,
    pub forks: bool,
}
impl<S> Clone for CommandContext<S> {
    fn clone(&self) -> Self {
        Self { source: self.source.clone(), input: self.input.clone(), arguments: self.arguments.clone(), root_node: self.root_node,
            nodes: self.nodes.clone(), range: self.range, child: self.child.clone(), command: self.command.clone(), modifier: self.modifier.clone(), forks: self.forks }
    }
}
impl<S> CommandContext<S> {
    pub fn new(source: Arc<S>, input: impl Into<String>, root_node: NodeId, start: usize) -> Self {
        Self { source, input: input.into(), arguments: BTreeMap::new(), root_node, nodes: Vec::new(), range: StringRange::at(start), child: None, command: None, modifier: None, forks: false }
    }
    pub fn get_argument<T: Any>(&self, name: &str) -> Option<&T> { self.arguments.get(name)?.value.downcast_ref() }
    pub fn copy_for(&self, source: Arc<S>) -> Self { let mut copy = self.clone(); copy.source = source; copy }
    pub fn last_child(&self) -> &Self { self.child.as_deref().map_or(self, Self::last_child) }
    pub fn find_suggestion_context(&self, cursor: usize) -> SuggestionContext<'_, S> {
        assert!(cursor >= self.range.start, "Can't find node before cursor");
        if self.range.end < cursor {
            if let Some(child) = &self.child { return child.find_suggestion_context(cursor); }
            return self.nodes.last().map_or(SuggestionContext { context: self, parent: self.root_node, start: self.range.start },
                |last| SuggestionContext { context: self, parent: last.node, start: last.range.end + 1 });
        }
        let mut prev = self.root_node;
        for node in &self.nodes {
            if node.range.start <= cursor && cursor <= node.range.end {
                return SuggestionContext { context: self, parent: prev, start: node.range.start };
            }
            prev = node.node;
        }
        SuggestionContext { context: self, parent: prev, start: self.range.start }
    }
}
pub struct SuggestionContext<'a, S> { pub context: &'a CommandContext<S>, pub parent: NodeId, pub start: usize }
pub struct ParseResults<S> {
    pub context: CommandContext<S>,
    pub reader: StringReader,
    pub exceptions: Vec<(NodeId, CommandSyntaxException)>,
}
impl<S> Clone for ParseResults<S> {
    fn clone(&self) -> Self { Self { context: self.context.clone(), reader: self.reader.clone(), exceptions: self.exceptions.clone() } }
}

/// Port of CommandDispatcher. The node arena exposes stable child and redirect edges.
pub struct CommandDispatcher<S> {
    nodes: Vec<CommandNode<S>>,
    consumer: ResultConsumer<S>,
    provider: Arc<dyn BuiltInExceptionProvider>,
}
impl<S: 'static> Default for CommandDispatcher<S> { fn default() -> Self { Self::new() } }
impl<S: 'static> CommandDispatcher<S> {
    pub fn new() -> Self {
        Self { nodes: vec![ArgumentBuilder::root().node], consumer: Arc::new(|_, _, _| {}), provider: Arc::new(BuiltInExceptions) }
    }
    pub fn root(&self) -> NodeId { NodeId(0) }
    pub fn node(&self, id: NodeId) -> &CommandNode<S> { &self.nodes[id.0] }
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &CommandNode<S>)> { self.nodes.iter().enumerate().map(|(i, node)| (NodeId(i), node)) }
    pub fn set_exception_provider(&mut self, provider: Arc<dyn BuiltInExceptionProvider>) { self.provider = provider; }
    pub fn set_consumer(&mut self, consumer: impl Fn(&CommandContext<S>, bool, i32) + Send + Sync + 'static) { self.consumer = Arc::new(consumer); }
    pub fn build(&mut self, builder: ArgumentBuilder<S>) -> NodeId {
        let ArgumentBuilder { node, children, child_handles } = builder;
        let id = NodeId(self.nodes.len()); self.nodes.push(node);
        for child in children { let child = self.build(child); self.add_child(id, child); }
        for child in child_handles { self.add_child(id, child); }
        id
    }
    /// Registers a node under root and returns the supplied handle, as upstream does on merges.
    pub fn register(&mut self, builder: ArgumentBuilder<S>) -> NodeId {
        let id = self.build(builder); self.add_child(self.root(), id); id
    }
    pub fn child(&self, parent: NodeId, name: &str) -> Option<NodeId> {
        self.node(parent).children.iter().copied().find(|id| self.node(*id).name == name)
    }
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        assert!(self.node(child).kind != NodeKind::Root, "Cannot add a root as a child");
        if let Some(existing) = self.child(parent, &self.node(child).name) {
            if self.node(child).command.is_some() { self.nodes[existing.0].command = self.node(child).command.clone(); }
            for grandchild in self.node(child).children.clone() { self.add_child(existing, grandchild); }
        } else { self.nodes[parent.0].children.push(child); }
    }
    pub fn relevant_nodes(&self, node: NodeId, input: &StringReader) -> Vec<NodeId> {
        let mut reader = input.clone(); let start = reader.cursor();
        while reader.can_read() && reader.peek() != 32 { reader.skip(); }
        let text = reader.slice(start, reader.cursor());
        if let Some(literal) = self.node(node).children.iter().copied().find(|id| self.node(*id).kind == NodeKind::Literal && self.node(*id).name == text) { return vec![literal]; }
        self.node(node).children.iter().copied().filter(|id| self.node(*id).kind == NodeKind::Argument).collect()
    }
    pub fn parse(&self, input: &str, source: S) -> ParseResults<S> { self.parse_shared(input, Arc::new(source)) }
    pub fn parse_shared(&self, input: &str, source: Arc<S>) -> ParseResults<S> {
        self.parse_reader(StringReader::with_provider(input, self.provider.clone()), source)
    }
    pub fn parse_reader(&self, reader: StringReader, source: Arc<S>) -> ParseResults<S> {
        let context = CommandContext::new(source, reader.string(), self.root(), reader.cursor());
        self.parse_nodes(self.root(), reader, context)
    }
    fn parse_node(&self, id: NodeId, reader: &mut StringReader, context: &mut CommandContext<S>) -> Result<(), CommandSyntaxException> {
        let node = self.node(id); let start = reader.cursor();
        match node.kind {
            NodeKind::Root => return Ok(()),
            NodeKind::Literal => {
                let length = node.name.encode_utf16().count();
                if !reader.can_read_length(length) || reader.slice(start, start + length) != node.name {
                    return Err(reader.error(ExceptionKind::LiteralIncorrect, &[node.name.clone()]));
                }
                reader.set_cursor(start + length);
                if reader.can_read() && reader.peek() != 32 {
                    reader.set_cursor(start); return Err(reader.error(ExceptionKind::LiteralIncorrect, &[node.name.clone()]));
                }
            }
            NodeKind::Argument => {
                let value = node.argument.as_ref().unwrap().parse(reader)?;
                context.arguments.insert(node.name.clone(), ParsedArgument { range: StringRange::between(start, reader.cursor()), value });
            }
        }
        let range = StringRange::between(start, reader.cursor());
        context.nodes.push(ParsedCommandNode { node: id, range });
        context.range = StringRange::encompassing(context.range, range);
        context.modifier = node.modifier.clone(); context.forks = node.forks;
        Ok(())
    }
    fn parse_nodes(&self, node: NodeId, original: StringReader, context: CommandContext<S>) -> ParseResults<S> {
        let mut errors = Vec::new(); let mut potentials = Vec::new();
        for child in self.relevant_nodes(node, &original) {
            if !self.node(child).can_use(&context.source) { continue; }
            let mut next = context.clone(); let mut reader = original.clone();
            let parsed = self.parse_node(child, &mut reader, &mut next).and_then(|()| {
                if reader.can_read() && reader.peek() != 32 { Err(reader.error(ExceptionKind::ExpectedArgumentSeparator, &[])) } else { Ok(()) }
            });
            if let Err(error) = parsed { errors.push((child, error)); continue; }
            next.command = self.node(child).command.clone();
            if reader.can_read_length(if self.node(child).redirect.is_some() { 1 } else { 2 }) {
                reader.skip();
                if let Some(target) = self.node(child).redirect {
                    let child_context = CommandContext::new(context.source.clone(), reader.string(), target, reader.cursor());
                    let parse = self.parse_nodes(target, reader, child_context);
                    next.child = Some(Box::new(parse.context));
                    return ParseResults { context: next, reader: parse.reader, exceptions: parse.exceptions };
                }
                potentials.push(self.parse_nodes(child, reader, next));
            } else { potentials.push(ParseResults { context: next, reader, exceptions: Vec::new() }); }
        }
        if !potentials.is_empty() {
            potentials.sort_by_key(|p| (p.reader.can_read(), !p.exceptions.is_empty()));
            return potentials.remove(0);
        }
        ParseResults { context, reader: original, exceptions: errors }
    }
    pub fn execute(&self, input: &str, source: S) -> Result<i32, CommandSyntaxException> { self.execute_parsed(&self.parse(input, source)) }
    pub fn execute_parsed(&self, parse: &ParseResults<S>) -> Result<i32, CommandSyntaxException> {
        if parse.reader.can_read() {
            return Err(if parse.exceptions.len() == 1 { parse.exceptions[0].1.clone() }
                else { parse.reader.error(if parse.context.range.is_empty() { ExceptionKind::UnknownCommand } else { ExceptionKind::UnknownArgument }, &[]) });
        }
        let Some(chain) = ContextChain::try_flatten(&parse.context) else {
            (self.consumer)(&parse.context, false, 0);
            return Err(parse.reader.error(ExceptionKind::UnknownCommand, &[]));
        };
        chain.execute_all(parse.context.source.clone(), &self.consumer)
    }
    pub fn get_completion_suggestions(&self, parse: &ParseResults<S>) -> Suggestions {
        self.get_completion_suggestions_at(parse, parse.reader.total_length())
    }
    pub fn get_completion_suggestions_at(&self, parse: &ParseResults<S>, cursor: usize) -> Suggestions {
        let suggestion = parse.context.find_suggestion_context(cursor);
        let start = suggestion.start.min(cursor);
        let input = parse.reader.slice(0, cursor);
        let mut context = suggestion.context.clone(); context.input = input.clone();
        let mut results = Vec::new();
        for id in &self.node(suggestion.parent).children {
            let node = self.node(*id); let mut builder = SuggestionsBuilder::new(&input, start);
            let result = match node.kind {
                NodeKind::Root => Ok(Suggestions::default()),
                NodeKind::Literal => {
                    if node.name.to_lowercase().starts_with(&builder.remaining_lower_case()) { builder.suggest(&node.name); }
                    Ok(builder.build())
                }
                NodeKind::Argument => {
                    if let Some(provider) = &node.suggestions { provider(&context, builder) }
                    else { node.argument.as_ref().unwrap().list_suggestions(&context, builder) }
                }
            };
            results.push(result.unwrap_or_default());
        }
        Suggestions::merge(parse.reader.string(), results)
    }
    pub fn find_node(&self, path: &[impl AsRef<str>]) -> Option<NodeId> {
        path.iter().try_fold(self.root(), |node, name| self.child(node, name.as_ref()))
    }
    pub fn get_path(&self, target: NodeId) -> Vec<String> {
        fn visit<S: 'static>(dispatcher: &CommandDispatcher<S>, node: NodeId, target: NodeId, path: &mut Vec<String>, visited: &mut Vec<NodeId>) -> bool {
            if node == target { return true; }
            if visited.contains(&node) { return false; } visited.push(node);
            for child in &dispatcher.node(node).children {
                path.push(dispatcher.node(*child).name.clone());
                if visit(dispatcher, *child, target, path, visited) { return true; } path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if visit(self, self.root(), target, &mut path, &mut Vec::new()) { path } else { Vec::new() }
    }
    fn redirect_usage(&self, target: NodeId) -> String { if target == self.root() { "...".into() } else { format!("-> {}", self.node(target).usage_text()) } }
    pub fn get_all_usage(&self, node: NodeId, source: &S, restricted: bool) -> Vec<String> {
        let mut result = Vec::new(); self.all_usage(node, source, restricted, "", &mut result); result
    }
    fn all_usage(&self, node: NodeId, source: &S, restricted: bool, prefix: &str, result: &mut Vec<String>) {
        let node = self.node(node);
        if restricted && !node.can_use(source) { return; }
        if node.command.is_some() { result.push(prefix.into()); }
        if let Some(target) = node.redirect {
            result.push(format!("{} {}", if prefix.is_empty() { node.usage_text() } else { prefix.into() }, self.redirect_usage(target)));
        } else {
            for child in &node.children {
                let usage = self.node(*child).usage_text();
                self.all_usage(*child, source, restricted, &if prefix.is_empty() { usage } else { format!("{prefix} {usage}") }, result);
            }
        }
    }
    pub fn get_smart_usage(&self, node: NodeId, source: &S) -> Vec<(NodeId, String)> {
        let node = self.node(node);
        node.children.iter().filter_map(|id| self.smart_usage(*id, source, node.command.is_some(), false).map(|usage| (*id, usage))).collect()
    }
    fn smart_usage(&self, id: NodeId, source: &S, optional: bool, deep: bool) -> Option<String> {
        let node = self.node(id); if !node.can_use(source) { return None; }
        let own = if optional { format!("[{}]", node.usage_text()) } else { node.usage_text() };
        let child_optional = node.command.is_some();
        if !deep {
            if let Some(target) = node.redirect { return Some(format!("{own} {}", self.redirect_usage(target))); }
            let children: Vec<_> = node.children.iter().copied().filter(|id| self.node(*id).can_use(source)).collect();
            if children.len() == 1 {
                if let Some(usage) = self.smart_usage(children[0], source, child_optional, child_optional) { return Some(format!("{own} {usage}")); }
            } else if children.len() > 1 {
                let mut usages = Vec::new();
                for child in &children {
                    if let Some(usage) = self.smart_usage(*child, source, child_optional, true) { if !usages.contains(&usage) { usages.push(usage); } }
                }
                if usages.len() == 1 {
                    return Some(format!("{own} {}", if child_optional { format!("[{}]", usages[0]) } else { usages.remove(0) }));
                } else if usages.len() > 1 {
                    let usage = children.iter().map(|id| self.node(*id).usage_text()).collect::<Vec<_>>().join("|");
                    return Some(format!("{own} {}{usage}{}", if child_optional { "[" } else { "(" }, if child_optional { "]" } else { ")" }));
                }
            }
        }
        Some(own)
    }
    pub fn is_valid_input(&self, id: NodeId, input: &str) -> bool {
        let node = self.node(id); let mut reader = StringReader::new(input);
        match node.kind {
            NodeKind::Root => false,
            NodeKind::Literal => input == node.name || input.starts_with(&format!("{} ", node.name)),
            NodeKind::Argument => node.argument.as_ref().unwrap().parse(&mut reader).is_ok() && (!reader.can_read() || reader.peek() == 32),
        }
    }
    pub fn find_ambiguities(&self, mut consumer: impl FnMut(NodeId, NodeId, NodeId, &[String])) {
        self.ambiguities(self.root(), &mut consumer);
    }
    fn ambiguities(&self, parent: NodeId, consumer: &mut impl FnMut(NodeId, NodeId, NodeId, &[String])) {
        for child in &self.node(parent).children {
            for sibling in &self.node(parent).children {
                if child == sibling { continue; }
                let mut matches = Vec::new();
                for example in self.node(*child).examples() {
                    if self.is_valid_input(*sibling, &example) && !matches.contains(&example) { matches.push(example); }
                }
                if !matches.is_empty() { consumer(parent, *child, *sibling, &matches); }
            }
            self.ambiguities(*child, consumer);
        }
    }
}

/// Port of ContextChain, separating redirect stages from the final command.
pub struct ContextChain<'a, S> { pub modifiers: Vec<&'a CommandContext<S>>, pub executable: &'a CommandContext<S> }
impl<'a, S> ContextChain<'a, S> {
    pub fn try_flatten(context: &'a CommandContext<S>) -> Option<Self> {
        let mut current = context; let mut modifiers = Vec::new();
        while let Some(child) = &current.child { modifiers.push(current); current = child; }
        current.command.as_ref()?; Some(Self { modifiers, executable: current })
    }
    pub fn run_modifier(context: &CommandContext<S>, source: Arc<S>, consumer: &ResultConsumer<S>, forked: bool) -> Result<Vec<Arc<S>>, CommandSyntaxException> {
        let Some(modifier) = &context.modifier else { return Ok(vec![source]); };
        let context = context.copy_for(source);
        match modifier(&context) {
            Ok(sources) => Ok(sources),
            Err(error) => { consumer(&context, false, 0); if forked { Ok(Vec::new()) } else { Err(error) } }
        }
    }
    pub fn run_executable(context: &CommandContext<S>, source: Arc<S>, consumer: &ResultConsumer<S>, forked: bool) -> Result<i32, CommandSyntaxException> {
        let context = context.copy_for(source);
        match context.command.as_ref().expect("Context must be executable")(&context) {
            Ok(result) => { consumer(&context, true, result); Ok(if forked { 1 } else { result }) }
            Err(error) => { consumer(&context, false, 0); if forked { Ok(0) } else { Err(error) } }
        }
    }
    pub fn execute_all(&self, source: Arc<S>, consumer: &ResultConsumer<S>) -> Result<i32, CommandSyntaxException> {
        let mut forked = false; let mut sources = vec![source];
        for modifier in &self.modifiers {
            forked |= modifier.forks;
            let mut next = Vec::new();
            for source in sources { next.extend(Self::run_modifier(modifier, source, consumer, forked)?); }
            if next.is_empty() { return Ok(0); } sources = next;
        }
        let mut result = 0i32;
        for source in sources { result = result.wrapping_add(Self::run_executable(self.executable, source, consumer, forked)?); }
        Ok(result)
    }
    pub fn next_stage(&self) -> Option<Self> {
        if self.modifiers.is_empty() { None } else { Some(Self { modifiers: self.modifiers[1..].to_vec(), executable: self.executable }) }
    }
    pub fn top_context(&self) -> &CommandContext<S> { self.modifiers.first().copied().unwrap_or(self.executable) }
}
