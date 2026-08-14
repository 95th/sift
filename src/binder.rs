use rustc_hash::FxHashSet;

use crate::{
    ast::{
        JSDeclarationKind, NodeFactory, NodeId, PrivateIdentifier, PropertyDeclaration, SourceFile,
    },
    diagnostics::{DiagnosticId, Message},
    flags::{ContainerFlags, NodeFlags, SymbolFlags},
    flow::{ActiveLabelId, FlowFactory, FlowLabel, FlowNodeId},
    symbol::{InternalSymbolName, SymbolId},
    syntax::SyntaxKind,
};

struct ExpandoAssignmentInfo {
    node: Option<NodeId>,
    container: Option<NodeId>,
    block_scope_container: Option<NodeId>,
}

pub struct Binder {
    file: NodeId,
    nodes: NodeFactory,

    flows: FlowFactory,
    unreachable_flow: Option<FlowNodeId>,

    container: Option<NodeId>,
    this_container: Option<NodeId>,
    block_scope_container: Option<NodeId>,
    last_container: Option<NodeId>,
    current_flow: Option<FlowNodeId>,
    current_break_target: Option<FlowLabel>,
    current_continue_target: Option<FlowLabel>,
    current_return_target: Option<FlowLabel>,
    current_true_target: Option<FlowLabel>,
    current_false_target: Option<FlowLabel>,
    current_exception_target: Option<FlowLabel>,
    pre_switch_case_flow: Option<FlowNodeId>,
    active_label_list: Option<ActiveLabelId>,
    emit_flags: NodeFlags,
    seen_this_keyword: bool,
    has_explicit_return: bool,
    has_flow_effects: bool,
    in_assignment_pattern: bool,
    seen_parse_error: bool,
    symbol_count: usize,
    not_const_enum_only_modules: FxHashSet<SymbolId>,
    expando_assignments: Vec<ExpandoAssignmentInfo>,
}

impl Binder {
    pub fn new(file: NodeId, nodes: NodeFactory) -> Self {
        Self {
            file,
            nodes,
            flows: FlowFactory::new(),
            unreachable_flow: None,
            container: None,
            this_container: None,
            block_scope_container: None,
            last_container: None,
            current_flow: None,
            current_break_target: None,
            current_continue_target: None,
            current_return_target: None,
            current_true_target: None,
            current_false_target: None,
            current_exception_target: None,
            pre_switch_case_flow: None,
            active_label_list: None,
            emit_flags: NodeFlags::empty(),
            seen_this_keyword: false,
            has_explicit_return: false,
            has_flow_effects: false,
            in_assignment_pattern: false,
            seen_parse_error: false,
            symbol_count: 0,
            not_const_enum_only_modules: FxHashSet::default(),
            expando_assignments: Vec::new(),
        }
    }

    pub fn bind(&mut self, node: NodeId) -> bool {
        // Even though in the AST the jsdoc @typedef node belongs to the current node,
        // its symbol might be in the same scope with the current node's symbol. Consider:
        //
        //     /** @typedef {string | number} MyType */
        //     function foo();
        //
        // Here the current node is "foo", which is a container, but the scope of "MyType" should
        // not be inside "foo". Therefore we always bind @typedef before bind the parent node,
        // and skip binding this tag later when binding all the other jsdoc tags.

        // First we bind declaration nodes to a symbol if possible. We'll both create a symbol
        // and then potentially add the symbol to an appropriate symbol table. Possible
        // destination symbol tables are:
        //
        //  1) The 'exports' table of the current container's symbol.
        //  2) The 'members' table of the current container's symbol.
        //  3) The 'locals' table of the current container.
        //
        // However, not all symbols will end up in any of these tables. 'Anonymous' symbols
        // (like TypeLiterals for example) will not be put in any table.
        match self.nodes[node].kind {
            SyntaxKind::Identifier => {
                self.nodes[node].flow_node = self.current_flow;
                self.check_contextual_identifier(node);
            }
            SyntaxKind::ThisKeyword => {
                self.seen_this_keyword = true;
                self.nodes[node].flow_node = self.current_flow;
            }
            SyntaxKind::SuperKeyword => {
                self.nodes[node].flow_node = self.current_flow;
            }
            SyntaxKind::QualifiedName => {
                if self.current_flow.is_some() && self.nodes.is_part_of_type_query(node) {
                    self.nodes[node].flow_node = self.current_flow;
                }
            }
            SyntaxKind::MetaProperty => {
                self.nodes[node].flow_node = self.current_flow;
            }
            SyntaxKind::PrivateIdentifier => self.check_private_identifier(node),
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression => {
                if self.current_flow.is_some() && self.nodes.is_narrowable_reference(node) {
                    self.nodes[node].flow_node = self.current_flow;
                }
            }
            SyntaxKind::BinaryExpression => {
                match self.nodes.get_assignment_declaration_kind(node) {
                    JSDeclarationKind::ModuleExports => self.bind_module_exports_assignment(node),
                    JSDeclarationKind::ExportsProperty => {
                        self.bind_exports_or_object_define_property(node)
                    }
                    JSDeclarationKind::Property => self.bind_expando_property_assignment(node),
                    JSDeclarationKind::ThisProperty => self.bind_this_property_assignment(node),
                    _ => {}
                }
                self.check_strict_mode_binary_expression(node);
            }
            SyntaxKind::CatchClause => self.check_strict_mode_catch_clause(node),
            SyntaxKind::DeleteExpression => self.check_strict_mode_delete_expression(node),
            SyntaxKind::PostfixUnaryExpression => {
                self.check_strict_mode_postfix_unary_expression(node)
            }
            SyntaxKind::PrefixUnaryExpression => {
                self.check_strict_mode_prefix_unary_expression(node)
            }
            SyntaxKind::WithStatement => self.check_strict_mode_with_statement(node),
            SyntaxKind::LabeledStatement => self.check_strict_mode_labeled_statement(node),
            SyntaxKind::ThisType => {
                self.seen_this_keyword = true;
            }
            SyntaxKind::TypeParameter => self.bind_type_parameter(node),
            SyntaxKind::Parameter => self.bind_parameter(node),
            SyntaxKind::VariableDeclaration => {
                self.bind_variable_declaration_or_binding_element(node)
            }
            SyntaxKind::BindingElement => {
                self.nodes[node].flow_node = self.current_flow;
                self.bind_variable_declaration_or_binding_element(node);
            }
            SyntaxKind::PropertyDeclaration | SyntaxKind::PropertySignature => {
                self.bind_property_worker(node)
            }
            SyntaxKind::PropertyAssignment | SyntaxKind::ShorthandPropertyAssignment => self
                .bind_property_or_method_or_accessor(
                    node,
                    SymbolFlags::Property,
                    SymbolFlags::PropertyExcludes,
                ),
            SyntaxKind::EnumMember => self.bind_property_or_method_or_accessor(
                node,
                SymbolFlags::EnumMember,
                SymbolFlags::EnumMemberExcludes,
            ),
            SyntaxKind::CallSignature
            | SyntaxKind::ConstructSignature
            | SyntaxKind::IndexSignature => {
                self.declare_symbol_and_add_to_symbol_table(
                    node,
                    SymbolFlags::Signature,
                    SymbolFlags::empty(),
                );
            }
            SyntaxKind::MethodDeclaration | SyntaxKind::MethodSignature => self
                .bind_property_or_method_or_accessor(
                    node,
                    SymbolFlags::Method | self.nodes.get_optional_symbol_flag_for_node(node),
                    if self.nodes.is_object_literal_method(node) {
                        SymbolFlags::Value
                    } else {
                        SymbolFlags::MethodExcludes
                    },
                ),
            SyntaxKind::FunctionDeclaration => self.bind_function_declaration(node),
            SyntaxKind::Constructor => {
                self.declare_symbol_and_add_to_symbol_table(
                    node,
                    SymbolFlags::Constructor,
                    SymbolFlags::empty(),
                );
            }
            SyntaxKind::GetAccessor => self.bind_property_or_method_or_accessor(
                node,
                SymbolFlags::GetAccessor,
                SymbolFlags::GetAccessorExcludes,
            ),
            SyntaxKind::SetAccessor => self.bind_property_or_method_or_accessor(
                node,
                SymbolFlags::SetAccessor,
                SymbolFlags::SetAccessorExcludes,
            ),
            SyntaxKind::FunctionType | SyntaxKind::ConstructorType => {
                self.bind_function_or_constructor_type(node)
            }
            SyntaxKind::TypeLiteral | SyntaxKind::MappedType => self.bind_anonymous_declaration(
                node,
                SymbolFlags::TypeLiteral,
                InternalSymbolName::TYPE,
            ),
            SyntaxKind::ObjectLiteralExpression => self.bind_anonymous_declaration(
                node,
                SymbolFlags::ObjectLiteral,
                InternalSymbolName::OBJECT,
            ),
            SyntaxKind::FunctionExpression | SyntaxKind::ArrowFunction => {
                self.bind_function_expression(node)
            }
            SyntaxKind::ClassExpression | SyntaxKind::ClassDeclaration => {
                self.bind_class_like_declaration(node)
            }
            SyntaxKind::InterfaceDeclaration => self.bind_block_scoped_declaration(
                node,
                SymbolFlags::Interface,
                SymbolFlags::InterfaceExcludes,
            ),
            SyntaxKind::CallExpression => {
                match self.nodes.get_assignment_declaration_kind(node) {
                    JSDeclarationKind::ObjectDefinePropertyValue => {
                        self.bind_expando_property_assignment(node)
                    }
                    JSDeclarationKind::ObjectDefinePropertyExports => {
                        self.bind_exports_or_object_define_property(node)
                    }
                    _ => {}
                }
                if self.nodes[node].is_in_js_file() {
                    self.bind_call_expression(node);
                }
            }
            SyntaxKind::TypeAliasDeclaration => self.bind_block_scoped_declaration(
                node,
                SymbolFlags::TypeAlias,
                SymbolFlags::TypeAliasExcludes,
            ),
            SyntaxKind::JSTypeAliasDeclaration =>
            // Top-level JSTypeAliasDeclaration nodes are processed in bindContainer
            {
                if self
                    .block_scope_container
                    .is_none_or(|x| !self.nodes.is(x, SyntaxKind::SourceFile))
                {
                    self.bind_block_scoped_declaration(
                        node,
                        SymbolFlags::TypeAlias,
                        SymbolFlags::TypeAliasExcludes,
                    );
                }
            }
            SyntaxKind::EnumDeclaration => self.bind_enum_declaration(node),
            SyntaxKind::ModuleDeclaration => self.bind_module_declaration(node),
            SyntaxKind::ImportEqualsDeclaration
            | SyntaxKind::NamespaceImport
            | SyntaxKind::ImportSpecifier
            | SyntaxKind::ExportSpecifier => {
                self.declare_symbol_and_add_to_symbol_table(
                    node,
                    SymbolFlags::Alias,
                    SymbolFlags::AliasExcludes,
                );
            }
            SyntaxKind::NamespaceExportDeclaration => self.bind_namespace_export_declaration(node),
            SyntaxKind::ImportClause => self.bind_import_clause(node),
            SyntaxKind::ExportDeclaration => self.bind_export_declaration(node),
            SyntaxKind::ExportAssignment => self.bind_export_assignment(node),
            SyntaxKind::SourceFile => self.bind_source_file_if_external_module(),
            SyntaxKind::JsxAttributes => self.bind_jsx_attributes(node),
            SyntaxKind::JsxAttribute => {
                self.bind_jsx_attribute(node, SymbolFlags::Property, SymbolFlags::PropertyExcludes)
            }
            _ => {}
        }

        // Then we recurse into the children of the node to bind them as well. For certain
        // symbols we do specialized work when we recurse. For example, we'll keep track of
        // the current 'container' node when it changes. This helps us know which symbol table
        // a local should go into for example. Since terminal nodes are known not to have
        // children, as an optimization we don't process those.
        let mut this_node_or_any_subnodes_has_error =
            self.nodes[node].flags.contains(NodeFlags::ThisNodeHasError);
        if self.nodes[node].kind > SyntaxKind::LAST_TOKEN {
            let save_seen_parse_error = self.seen_parse_error;
            self.seen_parse_error = false;
            let container_flags = self.nodes.get_container_flags(node);
            if container_flags.is_empty() {
                self.bind_children(node);
            } else {
                self.bind_container(node, container_flags);
            }
            if self.seen_parse_error {
                this_node_or_any_subnodes_has_error = true;
            }
            self.seen_parse_error = save_seen_parse_error;
        }
        if this_node_or_any_subnodes_has_error {
            self.nodes[node].flags.insert(NodeFlags::ThisNodeOrAnySubNodesHasError);
            self.seen_parse_error = true;
        }

        false
    }

    fn bind_module_exports_assignment(&mut self, node: NodeId) {}
    fn bind_exports_or_object_define_property(&mut self, node: NodeId) {}
    fn bind_expando_property_assignment(&mut self, node: NodeId) {}
    fn bind_this_property_assignment(&mut self, node: NodeId) {}
    fn check_strict_mode_binary_expression(&mut self, node: NodeId) {}
    fn check_strict_mode_catch_clause(&mut self, node: NodeId) {}
    fn check_strict_mode_delete_expression(&mut self, node: NodeId) {}
    fn check_strict_mode_postfix_unary_expression(&mut self, node: NodeId) {}
    fn check_strict_mode_prefix_unary_expression(&mut self, node: NodeId) {}
    fn check_strict_mode_with_statement(&mut self, node: NodeId) {}
    fn check_strict_mode_labeled_statement(&mut self, node: NodeId) {}

    fn check_contextual_identifier(&mut self, node: NodeId) {
        todo!()
    }

    fn check_private_identifier(&mut self, node: NodeId) {
        if self.nodes[node].data_ref::<PrivateIdentifier>().text == "#constructor" {
            // Report error only if there are no parse errors in file
            if self.nodes[self.file].data_ref::<SourceFile>().diagnostics.len() == 0 {
                self.error_on_node(
                    node,
                    Message::e18012_constructor_is_a_reserved_word(),
                    [self.nodes.declaration_name_to_string(node)],
                );
            }
        }
    }

    fn declare_symbol_and_add_to_symbol_table(
        &mut self,
        node: NodeId,
        symbol_flags: SymbolFlags,
        symbol_excludes: SymbolFlags,
    ) -> SymbolId {
        todo!()
    }

    fn error_on_node(
        &mut self,
        node: NodeId,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) -> DiagnosticId {
        todo!()
    }

    fn bind_type_parameter(&self, node: NodeId) {
        todo!()
    }

    fn bind_parameter(&self, node: NodeId) {
        todo!()
    }

    fn bind_variable_declaration_or_binding_element(&self, node: NodeId) {
        todo!()
    }

    fn bind_property_worker(&self, node: NodeId) {
        todo!()
    }

    fn bind_property_or_method_or_accessor(
        &self,
        node: NodeId,
        symbol_flags: SymbolFlags,
        symbol_excludes: SymbolFlags,
    ) {
        todo!()
    }

    fn bind_function_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_function_or_constructor_type(&self, node: NodeId) {
        todo!()
    }

    fn bind_anonymous_declaration(&self, node: NodeId, symbol_flags: SymbolFlags, name: &[u8]) {
        todo!()
    }

    fn bind_function_expression(&self, node: NodeId) {
        todo!()
    }

    fn bind_class_like_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_block_scoped_declaration(
        &self,
        node: NodeId,
        symbol_flags: SymbolFlags,
        symbol_excludes: SymbolFlags,
    ) {
        todo!()
    }

    fn bind_call_expression(&self, node: NodeId) {
        todo!()
    }

    fn bind_enum_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_module_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_namespace_export_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_import_clause(&self, node: NodeId) {
        todo!()
    }

    fn bind_export_declaration(&self, node: NodeId) {
        todo!()
    }

    fn bind_export_assignment(&self, node: NodeId) {
        todo!()
    }

    fn bind_source_file_if_external_module(&self) {
        todo!()
    }

    fn bind_jsx_attributes(&self, node: NodeId) {
        todo!()
    }

    fn bind_jsx_attribute(
        &self,
        node: NodeId,
        symbol_flags: SymbolFlags,
        symbol_excludes: SymbolFlags,
    ) {
        todo!()
    }

    fn bind_children(&self, node: NodeId) {
        todo!()
    }

    fn bind_container(&self, node: NodeId, container_flags: ContainerFlags) {
        todo!()
    }
}
