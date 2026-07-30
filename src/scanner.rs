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

    pub fn scan(&mut self) -> SyntaxKind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::empty();

        loop {
            let c = self.ascii();
            self.state.token_start = self.state.pos;

            match c {
                b'\t' | 0x0B | 0x0C | b' ' => {
                    self.state.pos += 1;
                    if self.skip_trivia {
                        continue;
                    }
                    loop {
                        let c = self.char();
                        if !is_whitespace_single_line(c) {
                            break;
                        }
                        self.state.pos += c.len_utf8();
                    }
                    self.state.token = SyntaxKind::WhitespaceTrivia;
                }
                b'\n' | b'\r' => {
                    self.state
                        .token_flags
                        .insert(TokenFlags::PrecedingLineBreak);
                    if self.skip_trivia {
                        self.state.pos += 1;
                        self.scan_ascii_while(|c| matches!(c, b' ' | b'\t'..=b'\r'));
                        continue;
                    }
                    if c == b'\r' && self.ascii_at(1) == b'\n' {
                        self.state.pos += 2;
                    } else {
                        self.state.pos += 1;
                    }
                    self.state.token = SyntaxKind::NewLineTrivia;
                }
                b'!' => {
                    if self.ascii_at(1) == b'=' {
                        if self.ascii_at(2) == b'=' {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::ExclamationEqualsEqualsToken;
                        } else {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::ExclamationEqualsToken;
                        }
                    } else {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::ExclamationToken;
                    }
                }
                b'"' | b'\'' => todo!("String"),
                b'`' => todo!("scan template"),
                b'%' => {
                    if self.ascii_at(1) == b'=' {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::PercentEqualsToken;
                    } else {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::PercentToken;
                    }
                }
                b'&' => match self.ascii_at(1) {
                    b'&' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::AmpersandAmpersandEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::AmpersandAmpersandToken;
                        }
                    },
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::AmpersandEqualsToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::AmpersandToken;
                    }
                },
                b'(' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::OpenParenToken;
                }
                b')' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::CloseParenToken;
                }
                b'*' => match self.ascii_at(1) {
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::AsteriskEqualsToken;
                    }
                    b'*' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::AsteriskAsteriskEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::AsteriskAsteriskToken;
                        }
                    },
                    _ => {
                        todo!("JSDoc handling")
                    }
                },
                b'+' => match self.ascii_at(1) {
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::PlusEqualsToken;
                    }
                    b'+' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::PlusPlusToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::PlusToken;
                    }
                },
                b',' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::CommaToken;
                }
                b'-' => match self.ascii_at(1) {
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::MinusEqualsToken;
                    }
                    b'-' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::MinusMinusToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::MinusToken;
                    }
                },
                b'.' => match self.ascii_at(1) {
                    b'.' if self.ascii_at(2) == b'.' => {
                        self.state.pos += 3;
                        self.state.token = SyntaxKind::DotDotDotToken;
                    }
                    b'0'..=b'9' => todo!("scan number"),
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::DotToken
                    }
                },
                _ => (),
            }
        }
        todo!()
    }

    fn ascii(&self) -> u8 {
        self.text
            .as_bytes()
            .get(self.state.pos)
            .copied()
            .unwrap_or_default()
    }

    fn ascii_at(&self, offset: usize) -> u8 {
        self.text
            .as_bytes()
            .get(self.state.pos + offset)
            .copied()
            .unwrap_or_default()
    }

    fn char(&self) -> char {
        self.text
            .get(self.state.pos..)
            .and_then(|s| s.chars().next())
            .unwrap_or_default()
    }

    fn scan_ascii_while(&mut self, predicate: impl Fn(u8) -> bool) {
        for i in self.state.pos..self.end {
            let b = self.text.as_bytes()[i];
            if b.is_ascii() && predicate(b) {
                self.state.pos = i;
            } else {
                break;
            }
        }
    }

    fn language_version(&self) -> ScriptTarget {
        if self.script_target == ScriptTarget::None {
            ScriptTarget::LATEST
        } else {
            self.script_target
        }
    }
}

fn is_whitespace_single_line(c: char) -> bool {
    // Note: nextLine is in the Zs space, and should be considered to be a whitespace.
    // It is explicitly not a line-break as it isn't in the exact set specified by EcmaScript.
    matches!(
        c,
        | ' '        // space
        | '\t'       // tab
        | '\u{0B}'   // verticalTab
        | '\u{0C}'   // formFeed
        | '\u{0085}' // nextLine
        | '\u{00A0}' // nonBreakingSpace
        | '\u{1680}' // ogham
        | '\u{2000}' // enQuad
        | '\u{2001}' // emQuad
        | '\u{2002}' // enSpace
        | '\u{2003}' // emSpace
        | '\u{2004}' // threePerEmSpace
        | '\u{2005}' // fourPerEmSpace
        | '\u{2006}' // sixPerEmSpace
        | '\u{2007}' // figureSpace
        | '\u{2008}' // punctuationEmSpace
        | '\u{2009}' // thinSpace
        | '\u{200A}' // hairSpace
        | '\u{200B}' // zeroWidthSpace
        | '\u{202F}' // narrowNoBreakSpace
        | '\u{205F}' // mathematicalSpace
        | '\u{3000}' // ideographicSpace
        | '\u{FEFF}' // byteOrderMark
    )
}
