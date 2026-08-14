use std::{
    any::Any,
    fmt,
    ops::{Index, IndexMut},
    rc::Rc,
};

use crate::{
    diagnostics::Diagnostics,
    flags::{
        ContainerFlags, ModifierFlags, NodeFlags, OuterExpressionKinds, SymbolFlags, TokenFlags,
    },
    flow::FlowNodeId,
    scanner::Scanner,
    symbol::SymbolTable,
    syntax::{CommentDirective, SyntaxKind, TextRange},
};

pub trait NodeData: Any + fmt::Debug {}

impl<T> NodeData for T where T: Any + fmt::Debug {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

#[derive(Debug)]
pub struct Node {
    pub kind: SyntaxKind,
    pub loc: TextRange,
    pub flags: NodeFlags,
    pub parent: Option<NodeId>,
    pub data: Rc<dyn NodeData>,
    pub flow_node: Option<FlowNodeId>,
    locals_data: Option<Box<LocalsData>>,
}

#[derive(Debug, Default)]
pub struct LocalsData {
    pub locals: SymbolTable,
    pub next_container: Option<NodeId>,
}

impl Node {
    pub fn is_js_type_alias_declaration(&self) -> bool {
        self.kind == SyntaxKind::TypeAliasDeclaration
    }

    pub fn is_js_import_declaration(&self) -> bool {
        self.kind == SyntaxKind::JSImportDeclaration
    }

    pub fn data<T: NodeData>(&self) -> Rc<T> {
        (self.data.clone() as Rc<dyn Any>).downcast().unwrap()
    }

    pub fn data_ref<T: NodeData>(&self) -> &T {
        (self.data.as_ref() as &dyn Any).downcast_ref().unwrap()
    }

    pub fn is_missing(&self) -> bool {
        self.loc.len() == 0 && self.kind != SyntaxKind::EndOfFile
    }

    pub fn is_present(&self) -> bool {
        !self.is_missing()
    }

    pub fn is_in_js_file(&self) -> bool {
        self.flags.contains(NodeFlags::JavaScriptFile)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JSDeclarationKind {
    None,
    // module.exports = expr, except for module.exports = exports
    ModuleExports,
    // exports.name = expr
    // module.exports.name = expr
    ExportsProperty,
    // this.name = expr
    ThisProperty,
    // F.name = expr, F[name] = expr, in JS or TS file
    Property,
    // Object.defineProperty(x, 'name', { value: any, writable?: boolean (false by default) });
    // Object.defineProperty(x, 'name', { get: Function, set: Function });
    // Object.defineProperty(x, 'name', { get: Function });
    // Object.defineProperty(x, 'name', { set: Function });
    ObjectDefinePropertyValue,
    // Object.defineProperty(exports || module.exports, 'name', ...);
    ObjectDefinePropertyExports,
}

pub struct NodeFactory {
    store: Vec<Node>,
}

impl NodeFactory {
    pub fn new() -> Self {
        Self { store: Vec::new() }
    }

    pub fn create<T: NodeData>(&mut self, kind: SyntaxKind, data: T) -> NodeId {
        let id = NodeId(self.store.len());
        self.store.push(Node {
            kind,
            data: Rc::new(data),
            flags: NodeFlags::default(),
            loc: TextRange::default(),
            parent: None,
            flow_node: None,
            locals_data: None,
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
            ForStatement,
            SwitchStatement,
            CaseBlock,
            CaseClause,
            DefaultClause,
            TryStatement,
            CatchClause,
            InterfaceDeclaration,
            TypeAliasDeclaration,
            EnumDeclaration,
            EnumMember,
            ModuleDeclaration,
            ModuleBlock,
            ImportDeclaration,
            ImportEqualsDeclaration,
            ExternalModuleReference,
            ImportClause,
            NamespaceImport,
            NamedImports,
            ImportSpecifier,
            ExportAssignment,
            NamespaceExportDeclaration,
            ExportDeclaration,
            NamespaceExport,
            NamedExports,
            ExportSpecifier,
            Decorator
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

    pub fn parent_is(&self, node: NodeId, kind: SyntaxKind) -> bool {
        self[node].parent.is_some_and(|parent| self[parent].kind == kind)
    }

    pub fn has_modifier(&self, modifiers: &Option<ModifierList>, modifier: ModifierFlags) -> bool {
        modifiers.as_ref().is_some_and(|x| x.flags.intersects(modifier))
    }

    pub fn is_left_hand_side_expression(&self, expr: NodeId) -> bool {
        let expr = self.skip_partially_emitted_expressions(expr);
        self[expr].kind.is_left_hand_side_expression()
    }

    pub fn is_part_of_type_query(&self, mut node: NodeId) -> bool {
        while let SyntaxKind::QualifiedName | SyntaxKind::Identifier = self[node].kind {
            node = self[node].parent.unwrap();
        }
        self.is(node, SyntaxKind::TypeQuery)
    }

    pub fn declaration_name_to_string(&self, node: NodeId) -> String {
        if self[node].is_missing() {
            return String::from("(Missing)");
        }
        self.get_text_of_node(node)
    }

    fn get_text_of_node(&self, node: NodeId) -> String {
        let source_file = self.get_source_file_of_node(node).unwrap();
        self.get_source_text_of_node_from_source_file(source_file, node, false)
    }

    fn get_source_text_of_node_from_source_file(
        &self,
        source_file: NodeId,
        node: NodeId,
        include_trivia: bool,
    ) -> String {
        let text = &self[source_file].data_ref::<SourceFile>().source_text;
        Scanner::get_text_of_node_from_source_text(text, &self[node], include_trivia)
    }

    fn get_source_file_of_node(&self, mut node: NodeId) -> Option<NodeId> {
        loop {
            if self.is(node, SyntaxKind::SourceFile) {
                return Some(node);
            }
            node = self[node].parent?;
        }
    }

    pub fn is_object_literal_method(&self, node: NodeId) -> bool {
        self.is(node, SyntaxKind::MethodDeclaration)
            && self.parent_is(node, SyntaxKind::ObjectLiteralExpression)
    }

    pub fn is_async_function(&self, node: NodeId) -> bool {
        match self[node].kind {
            SyntaxKind::FunctionDeclaration => {
                let data = self[node].data_ref::<FunctionDeclaration>();
                data.asterisk_token.is_none()
                    && self.has_modifier(&data.modifiers, ModifierFlags::Static)
            }
            SyntaxKind::FunctionExpression => {
                let data = self[node].data_ref::<FunctionExpression>();
                data.asterisk_token.is_none()
                    && self.has_modifier(&data.modifiers, ModifierFlags::Static)
            }
            SyntaxKind::ArrowFunction => {
                let data = self[node].data_ref::<ArrowFunction>();
                data.asterisk_token.is_none()
                    && self.has_modifier(&data.modifiers, ModifierFlags::Static)
            }
            SyntaxKind::MethodDeclaration => {
                let data = self[node].data_ref::<MethodDeclaration>();
                data.asterisk_token.is_none()
                    && self.has_modifier(&data.modifiers, ModifierFlags::Static)
            }
            _ => false,
        }
    }

    pub fn has_dynamic_name(&self, node: NodeId) -> bool {
        todo!()
    }

    pub fn get_optional_symbol_flag_for_node(&self, node: NodeId) -> SymbolFlags {
        let postfix_token = self.postfix_token(node);
        todo!()
    }

    pub fn is_narrowable_reference(&self, node: NodeId) -> bool {
        let node = &self[node];
        match node.kind {
            SyntaxKind::Identifier
            | SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::MetaProperty => true,
            SyntaxKind::PropertyAccessExpression => {
                self.is_narrowable_reference(node.data_ref::<PropertyAccessExpression>().expression)
            }
            SyntaxKind::ParenthesizedExpression => {
                self.is_narrowable_reference(node.data_ref::<ParenthesizedExpression>().expression)
            }
            SyntaxKind::NonNullExpression => {
                self.is_narrowable_reference(node.data_ref::<NonNullExpression>().expression)
            }
            SyntaxKind::ElementAccessExpression => {
                let expr = node.data_ref::<ElementAccessExpression>();
                self.is_string_or_numeric_literal_like(expr.argument_expression)
                    || self.is_entity_name_expression(expr.argument_expression)
                        && self.is_narrowable_reference(expr.expression)
            }
            SyntaxKind::BinaryExpression => {
                let expr = node.data_ref::<BinaryExpression>();
                self[expr.operator_token].kind == SyntaxKind::CommaToken
                    && self.is_narrowable_reference(expr.right)
                    || self[expr.operator_token].kind.is_assignment_operator()
                        && self.is_left_hand_side_expression(expr.left)
            }
            _ => false,
        }
    }

    pub fn get_assignment_declaration_kind(&self, node: NodeId) -> JSDeclarationKind {
        match self[node].kind {
            SyntaxKind::BinaryExpression => {
                let bin = self[node].data_ref::<BinaryExpression>();
                if self.is(bin.operator_token, SyntaxKind::EqualsToken)
                    && !self.is_access_expression(bin.left)
                {
                    let left = &self[bin.left];
                    if left.is_in_js_file() {
                        if self.is_module_exports_access_expression(bin.left)
                            && !self.is_exports_identifier(bin.right)
                        {
                            return JSDeclarationKind::ModuleExports;
                        }
                        if self
                            .is_module_exports_access_expression(self.expression(bin.left).unwrap())
                            || self.is_exports_identifier(self.expression(bin.left).unwrap())
                                && self.get_element_or_property_access_name(bin.left).is_some()
                        {
                            return JSDeclarationKind::ExportsProperty;
                        }
                        if self.is(self.expression(bin.left).unwrap(), SyntaxKind::ThisKeyword) {
                            return JSDeclarationKind::ThisProperty;
                        }
                    }
                    if left.kind == SyntaxKind::PropertyAccessExpression
                        && self.is_entity_name_expression_ex(
                            left.data_ref::<PropertyAccessExpression>().expression,
                            left.is_in_js_file(),
                        )
                        && self.is(self.name(bin.left).unwrap(), SyntaxKind::Identifier)
                        || left.kind == SyntaxKind::ElementAccessExpression
                            && self.is_entity_name_expression_ex(
                                left.data_ref::<ElementAccessExpression>().expression,
                                left.is_in_js_file(),
                            )
                    {
                        return JSDeclarationKind::Property;
                    }
                }
            }
            SyntaxKind::CallExpression => {
                if self[node].is_in_js_file() && self.is_bindable_object_define_property_call(node)
                {
                    let call = &self[node].data_ref::<CallExpression>();
                    let entity_name = call.argument_list.nodes[0];
                    return if self.is_exports_identifier(entity_name)
                        || self.is_module_exports_access_expression(entity_name)
                    {
                        JSDeclarationKind::ObjectDefinePropertyExports
                    } else {
                        JSDeclarationKind::ObjectDefinePropertyValue
                    };
                }
            }
            _ => {}
        }
        JSDeclarationKind::None
    }

    fn is_bindable_object_define_property_call(&self, node: NodeId) -> bool {
        todo!()
    }

    fn is_exports_identifier(&self, node: NodeId) -> bool {
        self.is(node, SyntaxKind::Identifier)
            && self[node].data_ref::<Identifier>().text == "exports"
    }

    fn is_module_identifier(&self, node: NodeId) -> bool {
        self.is(node, SyntaxKind::Identifier)
            && self[node].data_ref::<Identifier>().text == "module"
    }

    fn is_this_identifier(&self, node: NodeId) -> bool {
        self.is(node, SyntaxKind::Identifier) && self[node].data_ref::<Identifier>().text == "this"
    }

    fn is_module_exports_access_expression(&self, node: NodeId) -> bool {
        todo!()
    }

    fn get_element_or_property_access_name(&self, node: NodeId) -> Option<NodeId> {
        todo!()
    }

    /**
     * Declares a Symbol for the node and adds it to symbols. Reports errors for conflicting identifier names.
     * @param symbolTable - The symbol table which node will be added to.
     * @param parent - node's parent declaration.
     * @param node - The declaration to be added to the symbol table
     * @param includes - The SymbolFlags that node has in addition to its declaration type (eg: export, ambient, etc.)
     * @param excludes - The flags which node cannot be declared alongside in a symbol table. Used to report forbidden declarations.
     */
    pub fn get_container_flags(&self, node: NodeId) -> ContainerFlags {
        match self[node].kind {
            SyntaxKind::ClassExpression
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::ObjectLiteralExpression
            | SyntaxKind::TypeLiteral
            | SyntaxKind::JsxAttributes => ContainerFlags::IsContainer,
            SyntaxKind::InterfaceDeclaration => {
                ContainerFlags::IsContainer | ContainerFlags::IsInterface
            }
            SyntaxKind::ModuleDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::JSTypeAliasDeclaration
            | SyntaxKind::MappedType
            | SyntaxKind::IndexSignature => ContainerFlags::IsContainer | ContainerFlags::HasLocals,
            SyntaxKind::SourceFile => {
                ContainerFlags::IsContainer
                    | ContainerFlags::IsControlFlowContainer
                    | ContainerFlags::HasLocals
            }
            SyntaxKind::GetAccessor | SyntaxKind::SetAccessor | SyntaxKind::MethodDeclaration => {
                if self.is_object_literal_or_class_expression_method_or_accessor(node) {
                    ContainerFlags::IsContainer
                        | ContainerFlags::IsControlFlowContainer
                        | ContainerFlags::HasLocals
                        | ContainerFlags::IsFunctionLike
                        | ContainerFlags::IsObjectLiteralOrClassExpressionMethodOrAccessor
                        | ContainerFlags::IsThisContainer
                } else {
                    ContainerFlags::IsContainer
                        | ContainerFlags::IsControlFlowContainer
                        | ContainerFlags::HasLocals
                        | ContainerFlags::IsFunctionLike
                        | ContainerFlags::IsThisContainer
                }
            }
            SyntaxKind::Constructor
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::ClassStaticBlockDeclaration => {
                ContainerFlags::IsContainer
                    | ContainerFlags::IsControlFlowContainer
                    | ContainerFlags::HasLocals
                    | ContainerFlags::IsFunctionLike
                    | ContainerFlags::IsThisContainer
            }
            SyntaxKind::MethodSignature
            | SyntaxKind::CallSignature
            | SyntaxKind::FunctionType
            | SyntaxKind::ConstructSignature
            | SyntaxKind::ConstructorType => {
                ContainerFlags::IsContainer
                    | ContainerFlags::IsControlFlowContainer
                    | ContainerFlags::HasLocals
                    | ContainerFlags::IsFunctionLike
                    | ContainerFlags::PropagatesThisKeyword
            }
            SyntaxKind::FunctionExpression => {
                ContainerFlags::IsContainer
                    | ContainerFlags::IsControlFlowContainer
                    | ContainerFlags::HasLocals
                    | ContainerFlags::IsFunctionLike
                    | ContainerFlags::IsFunctionExpression
                    | ContainerFlags::IsThisContainer
            }
            SyntaxKind::ArrowFunction => {
                ContainerFlags::IsContainer
                    | ContainerFlags::IsControlFlowContainer
                    | ContainerFlags::HasLocals
                    | ContainerFlags::IsFunctionLike
                    | ContainerFlags::IsFunctionExpression
                    | ContainerFlags::PropagatesThisKeyword
            }
            SyntaxKind::ModuleBlock => ContainerFlags::IsControlFlowContainer,
            SyntaxKind::PropertyDeclaration => {
                if self[node].data_ref::<PropertyDeclaration>().initializer.is_some() {
                    ContainerFlags::IsControlFlowContainer | ContainerFlags::IsThisContainer
                } else {
                    ContainerFlags::empty()
                }
            }
            SyntaxKind::CatchClause
            | SyntaxKind::ForStatement
            | SyntaxKind::ForInStatement
            | SyntaxKind::ForOfStatement
            | SyntaxKind::CaseBlock => {
                ContainerFlags::IsBlockScopedContainer | ContainerFlags::HasLocals
            }
            SyntaxKind::Block => {
                if let Some(parent) = self[node].parent
                    && (self.is_function_like(parent)
                        || self.is_class_static_block_declaration(parent))
                {
                    ContainerFlags::empty()
                } else {
                    ContainerFlags::IsBlockScopedContainer | ContainerFlags::HasLocals
                }
            }
            _ => ContainerFlags::empty(),
        }
    }

    pub fn is_function_like(&self, node: NodeId) -> bool {
        self.is_function_like_kind(self[node].kind)
    }

    fn is_function_like_kind(&self, kind: SyntaxKind) -> bool {
        if matches!(
            kind,
            SyntaxKind::MethodSignature
                | SyntaxKind::CallSignature
                | SyntaxKind::JSDocSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType
        ) {
            return true;
        }
        self.is_function_like_declaration_kind(kind)
    }

    fn is_function_like_declaration_kind(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
        )
    }

    fn is_class_static_block_declaration(&self, node: NodeId) -> bool {
        self.is(node, SyntaxKind::ClassStaticBlockDeclaration)
    }

    fn is_object_literal_or_class_expression_method_or_accessor(&self, node: NodeId) -> bool {
        matches!(
            self[node].kind,
            SyntaxKind::MethodDeclaration | SyntaxKind::GetAccessor | SyntaxKind::SetAccessor
        ) && self[node].parent.is_some_and(|parent| {
            matches!(
                self[parent].kind,
                SyntaxKind::ObjectLiteralExpression | SyntaxKind::ClassExpression
            )
        })
    }

    pub fn expression(&self, node: NodeId) -> Option<NodeId> {
        let node = &self[node];
        Some(match node.kind {
            SyntaxKind::PropertyAccessExpression => {
                node.data_ref::<PropertyAccessExpression>().expression
            }
            SyntaxKind::ElementAccessExpression => {
                node.data_ref::<ElementAccessExpression>().expression
            }
            SyntaxKind::ParenthesizedExpression => {
                node.data_ref::<ParenthesizedExpression>().expression
            }
            SyntaxKind::CallExpression => node.data_ref::<CallExpression>().expression,
            SyntaxKind::NewExpression => node.data_ref::<NewExpression>().expression,
            SyntaxKind::ExpressionWithTypeArguments => {
                node.data_ref::<ExpressionWithTypeArguments>().expression
            }
            SyntaxKind::ComputedPropertyName => node.data_ref::<ComputedPropertyName>().expression,
            SyntaxKind::NonNullExpression => node.data_ref::<NonNullExpression>().expression,
            SyntaxKind::TypeAssertionExpression => {
                node.data_ref::<TypeAssertionExpression>().expression
            }
            SyntaxKind::AsExpression => node.data_ref::<AsExpression>().expression,
            SyntaxKind::SatisfiesExpression => node.data_ref::<SatisfiesExpression>().expression,
            SyntaxKind::TypeOfExpression => node.data_ref::<TypeOfExpression>().expression,
            SyntaxKind::SpreadAssignment => node.data_ref::<SpreadAssignment>().expression,
            SyntaxKind::SpreadElement => node.data_ref::<SpreadElement>().expression,
            SyntaxKind::TemplateSpan => node.data_ref::<TemplateSpan>().expression,
            SyntaxKind::DeleteExpression => node.data_ref::<DeleteExpression>().expression,
            SyntaxKind::VoidExpression => node.data_ref::<VoidExpression>().expression,
            SyntaxKind::AwaitExpression => node.data_ref::<AwaitExpression>().expression,
            SyntaxKind::YieldExpression => return node.data_ref::<YieldExpression>().expression,
            SyntaxKind::PartiallyEmittedExpression => {
                node.data_ref::<PartiallyEmittedExpression>().expression
            }
            SyntaxKind::IfStatement => node.data_ref::<IfStatement>().expression,
            SyntaxKind::DoStatement => node.data_ref::<DoStatement>().expression,
            SyntaxKind::WhileStatement => node.data_ref::<WhileStatement>().expression,
            SyntaxKind::WithStatement => node.data_ref::<WithStatement>().expression,
            SyntaxKind::ForInStatement => node.data_ref::<ForInStatement>().expression,
            SyntaxKind::ForOfStatement => node.data_ref::<ForOfStatement>().expression,
            SyntaxKind::SwitchStatement => node.data_ref::<SwitchStatement>().expression,
            SyntaxKind::CaseClause => node.data_ref::<CaseClause>().expression,
            SyntaxKind::ExpressionStatement => node.data_ref::<ExpressionStatement>().expression,
            SyntaxKind::ReturnStatement => return node.data_ref::<ReturnStatement>().expression,
            SyntaxKind::ThrowStatement => node.data_ref::<ThrowStatement>().expression,
            SyntaxKind::ExternalModuleReference => {
                node.data_ref::<ExternalModuleReference>().expression
            }
            SyntaxKind::ExportAssignment => node.data_ref::<ExportAssignment>().expression,
            SyntaxKind::Decorator => node.data_ref::<Decorator>().expression,
            // SyntaxKind::JsxExpression => node.data_ref::<JsxExpression>().expression,
            // SyntaxKind::JsxSpreadAttribute => node.data_ref::<JsxSpreadAttribute>().expression,
            _ => panic!("Unhandled case in nodes.expression()"),
        })
    }

    pub fn name(&self, node: NodeId) -> Option<NodeId> {
        let node = &self[node];
        Some(match node.kind {
            SyntaxKind::VariableDeclaration => node.data_ref::<VariableDeclaration>().name,
            SyntaxKind::Parameter => node.data_ref::<Parameter>().name,
            SyntaxKind::BindingElement => return node.data_ref::<BindingElement>().name,
            SyntaxKind::FunctionDeclaration => return node.data_ref::<FunctionDeclaration>().name,
            SyntaxKind::ClassDeclaration => return node.data_ref::<ClassDeclaration>().name,
            SyntaxKind::ClassExpression => return node.data_ref::<ClassExpression>().name,
            SyntaxKind::InterfaceDeclaration => node.data_ref::<InterfaceDeclaration>().name,
            SyntaxKind::TypeAliasDeclaration => node.data_ref::<TypeAliasDeclaration>().name,
            SyntaxKind::EnumMember => node.data_ref::<EnumMember>().name,
            SyntaxKind::EnumDeclaration => node.data_ref::<EnumDeclaration>().name,
            SyntaxKind::NamespaceImport => node.data_ref::<NamespaceImport>().name,
            SyntaxKind::NamespaceExportDeclaration => {
                node.data_ref::<NamespaceExportDeclaration>().name
            }
            SyntaxKind::NamespaceExport => node.data_ref::<NamespaceExport>().name,
            SyntaxKind::ExportSpecifier => node.data_ref::<ExportSpecifier>().name,
            SyntaxKind::GetAccessor => node.data_ref::<GetAccessor>().name,
            SyntaxKind::SetAccessor => node.data_ref::<SetAccessor>().name,
            SyntaxKind::MethodSignature => node.data_ref::<MethodSignature>().name,
            SyntaxKind::MethodDeclaration => node.data_ref::<MethodDeclaration>().name,
            SyntaxKind::PropertySignature => node.data_ref::<PropertySignature>().name,
            SyntaxKind::PropertyDeclaration => node.data_ref::<PropertyDeclaration>().name,
            SyntaxKind::FunctionExpression => return node.data_ref::<FunctionExpression>().name,
            SyntaxKind::PropertyAccessExpression => {
                node.data_ref::<PropertyAccessExpression>().name
            }
            SyntaxKind::MetaProperty => node.data_ref::<MetaProperty>().name,
            SyntaxKind::PropertyAssignment => node.data_ref::<PropertyAssignment>().name,
            SyntaxKind::ShorthandPropertyAssignment => {
                node.data_ref::<ShorthandPropertyAssignment>().name
            }
            SyntaxKind::ImportAttribute => return node.data_ref::<ImportAttribute>().name,
            SyntaxKind::NamedTupleMember => node.data_ref::<NamedTupleMember>().name,
            // SyntaxKind::JsxNamespacedName => node.data_ref::<JsxNamespacedName>().name,
            // SyntaxKind::JsxAttribute => node.data_ref::<JsxAttribute>().name,
            // SyntaxKind::JSDocCallbackTag => node.data_ref::<JSDocCallbackTag>().name,
            // SyntaxKind::JSDocTypedefTag => node.data_ref::<JSDocTypedefTag>().name,
            // SyntaxKind::JSDocNameReference => node.data_ref::<JSDocNameReference>().name,
            SyntaxKind::ModuleDeclaration => node.data_ref::<ModuleDeclaration>().name,
            SyntaxKind::ImportEqualsDeclaration => {
                return node.data_ref::<ImportEqualsDeclaration>().name;
            }
            SyntaxKind::ImportClause => return node.data_ref::<ImportClause>().name,
            SyntaxKind::ImportSpecifier => node.data_ref::<ImportSpecifier>().name,
            // SyntaxKind::JSDocLink => node.data_ref::<JSDocLink>().name,
            // SyntaxKind::JSDocLinkPlain => node.data_ref::<JSDocLinkPlain>().name,
            // SyntaxKind::JSDocLinkCode => node.data_ref::<JSDocLinkCode>().name,
            SyntaxKind::TypeParameter => node.data_ref::<TypeParameter>().name,
            // SyntaxKind::JSDocParameterOrPropertyTag => node.data_ref::<JSDocParameterOrPropertyTag>().name,
            _ => return None,
        })
    }

    pub fn postfix_token(&self, node: NodeId) -> Option<NodeId> {
        let node = &self[node];
        match node.kind {
            SyntaxKind::MethodDeclaration => node.data_ref::<MethodDeclaration>().postfix_token,
            SyntaxKind::ShorthandPropertyAssignment => {
                node.data_ref::<ShorthandPropertyAssignment>().postfix_token
            }
            SyntaxKind::MethodSignature => node.data_ref::<MethodSignature>().postfix_token,
            SyntaxKind::PropertySignature => node.data_ref::<PropertySignature>().postfix_token,
            SyntaxKind::PropertyAssignment => node.data_ref::<PropertyAssignment>().postfix_token,
            SyntaxKind::PropertyDeclaration => node.data_ref::<PropertyDeclaration>().postfix_token,
            SyntaxKind::EnumMember => node.data_ref::<EnumMember>().postfix_token,
            SyntaxKind::GetAccessor => node.data_ref::<GetAccessor>().postfix_token,
            SyntaxKind::SetAccessor => node.data_ref::<SetAccessor>().postfix_token,
            _ => None,
        }
    }

    pub fn is_access_expression(&self, node: NodeId) -> bool {
        matches!(
            self[node].kind,
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
        )
    }

    pub fn skip_partially_emitted_expressions(&self, node: NodeId) -> NodeId {
        self.skip_outer_expressions(node, OuterExpressionKinds::PartiallyEmittedExpressions)
    }

    fn is_string_or_numeric_literal_like(&self, node: NodeId) -> bool {
        self.is_string_literal_like(node) || self.is(node, SyntaxKind::NumericLiteral)
    }

    fn is_string_literal_like(&self, node: NodeId) -> bool {
        matches!(
            self[node].kind,
            SyntaxKind::StringLiteral | SyntaxKind::NoSubstitutionTemplateLiteral
        )
    }

    fn is_entity_name_expression(&self, node: NodeId) -> bool {
        self.is_entity_name_expression_ex(node, false)
    }

    fn is_entity_name_expression_ex(&self, node: NodeId, allow_js: bool) -> bool {
        self.is(node, SyntaxKind::Identifier)
            || self.is_property_access_entity_name_expression(node, allow_js)
            || allow_js
                && (self.is(node, SyntaxKind::ThisKeyword)
                    || self.is_element_access_entity_name_expression(node, allow_js))
    }

    fn is_property_access_entity_name_expression(&self, node: NodeId, allow_js: bool) -> bool {
        if !self.is(node, SyntaxKind::PropertyAccessExpression) {
            return false;
        }
        let expr = self[node].data_ref::<PropertyAccessExpression>();
        self.is(expr.name, SyntaxKind::Identifier)
            && self.is_entity_name_expression_ex(expr.expression, allow_js)
    }

    fn is_element_access_entity_name_expression(&self, node: NodeId, allow_js: bool) -> bool {
        if !self.is(node, SyntaxKind::ElementAccessExpression) {
            return false;
        }
        let expr = self[node].data_ref::<ElementAccessExpression>();
        self.is_string_or_numeric_literal_like(expr.argument_expression)
            && self.is_entity_name_expression_ex(expr.expression, allow_js)
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

    pub fn is_external_or_common_js_module(&self, node: NodeId) -> bool {
        let file = self[node].data_ref::<SourceFile>();
        file.external_module_indicator.is_some() || file.common_js_module_indicator.is_some()
    }

    pub fn get_locals_mut(&mut self, node: NodeId) -> &mut SymbolTable {
        &mut self[node].locals_data.get_or_insert_default().locals
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
    pub diagnostics: Diagnostics,
    pub bind_diagnostics: Diagnostics,
    pub external_module_indicator: Option<NodeId>,
    pub common_js_module_indicator: Option<NodeId>,
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

#[derive(Debug)]
pub struct Block {
    pub statements: NodeList,
    pub multiline: bool,
}

impl Visit for Block {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.statements.visit(nodes, visitor);
    }
}

#[derive(Debug, Clone)]
pub struct ModifierList {
    pub list: NodeList,
    pub flags: ModifierFlags,
}

impl Visit for ModifierList {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.list.visit(nodes, visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct VariableDeclarationList {
    pub declarations: NodeList,
    pub flags: NodeFlags,
}

impl Visit for VariableDeclarationList {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.declarations.visit(nodes, visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ArrayBindingPattern {
    pub elements: NodeList,
}

impl Visit for ArrayBindingPattern {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ObjectBindingPattern {
    pub elements: NodeList,
}

impl Visit for ObjectBindingPattern {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Identifier {
    pub text: String,
}

#[derive(Debug)]
pub struct PrivateIdentifier {
    pub text: String,
}

#[derive(Debug)]
pub struct StringLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

#[derive(Debug)]
pub struct NumericLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

#[derive(Debug)]
pub struct BigIntLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

#[derive(Debug)]
pub struct RegularExpressionLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

#[derive(Debug)]
pub struct NoSubstitutionTemplateLiteral {
    pub text: String,
    pub token_flags: TokenFlags,
}

#[derive(Debug)]
pub struct ComputedPropertyName {
    pub expression: NodeId,
}

impl Visit for ComputedPropertyName {
    fn visit(&self, nodes: &mut NodeFactory, visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct UnionType {
    pub types: NodeList,
}

impl Visit for UnionType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct IntersectionType {
    pub types: NodeList,
}

impl Visit for IntersectionType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TypeOperator {
    pub operator: SyntaxKind,
    pub type_node: NodeId,
}

impl Visit for TypeOperator {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct InferType {
    pub type_parameter: NodeId,
}

impl Visit for InferType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_parameter.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct JSDocNonNullableType {
    pub type_node: NodeId,
}

impl Visit for JSDocNonNullableType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct JSDocNullableType {
    pub type_node: NodeId,
}

impl Visit for JSDocNullableType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ParenthesizedType {
    pub type_node: NodeId,
}

impl Visit for ParenthesizedType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ArrayType {
    pub type_node: NodeId,
}

impl Visit for ArrayType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ArrowFunction {
    pub modifiers: Option<ModifierList>,
    pub asterisk_token: Option<NodeId>,
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct PrefixUnaryExpression {
    pub operator: SyntaxKind,
    pub expression: NodeId,
}

impl Visit for PrefixUnaryExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}
#[derive(Debug)]
pub struct PostfixUnaryExpression {
    pub expression: NodeId,
    pub operator: SyntaxKind,
}

impl Visit for PostfixUnaryExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct DeleteExpression {
    pub expression: NodeId,
}

impl Visit for DeleteExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TypeOfExpression {
    pub expression: NodeId,
}

impl Visit for TypeOfExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct VoidExpression {
    pub expression: NodeId,
}

impl Visit for VoidExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct AwaitExpression {
    pub expression: NodeId,
}

impl Visit for AwaitExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct MetaProperty {
    pub keyword_token: SyntaxKind,
    pub name: NodeId,
}

impl Visit for MetaProperty {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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
#[derive(Debug)]
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

#[derive(Debug)]
pub struct ParenthesizedExpression {
    pub expression: NodeId,
}

impl Visit for ParenthesizedExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ArrayLiteralExpression {
    pub elements: NodeList,
    pub multiline: bool,
}

impl Visit for ArrayLiteralExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct SpreadElement {
    pub expression: NodeId,
}

impl Visit for SpreadElement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ObjectLiteralExpression {
    pub properties: NodeList,
    pub multiline: bool,
}

impl Visit for ObjectLiteralExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.properties.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct SpreadAssignment {
    pub expression: NodeId,
}

impl Visit for SpreadAssignment {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct GetAccessor {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
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

#[derive(Debug)]
pub struct SetAccessor {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
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

#[derive(Debug)]
pub struct MethodDeclaration {
    pub modifiers: Option<ModifierList>,
    pub asterisk_token: Option<NodeId>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
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
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.full_signature.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct MissingDeclaration {
    pub modifiers: Option<ModifierList>,
}

impl Visit for MissingDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct TemplateHead {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

#[derive(Debug)]
pub struct TemplateMiddle {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

#[derive(Debug)]
pub struct TemplateTail {
    pub text: String,
    pub raw_text: String,
    pub template_flags: TokenFlags,
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct NonNullExpression {
    pub expression: NodeId,
}

impl Visit for NonNullExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ExpressionStatement {
    pub expression: NodeId,
}

impl Visit for ExpressionStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct LiteralType {
    pub expression: NodeId,
}

impl Visit for LiteralType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TypeLiteral {
    pub members: NodeList,
}

impl Visit for TypeLiteral {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.members.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TupleType {
    pub elements: NodeList,
}

impl Visit for TupleType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.elements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct MethodSignature {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub return_type: Option<NodeId>,
}

impl Visit for MethodSignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.parameters.visit(nodes, &mut visitor);
        self.return_type.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct PropertySignature {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub postfix_token: Option<NodeId>,
    pub type_node: Option<NodeId>,
    pub initializer: Option<NodeId>,
}

impl Visit for PropertySignature {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.postfix_token.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct OptionalType {
    pub type_node: NodeId,
}

impl Visit for OptionalType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct RestType {
    pub type_node: NodeId,
}

impl Visit for RestType {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct HeritageClause {
    pub token: SyntaxKind,
    pub types: NodeList,
}

impl Visit for HeritageClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.types.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct PartiallyEmittedExpression {
    pub expression: NodeId,
}

impl Visit for PartiallyEmittedExpression {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ContinueStatement {
    pub label: Option<NodeId>,
}

impl Visit for ContinueStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.label.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct BreakStatement {
    pub label: Option<NodeId>,
}

impl Visit for BreakStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.label.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ReturnStatement {
    pub expression: Option<NodeId>,
}

impl Visit for ReturnStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ThrowStatement {
    pub expression: NodeId,
}

impl Visit for ThrowStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct SwitchStatement {
    pub expression: NodeId,
    pub case_block: NodeId,
}

impl Visit for SwitchStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.case_block.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct CaseBlock {
    pub clauses: NodeList,
}

impl Visit for CaseBlock {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.clauses.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct CaseClause {
    pub expression: NodeId,
    pub statements: NodeList,
}

impl Visit for CaseClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
        self.statements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct DefaultClause {
    pub statements: NodeList,
}

impl Visit for DefaultClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.statements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TryStatement {
    pub try_block: NodeId,
    pub catch_clause: Option<NodeId>,
    pub finally_block: Option<NodeId>,
}

impl Visit for TryStatement {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.try_block.visit(nodes, &mut visitor);
        self.catch_clause.visit(nodes, &mut visitor);
        self.finally_block.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct CatchClause {
    pub variable_declaration: Option<NodeId>,
    pub block: NodeId,
}

impl Visit for CatchClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.variable_declaration.visit(nodes, &mut visitor);
        self.block.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct InterfaceDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

impl Visit for InterfaceDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.heritage_clauses.visit(nodes, &mut visitor);
        self.members.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct TypeAliasDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub type_parameters: Option<NodeList>,
    pub type_node: NodeId,
}

impl Visit for TypeAliasDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.type_parameters.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct EnumDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
    pub members: NodeList,
}

impl Visit for EnumDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.members.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct EnumMember {
    pub name: NodeId,
    pub initializer: Option<NodeId>,
    pub postfix_token: Option<NodeId>,
}

impl Visit for EnumMember {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
        self.initializer.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ModuleDeclaration {
    pub modifiers: Option<ModifierList>,
    pub keyword: SyntaxKind,
    pub name: NodeId,
    pub body: Option<NodeId>,
}

impl Visit for ModuleDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.body.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ModuleBlock {
    pub statements: NodeList,
}

impl Visit for ModuleBlock {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.statements.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ImportDeclaration {
    pub modifiers: Option<ModifierList>,
    pub import_clause: Option<NodeId>,
    pub module_specifier: NodeId,
    pub attributes: Option<NodeId>,
}

impl Visit for ImportDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.import_clause.visit(nodes, &mut visitor);
        self.module_specifier.visit(nodes, &mut visitor);
        self.attributes.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ImportEqualsDeclaration {
    pub modifiers: Option<ModifierList>,
    pub is_type_only: bool,
    pub name: Option<NodeId>,
    pub module_reference: NodeId,
}

impl Visit for ImportEqualsDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
        self.module_reference.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ExternalModuleReference {
    pub expression: NodeId,
}

impl Visit for ExternalModuleReference {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ImportClause {
    pub phase_modifier: SyntaxKind,
    pub name: Option<NodeId>,
    pub named_bindings: Option<NodeId>,
}

impl Visit for ImportClause {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
        self.named_bindings.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct NamespaceImport {
    pub name: NodeId,
}

impl Visit for NamespaceImport {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct NamedImports {
    pub imports: NodeList,
}

impl Visit for NamedImports {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.imports.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ImportSpecifier {
    pub is_type_only: bool,
    pub property_name: Option<NodeId>,
    pub name: NodeId,
}

impl Visit for ImportSpecifier {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.property_name.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ExportAssignment {
    pub modifiers: Option<ModifierList>,
    pub is_export_equals: bool,
    pub type_node: Option<NodeId>,
    pub expression: NodeId,
}

impl Visit for ExportAssignment {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.type_node.visit(nodes, &mut visitor);
        self.expression.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct NamespaceExportDeclaration {
    pub modifiers: Option<ModifierList>,
    pub name: NodeId,
}

impl Visit for NamespaceExportDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ExportDeclaration {
    pub modifiers: Option<ModifierList>,
    pub is_type_only: bool,
    pub export_clause: Option<NodeId>,
    pub module_specifier: Option<NodeId>,
    pub attributes: Option<NodeId>,
}

impl Visit for ExportDeclaration {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.modifiers.visit(nodes, &mut visitor);
        self.export_clause.visit(nodes, &mut visitor);
        self.module_specifier.visit(nodes, &mut visitor);
        self.attributes.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct NamespaceExport {
    pub name: NodeId,
}

impl Visit for NamespaceExport {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct NamedExports {
    pub exports: NodeList,
}

impl Visit for NamedExports {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.exports.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct ExportSpecifier {
    pub is_type_only: bool,
    pub property_name: Option<NodeId>,
    pub name: NodeId,
}

impl Visit for ExportSpecifier {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.property_name.visit(nodes, &mut visitor);
        self.name.visit(nodes, &mut visitor);
    }
}

#[derive(Debug)]
pub struct Decorator {
    pub expression: NodeId,
}

impl Visit for Decorator {
    fn visit(&self, nodes: &mut NodeFactory, mut visitor: impl FnMut(&mut Node)) {
        self.expression.visit(nodes, &mut visitor);
    }
}
