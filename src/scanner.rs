use icu_properties::{
    CodePointSetData,
    props::{IdContinue, IdStart},
};

use crate::{
    diagnostics,
    options::ScriptTarget,
    syntax::{
        CommentDirective, EscapeSequenceScanningFlags, SyntaxKind, TokenFlags, text_to_keyword,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LanguageVariant {
    #[default]
    Standard,
    JSX,
}

pub type ErrorCallback =
    fn(message: &'static diagnostics::Message, pos: usize, length: usize, args: &[String]);

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

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn token_value(&self) -> &str {
        &self.state.token_value
    }

    pub fn scan(&mut self) -> SyntaxKind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::empty();

        loop {
            self.state.token_start = self.state.pos;
            let Some(c) = self.ascii() else {
                self.state.token = SyntaxKind::EndOfFile;
                break;
            };

            match c {
                b'\t' | 0x0B | 0x0C | b' ' => {
                    self.state.pos += 1;
                    if self.skip_trivia {
                        continue;
                    }
                    while let Some(c) = self.char()
                        && is_whitespace_single_line(c)
                    {
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
                    if c == b'\r' && self.ascii_at(1) == Some(b'\n') {
                        self.state.pos += 2;
                    } else {
                        self.state.pos += 1;
                    }
                    self.state.token = SyntaxKind::NewLineTrivia;
                }
                b'!' => {
                    if self.ascii_at(1) == Some(b'=') {
                        if self.ascii_at(2) == Some(b'=') {
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
                b'`' => {
                    self.state.token = self.scan_template_and_set_token_value(false);
                }
                b'%' => {
                    if self.ascii_at(1) == Some(b'=') {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::PercentEqualsToken;
                    } else {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::PercentToken;
                    }
                }
                b'&' => match self.ascii_at(1) {
                    Some(b'&') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::AmpersandAmpersandEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::AmpersandAmpersandToken;
                        }
                    },
                    Some(b'=') => {
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
                    Some(b'=') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::AsteriskEqualsToken;
                    }
                    Some(b'*') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::AsteriskAsteriskEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::AsteriskAsteriskToken;
                        }
                    },
                    _ => {
                        self.state.pos += 1;
                        if self.state.skip_jsdoc_leading_asterisks != 0
                            && !self
                                .state
                                .token_flags
                                .contains(TokenFlags::PrecedingJSDocLeadingAsterisks)
                            && self
                                .state
                                .token_flags
                                .contains(TokenFlags::PrecedingLineBreak)
                        {
                            self.state
                                .token_flags
                                .insert(TokenFlags::PrecedingJSDocLeadingAsterisks);
                            continue;
                        }
                        self.state.token = SyntaxKind::AsteriskToken;
                    }
                },
                b'+' => match self.ascii_at(1) {
                    Some(b'=') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::PlusEqualsToken;
                    }
                    Some(b'+') => {
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
                    Some(b'=') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::MinusEqualsToken;
                    }
                    Some(b'-') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::MinusMinusToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::MinusToken;
                    }
                },
                b'.' => match self.ascii_at(1) {
                    Some(b'.') if self.ascii_at(2) == Some(b'.') => {
                        self.state.pos += 3;
                        self.state.token = SyntaxKind::DotDotDotToken;
                    }
                    Some(c) if c.is_ascii_digit() => {
                        self.state.token = self.scan_number();
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::DotToken
                    }
                },
                b'/' => match self.ascii_at(1) {
                    Some(b'/') => {
                        // Single-line comment
                        self.state.pos += 2;
                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'\r' | b'\n'));
                            if let Some(c) = self.char()
                                && !is_line_break(c)
                            {
                                self.state.pos += c.len_utf8();
                            } else {
                                break;
                            }
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
                    Some(b'*') => {
                        self.state.pos += 2;
                        let is_jsdoc = self.ascii() == Some(b'*') && self.ascii_at(1) != Some(b'/');

                        let mut comment_closed = false;
                        let mut last_line_start = self.state.token_start;

                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'*' | b'\n' | b'\r'));
                            let Some(c) = self.char() else {
                                break;
                            };

                            if c == '*' && self.ascii_at(1) == Some(b'/') {
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
                            self.error(diagnostics::E1010_ASTERISK_SLASH_EXPECTED);
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
                    Some(b'=') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::SlashEqualsToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::SlashToken;
                    }
                },
                b'0' => {
                    if let Some(b'x' | b'X') = self.ascii_at(1) {
                        self.state.pos += 2;
                        let mut digits = self.scan_hex_digits(1, true, true);
                        if digits.is_empty() {
                            self.error(diagnostics::E1125_HEXADECIMAL_DIGIT_EXPECTED);
                            digits = String::from('0');
                        }

                        self.state.token_value = format!("0x{digits}");
                        self.state.token_flags.insert(TokenFlags::HexSpecifier);
                        self.state.token = self.scan_big_int_suffix();
                        break;
                    }

                    if let Some(b'b' | b'B') = self.ascii_at(1) {
                        self.state.pos += 2;
                        let mut digits = self.scan_binary_or_octal_digits(2);
                        if digits.is_empty() {
                            self.error(diagnostics::E1177_BINARY_DIGIT_EXPECTED);
                            digits = String::from('0');
                        }

                        self.state.token_value = format!("0b{digits}");
                        self.state.token_flags.insert(TokenFlags::BinarySpecifier);
                        self.state.token = self.scan_big_int_suffix();
                        break;
                    }

                    if let Some(b'o' | b'O') = self.ascii_at(1) {
                        self.state.pos += 2;
                        let mut digits = self.scan_binary_or_octal_digits(8);
                        if digits.is_empty() {
                            self.error(diagnostics::E1178_OCTAL_DIGIT_EXPECTED);
                            digits = String::from('0');
                        }

                        self.state.token_value = format!("0o{digits}");
                        self.state.token_flags.insert(TokenFlags::OctalSpecifier);
                        self.state.token = self.scan_big_int_suffix();
                        break;
                    }

                    self.state.token = self.scan_number();
                }
                b'1'..=b'9' => {
                    self.state.token = self.scan_number();
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
                    Some(b'<') if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    Some(b'<') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::LessThanLessThanEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::LessThanLessThanToken;
                        }
                    },
                    Some(b'=') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::LessThanEqualsToken;
                    }
                    Some(b'/')
                        if self.language_variant == LanguageVariant::JSX
                            && self.ascii_at(2) != Some(b'*') =>
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
                    Some(b'=') if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    Some(b'=') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::EqualsEqualsEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::EqualsEqualsToken;
                        }
                    },
                    Some(b'>') => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::EqualsGreaterThanToken;
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::EqualsToken;
                    }
                },
                b'>' => match self.ascii_at(1) {
                    Some(b'>') if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    _ => {
                        self.state.pos += 1;
                        self.state.token = SyntaxKind::GreaterThanToken;
                    }
                },
                b'?' => match self.ascii_at(1) {
                    Some(b'.') if !matches!(self.ascii_at(2), Some(b'0'..=b'9')) => {
                        self.state.pos += 2;
                        self.state.token = SyntaxKind::QuestionDotToken;
                    }
                    Some(b'?') => match self.ascii_at(2) {
                        Some(b'=') => {
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
                    Some(b'=') => {
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
                    Some(b'|') if is_conflict_marker_trivia(&self.text, self.state.pos) => {
                        todo!("handle conflict trivia")
                    }
                    Some(b'|') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.state.pos += 3;
                            self.state.token = SyntaxKind::BarBarEqualsToken;
                        }
                        _ => {
                            self.state.pos += 2;
                            self.state.token = SyntaxKind::BarBarToken;
                        }
                    },
                    Some(b'=') => {
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
                    if let Some(c) = self.peek_unicode_escape()
                        && is_identifier_start(c)
                    {
                        self.scan_unicode_escape(true).unwrap();
                        self.state.token_value = format!("{c}{}", self.scan_identifier_parts());
                    }
                    todo!("Escape")
                }
                b'#' => match self.ascii_at(1) {
                    Some(b'!') => todo!("shebang parsing"),
                    Some(b'\\') => todo!("private identifier with escape"),
                    _ => todo!("private identifier"),
                },
                _ => {
                    if self.scan_identifier(0) {
                        self.state.token = get_identifier_token(&self.state.token_value);
                        break;
                    }

                    let c = self.char().unwrap();
                    if is_whitespace_single_line(c) {
                        self.state.pos += c.len_utf8();

                        // If we get here and it's not 0x0085 (nextLine), then we're handling non-ASCII whitespace.
                        // Handle skipTrivia like we do in the space case above.
                        if c == '\u{0085}' || self.skip_trivia {
                            continue;
                        }

                        while let Some(c) = self.char()
                            && is_whitespace_single_line(c)
                        {
                            self.state.pos += c.len_utf8();
                        }
                        self.state.token = SyntaxKind::WhitespaceTrivia;
                        break;
                    }

                    if is_line_break(c) {
                        self.state
                            .token_flags
                            .insert(TokenFlags::PrecedingLineBreak);
                        self.state.pos += c.len_utf8();
                        continue;
                    }

                    self.scan_invalid_character();
                }
            }
            break;
        }
        self.state.token
    }

    fn scan_string(&mut self, jsx_attribute_string: bool) -> String {
        let quote = self.ascii().unwrap();
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
                if jsx_attribute_string || !s.contains(|c| matches!(c, '\\' | '\r' | '\n')) {
                    self.state.pos += n + 1;
                    return String::from(s);
                }
            }
            None => {}
        }

        let mut buf = String::new();
        let mut start = self.state.pos;
        loop {
            let Some(c) = self.ascii() else {
                buf.push_str(&self.text[start..self.state.pos]);
                self.state.token_flags.insert(TokenFlags::Unterminated);
                self.error(diagnostics::E1002_UNTERMINATED_STRING_LITERAL);
                break;
            };

            if c == quote {
                buf.push_str(&self.text[start..self.state.pos]);
                self.state.pos += 1;
                break;
            }

            if c == b'\\' && !jsx_attribute_string {
                buf.push_str(&self.text[start..self.state.pos]);
                buf.push_str(&self.scan_escape_sequence(
                    EscapeSequenceScanningFlags::String | EscapeSequenceScanningFlags::ReportErrors,
                ));
                start = self.state.pos;
                continue;
            }

            if matches!(c, b'\r' | b'\n') && !jsx_attribute_string {
                buf.push_str(&self.text[start..self.state.pos]);
                self.state.token_flags.insert(TokenFlags::Unterminated);
                self.error(diagnostics::E1002_UNTERMINATED_STRING_LITERAL);
                break;
            }

            self.state.pos += 1;
        }

        buf
    }

    fn scan_template_and_set_token_value(
        &mut self,
        should_emit_invalid_escape_error: bool,
    ) -> SyntaxKind {
        let started_with_backtick = self.ascii() == Some(b'`');
        self.state.pos += 1;
        let mut start = self.state.pos;

        let mut buf = String::new();
        let token = loop {
            self.scan_ascii_while(|c| !matches!(c, b'`' | b'$' | b'\\' | b'\r'));
            let c = self.ascii();

            if c == None || c == Some(b'`') {
                buf.push_str(&self.text[start..self.state.pos]);
                if c == None {
                    self.state.token_flags.insert(TokenFlags::Unterminated);
                    self.error(diagnostics::E1160_UNTERMINATED_TEMPLATE_LITERAL);
                } else {
                    self.state.pos += 1;
                }
                break if started_with_backtick {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            }

            if c == Some(b'$') && self.ascii_at(1) == Some(b'{') {
                buf.push_str(&self.text[start..self.state.pos]);
                self.state.pos += 2;
                break if started_with_backtick {
                    SyntaxKind::TemplateHead
                } else {
                    SyntaxKind::TemplateMiddle
                };
            }

            if c == Some(b'\\') {
                buf.push_str(&self.text[start..self.state.pos]);
                buf.push_str(
                    &self.scan_escape_sequence(if should_emit_invalid_escape_error {
                        EscapeSequenceScanningFlags::String
                            | EscapeSequenceScanningFlags::ReportErrors
                    } else {
                        EscapeSequenceScanningFlags::String
                    }),
                );
                start = self.state.pos;
                continue;
            }

            // Speculated ECMAScript 6 Spec 11.8.6.1:
            // <CR><LF> and <CR> LineTerminatorSequences are normalized to <LF> for Template Values
            if c == Some(b'\r') {
                buf.push_str(&self.text[start..self.state.pos]);
                self.state.pos += 1;
                if self.ascii() == Some(b'\n') {
                    self.state.pos += 1;
                }
                buf.push('\n');
                start = self.state.pos;
                continue;
            }

            self.state.pos += 1;
        };
        self.state.token_value = buf;
        token
    }

    fn scan_escape_sequence(&mut self, flags: EscapeSequenceScanningFlags) -> String {
        let mut start = self.state.pos;
        self.state.pos += 1;
        let Some(c) = self.ascii() else {
            self.error(diagnostics::E1126_UNEXPECTED_END_OF_TEXT);
            return String::new();
        };
        self.state.pos += 1;
        if c == b'0' {
            if let Some(b'0'..=b'9') = self.ascii() {
                return String::from("\0");
            }
            // fallthrough
        }
        if let b'0'..=b'3' = c {
            if let Some(b'0'..=b'7') = self.ascii() {
                self.state.pos += 1;
            }
            // fallthrough
        }
        if let b'0'..=b'7' = c {
            if let Some(b'0'..=b'7') = self.ascii() {
                self.state.pos += 1;
            }
            self.state
                .token_flags
                .insert(TokenFlags::ContainsInvalidEscape);
            if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                todo!()
            }
            return self.text[start..self.state.pos].to_string();
        }
        if let b'8' | b'9' = c {
            self.state
                .token_flags
                .insert(TokenFlags::ContainsInvalidEscape);
            if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                todo!()
            }
            return self.text[start..self.state.pos].to_string();
        }
        match c {
            b'b' => String::from('\u{08}'),
            b't' => String::from('\t'),
            b'n' => String::from('\n'),
            b'v' => String::from('\x0B'),
            b'f' => String::from('\x0C'),
            b'r' => String::from('\r'),
            b'\'' => String::from('\''),
            b'"' => String::from('"'),
            b'u' => todo!("Unicode escape"),
            b'x' => todo!("hexadecimal escape"),
            b'\r' => {
                if self.ascii() == Some(b'\n') {
                    self.state.pos += 1;
                }
                return String::new();
            }
            b'\n' => return String::new(),
            _ => {
                // ch was read as a single byte; for multi-byte UTF-8 characters,
                // we need to decode the full rune and advance past all its bytes.
                let c = if c.is_ascii() {
                    c as char
                } else {
                    self.state.pos -= 1;
                    let c = self.char().unwrap();
                    self.state.pos += c.len_utf8();
                    c
                };

                // LineContinuation: a backslash followed by a line terminator is "the empty code unit sequence".
                if c == '\u{2028}' || c == '\u{2029}' {
                    return String::new();
                }
                if flags.contains(EscapeSequenceScanningFlags::AnyUnicodeMode)
                    || flags.contains(EscapeSequenceScanningFlags::RegularExpression)
                        && !flags.contains(EscapeSequenceScanningFlags::AnnexB)
                        && is_identifier_part(c)
                {
                    self.error_at(
                        diagnostics::E1535_THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
                        start,
                        self.state.pos - start,
                    );
                }
                String::from(c)
            }
        }
    }

    fn scan_identifier(&mut self, prefix_len: usize) -> bool {
        let start = self.state.pos;
        self.state.pos += prefix_len;

        // Fast path for simple ASCII identifiers
        if let Some(c) = self.ascii()
            && (c.is_ascii_alphabetic() || c == b'_' || c == b'$')
        {
            self.state.pos += 1;
            self.scan_ascii_while(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$');
            if let Some(c) = self.ascii()
                && c.is_ascii()
                && c != b'\\'
            {
                self.state.token_value = self.text[start..self.state.pos].to_string();
                return true;
            }
            self.state.pos = start + prefix_len;
        }

        if let Some(c) = self.char()
            && is_identifier_start(c)
        {
            self.state.pos += c.len_utf8();
            while let Some(c) = self.char()
                && is_identifier_part(c)
            {
                self.state.pos += c.len_utf8();
            }
            self.state.token_value = self.text[start..self.state.pos].to_string();
            if c == '\\' {
                let rest = self.scan_identifier_parts();
                self.state.token_value.push_str(&rest);
            }
            return true;
        }

        false
    }

    fn scan_identifier_parts(&mut self) -> String {
        let mut out = String::new();
        let mut start = self.state.pos;

        while let Some(c) = self.char() {
            if is_identifier_part(c) {
                self.state.pos += c.len_utf8();
                continue;
            }

            if c == '\\'
                && let Some(escaped) = self.peek_unicode_escape()
                && is_identifier_part(escaped)
            {
                self.scan_unicode_escape(true).unwrap();
                out.push_str(&self.text[start..self.state.pos]);
                out.push(escaped);
                start = self.state.pos;
                continue;
            }

            break;
        }
        out.push_str(&self.text[start..self.state.pos]);
        out
    }

    fn peek_unicode_escape(&mut self) -> Option<char> {
        if self.ascii_at(1) == Some(b'u') {
            let save_pos = self.state.pos;
            let save_token_flags = self.state.token_flags;
            let c = self.scan_unicode_escape(false);
            self.state.pos = save_pos;
            self.state.token_flags = save_token_flags;
            c
        } else {
            None
        }
    }

    // Known to be at \u
    fn scan_unicode_escape(&mut self, should_emit_invalid_escape_error: bool) -> Option<char> {
        todo!()
    }

    fn scan_invalid_character(&mut self) {
        let c = self.char().unwrap();
        self.error_at(
            diagnostics::E1127_INVALID_CHARACTER,
            self.state.pos,
            c.len_utf8(),
        );
        self.state.pos += c.len_utf8();
        self.state.token = SyntaxKind::Unknown;
    }

    fn scan_number(&mut self) -> SyntaxKind {
        let start = self.state.pos;
        let fixed_part: String;
        if self.ascii() == Some(b'0') {
            self.state.pos += 1;
            if self.ascii() == Some(b'_') {
                self.state
                    .token_flags
                    .insert(TokenFlags::ContainsSeparator | TokenFlags::ContainsInvalidSeparator);
                self.error_at(
                    diagnostics::E6188_NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                    self.state.pos,
                    1,
                );
                self.state.pos = start;
                fixed_part = self.scan_number_fragment();
            } else {
                let (digits, is_octal) = self.scan_digits();
                if digits.is_empty() {
                    fixed_part = String::from('0');
                } else if !is_octal {
                    self.state
                        .token_flags
                        .insert(TokenFlags::ContainsLeadingZero);
                    fixed_part = digits;
                } else {
                    todo!("scan rest")
                }
            }
        } else {
            fixed_part = self.scan_number_fragment();
        }
        let fixed_part_end = self.state.pos;
        let mut fractional_part = String::new();
        let mut exponent_preamble = String::new();
        let mut exponent_part = String::new();

        if self.ascii() == Some(b'.') {
            self.state.pos += 1;
            fractional_part = self.scan_number_fragment();
        }
        let mut end = self.state.pos;
        if let Some(b'e' | b'E') = self.ascii() {
            self.state.pos += 1;
            self.state.token_flags.insert(TokenFlags::Scientific);
            if let Some(b'+' | b'-') = self.ascii() {
                self.state.pos += 1;
            }
            let start_numeric_part = self.state.pos;
            exponent_part = self.scan_number_fragment();
            if exponent_part.is_empty() {
                self.error(diagnostics::E1124_DIGIT_EXPECTED);
            } else {
                exponent_preamble = self.text[end..start_numeric_part].to_string();
                end = self.state.pos;
            }
        }
        if self
            .state
            .token_flags
            .contains(TokenFlags::ContainsSeparator)
        {
            self.state.token_value = fixed_part;
            if !fractional_part.is_empty() {
                self.state.token_value.push('.');
                self.state.token_value.push_str(&fractional_part);
            }
            if !exponent_part.is_empty() {
                self.state.token_value.push_str(&exponent_preamble);
                self.state.token_value.push_str(&exponent_part);
            }
        } else {
            self.state.token_value = self.text[start..end].to_string();
        }
        if self
            .state
            .token_flags
            .contains(TokenFlags::ContainsLeadingZero)
        {
            self.error_at(
                diagnostics::E1489_DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED,
                start,
                self.state.pos - start,
            );
            self.state.token_value = todo!("jsnum");
            return SyntaxKind::NumericLiteral;
        }
        let result = if fixed_part_end == self.state.pos {
            self.scan_big_int_suffix()
        } else {
            self.state.token_value = todo!("jsnum");
            SyntaxKind::NumericLiteral
        };
        if let Some(c) = self.char()
            && is_identifier_start(c)
        {
            let id_start = self.state.pos;
            let id = self.scan_identifier_parts();
            if result != SyntaxKind::BigIntLiteral
                && id.len() == 1
                && self.text.as_bytes()[id_start] == b'n'
            {
                if self.state.token_flags.contains(TokenFlags::Scientific) {
                    self.error_at(
                        diagnostics::E1352_A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION,
                        start,
                        self.state.pos - start,
                    );
                    return result;
                }
                if fixed_part_end < id_start {
                    self.error_at(
                        diagnostics::E1353_A_BIGINT_LITERAL_MUST_BE_AN_INTEGER,
                        start,
                        self.state.pos - start,
                    );
                    return result;
                }
            }
            self.error_at(diagnostics::E1351_AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL, id_start, self.state.pos - id_start);
            self.state.pos = id_start;
        }
        result
    }

    fn scan_number_fragment(&mut self) -> String {
        let mut start = self.state.pos;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        let mut result = String::new();
        loop {
            let before = self.state.pos;
            self.scan_ascii_while(|c| c.is_ascii_digit());
            if self.state.pos > before {
                allow_separator = true;
                is_previous_token_separator = false;
            }

            if self.ascii() == Some(b'_') {
                self.state.token_flags.insert(TokenFlags::ContainsSeparator);
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                    result.push_str(&self.text[start..self.state.pos]);
                } else {
                    self.state
                        .token_flags
                        .insert(TokenFlags::ContainsInvalidSeparator);
                    if is_previous_token_separator {
                        self.error_at(diagnostics::E6189_MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED, self.state.pos, 1);
                    } else {
                        self.error_at(
                            diagnostics::E6188_NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                            self.state.pos,
                            1,
                        );
                    }
                }
                self.state.pos += 1;
                start = self.state.pos;
                continue;
            }
            break;
        }

        if is_previous_token_separator {
            self.state
                .token_flags
                .insert(TokenFlags::ContainsInvalidSeparator);
            self.error_at(
                diagnostics::E6188_NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                self.state.pos - 1,
                1,
            );
        }
        result.push_str(&self.text[start..self.state.pos]);
        result
    }

    fn scan_digits(&mut self) -> (String, bool) {
        let start = self.state.pos;
        let mut is_octal = true;
        while let Some(c) = self.ascii()
            && c.is_ascii_digit()
        {
            if c > b'7' {
                is_octal = false;
            }
            self.state.pos += 1;
        }
        (self.text[start..self.state.pos].to_string(), is_octal)
    }

    fn scan_hex_digits(
        &mut self,
        min_count: usize,
        scan_as_many_as_possible: bool,
        can_have_separators: bool,
    ) -> String {
        todo!()
    }

    fn scan_binary_or_octal_digits(&mut self, base: u8) -> String {
        let mut out = String::new();
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        while let Some(c) = self.ascii() {
            if c.is_ascii_digit() && c - b'0' < base {
                out.push(c as char);
                allow_separator = true;
                is_previous_token_separator = false;
            } else if c == b'_' {
                self.state.token_flags.insert(TokenFlags::ContainsSeparator);
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                } else if is_previous_token_separator {
                    self.error_at(diagnostics::E6189_MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED, self.state.pos, 1);
                } else {
                    self.error_at(
                        diagnostics::E6188_NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                        self.state.pos,
                        1,
                    );
                }
            } else {
                break;
            }
            self.state.pos += 1;
        }
        if is_previous_token_separator {
            self.error_at(
                diagnostics::E6188_NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                self.state.pos - 1,
                1,
            );
        }
        out
    }

    fn scan_big_int_suffix(&self) -> SyntaxKind {
        todo!()
    }

    fn process_comment_directive(&mut self, start: usize, end: usize, multiline: bool) {
        todo!()
    }

    fn ascii(&self) -> Option<u8> {
        self.text.as_bytes().get(self.state.pos).copied()
    }

    fn ascii_at(&self, offset: usize) -> Option<u8> {
        self.text.as_bytes().get(self.state.pos + offset).copied()
    }

    fn char(&self) -> Option<char> {
        self.text
            .get(self.state.pos..)
            .and_then(|s| s.chars().next())
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

    fn error(&self, message: &'static diagnostics::Message) {
        self.error_at(message, self.state.pos, 0);
    }

    fn error_at(&self, message: &'static diagnostics::Message, pos: usize, length: usize) {
        self.error_with_args(message, pos, length, &[])
    }

    fn error_with_args(
        &self,
        message: &'static diagnostics::Message,
        pos: usize,
        length: usize,
        args: &[String],
    ) {
        if let Some(on_error) = self.on_error {
            on_error(message, pos, length, args)
        }
    }
}

fn get_identifier_token(s: &str) -> SyntaxKind {
    if let 2..=12 = s.len()
        && let b'a'..=b'z' = s.as_bytes()[0]
    {
        if let Some(keyword) = text_to_keyword(s) {
            return keyword;
        }
    }
    SyntaxKind::Identifier
}

fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || is_unicode_identifier_start(c)
}

fn is_identifier_part(c: char) -> bool {
    is_identifier_part_ex(c, LanguageVariant::Standard)
}

fn is_identifier_part_ex(c: char, language_variant: LanguageVariant) -> bool {
    is_word_character(c)
        || c == '$'
        || is_unicode_identifier_part(c)
        || language_variant == LanguageVariant::JSX && matches!(c, '-' | ':') // "-" and ":" are valid in JSX Identifiers
}

fn is_word_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_unicode_identifier_start(c: char) -> bool {
    CodePointSetData::new::<IdStart>().contains(c)
}

fn is_unicode_identifier_part(c: char) -> bool {
    is_unicode_identifier_start(c) || CodePointSetData::new::<IdContinue>().contains(c)
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
