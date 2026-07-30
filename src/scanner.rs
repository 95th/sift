use std::ops::{Deref, DerefMut};

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
                b'"' | b'\'' => {
                    self.state.token_value = self.scan_string(false);
                    self.state.token = SyntaxKind::StringLiteral;
                }
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
                    c if c.is_ascii_digit() => todo!("scan number"),
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::DotToken
                    }
                },
                b'/' => match self.ascii_at(1) {
                    b'/' => {
                        // Single-line comment
                        self.state.pos += 2;
                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'\r' | b'\n'));
                            let c = self.char();
                            if c == '\0' || is_line_break(c) {
                                break;
                            }
                            self.state.pos += c.len_utf8();
                        }

                        self.process_comment_directive(
                            self.state.token_start,
                            self.state.pos,
                            false,
                        );
                        if self.skip_trivia {
                            continue;
                        }
                        self.state.token = SyntaxKind::SingleLineCommentTrivia;
                        return self.state.token;
                    }
                    b'*' => {
                        self.state.pos += 2;
                        let is_jsdoc = self.ascii() == b'*' && self.ascii_at(1) != b'/';

                        let mut comment_closed = false;
                        let mut last_line_start = self.state.token_start;

                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'*' | b'\n' | b'\r'));
                            let c = self.char();
                            if c == '\0' {
                                break;
                            }

                            if c == '*' && self.ascii_at(1) == b'/' {
                                self.state.pos += 2;
                                comment_closed = true;
                                break;
                            }

                            self.state.pos += c.len_utf8();
                            if is_line_break(c) {
                                last_line_start = self.state.pos;
                                self.state
                                    .token_flags
                                    .insert(TokenFlags::PrecedingLineBreak);
                            }
                        }

                        if is_jsdoc {
                            self.state
                                .token_flags
                                .insert(TokenFlags::PrecedingJSDocComment);
                            todo!("scan jsdoc for tags");
                        }

                        self.process_comment_directive(last_line_start, self.state.pos, true);

                        if !comment_closed {
                            todo!("Error */ expected");
                        }
                        if self.skip_trivia {
                            continue;
                        }
                        if !comment_closed {
                            self.state.token_flags.insert(TokenFlags::Unterminated);
                        }
                        self.state.token = SyntaxKind::MultiLineCommentTrivia;
                        return self.state.token;
                    }
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::SlashEqualsToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::SlashToken;
                    }
                },
                c if c.is_ascii_digit() => {
                    todo!("number")
                }
                b':' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::ColonToken;
                }
                b';' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::SemicolonToken;
                }
                b'<' => match self.ascii_at(1) {
                    b'<' if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    b'<' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::LessThanLessThanEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::LessThanLessThanToken;
                        }
                    },
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::LessThanEqualsToken;
                    }
                    b'/' if self.language_variant == LanguageVariant::JSX
                        && self.ascii_at(2) != b'*' =>
                    {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::LessThanSlashToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::LessThanToken;
                    }
                },
                b'=' => match self.ascii_at(1) {
                    b'=' if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    b'=' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::EqualsEqualsEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::EqualsEqualsToken;
                        }
                    },
                    b'>' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::EqualsGreaterThanToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::EqualsToken;
                    }
                },
                b'>' => match self.ascii_at(1) {
                    b'>' if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::GreaterThanToken;
                    }
                },
                b'?' => match self.ascii_at(1) {
                    b'.' if !self.ascii_at(2).is_ascii_digit() => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::QuestionDotToken;
                    }
                    b'?' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::QuestionQuestionEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::QuestionQuestionToken;
                        }
                    },
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::QuestionToken;
                    }
                },
                b'[' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::OpenBracketToken;
                }
                b']' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::CloseBracketToken;
                }
                b'^' => match self.ascii_at(1) {
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::CaretEqualsToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::CaretToken;
                    }
                },
                b'{' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::OpenBraceToken;
                }
                b'|' => match self.ascii_at(1) {
                    b'|' if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    b'|' => match self.ascii_at(2) {
                        b'=' => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::BarBarEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::BarBarToken;
                        }
                    },
                    b'=' => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::BarEqualsToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::BarToken;
                    }
                },
                b'}' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::CloseBraceToken;
                }
                b'~' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::TildeToken;
                }
                b'@' => {
                    self.state.pos += 1;
                    self.state.token = SyntaxKind::AtToken;
                }
                b'\\' => {
                    todo!("Escape")
                }
                b'#' => match self.ascii_at(1) {
                    b'!' => todo!("shebang parsing"),
                    b'\\' => todo!("private identifier with escape"),
                    _ => todo!("private identifier"),
                },
                _ => {
                    if c == 0 {
                        self.state.token = SyntaxKind::EndOfFile;
                        break;
                    }
                    todo!("Scan identifier etc")
                }
            }
        }
        todo!()
    }

    fn scan_string(&mut self, jsx_attr_string: bool) -> String {
        let quote = self.ascii();
        if quote == b'\\' {
            self.state.token_flags.insert(TokenFlags::SingleQuote);
        }
        self.state.pos += 1;
        // Fast path for simple strings without escape sequences.
        match self.text.as_bytes()[self.state.pos..]
            .iter()
            .position(|c| *c == quote)
        {
            Some(0) => {
                self.state.pos += 1;
                return String::new();
            }
            Some(n) => {
                let s = &self.text[self.state.pos..self.state.pos + n];
                if jsx_attr_string || !s.contains(|c| matches!(c, '\\' | '\r' | '\n')) {
                    self.state.pos += n + 1;
                    return String::from(s);
                }
            }
            None => {}
        }

        todo!()
    }

    fn process_comment_directive(&mut self, start: usize, end: usize, multiline: bool) {
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

fn is_conflict_marker_trivia(text: &str, pos: usize) -> bool {
    todo!()
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

fn is_line_break(c: char) -> bool {
    // ES5 7.3:
    // The ECMAScript line terminator characters are listed in Table 3.
    //     Table 3: Line Terminator Characters
    //     Code Unit Value     Name                    Formal Name
    //     \u000A              Line Feed               <LF>
    //     \u000D              Carriage Return         <CR>
    //     \u2028              Line separator          <LS>
    //     \u2029              Paragraph separator     <PS>
    // Only the characters in Table 3 are treated as line terminators. Other new line or line
    // breaking characters are treated as white space but not as line terminators.
    matches!(
        c,
        | '\n'       // lineFeed
        | '\r'       // carriageReturn
        | '\u{2028}' // lineSeparator
        | '\u{2029}' // paragraphSeparator
    )
}
