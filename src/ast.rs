use std::marker::PhantomData;

use crate::{
    flags::NodeFlags,
    syntax::{SyntaxKind, SyntaxNode, SyntaxNodeChildren, SyntaxToken, TextRange},
};

#[derive(Debug, Default)]
pub struct Node {
    pub loc: TextRange,
    pub flags: NodeFlags,
}

impl Node {
    pub fn is_js_type_alias_declaration(&self) -> bool {
        todo!()
    }

    pub fn is_js_import_declaration(&self) -> bool {
        todo!()
    }
}

pub struct JSDocInfo {}

/// The main trait to go from untyped `SyntaxNode`  to a typed ast. The
/// conversion itself has zero runtime cost: ast and syntax nodes have exactly
/// the same representation: a pointer to the tree root and a pointer to the
/// node itself.
pub trait AstNode {
    /// This panics if the `SyntaxKind` is not statically known.
    fn kind() -> SyntaxKind
    where
        Self: Sized,
    {
        panic!("dynamic `SyntaxKind` for `AstNode::kind()`")
    }

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxNode;

    fn clone_subtree(&self) -> Self
    where
        Self: Sized,
    {
        Self::cast(self.syntax().clone_subtree()).unwrap()
    }
}

/// Like `AstNode`, but wraps tokens rather than interior nodes.
pub trait AstToken {
    fn can_cast(token: SyntaxKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxToken;

    fn text(&self) -> &str {
        self.syntax().text()
    }
}

/// An iterator over `SyntaxNode` children of a particular AST type.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: SyntaxNodeChildren,
    ph: PhantomData<N>,
}

impl<N> AstChildren<N> {
    fn new(parent: &SyntaxNode) -> Self {
        AstChildren {
            inner: parent.children(),
            ph: PhantomData,
        }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;
    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

mod support {
    use super::{AstChildren, AstNode, SyntaxKind, SyntaxNode, SyntaxToken};

    #[inline]
    pub(super) fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
        parent.children().find_map(N::cast)
    }

    #[inline]
    pub(super) fn children<N: AstNode>(parent: &SyntaxNode) -> AstChildren<N> {
        AstChildren::new(parent)
    }

    #[inline]
    pub(super) fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == kind)
    }
}

include!(concat!(env!("OUT_DIR"), "/generated_ast.rs"));

// Hand-written accessors for fields excluded from codegen (see the
// `manually_implemented` labels in astgen/src/grammar.rs and the comment
// atop ts.ungram): nodes with two children of the same AST type can't be
// disambiguated by `support::child`, and an operator token drawn from an
// inline alternation isn't a single `SyntaxKind` to look up by.
impl BinaryExpression {
    pub fn lhs(&self) -> Option<Expression> {
        support::children(&self.syntax).next()
    }

    pub fn rhs(&self) -> Option<Expression> {
        support::children(&self.syntax).nth(1)
    }

    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::PlusToken
                        | SyntaxKind::MinusToken
                        | SyntaxKind::AsteriskToken
                        | SyntaxKind::SlashToken
                        | SyntaxKind::PercentToken
                        | SyntaxKind::LessThanToken
                        | SyntaxKind::GreaterThanToken
                        | SyntaxKind::LessThanEqualsToken
                        | SyntaxKind::GreaterThanEqualsToken
                        | SyntaxKind::EqualsEqualsToken
                        | SyntaxKind::EqualsEqualsEqualsToken
                        | SyntaxKind::ExclamationEqualsToken
                        | SyntaxKind::ExclamationEqualsEqualsToken
                        | SyntaxKind::AmpersandAmpersandToken
                        | SyntaxKind::BarBarToken
                        | SyntaxKind::InstanceOfKeyword
                        | SyntaxKind::InKeyword
                )
            })
    }
}

impl PrefixUnaryExpression {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::PlusToken
                        | SyntaxKind::MinusToken
                        | SyntaxKind::ExclamationToken
                        | SyntaxKind::TildeToken
                        | SyntaxKind::TypeOfKeyword
                        | SyntaxKind::VoidKeyword
                        | SyntaxKind::DeleteKeyword
                )
            })
    }
}

impl IfStatement {
    pub fn then_branch(&self) -> Option<Statement> {
        support::children(&self.syntax).next()
    }

    pub fn else_branch(&self) -> Option<Statement> {
        support::children(&self.syntax).nth(1)
    }
}

#[cfg(test)]
mod tests {
    use rowan::Language;

    use super::*;
    use crate::syntax::TsLang;

    /// Hand-builds the tree for `if (a) return 1; else return b + 2;` to
    /// prove the generated + hand-written accessors actually walk a real
    /// tree correctly, not just that they compile.
    fn build_if_else() -> SyntaxNode {
        let mut b = rowan::GreenNodeBuilder::new();
        let k = |kind: SyntaxKind| TsLang::kind_to_raw(kind);

        b.start_node(k(SyntaxKind::SourceFile));
        b.start_node(k(SyntaxKind::IfStatement));
        b.token(k(SyntaxKind::IfKeyword), "if");
        b.token(k(SyntaxKind::OpenParenToken), "(");
        b.start_node(k(SyntaxKind::Identifier));
        b.token(k(SyntaxKind::Identifier), "a");
        b.finish_node();
        b.token(k(SyntaxKind::CloseParenToken), ")");

        b.start_node(k(SyntaxKind::ReturnStatement));
        b.token(k(SyntaxKind::ReturnKeyword), "return");
        b.start_node(k(SyntaxKind::NumericLiteral));
        b.token(k(SyntaxKind::NumericLiteral), "1");
        b.finish_node();
        b.token(k(SyntaxKind::SemicolonToken), ";");
        b.finish_node(); // ReturnStatement (then)

        b.token(k(SyntaxKind::ElseKeyword), "else");

        b.start_node(k(SyntaxKind::ReturnStatement));
        b.token(k(SyntaxKind::ReturnKeyword), "return");
        b.start_node(k(SyntaxKind::BinaryExpression));
        b.start_node(k(SyntaxKind::Identifier));
        b.token(k(SyntaxKind::Identifier), "b");
        b.finish_node();
        b.token(k(SyntaxKind::PlusToken), "+");
        b.start_node(k(SyntaxKind::NumericLiteral));
        b.token(k(SyntaxKind::NumericLiteral), "2");
        b.finish_node();
        b.finish_node(); // BinaryExpression
        b.token(k(SyntaxKind::SemicolonToken), ";");
        b.finish_node(); // ReturnStatement (else)

        b.finish_node(); // IfStatement
        b.finish_node(); // SourceFile

        SyntaxNode::new_root(b.finish())
    }

    #[test]
    fn walks_generated_and_hand_written_accessors() {
        let root = build_if_else();
        let file = SourceFile::cast(root).expect("root should cast to SourceFile");

        let Some(Statement::IfStatement(if_stmt)) = file.statements().next() else {
            panic!("expected a single IfStatement");
        };

        assert!(matches!(
            if_stmt.expression(),
            Some(Expression::Identifier(_))
        ));

        let Some(Statement::ReturnStatement(then_ret)) = if_stmt.then_branch() else {
            panic!("expected then_branch to be a ReturnStatement");
        };
        assert!(matches!(
            then_ret.expression(),
            Some(Expression::Literal(Literal::NumericLiteral(_)))
        ));

        let Some(Statement::ReturnStatement(else_ret)) = if_stmt.else_branch() else {
            panic!("expected else_branch to be a ReturnStatement");
        };
        let Some(Expression::BinaryExpression(bin)) = else_ret.expression() else {
            panic!("expected else branch to return a BinaryExpression");
        };

        assert!(matches!(bin.lhs(), Some(Expression::Identifier(_))));
        assert!(matches!(
            bin.rhs(),
            Some(Expression::Literal(Literal::NumericLiteral(_)))
        ));
        assert_eq!(bin.op_token().unwrap().kind(), SyntaxKind::PlusToken);
    }
}
