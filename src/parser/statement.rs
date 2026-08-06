use crate::{
    ast::{
        Block, ModifierList, NodeId, NodeList, VariableDeclaration, VariableDeclarationList,
        VariableStatement,
    },
    diagnostics::Message,
    flags::{JSDocScannerInfo, NodeFlags, ParsingContext},
    parser::Parser,
    syntax::SyntaxKind,
};

impl Parser {
    pub fn parse_statement(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::OpenBraceToken => self.parse_block(false, None),
            SyntaxKind::VarKeyword => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            _ => todo!(),
        }
    }

    fn parse_empty_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::SemicolonToken);
        let node = self.nodes.create(SyntaxKind::EmptyStatement);
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_block(
        &mut self,
        ignore_missing_open_brace: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let open_brace_position = self.scanner.token_start();
        let open_brace_parsed = self.parse_expected_with_diagnostic(
            SyntaxKind::OpenBraceToken,
            diagnostic_message,
            true,
        );
        if open_brace_parsed || ignore_missing_open_brace {
            let multiline = self.has_preceding_line_break();
            let statements =
                self.parse_list(ParsingContext::BlockStatements, Self::parse_statement);
            self.parse_expected_matching_brackets(
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
                open_brace_parsed,
                open_brace_position,
            );
            let node = self.nodes.create(SyntaxKind::Block);
            self.nodes[node].set_data(Block {
                statements,
                multiline,
            });
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            if self.token == SyntaxKind::EqualsToken {
                self.parse_error_at_current_token(Message::e2809_declaration_or_statement_expected_this_follows_a_block_of_statements_so_if_you_intended_to_write_a_destructuring_assignment_you_might_need_to_wrap_the_whole_assignment_in_parentheses(), []);
                self.next_token();
            }
            return node;
        }

        let node = self.nodes.create(SyntaxKind::Block);
        self.nodes[node].set_data(Block {
            statements: NodeList::missing(),
            multiline: false,
        });
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_variable_statement(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let declaration_list = self.parse_variable_declaration_list(false);
        self.parse_semicolon();
        let node = self.nodes.create(SyntaxKind::VariableStatement);
        self.nodes[node].set_data(VariableStatement {
            modifiers,
            declaration_list,
        });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_semicolon(&mut self) -> bool {
        self.try_parse_semicolon() || self.parse_expected(SyntaxKind::SemicolonToken)
    }

    fn try_parse_semicolon(&mut self) -> bool {
        if !self.can_parse_semicolon() {
            return false;
        }
        if self.token == SyntaxKind::SemicolonToken {
            // consume the semicolon if it was explicitly provided.
            self.next_token();
        }
        true
    }

    fn parse_variable_declaration_list(&mut self, in_for_statement_initializer: bool) -> NodeId {
        let pos = self.node_pos();
        let flags = match self.token {
            SyntaxKind::VarKeyword => NodeFlags::empty(),
            SyntaxKind::LetKeyword => NodeFlags::Let,
            SyntaxKind::ConstKeyword => NodeFlags::Const,
            SyntaxKind::UsingKeyword => NodeFlags::Using,
            SyntaxKind::AwaitKeyword => {
                if !self.is_await_using_declaration() {
                    NodeFlags::empty()
                } else {
                    self.next_token();
                    NodeFlags::AwaitUsing
                }
            }
            _ => unreachable!("Unhandled case in parse_variable_declaration_list"),
        };
        self.next_token();
        // The user may have written the following:
        //
        //    for (let of X) { }
        //
        // In this case, we want to parse an empty declaration list, and then parse 'of'
        // as a keyword. The reason this is not automatic is that 'of' is a valid identifier.
        // So we need to look ahead to determine if 'of' should be treated as a keyword in
        // this context.
        // The checker will then give an error that there is an empty declaration list.
        let declarations = if self.token == SyntaxKind::OfKeyword
            && self.next_token_and(Self::is_identifier_and_close_paren)
        {
            NodeList::missing()
        } else {
            let save_context_flags = self.context_flags;
            self.set_context_flags(NodeFlags::DisallowInContext, in_for_statement_initializer);
            let declarations = self
                .parse_delimited_list(ParsingContext::VariableDeclarations, |p| {
                    Some(if in_for_statement_initializer {
                        p.parse_variable_declaration()
                    } else {
                        p.parse_variable_declaration_allow_exclamation()
                    })
                })
                .unwrap();
            self.context_flags = save_context_flags;
            declarations
        };

        let node = self.nodes.create(SyntaxKind::VariableDeclarationList);
        self.nodes[node].set_data(VariableDeclarationList {
            declarations,
            flags,
        });
        self.finish_node(node, pos);

        todo!()
    }

    fn is_identifier_and_close_paren(&mut self) -> bool {
        self.is_identifier() && self.next_token() == SyntaxKind::CloseParenToken
    }

    fn parse_variable_declaration(&mut self) -> NodeId {
        self.parse_variable_declaration_worker(false)
    }

    fn parse_variable_declaration_allow_exclamation(&mut self) -> NodeId {
        self.parse_variable_declaration_worker(true)
    }

    fn parse_variable_declaration_worker(&mut self, allow_exclamation: bool) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let name = self.parse_identifier_or_pattern_with_diagnostic(
            Message::e18029_private_identifiers_are_not_allowed_in_variable_declarations(),
        );
        let exclamation_token = if allow_exclamation
            && self.nodes[name].kind == SyntaxKind::Identifier
            && self.token == SyntaxKind::ExclamationToken
            && !self.has_preceding_line_break()
        {
            Some(self.parse_token_node())
        } else {
            None
        };
        let type_annotation = self.parse_type_annotation();
        let initializer = if !matches!(self.token, SyntaxKind::InKeyword | SyntaxKind::OfKeyword) {
            Some(self.parse_initializer())
        } else {
            None
        };
        let node = self.nodes.create(SyntaxKind::VariableDeclaration);
        self.nodes[node].set_data(VariableDeclaration {
            name,
            exclamation_token,
            type_annotation,
            initializer,
        });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn check_js_syntax(&self, node: NodeId) {
        let node = &self.nodes[node];
        if !node.flags.contains(NodeFlags::JavaScriptFile)
            || node
                .flags
                .intersects(NodeFlags::JSDoc | NodeFlags::Reparsed)
        {
            return;
        }

        todo!()
    }

    fn parse_identifier_or_pattern_with_diagnostic(
        &self,
        diagnostic_message: &'static Message,
    ) -> NodeId {
        todo!()
    }

    fn parse_type_annotation(&self) -> Option<NodeId> {
        todo!()
    }

    fn parse_initializer(&self) -> NodeId {
        todo!()
    }
}
