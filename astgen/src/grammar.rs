use std::{collections::BTreeSet, fs, path::PathBuf};

use either::Either;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use ungrammar::{Grammar, Rule};

use crate::grammar::ast_src::{AstEnumSrc, AstNodeSrc, AstSrc, Cardinality, Field};

mod ast_src;

fn project_root() -> PathBuf {
    // `env!` is a compile-time constant fixed to *this* crate's manifest
    // directory, unlike the runtime `CARGO_MANIFEST_DIR` env var, which
    // reflects whichever crate's build script is currently executing (e.g.
    // `sift`'s, when this is called from `sift`'s build.rs) and would
    // resolve to the wrong directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

/// Parses `ts.ungram` and emits the typed AST layer (node/enum wrapper
/// structs implementing `AstNode`, plus token wrapper structs implementing
/// `AstToken`) as a string of Rust source, meant to be `include!`d.
pub fn generate() -> String {
    let grammar_path = project_root().join("astgen/ts.ungram");
    let grammar: Grammar = fs::read_to_string(grammar_path).unwrap().parse().unwrap();
    let ast = lower(&grammar);

    let tokens = generate_tokens(&ast);
    let nodes = generate_nodes(&ast);

    quote! {
        #tokens
        #nodes
    }
    .to_string()
}

fn lower(grammar: &Grammar) -> AstSrc {
    // Raw leaf-token wrapper types, named to match `SyntaxKind` variants
    // exactly (no case conversion needed for these). Kinds that also
    // appear as grammar nodes (e.g. `Identifier`, `StringLiteral`) belong
    // in ts.ungram instead, not here, to avoid defining the type twice.
    let mut res = AstSrc {
        tokens: "WhitespaceTrivia SingleLineCommentTrivia MultiLineCommentTrivia"
            .split_ascii_whitespace()
            .map(|it| it.to_owned())
            .collect::<Vec<_>>(),
        ..Default::default()
    };
    let nodes = grammar.iter().collect::<Vec<_>>();
    for &node in &nodes {
        let name = grammar[node].name.clone();
        let rule = &grammar[node].rule;
        match lower_enum(grammar, rule) {
            Some(variants) => {
                let enum_src = AstEnumSrc {
                    doc: Vec::new(),
                    name,
                    traits: Vec::new(),
                    variants,
                };
                res.enums.push(enum_src);
            }
            None => {
                let mut fields = Vec::new();
                lower_rule(&mut fields, grammar, None, rule);
                res.nodes.push(AstNodeSrc {
                    doc: Vec::new(),
                    name,
                    traits: Vec::new(),
                    fields,
                });
            }
        }
    }

    deduplicate_fields(&mut res);
    extract_enums(&mut res);
    extract_struct_traits(&mut res);
    extract_enum_traits(&mut res);
    res.nodes.sort_by_key(|it| it.name.clone());
    res.enums.sort_by_key(|it| it.name.clone());
    res.tokens.sort();
    res.nodes.iter_mut().for_each(|it| {
        it.traits.sort();
        it.fields.sort_by_key(|it| match it {
            Field::Token { token, .. } => (true, token.clone()),
            Field::Node { name, .. } => (false, name.clone()),
        });
    });
    res.enums.iter_mut().for_each(|it| {
        it.traits.sort();
        it.variants.sort();
    });
    res
}

fn lower_enum(grammar: &Grammar, rule: &Rule) -> Option<Vec<String>> {
    let alternatives = match rule {
        Rule::Alt(it) => it,
        _ => return None,
    };
    let mut variants = Vec::new();
    for alternative in alternatives {
        match alternative {
            Rule::Node(it) => variants.push(grammar[*it].name.clone()),
            Rule::Token(it) if grammar[*it].name == ";" => (),
            _ => return None,
        }
    }
    Some(variants)
}

fn lower_rule(acc: &mut Vec<Field>, grammar: &Grammar, label: Option<&String>, rule: &Rule) {
    if lower_separated_list(acc, grammar, label, rule) {
        return;
    }

    match rule {
        Rule::Node(node) => {
            let ty = grammar[*node].name.clone();
            let name = label.cloned().unwrap_or_else(|| to_lower_snake_case(&ty));
            let field = Field::Node {
                name,
                ty,
                cardinality: Cardinality::Optional,
            };
            acc.push(field);
        }
        Rule::Token(token) => {
            let mut token = clean_token_name(&grammar[*token].name);
            if "[]{}()".contains(&token) {
                token = format!("'{token}'");
            }
            let field = Field::Token {
                name: label.cloned(),
                token,
            };
            acc.push(field);
        }
        Rule::Rep(inner) => {
            if let Rule::Node(node) = &**inner {
                let ty = grammar[*node].name.clone();
                let name = label
                    .cloned()
                    .unwrap_or_else(|| pluralize(&to_lower_snake_case(&ty)));
                let field = Field::Node {
                    name,
                    ty,
                    cardinality: Cardinality::Many,
                };
                acc.push(field);
                return;
            }
            panic!("unhandled rule: {rule:?}")
        }
        Rule::Labeled { label: l, rule } => {
            assert!(label.is_none());
            let manually_implemented = matches!(
                l.as_str(),
                "lhs"
                    | "rhs"
                    | "then_branch"
                    | "else_branch"
                    | "start"
                    | "end"
                    | "op"
                    | "index"
                    | "base"
                    | "value"
                    | "trait"
                    | "self_ty"
                    | "iterable"
                    | "condition"
                    | "args"
                    | "body"
            );
            if manually_implemented {
                return;
            }
            lower_rule(acc, grammar, Some(l), rule);
        }
        Rule::Seq(rules) | Rule::Alt(rules) => {
            for rule in rules {
                lower_rule(acc, grammar, label, rule)
            }
        }
        Rule::Opt(rule) => lower_rule(acc, grammar, label, rule),
    }
}

// (T (',' T)* ','?)
fn lower_separated_list(
    acc: &mut Vec<Field>,
    grammar: &Grammar,
    label: Option<&String>,
    rule: &Rule,
) -> bool {
    let rule = match rule {
        Rule::Seq(it) => it,
        _ => return false,
    };

    let (nt, repeat, trailing_sep) = match rule.as_slice() {
        [Rule::Node(node), Rule::Rep(repeat), Rule::Opt(trailing_sep)] => {
            (Either::Left(node), repeat, Some(trailing_sep))
        }
        [Rule::Node(node), Rule::Rep(repeat)] => (Either::Left(node), repeat, None),
        [
            Rule::Token(token),
            Rule::Rep(repeat),
            Rule::Opt(trailing_sep),
        ] => (Either::Right(token), repeat, Some(trailing_sep)),
        [Rule::Token(token), Rule::Rep(repeat)] => (Either::Right(token), repeat, None),
        _ => return false,
    };
    let repeat = match &**repeat {
        Rule::Seq(it) => it,
        _ => return false,
    };
    if !matches!(
        repeat.as_slice(),
        [comma, nt_]
            if trailing_sep.is_none_or(|it| comma == &**it) && match (nt, nt_) {
                (Either::Left(node), Rule::Node(nt_)) => node == nt_,
                (Either::Right(token), Rule::Token(nt_)) => token == nt_,
                _ => false,
            }
    ) {
        return false;
    }
    match nt {
        Either::Right(token) => {
            let token = clean_token_name(&grammar[*token].name);
            let field = Field::Token { token, name: None };
            acc.push(field);
        }
        Either::Left(node) => {
            let ty = grammar[*node].name.clone();
            let name = label
                .cloned()
                .unwrap_or_else(|| pluralize(&to_lower_snake_case(&ty)));
            let field = Field::Node {
                name,
                ty,
                cardinality: Cardinality::Many,
            };
            acc.push(field);
        }
    }
    true
}

fn deduplicate_fields(ast: &mut AstSrc) {
    for node in &mut ast.nodes {
        let mut i = 0;
        'outer: while i < node.fields.len() {
            for j in 0..i {
                let f1 = &node.fields[i];
                let f2 = &node.fields[j];
                if f1 == f2 {
                    node.fields.remove(i);
                    continue 'outer;
                }
            }
            i += 1;
        }
    }
}

fn extract_enums(ast: &mut AstSrc) {
    for node in &mut ast.nodes {
        for enm in &ast.enums {
            let mut to_remove = Vec::new();
            for (i, field) in node.fields.iter().enumerate() {
                let ty = field.ty().to_string();
                if enm.variants.iter().any(|it| it == &ty) {
                    to_remove.push(i);
                }
            }
            if to_remove.len() == enm.variants.len() {
                node.remove_field(to_remove);
                let ty = enm.name.clone();
                let name = to_lower_snake_case(&ty);
                node.fields.push(Field::Node {
                    name,
                    ty,
                    cardinality: Cardinality::Optional,
                });
            }
        }
    }
}

/// Fields whose presence on a node implies the node should implement a
/// shared trait (e.g. every node with a `name` field implements `HasName`).
/// This table starts small and is meant to grow as shared TS/JS node shapes
/// (modifiers, type arguments, ...) are ported.
const STRUCT_TRAITS: &[(&str, &[&str])] = &[("HasName", &["name"])];

fn extract_struct_traits(ast: &mut AstSrc) {
    for node in &mut ast.nodes {
        for &(trait_name, methods) in STRUCT_TRAITS {
            extract_struct_trait(node, trait_name, methods);
        }
    }
}

fn extract_struct_trait(node: &mut AstNodeSrc, trait_name: &str, methods: &[&str]) {
    let mut to_remove = Vec::new();
    for (i, field) in node.fields.iter().enumerate() {
        let method_name = field.method_name();
        if methods.iter().any(|&it| it == method_name) {
            to_remove.push(i);
        }
    }
    if to_remove.len() == methods.len() {
        node.remove_field(to_remove);
        node.traits.push(trait_name.to_owned());
    }
}

/// A variant-holding enum implements a trait only if every one of its
/// variants does, so this intersects the trait sets of an enum's variants.
fn extract_enum_traits(ast: &mut AstSrc) {
    for enm in &mut ast.enums {
        let nodes = &ast.nodes;
        let mut variant_traits = enm
            .variants
            .iter()
            .filter_map(|var| nodes.iter().find(|it| &it.name == var))
            .map(|node| node.traits.iter().cloned().collect::<BTreeSet<_>>());

        let Some(mut enum_traits) = variant_traits.next() else {
            continue;
        };
        for traits in variant_traits {
            enum_traits = enum_traits.intersection(&traits).cloned().collect();
        }
        enm.traits = enum_traits.into_iter().collect();
    }
}

/// Strips ungrammar's token-kind sigils (`@` literal tokens, `#` generic
/// tokens) so e.g. `'@string_literal'` becomes `string_literal`.
fn clean_token_name(name: &str) -> String {
    let cleaned = name.trim_start_matches(['@', '#']);
    if cleaned.is_empty() {
        name.to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn pluralize(s: &str) -> String {
    format!("{s}s")
}

/// Converts a `PascalCase` grammar node name into a `snake_case` accessor
/// name, e.g. `BinaryExpression` -> `binary_expression`.
fn to_lower_snake_case(s: &str) -> String {
    let mut buf = String::with_capacity(s.len());
    let mut prev_is_upper = false;
    for (i, c) in s.char_indices() {
        if c.is_ascii_uppercase() {
            if i != 0 && !prev_is_upper {
                buf.push('_');
            }
            buf.push(c.to_ascii_lowercase());
            prev_is_upper = true;
        } else {
            buf.push(c);
            prev_is_upper = false;
        }
    }
    buf
}

/// Punctuation and keyword terminals (written as bare `'...'` literals in
/// the grammar) resolved directly to their `SyntaxKind` variant name. This
/// mirrors `text_to_token`/`text_to_keyword` in `src/syntax.rs`, but lives
/// here too since `astgen` (a build-dependency) can't depend on the `sift`
/// binary crate it helps build. Extend as the grammar grows.
const PUNCT_AND_KEYWORDS: &[(&str, &str)] = &[
    ("(", "OpenParenToken"),
    (")", "CloseParenToken"),
    ("{", "OpenBraceToken"),
    ("}", "CloseBraceToken"),
    ("[", "OpenBracketToken"),
    ("]", "CloseBracketToken"),
    (";", "SemicolonToken"),
    (",", "CommaToken"),
    (".", "DotToken"),
    ("=", "EqualsToken"),
    ("+", "PlusToken"),
    ("-", "MinusToken"),
    ("*", "AsteriskToken"),
    ("/", "SlashToken"),
    ("%", "PercentToken"),
    ("<", "LessThanToken"),
    (">", "GreaterThanToken"),
    ("<=", "LessThanEqualsToken"),
    (">=", "GreaterThanEqualsToken"),
    ("==", "EqualsEqualsToken"),
    ("===", "EqualsEqualsEqualsToken"),
    ("!=", "ExclamationEqualsToken"),
    ("!==", "ExclamationEqualsEqualsToken"),
    ("&&", "AmpersandAmpersandToken"),
    ("||", "BarBarToken"),
    ("!", "ExclamationToken"),
    ("~", "TildeToken"),
    ("var", "VarKeyword"),
    ("let", "LetKeyword"),
    ("const", "ConstKeyword"),
    ("if", "IfKeyword"),
    ("else", "ElseKeyword"),
    ("return", "ReturnKeyword"),
    ("typeof", "TypeOfKeyword"),
    ("void", "VoidKeyword"),
    ("delete", "DeleteKeyword"),
    ("instanceof", "InstanceOfKeyword"),
    ("in", "InKeyword"),
];

/// Converts an ungrammar token name into the `SyntaxKind` variant it refers
/// to. Handles both punctuation/keyword literals (`'('`, `'if'`, looked up
/// in `PUNCT_AND_KEYWORDS`) and `snake_case` generic/literal tokens
/// (`string_literal` -> `StringLiteral`, via naive PascalCase conversion).
///
/// Add an entry to `OVERRIDES` when naive snake_case -> PascalCase
/// conversion doesn't round-trip a `SyntaxKind` name, e.g. because of an
/// internal camel-hump like `BigInt` or `Jsx`.
fn token_kind_name(token: &str) -> String {
    // `lower_rule` wraps single bracket/paren/brace characters in literal
    // quotes (see the `"[]{}()".contains(&token)` check above) to flag them
    // as punctuation; undo that before matching.
    let token = token.trim_matches('\'');

    if let Some((_, name)) = PUNCT_AND_KEYWORDS.iter().find(|(k, _)| *k == token) {
        return (*name).to_owned();
    }

    const OVERRIDES: &[(&str, &str)] = &[("bigint_literal", "BigIntLiteral")];
    if let Some((_, name)) = OVERRIDES.iter().find(|(k, _)| *k == token) {
        return (*name).to_owned();
    }
    token
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn generate_tokens(ast: &AstSrc) -> TokenStream {
    let tokens = ast.tokens.iter().map(|token| {
        let name = format_ident!("{}", token);
        let kind = format_ident!("{}", token);
        quote! {
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct #name {
                pub(crate) syntax: SyntaxToken,
            }

            impl std::fmt::Display for #name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::Display::fmt(&self.syntax, f)
                }
            }

            impl AstToken for #name {
                fn can_cast(kind: SyntaxKind) -> bool {
                    kind == SyntaxKind::#kind
                }

                fn cast(syntax: SyntaxToken) -> Option<Self> {
                    if Self::can_cast(syntax.kind()) {
                        Some(Self { syntax })
                    } else {
                        None
                    }
                }

                fn syntax(&self) -> &SyntaxToken {
                    &self.syntax
                }
            }
        }
    });
    quote! { #(#tokens)* }
}

fn generate_nodes(ast: &AstSrc) -> TokenStream {
    let node_defs = ast.nodes.iter().map(generate_node);
    let enum_defs = ast.enums.iter().map(generate_enum);
    quote! {
        #(#node_defs)*
        #(#enum_defs)*
    }
}

fn field_method_name(name: &str) -> proc_macro2::Ident {
    // `type` and `self` are reserved words and can't be used as an accessor
    // method name.
    match name {
        "type" => format_ident!("ty"),
        "self" => format_ident!("self_"),
        _ => format_ident!("{}", name),
    }
}

fn generate_node(node: &AstNodeSrc) -> TokenStream {
    let name = format_ident!("{}", node.name);
    let kind = format_ident!("{}", node.name);

    let methods = node.fields.iter().map(|field| match field {
        Field::Node {
            name,
            ty,
            cardinality,
        } => {
            let method_name = field_method_name(name);
            let ty = format_ident!("{}", ty);
            match cardinality {
                Cardinality::Optional => quote! {
                    pub fn #method_name(&self) -> Option<#ty> {
                        support::child(&self.syntax)
                    }
                },
                Cardinality::Many => quote! {
                    pub fn #method_name(&self) -> AstChildren<#ty> {
                        support::children(&self.syntax)
                    }
                },
            }
        }
        Field::Token { name, token } => {
            let kind_name = token_kind_name(token);
            let kind_ident = format_ident!("{}", kind_name);

            // Symbolic punctuation (`(`, `<=`, ...) can't itself form part
            // of an identifier, so name the accessor after the resolved
            // `SyntaxKind` (`open_paren_token`) instead of the raw text.
            let label = name
                .clone()
                .unwrap_or_else(|| token.trim_matches('\'').to_owned());
            let is_ident_safe = label
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let method_name = if is_ident_safe {
                format_ident!("{}_token", label)
            } else {
                format_ident!("{}", to_lower_snake_case(&kind_name))
            };

            quote! {
                pub fn #method_name(&self) -> Option<SyntaxToken> {
                    support::token(&self.syntax, SyntaxKind::#kind_ident)
                }
            }
        }
    });

    quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct #name {
            pub(crate) syntax: SyntaxNode,
        }

        impl AstNode for #name {
            fn kind() -> SyntaxKind {
                SyntaxKind::#kind
            }

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::#kind
            }

            fn cast(syntax: SyntaxNode) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self { syntax })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }

        impl #name {
            #(#methods)*
        }
    }
}

fn generate_enum(enm: &AstEnumSrc) -> TokenStream {
    let name = format_ident!("{}", enm.name);
    let variants: Vec<_> = enm
        .variants
        .iter()
        .map(|v| format_ident!("{}", v))
        .collect();

    quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum #name {
            #(#variants(#variants),)*
        }

        #(
            impl From<#variants> for #name {
                fn from(node: #variants) -> #name {
                    #name::#variants(node)
                }
            }
        )*

        impl AstNode for #name {
            // A variant may itself be an enum (e.g. `Expression`'s
            // `Literal` variant), so delegate to the variant's own
            // `AstNode` impl rather than assuming it's a plain
            // `syntax`-holding struct or that its name is a `SyntaxKind`.
            fn can_cast(kind: SyntaxKind) -> bool {
                #(if #variants::can_cast(kind) { return true; })*
                false
            }

            fn cast(syntax: SyntaxNode) -> Option<Self> {
                #(
                    if let Some(it) = #variants::cast(syntax.clone()) {
                        return Some(#name::#variants(it));
                    }
                )*
                None
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    #(#name::#variants(it) => it.syntax(),)*
                }
            }
        }
    }
}
