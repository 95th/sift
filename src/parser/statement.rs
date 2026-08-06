use crate::{ast::NodeId, parser::Parser, syntax::SyntaxKind};

impl Parser {
    pub fn parse_statement(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            _ => todo!(),
        }
    }

    fn parse_empty_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::SemicolonToken);
        let node = self.nodes.create();
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }
}
