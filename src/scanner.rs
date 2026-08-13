use icu_properties::{
    CodePointSetData,
    props::{IdContinue, IdStart},
};
use rustc_hash::FxHashMap;

use crate::{
    ast::{CommentRange, Identifier, Node, NodeFactory, StringLiteral},
    diagnostics::{Diagnostics, Message},
    flags::{EscapeSequenceScanningFlags, NodeFlags, RegexpFlags, TokenFlags},
    number::{self, Number},
    options::{LanguageVariant, ScriptTarget},
    regexp_parser::RegexpParser,
    syntax::{CommentDirective, CommentDirectiveKind, SyntaxKind, TextRange, text_to_keyword},
};

#[derive(Debug, Default)]
pub struct ScannerState {
    pos: usize,
    full_start_pos: usize,
    token_start: usize,
    token: SyntaxKind,
    token_value: String,
    token_flags: TokenFlags,
    comment_directives: Vec<CommentDirective>,
    skip_jsdoc_leading_asterisks: u32,
}

#[derive(Debug, Default)]
pub struct Scanner {
    pub(crate) text: String,
    end: usize,
    language_variant: LanguageVariant,
    script_target: ScriptTarget,
    diagnostics: Option<Diagnostics>,
    skip_trivia: bool,
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

impl Scanner {
    pub fn new() -> Self {
        Self { skip_trivia: true, ..Self::default() }
    }

    pub fn set_text(&mut self, text: String) {
        self.end = text.len();
        self.text = text;
        self.rewind(ScannerState::default());
    }

    pub fn set_diagnostics(&mut self, diagnostics: Diagnostics) {
        self.diagnostics.replace(diagnostics);
    }

    pub fn reset_pos(&mut self, pos: usize) {
        self.pos = pos;
        self.full_start_pos = pos;
        self.token_start = pos;
    }

    pub fn token_value(&self) -> &str {
        &self.token_value
    }

    pub fn token_flags(&self) -> TokenFlags {
        self.token_flags
    }

    pub fn token_text(&self) -> &str {
        &self.text[self.token_start..self.pos]
    }

    pub fn comment_directives(&self) -> Vec<CommentDirective> {
        self.comment_directives.clone()
    }

    pub fn has_unicode_escape(&self) -> bool {
        self.token_flags.contains(TokenFlags::UnicodeEscape)
    }

    pub fn has_extended_unicode_escape(&self) -> bool {
        self.token_flags.contains(TokenFlags::ExtendedUnicodeEscape)
    }

    pub fn has_preceding_line_break(&self) -> bool {
        self.token_flags.contains(TokenFlags::PrecedingLineBreak)
    }

    pub fn has_preceding_jsdoc_comment(&self) -> bool {
        self.token_flags.contains(TokenFlags::PrecedingJSDocComment)
    }

    pub fn has_preceding_jsdoc_with_deprecated_tag(&self) -> bool {
        self.token_flags.contains(TokenFlags::PrecedingJSDocWithDeprecated)
    }

    pub fn has_preceding_jsdoc_with_see_or_link(&self) -> bool {
        self.token_flags.contains(TokenFlags::PrecedingJSDocWithSeeOrLink)
    }

    pub(crate) fn has_preceding_jsdoc_leading_asterisks(&self) -> bool {
        self.token_flags.contains(TokenFlags::PrecedingJSDocLeadingAsterisks)
    }

    pub fn mark(&self) -> ScannerState {
        ScannerState {
            pos: self.pos,
            full_start_pos: self.full_start_pos,
            token_start: self.token_start,
            token: self.token,
            token_value: self.token_value.clone(),
            token_flags: self.token_flags,
            comment_directives: self.comment_directives.clone(),
            skip_jsdoc_leading_asterisks: self.skip_jsdoc_leading_asterisks,
        }
    }

    pub fn rewind(&mut self, state: ScannerState) {
        self.pos = state.pos;
        self.full_start_pos = state.full_start_pos;
        self.token_start = state.token_start;
        self.token = state.token;
        self.token_value = state.token_value;
        self.token_flags = state.token_flags;
        self.comment_directives = state.comment_directives;
        self.skip_jsdoc_leading_asterisks = state.skip_jsdoc_leading_asterisks;
    }

    pub fn token(&self) -> SyntaxKind {
        self.token
    }

    pub fn token_range(&self) -> TextRange {
        TextRange::new(self.token_start, self.pos)
    }

    pub fn full_token_start(&self) -> usize {
        self.full_start_pos
    }

    pub(crate) fn token_start(&self) -> usize {
        self.token_start
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_skip_jsdoc_leading_asterisks(&mut self, skip: bool) {
        if skip {
            self.skip_jsdoc_leading_asterisks += 1;
        } else {
            self.skip_jsdoc_leading_asterisks -= 1;
        }
    }

    pub fn scan(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = TokenFlags::empty();
        self.token_value = String::new();

        loop {
            self.token_start = self.pos;
            let Some(c) = self.ascii() else {
                self.token = SyntaxKind::EndOfFile;
                break;
            };

            match c {
                b'\t' | 0x0B | 0x0C | b' ' => {
                    self.pos += 1;
                    if self.skip_trivia {
                        continue;
                    }
                    while let Some(c) = self.char()
                        && is_whitespace_single_line(c)
                    {
                        self.pos += c.len_utf8();
                    }
                    self.token = SyntaxKind::WhitespaceTrivia;
                }
                b'\n' | b'\r' => {
                    self.token_flags.insert(TokenFlags::PrecedingLineBreak);
                    if self.skip_trivia {
                        self.pos += 1;
                        self.scan_ascii_while(|c| matches!(c, b' ' | b'\t'..=b'\r'));
                        continue;
                    }
                    if c == b'\r' && self.ascii_at(1) == Some(b'\n') {
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                    }
                    self.token = SyntaxKind::NewLineTrivia;
                }
                b'!' => {
                    if self.ascii_at(1) == Some(b'=') {
                        if self.ascii_at(2) == Some(b'=') {
                            self.pos += 3;
                            self.token = SyntaxKind::ExclamationEqualsEqualsToken;
                        } else {
                            self.pos += 2;
                            self.token = SyntaxKind::ExclamationEqualsToken;
                        }
                    } else {
                        self.pos += 1;
                        self.token = SyntaxKind::ExclamationToken;
                    }
                }
                b'"' | b'\'' => {
                    self.token_value = self.scan_string(false);
                    self.token = SyntaxKind::StringLiteral;
                }
                b'`' => {
                    self.token = self.scan_template_and_set_token_value(false);
                }
                b'%' => {
                    if self.ascii_at(1) == Some(b'=') {
                        self.pos += 2;
                        self.token = SyntaxKind::PercentEqualsToken;
                    } else {
                        self.pos += 1;
                        self.token = SyntaxKind::PercentToken;
                    }
                }
                b'&' => match self.ascii_at(1) {
                    Some(b'&') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::AmpersandAmpersandEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::AmpersandAmpersandToken;
                        }
                    },
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::AmpersandEqualsToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::AmpersandToken;
                    }
                },
                b'(' => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenParenToken;
                }
                b')' => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseParenToken;
                }
                b'*' => match self.ascii_at(1) {
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::AsteriskEqualsToken;
                    }
                    Some(b'*') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::AsteriskAsteriskEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::AsteriskAsteriskToken;
                        }
                    },
                    _ => {
                        self.pos += 1;
                        if self.skip_jsdoc_leading_asterisks != 0
                            && !self
                                .token_flags
                                .contains(TokenFlags::PrecedingJSDocLeadingAsterisks)
                            && self.token_flags.contains(TokenFlags::PrecedingLineBreak)
                        {
                            self.token_flags.insert(TokenFlags::PrecedingJSDocLeadingAsterisks);
                            continue;
                        }
                        self.token = SyntaxKind::AsteriskToken;
                    }
                },
                b'+' => match self.ascii_at(1) {
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::PlusEqualsToken;
                    }
                    Some(b'+') => {
                        self.pos += 2;
                        self.token = SyntaxKind::PlusPlusToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::PlusToken;
                    }
                },
                b',' => {
                    self.pos += 1;
                    self.token = SyntaxKind::CommaToken;
                }
                b'-' => match self.ascii_at(1) {
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::MinusEqualsToken;
                    }
                    Some(b'-') => {
                        self.pos += 2;
                        self.token = SyntaxKind::MinusMinusToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::MinusToken;
                    }
                },
                b'.' => match self.ascii_at(1) {
                    Some(b'.') if self.ascii_at(2) == Some(b'.') => {
                        self.pos += 3;
                        self.token = SyntaxKind::DotDotDotToken;
                    }
                    Some(c) if c.is_ascii_digit() => {
                        self.token = self.scan_number();
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::DotToken
                    }
                },
                b'/' => match self.ascii_at(1) {
                    Some(b'/') => {
                        // Single-line comment
                        self.pos += 2;
                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'\r' | b'\n'));
                            if let Some(c) = self.char()
                                && !is_line_break(c)
                            {
                                self.pos += c.len_utf8();
                            } else {
                                break;
                            }
                        }

                        self.process_comment_directive(self.token_start, self.pos, false);
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::SingleLineCommentTrivia;
                        return self.token;
                    }
                    Some(b'*') => {
                        self.pos += 2;
                        let is_jsdoc = self.ascii() == Some(b'*') && self.ascii_at(1) != Some(b'/');

                        let mut comment_closed = false;
                        let mut last_line_start = self.token_start;

                        loop {
                            self.scan_ascii_while(|c| !matches!(c, b'*' | b'\n' | b'\r'));
                            let Some(c) = self.char() else {
                                break;
                            };

                            if c == '*' && self.ascii_at(1) == Some(b'/') {
                                self.pos += 2;
                                comment_closed = true;
                                break;
                            }

                            self.pos += c.len_utf8();
                            if is_line_break(c) {
                                last_line_start = self.pos;
                                self.token_flags.insert(TokenFlags::PrecedingLineBreak);
                            }
                        }

                        if is_jsdoc {
                            self.token_flags.insert(TokenFlags::PrecedingJSDocComment);
                            self.scan_jsdoc_comment_for_tags(self.token_start, self.pos);
                        }

                        self.process_comment_directive(last_line_start, self.pos, true);

                        if !comment_closed {
                            self.error(Message::e1010_asterisk_slash_expected());
                        }
                        if self.skip_trivia {
                            continue;
                        }
                        if !comment_closed {
                            self.token_flags.insert(TokenFlags::Unterminated);
                        }
                        self.token = SyntaxKind::MultiLineCommentTrivia;
                        return self.token;
                    }
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::SlashEqualsToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::SlashToken;
                    }
                },
                b'0' => {
                    if let Some(b'x' | b'X') = self.ascii_at(1) {
                        self.pos += 2;
                        let mut digits = self.scan_hex_digits(1, true, true);
                        if digits.is_empty() {
                            self.error(Message::e1125_hexadecimal_digit_expected());
                            digits = String::from('0');
                        }

                        self.token_value = format!("0x{digits}");
                        self.token_flags.insert(TokenFlags::HexSpecifier);
                        self.token = self.scan_big_int_suffix();
                        break;
                    }

                    if let Some(b'b' | b'B') = self.ascii_at(1) {
                        self.pos += 2;
                        let mut digits = self.scan_binary_or_octal_digits(2);
                        if digits.is_empty() {
                            self.error(Message::e1177_binary_digit_expected());
                            digits = String::from('0');
                        }

                        self.token_value = format!("0b{digits}");
                        self.token_flags.insert(TokenFlags::BinarySpecifier);
                        self.token = self.scan_big_int_suffix();
                        break;
                    }

                    if let Some(b'o' | b'O') = self.ascii_at(1) {
                        self.pos += 2;
                        let mut digits = self.scan_binary_or_octal_digits(8);
                        if digits.is_empty() {
                            self.error(Message::e1178_octal_digit_expected());
                            digits = String::from('0');
                        }

                        self.token_value = format!("0o{digits}");
                        self.token_flags.insert(TokenFlags::OctalSpecifier);
                        self.token = self.scan_big_int_suffix();
                        break;
                    }

                    self.token = self.scan_number();
                }
                b'1'..=b'9' => {
                    self.token = self.scan_number();
                }
                b':' => {
                    self.pos += 1;
                    self.token = SyntaxKind::ColonToken;
                }
                b';' => {
                    self.pos += 1;
                    self.token = SyntaxKind::SemicolonToken;
                }
                b'<' => match self.ascii_at(1) {
                    Some(b'<') if is_conflict_marker_trivia(&self.text, self.pos) => {
                        self.pos = scan_conflict_marker_trivia(
                            &self.text,
                            self.pos,
                            self.diagnostics.as_ref(),
                        );
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
                    Some(b'<') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::LessThanLessThanEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::LessThanLessThanToken;
                        }
                    },
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::LessThanEqualsToken;
                    }
                    Some(b'/')
                        if self.language_variant == LanguageVariant::JSX
                            && self.ascii_at(2) != Some(b'*') =>
                    {
                        self.pos += 2;
                        self.token = SyntaxKind::LessThanSlashToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::LessThanToken;
                    }
                },
                b'=' => match self.ascii_at(1) {
                    Some(b'=') if is_conflict_marker_trivia(&self.text, self.pos) => {
                        self.pos = scan_conflict_marker_trivia(
                            &self.text,
                            self.pos,
                            self.diagnostics.as_ref(),
                        );
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
                    Some(b'=') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::EqualsEqualsEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::EqualsEqualsToken;
                        }
                    },
                    Some(b'>') => {
                        self.pos += 2;
                        self.token = SyntaxKind::EqualsGreaterThanToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::EqualsToken;
                    }
                },
                b'>' => match self.ascii_at(1) {
                    Some(b'>') if is_conflict_marker_trivia(&self.text, self.pos) => {
                        self.pos = scan_conflict_marker_trivia(
                            &self.text,
                            self.pos,
                            self.diagnostics.as_ref(),
                        );
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::GreaterThanToken;
                    }
                },
                b'?' => match self.ascii_at(1) {
                    Some(b'.') if !matches!(self.ascii_at(2), Some(b'0'..=b'9')) => {
                        self.pos += 2;
                        self.token = SyntaxKind::QuestionDotToken;
                    }
                    Some(b'?') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::QuestionQuestionEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::QuestionQuestionToken;
                        }
                    },
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::QuestionToken;
                    }
                },
                b'[' => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenBracketToken;
                }
                b']' => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseBracketToken;
                }
                b'^' => match self.ascii_at(1) {
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::CaretEqualsToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::CaretToken;
                    }
                },
                b'{' => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenBraceToken;
                }
                b'|' => match self.ascii_at(1) {
                    Some(b'|') if is_conflict_marker_trivia(&self.text, self.pos) => {
                        self.pos = scan_conflict_marker_trivia(
                            &self.text,
                            self.pos,
                            self.diagnostics.as_ref(),
                        );
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
                    Some(b'|') => match self.ascii_at(2) {
                        Some(b'=') => {
                            self.pos += 3;
                            self.token = SyntaxKind::BarBarEqualsToken;
                        }
                        _ => {
                            self.pos += 2;
                            self.token = SyntaxKind::BarBarToken;
                        }
                    },
                    Some(b'=') => {
                        self.pos += 2;
                        self.token = SyntaxKind::BarEqualsToken;
                    }
                    _ => {
                        self.pos += 1;
                        self.token = SyntaxKind::BarToken;
                    }
                },
                b'}' => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseBraceToken;
                }
                b'~' => {
                    self.pos += 1;
                    self.token = SyntaxKind::TildeToken;
                }
                b'@' => {
                    self.pos += 1;
                    self.token = SyntaxKind::AtToken;
                }
                b'\\' => {
                    if let Some(c) = self.peek_unicode_escape()
                        && is_identifier_start(c)
                    {
                        self.scan_unicode_escape(true);
                        self.token_value = format!("{c}{}", self.scan_identifier_parts());
                        self.token = get_identifier_token(&self.token_value);
                    } else {
                        self.scan_invalid_character();
                    }
                }
                b'#' => {
                    if self.ascii_at(1) == Some(b'!') {
                        if self.pos == 0 {
                            self.pos += 2;
                            while let Some(c) = self.char()
                                && !is_line_break(c)
                            {
                                self.pos += c.len_utf8();
                            }
                            continue;
                        }
                        self.error_at(
                            Message::e18026_can_only_be_used_at_the_start_of_a_file(),
                            self.pos,
                            2,
                        );
                        self.pos += 1;
                        self.token = SyntaxKind::Unknown;
                        break;
                    }

                    if self.ascii_at(1) == Some(b'\\') {
                        self.pos += 1;
                        if let Some(c) = self.peek_unicode_escape()
                            && is_identifier_start(c)
                        {
                            self.scan_unicode_escape(true);
                            self.token_value = format!("#{c}{}", self.scan_identifier_parts());
                            self.token = SyntaxKind::PrivateIdentifier;
                            break;
                        }
                        self.pos -= 1;
                    }

                    if !self.scan_identifier(1) {
                        self.error_at(Message::e1127_invalid_character(), self.pos - 1, 1);
                        self.token_value = String::from("#");
                    }
                    self.token = SyntaxKind::PrivateIdentifier;
                }
                _ => {
                    if self.scan_identifier(0) {
                        self.token = get_identifier_token(&self.token_value);
                        break;
                    }

                    let c = self.char().unwrap();
                    if is_whitespace_single_line(c) {
                        self.pos += c.len_utf8();

                        // If we get here and it's not 0x0085 (nextLine), then we're handling non-ASCII whitespace.
                        // Handle skipTrivia like we do in the space case above.
                        if c == '\u{0085}' || self.skip_trivia {
                            continue;
                        }

                        while let Some(c) = self.char()
                            && is_whitespace_single_line(c)
                        {
                            self.pos += c.len_utf8();
                        }
                        self.token = SyntaxKind::WhitespaceTrivia;
                        break;
                    }

                    if is_line_break(c) {
                        self.token_flags.insert(TokenFlags::PrecedingLineBreak);
                        self.pos += c.len_utf8();
                        continue;
                    }

                    self.scan_invalid_character();
                }
            }
            break;
        }
        self.token
    }

    pub fn rescan_greater_than_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::GreaterThanToken {
            self.rescan_greater_than_token_inner();
        }
        self.token
    }

    pub fn rescan_less_than_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::LessThanLessThanToken {
            self.pos = self.token_start + 1;
            self.token = SyntaxKind::LessThanToken;
        }
        self.token
    }

    pub fn rescan_asterisk_equals_token(&mut self) -> SyntaxKind {
        assert_eq!(self.token, SyntaxKind::AsteriskEqualsToken);
        self.pos = self.token_start + 1;
        self.token = SyntaxKind::EqualsToken;
        self.token
    }

    pub fn rescan_template_token(&mut self, is_tagged_template: bool) -> SyntaxKind {
        self.pos = self.token_start;
        self.token = self.scan_template_and_set_token_value(!is_tagged_template);
        self.token
    }

    pub fn rescan_question_token(&mut self) -> SyntaxKind {
        assert_eq!(self.token, SyntaxKind::QuestionQuestionToken);
        self.pos = self.token_start + 1;
        self.token = SyntaxKind::QuestionToken;
        self.token
    }

    pub fn rescan_slash_token(&mut self, should_report_errors: bool) -> SyntaxKind {
        if matches!(self.token, SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken) {
            // Quickly get to the end of regex such that we know the flags
            let start_of_regexp_body = self.token_start + 1;
            let mut p = start_of_regexp_body;
            let mut in_escape = false;
            let mut named_capture_groups = false;
            // Although nested character classes are allowed in Unicode Sets mode,
            // an unescaped slash is nevertheless invalid even in a character class in any Unicode mode.
            // This is indicated by Section 12.9.5 Regular Expression Literals of the specification,
            // where nested character classes are not considered at all. (A `[` RegularExpressionClassChar
            // does nothing in a RegularExpressionClass, and a `]` always closes the class.)
            // Additionally, parsing nested character classes will misinterpret regexes like `/[[]/`
            // as unterminated, consuming characters beyond the slash. (This even applies to `/[[]/v`,
            // which should be parsed as a well-terminated regex with an incomplete character class.)
            // Thus we must not handle nested character classes in the first pass.
            let mut in_character_class = false;
            let text = self.text.as_bytes();
            loop {
                // If we reach the end of a file, or hit a newline, then this is an unterminated
                // regex. Report error and return what we have so far.
                if p >= self.end {
                    self.token_flags.insert(TokenFlags::Unterminated);
                    break;
                }

                let c = text[p] as char;
                if is_line_break(c) {
                    self.token_flags.insert(TokenFlags::Unterminated);
                    break;
                }

                if in_escape {
                    // Parsing an escape character;
                    // reset the flag and just advance to the next char.
                    in_escape = false;
                } else if c == '/' && !in_character_class {
                    // A slash within a character class is permissible,
                    // but in general it signals the end of the regexp literal.
                    break;
                } else if c == '[' {
                    in_character_class = true;
                } else if c == '\\' {
                    in_escape = true;
                } else if c == ']' {
                    in_character_class = false;
                } else if !in_character_class
                    && c == '('
                    && p + 1 < self.end
                    && text[p + 1] == b'?'
                    && p + 2 < self.end
                    && text[p + 2] == b'<'
                    && (p + 3 >= self.end || !matches!(text[p + 3], b'=' | b'!'))
                {
                    named_capture_groups = true;
                }
                p += 1;
            }

            let end_of_regexp_body = p;
            if self.token_flags.contains(TokenFlags::Unterminated) {
                // Search for the nearest unbalanced bracket for better recovery. Since the expression is
                // invalid anyways, we take nested square brackets into consideration for the best guess.
                p = start_of_regexp_body;
                in_escape = false;
                let mut character_class_depth = 0;
                let mut in_decimal_quantifier = false;
                let mut group_depth = 0;
                while p < end_of_regexp_body {
                    let c = text[p] as char;
                    if in_escape {
                        in_escape = false;
                    } else if c == '\\' {
                        in_escape = true;
                    } else if c == '[' {
                        character_class_depth += 1;
                    } else if c == ']' && character_class_depth > 0 {
                        character_class_depth -= 1;
                    } else if character_class_depth == 0 {
                        if c == '{' {
                            in_decimal_quantifier = true;
                        } else if c == '}' && in_decimal_quantifier {
                            in_decimal_quantifier = false;
                        } else if !in_decimal_quantifier {
                            if c == '(' {
                                group_depth += 1;
                            } else if c == ')' && group_depth > 0 {
                                group_depth -= 1;
                            } else if c == ')' || c == ']' || c == '}' {
                                // We encountered an unbalanced bracket outside a character class. Treat this position as the end of regex.
                                break;
                            }
                        }
                    }
                    p += 1;
                }

                // Whitespaces and semicolons at the end are not likely to be part of the regex
                while p > start_of_regexp_body {
                    let c = self.text[..p].chars().next_back().unwrap();
                    if is_whitespace_like(c) || c == ';' {
                        p -= c.len_utf8();
                    } else {
                        break;
                    }
                }

                self.error_at(
                    Message::e1161_unterminated_regular_expression_literal(),
                    self.token_start,
                    p - self.token_start,
                );
            } else {
                // Consume the slash character
                p += 1;
                let mut regexp_flags = RegexpFlags::empty();
                let chars = self.text[p..self.end].chars();
                for c in chars {
                    if !is_identifier_part(c) {
                        break;
                    }

                    if should_report_errors {
                        let flag = RegexpFlags::from_char(c);
                        if flag.is_empty() {
                            self.error_at(
                                Message::e1499_unknown_regular_expression_flag(),
                                p,
                                c.len_utf8(),
                            );
                        } else if regexp_flags.intersects(flag) {
                            self.error_at(
                                Message::e1500_duplicate_regular_expression_flag(),
                                p,
                                c.len_utf8(),
                            );
                        } else if (regexp_flags | flag).intersects(RegexpFlags::AnyUnicodeMode) {
                            self.error_at(Message::e1502_the_unicode_u_flag_and_the_unicode_sets_v_flag_cannot_be_set_simultaneously(), p, c.len_utf8());
                        } else {
                            regexp_flags.insert(flag);
                            self.check_regular_expression_flag_availability(flag, p, c.len_utf8());
                        }
                    }
                    p += c.len_utf8();
                }

                if should_report_errors {
                    self.pos = start_of_regexp_body;
                    let save_end = self.end;
                    let save_token_pos = self.token_start;
                    let save_token_flags = self.token_flags;
                    self.end = end_of_regexp_body;
                    let mut parser = RegexpParser {
                        scanner: self,
                        end: end_of_regexp_body,
                        regexp_flags,
                        any_unicode_mode: regexp_flags.intersects(RegexpFlags::AnyUnicodeMode),
                        unicode_sets_mode: regexp_flags.intersects(RegexpFlags::UnicodeSets),
                        annex_b: true,
                        named_capture_groups,
                        group_specifiers: FxHashMap::default(),
                    };
                    parser.run();
                    self.end = save_end;
                    self.pos = p;
                    self.token_start = save_token_pos;
                    self.token_flags = save_token_flags;
                } else {
                    self.pos = p;
                }
            }

            self.pos = p;
            self.token_value = self.text[self.token_start..self.pos].to_string();
            self.token = SyntaxKind::RegularExpressionLiteral;
        }
        self.token
    }

    fn check_regular_expression_flag_availability(
        &self,
        flags: RegexpFlags,
        pos: usize,
        length: usize,
    ) {
        if let Some(available_from) = flags.available_from()
            && self.language_version() < available_from
        {
            self.error_with_args(Message::e1501_this_regular_expression_flag_is_only_available_when_targeting_0_or_later(), pos, length,[
                format!("{available_from:?}")
            ]);
        }
    }

    fn rescan_greater_than_token_inner(&mut self) {
        self.pos = self.token_start + 1;
        match self.ascii() {
            Some(b'>') => match self.ascii_at(1) {
                Some(b'>') => {
                    if self.ascii_at(2) == Some(b'=') {
                        self.pos += 3;
                        self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken;
                    } else {
                        self.pos += 2;
                        self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanToken;
                    }
                }
                Some(b'=') => {
                    self.pos += 2;
                    self.token = SyntaxKind::GreaterThanGreaterThanEqualsToken;
                }
                _ => {
                    self.pos += 1;
                    self.token = SyntaxKind::GreaterThanGreaterThanToken;
                }
            },
            Some(b'=') => {
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanEqualsToken;
            }
            _ => {}
        }
    }

    fn scan_string(&mut self, jsx_attribute_string: bool) -> String {
        let quote = self.ascii().unwrap();
        if quote == b'\\' {
            self.token_flags.insert(TokenFlags::SingleQuote);
        }
        self.pos += 1;
        // Fast path for simple strings without escape sequences.
        match self.text.as_bytes()[self.pos..].iter().position(|c| *c == quote) {
            Some(0) => {
                self.pos += 1;
                return String::new();
            }
            Some(n) => {
                let s = &self.text[self.pos..self.pos + n];
                if jsx_attribute_string || !s.contains(['\\', '\r', '\n']) {
                    self.pos += n + 1;
                    return String::from(s);
                }
            }
            None => {}
        }

        let mut buf = String::new();
        let mut start = self.pos;
        loop {
            let Some(c) = self.ascii() else {
                buf.push_str(&self.text[start..self.pos]);
                self.token_flags.insert(TokenFlags::Unterminated);
                self.error(Message::e1002_unterminated_string_literal());
                break;
            };

            if c == quote {
                buf.push_str(&self.text[start..self.pos]);
                self.pos += 1;
                break;
            }

            if c == b'\\' && !jsx_attribute_string {
                buf.push_str(&self.text[start..self.pos]);
                buf.push_str(&self.scan_escape_sequence(
                    EscapeSequenceScanningFlags::String | EscapeSequenceScanningFlags::ReportErrors,
                ));
                start = self.pos;
                continue;
            }

            if matches!(c, b'\r' | b'\n') && !jsx_attribute_string {
                buf.push_str(&self.text[start..self.pos]);
                self.token_flags.insert(TokenFlags::Unterminated);
                self.error(Message::e1002_unterminated_string_literal());
                break;
            }

            self.pos += 1;
        }

        buf
    }

    fn scan_template_and_set_token_value(
        &mut self,
        should_emit_invalid_escape_error: bool,
    ) -> SyntaxKind {
        let started_with_backtick = self.ascii() == Some(b'`');
        self.pos += 1;
        let mut start = self.pos;

        let mut buf = String::new();
        let token = loop {
            self.scan_ascii_while(|c| !matches!(c, b'`' | b'$' | b'\\' | b'\r'));
            let c = self.ascii();

            if c.is_none() || c == Some(b'`') {
                buf.push_str(&self.text[start..self.pos]);
                if c.is_none() {
                    self.token_flags.insert(TokenFlags::Unterminated);
                    self.error(Message::e1160_unterminated_template_literal());
                } else {
                    self.pos += 1;
                }
                break if started_with_backtick {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            }

            if c == Some(b'$') && self.ascii_at(1) == Some(b'{') {
                buf.push_str(&self.text[start..self.pos]);
                self.pos += 2;
                break if started_with_backtick {
                    SyntaxKind::TemplateHead
                } else {
                    SyntaxKind::TemplateMiddle
                };
            }

            if c == Some(b'\\') {
                buf.push_str(&self.text[start..self.pos]);
                buf.push_str(&self.scan_escape_sequence(if should_emit_invalid_escape_error {
                    EscapeSequenceScanningFlags::String | EscapeSequenceScanningFlags::ReportErrors
                } else {
                    EscapeSequenceScanningFlags::String
                }));
                start = self.pos;
                continue;
            }

            // Speculated ECMAScript 6 Spec 11.8.6.1:
            // <CR><LF> and <CR> LineTerminatorSequences are normalized to <LF> for Template Values
            if c == Some(b'\r') {
                buf.push_str(&self.text[start..self.pos]);
                self.pos += 1;
                if self.ascii() == Some(b'\n') {
                    self.pos += 1;
                }
                buf.push('\n');
                start = self.pos;
                continue;
            }

            self.pos += 1;
        };
        self.token_value = buf;
        token
    }

    fn scan_escape_sequence(&mut self, flags: EscapeSequenceScanningFlags) -> String {
        let start = self.pos;
        self.pos += 1;
        let Some(c) = self.ascii() else {
            self.error(Message::e1126_unexpected_end_of_text());
            return String::new();
        };
        self.pos += 1;
        let mut fallthrough = false;
        if c == b'0' {
            // Although '0' preceding any digit is treated as LegacyOctalEscapeSequence,
            // '\08' should separately be interpreted as '\0' + '8'.
            if let Some(b'0'..=b'9') = self.ascii() {
                return String::from("\0");
            }
            // '\01', '\011'
            fallthrough = true;
        }
        if fallthrough || matches!(c, b'1'..=b'3') {
            // '\1', '\17', '\177'
            if let Some(b'0'..=b'7') = self.ascii() {
                self.pos += 1;
            }
            // '\17', '\177'
            fallthrough = true;
        }
        if fallthrough || matches!(c, b'4'..=b'7') {
            // '\4', '\47' but not '\477'
            if let Some(b'0'..=b'7') = self.ascii() {
                self.pos += 1;
            }
            // '\47'
            self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
            if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                let code =
                    u32::from_str_radix(&self.text[start + 1..self.pos], 8).unwrap_or_default();
                if flags.contains(EscapeSequenceScanningFlags::RegularExpression)
                    && !flags.contains(EscapeSequenceScanningFlags::AtomEscape)
                    && code != b'0' as u32
                {
                    self.error_with_args(Message::e1536_octal_escape_sequences_and_backreferences_are_not_allowed_in_a_character_class_if_this_was_intended_as_an_escape_sequence_use_the_syntax_0_instead(), start, self.pos - start, [format!("\\x{code:02}")]);
                } else {
                    self.error_with_args(
                        Message::e1487_octal_escape_sequences_are_not_allowed_use_the_syntax_0(),
                        start,
                        self.pos - start,
                        [format!("\\x{code:02}")],
                    );
                }
                return char::from_u32(code).unwrap_or_default().to_string();
            }
            return self.text[start..self.pos].to_string();
        }
        if let b'8' | b'9' = c {
            // the invalid '\8' and '\9'
            self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
            if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                if flags.contains(EscapeSequenceScanningFlags::RegularExpression)
                    && !flags.contains(EscapeSequenceScanningFlags::AtomEscape)
                {
                    self.error_at(
                        Message::e1537_decimal_escape_sequences_and_backreferences_are_not_allowed_in_a_character_class(),
                        start,
                        self.pos - start,
                    );
                } else {
                    self.error_with_args(
                        Message::e1488_escape_sequence_0_is_not_allowed(),
                        start,
                        self.pos - start,
                        [self.text[start..self.pos].to_string()],
                    );
                }
                return String::from(c as char);
            }
            return self.text[start..self.pos].to_string();
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
            b'u' => {
                // '\uDDDD' and '\u{DDDDDD}'
                let extended = self.ascii() == Some(b'{');
                self.pos -= 2;
                let codepoint = self.scan_unicode_escape(
                    flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors),
                );
                if extended {
                    if !flags.contains(EscapeSequenceScanningFlags::AllowExtendedUnicodeEscape) {
                        self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
                        if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                            self.error_at(Message::e1538_unicode_escape_sequences_are_only_available_when_the_unicode_u_flag_or_the_unicode_sets_v_flag_is_set(), start, self.pos - start);
                        }
                    }
                    let Some(codepoint) = codepoint else {
                        return self.text[start..self.pos].to_string();
                    };

                    // In string literals, a high surrogate \u{...} followed by a low
                    // surrogate escape forms a single code point, exactly as adjacent
                    // UTF-16 code units would in a JavaScript string.
                    if !flags.contains(EscapeSequenceScanningFlags::RegularExpression)
                        && is_high_surrogate(codepoint)
                        && let Some(combined) = self.scan_low_surrogate_escape(codepoint)
                    {
                        return String::from(combined);
                    }
                    return encode_js_string_char(codepoint);
                }
                let Some(codepoint) = codepoint else {
                    return self.text[start..self.pos].to_string();
                };
                if is_high_surrogate(codepoint) {
                    if !flags.contains(EscapeSequenceScanningFlags::RegularExpression) {
                        // Combine \uHigh followed by any low surrogate escape (\uLow or
                        // \u{Low}) into a single code point in string literals, matching
                        // how adjacent UTF-16 code units pair in a JavaScript string.
                        if let Some(combined) = self.scan_low_surrogate_escape(codepoint) {
                            return String::from(combined);
                        }
                    } else if flags.contains(EscapeSequenceScanningFlags::AnyUnicodeMode)
                        && self.ascii() == Some(b'\\')
                        && self.ascii_at(1) == Some(b'u')
                        && self.ascii_at(2) == Some(b'{')
                    {
                        // In regex AnyUnicodeMode, combine \uHigh\uLow so scanClassRanges
                        // can compare the pair numerically. In non-unicode regex mode they
                        // are separate atoms, and extended \u{...} escapes never combine.
                        let saved_pos = self.pos;
                        let low = self.scan_unicode_escape(
                            flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors),
                        );
                        if let Some(low) = low
                            && is_low_surrogate(low)
                        {
                            return String::from(surrogate_pair_to_codepoint(codepoint, low));
                        }
                        self.pos = saved_pos;
                    }
                }
                // Lone surrogate: encode as CESU-8 so it survives losslessly. In a
                // non-unicode regex this also lets scanClassRanges compare it numerically.
                encode_js_string_char(codepoint)
            }
            b'x' => {
                while self.pos < start + 4 {
                    if !self.ascii().is_some_and(|c| c.is_ascii_hexdigit()) {
                        self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
                        if flags.contains(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                            self.error(Message::e1125_hexadecimal_digit_expected());
                        }
                        return self.text[start..self.pos].to_string();
                    }
                    self.pos += 1;
                }
                self.token_flags.insert(TokenFlags::HexEscape);
                let value = u32::from_str_radix(&self.text[start + 2..self.pos], 16).unwrap();
                String::from(char::from_u32(value).unwrap())
            }
            b'\r' => {
                if self.ascii() == Some(b'\n') {
                    self.pos += 1;
                }
                String::new()
            }
            b'\n' => String::new(),
            _ => {
                // ch was read as a single byte; for multi-byte UTF-8 characters,
                // we need to decode the full rune and advance past all its bytes.
                let c = if c.is_ascii() {
                    c as char
                } else {
                    self.pos -= 1;
                    let c = self.char().unwrap();
                    self.pos += c.len_utf8();
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
                        Message::e1535_this_character_cannot_be_escaped_in_a_regular_expression(),
                        start,
                        self.pos - start,
                    );
                }
                String::from(c)
            }
        }
    }

    // scanLowSurrogateEscape attempts to consume a low-surrogate Unicode escape
    // (either '\uLow' or '\u{Low}') immediately following an already-scanned high
    // surrogate and combine them into a single supplementary code point. This
    // mirrors how adjacent UTF-16 code units form a surrogate pair in a JavaScript
    // string, regardless of which escape syntax produced each half. On success it
    // returns the combined code point and true; otherwise it restores the scanner
    // position and returns false.
    fn scan_low_surrogate_escape(&mut self, high: char) -> Option<char> {
        if self.ascii() != Some(b'\\') || self.ascii_at(1) != Some(b'u') {
            return None;
        }
        let saved_pos = self.pos;
        let saved_token_flags = self.token_flags;
        // Speculatively scan the escape with diagnostics suppressed: if it isn't a
        // low surrogate we rewind below, and the caller re-scans the same escape and
        // reports any error then, so reporting here would duplicate diagnostics.
        if let Some(low) = self.scan_unicode_escape(false)
            && is_low_surrogate(low)
        {
            return Some(surrogate_pair_to_codepoint(high, low));
        }
        self.pos = saved_pos;
        self.token_flags = saved_token_flags;
        None
    }

    fn scan_identifier(&mut self, prefix_len: usize) -> bool {
        let start = self.pos;
        self.pos += prefix_len;

        // Fast path for simple ASCII identifiers
        if let Some(c) = self.ascii()
            && (c.is_ascii_alphabetic() || c == b'_' || c == b'$')
        {
            self.pos += 1;
            self.scan_ascii_while(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$');
            if let Some(c) = self.ascii()
                && c.is_ascii()
                && c != b'\\'
            {
                self.token_value = self.text[start..self.pos].to_string();
                return true;
            }
            self.pos = start + prefix_len;
        }

        if let Some(c) = self.char()
            && is_identifier_start(c)
        {
            self.pos += c.len_utf8();
            while let Some(c) = self.char()
                && is_identifier_part(c)
            {
                self.pos += c.len_utf8();
            }
            self.token_value = self.text[start..self.pos].to_string();
            if c == '\\' {
                let rest = self.scan_identifier_parts();
                self.token_value.push_str(&rest);
            }
            return true;
        }

        false
    }

    fn scan_identifier_parts(&mut self) -> String {
        let mut out = String::new();
        let mut start = self.pos;

        while let Some(c) = self.char() {
            if is_identifier_part(c) {
                self.pos += c.len_utf8();
                continue;
            }

            if c == '\\'
                && let Some(escaped) = self.peek_unicode_escape()
                && is_identifier_part(escaped)
            {
                self.scan_unicode_escape(true);
                out.push_str(&self.text[start..self.pos]);
                out.push(escaped);
                start = self.pos;
                continue;
            }

            break;
        }
        out.push_str(&self.text[start..self.pos]);
        out
    }

    fn peek_unicode_escape(&mut self) -> Option<char> {
        if self.ascii_at(1) == Some(b'u') {
            let save_pos = self.pos;
            let save_token_flags = self.token_flags;
            let c = self.scan_unicode_escape(false);
            self.pos = save_pos;
            self.token_flags = save_token_flags;
            c
        } else {
            None
        }
    }

    fn scan_unicode_escape(&mut self, should_emit_invalid_escape_error: bool) -> Option<char> {
        // Known to be at \u
        self.pos += 2;
        let start = self.pos;
        let extended = self.ascii() == Some(b'{');
        let hex_digits = if extended {
            self.pos += 1;
            self.scan_hex_digits(1, true, false)
        } else {
            self.token_flags.insert(TokenFlags::UnicodeEscape);
            self.scan_hex_digits(4, false, false)
        };
        if hex_digits.is_empty() {
            self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
            if should_emit_invalid_escape_error {
                self.error(Message::e1125_hexadecimal_digit_expected());
            }
            return None;
        }
        let hex_value = u32::from_str_radix(&hex_digits, 16).ok();
        if extended {
            let mut is_invalid_extended_escape = false;
            if hex_value.is_none() || hex_value.is_some_and(|c| c > 0x10FFFF) {
                if should_emit_invalid_escape_error {
                    self.error_at(
                        Message::e1198_an_extended_unicode_escape_value_must_be_between_0x0_and_0x10ffff_inclusive(),
                        start + 1,
                        self.pos - start - 1,
                    );
                }
                is_invalid_extended_escape = true;
            }
            if self.pos >= self.end {
                if should_emit_invalid_escape_error {
                    self.error(Message::e1126_unexpected_end_of_text());
                }
                is_invalid_extended_escape = true;
            } else if self.ascii() == Some(b'}') {
                self.pos += 1;
            } else {
                if should_emit_invalid_escape_error {
                    self.error(Message::e1199_unterminated_unicode_escape_sequence());
                }
                is_invalid_extended_escape = true;
            }
            if is_invalid_extended_escape {
                self.token_flags.insert(TokenFlags::ContainsInvalidEscape);
                return None;
            }
            self.token_flags.insert(TokenFlags::ExtendedUnicodeEscape);
        }
        hex_value.and_then(char::from_u32)
    }

    fn scan_invalid_character(&mut self) {
        let c = self.char().unwrap();
        self.error_at(Message::e1127_invalid_character(), self.pos, c.len_utf8());
        self.pos += c.len_utf8();
        self.token = SyntaxKind::Unknown;
    }

    fn scan_number(&mut self) -> SyntaxKind {
        let mut start = self.pos;
        let fixed_part: String;
        if self.ascii() == Some(b'0') {
            self.pos += 1;
            if self.ascii() == Some(b'_') {
                self.token_flags
                    .insert(TokenFlags::ContainsSeparator | TokenFlags::ContainsInvalidSeparator);
                self.error_at(
                    Message::e6188_numeric_separators_are_not_allowed_here(),
                    self.pos,
                    1,
                );
                self.pos = start;
                fixed_part = self.scan_number_fragment();
            } else {
                let (digits, is_octal) = self.scan_digits();
                if digits.is_empty() {
                    fixed_part = String::from('0');
                } else if !is_octal {
                    self.token_flags.insert(TokenFlags::ContainsLeadingZero);
                    fixed_part = digits;
                } else {
                    let value = u64::from_str_radix(&digits, 8).unwrap_or_default();
                    self.token_value = value.to_string();
                    self.token_flags.insert(TokenFlags::Octal);
                    let with_minus = self.token == SyntaxKind::MinusToken;
                    let literal = format!("{}0o{:o}", if with_minus { "-" } else { "" }, value);
                    if with_minus {
                        start -= 1;
                    }
                    self.error_with_args(
                        Message::e1121_octal_literals_are_not_allowed_use_the_syntax_0(),
                        start,
                        self.pos - start,
                        [literal],
                    );
                    return SyntaxKind::NumericLiteral;
                }
            }
        } else {
            fixed_part = self.scan_number_fragment();
        }
        let fixed_part_end = self.pos;
        let mut fractional_part = String::new();
        let mut exponent_preamble = String::new();
        let mut exponent_part = String::new();

        if self.ascii() == Some(b'.') {
            self.pos += 1;
            fractional_part = self.scan_number_fragment();
        }
        let mut end = self.pos;
        if let Some(b'e' | b'E') = self.ascii() {
            self.pos += 1;
            self.token_flags.insert(TokenFlags::Scientific);
            if let Some(b'+' | b'-') = self.ascii() {
                self.pos += 1;
            }
            let start_numeric_part = self.pos;
            exponent_part = self.scan_number_fragment();
            if exponent_part.is_empty() {
                self.error(Message::e1124_digit_expected());
            } else {
                exponent_preamble = self.text[end..start_numeric_part].to_string();
                end = self.pos;
            }
        }
        if self.token_flags.contains(TokenFlags::ContainsSeparator) {
            self.token_value = fixed_part;
            if !fractional_part.is_empty() {
                self.token_value.push('.');
                self.token_value.push_str(&fractional_part);
            }
            if !exponent_part.is_empty() {
                self.token_value.push_str(&exponent_preamble);
                self.token_value.push_str(&exponent_part);
            }
        } else {
            self.token_value = self.text[start..end].to_string();
        }
        if self.token_flags.contains(TokenFlags::ContainsLeadingZero) {
            self.error_at(
                Message::e1489_decimals_with_leading_zeros_are_not_allowed(),
                start,
                self.pos - start,
            );
            self.token_value = Number::from_str(&self.token_value).to_string();
            return SyntaxKind::NumericLiteral;
        }
        let result = if fixed_part_end == self.pos {
            self.scan_big_int_suffix()
        } else {
            self.token_value = Number::from_str(&self.token_value).to_string();
            SyntaxKind::NumericLiteral
        };
        if let Some(c) = self.char()
            && is_identifier_start(c)
        {
            let id_start = self.pos;
            let id = self.scan_identifier_parts();
            if result != SyntaxKind::BigIntLiteral
                && id.len() == 1
                && self.text.as_bytes()[id_start] == b'n'
            {
                if self.token_flags.contains(TokenFlags::Scientific) {
                    self.error_at(
                        Message::e1352_a_bigint_literal_cannot_use_exponential_notation(),
                        start,
                        self.pos - start,
                    );
                    return result;
                }
                if fixed_part_end < id_start {
                    self.error_at(
                        Message::e1353_a_bigint_literal_must_be_an_integer(),
                        start,
                        self.pos - start,
                    );
                    return result;
                }
            }
            self.error_at(
                Message::e1351_an_identifier_or_keyword_cannot_immediately_follow_a_numeric_literal(
                ),
                id_start,
                self.pos - id_start,
            );
            self.pos = id_start;
        }
        result
    }

    fn scan_number_fragment(&mut self) -> String {
        let mut start = self.pos;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        let mut result = String::new();
        loop {
            let before = self.pos;
            self.scan_ascii_while(|c| c.is_ascii_digit());
            if self.pos > before {
                allow_separator = true;
                is_previous_token_separator = false;
            }

            if self.ascii() == Some(b'_') {
                self.token_flags.insert(TokenFlags::ContainsSeparator);
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                    result.push_str(&self.text[start..self.pos]);
                } else {
                    self.token_flags.insert(TokenFlags::ContainsInvalidSeparator);
                    if is_previous_token_separator {
                        self.error_at(
                            Message::e6189_multiple_consecutive_numeric_separators_are_not_permitted(),
                            self.pos,
                            1,
                        );
                    } else {
                        self.error_at(
                            Message::e6188_numeric_separators_are_not_allowed_here(),
                            self.pos,
                            1,
                        );
                    }
                }
                self.pos += 1;
                start = self.pos;
                continue;
            }
            break;
        }

        if is_previous_token_separator {
            self.token_flags.insert(TokenFlags::ContainsInvalidSeparator);
            self.error_at(
                Message::e6188_numeric_separators_are_not_allowed_here(),
                self.pos - 1,
                1,
            );
        }
        result.push_str(&self.text[start..self.pos]);
        result
    }

    fn scan_digits(&mut self) -> (String, bool) {
        let start = self.pos;
        let mut is_octal = true;
        while let Some(c) = self.ascii()
            && c.is_ascii_digit()
        {
            if c > b'7' {
                is_octal = false;
            }
            self.pos += 1;
        }
        (self.text[start..self.pos].to_string(), is_octal)
    }

    fn scan_hex_digits(
        &mut self,
        min_count: usize,
        scan_as_many_as_possible: bool,
        can_have_separators: bool,
    ) -> String {
        let mut digit_count = 0;
        let start = self.pos;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        while (digit_count < min_count || scan_as_many_as_possible)
            && let Some(c) = self.ascii()
        {
            if c.is_ascii_hexdigit() {
                allow_separator = can_have_separators;
                is_previous_token_separator = false;
                digit_count += 1;
            } else if can_have_separators && c == b'_' {
                self.token_flags.insert(TokenFlags::ContainsSeparator);
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                } else if is_previous_token_separator {
                    self.error_at(
                        Message::e6189_multiple_consecutive_numeric_separators_are_not_permitted(),
                        self.pos,
                        1,
                    );
                } else {
                    self.error_at(
                        Message::e6188_numeric_separators_are_not_allowed_here(),
                        self.pos,
                        1,
                    );
                }
            } else {
                break;
            }
            self.pos += 1;
        }
        if is_previous_token_separator {
            self.error_at(
                Message::e6188_numeric_separators_are_not_allowed_here(),
                self.pos - 1,
                1,
            );
        }
        if digit_count < min_count {
            return String::new();
        }
        let mut digits = self.text[start..self.pos].to_string();
        if self.token_flags.contains(TokenFlags::ContainsSeparator) {
            digits = digits.replace('_', "");
        }
        digits.make_ascii_lowercase(); // standardize hex literals to lowercase
        digits
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
                self.token_flags.insert(TokenFlags::ContainsSeparator);
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                } else if is_previous_token_separator {
                    self.error_at(
                        Message::e6189_multiple_consecutive_numeric_separators_are_not_permitted(),
                        self.pos,
                        1,
                    );
                } else {
                    self.error_at(
                        Message::e6188_numeric_separators_are_not_allowed_here(),
                        self.pos,
                        1,
                    );
                }
            } else {
                break;
            }
            self.pos += 1;
        }
        if is_previous_token_separator {
            self.error_at(
                Message::e6188_numeric_separators_are_not_allowed_here(),
                self.pos - 1,
                1,
            );
        }
        out
    }

    fn scan_big_int_suffix(&mut self) -> SyntaxKind {
        if self.ascii() == Some(b'n') {
            self.token_value.push('n');
            if self.token_flags.contains(TokenFlags::OctalSpecifier) {
                self.token_value = number::parse_pseudo_big_int(&self.token_value);
                self.token_value.push('n');
            }
            self.pos += 1;
            return SyntaxKind::BigIntLiteral;
        }
        self.token_value = Number::from_str(&self.token_value).to_string();
        SyntaxKind::NumericLiteral
    }

    // scanJSDocCommentForTags scans a JSDoc comment for @deprecated, @see, and @link tags,
    // setting the appropriate token flags. Called during scanning when a JSDoc comment is detected.
    fn scan_jsdoc_comment_for_tags(&mut self, start: usize, end: usize) {
        let mut text = &self.text[start..end];
        loop {
            let Some((_, rest)) = text.split_once('@') else {
                return;
            };
            text = rest;
            if !self.token_flags.contains(TokenFlags::PrecedingJSDocWithDeprecated)
                && has_jsdoc_tag(text, &["deprecated"])
            {
                self.token_flags.insert(TokenFlags::PrecedingJSDocWithDeprecated);
            }

            if !self.token_flags.contains(TokenFlags::PrecedingJSDocWithSeeOrLink)
                && has_jsdoc_tag(text, &["see", "link", "linkcode", "linkplain"])
            {
                self.token_flags.insert(TokenFlags::PrecedingJSDocWithSeeOrLink);
            }

            if self.token_flags.contains(
                TokenFlags::PrecedingJSDocWithDeprecated | TokenFlags::PrecedingJSDocWithSeeOrLink,
            ) {
                return;
            }
        }
    }

    fn process_comment_directive(&mut self, start: usize, end: usize, multiline: bool) {
        // Skip starting slashes and whitespace
        let mut pos = start;
        let text = self.text.as_bytes();
        if multiline {
            // Skip whitespace
            while pos < end && matches!(text[pos], b' ' | b'\t') {
                pos += 1;
            }
            // Skip combinations of / and *
            while pos < end && matches!(text[pos], b'/' | b'*') {
                pos += 1;
            }
        } else {
            // Skip opening //
            pos += 2;
            // Skip another / if present
            while pos < end && text[pos] == b'/' {
                pos += 1;
            }
        }
        // Skip whitespace
        while pos < end && matches!(text[pos], b' ' | b'\t') {
            pos += 1;
        }
        // Directive must start with '@'
        if !(pos < end && text[pos] == b'@') {
            return;
        }
        pos += 1;
        let kind = if text[pos..].starts_with(b"ts-expect-error") {
            CommentDirectiveKind::ExpectError
        } else if text[pos..].starts_with(b"ts-ignore") {
            CommentDirectiveKind::Ignore
        } else {
            return;
        };
        self.comment_directives.push(CommentDirective { loc: TextRange::new(start, end), kind });
    }

    fn ascii(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn ascii_at(&self, offset: usize) -> Option<u8> {
        self.text.as_bytes().get(self.pos + offset).copied()
    }

    fn char(&self) -> Option<char> {
        self.text.get(self.pos..).and_then(|s| s.chars().next())
    }

    fn scan_ascii_while(&mut self, predicate: impl Fn(u8) -> bool) {
        for i in self.pos..self.end {
            let b = self.text.as_bytes()[i];
            if b.is_ascii() && predicate(b) {
                self.pos = i + 1;
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

    fn error(&self, message: &'static Message) {
        self.error_at(message, self.pos, 0);
    }

    fn error_at(&self, message: &'static Message, pos: usize, length: usize) {
        self.error_with_args(message, pos, length, [])
    }

    fn error_with_args(
        &self,
        message: &'static Message,
        pos: usize,
        length: usize,
        args: impl IntoIterator<Item = String>,
    ) {
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.report(message, TextRange::new(pos, pos + length), args);
        }
    }

    pub fn skip_trivia(text: &str, pos: usize) -> usize {
        Self::skip_trivia_ex(text, pos, SkipTriviaOptions::default())
    }

    pub fn skip_trivia_ex(text: &str, mut pos: usize, options: SkipTriviaOptions) -> usize {
        fn char(text: &str, i: usize) -> Option<char> {
            text.get(i..).and_then(|c| c.chars().next())
        }

        let mut can_consume_star = false;
        // Keep in sync with couldStartTrivia
        while let Some(c) = char(text, pos) {
            match c {
                '\r' | '\n' => {
                    if c == '\r'
                        && let Some('\n') = char(text, pos + 1)
                    {
                        pos += 1;
                    }
                    pos += 1;
                    if options.stop_after_line_break {
                        break;
                    }
                    can_consume_star = options.in_jsdoc;
                    continue;
                }
                '\t' | '\x0B' | '\x0C' | ' ' => {
                    pos += 1;
                    continue;
                }
                '/' => {
                    if options.stop_at_comments {
                        break;
                    }
                    match char(text, pos + 1) {
                        Some('/') => {
                            pos += 2;
                            while let Some(c) = char(text, pos) {
                                if is_line_break(c) {
                                    break;
                                }
                                pos += c.len_utf8();
                            }
                            can_consume_star = false;
                            continue;
                        }
                        Some('*') => {
                            pos += 2;
                            while let Some(c) = char(text, pos) {
                                if c == '*'
                                    && let Some('/') = char(text, pos + 1)
                                {
                                    pos += 2;
                                    break;
                                }
                                pos += c.len_utf8();
                            }
                            can_consume_star = false;
                            continue;
                        }
                        _ => (),
                    }
                }
                '<' | '|' | '=' | '>' => {
                    if is_conflict_marker_trivia(text, pos) {
                        pos = scan_conflict_marker_trivia(text, pos, None);
                        can_consume_star = false;
                        continue;
                    }
                }
                '#' => {
                    if pos == 0 && is_shebang_trivia(text, pos) {
                        pos = scan_shebang_trivia(text, pos);
                        can_consume_star = false;
                        continue;
                    }
                }
                '*' => {
                    if can_consume_star {
                        pos += 1;
                        can_consume_star = false;
                        continue;
                    }
                }
                _ => {
                    if !c.is_ascii() && is_whitespace_like(c) {
                        pos += c.len_utf8();
                        continue;
                    }
                }
            }
            break;
        }
        pos
    }

    pub fn get_text_of_node_from_source_text(
        mut text: &str,
        node: &Node,
        include_trivia: bool,
    ) -> String {
        if node.is_missing() {
            return String::new();
        }

        let mut pos = node.loc.pos as usize;
        if !include_trivia {
            pos = Self::skip_trivia(text, pos);
        }
        text = &text[pos..node.loc.end as usize];
        if node.flags.contains(NodeFlags::ReparserTransformedLiteral) {
            // This is similar to `getLiteralTextOfNode` in the printer, but without the context of an `emitContext` to provide overrides
            if node.kind == SyntaxKind::StringLiteral {
                return if node
                    .data_ref::<StringLiteral>()
                    .token_flags
                    .contains(TokenFlags::SingleQuote)
                {
                    format!("'{text}'")
                } else {
                    format!("\"{text}\"")
                };
            } else if node.kind == SyntaxKind::Identifier {
                return node.data_ref::<Identifier>().text.clone();
            }

            // Only the above node kinds are currently transformed into one another by the reparser, requiring the textual remapping.
            // (Any reamppings done by emit transforms are handled by `getLiteralTextOfNode` in the printer)
            // Fail on any other kinds.
            panic!("Unexpected reparser-transformed node kind: {:?}", node.kind);
        }

        // if (isJSDocTypeExpressionOrChild(node)) {
        //     // strip space + asterisk at line start
        //     text = text.split(/\r\n|\n|\r/).map(line => line.replace(/^\s*\*/, "").trimStart()).join("\n");
        // }
        String::from(text)
    }

    pub fn get_leading_comment_ranges(
        text: &str,
        pos: usize,
    ) -> impl Iterator<Item = CommentRange> {
        Self::iterate_comment_range(text, pos, false)
    }

    pub fn get_trailing_comment_ranges(
        text: &str,
        pos: usize,
    ) -> impl Iterator<Item = CommentRange> {
        Self::iterate_comment_range(text, pos, true)
    }

    pub fn is_jsdoc_like_text(text: &str) -> bool {
        matches!(text.as_bytes(), [_, b'*', b'*', c, ..] if *c != b'/')
    }

    fn iterate_comment_range(
        text: &str,
        mut pos: usize,
        trailing: bool,
    ) -> impl Iterator<Item = CommentRange> {
        let mut pending: Option<CommentRange> = None;
        let mut collecting = trailing;
        if pos == 0 {
            collecting = true;
            if is_shebang_trivia(text, pos) {
                pos = scan_shebang_trivia(text, pos);
            }
        }
        let mut done = false;
        std::iter::from_fn(move || {
            if done {
                return None;
            }

            while let Some(c) = text[pos..].chars().next() {
                match c {
                    '\r' | '\n' => {
                        if c == '\r' {
                            if pos + 1 < text.len() && text[pos + 1..].chars().next() == Some('\n')
                            {
                                pos += 1;
                            }
                        }
                        pos += 1;
                        if trailing {
                            break;
                        }
                        collecting = true;
                        if let Some(pending) = &mut pending {
                            pending.has_trailing_new_line = true;
                        }
                    }
                    '\t' | '\x0B' | '\x0C' | ' ' => {
                        pos += 1;
                    }
                    '/' => {
                        let mut next_char = None;
                        if pos + 1 < text.len() {
                            next_char = text[pos + 1..].chars().next();
                        }
                        let mut has_trailing_new_line = false;
                        if let Some(next_char @ ('/' | '*')) = next_char {
                            let kind = if next_char == '/' {
                                SyntaxKind::SingleLineCommentTrivia
                            } else {
                                SyntaxKind::MultiLineCommentTrivia
                            };
                            let start_pos = pos;
                            pos += 2;
                            if next_char == '/' {
                                while pos < text.len() {
                                    let c = text[pos..].chars().next().unwrap();
                                    if is_line_break(c) {
                                        has_trailing_new_line = true;
                                        break;
                                    }
                                    pos += c.len_utf8();
                                }
                            } else {
                                if let Some(i) = text[pos..].find("*/") {
                                    pos += i + 2;
                                } else {
                                    pos = text.len();
                                }
                            }
                            if collecting {
                                let range = pending.replace(CommentRange {
                                    range: TextRange::new(start_pos, pos),
                                    kind,
                                    has_trailing_new_line,
                                });
                                if range.is_some() {
                                    return range;
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    _ => {
                        if !c.is_ascii() && is_whitespace_like(c) {
                            if let Some(pending) = &mut pending
                                && is_line_break(c)
                            {
                                pending.has_trailing_new_line = true;
                            }
                            pos += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }

            done = true;
            pending.take()
        })
    }
}

fn scan_shebang_trivia(text: &str, mut pos: usize) -> usize {
    pos += 2;
    for c in text[pos..].chars() {
        if is_line_break(c) {
            break;
        }
        pos += c.len_utf8();
    }
    pos
}

fn is_shebang_trivia(text: &str, pos: usize) -> bool {
    assert_eq!(pos, 0, "Shebang check must be at the start of file");
    text.starts_with("#!")
}

fn get_identifier_token(s: &str) -> SyntaxKind {
    if let 2..=12 = s.len()
        && let b'a'..=b'z' = s.as_bytes()[0]
        && let Some(keyword) = text_to_keyword(s)
    {
        return keyword;
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

// All conflict markers consist of the same character repeated seven times.  If it is
// a <<<<<<< or >>>>>>> marker then it is also followed by a space.
const MERGE_CONFLICT_MARKER_LENGTH: usize = "<<<<<<<".len();

fn is_conflict_marker_trivia(text: &str, pos: usize) -> bool {
    // Fast reject: a conflict marker is the same byte repeated seven times. If the
    // second byte differs (the overwhelmingly common case for `<`, `>`, `=`, `|`
    // tokens), it cannot be a marker, so skip the line-start check entirely.
    let b = text.as_bytes();
    if b.get(pos) != b.get(pos + 1) {
        return false;
    }

    // Conflict markers must be at the start of a line.
    let mut at_line_start = pos == 0 || is_line_break(b[pos - 1] as char);
    if !at_line_start && pos >= 2 {
        let prev = text[..pos - 2].chars().next_back().unwrap_or_default();
        at_line_start = is_line_break(prev);
    }

    if at_line_start {
        let first = b[pos];
        if pos + MERGE_CONFLICT_MARKER_LENGTH < text.len() {
            if !b.iter().take(MERGE_CONFLICT_MARKER_LENGTH).all(|&c| c == first) {
                return false;
            }

            return first == b'=' || b[pos + MERGE_CONFLICT_MARKER_LENGTH] == b' ';
        }
    }
    false
}

fn scan_conflict_marker_trivia(
    text: &str,
    mut pos: usize,
    diagnostics: Option<&Diagnostics>,
) -> usize {
    if let Some(diagnostics) = diagnostics {
        diagnostics.report(
            Message::e1185_merge_conflict_marker_encountered(),
            TextRange::new(pos, pos + MERGE_CONFLICT_MARKER_LENGTH),
            None,
        );
    }

    let mut chars = text[pos..].chars();
    let first = chars.next();
    match first {
        Some('<' | '>') => {
            pos += 1;
            while let Some(c) = chars.next()
                && !is_line_break(c)
            {
                pos += c.len_utf8();
            }
        }
        Some(first @ ('|' | '=')) => {
            // Consume everything from the start of a ||||||| or ======= marker to the start
            // of the next ======= or >>>>>>> marker.
            pos += 1;
            for c in chars {
                if matches!(c, '=' | '>') && c != first && is_conflict_marker_trivia(text, pos) {
                    break;
                }
                pos += first.len_utf8();
            }
        }
        _ => unreachable!(),
    }
    pos
}

fn is_whitespace_like(c: char) -> bool {
    is_whitespace_single_line(c) || is_line_break(c)
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

// SurrogateLowStart is the boundary between the high and low halves of the
// UTF-16 surrogate range. unicode/utf16 only exposes IsSurrogate for the
// whole range, so this split point is defined here to distinguish the two.
const SURROGATE_LOW_START: u32 = 0xDC00;
// 0xd800-0xdc00 encodes the high 10 bits of a pair.
// 0xdc00-0xe000 encodes the low 10 bits of a pair.
// the value is those 20 bits plus 0x10000.
const SURR_1: u32 = 0xd800;
const SURR_2: u32 = 0xdc00;
const SURR_3: u32 = 0xe000;
const SURR_SELF: u32 = 0x10000;

// A lone surrogate (U+D800–U+DFFF) cannot be represented in valid UTF-8, so
// EncodeJSStringRune stores it as the 3-byte CESU-8/WTF-8 sentinel that UTF-8
// would use for that code point if surrogates were encodable. unicode/utf8
// and unicode/utf16 deliberately refuse to encode or decode surrogates, so
// the byte math is spelled out here.
//
// Byte layout for a code point cp in U+D000–U+DFFF (lead nibble 0xD):
//   byte0 = 0xE0 | (cp >> 12)          == 0xED
//   byte1 = 0x80 | ((cp >> 6) & 0x3F)
//   byte2 = 0x80 | (cp & 0x3F)
const SURROGATE_UTF8_LEAD: u8 = 0xED; // byte0, shared by the whole U+D000–U+DFFF block
const SURROGATE_UTF8_LEAD_BITS: u16 = 0xD000; // (surrogateUTF8Lead & 0x0F) << 12, byte0's decoded contribution
const UTF8_CONT_MARKER: u16 = 0x80; // continuation byte marker / min value (10xxxxxx)
const UTF8_CONT_MAX: u16 = 0xBF; // continuation byte max value
const UTF8_CONT_MASK: u16 = 0x3F; // data bits carried by a continuation byte

fn is_high_surrogate(c: char) -> bool {
    is_surrogate(c) && (c as u32) < SURROGATE_LOW_START
}

fn is_low_surrogate(c: char) -> bool {
    is_surrogate(c) && (c as u32) >= SURROGATE_LOW_START
}

fn is_surrogate(c: char) -> bool {
    let c = c as u32;
    (SURR_1..SURR_3).contains(&c)
}

fn surrogate_pair_to_codepoint(high: char, low: char) -> char {
    char::decode_utf16([high as u16, low as u16]).next().unwrap().unwrap()
}

fn encode_js_string_char(c: char) -> String {
    if is_surrogate(c) {
        let c = c as u16;
        return String::from_utf8(vec![
            SURROGATE_UTF8_LEAD,
            (UTF8_CONT_MARKER | (c >> 6) & UTF8_CONT_MASK) as u8,
            (UTF8_CONT_MARKER | (c & UTF8_CONT_MASK)) as u8,
        ])
        .unwrap();
    }

    String::from(c)
}

fn has_jsdoc_tag(text: &str, tags: &[&str]) -> bool {
    for tag in tags {
        if !text.starts_with(tag) {
            continue;
        }
        if text.len() == tag.len() {
            return true;
        }
        let c = text.as_bytes()[tag.len()];
        if let ' ' | '\t' | '\n' | '\r' | '}' | '*' = c as char {
            return true;
        }
    }
    false
}

impl RegexpFlags {
    fn from_char(c: char) -> Self {
        match c {
            'd' => Self::HasIndices,
            'g' => Self::Global,
            'i' => Self::IgnoreCase,
            'm' => Self::Multiline,
            's' => Self::DotAll,
            'u' => Self::Unicode,
            'v' => Self::UnicodeSets,
            'y' => Self::Sticky,
            _ => Self::empty(),
        }
    }

    fn available_from(self) -> Option<ScriptTarget> {
        Some(match self {
            Self::HasIndices => ScriptTarget::ES2022,
            Self::DotAll => ScriptTarget::ES2018,
            Self::UnicodeSets => ScriptTarget::ES2024,
            _ => return None,
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SkipTriviaOptions {
    stop_after_line_break: bool,
    stop_at_comments: bool,
    in_jsdoc: bool,
}
