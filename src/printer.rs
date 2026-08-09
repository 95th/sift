use std::fmt;

use crate::{ast::*, syntax::SyntaxKind};

pub struct NodePrinter<'a> {
    id: NodeId,
    factory: &'a NodeFactory,
}

impl NodeFactory {
    pub fn print(&self, id: NodeId) -> NodePrinter<'_> {
        NodePrinter { id, factory: self }
    }
}

impl fmt::Display for NodePrinter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let NodePrinter { id, factory } = *self;
        let node = &factory[id];
        match node.kind {
            SyntaxKind::SourceFile => {
                for stmt in node.data_ref::<SourceFile>().statements.nodes.iter() {
                    write!(f, "{}", factory.print(*stmt))?;
                }
            }
            SyntaxKind::ExpressionStatement => {
                let stmt = node.data_ref::<ExpressionStatement>();
                write!(f, "{};", factory.print(stmt.expression))?;
            }
            SyntaxKind::BinaryExpression => {
                let expr = node.data_ref::<BinaryExpression>();
                write!(
                    f,
                    "{} {} {}",
                    factory.print(expr.left),
                    factory.print(expr.operator_token),
                    factory.print(expr.right)
                )?;
            }
            SyntaxKind::ParenthesizedExpression => {
                let expr = node.data_ref::<ParenthesizedExpression>();
                write!(f, "({})", factory.print(expr.expression))?;
            }
            SyntaxKind::NumericLiteral => {
                let expr = node.data_ref::<NumericLiteral>();
                write!(f, "{}", expr.text)?;
            }
            SyntaxKind::StringLiteral => {
                let expr = node.data_ref::<StringLiteral>();
                write!(f, "{:?}", expr.text)?;
            }
            SyntaxKind::PlusToken => write!(f, "+")?,
            kind => write!(f, "{kind:?}")?,
        }

        Ok(())
    }
}
