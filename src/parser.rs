use crate::{
    ast::{NodeFlags, ParsingContext, ScriptKind},
    diagnostics,
    scanner::{LanguageVariant, Scanner},
    syntax::SyntaxKind,
};

pub struct Parser {
    scanner: Scanner,
    script_kind: ScriptKind,
    language_variant: LanguageVariant,

    token: SyntaxKind,
    source_flags: NodeFlags,
    context_flags: NodeFlags,
    parsing_context: ParsingContext,
    statement_has_await_identifier: bool,
    has_deprecated_tag: bool,
    has_parse_error: bool,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            scanner: Scanner::new(),
            script_kind: ScriptKind::Unknown,
            language_variant: LanguageVariant::Standard,

            token: SyntaxKind::Unknown,
            context_flags: NodeFlags::empty(),
            source_flags: NodeFlags::empty(),
            parsing_context: ParsingContext::empty(),
            statement_has_await_identifier: false,
            has_deprecated_tag: false,
            has_parse_error: false,
        }
    }

    pub fn parse(&mut self, contents: String, script_kind: ScriptKind) {
        self.init(contents, script_kind);
        self.next_token();
        self.parse_worker()
    }

    fn parse_worker(&self) {
        todo!()
    }

    fn init(&mut self, contents: String, script_kind: ScriptKind) {
        assert_ne!(script_kind, ScriptKind::Unknown);

        self.scanner = Scanner::new();
        self.scanner.set_text(contents);
        self.script_kind = script_kind;
        self.language_variant = script_kind.language_variant();
        match script_kind {
            ScriptKind::JS | ScriptKind::JSX => {
                self.context_flags.insert(NodeFlags::JavaScriptFile);
            }
            ScriptKind::JSON => {
                self.context_flags
                    .insert(NodeFlags::JavaScriptFile | NodeFlags::JsonFile);
            }
            _ => {
                self.context_flags = NodeFlags::empty();
            }
        }
        self.scanner.set_on_error(scan_error);
    }

    fn next_token(&mut self) -> SyntaxKind {
        // if the keyword had an escape
        if self.token.is_keyword()
            && (self.scanner.has_unicode_escape() || self.scanner.has_extended_unicode_escape())
        {
            todo!("parse error at current token");
        }
        self.token = self.scanner.scan();
        self.token
    }
}

fn scan_error(message: &'static diagnostics::Message, pos: usize, len: usize, args: &[String]) {}
