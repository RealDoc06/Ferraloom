use crate::{
    ArgumentType, CommandContext, CommandSyntaxException, Suggestions, SuggestionsBuilder,
    arguments::ErasedArgumentType,
};
use std::{any::Any, cmp::Ordering, sync::Arc};

/// Stable handle into a dispatcher's command-node arena. Redirects may form cycles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);
pub type Command<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<i32, CommandSyntaxException> + Send + Sync>;
pub type Requirement<S> = Arc<dyn Fn(&S) -> bool + Send + Sync>;
pub type RedirectModifier<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<Vec<Arc<S>>, CommandSyntaxException> + Send + Sync>;
pub type SuggestionProvider<S> = Arc<
    dyn Fn(&CommandContext<S>, SuggestionsBuilder) -> Result<Suggestions, CommandSyntaxException>
        + Send
        + Sync,
>;
pub type ResultConsumer<S> = Arc<dyn Fn(&CommandContext<S>, bool, i32) + Send + Sync>;

/// Port of the root, literal and argument command node kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Root,
    Literal,
    Argument,
}

/// Port of CommandNode. Child order is registration order; parsing prioritizes exact literals.
pub struct CommandNode<S> {
    pub(crate) name: String,
    pub(crate) kind: NodeKind,
    pub(crate) children: Vec<NodeId>,
    pub(crate) command: Option<Command<S>>,
    pub(crate) requirement: Requirement<S>,
    pub(crate) redirect: Option<NodeId>,
    pub(crate) modifier: Option<RedirectModifier<S>>,
    pub(crate) forks: bool,
    pub(crate) argument: Option<Arc<dyn ErasedArgumentType<S>>>,
    pub(crate) suggestions: Option<SuggestionProvider<S>>,
}
impl<S> CommandNode<S> {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> NodeKind {
        self.kind
    }
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
    pub fn redirect(&self) -> Option<NodeId> {
        self.redirect
    }
    pub fn is_fork(&self) -> bool {
        self.forks
    }
    pub fn is_executable(&self) -> bool {
        self.command.is_some()
    }
    pub fn has_custom_suggestions(&self) -> bool {
        self.suggestions.is_some()
    }
    pub fn can_use(&self, source: &S) -> bool {
        (self.requirement)(source)
    }
    pub fn argument_type<A: Any>(&self) -> Option<&A> {
        self.argument.as_ref()?.as_any().downcast_ref()
    }
    pub fn usage_text(&self) -> String {
        if self.kind == NodeKind::Argument {
            format!("<{}>", self.name)
        } else {
            self.name.clone()
        }
    }
    pub fn examples(&self) -> Vec<String> {
        match self.kind {
            NodeKind::Root => Vec::new(),
            NodeKind::Literal => vec![self.name.clone()],
            NodeKind::Argument => self.argument.as_ref().unwrap().examples(),
        }
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        match (
            self.kind == NodeKind::Literal,
            other.kind == NodeKind::Literal,
        ) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self.name.encode_utf16().cmp(other.name.encode_utf16()),
        }
    }
    /// Commands-packet flags: node kind, executable, redirect and custom suggestions.
    pub fn flags(&self) -> u8 {
        let kind = match self.kind {
            NodeKind::Root => 0,
            NodeKind::Literal => 1,
            NodeKind::Argument => 2,
        };
        kind | if self.is_executable() { 4 } else { 0 }
            | if self.redirect.is_some() { 8 } else { 0 }
            | if self.has_custom_suggestions() { 16 } else { 0 }
    }
}

/// Port of ArgumentBuilder, LiteralArgumentBuilder and RequiredArgumentBuilder.
/// A builder with children cannot redirect, matching Brigadier's construction rules.
pub struct ArgumentBuilder<S> {
    pub(crate) node: CommandNode<S>,
    pub(crate) children: Vec<Self>,
    pub(crate) child_handles: Vec<NodeId>,
}
impl<S: 'static> ArgumentBuilder<S> {
    fn new(name: String, kind: NodeKind) -> Self {
        Self {
            node: CommandNode {
                name,
                kind,
                children: Vec::new(),
                command: None,
                requirement: Arc::new(|_| true),
                redirect: None,
                modifier: None,
                forks: false,
                argument: None,
                suggestions: None,
            },
            children: Vec::new(),
            child_handles: Vec::new(),
        }
    }
    pub fn root() -> Self {
        Self::new(String::new(), NodeKind::Root)
    }
    pub fn literal(name: impl Into<String>) -> Self {
        Self::new(name.into(), NodeKind::Literal)
    }
    pub fn argument(name: impl Into<String>, argument: impl ArgumentType) -> Self {
        let mut builder = Self::new(name.into(), NodeKind::Argument);
        builder.node.argument = Some(Arc::new(argument));
        builder
    }
    pub fn then(mut self, child: Self) -> Self {
        assert!(
            self.node.redirect.is_none(),
            "Cannot add children to a redirected node"
        );
        assert!(
            child.node.kind != NodeKind::Root,
            "Cannot add a root as a child"
        );
        self.children.push(child);
        self
    }
    pub fn then_node(mut self, child: NodeId) -> Self {
        assert!(
            self.node.redirect.is_none(),
            "Cannot add children to a redirected node"
        );
        self.child_handles.push(child);
        self
    }
    pub fn executes(
        mut self,
        command: impl Fn(&CommandContext<S>) -> Result<i32, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.node.command = Some(Arc::new(command));
        self
    }
    pub fn requires(mut self, requirement: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        self.node.requirement = Arc::new(requirement);
        self
    }
    pub fn redirect(self, target: NodeId) -> Self {
        self.forward(target, None, false)
    }
    pub fn redirect_with(
        self,
        target: NodeId,
        modifier: impl Fn(&CommandContext<S>) -> Result<Arc<S>, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.forward(
            target,
            Some(Arc::new(move |context| modifier(context).map(|s| vec![s]))),
            false,
        )
    }
    pub fn fork(
        self,
        target: NodeId,
        modifier: impl Fn(&CommandContext<S>) -> Result<Vec<Arc<S>>, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.forward(target, Some(Arc::new(modifier)), true)
    }
    pub fn forward(
        mut self,
        target: NodeId,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        assert!(
            self.children.is_empty() && self.child_handles.is_empty(),
            "Cannot forward a node with children"
        );
        self.node.redirect = Some(target);
        self.node.modifier = modifier;
        self.node.forks = forks;
        self
    }
    pub fn suggests(
        mut self,
        provider: impl Fn(
            &CommandContext<S>,
            SuggestionsBuilder,
        ) -> Result<Suggestions, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.node.suggestions = Some(Arc::new(provider));
        self
    }
}
pub fn literal<S: 'static>(name: impl Into<String>) -> ArgumentBuilder<S> {
    ArgumentBuilder::literal(name)
}
pub fn argument<S: 'static>(
    name: impl Into<String>,
    kind: impl ArgumentType,
) -> ArgumentBuilder<S> {
    ArgumentBuilder::argument(name, kind)
}
