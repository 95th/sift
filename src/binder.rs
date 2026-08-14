use rustc_hash::FxHashSet;

use crate::{
    ast::{JSDeclarationKind, NodeFactory, NodeId, PrivateIdentifier, SourceFile},
    diagnostics::{DiagnosticId, Message},
    flags::NodeFlags,
    flow::{ActiveLabelId, FlowFactory, FlowLabel, FlowNodeId},
    symbol::SymbolId,
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
            _ => {}
        }
        todo!()
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

    fn error_on_node(
        &mut self,
        node: NodeId,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) -> DiagnosticId {
        todo!()
    }
}
