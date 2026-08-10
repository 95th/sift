use std::{
    any::Any,
    ops::{Index, IndexMut},
    rc::Rc,
};

use crate::{
    flags::{ModifierFlags, NodeFlags, OuterExpressionKinds, TokenFlags},
    syntax::{CommentDirective, SyntaxKind, TextRange},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

#[derive(Debug)]
pub struct Node {
    pub kind: SyntaxKind,
    pub loc: TextRange,
    pub flags: NodeFlags,
    pub parent: Option<NodeId>,
    data: Rc<dyn Any>,
}

impl Node {
    pub fn is_js_type_alias_declaration(&self) -> bool {
        self.kind == SyntaxKind::TypeAliasDeclaration
    }

    pub fn is_js_import_declaration(&self) -> bool {
        self.kind == SyntaxKind::JSImportDeclaration
    }

    pub fn data<T: 'static>(&self) -> Rc<T> {
        self.data.clone().downcast().unwrap()
    }

    pub fn data_ref<T: 'static>(&self) -> &T {
        self.data.as_ref().downcast_ref().unwrap()
    }

    pub fn is_missing(&self) -> bool {
        self.loc.len() == 0 && self.kind != SyntaxKind::EndOfFile
    }

    pub fn is_present(&self) -> bool {
        !self.is_missing()
    }

    fn is_in_js_file(&self) -> bool {
        self.flags.contains(NodeFlags::JavaScriptFile)
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
            data: Rc::new(data),
            flags: NodeFlags::default(),
            loc: TextRange::default(),
            parent: None,
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
            TypeParameter,
            JSDocNonNullableType,
            JSDocNullableType,
            IndexedAccessType,
            ArrayType,
            ConstructorType,
            FunctionType,
            Parameter,
            TypePredicate,
            YieldExpression,
            ArrowFunction,
            SatisfiesExpression,
            AsExpression,
            ConditionalExpression,
            PrefixUnaryExpression,
            DeleteExpression,
            TypeOfExpression,
            VoidExpression,
            AwaitExpression,
            TypeAssertionExpression,
            PostfixUnaryExpression,
            MetaProperty,
            PropertyAccessExpression,
            ExpressionWithTypeArguments,
            ParenthesizedExpression,
            ArrayLiteralExpression,
            SpreadElement,
            ObjectLiteralExpression,
            SpreadAssignment,
            ShorthandPropertyAssignment,
            PropertyAssignment,
            GetAccessor,
            SetAccessor,
            MethodDeclaration,
            FunctionExpression,
            MissingDeclaration,
            TemplateExpression,
            TemplateSpan,
            CallExpression,
            NonNullExpression,
            TaggedTemplateExpression,
            ElementAccessExpression,
            LabeledStatement,
            ExpressionStatement,
            LiteralType,
            TypeLiteral,
            TupleType,
            TypeReference,
            QualifiedName,
            ParenthesizedType,
            TemplateLiteralType,
            TemplateLiteralTypeSpan,
            ImportType,
            ImportAttributes,
            ImportAttribute,
            CallSignature,
            ConstructSignature,
            IndexSignature,
            MethodSignature,
            PropertySignature,
            TypeQuery,
            MappedType,
            NamedTupleMember,
            OptionalType,
            RestType,
            ClassDeclaration,
            ClassExpression,
            HeritageClause,
            Constructor,
            PropertyDeclaration,
            ClassStaticBlockDeclaration,
            NewExpression,
            PartiallyEmittedExpression,
            FunctionDeclaration,
            IfStatement,
            DoStatement,
            WhileStatement,
            ContinueStatement,
            BreakStatement,
            ReturnStatement,
            WithStatement,
            ThrowStatement,
            ForOfStatement,
            ForInStatement,
            ForStatement
        ];
    }

    pub fn new_modifier_list(&self, nodes: Vec<NodeId>, loc: TextRange) -> ModifierList {
        let flags = self.modifiers_to_flags(&nodes);
        ModifierList { list: NodeList { loc, nodes }, flags }
    }

    pub fn new_property_access_expression(
        &mut self,
        expression: NodeId,
        question_dot_token: Option<NodeId>,
        name: NodeId,
        flags: NodeFlags,
    ) -> NodeId {
        let node = self.create(
            SyntaxKind::PropertyAccessExpression,
            PropertyAccessExpression { expression, question_dot_token, name },
        );
        self[node].flags.insert(flags & NodeFlags::OptionalChain);
        node
    }

    pub fn new_call_expression(
        &mut self,
        expression: NodeId,
        question_dot_token: Option<NodeId>,
        type_arguments: Option<NodeList>,
        argument_list: NodeList,
        flags: NodeFlags,
    ) -> NodeId {
        let node = self.create(
            SyntaxKind::CallExpression,
            CallExpression { expression, question_dot_token, type_arguments, argument_list },
        );
        self[node].flags.insert(flags & NodeFlags::OptionalChain);
        node
    }

    pub fn new_tagged_template_expression(
        &mut self,
        tag: NodeId,
        question_dot_token: Option<NodeId>,
        type_arguments: Option<NodeList>,
        template: NodeId,
        flags: NodeFlags,
    ) -> NodeId {
        let node = self.create(
            SyntaxKind::TaggedTemplateExpression,
            TaggedTemplateExpression { tag, question_dot_token, type_arguments, template },
        );
        self[node].flags.insert(flags & NodeFlags::OptionalChain);
        node
    }

    pub fn new_non_null_expression(&mut self, expression: NodeId, flags: NodeFlags) -> NodeId {
        let node = self.create(SyntaxKind::NonNullExpression, NonNullExpression { expression });
        self[node].flags.insert(flags & NodeFlags::OptionalChain);
        node
    }

    pub fn new_element_access_expression(
        &mut self,
        expression: NodeId,
        question_dot_token: Option<NodeId>,
        argument_expression: NodeId,
        flags: NodeFlags,
    ) -> NodeId {
        let node = self.create(
            SyntaxKind::ElementAccessExpression,
            ElementAccessExpression { expression, question_dot_token, argument_expression },
        );
        self[node].flags.insert(flags & NodeFlags::OptionalChain);
        node
    }

    pub fn is(&self, node: NodeId, kind: SyntaxKind) -> bool {
        self[node].kind == kind
    }

    pub fn has_modifier(&self, modifiers: &Option<ModifierList>, modifier: ModifierFlags) -> bool {
        modifiers.as_ref().is_some_and(|x| x.flags.intersects(modifier))
    }

    pub fn is_left_hand_side_expression(&self, expr: NodeId) -> bool {
        let expr = self.skip_partially_emitted_expressions(expr);
        self[expr].kind.is_left_hand_side_expression()
    }

    pub fn skip_partially_emitted_expressions(&self, node: NodeId) -> NodeId {
        self.skip_outer_expressions(node, OuterExpressionKinds::PartiallyEmittedExpressions)
    }

    fn skip_outer_expressions(&self, mut node: NodeId, kinds: OuterExpressionKinds) -> NodeId {
        while self.is_outer_expression(node, kinds) {
            match self[node].kind {
                SyntaxKind::ParenthesizedExpression => {
                    node = self[node].data_ref::<ParenthesizedExpression>().expression;
                }
                SyntaxKind::TypeAssertionExpression => {
                    node = self[node].data_ref::<TypeAssertionExpression>().expression;
                }
                SyntaxKind::AsExpression => {
                    node = self[node].data_ref::<AsExpression>().expression;
                }
                SyntaxKind::SatisfiesExpression => {
                    node = self[node].data_ref::<SatisfiesExpression>().expression;
                }
                SyntaxKind::ExpressionWithTypeArguments => {
                    node = self[node].data_ref::<ExpressionWithTypeArguments>().expression;
                }
                SyntaxKind::NonNullExpression => {
                    node = self[node].data_ref::<NonNullExpression>().expression;
                }
                SyntaxKind::PartiallyEmittedExpression => {
                    node = self[node].data_ref::<PartiallyEmittedExpression>().expression;
                }
                SyntaxKind::BinaryExpression => {
                    node = self[node].data_ref::<BinaryExpression>().right;
                }
                _ => unreachable!(),
            }
        }
        node
    }

    fn is_outer_expression(&self, node: NodeId, kinds: OuterExpressionKinds) -> bool {
        use OuterExpressionKinds as OEK;
        match self[node].kind {
            SyntaxKind::ParenthesizedExpression => {
                kinds.contains(OEK::Parentheses)
                    && !(kinds.contains(OEK::ExcludeJSDocTypeAssertion)
                        && self.is_jsdoc_type_assertion(node))
            }
            SyntaxKind::TypeAssertionExpression | SyntaxKind::AsExpression => {
                kinds.contains(OEK::TypeAssertions)
            }
            SyntaxKind::SatisfiesExpression => {
                kinds.intersects(OEK::ExpressionsWithTypeArguments | OEK::Satisfies)
            }
            SyntaxKind::ExpressionWithTypeArguments => {
                kinds.contains(OEK::ExpressionsWithTypeArguments)
            }
            SyntaxKind::NonNullExpression => kinds.contains(OEK::NonNullAssertions),
            SyntaxKind::PartiallyEmittedExpression => {
                kinds.contains(OEK::PartiallyEmittedExpressions)
            }
            SyntaxKind::BinaryExpression => {
                let operator_token = self[node].data_ref::<BinaryExpression>().operator_token;
                match self[operator_token].kind {
                    SyntaxKind::EqualsToken => kinds.contains(OEK::Assignments),
                    SyntaxKind::CommaToken => kinds.contains(OEK::Comma),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn is_jsdoc_type_assertion(&self, node: NodeId) -> bool {
        if !self.is(node, SyntaxKind::ParenthesizedExpression) || !self[node].is_in_js_file() {
            return false;
        }
        let expr = self[node].data_ref::<ParenthesizedExpression>().expression;
        if !self.is(expr, SyntaxKind::AsExpression) {
            return false;
        }
        let type_node = self[expr].data_ref::<AsExpression>().type_node;
        self[type_node].flags.contains(NodeFlags::Reparsed)
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

#[derive(Debug)]
pub struct SourceFile {
    pub statements: NodeList,
    pub source_text: String,
    pub eof_token: NodeId,
    pub comment_directives: Vec<CommentDirective>,
    pub is_declaration_file: bool,
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

#[derive(Debug, Clone)]
pub struct NodeList {
    pub loc: TextRange,
    pub nodes: Vec<NodeId>,
}

impl NodeList {
    pub fn missing() -> Self {
        Self { loc: TextRange::invalid(), nodes: Vec::new() }
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

#[derive(Clone)]
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

pub struct JSDocNonNullableType {
    pub type_node: NodeId,
}

impl Visit for JSDocNonNullableType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct JSDocNullableType {
    pub type_node: NodeId,
}

impl Visit for JSDocNullableType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ParenthesizedType {
    pub type_node: NodeId,
}

impl Visit for ParenthesizedType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct IndexedAccessType {
    pub type_node: NodeId,
    pub index_type: NodeId,
}

impl Visit for IndexedAccessType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
        self.index_type.visit(nodes, &mut visitor);
    }
}

pub struct ArrayType {
    pub type_node: NodeId,
}

impl Visit for ArrayType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ConstructorType {
    pub modifiers: Option<ModifierList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
}

impl Visit for ConstructorType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
    }
}

pub struct FunctionType {
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
}

impl Visit for FunctionType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
    }
}

pub struct Parameter {
    pub modifiers: Option<ModifierList>,
    pub dot_dot_dot_token: Option<NodeId>,
    pub name: NodeId,
    pub question_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for Parameter {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.dot_dot_dot_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

pub struct TypePredicate {
    pub asserts_modifier: Option<NodeId>,
    pub parameter_name: NodeId,
    pub type_node: Option<NodeId>,
}

impl Visit for TypePredicate {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.asserts_modifier.visit(nodes, &mut visitor);
        self.parameter_name.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct YieldExpression {
    pub asterisk_token: Option<NodeId>,
    pub expression: Option<NodeId>,
}

impl Visit for YieldExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.asterisk_token.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct ArrowFunction {
    pub modifiers: Option<ModifierList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub equals_greater_than_token: NodeId,
    pub body: NodeId,
}

impl Visit for ArrowFunction {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.equals_greater_than_token.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct SatisfiesExpression {
    pub expression: NodeId,
    pub type_node: NodeId,
}

impl Visit for SatisfiesExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct AsExpression {
    pub expression: NodeId,
    pub type_node: NodeId,
}

impl Visit for AsExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ConditionalExpression {
    pub condition: NodeId,
    pub question_token: NodeId,
    pub when_true: NodeId,
    pub colon_token: NodeId,
    pub when_false: NodeId,
}

impl Visit for ConditionalExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.condition.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.when_true.visit(nodes, &mut visitor);
        self.colon_token.visit(nodes, &mut visitor);
        self.when_false.visit(nodes, &mut visitor);
    }
}

pub struct PrefixUnaryExpression {
    pub operator: SyntaxKind,
    pub expression: NodeId,
}

impl Visit for PrefixUnaryExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}
pub struct PostfixUnaryExpression {
    pub expression: NodeId,
    pub operator: SyntaxKind,
}

impl Visit for PostfixUnaryExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct DeleteExpression {
    pub expression: NodeId,
}

impl Visit for DeleteExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct TypeOfExpression {
    pub expression: NodeId,
}

impl Visit for TypeOfExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct VoidExpression {
    pub expression: NodeId,
}

impl Visit for VoidExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct AwaitExpression {
    pub expression: NodeId,
}

impl Visit for AwaitExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct TypeAssertionExpression {
    pub type_node: NodeId,
    pub expression: NodeId,
}

impl Visit for TypeAssertionExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct MetaProperty {
    pub keyword_token: SyntaxKind,
    pub name: NodeId,
}

impl Visit for MetaProperty {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
    }
}

pub struct PropertyAccessExpression {
    pub expression: NodeId,
    pub question_dot_token: Option<NodeId>,
    pub name: NodeId,
}

impl Visit for PropertyAccessExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.question_dot_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
    }
}
pub struct ExpressionWithTypeArguments {
    pub expression: NodeId,
    pub type_arguments: Option<NodeList>,
}

impl Visit for ExpressionWithTypeArguments {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
    }
}

pub struct ParenthesizedExpression {
    pub expression: NodeId,
}

impl Visit for ParenthesizedExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct ArrayLiteralExpression {
    pub elements: NodeList,
    pub multiline: bool,
}

impl Visit for ArrayLiteralExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

pub struct SpreadElement {
    pub expression: NodeId,
}

impl Visit for SpreadElement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct ObjectLiteralExpression {
    pub properties: NodeList,
    pub multiline: bool,
}

impl Visit for ObjectLiteralExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.properties.visit(nodes, &mut visitor);
    }
}

pub struct SpreadAssignment {
    pub expression: NodeId,
}

impl Visit for SpreadAssignment {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct ShorthandPropertyAssignment {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub equals_token: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for ShorthandPropertyAssignment {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.equals_token.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

pub struct PropertyAssignment {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for PropertyAssignment {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

pub struct GetAccessor {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for GetAccessor {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct SetAccessor {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for SetAccessor {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct MethodDeclaration {
    pub modifiers: Option<ModifierList>,
    pub asterisk_token: Option<NodeId>,
    pub name: NodeId,
    pub question_token: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub type_node: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for MethodDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.asterisk_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct FunctionExpression {
    pub modifiers: Option<ModifierList>,
    pub asterisk_token: Option<NodeId>,
    pub name: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for FunctionExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.asterisk_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct MissingDeclaration {
    pub modifiers: Option<ModifierList>,
}

impl Visit for MissingDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
    }
}

pub struct TemplateExpression {
    pub head: NodeId,
    pub template_spans: NodeList,
}

impl Visit for TemplateExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.head.visit(nodes, &mut visitor);
        self.template_spans.visit(nodes, &mut visitor);
    }
}

pub struct TemplateHead {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

pub struct TemplateMiddle {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

pub struct TemplateTail {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

pub struct TemplateSpan {
    pub expression: NodeId,
    pub literal: NodeId,
}

impl Visit for TemplateSpan {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.literal.visit(nodes, &mut visitor);
    }
}

pub struct CallExpression {
    pub expression: NodeId,
    pub question_dot_token: Option<NodeId>,
    pub type_arguments: Option<NodeList>,
    pub argument_list: NodeList,
}

impl Visit for CallExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.question_dot_token.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
        self.argument_list.visit(nodes, &mut visitor);
    }
}

pub struct NonNullExpression {
    pub expression: NodeId,
}

impl Visit for NonNullExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct TaggedTemplateExpression {
    pub tag: NodeId,
    pub question_dot_token: Option<NodeId>,
    pub type_arguments: Option<NodeList>,
    pub template: NodeId,
}

impl Visit for TaggedTemplateExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.tag.visit(nodes, &mut visitor);
        self.question_dot_token.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
        self.template.visit(nodes, &mut visitor);
    }
}

pub struct ElementAccessExpression {
    pub expression: NodeId,
    pub question_dot_token: Option<NodeId>,
    pub argument_expression: NodeId,
}

impl Visit for ElementAccessExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.question_dot_token.visit(nodes, &mut visitor);
        self.argument_expression.visit(nodes, &mut visitor);
    }
}

pub struct LabeledStatement {
    pub expression: NodeId,
    pub statement: NodeId,
}

impl Visit for LabeledStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}

pub struct ExpressionStatement {
    pub expression: NodeId,
}

impl Visit for ExpressionStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct LiteralType {
    pub expression: NodeId,
}

impl Visit for LiteralType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct TypeLiteral {
    pub members: NodeList,
}

impl Visit for TypeLiteral {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.members.visit(nodes, &mut visitor);
    }
}

pub struct TupleType {
    pub elements: NodeList,
}

impl Visit for TupleType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

pub struct TypeReference {
    pub type_name: NodeId,
    pub type_arguments: Option<NodeList>,
}

impl Visit for TypeReference {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_name.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
    }
}

pub struct QualifiedName {
    pub left: NodeId,
    pub right: NodeId,
}

impl Visit for QualifiedName {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.left.visit(nodes, &mut visitor);
        self.right.visit(nodes, &mut visitor);
    }
}

pub struct TemplateLiteralType {
    pub head: NodeId,
    pub template_spans: NodeList,
}

impl Visit for TemplateLiteralType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.head.visit(nodes, &mut visitor);
        self.template_spans.visit(nodes, &mut visitor);
    }
}

pub struct TemplateLiteralTypeSpan {
    pub type_node: NodeId,
    pub literal: NodeId,
}

impl Visit for TemplateLiteralTypeSpan {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
        self.literal.visit(nodes, &mut visitor);
    }
}

pub struct ImportType {
    pub is_typeof: bool,
    pub type_node: NodeId,
    pub attributes: Option<NodeId>,
    pub qualifier: Option<NodeId>,
    pub type_arguments: Option<NodeList>,
}

impl Visit for ImportType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
        self.attributes.visit(nodes, &mut visitor);
        self.qualifier.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
    }
}

pub struct ImportAttributes {
    pub token: SyntaxKind,
    pub elements: NodeList,
    pub multiline: bool,
}

impl Visit for ImportAttributes {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

pub struct ImportAttribute {
    pub name: Option<NodeId>,
    pub value: NodeId,
}

impl Visit for ImportAttribute {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
        self.value.visit(nodes, &mut visitor);
    }
}

pub struct CallSignature {
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub type_node: Option<NodeId>,
}

impl Visit for CallSignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ConstructSignature {
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub type_node: Option<NodeId>,
}

impl Visit for ConstructSignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct IndexSignature {
    pub modifiers: Option<ModifierList>,
    pub parameters: Option<NodeList>,
    pub type_node: Option<NodeId>,
}

impl Visit for IndexSignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct MethodSignature {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub question_token: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
}

impl Visit for MethodSignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
    }
}

pub struct PropertySignature {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub question_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for PropertySignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

pub struct TypeQuery {
    pub entity_name: NodeId,
    pub type_arguments: Option<NodeList>,
}

impl Visit for TypeQuery {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.entity_name.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
    }
}

pub struct MappedType {
    pub readonly_token: Option<NodeId>,
    pub type_parameter: NodeId,
    pub name_type: Option<NodeId>,
    pub question_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub members: NodeList,
}

impl Visit for MappedType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.readonly_token.visit(nodes, &mut visitor);
        self.type_parameter.visit(nodes, &mut visitor);
        self.name_type.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.members.visit(nodes, &mut visitor);
    }
}

pub struct NamedTupleMember {
    pub dot_dot_dot_token: Option<NodeId>,
    pub name: NodeId,
    pub question_token: Option<NodeId>,
    pub type_node: NodeId,
}

impl Visit for NamedTupleMember {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.dot_dot_dot_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.question_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct OptionalType {
    pub type_node: NodeId,
}

impl Visit for OptionalType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct RestType {
    pub type_node: NodeId,
}

impl Visit for RestType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

pub struct ClassDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

impl Visit for ClassDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.heritage_clauses.visit(nodes, &mut visitor);
        self.members.visit(nodes, &mut visitor);
    }
}

pub struct ClassExpression {
    pub modifiers: Option<ModifierList>,
    pub name: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

impl Visit for ClassExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.heritage_clauses.visit(nodes, &mut visitor);
        self.members.visit(nodes, &mut visitor);
    }
}

pub struct HeritageClause {
    pub token: SyntaxKind,
    pub types: NodeList,
}

impl Visit for HeritageClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

pub struct Constructor {
    pub modifiers: Option<ModifierList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for Constructor {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct PropertyDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for PropertyDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

pub struct ClassStaticBlockDeclaration {
    pub modifiers: Option<ModifierList>,
    pub body: NodeId,
}

impl Visit for ClassStaticBlockDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct NewExpression {
    pub expression: NodeId,
    pub type_arguments: Option<NodeList>,
    pub argument_list: Option<NodeList>,
}

impl Visit for NewExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.type_arguments.visit(nodes, &mut visitor);
        self.argument_list.visit(nodes, &mut visitor);
    }
}

pub struct PartiallyEmittedExpression {
    pub expression: NodeId,
}

impl Visit for PartiallyEmittedExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct FunctionDeclaration {
    pub modifiers: Option<ModifierList>,
    pub asterisk_token: Option<NodeId>,
    pub name: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
    pub full_signature: Option<NodeId>,
    pub body: Option<NodeId>,
}

impl Visit for FunctionDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.asterisk_token.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

pub struct IfStatement {
    pub expression: NodeId,
    pub then_statement: NodeId,
    pub else_statement: Option<NodeId>,
}

impl Visit for IfStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.then_statement.visit(nodes, &mut visitor);
        self.else_statement.visit(nodes, &mut visitor);
    }
}

pub struct DoStatement {
    pub statement: NodeId,
    pub expression: NodeId,
}

impl Visit for DoStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.statement.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct WhileStatement {
    pub expression: NodeId,
    pub statement: NodeId,
}

impl Visit for WhileStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}

pub struct ContinueStatement {
    pub label: Option<NodeId>,
}

impl Visit for ContinueStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.label.visit(nodes, &mut visitor);
    }
}

pub struct BreakStatement {
    pub label: Option<NodeId>,
}

impl Visit for BreakStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.label.visit(nodes, &mut visitor);
    }
}

pub struct ReturnStatement {
    pub expression: Option<NodeId>,
}

impl Visit for ReturnStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct WithStatement {
    pub expression: NodeId,
    pub statement: NodeId,
}

impl Visit for WithStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}

pub struct ThrowStatement {
    pub expression: NodeId,
}

impl Visit for ThrowStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

pub struct ForOfStatement {
    pub await_modifier: Option<NodeId>,
    pub initializer: Option<NodeId>,
    pub expression: NodeId,
    pub statement: NodeId,
}

impl Visit for ForOfStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.await_modifier.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}

pub struct ForInStatement {
    pub initializer: Option<NodeId>,
    pub expression: NodeId,
    pub statement: NodeId,
}

impl Visit for ForInStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.initializer.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}

pub struct ForStatement {
    pub initializer: Option<NodeId>,
    pub condition: Option<NodeId>,
    pub incrementor: Option<NodeId>,
    pub statement: NodeId,
}

impl Visit for ForStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.initializer.visit(nodes, &mut visitor);
        self.condition.visit(nodes, &mut visitor);
        self.incrementor.visit(nodes, &mut visitor);
        self.statement.visit(nodes, &mut visitor);
    }
}
