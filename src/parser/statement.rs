use crate::{ast::Node, parser::Parser, syntax::SyntaxKind};

impl Parser {
    pub fn parse_statement(&mut self) -> Node {
        match self.token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            _ => todo!(),
        }
    }

    fn parse_empty_statement(&mut self) -> Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::SemicolonToken);
        let mut node = Node::default();
        self.finish_node(&mut node, pos);
        self.with_jsdoc(&mut node, jsdoc);
        node
    }
}
