use crate::{
    options::ScriptTarget,
    syntax::{CommentDirective, SyntaxKind, TokenFlags},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LanguageVariant {
    #[default]
    Standard,
    JSX,
}

pub type ErrorCallback = fn(s: String);

#[derive(Debug, Default)]
pub struct ScannerState {
    /// Current position in text (and ending position of current token)
    pos: usize,
    /// Starting position of current token including preceding whitespace
    full_start_pos: usize,
    /// Starting position of non-whitespace part of current token
    token_start: usize,
    /// Kind of current token
    token: SyntaxKind,
    /// Parsed value of current token
    token_value: String,
    /// Flags for current token
    token_flags: TokenFlags,
    comment_directives: Vec<CommentDirective>,
    /// Leading asterisks to skip when scanning types inside JSDoc. Should be 0 outside JSDoc
    skip_jsdoc_leading_asterisks: u32,
}

#[derive(Debug, Default)]
pub struct Scanner {
    text: String,
    end: usize,
    language_variant: LanguageVariant,
    script_target: ScriptTarget,
    on_error: Option<ErrorCallback>,
    skip_trivia: bool,
    state: ScannerState,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            skip_trivia: true,
            ..Self::default()
        }
    }
}