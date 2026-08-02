#[derive(Default, Debug)]
pub struct AstSrc {
    pub tokens: Vec<String>,
    pub nodes: Vec<AstNodeSrc>,
    pub enums: Vec<AstEnumSrc>,
}

#[derive(Debug)]
pub(crate) struct AstNodeSrc {
    pub(crate) doc: Vec<String>,
    pub(crate) name: String,
    pub(crate) traits: Vec<String>,
    pub(crate) fields: Vec<Field>,
}

impl AstNodeSrc {
    /// Removes the fields at the given indices (highest indices first so
    /// earlier indices stay valid while removing).
    pub(crate) fn remove_field(&mut self, mut indices: Vec<usize>) {
        indices.sort_unstable();
        indices.into_iter().rev().for_each(|idx| {
            self.fields.remove(idx);
        });
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Field {
    Token {
        name: Option<String>,
        token: String,
    },
    Node {
        name: String,
        ty: String,
        cardinality: Cardinality,
    },
}

impl Field {
    /// The type name this field refers to: the cleaned-up token name for
    /// `Token` fields, or the node/enum type name for `Node` fields.
    pub(crate) fn ty(&self) -> &str {
        match self {
            Field::Token { token, .. } => token,
            Field::Node { ty, .. } => ty,
        }
    }

    /// The name the generated accessor method will use.
    pub(crate) fn method_name(&self) -> String {
        match self {
            Field::Token { name, token } => name.clone().unwrap_or_else(|| token.clone()),
            Field::Node { name, .. } => name.clone(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Cardinality {
    Optional,
    Many,
}

#[derive(Debug)]
pub(crate) struct AstEnumSrc {
    pub(crate) doc: Vec<String>,
    pub(crate) name: String,
    pub(crate) traits: Vec<String>,
    pub(crate) variants: Vec<String>,
}
