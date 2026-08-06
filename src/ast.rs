use std::{
    any::Any,
    ops::{Index, IndexMut},
    rc::Rc,
};

use crate::{
    flags::{ModifierFlags, NodeFlags},
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
    data: Option<Rc<dyn Any>>,
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

    pub fn create<T: 'static>(&mut self, kind: SyntaxKind, data: T) -> NodeId {
        let id = NodeId(self.store.len());
        self.store.push(Node {
            kind,
            data: Some(Rc::new(data)),
            ..Node::default()
        });
        id
    }

    pub fn for_each_child(&mut self, node: NodeId, visitor: impl FnMut(&mut Node)) {
        let node = &self[node];
        match node.kind {
            SyntaxKind::SourceFile => node.data::<SourceFile>().visit(self, visitor),
            SyntaxKind::Block => node.data::<Block>().visit(self, visitor),
            SyntaxKind::VariableStatement => node.data::<VariableStatement>().visit(self, visitor),
            SyntaxKind::VariableDeclarationList => {
                node.data::<VariableDeclarationList>().visit(self, visitor)
            }
            SyntaxKind::VariableDeclaration => {
                node.data::<VariableDeclaration>().visit(self, visitor)
            }
            SyntaxKind::ArrayBindingPattern => {
                node.data::<ArrayBindingPattern>().visit(self, visitor)
            }
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

pub trait Visit {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node));
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

pub struct SourceFile {
    pub statements: NodeList,
    pub source_text: String,
    pub eof_token: NodeId,
}

impl Visit for NodeId {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        visitor(&mut nodes[*self]);
    }
}

impl Visit for [NodeId] {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        for &node in self {
            visitor(&mut nodes[node]);
        }
    }
}

impl Visit for SourceFile {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.statements.visit(nodes, &mut visitor);
        visitor(&mut nodes[self.eof_token]);
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

impl Visit for NodeList {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.nodes.visit(nodes, visitor);
    }
}

pub struct Block {
    pub statements: NodeList,
    pub multiline: bool,
}

impl Visit for Block {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.statements.visit(nodes, visitor);
    }
}

pub struct ModifierList {
    pub list: NodeList,
    pub flags: ModifierFlags,
}

impl Visit for ModifierList {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.list.visit(nodes, visitor);
    }
}

pub struct VariableStatement {
    pub modifiers: Option<ModifierList>,
    pub declaration_list: NodeId,
}

impl Visit for VariableStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        if let Some(modifiers) = self.modifiers.as_ref() {
            modifiers.visit(nodes, &mut visitor);
        }
        self.declaration_list.visit(nodes, visitor);
    }
}

pub struct VariableDeclarationList {
    pub declarations: NodeList,
    pub flags: NodeFlags,
}

impl Visit for VariableDeclarationList {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.declarations.visit(nodes, visitor);
    }
}

pub struct VariableDeclaration {
    pub name: NodeId,
    pub exclamation_token: Option<NodeId>,
    pub type_annotation: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for VariableDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
        if let Some(x) = self.exclamation_token {
            x.visit(nodes, &mut visitor);
        }
        if let Some(x) = self.type_annotation {
            x.visit(nodes, &mut visitor);
        }
        if let Some(x) = self.initializer {
            x.visit(nodes, visitor);
        }
    }
}

pub struct ArrayBindingPattern {
    pub elements: NodeList,
}

impl Visit for ArrayBindingPattern {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

pub struct ObjectBindingPattern {
    pub elements: NodeList,
}

impl Visit for ObjectBindingPattern {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

pub struct BindingElement {
    pub dot_dot_dot_token: Option<NodeId>,
    pub property_name: Option<NodeId>,
    pub name: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for BindingElement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        if let Some(x) = self.dot_dot_dot_token {
            x.visit(nodes, &mut visitor);
        }
        if let Some(x) = self.property_name {
            x.visit(nodes, &mut visitor);
        }
        if let Some(x) = self.name {
            x.visit(nodes, &mut visitor);
        }
        if let Some(x) = self.initializer {
            x.visit(nodes, visitor);
        }
    }
}
