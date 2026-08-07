use std::{
    any::Any,
    ops::{Index, IndexMut},
    rc::Rc,
};

use crate::{
    flags::{ModifierFlags, NodeFlags, TokenFlags},
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
        macro_rules! visit {
            ($($name:ident),+) => {
                match node.kind {
                    $(SyntaxKind::$name => node.data::<$name>().visit(self, visitor),)+
                    _ => {}
                }
            };
        }
        visit![
            SourceFile,
            Block,
            VariableStatement,
            VariableDeclarationList,
            VariableDeclaration,
            ArrayBindingPattern,
            ObjectBindingPattern,
            BindingElement,
            ComputedPropertyName,
            BinaryExpression,
            ConditionalType,
            UnionType,
            IntersectionType,
            TypeOperator,
            InferType,
            TypeParameter
        ];
    }

    pub fn new_modifier_list(&self, nodes: Vec<NodeId>, loc: TextRange) -> ModifierList {
        let flags = self.modifiers_to_flags(&nodes);
        ModifierList {
            list: NodeList { loc, nodes },
            flags,
        }
    }

    fn modifiers_to_flags(&self, nodes: &[NodeId]) -> ModifierFlags {
        let mut flags = ModifierFlags::empty();
        for &node in nodes {
            flags.insert(self[node].kind.modifier_to_flag());
        }
        flags
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

impl<T> Visit for Option<T>
where
    T: Visit,
{
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        if let Some(x) = self {
            x.visit(nodes, visitor);
        }
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
        self.modifiers.visit(nodes, &mut visitor);
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
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for VariableDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
        self.exclamation_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, visitor);
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
        self.dot_dot_dot_token.visit(nodes, &mut visitor);
        self.property_name.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, visitor);
    }
}

pub struct Identifier {
    pub text: String,
}

pub struct PrivateIdentifier {
    pub text: String,
}

pub struct StringLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

pub struct NumericLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

pub struct BigIntLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

pub struct RegularExpressionLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

pub struct NoSubstitutionTemplateLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

pub struct ComputedPropertyName {
    pub expression: NodeId,
}

impl Visit for ComputedPropertyName {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, visitor);
    }
}

pub struct BinaryExpression {
    pub left: NodeId,
    pub operator_token: NodeId,
    pub right: NodeId,
    pub modifiers: Option<ModifierList>,
    pub type_node: Option<NodeId>,
}

impl Visit for BinaryExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.left.visit(nodes, &mut visitor);
        self.operator_token.visit(nodes, &mut visitor);
        self.right.visit(nodes, &mut visitor);
        self.modifiers.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ConditionalType {
    pub type_node: NodeId,
    pub extends_type: NodeId,
    pub true_type: NodeId,
    pub false_type: NodeId,
}

impl Visit for ConditionalType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
        self.extends_type.visit(nodes, &mut visitor);
        self.true_type.visit(nodes, &mut visitor);
        self.false_type.visit(nodes, &mut visitor);
    }
}

pub struct UnionType {
    pub types: NodeList,
}

impl Visit for UnionType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

pub struct IntersectionType {
    pub types: NodeList,
}

impl Visit for IntersectionType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

pub struct TypeOperator {
    pub operator: SyntaxKind,
    pub type_node: NodeId,
}

impl Visit for TypeOperator {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct InferType {
    pub type_parameter: NodeId,
}

impl Visit for InferType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_parameter.visit(nodes, &mut visitor);
    }
}

pub struct TypeParameter {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub constraint: Option<NodeId>,
    pub expression: Option<NodeId>,
    pub default_type: Option<NodeId>,
}

impl Visit for TypeParameter {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.constraint.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
        self.default_type.visit(nodes, &mut visitor);
    }
}
