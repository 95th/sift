use std::rc::Rc;

use crate::{
    ast::{BlockData, NodeId, NodeList},
    diagnostics::Message,
    flags::ParsingContext,
    parser::Parser,
    syntax::{SyntaxKind, TextRange},
};

impl Parser {
    pub fn parse_statement(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::OpenBraceToken => self.parse_block(false, None),
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
            self.nodes[node].data = Some(Rc::new(BlockData {
                statements,
                multiline,
            }));
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            if self.token == SyntaxKind::EqualsToken {
                self.parse_error_at_current_token(Message::e2809_declaration_or_statement_expected_this_follows_a_block_of_statements_so_if_you_intended_to_write_a_destructuring_assignment_you_might_need_to_wrap_the_whole_assignment_in_parentheses(), []);
                self.next_token();
            }
            return node;
        }

        let node = self.nodes.create(SyntaxKind::Block);
        self.nodes[node].data = Some(Rc::new(BlockData {
            statements: NodeList::missing(),
            multiline: false,
        }));
        self.with_jsdoc(node, jsdoc);
        node
    }
}
