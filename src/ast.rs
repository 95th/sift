use std::{
    any::Any,
    ops::{Index, IndexMut},
    rc::Rc,
};

use crate::{
    flags::NodeFlags,
    syntax::{SyntaxKind, TextRange},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

#[derive(Debug, Default)]
pub struct Node {
    pub kind: SyntaxKind,
    pub loc: TextRange,
    pub flags: NodeFlags,
    pub parent: Option<NodeId>,
    pub data: Option<Rc<dyn Any>>,
}

impl Node {
    pub fn is_js_type_alias_declaration(&self) -> bool {
        self.kind == SyntaxKind::TypeAliasDeclaration
    }

    pub fn is_js_import_declaration(&self) -> bool {
        self.kind == SyntaxKind::JSImportDeclaration
    }

    pub fn data<T: 'static>(&self) -> Rc<T> {
        self.data.clone().unwrap().downcast().unwrap()
    }
}

pub struct NodeFactory {
    store: Vec<Node>,
}

impl NodeFactory {
    pub fn new() -> Self {
        Self { store: Vec::new() }
    }

    pub fn create(&mut self, kind: SyntaxKind) -> NodeId {
        let id = NodeId(self.store.len());
        self.store.push(Node {
            kind,
            ..Node::default()
        });
        id
    }

    pub fn for_each_child(&mut self, node: NodeId, func: impl FnMut(&mut Node)) {
        let node = &self[node];
        match node.kind {
            SyntaxKind::SourceFile => node.data::<SourceFileData>().for_each_child(self, func),
            SyntaxKind::Block => node.data::<BlockData>().for_each_child(self, func),
            _ => {}
        }
    }
}

impl Index<NodeId> for NodeFactory {
    type Output = Node;

    fn index(&self, index: NodeId) -> &Self::Output {
        &self.store[index.0]
    }
}

impl IndexMut<NodeId> for NodeFactory {
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        &mut self.store[index.0]
    }
}

pub trait ForEachChild {
    fn for_each_child(&self, nodes: &mut NodeFactory, func: impl FnMut(&mut Node));
}

#[derive(Debug, Clone)]
pub struct CommentRange {
    pub range: TextRange,
    pub kind: SyntaxKind,
    pub has_trailing_new_line: bool,
}

pub struct JSDocInfo {
    pub parent: NodeId,
    pub jsdocs: Vec<NodeId>,
}

pub struct SourceFileData {
    pub statements: Vec<NodeId>,
    pub source_text: String,
    pub eof_token: NodeId,
}

impl ForEachChild for SourceFileData {
    fn for_each_child(&self, nodes: &mut NodeFactory, mut func: impl FnMut(&mut Node)) {
        for statement in self.statements.iter() {
            func(&mut nodes[*statement]);
        }
        func(&mut nodes[self.eof_token]);
    }
}

pub struct NodeList {
    pub loc: TextRange,
    pub nodes: Vec<NodeId>,
}

impl NodeList {
    pub fn missing() -> Self {
        Self {
            loc: TextRange::invalid(),
            nodes: Vec::new(),
        }
    }

    pub fn is_missing(&self) -> bool {
        self.loc.is_invalid()
    }
}

pub struct BlockData {
    pub statements: NodeList,
    pub multiline: bool,
}

impl ForEachChild for BlockData {
    fn for_each_child(&self, nodes: &mut NodeFactory, mut func: impl FnMut(&mut Node)) {
        for statement in self.statements.nodes.iter() {
            func(&mut nodes[*statement]);
        }
    }
}
