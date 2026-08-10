use rustc_hash::FxHashSet;

use crate::{
    ast::*,
    diagnostics::{DiagnosticId, Diagnostics, Message},
    flags::{JSDocScannerInfo, ModifierFlags, NodeFlags, ParseFlags, ParsingContext, TokenFlags},
    options::{LanguageVariant, ScriptKind},
    scanner::{Scanner, ScannerState},
    syntax::{OperatorPrecedence, SyntaxKind, TextPos, TextRange, token_to_text},
};

struct ParserState {
    scanner_state: ScannerState,
    context_flags: NodeFlags,
    diagnostics_len: usize,
    js_diagnostics_len: usize,
    jsdoc_infos_len: usize,
    reparsed_clones_len: usize,
    statement_has_await_identifier: bool,
    has_parse_error: bool,
}

pub struct SourceFileParseOptions {
    pub file_name: String,
}

pub struct Parser {
    options: SourceFileParseOptions,
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
    diagnostics: Diagnostics,
    js_diagnostics: Diagnostics,
    jsdoc_diagnostics: Diagnostics,

    jsdoc_infos: Vec<JSDocInfo>,
    reparsed_clones: Vec<NodeId>,
    reparse_list: Vec<NodeId>,
    possible_await_spans: Vec<usize>,
    jsdoc_comment_ranges_space: Vec<CommentRange>,
    nodes: NodeFactory,
    current_parent: Option<NodeId>,
    not_parenthesized_arrow: FxHashSet<usize>,

    identifier_count: usize,
}

impl Parser {
    pub fn new(options: SourceFileParseOptions) -> Self {
        Self {
            options,
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
            diagnostics: Diagnostics::new(),
            js_diagnostics: Diagnostics::new(),
            jsdoc_diagnostics: Diagnostics::new(),
            jsdoc_infos: Vec::new(),
            reparsed_clones: Vec::new(),
            reparse_list: Vec::new(),
            possible_await_spans: Vec::new(),
            jsdoc_comment_ranges_space: Vec::new(),
            nodes: NodeFactory::new(),
            current_parent: None,
            not_parenthesized_arrow: FxHashSet::default(),
            identifier_count: 0,
        }
    }

    pub fn parse_source_file(
        mut self,
        contents: String,
        script_kind: ScriptKind,
    ) -> (NodeId, NodeFactory) {
        self.init(contents, script_kind);
        self.next_token();
        let source_file = self.parse_source_file_worker();
        (source_file, self.nodes)
    }

    fn parse_source_file_worker(&mut self) -> NodeId {
        let is_declaration_file = self.options.file_name.ends_with(".d.ts");
        let pos = self.node_pos();
        let mut statements =
            self.parse_list_index(ParsingContext::SourceElements, Self::parse_top_level_statement);
        let end = self.node_pos();
        let end_jsdoc = self.jsdoc_scanner_info();
        let eof = self.parse_token_node();
        self.with_jsdoc(eof, end_jsdoc);
        if !self.nodes.is(eof, SyntaxKind::EndOfFile) {
            panic!("Expected end of file token from scanner.");
        }
        if !self.reparse_list.is_empty() {
            statements.extend(std::mem::take(&mut self.reparse_list));
        }
        let node = self.nodes.create(
            SyntaxKind::SourceFile,
            SourceFile {
                statements: NodeList { loc: TextRange::new(pos, end), nodes: statements },
                source_text: self.scanner.text.clone(),
                eof_token: eof,
                comment_directives: self.scanner.comment_directives(),
                is_declaration_file,
            },
        );
        self.finish_node(node, pos)
    }

    fn parse_list(
        &mut self,
        context: ParsingContext,
        mut parse_element: impl FnMut(&mut Parser) -> NodeId,
    ) -> NodeList {
        let pos = self.node_pos();
        let nodes = self.parse_list_index(context, |parser, _index| parse_element(parser));
        NodeList { loc: TextRange::new(pos, self.node_pos()), nodes }
    }

    fn parse_bracketed_list(
        &mut self,
        context: ParsingContext,
        parse_element: impl FnMut(&mut Parser) -> Option<NodeId>,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) -> Option<NodeList> {
        if self.parse_expected(opening) {
            let result = self.parse_delimited_list(context, parse_element);
            self.parse_expected(closing);
            result
        } else {
            Some(NodeList::missing())
        }
    }

    fn parse_delimited_list(
        &mut self,
        context: ParsingContext,
        mut parse_element: impl FnMut(&mut Parser) -> Option<NodeId>,
    ) -> Option<NodeList> {
        let pos = self.node_pos();
        let save_parsing_context = self.parsing_context;
        self.parsing_context.insert(context);
        let mut nodes = Vec::new();
        loop {
            if self.is_list_element(context, false) {
                let start_pos = self.node_pos();
                let Some(element) = parse_element(self) else {
                    self.parsing_context = save_parsing_context;
                    // Return None to indicate parseElement failed
                    return None;
                };
                nodes.push(element);
                if self.parse_optional(SyntaxKind::CommaToken) {
                    // No need to check for a zero length node since we know we parsed a comma
                    continue;
                }
                if self.is_list_terminator(context) {
                    break;
                }
                // We didn't get a comma, and the list wasn't terminated, explicitly parse
                // out a comma so we give a good error message.
                if self.token != SyntaxKind::CommaToken && context == ParsingContext::EnumMembers {
                    self.parse_error_at_current_token(
                        Message::e1357_an_enum_member_name_must_be_followed_by_a_or(),
                        [],
                    );
                } else {
                    self.parse_expected(SyntaxKind::CommaToken);
                }
                // If the token was a semicolon, and the caller allows that, then skip it and
                // continue.  This ensures we get back on track and don't result in tons of
                // parse errors.  For example, this can happen when people do things like use
                // a semicolon to delimit object literal members.   Note: we'll have already
                // reported an error when we called parseExpected above.
                if matches!(
                    context,
                    ParsingContext::ObjectLiteralMembers | ParsingContext::ImportAttributes,
                ) && self.token == SyntaxKind::SemicolonToken
                    && !self.has_preceding_line_break()
                {
                    self.next_token();
                }
                if start_pos == self.node_pos() {
                    // What we're parsing isn't actually remotely recognizable as a element and we've consumed no tokens whatsoever
                    // Consume a token to advance the parser in some way and avoid an infinite loop
                    // This can happen when we're speculatively parsing parenthesized expressions which we think may be arrow functions,
                    // or when a modifier keyword which is disallowed as a parameter name (ie, `static` in strict mode) is supplied
                    self.next_token();
                }
                continue;
            }
            if self.is_list_terminator(context) {
                break;
            }
            if self.abort_parsing_list_or_move_to_next_token(context) {
                break;
            }
        }
        self.parsing_context = save_parsing_context;
        Some(NodeList { loc: TextRange::new(pos, self.node_pos()), nodes })
    }

    fn parse_list_index(
        &mut self,
        context: ParsingContext,
        mut parse_element: impl FnMut(&mut Parser, usize) -> NodeId,
    ) -> Vec<NodeId> {
        let save_parsing_context = self.parsing_context;
        self.parsing_context.insert(context);
        let mut outer_reparse_list = std::mem::take(&mut self.reparse_list);

        let mut list = Vec::new();
        while !self.is_list_terminator(context) {
            if self.is_list_element(context, false) {
                let elt = parse_element(self, list.len());
                for e in self.reparse_list.drain(..) {
                    // Propagate @typedef type alias declarations outwards to a context that permits them.
                    if (self.nodes[e].is_js_type_alias_declaration()
                        || self.nodes[e].is_js_import_declaration())
                        && !matches!(
                            context,
                            ParsingContext::SourceElements | ParsingContext::BlockStatements
                        )
                    {
                        outer_reparse_list.push(e);
                    } else {
                        list.push(e);
                    }
                }
                list.push(elt);
                continue;
            }

            if self.abort_parsing_list_or_move_to_next_token(context) {
                break;
            }
        }

        self.reparse_list = outer_reparse_list;
        self.parsing_context = save_parsing_context;
        list
    }

    fn abort_parsing_list_or_move_to_next_token(&mut self, context: ParsingContext) -> bool {
        self.parsing_context_errors(context);
        if self.is_in_some_parsing_context() {
            return true;
        }
        self.next_token();
        false
    }

    fn parse_top_level_statement(&mut self, mut i: usize) -> NodeId {
        self.statement_has_await_identifier = false;
        let statement = self.parse_statement();
        // Reparsed nodes (e.g. JSDoc @typedef) produced while parsing this statement are inserted
        // into the statement list before this statement, so account for them when recording the
        // statement's index for possibleAwaitSpans.
        i += self.reparse_list.len();
        if self.statement_has_await_identifier
            && !self.nodes[statement].flags.contains(NodeFlags::AwaitContext)
        {
            if self.possible_await_spans.last().is_none_or(|&last| last != i) {
                self.possible_await_spans.push(i);
                self.possible_await_spans.push(i + 1);
            } else {
                *self.possible_await_spans.last_mut().unwrap() = i + 1;
            }
        }
        statement
    }

    fn is_list_element(&mut self, context: ParsingContext, in_error_recovery: bool) -> bool {
        match context {
            ParsingContext::SourceElements
            | ParsingContext::BlockStatements
            | ParsingContext::SwitchClauseStatements => {
                // If we're in error recovery, then we don't want to treat ';' as an empty statement.
                // The problem is that ';' can show up in far too many contexts, and if we see one
                // and assume it's a statement, then we may bail out inappropriately from whatever
                // we're parsing.  For example, if we have a semicolon in the middle of a class, then
                // we really don't want to assume the class is over and we're on a statement in the
                // outer module.  We just want to consume and move on.
                !(self.token == SyntaxKind::SemicolonToken && in_error_recovery)
                    && self.is_start_of_statement()
            }
            ParsingContext::SwitchClauses => {
                self.token == SyntaxKind::CaseKeyword || self.token == SyntaxKind::DefaultKeyword
            }
            ParsingContext::TypeMembers => self.look_ahead(Self::scan_type_member_start),
            ParsingContext::ClassMembers => {
                // We allow semicolons as class elements (as specified by ES6) as long as we're
                // not in error recovery.  If we're in error recovery, we don't want an errant
                // semicolon to be treated as a class member (since they're almost always used
                // for statements.
                self.look_ahead(Self::scan_class_member_start)
                    || self.token == SyntaxKind::SemicolonToken && !in_error_recovery
            }
            ParsingContext::EnumMembers => {
                // Include open bracket computed properties. This technically also lets in indexers,
                // which would be a candidate for improved error reporting.
                self.token == SyntaxKind::OpenBracketToken || self.is_literal_property_name()
            }
            ParsingContext::ObjectLiteralMembers => {
                match self.token {
                    SyntaxKind::OpenBracketToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::DotDotDotToken
                    | SyntaxKind::DotToken =>
                    // Not an object literal member, but don't want to close the object (see `tests/cases/fourslash/completionsDotInObjectLiteral.ts`)
                    {
                        true
                    }
                    _ => self.is_literal_property_name(),
                }
            }
            ParsingContext::RestProperties => self.is_literal_property_name(),
            ParsingContext::ObjectBindingElements => {
                self.token == SyntaxKind::OpenBracketToken
                    || self.token == SyntaxKind::DotDotDotToken
                    || self.is_literal_property_name()
            }
            ParsingContext::ImportAttributes => self.is_import_attribute_name(),
            ParsingContext::HeritageClauseElement => {
                // If we see `{ ... }` then only consume it as an expression if it is followed by `,` or `{`
                // That way we won't consume the body of a class in its heritage clause.
                if self.token == SyntaxKind::OpenBraceToken {
                    return self.is_valid_heritage_clause_object_literal();
                }
                if !in_error_recovery {
                    return self.is_start_of_left_hand_side_expression()
                        && !self.is_heritage_clause_extends_or_implements_keyword();
                }
                // If we're in error recovery we tighten up what we're willing to match.
                // That way we don't treat something like "this" as a valid heritage clause
                // element during recovery.
                self.is_identifier() && !self.is_heritage_clause_extends_or_implements_keyword()
            }
            ParsingContext::VariableDeclarations => {
                self.is_binding_identifier_or_private_identifier_or_pattern()
            }
            ParsingContext::ArrayBindingElements => {
                self.token == SyntaxKind::CommaToken
                    || self.token == SyntaxKind::DotDotDotToken
                    || self.is_binding_identifier_or_private_identifier_or_pattern()
            }
            ParsingContext::TypeParameters => {
                self.token == SyntaxKind::InKeyword
                    || self.token == SyntaxKind::ConstKeyword
                    || self.is_identifier()
            }
            ParsingContext::ArrayLiteralMembers => {
                // Not an array literal member, but don't want to close the array (see `tests/cases/fourslash/completionsDotInArrayLiteralInObjectLiteral.ts`)
                if self.token == SyntaxKind::CommaToken || self.token == SyntaxKind::DotToken {
                    return true;
                }
                self.token == SyntaxKind::DotDotDotToken || self.is_start_of_expression()
            }
            ParsingContext::ArgumentExpressions => {
                self.token == SyntaxKind::DotDotDotToken || self.is_start_of_expression()
            }
            ParsingContext::Parameters => {
                self.is_start_of_parameter(false /*isJSDocParameter*/)
            }
            ParsingContext::JSDocParameters => {
                self.is_start_of_parameter(true /*isJSDocParameter*/)
            }
            ParsingContext::TypeArguments | ParsingContext::TupleElementTypes => {
                self.token == SyntaxKind::CommaToken
                    || self.is_start_of_type(false /*inStartOfParameter*/)
            }
            ParsingContext::HeritageClauses => self.is_heritage_clause(),
            ParsingContext::ImportOrExportSpecifiers => {
                // bail out if the next token is [FromKeyword StringLiteral].
                // That means we're in something like `import { from "mod"`. Stop here can give better error message.
                if self.token == SyntaxKind::FromKeyword
                    && self.look_ahead(Self::next_token_is_string_literal)
                {
                    return false;
                }
                if self.token == SyntaxKind::StringLiteral {
                    return true; // For "arbitrary module namespace identifiers"
                }
                self.token.is_identifier_or_keyword()
            }
            ParsingContext::JsxAttributes => {
                self.token.is_identifier_or_keyword() || self.token == SyntaxKind::OpenBraceToken
            }
            ParsingContext::JsxChildren => true,
            ParsingContext::JSDocComment => true,
            _ => panic!("Unhandled case in isListElement"),
        }
    }

    fn is_list_terminator(&mut self, context: ParsingContext) -> bool {
        if self.token == SyntaxKind::EndOfFile {
            return true;
        }

        match context {
            ParsingContext::BlockStatements
            | ParsingContext::SwitchClauses
            | ParsingContext::TypeMembers
            | ParsingContext::ClassMembers
            | ParsingContext::EnumMembers
            | ParsingContext::ObjectLiteralMembers
            | ParsingContext::ObjectBindingElements
            | ParsingContext::ImportOrExportSpecifiers
            | ParsingContext::ImportAttributes => self.token == SyntaxKind::CloseBraceToken,

            ParsingContext::SwitchClauseStatements => matches!(
                self.token,
                SyntaxKind::CloseBraceToken | SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword
            ),
            ParsingContext::HeritageClauseElement => matches!(
                self.token,
                SyntaxKind::OpenBraceToken
                    | SyntaxKind::ExtendsKeyword
                    | SyntaxKind::ImplementsKeyword
            ),
            ParsingContext::VariableDeclarations => {
                // If we can consume a semicolon (either explicitly, or with ASI), then consider us done
                // with parsing the list of variable declarators.
                // In the case where we're parsing the variable declarator of a 'for-in' statement, we
                // are done if we see an 'in' keyword in front of us. Same with for-of
                // ERROR RECOVERY TWEAK:
                // For better error recovery, if we see an '=>' then we just stop immediately.  We've got an
                // arrow function here and it's going to be very unlikely that we'll resynchronize and get
                // another variable declaration.
                self.can_parse_semicolon()
                    || matches!(
                        self.token,
                        SyntaxKind::InKeyword
                            | SyntaxKind::OfKeyword
                            | SyntaxKind::EqualsGreaterThanToken
                    )
            }
            ParsingContext::TypeParameters => {
                // Tokens other than '>' are here for better error recovery
                matches!(
                    self.token,
                    SyntaxKind::GreaterThanToken
                        | SyntaxKind::OpenParenToken
                        | SyntaxKind::OpenBraceToken
                        | SyntaxKind::ExtendsKeyword
                        | SyntaxKind::ImplementsKeyword
                )
            }
            ParsingContext::ArgumentExpressions => {
                // Tokens other than ')' are here for better error recovery
                matches!(self.token, SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken)
            }
            ParsingContext::ArrayLiteralMembers
            | ParsingContext::TupleElementTypes
            | ParsingContext::ArrayBindingElements => self.token == SyntaxKind::CloseBracketToken,
            ParsingContext::JSDocParameters
            | ParsingContext::Parameters
            | ParsingContext::RestProperties => {
                // Tokens other than ')' and ']' (the latter for index signatures) are here for better error recovery
                matches!(self.token, SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken)
            }
            ParsingContext::TypeArguments => {
                // All other tokens should cause the type-argument to terminate except comma token
                self.token != SyntaxKind::CommaToken
            }
            ParsingContext::HeritageClauses => {
                self.token == SyntaxKind::OpenBraceToken
                    || self.token == SyntaxKind::CloseBraceToken
            }
            ParsingContext::JsxAttributes => {
                matches!(self.token, SyntaxKind::GreaterThanToken | SyntaxKind::SlashToken)
            }
            ParsingContext::JsxChildren => {
                self.token == SyntaxKind::LessThanToken
                    && self.look_ahead(Self::next_token_is_slash)
            }
            _ => false,
        }
    }

    fn look_ahead<T>(&mut self, callback: impl FnOnce(&mut Parser) -> T) -> T {
        let state = self.mark();
        let result = callback(self);
        self.rewind(state);
        result
    }

    fn mark(&self) -> ParserState {
        ParserState {
            scanner_state: self.scanner.mark(),
            context_flags: self.context_flags,
            diagnostics_len: self.diagnostics.len(),
            js_diagnostics_len: self.js_diagnostics.len(),
            jsdoc_infos_len: self.jsdoc_infos.len(),
            reparsed_clones_len: self.reparsed_clones.len(),
            statement_has_await_identifier: self.statement_has_await_identifier,
            has_parse_error: self.has_parse_error,
        }
    }

    fn rewind(&mut self, state: ParserState) {
        self.scanner.rewind(state.scanner_state);
        self.token = self.scanner.token();
        self.context_flags = state.context_flags;
        self.diagnostics.truncate(state.diagnostics_len);
        self.js_diagnostics.truncate(state.js_diagnostics_len);
        self.jsdoc_infos.truncate(state.jsdoc_infos_len);
        self.reparsed_clones.truncate(state.reparsed_clones_len);
        self.statement_has_await_identifier = state.statement_has_await_identifier;
        self.has_parse_error = state.has_parse_error;
    }

    fn can_parse_semicolon(&self) -> bool {
        // If there's a real semicolon, then we can always parse it out.
        // We can parse out an optional semicolon in ASI cases in the following cases.
        self.token == SyntaxKind::SemicolonToken
            || self.token == SyntaxKind::CloseBraceToken
            || self.token == SyntaxKind::EndOfFile
            || self.has_preceding_line_break()
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
                self.context_flags.insert(NodeFlags::JavaScriptFile | NodeFlags::JsonFile);
            }
            _ => {
                self.context_flags = NodeFlags::empty();
            }
        }
        self.scanner.set_diagnostics(self.diagnostics.clone());
    }

    fn next_token(&mut self) -> SyntaxKind {
        // if the keyword had an escape
        if self.token.is_keyword()
            && (self.scanner.has_unicode_escape() || self.scanner.has_extended_unicode_escape())
        {
            self.parse_error_at_current_token(
                Message::e1260_keywords_cannot_contain_escape_characters(),
                None,
            );
        }
        self.token = self.scanner.scan();
        self.token
    }

    fn next_token_without_check(&mut self) -> SyntaxKind {
        self.token = self.scanner.scan();
        self.token
    }

    fn node_pos(&self) -> usize {
        self.scanner.full_token_start()
    }

    fn in_context<T>(
        &mut self,
        context: NodeFlags,
        value: bool,
        func: impl FnOnce(&mut Parser) -> T,
    ) -> T {
        let save_context_flags = self.context_flags;
        self.set_context_flags(context, value);
        let result = func(self);
        self.context_flags = save_context_flags;
        result
    }

    fn is_start_of_statement(&mut self) -> bool {
        match self.token {
            // 'catch' and 'finally' do not actually indicate that the code is part of a statement,
            // however, we say they are here so that we may gracefully parse them and error later.
            SyntaxKind::AtToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::IfKeyword
            | SyntaxKind::DoKeyword
            | SyntaxKind::WhileKeyword
            | SyntaxKind::ForKeyword
            | SyntaxKind::ContinueKeyword
            | SyntaxKind::BreakKeyword
            | SyntaxKind::ReturnKeyword
            | SyntaxKind::WithKeyword
            | SyntaxKind::SwitchKeyword
            | SyntaxKind::ThrowKeyword
            | SyntaxKind::TryKeyword
            | SyntaxKind::DebuggerKeyword
            | SyntaxKind::CatchKeyword
            | SyntaxKind::FinallyKeyword => true,
            SyntaxKind::ImportKeyword => {
                self.is_start_of_declaration()
                    || self.is_next_token_open_paren_or_less_than_or_dot()
            }
            SyntaxKind::ConstKeyword | SyntaxKind::ExportKeyword => self.is_start_of_declaration(),
            SyntaxKind::AsyncKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::GlobalKeyword
            | SyntaxKind::DeferKeyword => {
                // When these don't start a declaration, they're an identifier in an expression statement
                true
            }
            SyntaxKind::AccessorKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadonlyKeyword => {
                // When these don't start a declaration, they may be the start of a class member if an identifier
                // immediately follows. Otherwise they're an identifier in an expression statement.
                self.is_start_of_declaration()
                    || !self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line)
            }

            _ => self.is_start_of_expression(),
        }
    }

    fn is_start_of_declaration(&mut self) -> bool {
        self.look_ahead(Self::scan_start_of_declaration)
    }

    fn next_is_start_of_expression(&mut self) -> bool {
        self.next_token();
        self.is_start_of_expression()
    }

    fn is_start_of_expression(&mut self) -> bool {
        if self.is_start_of_left_hand_side_expression() {
            return true;
        }
        if let SyntaxKind::PlusToken
        | SyntaxKind::MinusToken
        | SyntaxKind::TildeToken
        | SyntaxKind::ExclamationToken
        | SyntaxKind::DeleteKeyword
        | SyntaxKind::TypeOfKeyword
        | SyntaxKind::VoidKeyword
        | SyntaxKind::PlusPlusToken
        | SyntaxKind::MinusMinusToken
        | SyntaxKind::LessThanToken
        | SyntaxKind::AwaitKeyword
        | SyntaxKind::YieldKeyword
        | SyntaxKind::PrivateIdentifier
        | SyntaxKind::AtToken = self.token
        {
            return true;
        }
        // Error tolerance.  If we see the start of some binary operator, we consider
        // that the start of an expression.  That way we'll parse out a missing identifier,
        // give a good message about an identifier being missing, and then consume the
        // rest of the binary expression.
        if self.is_binary_operator() {
            return true;
        }
        self.is_identifier()
    }

    fn is_heritage_clause(&self) -> bool {
        matches!(self.token, SyntaxKind::ExtendsKeyword | SyntaxKind::ImplementsKeyword)
    }

    fn is_literal_property_name(&self) -> bool {
        self.token.is_identifier_or_keyword()
            || matches!(
                self.token,
                SyntaxKind::StringLiteral | SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
            )
    }

    fn is_import_attribute_name(&self) -> bool {
        self.token.is_identifier_or_keyword() || self.token == SyntaxKind::StringLiteral
    }

    fn is_binding_identifier_or_private_identifier_or_pattern(&self) -> bool {
        matches!(
            self.token,
            SyntaxKind::OpenBraceToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::PrivateIdentifier
        ) || self.is_binding_identifier()
    }

    fn is_start_of_parameter(&mut self, is_jsdoc_parameter: bool) -> bool {
        self.token == SyntaxKind::DotDotDotToken
            || self.is_binding_identifier_or_private_identifier_or_pattern()
            || self.token.is_modifier()
            || self.token == SyntaxKind::AtToken
            || self.is_start_of_type(!is_jsdoc_parameter)
    }

    fn is_identifier(&self) -> bool {
        if self.token == SyntaxKind::Identifier {
            return true;
        }

        // If we have a 'yield' keyword, and we're in the [yield] context, then 'yield' is
        // considered a keyword and is not an identifier.
        // If we have a 'await' keyword, and we're in the [Await] context, then 'await' is
        // considered a keyword and is not an identifier.
        if self.token == SyntaxKind::YieldKeyword && self.in_yield_context()
            || self.token == SyntaxKind::AwaitKeyword && self.in_await_context()
        {
            return false;
        }

        self.token > SyntaxKind::LAST_RESERVED_WORD
    }

    fn next_is_start_of_type(&mut self) -> bool {
        self.next_token();
        self.is_start_of_type(false)
    }

    fn is_start_of_type(&mut self, in_start_of_parameter: bool) -> bool {
        match self.token {
            SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::UniqueKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::ThisKeyword
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::BarToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::NewKeyword
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::AsteriskToken
            | SyntaxKind::QuestionToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::InferKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::AssertsKeyword
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,
            SyntaxKind::FunctionKeyword => !in_start_of_parameter,
            SyntaxKind::MinusToken => {
                !in_start_of_parameter
                    && self.look_ahead(Self::next_token_is_numeric_or_big_int_literal)
            }
            SyntaxKind::OpenParenToken => {
                // Only consider '(' the start of a type if followed by ')', '...', an identifier, a modifier,
                // or something that starts a type. We don't want to consider things like '(1)' a type.
                !in_start_of_parameter
                    && self.look_ahead(Self::next_token_is_parenthesized_or_function_type)
            }
            _ => self.is_identifier(),
        }
    }

    fn is_heritage_clause_extends_or_implements_keyword(&mut self) -> bool {
        self.is_heritage_clause() && self.look_ahead(Self::next_is_start_of_expression)
    }

    fn is_start_of_left_hand_side_expression(&mut self) -> bool {
        match self.token {
            SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::NewKeyword
            | SyntaxKind::SlashToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::Identifier => true,
            SyntaxKind::ImportKeyword => self.is_next_token_open_paren_or_less_than_or_dot(),
            _ => self.is_identifier(),
        }
    }

    fn scan_class_member_start(&mut self) -> bool {
        let mut id_token = SyntaxKind::Unknown;
        if self.token == SyntaxKind::AtToken {
            return true;
        }
        // Eat up all modifiers, but hold on to the last one in case it is actually an identifier.
        while self.token.is_modifier() {
            id_token = self.token;
            // If the idToken is a class modifier (protected, private, public, and static), it is
            // certain that we are starting to parse class member. This allows better error recovery
            // Example:
            //      public foo() ...     // true
            //      public @dec blah ... // true; we will then report an error later
            //      export public ...    // true; we will then report an error later
            if id_token.is_class_member_modifier() {
                return true;
            }
            self.next_token();
        }
        if self.token == SyntaxKind::AsteriskToken {
            return true;
        }
        // Try to get the first property-like token following all modifiers.
        // This can either be an identifier or the 'get' or 'set' keywords.
        if self.is_literal_property_name() {
            id_token = self.token;
            self.next_token();
        }
        // Index signatures and computed properties are class members; we can parse.
        if self.token == SyntaxKind::OpenBracketToken {
            return true;
        }
        // If we were able to get any potential identifier...
        if id_token != SyntaxKind::Unknown {
            // If we have a non-keyword identifier, or if we have an accessor, then it's safe to parse.
            if !id_token.is_keyword()
                || id_token == SyntaxKind::SetKeyword
                || id_token == SyntaxKind::GetKeyword
            {
                return true;
            }
            // If it *is* a keyword, but not an accessor, check a little farther along
            // to see if it should actually be parsed as a class member.
            // SyntaxKind::OpenParenToken => Method declaration
            // SyntaxKind::LessThanToken => Generic Method declaration
            // SyntaxKind::ExclamationToken => Non-null assertion on property name
            // SyntaxKind::ColonToken => Type Annotation for declaration
            // SyntaxKind::EqualsToken => Initializer for declaration
            // SyntaxKind::QuestionToken => Not valid, but permitted so that it gets caught later on.
            return match self.token {
                SyntaxKind::OpenParenToken => true,   // Method declaration
                SyntaxKind::LessThanToken => true,    // Generic Method declaration
                SyntaxKind::ExclamationToken => true, // Non-null assertion on property name
                SyntaxKind::ColonToken => true,       // Type Annotation for declaration
                SyntaxKind::EqualsToken => true,      // Initializer for declaration
                SyntaxKind::QuestionToken => true, // Not valid, but permitted so that it gets caught later on.
                _ => {
                    // Covers
                    //  - Semicolons     (declaration termination)
                    //  - Closing braces (end-of-class, must be declaration)
                    //  - End-of-files   (not valid, but permitted so that it gets caught later on)
                    //  - Line-breaks    (enabling *automatic semicolon insertion*)
                    self.can_parse_semicolon()
                }
            };
        }
        false
    }

    fn scan_type_member_start(&mut self) -> bool {
        // Return true if we have the start of a signature member
        if matches!(
            self.token,
            SyntaxKind::OpenParenToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
        ) {
            return true;
        }
        let mut id_token = false;
        // Eat up all modifiers, but hold on to the last one in case it is actually an identifier
        while self.token.is_modifier() {
            id_token = true;
            self.next_token();
        }
        // Index signatures and computed property names are type members
        if self.token == SyntaxKind::OpenBracketToken {
            return true;
        }
        // Try to get the first property-like token following all modifiers
        if self.is_literal_property_name() {
            id_token = true;
            self.next_token();
        }
        // If we were able to get any potential identifier, check that it is
        // the start of a member declaration
        if id_token {
            return matches!(
                self.token,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
            ) || self.can_parse_semicolon();
        }
        false
    }

    fn scan_start_of_declaration(&mut self) -> bool {
        loop {
            match self.token {
                SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::EnumKeyword => return true,
                SyntaxKind::UsingKeyword => return self.is_using_declaration(),
                SyntaxKind::AwaitKeyword => return self.is_await_using_declaration(),
                // 'declare', 'module', 'namespace', 'interface'* and 'type' are all legal JavaScript identifiers;
                // however, an identifier cannot be followed by another identifier on the same line. This is what we
                // count on to parse out the respective declarations. For instance, we exploit this to say that
                //
                //    namespace n
                //
                // can be none other than the beginning of a namespace declaration, but need to respect that JavaScript sees
                //
                //    namespace
                //    n
                //
                // as the identifier 'namespace' on one line followed by the identifier 'n' on another.
                // We need to look one token ahead to see if it permissible to try parsing a declaration.
                //
                // *Note*: 'interface' is actually a strict mode reserved word. So while
                //
                //   "use strict"
                //   interface
                //   I {}
                //
                // could be legal, it would add complexity for very little gain.
                SyntaxKind::InterfaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::DeferKeyword => {
                    return self.look_ahead(Self::next_token_is_identifier_on_same_line);
                }
                SyntaxKind::ModuleKeyword | SyntaxKind::NamespaceKeyword => {
                    return self
                        .look_ahead(Self::next_token_is_identifier_or_string_literal_on_same_line);
                }
                SyntaxKind::AbstractKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ReadonlyKeyword => {
                    let previous_token = self.token;
                    self.next_token();
                    // ASI takes effect for this modifier.
                    if self.has_preceding_line_break() {
                        return false;
                    }
                    if previous_token == SyntaxKind::DeclareKeyword
                        && self.token == SyntaxKind::TypeKeyword
                    {
                        // If we see 'declare type', then commit to parsing a type alias. parseTypeAliasDeclaration will
                        // report Line_break_not_permitted_here if needed.
                        return true;
                    }
                    continue;
                }
                SyntaxKind::GlobalKeyword => {
                    self.next_token();
                    return self.token == SyntaxKind::OpenBraceToken
                        || self.token == SyntaxKind::Identifier
                        || self.token == SyntaxKind::ExportKeyword;
                }
                SyntaxKind::ImportKeyword => {
                    self.next_token();
                    return self.token == SyntaxKind::DeferKeyword
                        || self.token == SyntaxKind::StringLiteral
                        || self.token == SyntaxKind::AsteriskToken
                        || self.token == SyntaxKind::OpenBraceToken
                        || self.token.is_identifier_or_keyword();
                }
                SyntaxKind::ExportKeyword => {
                    self.next_token();
                    if self.token == SyntaxKind::EqualsToken
                        || self.token == SyntaxKind::AsteriskToken
                        || self.token == SyntaxKind::OpenBraceToken
                        || self.token == SyntaxKind::DefaultKeyword
                        || self.token == SyntaxKind::AsKeyword
                        || self.token == SyntaxKind::AtToken
                    {
                        return true;
                    }
                    if self.token == SyntaxKind::TypeKeyword {
                        self.next_token();
                        return self.token == SyntaxKind::AsteriskToken
                            || self.token == SyntaxKind::OpenBraceToken
                            || self.is_identifier() && !self.has_preceding_line_break();
                    }
                    continue;
                }
                SyntaxKind::StaticKeyword => {
                    self.next_token();
                    continue;
                }
                _ => return false,
            }
        }
    }

    fn next_token_is_string_literal(&mut self) -> bool {
        self.next_token() == SyntaxKind::StringLiteral
    }

    fn next_token_is_slash(&mut self) -> bool {
        self.next_token() == SyntaxKind::SlashToken
    }

    fn next_token_is_identifier_or_keyword(&mut self) -> bool {
        self.next_token().is_identifier_or_keyword()
    }

    fn next_token_is_identifier_or_keyword_on_same_line(&mut self) -> bool {
        self.next_token_is_identifier_or_keyword() && !self.has_preceding_line_break()
    }

    fn next_token_is_identifier_or_keyword_or_literal_on_same_line(&mut self) -> bool {
        (self.next_token_is_identifier_or_keyword()
            || matches!(
                self.token,
                SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral | SyntaxKind::StringLiteral
            ))
            && !self.has_preceding_line_break()
    }

    fn next_token_is_identifier_or_keyword_or_open_bracket_or_template(&mut self) -> bool {
        self.next_token();
        self.token.is_identifier_or_keyword()
            || self.token == SyntaxKind::OpenBracketToken
            || self.is_template_start_of_tagged_template()
    }

    fn is_next_token_open_paren_or_less_than_or_dot(&mut self) -> bool {
        self.look_ahead(Self::next_token_is_open_paren_or_less_than_or_dot)
    }

    fn next_token_is_open_paren_or_less_than_or_dot(&mut self) -> bool {
        matches!(
            self.next_token(),
            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken | SyntaxKind::DotToken
        )
    }

    fn is_binary_operator(&self) -> bool {
        if self.in_disallow_in_context() && self.token == SyntaxKind::InKeyword {
            return false;
        }
        self.token.binary_operator_precedence() != OperatorPrecedence::Invalid
    }

    fn is_binding_identifier(&self) -> bool {
        // `let await`/`let yield` in [Yield] or [Await] are allowed here and disallowed in the binder.
        self.token == SyntaxKind::Identifier || self.token > SyntaxKind::LAST_RESERVED_WORD
    }

    fn is_valid_heritage_clause_object_literal(&mut self) -> bool {
        self.look_ahead(Self::next_is_valid_heritage_clause_object_literal)
    }

    fn next_is_valid_heritage_clause_object_literal(&mut self) -> bool {
        if self.next_token() == SyntaxKind::CloseBraceToken {
            // if we see "extends {}" then only treat the {} as what we're extending (and not
            // the class body) if we have:
            //
            //      extends {} {
            //      extends {},
            //      extends {} extends
            //      extends {} implements
            matches!(
                self.next_token(),
                SyntaxKind::CommaToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::ExtendsKeyword
                    | SyntaxKind::ImplementsKeyword
            )
        } else {
            true
        }
    }

    fn next_token_is_numeric_or_big_int_literal(&mut self) -> bool {
        matches!(self.next_token(), SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral)
    }

    fn next_token_is_parenthesized_or_function_type(&mut self) -> bool {
        self.next_token();
        self.token == SyntaxKind::CloseParenToken
            || self.is_start_of_parameter(false)
            || self.is_start_of_type(false)
    }

    fn in_yield_context(&self) -> bool {
        self.context_flags.contains(NodeFlags::YieldContext)
    }

    fn in_await_context(&self) -> bool {
        self.context_flags.contains(NodeFlags::AwaitContext)
    }

    fn in_decorator_context(&self) -> bool {
        self.context_flags.contains(NodeFlags::AwaitContext)
    }

    fn in_disallow_in_context(&self) -> bool {
        self.context_flags.contains(NodeFlags::DisallowInContext)
    }

    fn in_disallow_conditional_types_context(&self) -> bool {
        self.context_flags.contains(NodeFlags::DisallowConditionalTypesContext)
    }

    fn is_using_declaration(&mut self) -> bool {
        // 'using' always starts a lexical declaration if followed by an identifier. We also eagerly parse
        // |ObjectBindingPattern| so that we can report a grammar error during check. We don't parse out
        // |ArrayBindingPattern| since it potentially conflicts with element access (i.e., `using[x]`).
        self.look_ahead(|p| {
            p.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(false)
        })
    }

    fn is_await_using_declaration(&mut self) -> bool {
        self.look_ahead(
            Self::next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line,
        )
    }

    fn next_token_is_identifier_on_same_line(&mut self) -> bool {
        self.next_token();
        self.is_identifier() && !self.has_preceding_line_break()
    }

    fn next_token_is_identifier_or_string_literal_on_same_line(&mut self) -> bool {
        self.next_token();
        (self.is_identifier() || self.token == SyntaxKind::StringLiteral)
            && !self.has_preceding_line_break()
    }

    fn has_preceding_line_break(&self) -> bool {
        self.scanner.has_preceding_line_break()
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring(&mut self) -> bool {
        self.next_token();
        self.is_binding_identifier()
            || matches!(self.token, SyntaxKind::OpenBraceToken | SyntaxKind::OpenBracketToken)
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring_on_same_line_disallow_of(
        &mut self,
    ) -> bool {
        self.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(true)
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(
        &mut self,
        disallow_of: bool,
    ) -> bool {
        self.next_token();
        if disallow_of && self.token == SyntaxKind::OfKeyword {
            return self.look_ahead(Self::next_token_is_equals_or_semicolon_or_colon_token);
        }
        (self.is_binding_identifier() || self.token == SyntaxKind::OpenBraceToken)
            && !self.has_preceding_line_break()
    }

    fn next_token_is_equals_or_semicolon_or_colon_token(&mut self) -> bool {
        matches!(
            self.next_token(),
            SyntaxKind::EqualsToken | SyntaxKind::SemicolonToken | SyntaxKind::ColonToken
        )
    }

    fn next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line(
        &mut self,
    ) -> bool {
        self.next_token() == SyntaxKind::UsingKeyword
            && self.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(false)
    }

    fn parse_error_for_missing_semicolon_after(&mut self, node: NodeId) {
        let kind = self.nodes[node].kind;

        // Tagged template literals are sometimes used in places where only simple strings are allowed, i.e.:
        //   module `M1` {
        //   ^^^^^^^^^^^ This block is parsed as a template literal like module`M1`.
        if kind == SyntaxKind::TaggedTemplateExpression {
            let template = self.nodes[node].data_ref::<TaggedTemplateExpression>().template;
            let loc = self.skip_range_trivia(self.nodes[template].loc);
            self.parse_error_at_range(
                loc,
                Message::e1443_module_declaration_names_may_only_use_or_quoted_strings(),
                [],
            );
            return;
        }

        todo!()
    }

    fn parse_error_at_current_token(
        &mut self,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) -> Option<DiagnosticId> {
        self.parse_error_at_range(self.scanner.token_range(), message, args)
    }

    fn parse_error_at_range(
        &mut self,
        loc: TextRange,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) -> Option<DiagnosticId> {
        let mut diagnostic = None;
        // Don't report another error if it would just be at the same location as the last error
        if self.diagnostics.last_and(|d| d.loc.pos == loc.pos).is_none() {
            diagnostic = Some(self.diagnostics.report(message, loc, args));
        }
        self.has_parse_error = true;
        diagnostic
    }

    fn parsing_context_errors(&mut self, context: ParsingContext) {
        match context {
            ParsingContext::SourceElements => {
                if self.token == SyntaxKind::DefaultKeyword {
                    self.parse_error_at_current_token(
                        Message::e1005_0_expected(),
                        ["export".to_string()],
                    );
                } else {
                    self.parse_error_at_current_token(
                        Message::e1128_declaration_or_statement_expected(),
                        None,
                    );
                }
            }
            ParsingContext::BlockStatements => {
                self.parse_error_at_current_token(
                    Message::e1128_declaration_or_statement_expected(),
                    None,
                );
            }
            ParsingContext::SwitchClauses => {
                self.parse_error_at_current_token(Message::e1130_case_or_default_expected(), []);
            }
            ParsingContext::SwitchClauseStatements => {
                self.parse_error_at_current_token(Message::e1129_statement_expected(), []);
            }
            ParsingContext::RestProperties | ParsingContext::TypeMembers => {
                self.parse_error_at_current_token(
                    Message::e1131_property_or_signature_expected(),
                    [],
                );
            }
            ParsingContext::ClassMembers => {
                self.parse_error_at_current_token(
                Message::e1068_unexpected_token_a_constructor_method_accessor_or_property_was_expected(),
                None
            );
            }
            ParsingContext::EnumMembers => {
                self.parse_error_at_current_token(Message::e1132_enum_member_expected(), []);
            }
            ParsingContext::HeritageClauseElement => {
                self.parse_error_at_current_token(Message::e1109_expression_expected(), []);
            }
            ParsingContext::VariableDeclarations => {
                if self.token.is_keyword() {
                    self.parse_error_at_current_token(
                        Message::e1389_0_is_not_allowed_as_a_variable_declaration_name(),
                        [token_to_text(self.token).to_string()],
                    );
                } else {
                    self.parse_error_at_current_token(
                        Message::e1134_variable_declaration_expected(),
                        [],
                    );
                }
            }
            ParsingContext::ObjectBindingElements => {
                self.parse_error_at_current_token(
                    Message::e1180_property_destructuring_pattern_expected(),
                    [],
                );
            }
            ParsingContext::ArrayBindingElements => {
                self.parse_error_at_current_token(
                    Message::e1181_array_element_destructuring_pattern_expected(),
                    None,
                );
            }
            ParsingContext::ArgumentExpressions => {
                self.parse_error_at_current_token(
                    Message::e1135_argument_expression_expected(),
                    [],
                );
            }
            ParsingContext::ObjectLiteralMembers => {
                self.parse_error_at_current_token(
                    Message::e1136_property_assignment_expected(),
                    [],
                );
            }
            ParsingContext::ArrayLiteralMembers => {
                self.parse_error_at_current_token(
                    Message::e1137_expression_or_comma_expected(),
                    [],
                );
            }
            ParsingContext::JSDocParameters => {
                self.parse_error_at_current_token(
                    Message::e1138_parameter_declaration_expected(),
                    [],
                );
            }
            ParsingContext::Parameters => {
                if self.token.is_keyword() {
                    self.parse_error_at_current_token(
                        Message::e1390_0_is_not_allowed_as_a_parameter_name(),
                        [token_to_text(self.token).to_string()],
                    );
                } else {
                    self.parse_error_at_current_token(
                        Message::e1138_parameter_declaration_expected(),
                        [],
                    );
                }
            }
            ParsingContext::TypeParameters => {
                self.parse_error_at_current_token(
                    Message::e1139_type_parameter_declaration_expected(),
                    [],
                );
            }
            ParsingContext::TypeArguments => {
                self.parse_error_at_current_token(Message::e1140_type_argument_expected(), []);
            }
            ParsingContext::TupleElementTypes => {
                self.parse_error_at_current_token(Message::e1110_type_expected(), []);
            }
            ParsingContext::HeritageClauses => {
                self.parse_error_at_current_token(Message::e1179_unexpected_token_expected(), []);
            }
            ParsingContext::ImportOrExportSpecifiers => {
                if self.token == SyntaxKind::FromKeyword {
                    self.parse_error_at_current_token(
                        Message::e1005_0_expected(),
                        ["}".to_string()],
                    );
                } else {
                    self.parse_error_at_current_token(Message::e1003_identifier_expected(), []);
                }
            }
            ParsingContext::JsxAttributes
            | ParsingContext::JsxChildren
            | ParsingContext::JSDocComment => {
                self.parse_error_at_current_token(Message::e1003_identifier_expected(), []);
            }
            ParsingContext::ImportAttributes => {
                self.parse_error_at_current_token(
                    Message::e1478_identifier_or_string_literal_expected(),
                    [],
                );
            }
            _ => panic!("Unhandled case in parsingContextErrors"),
        }
    }

    fn is_in_some_parsing_context(&mut self) -> bool {
        // We should be in at least one parsing context, be it SourceElements while parsing
        // a SourceFile, or JSDocComment when lazily parsing JSDoc.
        debug_assert_ne!(self.parsing_context, ParsingContext::empty());

        for context in self.parsing_context.iter() {
            if self.is_list_element(context, true) || self.is_list_terminator(context) {
                return true;
            }
        }
        false
    }

    fn jsdoc_scanner_info(&self) -> JSDocScannerInfo {
        if !self.scanner.has_preceding_jsdoc_comment() {
            return JSDocScannerInfo::empty();
        }
        let mut info = JSDocScannerInfo::HasJSDoc;
        if self.scanner.has_preceding_jsdoc_with_deprecated_tag() {
            info.insert(JSDocScannerInfo::HasDeprecated);
        }
        if self.scanner.has_preceding_jsdoc_with_see_or_link() {
            info.insert(JSDocScannerInfo::HasSeeOrLink);
        }
        info
    }

    fn parse_expected(&mut self, kind: SyntaxKind) -> bool {
        self.parse_expected_with_diagnostic(kind, None, true)
    }

    fn parse_expected_without_advancing(&mut self, kind: SyntaxKind) -> bool {
        self.parse_expected_with_diagnostic(kind, None, false)
    }

    fn parse_expected_with_diagnostic(
        &mut self,
        kind: SyntaxKind,
        message: Option<&'static Message>,
        should_advance: bool,
    ) -> bool {
        if self.token == kind {
            if should_advance {
                self.next_token();
            }
            return true;
        }
        // Report specific message if provided with one.  Otherwise, report generic fallback message.
        if let Some(message) = message {
            self.parse_error_at_current_token(message, []);
        } else {
            self.parse_error_at_current_token(
                Message::e1005_0_expected(),
                [token_to_text(kind).to_string()],
            );
        }
        false
    }

    fn finish_node(&mut self, node: NodeId, pos: usize) -> NodeId {
        self.finish_node_with_end(node, pos, self.node_pos());
        node
    }

    fn finish_node_with_end(&mut self, node: NodeId, pos: usize, end: usize) {
        self.nodes[node].loc = TextRange::new(pos, end);
        self.nodes[node].flags.insert(self.context_flags);
        if self.has_parse_error {
            self.nodes[node].flags.insert(NodeFlags::ThisNodeHasError);
            self.has_parse_error = false;
        }
        self.override_parent_in_immediate_children(node);
    }

    fn override_parent_in_immediate_children(&mut self, node: NodeId) {
        self.current_parent = Some(node);
        self.nodes.for_each_child(node, |child| child.parent = self.current_parent);
        self.current_parent = None;
    }

    fn with_jsdoc(&mut self, node: NodeId, jsdoc: JSDocScannerInfo) -> Vec<NodeId> {
        if !jsdoc.contains(JSDocScannerInfo::HasJSDoc) {
            return Vec::new();
        }

        // For TS/TSX files, defer JSDoc parsing to first access, unless the comment
        // contains @see/@link (needed for unused-identifier checks).
        // @deprecated is detected via cheap text scan to set PossiblyContainsDeprecatedTag;
        // callers must confirm via JSDoc lookup.
        if !self.is_javascript() {
            self.nodes[node].flags.insert(NodeFlags::HasJSDoc);
            if jsdoc.contains(JSDocScannerInfo::HasDeprecated) {
                self.nodes[node].flags.insert(NodeFlags::PossiblyContainsDeprecatedTag);
            }
            if !jsdoc.contains(JSDocScannerInfo::HasSeeOrLink) {
                return Vec::new();
            }
            // Fall through to eager parse for @see/@link
        }

        let ranges = get_jsdoc_comment_ranges(&mut self.nodes, node, &self.scanner.text);
        self.jsdoc_comment_ranges_space = ranges.clone();

        // Should only be called once per node
        self.has_deprecated_tag = false;
        let mut jsdoc = Vec::new();
        let mut pos = self.nodes[node].loc.pos;
        for comment in ranges {
            if let Some(parsed) =
                self.parse_jsdoc_comment(node, comment.range.pos, comment.range.end, pos)
            {
                self.nodes[parsed].parent = Some(node);
                jsdoc.push(parsed);
                pos = self.nodes[parsed].loc.end;
            }
        }

        if !jsdoc.is_empty() {
            self.nodes[node].flags.insert(NodeFlags::JSDoc);
            if self.has_deprecated_tag {
                self.has_deprecated_tag = false;
                self.nodes[node].flags.insert(NodeFlags::PossiblyContainsDeprecatedTag);
            }
            if self.is_javascript() {
                self.reparse_tags(node, &jsdoc);
            }
            self.jsdoc_infos.push(JSDocInfo { parent: node, jsdocs: jsdoc.clone() });
        }
        jsdoc
    }

    fn is_javascript(&self) -> bool {
        matches!(self.script_kind, ScriptKind::JS | ScriptKind::JSX)
    }

    fn parse_jsdoc_comment(
        &self,
        node: NodeId,
        start: TextPos,
        end: TextPos,
        full_start: TextPos,
    ) -> Option<NodeId> {
        todo!()
    }

    fn reparse_tags(&self, parent: NodeId, jsdocs: &[NodeId]) {
        todo!()
    }

    fn parse_token_node(&mut self) -> NodeId {
        let pos = self.node_pos();
        let kind = self.token;
        self.next_token();
        let node = self.nodes.create(kind, ());
        self.finish_node(node, pos)
    }

    fn parse_expected_token(&mut self, kind: SyntaxKind) -> NodeId {
        let token = self.parse_optional_token(kind);
        if let Some(token) = token {
            token
        } else {
            self.parse_error_at_current_token(
                Message::e1005_0_expected(),
                [token_to_text(kind).to_string()],
            );
            let token = self.nodes.create(kind, ());
            self.finish_node(token, self.node_pos())
        }
    }

    fn parse_expected_matching_brackets(
        &mut self,
        open_token: SyntaxKind,
        close_token: SyntaxKind,
        open_parsed: bool,
        open_position: usize,
    ) {
        if self.token == close_token {
            self.next_token();
            return;
        }

        let last_error = self.parse_error_at_current_token(
            Message::e1005_0_expected(),
            [token_to_text(close_token).to_string()],
        );
        if !open_parsed {
            return;
        }
        if let Some(last_error) = last_error {
            self.diagnostics.add_related_info(
                last_error,
                Message::e1007_the_parser_expected_to_find_a_1_to_match_the_0_token_here(),
                TextRange::new(open_position, open_position),
                [token_to_text(open_token).to_string(), token_to_text(close_token).to_string()],
            )
        }
    }

    fn set_context_flags(&mut self, flags: NodeFlags, value: bool) {
        if value {
            self.context_flags.insert(flags);
        } else {
            self.context_flags.remove(flags);
        }
    }

    fn parse_optional(&mut self, token: SyntaxKind) -> bool {
        if self.token == token {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn parse_optional_token(&mut self, token: SyntaxKind) -> Option<NodeId> {
        if self.token == token { Some(self.parse_token_node()) } else { None }
    }

    pub fn parse_statement(&mut self) -> NodeId {
        let token = self.token;
        match token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::OpenBraceToken => self.parse_block(false, None),
            SyntaxKind::VarKeyword => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::LetKeyword if self.is_let_declaration() => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::AwaitKeyword if self.is_await_using_declaration() => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::UsingKeyword if self.is_using_declaration() => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::FunctionKeyword => {
                self.parse_function_declaration(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            SyntaxKind::IfKeyword => self.parse_if_statement(),
            SyntaxKind::DoKeyword => self.parse_do_statement(),
            SyntaxKind::WhileKeyword => self.parse_while_statement(),
            SyntaxKind::ForKeyword => self.parse_for_or_for_in_or_for_of_statement(),
            SyntaxKind::ContinueKeyword => self.parse_continue_statement(),
            SyntaxKind::BreakKeyword => self.parse_break_statement(),
            SyntaxKind::ReturnKeyword => self.parse_return_statement(),
            SyntaxKind::WithKeyword => self.parse_with_statement(),
            SyntaxKind::SwitchKeyword => self.parse_switch_statement(),
            SyntaxKind::ThrowKeyword => self.parse_throw_statement(),
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                self.parse_try_statement()
            }
            SyntaxKind::DebuggerKeyword => self.parse_debugger_statement(),
            SyntaxKind::AtToken => self.parse_declaration(),
            SyntaxKind::AsyncKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::ExportKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::AbstractKeyword
            | SyntaxKind::AccessorKeyword
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::GlobalKeyword
                if self.is_start_of_declaration() =>
            {
                self.parse_declaration()
            }
            _ => self.parse_expression_or_labeled_statement(),
        }
    }

    fn parse_function_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::FunctionKeyword);
        let asterisk_token = self.parse_optional_token(SyntaxKind::AsteriskToken);
        let mut name = None;
        if !self.nodes.has_modifier(&modifiers, ModifierFlags::Default)
            || self.is_binding_identifier()
        {
            name = Some(self.parse_binding_identifier());
        }
        let mut signature_flags = ParseFlags::empty();
        signature_flags.set(ParseFlags::Yield, asterisk_token.is_some());
        signature_flags
            .set(ParseFlags::Await, self.nodes.has_modifier(&modifiers, ModifierFlags::Async));
        let type_parameters = self.parse_type_parameters();
        let save_context_flags = self.context_flags;
        if self.nodes.has_modifier(&modifiers, ModifierFlags::Export) {
            self.set_context_flags(NodeFlags::AwaitContext, true);
        }
        let parameters = self.parse_parameters(signature_flags);
        let return_type = self.parse_return_type(SyntaxKind::ColonToken, false);
        let body = self
            .parse_function_block_or_semicolon(signature_flags, Some(Message::e1144_or_expected()));
        self.context_flags = save_context_flags;
        let node = self.nodes.create(
            SyntaxKind::FunctionDeclaration,
            FunctionDeclaration {
                modifiers,
                asterisk_token,
                name,
                type_parameters,
                parameters,
                return_type,
                full_signature: None,
                body,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_if_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::IfKeyword);
        let open_paren_position = self.scanner.token_start();
        let open_paren_parsed = self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        let then_statement = self.parse_statement();
        let mut else_statement = None;
        if self.parse_optional(SyntaxKind::ElseKeyword) {
            else_statement = Some(self.parse_statement());
        }
        let node = self.nodes.create(
            SyntaxKind::IfStatement,
            IfStatement { expression, then_statement, else_statement },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_do_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::DoKeyword);
        let statement = self.parse_statement();
        self.parse_expected(SyntaxKind::WhileKeyword);
        let open_paren_position = self.scanner.token_start();
        let open_paren_parsed = self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        // From: https://mail.mozilla.org/pipermail/es-discuss/2011-August/016188.html
        // 157 min --- All allen at wirfs-brock.com CONF --- "do{;}while(false)false" prohibited in
        // spec but allowed in consensus reality. Approved -- this is the de-facto standard whereby
        //  do;while(0)x will have a semicolon inserted before x.
        self.parse_optional(SyntaxKind::SemicolonToken);
        let node =
            self.nodes.create(SyntaxKind::DoStatement, DoStatement { statement, expression });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_while_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::WhileKeyword);
        let open_paren_position = self.scanner.token_start();
        let open_paren_parsed = self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );

        let statement = self.parse_statement();
        let node =
            self.nodes.create(SyntaxKind::WhileStatement, WhileStatement { statement, expression });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_for_or_for_in_or_for_of_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::ForKeyword);
        let await_token = self.parse_optional_token(SyntaxKind::AwaitKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let mut initializer = None;
        if self.token != SyntaxKind::SemicolonToken {
            if self.token == SyntaxKind::VarKeyword
                || self.token == SyntaxKind::LetKeyword
                || self.token == SyntaxKind::ConstKeyword
                || self.token == SyntaxKind::UsingKeyword && self.look_ahead(Self::next_token_is_binding_identifier_or_start_of_destructuring_on_same_line_disallow_of)
                || self.token == SyntaxKind::AwaitKeyword && self.look_ahead(Self::next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line)
            {
                initializer = Some(self.parse_variable_declaration_list(true));
            } else {
                initializer = Some(self.in_context(
                    NodeFlags::DisallowInContext,
                    true,
                    Self::parse_expression,
                ));
            }
        }

        let node = if await_token.is_some() && self.parse_expected(SyntaxKind::OfKeyword)
            || await_token.is_none() && self.parse_optional(SyntaxKind::OfKeyword)
        {
            let expression = self.in_context(
                NodeFlags::DisallowInContext,
                false,
                Self::parse_assignment_expression_or_higher,
            );
            self.parse_expected(SyntaxKind::CloseParenToken);
            let statement = self.parse_statement();
            self.nodes.create(
                SyntaxKind::ForOfStatement,
                ForOfStatement { await_modifier: await_token, initializer, expression, statement },
            )
        } else if self.parse_optional(SyntaxKind::InKeyword) {
            let expression = self.parse_expression_allow_in();
            self.parse_expected(SyntaxKind::CloseParenToken);
            let statement = self.parse_statement();
            self.nodes.create(
                SyntaxKind::ForInStatement,
                ForInStatement { initializer, expression, statement },
            )
        } else {
            self.parse_expected(SyntaxKind::SemicolonToken);
            let mut condition = None;
            if self.token != SyntaxKind::SemicolonToken && self.token != SyntaxKind::CloseParenToken
            {
                condition = Some(self.parse_expression_allow_in());
            }
            self.parse_expected(SyntaxKind::SemicolonToken);
            let mut incrementor = None;
            if self.token != SyntaxKind::CloseParenToken {
                incrementor = Some(self.parse_expression_allow_in());
            }
            self.parse_expected(SyntaxKind::CloseParenToken);
            let statement = self.parse_statement();
            self.nodes.create(
                SyntaxKind::ForStatement,
                ForStatement { initializer, condition, incrementor, statement },
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_continue_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::ContinueKeyword);
        let label = self.parse_identifier_unless_at_semicolon();
        self.parse_semicolon();
        let node = self.nodes.create(SyntaxKind::ContinueStatement, ContinueStatement { label });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_break_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::BreakKeyword);
        let label = self.parse_identifier_unless_at_semicolon();
        self.parse_semicolon();
        let node = self.nodes.create(SyntaxKind::BreakStatement, BreakStatement { label });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_return_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::ReturnKeyword);
        let mut expression = None;
        if !self.can_parse_semicolon() {
            expression = Some(self.parse_expression_allow_in());
        }
        self.parse_semicolon();
        let node = self.nodes.create(SyntaxKind::ReturnStatement, ReturnStatement { expression });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_with_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::WithKeyword);
        let open_paren_position = self.scanner.token_start();
        let open_paren_parsed = self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        let statement = self.in_context(NodeFlags::InWithStatement, true, Self::parse_statement);
        let node =
            self.nodes.create(SyntaxKind::WithStatement, WithStatement { expression, statement });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_switch_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::SwitchKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let case_block = self.parse_case_block();
        let node = self
            .nodes
            .create(SyntaxKind::SwitchStatement, SwitchStatement { expression, case_block });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_case_block(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let clauses =
            self.parse_list(ParsingContext::SwitchClauses, Self::parse_case_or_default_clause);
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let node = self.nodes.create(SyntaxKind::CaseBlock, CaseBlock { clauses });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_case_or_default_clause(&mut self) -> NodeId {
        if self.token == SyntaxKind::CaseKeyword {
            self.parse_case_clause()
        } else {
            self.parse_default_clause()
        }
    }

    fn parse_case_clause(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::CaseKeyword);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(SyntaxKind::ColonToken);
        let statements =
            self.parse_list(ParsingContext::SwitchClauseStatements, Self::parse_statement);
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let node = self.nodes.create(SyntaxKind::CaseClause, CaseClause { expression, statements });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_default_clause(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::DefaultKeyword);
        self.parse_expected(SyntaxKind::ColonToken);
        let statements =
            self.parse_list(ParsingContext::SwitchClauseStatements, Self::parse_statement);
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let node = self.nodes.create(SyntaxKind::DefaultClause, DefaultClause { statements });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_throw_statement(&mut self) -> NodeId {
        // ThrowStatement[Yield] :
        //      throw [no LineTerminator here]Expression[In, ?Yield];
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::ThrowKeyword);
        // Because of automatic semicolon insertion, we need to report error if this
        // throw could be terminated with a semicolon.  Note: we can't call 'parseExpression'
        // directly as that might consume an expression on the following line.
        // Instead, we create a "missing" identifier, but don't report an error. The actual error
        // will be reported in the grammar walker.
        let expression = if !self.has_preceding_line_break() {
            self.parse_expression_allow_in()
        } else {
            self.create_missing_identifier()
        };
        if !self.try_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
        }
        let node = self.nodes.create(SyntaxKind::ThrowStatement, ThrowStatement { expression });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_try_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::TryKeyword);
        let try_block = self.parse_block(false, None);
        let mut catch_clause = None;
        if self.token == SyntaxKind::CatchKeyword {
            catch_clause = Some(self.parse_catch_clause());
        }
        // If we don't have a catch clause, then we must have a finally clause.  Try to parse
        // one out no matter what.
        let mut finally_block = None;
        if catch_clause.is_none() || self.token == SyntaxKind::FinallyKeyword {
            self.parse_expected_with_diagnostic(
                SyntaxKind::FinallyKeyword,
                Some(Message::e1472_catch_or_finally_expected()),
                true,
            );
            finally_block = Some(self.parse_block(false, None));
        }
        self.parse_semicolon();
        let node = self.nodes.create(
            SyntaxKind::TryStatement,
            TryStatement { try_block, catch_clause, finally_block },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_catch_clause(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::CatchKeyword);
        let mut variable_declaration = None;
        if self.parse_optional(SyntaxKind::OpenParenToken) {
            variable_declaration = Some(self.parse_variable_declaration());
            self.parse_expected(SyntaxKind::CloseParenToken);
        }
        let block = self.parse_block(false, None);
        let node =
            self.nodes.create(SyntaxKind::CatchClause, CatchClause { variable_declaration, block });
        self.finish_node(node, pos)
    }

    fn parse_debugger_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::DebuggerKeyword);
        self.parse_semicolon();
        let node = self.nodes.create(SyntaxKind::DebuggerStatement, ());
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_declaration(&mut self) -> NodeId {
        // `parseListElement` attempted to get the reused node at this position,
        // but the ambient context flag was not yet set, so the node appeared
        // not reusable in that context.
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_ex(true, false, false);
        let is_ambient = self.nodes.has_modifier(&modifiers, ModifierFlags::Ambient);
        if is_ambient {
            // !!! incremental parsing
            // node := p.tryReuseAmbientDeclaration(pos)
            // if node {
            // 	return node
            // }
            for &m in modifiers.iter().flat_map(|x| x.list.nodes.iter()) {
                self.nodes[m].flags.insert(NodeFlags::Ambient);
            }
            let save_context_flags = self.context_flags;
            self.set_context_flags(NodeFlags::Ambient, true);
            let node = self.parse_declaration_worker(pos, jsdoc, modifiers);
            self.context_flags = save_context_flags;
            node
        } else {
            self.parse_declaration_worker(pos, jsdoc, modifiers)
        }
    }

    fn parse_declaration_worker(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let token = self.token;
        match token {
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::UsingKeyword => self.parse_variable_statement(pos, jsdoc, modifiers),
            SyntaxKind::AwaitKeyword if self.is_await_using_declaration() => {
                self.parse_variable_statement(pos, jsdoc, modifiers)
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(pos, jsdoc, modifiers),
            SyntaxKind::ClassKeyword => self.parse_class_declaration(pos, jsdoc, modifiers),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(pos, jsdoc, modifiers),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(pos, jsdoc, modifiers),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(pos, jsdoc, modifiers),
            SyntaxKind::GlobalKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword => self.parse_module_declaration(pos, jsdoc, modifiers),
            SyntaxKind::ImportKeyword => {
                self.parse_import_declaration_or_import_equals_declaration(pos, jsdoc, modifiers)
            }
            SyntaxKind::ExportKeyword => {
                self.next_token();
                match self.token {
                    SyntaxKind::DefaultKeyword | SyntaxKind::EqualsToken => {
                        self.parse_export_assignment(pos, jsdoc, modifiers)
                    }
                    SyntaxKind::AsKeyword => {
                        self.parse_namespace_export_declaration(pos, jsdoc, modifiers)
                    }
                    _ => self.parse_export_declaration(pos, jsdoc, modifiers),
                }
            }
            _ => {
                if modifiers.is_some() {
                    // We reached this point because we encountered decorators and/or modifiers and assumed a declaration
                    // would follow. For recovery and error reporting purposes, return an incomplete declaration.
                    self.parse_error_at_range(
                        TextRange::new(self.node_pos(), self.node_pos()),
                        Message::e1146_declaration_expected(),
                        [],
                    );
                    let node = self
                        .nodes
                        .create(SyntaxKind::MissingDeclaration, MissingDeclaration { modifiers });
                    self.finish_node(node, pos);
                }
                panic!("Unhandled case in parseDeclarationWorker");
            }
        }
    }

    fn parse_interface_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::InterfaceKeyword);
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        let heritage_clauses = self.parse_heritage_clauses();
        let members = self.parse_object_type_members();
        let node = self.nodes.create(
            SyntaxKind::InterfaceDeclaration,
            InterfaceDeclaration { modifiers, name, type_parameters, heritage_clauses, members },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_type_alias_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::TypeKeyword);
        if self.has_preceding_line_break() {
            self.parse_error_at_current_token(Message::e1142_line_break_not_permitted_here(), []);
        }
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        self.parse_expected(SyntaxKind::EqualsToken);
        let type_node;
        if self.token == SyntaxKind::IntrinsicKeyword && self.look_ahead(Self::next_is_not_dot) {
            type_node = self.parse_keyword_type_node();
        } else {
            type_node = self.parse_type();
        }
        self.parse_semicolon();
        let node = self.nodes.create(
            SyntaxKind::TypeAliasDeclaration,
            TypeAliasDeclaration { modifiers, name, type_parameters, type_node },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn next_is_not_dot(&mut self) -> bool {
        self.next_token() != SyntaxKind::DotToken
    }

    fn parse_enum_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let save_has_await_identifier = self.statement_has_await_identifier;
        self.parse_expected(SyntaxKind::EnumKeyword);
        let name = self.parse_identifier();
        let members;
        if self.parse_expected(SyntaxKind::OpenBraceToken) {
            let save_context_flags = self.context_flags;
            self.set_context_flags(NodeFlags::YieldContext | NodeFlags::AwaitContext, false);
            members = self
                .parse_delimited_list(ParsingContext::EnumMembers, |p| Some(p.parse_enum_member()))
                .unwrap();
            self.context_flags = save_context_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
        } else {
            members = NodeList::missing();
        }
        let node = self
            .nodes
            .create(SyntaxKind::EnumDeclaration, EnumDeclaration { modifiers, name, members });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        self.statement_has_await_identifier = save_has_await_identifier;
        node
    }

    fn parse_enum_member(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let name = self.parse_property_name();
        let initializer =
            self.in_context(NodeFlags::DisallowInContext, false, Self::parse_initializer);
        let node = self.nodes.create(SyntaxKind::EnumMember, EnumMember { name, initializer });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_module_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let mut keyword = SyntaxKind::ModuleKeyword;
        if self.token == SyntaxKind::GlobalKeyword {
            // global augmentation
            return self.parse_ambient_external_module_declaration(pos, jsdoc, modifiers);
        } else if self.parse_optional(SyntaxKind::NamespaceKeyword) {
            keyword = SyntaxKind::NamespaceKeyword;
        } else {
            self.parse_expected(SyntaxKind::ModuleKeyword);
            if self.token == SyntaxKind::StringLiteral {
                return self.parse_ambient_external_module_declaration(pos, jsdoc, modifiers);
            }
        }
        self.parse_module_or_namespace_declaration(pos, jsdoc, modifiers, false, keyword)
    }

    fn parse_ambient_external_module_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let name;
        let mut keyword = SyntaxKind::ModuleKeyword;
        let save_has_await_identifier = self.statement_has_await_identifier;
        if self.token == SyntaxKind::GlobalKeyword {
            // parse 'global' as name of global scope augmentation
            name = self.parse_identifier();
            keyword = SyntaxKind::GlobalKeyword;
        } else {
            // parse string literal
            name = self.parse_literal_expression();
        }
        let mut body = None;
        if self.token == SyntaxKind::OpenBraceToken {
            body = Some(self.parse_module_block());
        } else {
            self.parse_semicolon();
        }
        let node = self.nodes.create(
            SyntaxKind::ModuleDeclaration,
            ModuleDeclaration { modifiers, keyword, name, body },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.statement_has_await_identifier = save_has_await_identifier;
        node
    }

    fn parse_module_block(&mut self) -> NodeId {
        let pos = self.node_pos();
        let statements;
        if self.parse_expected(SyntaxKind::OpenBraceToken) {
            statements = self.parse_list(ParsingContext::BlockStatements, Self::parse_statement);
            self.parse_expected(SyntaxKind::CloseBraceToken);
        } else {
            statements = NodeList::missing();
        }
        let node = self.nodes.create(SyntaxKind::ModuleBlock, ModuleBlock { statements });
        self.finish_node(node, pos)
    }

    fn parse_module_or_namespace_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        nested: bool,
        keyword: SyntaxKind,
    ) -> NodeId {
        let save_has_await_identifier = self.statement_has_await_identifier;
        let name = if nested { self.parse_identifier_name() } else { self.parse_identifier() };
        let body = if self.parse_optional(SyntaxKind::DotToken) {
            let implicit_export = self.nodes.create(SyntaxKind::ExportKeyword, ());
            self.nodes[implicit_export].loc = TextRange::new(self.node_pos(), self.node_pos());
            self.nodes[implicit_export].flags = NodeFlags::Reparsed;
            let implicit_modifiers = self
                .nodes
                .new_modifier_list(vec![implicit_export], self.nodes[implicit_export].loc);
            self.parse_module_or_namespace_declaration(
                self.node_pos(),
                JSDocScannerInfo::empty(),
                Some(implicit_modifiers),
                true,
                keyword,
            )
        } else {
            self.parse_module_block()
        };
        let result = self.nodes.create(
            SyntaxKind::ModuleDeclaration,
            ModuleDeclaration { modifiers, keyword, name, body: Some(body) },
        );
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        self.statement_has_await_identifier = save_has_await_identifier;
        return result;
    }

    fn parse_import_declaration_or_import_equals_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::ImportKeyword);
        let after_import_pos = self.node_pos();
        // We don't parse the identifier here in await context, instead we will report a grammar error in the checker.
        let save_has_await_identifier = self.statement_has_await_identifier;
        let mut identifier = None;
        if self.is_identifier() {
            identifier = Some(self.parse_identifier());
        }
        let mut phase_modifier = SyntaxKind::Unknown;
        if let Some(ident) = identifier
            && self.nodes[ident].data_ref::<Identifier>().text == "type"
            && (self.token != SyntaxKind::FromKeyword
                || self.is_identifier()
                    && self.look_ahead(Self::next_token_is_from_keyword_or_equals_token))
            && (self.is_identifier()
                || self.token_after_import_definitely_produces_import_declaration())
        {
            phase_modifier = SyntaxKind::TypeKeyword;
            identifier = None;
            if self.is_identifier() {
                identifier = Some(self.parse_identifier());
            }
        } else if let Some(ident) = identifier
            && self.nodes[ident].data_ref::<Identifier>().text == "defer"
        {
            let should_parse_as_defer_modifier;
            if self.token == SyntaxKind::FromKeyword {
                should_parse_as_defer_modifier =
                    !self.look_ahead(Self::next_token_is_token_string_literal)
            } else {
                should_parse_as_defer_modifier =
                    self.token != SyntaxKind::CommaToken && self.token != SyntaxKind::EqualsToken
            }
            if should_parse_as_defer_modifier {
                phase_modifier = SyntaxKind::DeferKeyword;
                identifier = None;
                if self.is_identifier() {
                    identifier = Some(self.parse_identifier());
                }
            }
        }
        if identifier.is_some()
            && !self.token_after_imported_identifier_definitely_produces_import_declaration()
            && phase_modifier != SyntaxKind::DeferKeyword
        {
            let import_equals = self.parse_import_equals_declaration(
                pos,
                jsdoc,
                modifiers,
                identifier,
                phase_modifier == SyntaxKind::TypeKeyword,
            );
            self.check_js_syntax(import_equals);
            self.statement_has_await_identifier = save_has_await_identifier; // Import= declaration is always parsed in an Await context, no need to reparse
            return import_equals;
        }
        let import_clause =
            self.try_parse_import_clause(identifier, after_import_pos, phase_modifier, false);
        self.statement_has_await_identifier = save_has_await_identifier; // import clause is always parsed in an Await context
        let module_specifier = self.parse_module_specifier();
        let attributes = self.try_parse_import_attributes();
        self.parse_semicolon();
        let node = self.nodes.create(
            SyntaxKind::ImportDeclaration,
            ImportDeclaration { modifiers, import_clause, module_specifier, attributes },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn next_token_is_from_keyword_or_equals_token(&mut self) -> bool {
        self.next_token();
        self.token == SyntaxKind::FromKeyword || self.token == SyntaxKind::EqualsToken
    }

    fn token_after_import_definitely_produces_import_declaration(&self) -> bool {
        self.token == SyntaxKind::AsteriskToken || self.token == SyntaxKind::OpenBraceToken
    }

    fn next_token_is_token_string_literal(&mut self) -> bool {
        self.next_token() == SyntaxKind::StringLiteral
    }

    fn token_after_imported_identifier_definitely_produces_import_declaration(&self) -> bool {
        // In `import id ___`, the current token decides whether to produce
        // an ImportDeclaration or ImportEqualsDeclaration.
        self.token == SyntaxKind::CommaToken || self.token == SyntaxKind::FromKeyword
    }

    fn parse_import_equals_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        identifier: Option<NodeId>,
        is_type_only: bool,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::EqualsToken);
        let module_reference = self.parse_module_reference();
        self.parse_semicolon();
        let node = self.nodes.create(
            SyntaxKind::ImportEqualsDeclaration,
            ImportEqualsDeclaration { modifiers, is_type_only, identifier, module_reference },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_module_reference(&mut self) -> NodeId {
        if self.token == SyntaxKind::RequireKeyword
            && self.look_ahead(Self::next_token_is_open_paren)
        {
            return self.parse_external_module_reference();
        }
        self.parse_entity_name(false, None)
    }

    fn parse_external_module_reference(&mut self) -> NodeId {
        let save_has_await_identifier = self.statement_has_await_identifier;
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::RequireKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_module_specifier();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let node = self
            .nodes
            .create(SyntaxKind::ExternalModuleReference, ExternalModuleReference { expression });
        self.finish_node(node, pos);
        self.statement_has_await_identifier = save_has_await_identifier;
        node
    }

    fn try_parse_import_clause(
        &mut self,
        identifier: Option<NodeId>,
        pos: usize,
        phase_modifier: SyntaxKind,
        skip_jsdoc_leading_asterisks: bool,
    ) -> Option<NodeId> {
        // ImportDeclaration:
        //  import ImportClause from ModuleSpecifier ;
        //  import ModuleSpecifier;
        if identifier.is_some()
            || self.token == SyntaxKind::AsteriskToken
            || self.token == SyntaxKind::OpenBraceToken
        {
            let import_clause = self.parse_import_clause(
                identifier,
                pos,
                phase_modifier,
                skip_jsdoc_leading_asterisks,
            );
            self.parse_expected(SyntaxKind::FromKeyword);
            Some(import_clause)
        } else {
            None
        }
    }

    fn parse_import_clause(
        &mut self,
        identifier: Option<NodeId>,
        pos: usize,
        phase_modifier: SyntaxKind,
        skip_jsdoc_leading_asterisks: bool,
    ) -> NodeId {
        // ImportClause:
        //  ImportedDefaultBinding
        //  NameSpaceImport
        //  NamedImports
        //  ImportedDefaultBinding, NameSpaceImport
        //  ImportedDefaultBinding, NamedImports
        // If there was no default import or if there is comma token after default import
        // parse namespace or named imports
        let mut named_bindings = None;
        let save_has_await_identifier = self.statement_has_await_identifier;
        if identifier.is_none() || self.parse_optional(SyntaxKind::CommaToken) {
            if skip_jsdoc_leading_asterisks {
                self.scanner.set_skip_jsdoc_leading_asterisks(true);
            }
            if self.token == SyntaxKind::AsteriskToken {
                named_bindings = Some(self.parse_namespace_import());
            } else {
                named_bindings = Some(self.parse_named_imports());
            }
            if skip_jsdoc_leading_asterisks {
                self.scanner.set_skip_jsdoc_leading_asterisks(false);
            }
        }
        let node = self.nodes.create(
            SyntaxKind::ImportClause,
            ImportClause { phase_modifier, identifier, named_bindings },
        );
        self.finish_node(node, pos);
        self.statement_has_await_identifier = save_has_await_identifier;
        node
    }

    fn parse_namespace_import(&mut self) -> NodeId {
        // NameSpaceImport:
        //  * as ImportedBinding
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::AsteriskToken);
        self.parse_expected(SyntaxKind::AsKeyword);
        let name = self.parse_identifier();
        let node = self.nodes.create(SyntaxKind::NamespaceImport, NamespaceImport { name });
        self.finish_node(node, pos)
    }

    fn parse_named_imports(&mut self) -> NodeId {
        let pos = self.node_pos();
        // NamedImports:
        //  { }
        //  { ImportsList }
        //  { ImportsList, }
        let imports = self
            .parse_bracketed_list(
                ParsingContext::ImportOrExportSpecifiers,
                |p| Some(p.parse_import_specifier()),
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
            )
            .unwrap();
        let node = self.nodes.create(SyntaxKind::NamedImports, NamedImports { imports });
        self.finish_node(node, pos)
    }

    fn parse_import_specifier(&mut self) -> NodeId {
        let pos = self.node_pos();
        let (is_type_only, property_name, name) =
            self.parse_import_or_export_specifier(SyntaxKind::ImportSpecifier);
        let identifier_name;
        if self.nodes[name].kind == SyntaxKind::Identifier {
            identifier_name = name;
        } else {
            let loc = self.skip_range_trivia(self.nodes[name].loc);
            self.parse_error_at_range(loc, Message::e1003_identifier_expected(), []);
            identifier_name = self.new_identifier(String::new());
            self.finish_node(identifier_name, self.nodes[name].loc.pos as usize);
        }
        let node = self.nodes.create(
            SyntaxKind::ImportSpecifier,
            ImportSpecifier { is_type_only, property_name, identifier_name },
        );
        self.finish_node(node, pos);
        self.check_js_syntax(node);
        node
    }

    fn parse_import_or_export_specifier(
        &mut self,
        kind: SyntaxKind,
    ) -> (bool, Option<NodeId>, NodeId) {
        // ImportSpecifier:
        //   BindingIdentifier
        //   ModuleExportName as BindingIdentifier
        // ExportSpecifier:
        //   ModuleExportName
        //   ModuleExportName as ModuleExportName
        // let checkIdentifierIsKeyword = isKeyword(token()) && !isIdentifier();
        // let checkIdentifierStart = scanner.getTokenStart();
        // let checkIdentifierEnd = scanner.getTokenEnd();
        let mut can_parse_as_keyword = true;
        let disallow_keywords = kind == SyntaxKind::ImportSpecifier;
        let (mut name, mut name_ok) = self.parse_module_export_name(disallow_keywords);
        let mut is_type_only = false;
        let mut property_name = None;
        if self.nodes[name].kind == SyntaxKind::Identifier
            && self.nodes[name].data_ref::<Identifier>().text == "type"
        {
            // If the first token of an import specifier is 'type', there are a lot of possibilities,
            // especially if we see 'as' afterwards:
            //
            // import { type } from "mod";          - isTypeOnly: false,   name: type
            // import { type as } from "mod";       - isTypeOnly: true,    name: as
            // import { type as as } from "mod";    - isTypeOnly: false,   name: as,    propertyName: type
            // import { type as as as } from "mod"; - isTypeOnly: true,    name: as,    propertyName: as
            if self.token == SyntaxKind::AsKeyword {
                // { type as ...? }
                let first_as = self.parse_identifier_name();
                if self.token == SyntaxKind::AsKeyword {
                    // { type as as ...? }
                    let second_as = self.parse_identifier_name();
                    if self.can_parse_module_export_name() {
                        // { type as as something }
                        // { type as as "something" }
                        is_type_only = true;
                        property_name = Some(first_as);
                        (name, name_ok) = self.parse_module_export_name(disallow_keywords);
                        can_parse_as_keyword = false
                    } else {
                        // { type as as }
                        property_name = Some(name);
                        name = second_as;
                        can_parse_as_keyword = false;
                    }
                } else if self.can_parse_module_export_name() {
                    // { type as something }
                    // { type as "something" }
                    property_name = Some(name);
                    can_parse_as_keyword = false;
                    (name, name_ok) = self.parse_module_export_name(disallow_keywords);
                } else {
                    // { type as }
                    is_type_only = true;
                    name = first_as;
                }
            } else if self.can_parse_module_export_name() {
                // { type something ...? }
                // { type "something" ...? }
                is_type_only = true;
                (name, name_ok) = self.parse_module_export_name(disallow_keywords);
            }
        }
        if can_parse_as_keyword && self.token == SyntaxKind::AsKeyword {
            property_name = Some(name);
            self.parse_expected(SyntaxKind::AsKeyword);
            (name, name_ok) = self.parse_module_export_name(disallow_keywords);
        }

        if !name_ok {
            let loc = self.skip_range_trivia(self.nodes[name].loc);
            self.parse_error_at_range(loc, Message::e1003_identifier_expected(), []);
        }

        (is_type_only, property_name, name)
    }

    fn parse_module_export_name(&mut self, disallow_keywords: bool) -> (NodeId, bool) {
        let mut name_ok = true;
        if self.token == SyntaxKind::StringLiteral {
            return (self.parse_literal_expression(), name_ok);
        }
        if disallow_keywords && self.token.is_keyword() && !self.is_identifier() {
            name_ok = false;
        }
        (self.parse_identifier_name(), name_ok)
    }

    fn can_parse_module_export_name(&mut self) -> bool {
        self.token.is_identifier_or_keyword() || self.token == SyntaxKind::StringLiteral
    }

    fn parse_module_specifier(&mut self) -> NodeId {
        if self.token == SyntaxKind::StringLiteral {
            return self.parse_literal_expression();
        }
        // We allow arbitrary expressions here, even though the grammar only allows string
        // literals.  We check to ensure that it is only a string literal later in the grammar
        // check pass.
        self.parse_expression()
    }

    fn try_parse_import_attributes(&mut self) -> Option<NodeId> {
        if self.token == SyntaxKind::WithKeyword
            || (self.token == SyntaxKind::AssertKeyword && !self.has_preceding_line_break())
        {
            if self.token == SyntaxKind::AssertKeyword {
                self.parse_error_at_current_token(Message::e2880_import_assertions_have_been_replaced_by_import_attributes_use_with_instead_of_assert(), []);
            }
            return Some(self.parse_import_attributes(self.token, false));
        }
        None
    }

    fn parse_export_assignment(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let save_context_flags = self.context_flags;
        let save_has_await_identifier = self.statement_has_await_identifier;
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let mut is_export_equals = false;
        if self.parse_optional(SyntaxKind::EqualsToken) {
            is_export_equals = true;
        } else {
            self.parse_expected(SyntaxKind::DefaultKeyword);
        }
        let expression = self.parse_assignment_expression_or_higher();
        self.parse_semicolon();
        self.context_flags = save_context_flags;
        self.statement_has_await_identifier = save_has_await_identifier;
        let node = self.nodes.create(
            SyntaxKind::ExportAssignment,
            ExportAssignment { modifiers, is_export_equals, type_node: None, expression },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_namespace_export_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::AsKeyword);
        self.parse_expected(SyntaxKind::NamespaceKeyword);
        let save_has_await_identifier = self.statement_has_await_identifier;
        let name = self.parse_identifier();
        self.statement_has_await_identifier = save_has_await_identifier;
        self.parse_semicolon();
        // NamespaceExportDeclaration nodes cannot have decorators or modifiers, we attach them here so we can report them in the grammar checker
        let node = self.nodes.create(
            SyntaxKind::NamespaceExportDeclaration,
            NamespaceExportDeclaration { modifiers, name },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_export_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let save_context_flags = self.context_flags;
        let save_has_await_identifier = self.statement_has_await_identifier;
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let mut export_clause = None;
        let mut module_specifier = None;
        let mut attributes = None;
        let is_type_only = self.parse_optional(SyntaxKind::TypeKeyword);
        let namespace_export_pos = self.node_pos();
        if self.parse_optional(SyntaxKind::AsteriskToken) {
            if self.parse_optional(SyntaxKind::AsKeyword) {
                export_clause = Some(self.parse_namespace_export(namespace_export_pos))
            }
            self.parse_expected(SyntaxKind::FromKeyword);
            module_specifier = Some(self.parse_module_specifier());
        } else {
            export_clause = Some(self.parse_named_exports());
            // It is not uncommon to accidentally omit the 'from' keyword. Additionally, in editing scenarios,
            // the 'from' keyword can be parsed as a named export when the export clause is unterminated (i.e. `export { from "moduleName";`)
            // If we don't have a 'from' keyword, see if we have a string literal such that ASI won't take effect.
            if self.token == SyntaxKind::FromKeyword
                || (self.token == SyntaxKind::StringLiteral && !self.has_preceding_line_break())
            {
                self.parse_expected(SyntaxKind::FromKeyword);
                module_specifier = Some(self.parse_module_specifier());
            }
        }
        if module_specifier.is_some()
            && (self.token == SyntaxKind::WithKeyword || self.token == SyntaxKind::AssertKeyword)
            && !self.has_preceding_line_break()
        {
            if self.token == SyntaxKind::AssertKeyword {
                self.parse_error_at_current_token(Message::e2880_import_assertions_have_been_replaced_by_import_attributes_use_with_instead_of_assert(), []);
            }
            attributes = Some(self.parse_import_attributes(self.token, false));
        }
        self.parse_semicolon();
        self.context_flags = save_context_flags;
        self.statement_has_await_identifier = save_has_await_identifier;
        let node = self.nodes.create(
            SyntaxKind::ExportDeclaration,
            ExportDeclaration {
                modifiers,
                is_type_only,
                export_clause,
                module_specifier,
                attributes,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_namespace_export(&mut self, pos: usize) -> NodeId {
        let (export_name, _) = self.parse_module_export_name(false);
        let node = self.nodes.create(SyntaxKind::NamespaceExport, NamespaceExport { export_name });
        self.finish_node(node, pos)
    }

    fn parse_named_exports(&mut self) -> NodeId {
        let pos = self.node_pos();
        // 	// NamedImports:
        // 	//  { }
        // 	//  { ImportsList }
        // 	//  { ImportsList, }
        let exports = self
            .parse_bracketed_list(
                ParsingContext::ImportOrExportSpecifiers,
                |p| Some(p.parse_export_specifier()),
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
            )
            .unwrap();
        let node = self.nodes.create(SyntaxKind::NamedExports, NamedExports { exports });
        self.finish_node(node, pos)
    }

    fn parse_export_specifier(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let (is_type_only, property_name, name) =
            self.parse_import_or_export_specifier(SyntaxKind::ExportSpecifier);
        let result = self.nodes.create(
            SyntaxKind::ExportSpecifier,
            ExportSpecifier { is_type_only, property_name, name },
        );
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_identifier_unless_at_semicolon(&mut self) -> Option<NodeId> {
        if !self.can_parse_semicolon() { Some(self.parse_identifier()) } else { None }
    }

    fn parse_expression_or_labeled_statement(&mut self) -> NodeId {
        // Avoiding having to do the lookahead for a labeled statement by just trying to parse
        // out an expression, seeing if it is identifier and then seeing if it is followed by
        // a colon.
        let pos = self.node_pos();
        let mut jsdoc = self.jsdoc_scanner_info();
        let has_paren = self.token == SyntaxKind::OpenParenToken;
        let expression = self.parse_expression();

        if self.nodes.is(expression, SyntaxKind::Identifier)
            && self.parse_optional(SyntaxKind::ColonToken)
        {
            let statement = self.parse_statement();
            let node = self
                .nodes
                .create(SyntaxKind::LabeledStatement, LabeledStatement { expression, statement });
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            return node;
        }

        if !self.try_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
        }
        let node =
            self.nodes.create(SyntaxKind::ExpressionStatement, ExpressionStatement { expression });
        self.finish_node(node, pos);
        if has_paren {
            jsdoc.remove(JSDocScannerInfo::HasJSDoc);
        }
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn is_let_declaration(&mut self) -> bool {
        // In ES6 'let' always starts a lexical declaration if followed by an identifier or {
        // or [.
        self.look_ahead(Self::next_token_is_binding_identifier_or_start_of_destructuring)
    }

    fn parse_empty_statement(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::SemicolonToken);
        let node = self.nodes.create(SyntaxKind::EmptyStatement, ());
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_block(
        &mut self,
        ignore_missing_open_brace: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let open_brace_position = self.scanner.token_start();
        let open_brace_parsed = self.parse_expected_with_diagnostic(
            SyntaxKind::OpenBraceToken,
            diagnostic_message,
            true,
        );
        if open_brace_parsed || ignore_missing_open_brace {
            let multiline = self.has_preceding_line_break();
            let statements =
                self.parse_list(ParsingContext::BlockStatements, Self::parse_statement);
            self.parse_expected_matching_brackets(
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
                open_brace_parsed,
                open_brace_position,
            );
            let node = self.nodes.create(SyntaxKind::Block, Block { statements, multiline });
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            if self.token == SyntaxKind::EqualsToken {
                self.parse_error_at_current_token(Message::e2809_declaration_or_statement_expected_this_follows_a_block_of_statements_so_if_you_intended_to_write_a_destructuring_assignment_you_might_need_to_wrap_the_whole_assignment_in_parentheses(), []);
                self.next_token();
            }
            return node;
        }

        let node = self
            .nodes
            .create(SyntaxKind::Block, Block { statements: NodeList::missing(), multiline: false });
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_variable_statement(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let declaration_list = self.parse_variable_declaration_list(false);
        self.parse_semicolon();
        let node = self.nodes.create(
            SyntaxKind::VariableStatement,
            VariableStatement { modifiers, declaration_list },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_semicolon(&mut self) -> bool {
        self.try_parse_semicolon() || self.parse_expected(SyntaxKind::SemicolonToken)
    }

    fn try_parse_semicolon(&mut self) -> bool {
        if !self.can_parse_semicolon() {
            return false;
        }
        if self.token == SyntaxKind::SemicolonToken {
            // consume the semicolon if it was explicitly provided.
            self.next_token();
        }
        true
    }

    fn parse_variable_declaration_list(&mut self, in_for_statement_initializer: bool) -> NodeId {
        let pos = self.node_pos();
        let flags = match self.token {
            SyntaxKind::VarKeyword => NodeFlags::empty(),
            SyntaxKind::LetKeyword => NodeFlags::Let,
            SyntaxKind::ConstKeyword => NodeFlags::Const,
            SyntaxKind::UsingKeyword => NodeFlags::Using,
            SyntaxKind::AwaitKeyword => {
                if !self.is_await_using_declaration() {
                    NodeFlags::empty()
                } else {
                    self.next_token();
                    NodeFlags::AwaitUsing
                }
            }
            _ => unreachable!("Unhandled case in parse_variable_declaration_list"),
        };
        self.next_token();
        // The user may have written the following:
        //
        //    for (let of X) { }
        //
        // In this case, we want to parse an empty declaration list, and then parse 'of'
        // as a keyword. The reason this is not automatic is that 'of' is a valid identifier.
        // So we need to look ahead to determine if 'of' should be treated as a keyword in
        // this context.
        // The checker will then give an error that there is an empty declaration list.
        let declarations = if self.token == SyntaxKind::OfKeyword
            && self.look_ahead(Self::next_is_identifier_and_close_paren)
        {
            NodeList::missing()
        } else {
            let save_context_flags = self.context_flags;
            self.set_context_flags(NodeFlags::DisallowInContext, in_for_statement_initializer);
            let declarations = self
                .parse_delimited_list(ParsingContext::VariableDeclarations, |p| {
                    Some(if in_for_statement_initializer {
                        p.parse_variable_declaration()
                    } else {
                        p.parse_variable_declaration_allow_exclamation()
                    })
                })
                .unwrap();
            self.context_flags = save_context_flags;
            declarations
        };

        let node = self.nodes.create(
            SyntaxKind::VariableDeclarationList,
            VariableDeclarationList { declarations, flags },
        );
        self.finish_node(node, pos)
    }

    fn next_is_identifier_and_close_paren(&mut self) -> bool {
        self.next_token_is_identifier() && self.next_token() == SyntaxKind::CloseParenToken
    }

    fn next_token_is_identifier(&mut self) -> bool {
        self.next_token();
        self.is_identifier()
    }

    fn parse_variable_declaration(&mut self) -> NodeId {
        self.parse_variable_declaration_worker(false)
    }

    fn parse_variable_declaration_allow_exclamation(&mut self) -> NodeId {
        self.parse_variable_declaration_worker(true)
    }

    fn parse_variable_declaration_worker(&mut self, allow_exclamation: bool) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let name = self.parse_identifier_or_pattern_with_diagnostic(Some(
            Message::e18029_private_identifiers_are_not_allowed_in_variable_declarations(),
        ));
        let exclamation_token = if allow_exclamation
            && self.nodes.is(name, SyntaxKind::Identifier)
            && self.token == SyntaxKind::ExclamationToken
            && !self.has_preceding_line_break()
        {
            Some(self.parse_token_node())
        } else {
            None
        };
        let type_annotation = self.parse_type_annotation();
        let initializer = if !matches!(self.token, SyntaxKind::InKeyword | SyntaxKind::OfKeyword) {
            self.parse_initializer()
        } else {
            None
        };
        let node = self.nodes.create(
            SyntaxKind::VariableDeclaration,
            VariableDeclaration {
                name,
                exclamation_token,
                type_node: type_annotation,
                initializer,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn check_js_syntax(&self, node: NodeId) {
        let node = &self.nodes[node];
        if !node.flags.contains(NodeFlags::JavaScriptFile)
            || node.flags.intersects(NodeFlags::JSDoc | NodeFlags::Reparsed)
        {
            return;
        }

        todo!()
    }

    fn parse_identifier_or_pattern_with_diagnostic(
        &mut self,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        match self.token {
            SyntaxKind::OpenBracketToken => self.parse_array_binding_pattern(),
            SyntaxKind::OpenBraceToken => self.parse_object_binding_pattern(),
            _ => {
                self.parse_binding_identifier_with_diagnostic(private_identifier_diagnostic_message)
            }
        }
    }

    fn parse_type_annotation(&mut self) -> Option<NodeId> {
        if self.parse_optional(SyntaxKind::ColonToken) { Some(self.parse_type()) } else { None }
    }

    fn parse_initializer(&mut self) -> Option<NodeId> {
        if self.parse_optional(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression_or_higher())
        } else {
            None
        }
    }

    fn parse_array_binding_pattern(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);
        let save_context_flags = self.parsing_context;
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let elements = self
            .parse_delimited_list(ParsingContext::ArrayBindingElements, |p| {
                Some(p.parse_array_binding_element())
            })
            .unwrap();
        self.parsing_context = save_context_flags;
        self.parse_expected(SyntaxKind::CloseBracketToken);
        let node =
            self.nodes.create(SyntaxKind::ArrayBindingPattern, ArrayBindingPattern { elements });
        self.finish_node(node, pos)
    }

    fn parse_object_binding_pattern(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let save_context_flags = self.parsing_context;
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let elements = self
            .parse_delimited_list(ParsingContext::ObjectBindingElements, |p| {
                Some(p.parse_object_binding_element())
            })
            .unwrap();
        self.parsing_context = save_context_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let node =
            self.nodes.create(SyntaxKind::ObjectBindingPattern, ObjectBindingPattern { elements });
        self.finish_node(node, pos)
    }

    fn parse_optional_binding_identifier(&mut self) -> Option<NodeId> {
        if self.is_binding_identifier() { Some(self.parse_binding_identifier()) } else { None }
    }

    fn parse_binding_identifier(&mut self) -> NodeId {
        self.parse_binding_identifier_with_diagnostic(None)
    }

    fn parse_binding_identifier_with_diagnostic(
        &mut self,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let save_statement_has_await_identifier = self.statement_has_await_identifier;
        let id = self.create_identifier_with_diagnostic(
            self.is_binding_identifier(),
            None,
            private_identifier_diagnostic_message,
        );
        self.statement_has_await_identifier = save_statement_has_await_identifier;
        id
    }

    fn parse_array_binding_element(&mut self) -> NodeId {
        let pos = self.node_pos();
        let mut dot_dot_dot_token = None;
        let mut name = None;
        let mut initializer = None;
        if self.token != SyntaxKind::CommaToken {
            // These are all nil for a missing element
            dot_dot_dot_token = self.parse_optional_token(SyntaxKind::DotDotDotToken);
            name = Some(self.parse_identifier_or_pattern());
            initializer = self.parse_initializer();
        };
        let node = self.nodes.create(
            SyntaxKind::BindingElement,
            BindingElement { dot_dot_dot_token, property_name: None, name, initializer },
        );
        self.finish_node(node, pos)
    }

    fn parse_object_binding_element(&mut self) -> NodeId {
        let pos = self.node_pos();
        let dot_dot_dot_token = self.parse_optional_token(SyntaxKind::DotDotDotToken);
        let token_is_identifier = self.is_binding_identifier();
        let mut property_name = Some(self.parse_property_name());
        let name = if token_is_identifier && self.token != SyntaxKind::ColonToken {
            property_name.take()
        } else {
            self.parse_expected(SyntaxKind::ColonToken);
            Some(self.parse_identifier_or_pattern())
        };
        let initializer = self.parse_initializer();
        let node = self.nodes.create(
            SyntaxKind::BindingElement,
            BindingElement { dot_dot_dot_token, property_name, name, initializer },
        );
        self.finish_node(node, pos)
    }

    fn parse_identifier_or_pattern(&mut self) -> NodeId {
        self.parse_identifier_or_pattern_with_diagnostic(None)
    }

    fn parse_property_name(&mut self) -> NodeId {
        let save_statement_has_await_identifier = self.statement_has_await_identifier;
        let property = self.parse_property_name_worker(true);
        self.statement_has_await_identifier = save_statement_has_await_identifier;
        property
    }

    fn parse_property_name_worker(&mut self, allow_computed_property_names: bool) -> NodeId {
        if matches!(
            self.token,
            SyntaxKind::StringLiteral | SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
        ) {
            return self.parse_literal_expression();
        }

        if allow_computed_property_names && self.token == SyntaxKind::OpenBracketToken {
            return self.parse_computed_property_name();
        }

        if self.token == SyntaxKind::PrivateIdentifier {
            return self.parse_private_identifier();
        }

        self.parse_identifier_name()
    }

    fn create_identifier_with_diagnostic(
        &mut self,
        is_identifier: bool,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        if is_identifier {
            let pos = if self.scanner.has_preceding_jsdoc_leading_asterisks() {
                self.scanner.token_start()
            } else {
                self.node_pos()
            };
            let text = self.scanner.token_value().to_string();
            self.next_token_without_check();
            let node = self.new_identifier(text);
            self.finish_node(node, pos);
            return node;
        }

        if self.token == SyntaxKind::PrivateIdentifier {
            self.parse_error_at_current_token(
                private_identifier_diagnostic_message.unwrap_or(
                    Message::e18016_private_identifiers_are_not_allowed_outside_class_bodies(),
                ),
                [],
            );
            return self.create_identifier(true);
        }

        // Only for end of file because the error gets reported incorrectly on embedded script tags.
        let loc = if self.token == SyntaxKind::EndOfFile {
            let pos = self.scanner.full_token_start();
            TextRange::new(pos, pos)
        } else {
            self.scanner.token_range()
        };
        if let Some(diagnostic_message) = diagnostic_message {
            self.parse_error_at_range(loc, diagnostic_message, []);
        } else if self.token.is_reserved_word() {
            self.parse_error_at_range(
                loc,
                Message::e1359_identifier_expected_0_is_a_reserved_word_that_cannot_be_used_here(),
                [self.scanner.token_text().to_string()],
            );
        } else {
            self.parse_error_at_range(loc, Message::e1003_identifier_expected(), []);
        }

        self.create_missing_identifier()
    }

    fn create_identifier(&mut self, is_identifier: bool) -> NodeId {
        self.create_identifier_with_diagnostic(is_identifier, None, None)
    }

    fn create_missing_identifier(&mut self) -> NodeId {
        let node = self.new_identifier(String::new());
        self.finish_node(node, self.node_pos());
        node
    }

    fn new_identifier(&mut self, text: String) -> NodeId {
        self.identifier_count += 1;
        if text == "await" {
            self.statement_has_await_identifier = true;
        }
        self.nodes.create(SyntaxKind::Identifier, Identifier { text })
    }

    fn parse_literal_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let text = self.scanner.token_value().to_string();
        let token_flags = self.scanner.token_flags();
        let node = match self.token {
            SyntaxKind::StringLiteral => {
                self.nodes.create(self.token, StringLiteral { text, token_flags })
            }
            SyntaxKind::NumericLiteral => {
                self.nodes.create(self.token, NumericLiteral { text, token_flags })
            }
            SyntaxKind::BigIntLiteral => {
                self.nodes.create(self.token, BigIntLiteral { text, token_flags })
            }
            SyntaxKind::RegularExpressionLiteral => {
                self.nodes.create(self.token, RegularExpressionLiteral { text, token_flags })
            }
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                self.nodes.create(self.token, NoSubstitutionTemplateLiteral { text, token_flags })
            }
            _ => unreachable!("Unhandled case in parse_literal_expression"),
        };
        self.next_token();
        self.finish_node(node, pos)
    }

    fn parse_computed_property_name(&mut self) -> NodeId {
        // PropertyName [Yield]:
        //      LiteralPropertyName
        //      ComputedPropertyName[?Yield]
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);
        // We parse any expression (including a comma expression). But the grammar
        // says that only an assignment expression is allowed, so the grammar checker
        // will error if it sees a comma expression.
        let expression = self.parse_expression_allow_in();
        self.parse_expected(SyntaxKind::CloseBracketToken);
        let node = self
            .nodes
            .create(SyntaxKind::ComputedPropertyName, ComputedPropertyName { expression });
        self.finish_node(node, pos)
    }

    fn parse_private_identifier(&mut self) -> NodeId {
        let pos = self.node_pos();
        let text = self.scanner.token_value().to_string();
        self.next_token();
        let node = self.nodes.create(SyntaxKind::PrivateIdentifier, PrivateIdentifier { text });
        self.finish_node(node, pos)
    }

    fn parse_identifier_name(&mut self) -> NodeId {
        self.parse_identifier_name_with_diagnostic(None)
    }

    fn parse_identifier_name_error_on_unicode_escape_sequence(&mut self) -> NodeId {
        if self.scanner.has_unicode_escape() || self.scanner.has_extended_unicode_escape() {
            self.parse_error_at_current_token(
                Message::e17021_unicode_escape_sequence_cannot_appear_here(),
                [],
            );
        }
        self.create_identifier(self.token.is_identifier_or_keyword())
    }

    fn parse_identifier_name_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        self.create_identifier_with_diagnostic(
            self.token.is_identifier_or_keyword(),
            diagnostic_message,
            None,
        )
    }

    fn parse_identifier(&mut self) -> NodeId {
        self.parse_identifier_with_diagnostic(None, None)
    }

    fn parse_identifier_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        self.create_identifier_with_diagnostic(
            self.is_identifier(),
            diagnostic_message,
            private_identifier_diagnostic_message,
        )
    }

    fn parse_expression_allow_in(&mut self) -> NodeId {
        self.in_context(NodeFlags::DisallowInContext, false, Self::parse_expression)
    }

    fn parse_expression(&mut self) -> NodeId {
        // Expression[in]:
        //      AssignmentExpression[in]
        //      Expression[in] , AssignmentExpression[in]

        // clear the decorator context when parsing Expression, as it should be unambiguous when parsing a decorator
        let save_context_flags = self.context_flags;
        self.context_flags.remove(NodeFlags::DecoratorContext);
        let pos = self.node_pos();
        let mut expr = self.parse_assignment_expression_or_higher();
        loop {
            let Some(operator_token) = self.parse_optional_token(SyntaxKind::CommaToken) else {
                break;
            };
            let rhs = self.parse_assignment_expression_or_higher();
            expr = self.make_binary_expression(expr, operator_token, rhs, pos)
        }
        self.context_flags = save_context_flags;
        expr
    }

    fn parse_assignment_expression_or_higher(&mut self) -> NodeId {
        self.parse_assignment_expression_or_higher_worker(true)
    }

    fn parse_assignment_expression_or_higher_worker(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> NodeId {
        //  AssignmentExpression[in,yield]:
        //      1) ConditionalExpression[?in,?yield]
        //      2) LeftHandSideExpression = AssignmentExpression[?in,?yield]
        //      3) LeftHandSideExpression AssignmentOperator AssignmentExpression[?in,?yield]
        //      4) ArrowFunctionExpression[?in,?yield]
        //      5) AsyncArrowFunctionExpression[in,yield,await]
        //      6) [+Yield] YieldExpression[?In]
        //
        // Note: for ease of implementation we treat productions '2' and '3' as the same thing.
        // (i.e. they're both BinaryExpressions with an assignment operator in it).
        // First, do the simple check if we have a YieldExpression (production '6').
        if self.is_yield_expression() {
            return self.parse_yield_expression();
        }

        // Then, check if we have an arrow function (production '4' and '5') that starts with a parenthesized
        // parameter list or is an async arrow function.
        // AsyncArrowFunctionExpression:
        //      1) async[no LineTerminator here]AsyncArrowBindingIdentifier[?Yield][no LineTerminator here]=>AsyncConciseBody[?In]
        //      2) CoverCallExpressionAndAsyncArrowHead[?Yield, ?Await][no LineTerminator here]=>AsyncConciseBody[?In]
        // Production (1) of AsyncArrowFunctionExpression is parsed in "tryParseAsyncSimpleArrowFunctionExpression".
        // And production (2) is parsed in "tryParseParenthesizedArrowFunctionExpression".
        //
        // If we do successfully parse arrow-function, we must *not* recurse for productions 1, 2 or 3. An ArrowFunction is
        // not a LeftHandSideExpression, nor does it start a ConditionalExpression.  So we are done
        // with AssignmentExpression if we see one.
        let arrow_expression = self
            .try_parse_parenthesized_arrow_function_expression(allow_return_type_in_arrow_function);
        if let Some(arrow_expression) = arrow_expression {
            return arrow_expression;
        }

        let arrow_expression = self
            .try_parse_async_simple_arrow_function_expression(allow_return_type_in_arrow_function);
        if let Some(arrow_expression) = arrow_expression {
            return arrow_expression;
        }

        // arrowExpression2 := p.tryParseAsyncSimpleArrowFunctionExpression(allowReturnTypeInArrowFunction)
        // if arrowExpression2 != nil {
        // 	return arrowExpression2
        // }
        // Now try to see if we're in production '1', '2' or '3'.  A conditional expression can
        // start with a LogicalOrExpression, while the assignment productions can only start with
        // LeftHandSideExpressions.
        //
        // So, first, we try to just parse out a BinaryExpression.  If we get something that is a
        // LeftHandSide or higher, then we can try to parse out the assignment expression part.
        // Otherwise, we try to parse out the conditional expression bit.  We want to allow any
        // binary expression here, so we pass in the 'lowest' precedence here so that it matches
        // and consumes anything.
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let expr = self.parse_binary_expression_or_higher(OperatorPrecedence::LOWEST);
        // To avoid a look-ahead, we did not handle the case of an arrow function with a single un-parenthesized
        // parameter ('x => ...') above. We handle it here by checking if the parsed expression was a single
        // identifier and the current token is an arrow.
        if self.nodes.is(expr, SyntaxKind::Identifier)
            && self.token == SyntaxKind::EqualsGreaterThanToken
        {
            return self.parse_simple_arrow_function_expression(
                pos,
                expr,
                allow_return_type_in_arrow_function,
                jsdoc,
                None,
            );
        }

        // Now see if we might be in cases '2' or '3'.
        // If the expression was a LHS expression, and we have an assignment operator, then
        // we're in '2' or '3'. Consume the assignment and return.
        //
        // Note: we call reScanGreaterToken so that we get an appropriately merged token
        // for cases like `> > =` becoming `>>=`
        if self.nodes.is_left_hand_side_expression(expr)
            && self.rescan_greater_than_token().is_assignment_operator()
        {
            let operator_token = self.parse_token_node();
            let right = self
                .parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function);
            return self.make_binary_expression(expr, operator_token, right, pos);
        }

        // It wasn't an assignment or a lambda.  This is a conditional expression:
        self.parse_conditional_expression_rest(expr, pos, allow_return_type_in_arrow_function)
    }

    fn is_yield_expression(&mut self) -> bool {
        if self.token == SyntaxKind::YieldKeyword {
            // If we have a 'yield' keyword, and this is a context where yield expressions are
            // allowed, then definitely parse out a yield expression.
            if self.in_yield_context() {
                return true;
            }

            // We're in a context where 'yield expr' is not allowed.  However, if we can
            // definitely tell that the user was trying to parse a 'yield expr' and not
            // just a normal expr that start with a 'yield' identifier, then parse out
            // a 'yield expr'.  We can then report an error later that they are only
            // allowed in generator expressions.
            //
            // for example, if we see 'yield(foo)', then we'll have to treat that as an
            // invocation expression of something called 'yield'.  However, if we have
            // 'yield foo' then that is not legal as a normal expression, so we can
            // definitely recognize this as a yield expression.
            //
            // for now we just check if the next token is an identifier.  More heuristics
            // can be added here later as necessary.  We just need to make sure that we
            // don't accidentally consume something legal.
            self.look_ahead(Self::next_token_is_identifier_or_keyword_or_literal_on_same_line)
        } else {
            false
        }
    }

    fn parse_yield_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        // YieldExpression[In] :
        //      yield
        //      yield [no LineTerminator here] [Lexical goal InputElementRegExp]AssignmentExpression[?In, Yield]
        //      yield [no LineTerminator here] * [Lexical goal InputElementRegExp]AssignmentExpression[?In, Yield]
        self.next_token();
        let node = if !self.has_preceding_line_break()
            && (self.token == SyntaxKind::AsteriskToken || self.is_start_of_expression())
        {
            let asterisk_token = self.parse_optional_token(SyntaxKind::AsteriskToken);
            let expression = self.parse_assignment_expression_or_higher();
            self.nodes.create(
                SyntaxKind::YieldExpression,
                YieldExpression { asterisk_token, expression: Some(expression) },
            )
        } else {
            // if the next token is not on the same line as yield.  or we don't have an '*' or
            // the start of an expression, then this is just a simple "yield" expression.
            self.nodes.create(
                SyntaxKind::YieldExpression,
                YieldExpression { asterisk_token: None, expression: None },
            )
        };
        self.finish_node(node, pos)
    }

    fn try_parse_parenthesized_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<NodeId> {
        let result = self.is_parenthesized_arrow_function_expression();
        match result {
            Some(false) => {
                // It's definitely not a parenthesized arrow function expression.
                None
            }
            Some(true) => self.parse_parenthesized_arrow_function_expression(true, true),
            None => {
                let state = self.mark();
                let result = self.parse_possible_parenthesized_arrow_function_expression(
                    allow_return_type_in_arrow_function,
                );
                if result.is_none() {
                    self.rewind(state);
                }
                result
            }
        }
    }

    fn try_parse_async_simple_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<NodeId> {
        // We do a check here so that we won't be doing unnecessarily call to "lookAhead"
        if self.token == SyntaxKind::AsyncKeyword
            && self.look_ahead(Self::next_is_unparenthesized_async_arrow_function)
        {
            let pos = self.node_pos();
            let jsdoc = self.jsdoc_scanner_info();
            let async_modifier = self.parse_modifiers_for_arrow_function();
            let expr = self.parse_binary_expression_or_higher(OperatorPrecedence::LOWEST);
            Some(self.parse_simple_arrow_function_expression(
                pos,
                expr,
                allow_return_type_in_arrow_function,
                jsdoc,
                async_modifier,
            ))
        } else {
            None
        }
    }

    fn next_is_unparenthesized_async_arrow_function(&mut self) -> bool {
        // AsyncArrowFunctionExpression:
        //      1) async[no LineTerminator here]AsyncArrowBindingIdentifier[?Yield][no LineTerminator here]=>AsyncConciseBody[?In]
        //      2) CoverCallExpressionAndAsyncArrowHead[?Yield, ?Await][no LineTerminator here]=>AsyncConciseBody[?In]
        if self.token == SyntaxKind::AsyncKeyword {
            self.next_token();
            // If the "async" is followed by "=>" token then it is not a beginning of an async arrow-function
            // but instead a simple arrow-function which will be parsed inside "parseAssignmentExpressionOrHigher"
            if self.has_preceding_line_break() || self.token == SyntaxKind::EqualsGreaterThanToken {
                return false;
            }
            // Check for un-parenthesized AsyncArrowFunction
            if !self.is_identifier() {
                return false;
            }
            self.next_token_without_check();
            return !self.has_preceding_line_break()
                && self.token == SyntaxKind::EqualsGreaterThanToken;
        }

        false
    }

    fn parse_parenthesized_arrow_function_expression(
        &mut self,
        allow_ambiguity: bool,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<NodeId> {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_for_arrow_function();
        let is_async = self.nodes.has_modifier(&modifiers, ModifierFlags::Async);
        let signature_flags = if is_async { ParseFlags::Await } else { ParseFlags::empty() };
        // Arrow functions are never generators.
        //
        // If we're speculatively parsing a signature for a parenthesized arrow function, then
        // we have to have a complete parameter list.  Otherwise we might see something like
        // a => (b => c)
        // And think that "(b =>" was actually a parenthesized arrow function with a missing
        // close paren.
        let type_parameters = self.parse_type_parameters();
        let parameters;
        if !self.parse_expected(SyntaxKind::OpenParenToken) {
            if !allow_ambiguity {
                return None;
            }
            parameters = Some(NodeList::missing());
        } else {
            if !allow_ambiguity {
                let Some(params) = self.parse_parameters_worker(signature_flags, allow_ambiguity)
                else {
                    return None;
                };
                parameters = Some(params);
            } else {
                parameters = self.parse_parameters_worker(signature_flags, allow_ambiguity);
            }
            if !self.parse_expected(SyntaxKind::CloseParenToken) && !allow_ambiguity {
                return None;
            }
        }
        let has_return_colon = self.token == SyntaxKind::ColonToken;
        let return_type = self.parse_return_type(SyntaxKind::ColonToken /*isType*/, false);
        if let Some(return_type) = return_type
            && !allow_ambiguity
            && self.type_has_arrow_function_blocking_parse_error(return_type)
        {
            return None;
        }
        // Parsing a signature isn't enough.
        // Parenthesized arrow signatures often look like other valid expressions.
        // For instance:
        //  - "(x = 10)" is an assignment expression parsed as a signature with a default parameter value.
        //  - "(x,y)" is a comma expression parsed as a signature with two parameters.
        //  - "a ? (b): c" will have "(b):" parsed as a signature with a return type annotation.
        //  - "a ? (b): function() {}" will too, since function() is a valid JSDoc function type.
        //  - "a ? (b): (function() {})" as well, but inside of a parenthesized type with an arbitrary amount of nesting.
        //
        // So we need just a bit of lookahead to ensure that it can only be a signature.
        let mut unwrapped_type = return_type;
        while let Some(inner) = unwrapped_type
            && self.nodes.is(inner, SyntaxKind::ParenthesizedType)
        {
            unwrapped_type = Some(self.nodes[inner].data_ref::<ParenthesizedType>().type_node); // Skip parens if need be
        }
        if !allow_ambiguity
            && self.token != SyntaxKind::EqualsGreaterThanToken
            && self.token != SyntaxKind::OpenBraceToken
        {
            // Returning undefined here will cause our caller to rewind to where we started from.
            return None;
        }
        // If we have an arrow, then try to parse the body. Even if not, try to parse if we
        // have an opening brace, just in case we're in an error state.
        let last_token = self.token;
        let equals_greater_than_token =
            self.parse_expected_token(SyntaxKind::EqualsGreaterThanToken);
        let body = if last_token == SyntaxKind::EqualsGreaterThanToken
            || last_token == SyntaxKind::OpenBraceToken
        {
            self.parse_arrow_function_expression_body(is_async, allow_return_type_in_arrow_function)
        } else {
            self.parse_identifier()
        };
        // Given:
        //     x ? y => ({ y }) : z => ({ z })
        // We try to parse the body of the first arrow function by looking at:
        //     ({ y }) : z => ({ z })
        // This is a valid arrow function with "z" as the return type.
        //
        // But, if we're in the true side of a conditional expression, this colon
        // terminates the expression, so we cannot allow a return type if we aren't
        // certain whether or not the preceding text was parsed as a parameter list.
        //
        // For example,
        //     a() ? (b: number, c?: string): void => d() : e
        // is determined by isParenthesizedArrowFunctionExpression to unambiguously
        // be an arrow expression, so we allow a return type.
        if !allow_return_type_in_arrow_function && has_return_colon {
            // However, if the arrow function we were able to parse is followed by another colon
            // as in:
            //     a ? (x): string => x : null
            // Then allow the arrow function, and treat the second colon as terminating
            // the conditional expression. It's okay to do this because this code would
            // be a syntax error in JavaScript (as the second colon shouldn't be there).
            if self.token != SyntaxKind::ColonToken {
                return None;
            }
        }
        let node = self.nodes.create(
            SyntaxKind::ArrowFunction,
            ArrowFunction {
                modifiers,
                type_parameters,
                parameters,
                return_type,
                full_signature: None,
                equals_greater_than_token,
                body,
            },
        );
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        Some(result)
    }

    /// If true, we should abort parsing an error function.
    fn type_has_arrow_function_blocking_parse_error(&self, node: NodeId) -> bool {
        let node = &self.nodes[node];
        match node.kind {
            SyntaxKind::TypeReference => {
                let type_name = node.data_ref::<TypeReference>().type_name;
                self.nodes[type_name].is_missing()
            }
            SyntaxKind::FunctionType => {
                let function = node.data_ref::<FunctionType>();
                function.parameters.as_ref().is_some_and(|x| x.is_missing())
                    || function
                        .return_type
                        .is_some_and(|t| self.type_has_arrow_function_blocking_parse_error(t))
            }
            SyntaxKind::ConstructorType => {
                let constructor = node.data_ref::<ConstructorType>();
                constructor.parameters.as_ref().is_some_and(|x| x.is_missing())
                    || constructor
                        .return_type
                        .is_some_and(|t| self.type_has_arrow_function_blocking_parse_error(t))
            }
            SyntaxKind::ParenthesizedType => {
                let type_node = node.data_ref::<ParenthesizedType>().type_node;
                self.type_has_arrow_function_blocking_parse_error(type_node)
            }
            _ => false,
        }
    }

    fn parse_arrow_function_expression_body(
        &mut self,
        is_async: bool,
        allow_return_type_in_arrow_function: bool,
    ) -> NodeId {
        if self.token == SyntaxKind::OpenBraceToken {
            return self.parse_function_block(
                if is_async { ParseFlags::Await } else { ParseFlags::empty() },
                None, /*diagnosticMessage*/
            );
        }
        if self.token != SyntaxKind::SemicolonToken
            && self.token != SyntaxKind::FunctionKeyword
            && self.token != SyntaxKind::ClassKeyword
            && self.is_start_of_statement()
            && !self.is_start_of_expression_statement()
        {
            // Check if we got a plain statement (i.e. no expression-statements, no function/class expressions/declarations)
            //
            // Here we try to recover from a potential error situation in the case where the
            // user meant to supply a block. For example, if the user wrote:
            //
            //  a =>
            //      let v = 0;
            //  }
            //
            // they may be missing an open brace.  Check to see if that's the case so we can
            // try to recover better.  If we don't do this, then the next close curly we see may end
            // up preemptively closing the containing construct.
            //
            // Note: even when 'IgnoreMissingOpenBrace' is passed, parseBody will still error.
            return self.parse_function_block(
                ParseFlags::IgnoreMissingOpenBrace
                    | if is_async { ParseFlags::Await } else { ParseFlags::empty() },
                None, /*diagnosticMessage*/
            );
        }
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::AwaitContext, is_async);
        self.set_context_flags(NodeFlags::YieldContext, false);
        let node =
            self.parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function);
        self.context_flags = save_context_flags;
        node
    }

    fn is_start_of_expression_statement(&mut self) -> bool {
        // As per the grammar, none of '{' or 'function' or 'class' can start an expression statement.
        !matches!(
            self.token,
            SyntaxKind::OpenBraceToken
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::AtToken
        ) && self.is_start_of_expression()
    }

    fn parse_function_block(
        &mut self,
        flags: ParseFlags,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let save_context_flags = self.context_flags;
        let save_has_await_identifier = self.statement_has_await_identifier;
        self.set_context_flags(NodeFlags::YieldContext, flags.contains(ParseFlags::Yield));
        self.set_context_flags(NodeFlags::AwaitContext, flags.contains(ParseFlags::Await));
        // We may be in a [Decorator] context when parsing a function expression or
        // arrow function. The body of the function is not in [Decorator] context.
        self.set_context_flags(NodeFlags::DecoratorContext, false);
        let block = self
            .parse_block(flags.contains(ParseFlags::IgnoreMissingOpenBrace), diagnostic_message);
        self.context_flags = save_context_flags;
        self.statement_has_await_identifier = save_has_await_identifier;
        block
    }

    fn parse_possible_parenthesized_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<NodeId> {
        let token_pos = self.scanner.token_start();
        if self.not_parenthesized_arrow.contains(&token_pos) {
            return None;
        }
        let result = self.parse_parenthesized_arrow_function_expression(
            false,
            allow_return_type_in_arrow_function,
        );
        if result.is_none() {
            self.not_parenthesized_arrow.insert(token_pos);
        }
        result
    }

    fn parse_simple_arrow_function_expression(
        &mut self,
        pos: usize,
        identifier: NodeId,
        allow_return_type_in_arrow_function: bool,
        jsdoc: JSDocScannerInfo,
        async_modifier: Option<ModifierList>,
    ) -> NodeId {
        debug_assert_eq!(
            self.token,
            SyntaxKind::EqualsGreaterThanToken,
            "parse_simple_arrow_function_expression should only have been called if we had a =>"
        );
        let parameter = self.nodes.create(
            SyntaxKind::Parameter,
            Parameter {
                modifiers: None,
                dot_dot_dot_token: None,
                name: identifier,
                question_token: None,
                initializer: None,
                type_node: None,
            },
        );
        self.finish_node(parameter, self.nodes[identifier].loc.pos as usize);
        let parameters = NodeList { loc: self.nodes[parameter].loc, nodes: vec![parameter] };
        let equals_greater_than_token =
            self.parse_expected_token(SyntaxKind::EqualsGreaterThanToken);
        let body = self.parse_arrow_function_expression_body(
            async_modifier.is_some(),
            allow_return_type_in_arrow_function,
        );
        let node = self.nodes.create(
            SyntaxKind::ArrowFunction,
            ArrowFunction {
                modifiers: async_modifier,
                type_parameters: None,
                parameters: Some(parameters),
                return_type: None,
                full_signature: None,
                equals_greater_than_token,
                body,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn is_parenthesized_arrow_function_expression(&mut self) -> Option<bool> {
        if matches!(
            self.token,
            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken | SyntaxKind::AsyncKeyword
        ) {
            let state = self.mark();
            let result = self.next_is_parenthesized_arrow_function_expression();
            self.rewind(state);
            return result;
        }

        if self.token == SyntaxKind::EqualsGreaterThanToken {
            // ERROR RECOVERY TWEAK:
            // If we see a standalone => try to parse it as an arrow function expression as that's
            // likely what the user intended to write.
            return Some(true);
        }

        // Definitely not a parenthesized arrow function.
        Some(false)
    }

    fn next_is_parenthesized_arrow_function_expression(&mut self) -> Option<bool> {
        if self.token == SyntaxKind::AsyncKeyword {
            self.next_token();
            if self.has_preceding_line_break() {
                return Some(false);
            }
            if self.token != SyntaxKind::OpenParenToken && self.token != SyntaxKind::LessThanToken {
                return Some(false);
            }
        }
        let first = self.token;
        let second = self.next_token();
        if first == SyntaxKind::OpenParenToken {
            if second == SyntaxKind::CloseParenToken {
                // Simple cases: "() =>", "(): ", and "() {".
                // This is an arrow function with no parameters.
                // The last one is not actually an arrow function,
                // but this is probably what the user intended.
                let third = self.next_token();
                let x = matches!(
                    third,
                    SyntaxKind::EqualsGreaterThanToken
                        | SyntaxKind::ColonToken
                        | SyntaxKind::OpenBraceToken
                );
                return Some(x);
            }
            // If encounter "([" or "({", this could be the start of a binding pattern.
            // Examples:
            //      ([ x ]) => { }
            //      ({ x }) => { }
            //      ([ x ])
            //      ({ x })
            if second == SyntaxKind::OpenBracketToken || second == SyntaxKind::OpenBraceToken {
                return None;
            }
            // Simple case: "(..."
            // This is an arrow function with a rest parameter.
            if second == SyntaxKind::DotDotDotToken {
                return Some(true);
            }
            // Check for "(xxx yyy", where xxx is a modifier and yyy is an identifier. This
            // isn't actually allowed, but we want to treat it as a lambda so we can provide
            // a good error message.
            if second.is_modifier()
                && second != SyntaxKind::AsyncKeyword
                && self.look_ahead(Self::next_token_is_identifier)
            {
                if self.next_token() == SyntaxKind::AsKeyword {
                    // https://github.com/microsoft/TypeScript/issues/44466
                    return Some(false);
                }
                return Some(true);
            }
            // If we had "(" followed by something that's not an identifier,
            // then this definitely doesn't look like a lambda.  "this" is not
            // valid, but we want to parse it and then give a semantic error.
            if !self.is_identifier() && second != SyntaxKind::ThisKeyword {
                return Some(false);
            }
            match self.next_token() {
                SyntaxKind::ColonToken => {
                    // If we have something like "(a:", then we must have a
                    // type-annotated parameter in an arrow function expression.
                    return Some(true);
                }
                SyntaxKind::QuestionToken => {
                    self.next_token();
                    // If we have "(a?:" or "(a?," or "(a?=" or "(a?)" then it is definitely a lambda.
                    if self.token == SyntaxKind::ColonToken
                        || self.token == SyntaxKind::CommaToken
                        || self.token == SyntaxKind::EqualsToken
                        || self.token == SyntaxKind::CloseParenToken
                    {
                        return Some(true);
                    }
                    // Otherwise it is definitely not a lambda.
                    return Some(false);
                }
                SyntaxKind::CommaToken | SyntaxKind::EqualsToken | SyntaxKind::CloseParenToken => {
                    // If we have "(a," or "(a=" or "(a)" this *could* be an arrow function
                    return None;
                }
                _ => {
                    // It is definitely not an arrow function
                    return Some(false);
                }
            }
        } else {
            debug_assert_eq!(first, SyntaxKind::LessThanToken);
            // If we have "<" not followed by an identifier,
            // then this definitely is not an arrow function.
            if !self.is_identifier() && self.token != SyntaxKind::ConstKeyword {
                return Some(false);
            }
            // JSX overrides
            if self.language_variant == LanguageVariant::JSX {
                let is_arrow_function_in_jsx = self.look_ahead(|p| {
                    p.parse_optional(SyntaxKind::ConstKeyword);
                    let third = p.next_token();
                    if third == SyntaxKind::ExtendsKeyword {
                        let fourth = p.next_token();
                        !matches!(
                            fourth,
                            SyntaxKind::EqualsToken
                                | SyntaxKind::GreaterThanToken
                                | SyntaxKind::SlashToken
                        )
                    } else if matches!(third, SyntaxKind::CommaToken | SyntaxKind::EqualsToken) {
                        true
                    } else {
                        false
                    }
                });
                if is_arrow_function_in_jsx {
                    return Some(true);
                }
                return Some(false);
            }
            // This *could* be a parenthesized arrow function.
            None
        }
    }

    fn parse_binary_expression_or_higher(&mut self, precedence: OperatorPrecedence) -> NodeId {
        let pos = self.node_pos();
        let left_operand = self.parse_unary_expression_or_higher();
        self.parse_binary_expression_rest(precedence, left_operand, pos)
    }

    fn parse_binary_expression_rest(
        &mut self,
        precedence: OperatorPrecedence,
        mut left_operand: NodeId,
        pos: usize,
    ) -> NodeId {
        let mut last_operand = left_operand;
        loop {
            // We either have a binary operator here, or we're finished.  We call
            // reScanGreaterToken so that we merge token sequences like > and = into >=
            let operator = self.rescan_greater_than_token();
            let new_precedence = operator.binary_operator_precedence();
            // Check the precedence to see if we should "take" this operator
            // - For left associative operator (all operator but **), consume the operator,
            //   recursively call the function below, and parse binaryExpression as a rightOperand
            //   of the caller if the new precedence of the operator is greater then or equal to the current precedence.
            //   For example:
            //      a - b - c;
            //            ^token; left_operand = b. Return b to the caller as a rightOperand
            //      a * b - c
            //            ^token; left_operand = b. Return b to the caller as a rightOperand
            //      a - b * c;
            //            ^token; left_operand = b. Return b * c to the caller as a rightOperand
            // - For right associative operator (**), consume the operator, recursively call the function
            //   and parse binaryExpression as a rightOperand of the caller if the new precedence of
            //   the operator is strictly grater than the current precedence
            //   For example:
            //      a ** b ** c;
            //             ^^token; left_operand = b. Return b ** c to the caller as a rightOperand
            //      a - b ** c;
            //            ^^token; left_operand = b. Return b ** c to the caller as a rightOperand
            //      a ** b - c
            //             ^token; left_operand = b. Return b to the caller as a rightOperand
            let consume_current_operator = if operator == SyntaxKind::AsteriskAsteriskToken {
                new_precedence >= precedence
            } else {
                new_precedence > precedence
            };
            if !consume_current_operator {
                break;
            }
            if operator == SyntaxKind::InKeyword && self.in_disallow_in_context() {
                break;
            }
            if operator == SyntaxKind::AsKeyword || operator == SyntaxKind::SatisfiesKeyword {
                // Make sure we *do* perform ASI for constructs like this:
                //    var x = foo
                //    as (Bar)
                // This should be parsed as an initialized variable, followed
                // by a function call to 'as' with the argument 'Bar'
                if self.has_preceding_line_break() {
                    break;
                } else {
                    self.next_token();
                    // When we have 'a ## b as SomeType' or 'a ## b satisfies SomeType', where ## is some binary
                    // operator, we want to stop parsing on any following operator with a higher precedence than ##
                    // because continuing would make it impossible to erase the `as` or `satisfies` without changing
                    // the meaning of the expression. See https://github.com/microsoft/TypeScript/issues/63527.
                    let mut last_precedence = OperatorPrecedence::HIGHEST;
                    if self.nodes.is(last_operand, SyntaxKind::BinaryExpression) {
                        let operator_token =
                            self.nodes[last_operand].data_ref::<BinaryExpression>().operator_token;
                        last_precedence =
                            self.nodes[operator_token].kind.binary_operator_precedence();
                    }
                    let type_node = self.parse_type();
                    left_operand = if operator == SyntaxKind::SatisfiesKeyword {
                        self.make_satisfies_expression(left_operand, type_node)
                    } else {
                        self.make_as_expression(left_operand, type_node)
                    };
                    // Stop if the precedence of the next operator is too high.
                    if self.rescan_greater_than_token().binary_operator_precedence()
                        > last_precedence
                    {
                        break;
                    }
                }
            } else {
                let operator = self.parse_token_node();
                let right = self.parse_binary_expression_or_higher(new_precedence);
                left_operand = self.make_binary_expression(left_operand, operator, right, pos);
                last_operand = left_operand
            }
        }
        return left_operand;
    }

    fn parse_conditional_expression_rest(
        &mut self,
        left_operand: NodeId,
        pos: usize,
        allow_return_type_in_arrow_function: bool,
    ) -> NodeId {
        // Note: we are passed in an expression which was produced from parseBinaryExpressionOrHigher.
        let Some(question_token) = self.parse_optional_token(SyntaxKind::QuestionToken) else {
            return left_operand;
        };

        // Note: we explicitly 'allowIn' in the whenTrue part of the condition expression, and
        // we do not that for the 'whenFalse' part.
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let when_true = self.parse_assignment_expression_or_higher_worker(false);
        self.context_flags = save_context_flags;
        let colon_token = self.parse_expected_token(SyntaxKind::ColonToken);
        let when_false = if self.nodes[colon_token].is_present() {
            self.parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function)
        } else {
            self.create_missing_identifier()
        };
        let node = self.nodes.create(
            SyntaxKind::ConditionalExpression,
            ConditionalExpression {
                condition: left_operand,
                question_token,
                when_true,
                colon_token,
                when_false,
            },
        );
        self.finish_node(node, pos)
    }

    fn parse_unary_expression_or_higher(&mut self) -> NodeId {
        // ES7 UpdateExpression:
        //      1) LeftHandSideExpression[?Yield]
        //      2) LeftHandSideExpression[?Yield][no LineTerminator here]++
        //      3) LeftHandSideExpression[?Yield][no LineTerminator here]--
        //      4) ++UnaryExpression[?Yield]
        //      5) --UnaryExpression[?Yield]
        if self.is_update_expression() {
            let pos = self.node_pos();
            let update_expression = self.parse_update_expression();
            if self.token == SyntaxKind::AsteriskAsteriskToken {
                return self.parse_binary_expression_rest(
                    self.token.binary_operator_precedence(),
                    update_expression,
                    pos,
                );
            }
            return update_expression;
        }

        // ES7 UnaryExpression:
        //      1) UpdateExpression[?yield]
        //      2) delete UpdateExpression[?yield]
        //      3) void UpdateExpression[?yield]
        //      4) typeof UpdateExpression[?yield]
        //      5) + UpdateExpression[?yield]
        //      6) - UpdateExpression[?yield]
        //      7) ~ UpdateExpression[?yield]
        //      8) ! UpdateExpression[?yield]
        let unary_operator = self.token;
        let simple_unary_expression = self.parse_simple_unary_expression();
        if self.token == SyntaxKind::AsteriskAsteriskToken {
            let pos = Scanner::skip_trivia(
                &self.scanner.text,
                self.nodes[simple_unary_expression].loc.pos as usize,
            );
            let end = self.nodes[simple_unary_expression].loc.end as usize;
            if self.nodes[simple_unary_expression].kind == SyntaxKind::TypeAssertionExpression {
                self.parse_error_at_range(TextRange::new(pos, end), Message::e17007_a_type_assertion_expression_is_not_allowed_in_the_left_hand_side_of_an_exponentiation_expression_consider_enclosing_the_expression_in_parentheses(), []);
            } else {
                debug_assert!(unary_operator.is_keyword_or_punctuation());
                self.parse_error_at_range(TextRange::new(pos, end), Message::e17006_an_unary_expression_with_the_0_operator_is_not_allowed_in_the_left_hand_side_of_an_exponentiation_expression_consider_enclosing_the_expression_in_parentheses(), [token_to_text(unary_operator).to_string()]);
            }
        }
        simple_unary_expression
    }

    fn parse_simple_unary_expression(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken => self.parse_prefix_unary_expression(),
            SyntaxKind::DeleteKeyword => self.parse_delete_expression(),
            SyntaxKind::TypeOfKeyword => self.parse_typeof_expression(),
            SyntaxKind::VoidKeyword => self.parse_void_expression(),
            SyntaxKind::LessThanToken => {
                // Just like in parseUpdateExpression, we need to avoid parsing type assertions when
                // in JSX and we see an expression like "+ <foo> bar".
                if self.language_variant == LanguageVariant::JSX {
                    todo!()
                }
                // This is modified UnaryExpression grammar in TypeScript
                //  UnaryExpression (modified):
                //      < type > UnaryExpression
                self.parse_type_assertion()
            }
            SyntaxKind::AwaitKeyword => {
                if self.is_await_expression() {
                    self.parse_await_expression()
                } else {
                    self.parse_update_expression()
                }
            }
            _ => self.parse_update_expression(),
        }
    }

    fn parse_prefix_unary_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let operator = self.token;
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(
            SyntaxKind::PrefixUnaryExpression,
            PrefixUnaryExpression { operator, expression },
        );
        self.finish_node(node, pos)
    }

    fn parse_delete_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(SyntaxKind::DeleteExpression, DeleteExpression { expression });
        self.finish_node(node, pos)
    }

    fn parse_typeof_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(SyntaxKind::TypeOfExpression, TypeOfExpression { expression });
        self.finish_node(node, pos)
    }

    fn parse_void_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(SyntaxKind::VoidExpression, VoidExpression { expression });
        self.finish_node(node, pos)
    }

    fn parse_type_assertion(&mut self) -> NodeId {
        debug_assert_ne!(self.language_variant, LanguageVariant::JSX);
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::LessThanToken);
        let type_node = self.parse_type();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(
            SyntaxKind::TypeAssertionExpression,
            TypeAssertionExpression { type_node, expression },
        );
        self.finish_node(node, pos)
    }

    fn parse_await_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.nodes.create(SyntaxKind::AwaitExpression, AwaitExpression { expression });
        self.finish_node(node, pos)
    }

    fn is_await_expression(&mut self) -> bool {
        if self.token == SyntaxKind::AwaitKeyword {
            if self.in_await_context() {
                return true;
            }
            // here we are using similar heuristics as 'isYieldExpression'
            self.look_ahead(Self::next_token_is_identifier_or_keyword_or_literal_on_same_line)
        } else {
            false
        }
    }

    fn parse_update_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        if matches!(self.token, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken) {
            let operator = self.token;
            self.next_token();
            let expression = self.parse_left_hand_side_expression_or_higher();
            let node = self.nodes.create(
                SyntaxKind::PrefixUnaryExpression,
                PrefixUnaryExpression { operator, expression },
            );
            return self.finish_node(node, pos);
        }

        if self.language_variant == LanguageVariant::JSX
            && self.token == SyntaxKind::LessThanToken
            && self.look_ahead(Self::next_token_is_identifier_or_keyword_or_greater_than)
        {
            todo!()
        }

        let expression = self.parse_left_hand_side_expression_or_higher();
        if matches!(self.token, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
            && !self.has_preceding_line_break()
        {
            let operator = self.token;
            self.next_token();
            let node = self.nodes.create(
                SyntaxKind::PostfixUnaryExpression,
                PostfixUnaryExpression { expression, operator },
            );
            return self.finish_node(node, pos);
        }
        expression
    }

    fn parse_left_hand_side_expression_or_higher(&mut self) -> NodeId {
        // Original Ecma:
        // LeftHandSideExpression: See 11.2
        //      NewExpression
        //      CallExpression
        //
        // Our simplification:
        //
        // LeftHandSideExpression: See 11.2
        //      MemberExpression
        //      CallExpression
        //
        // See comment in parseMemberExpressionOrHigher on how we replaced NewExpression with
        // MemberExpression to make our lives easier.
        //
        // to best understand the below code, it's important to see how CallExpression expands
        // out into its own productions:
        //
        // CallExpression:
        //      MemberExpression Arguments
        //      CallExpression Arguments
        //      CallExpression[Expression]
        //      CallExpression.IdentifierName
        //      import (AssignmentExpression)
        //      super Arguments
        //      super.IdentifierName
        //
        // Because of the recursion in these calls, we need to bottom out first. There are three
        // bottom out states we can run into: 1) We see 'super' which must start either of
        // the last two CallExpression productions. 2) We see 'import' which must start import call.
        // 3)we have a MemberExpression which either completes the LeftHandSideExpression,
        // or starts the beginning of the first four CallExpression productions.
        let pos = self.node_pos();
        let expression;
        if self.token == SyntaxKind::ImportKeyword {
            if self.look_ahead(Self::next_token_is_open_paren_or_less_than) {
                // We don't want to eagerly consume all import keyword as import call expression so we look ahead to find "("
                // For example:
                //      var foo3 = require("subfolder
                //      import * as foo1 from "module-from-node
                // We want this import to be a statement rather than import call expression
                self.source_flags.insert(NodeFlags::PossiblyContainsDynamicImport);
                expression = self.parse_keyword_expression()
            } else if self.look_ahead(Self::next_token_is_dot) {
                // This is an 'import.*' metaproperty (i.e. 'import.meta')
                self.next_token(); // advance past the 'import'
                self.next_token(); // advance past the dot
                let name = self.parse_identifier_name();
                expression = self.nodes.create(
                    SyntaxKind::MetaProperty,
                    MetaProperty { keyword_token: SyntaxKind::ImportKeyword, name },
                );
                self.finish_node(expression, pos);
                if self.nodes[name].data_ref::<Identifier>().text == "defer" {
                    if self.token == SyntaxKind::OpenParenToken
                        || self.token == SyntaxKind::LessThanToken
                    {
                        self.source_flags.insert(NodeFlags::PossiblyContainsDynamicImport);
                    }
                } else {
                    self.source_flags.insert(NodeFlags::PossiblyContainsImportMeta);
                }
            } else {
                expression = self.parse_member_expression_or_higher()
            }
        } else if self.token == SyntaxKind::SuperKeyword {
            expression = self.parse_super_expression()
        } else {
            expression = self.parse_member_expression_or_higher()
        }
        // Now, we *may* be complete.  However, we might have consumed the start of a
        // CallExpression or OptionalExpression.  As such, we need to consume the rest
        // of it here to be complete.
        self.parse_call_expression_rest(pos, expression)
    }

    fn parse_keyword_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let node = self.nodes.create(self.token, ());
        self.next_token();
        self.finish_node(node, pos)
    }

    fn parse_member_expression_or_higher(&mut self) -> NodeId {
        // Note: to make our lives simpler, we decompose the NewExpression productions and
        // place ObjectCreationExpression and FunctionExpression into PrimaryExpression.
        // like so:
        //
        //   PrimaryExpression : See 11.1
        //      this
        //      Identifier
        //      Literal
        //      ArrayLiteral
        //      ObjectLiteral
        //      (Expression)
        //      FunctionExpression
        //      new MemberExpression Arguments?
        //
        //   MemberExpression : See 11.2
        //      PrimaryExpression
        //      MemberExpression[Expression]
        //      MemberExpression.IdentifierName
        //
        //   CallExpression : See 11.2
        //      MemberExpression
        //      CallExpression Arguments
        //      CallExpression[Expression]
        //      CallExpression.IdentifierName
        //
        // Technically this is ambiguous.  i.e. CallExpression defines:
        //
        //   CallExpression:
        //      CallExpression Arguments
        //
        // If you see: "new Foo()"
        //
        // Then that could be treated as a single ObjectCreationExpression, or it could be
        // treated as the invocation of "new Foo".  We disambiguate that in code (to match
        // the original grammar) by making sure that if we see an ObjectCreationExpression
        // we always consume arguments if they are there. So we treat "new Foo()" as an
        // object creation only, and not at all as an invocation.  Another way to think
        // about this is that for every "new" that we see, we will consume an argument list if
        // it is there as part of the *associated* object creation node.  Any additional
        // argument lists we see, will become invocation expressions.
        //
        // Because there are no other places in the grammar now that refer to FunctionExpression
        // or ObjectCreationExpression, it is safe to push down into the PrimaryExpression
        // production.
        //
        // Because CallExpression and MemberExpression are left recursive, we need to bottom out
        // of the recursion immediately.  So we parse out a primary expression to start with.
        let pos = self.node_pos();
        let expression = self.parse_primary_expression();
        self.parse_member_expression_rest(pos, expression, true)
    }

    fn parse_primary_expression(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                if self.scanner.token_flags().contains(TokenFlags::IsInvalid) {
                    self.rescan_template_token(false);
                }
                return self.parse_literal_expression();
            }
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral | SyntaxKind::StringLiteral => {
                return self.parse_literal_expression();
            }
            SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword => return self.parse_keyword_expression(),
            SyntaxKind::OpenParenToken => return self.parse_parenthesized_expression(),
            SyntaxKind::OpenBracketToken => return self.parse_array_literal_expression(),
            SyntaxKind::OpenBraceToken => return self.parse_object_literal_expression(),
            SyntaxKind::AsyncKeyword => {
                // Async arrow functions are parsed earlier in parseAssignmentExpressionOrHigher.
                // If we encounter `async [no LineTerminator here] function` then this is an async
                // function; otherwise, its an identifier.
                if self.look_ahead(Self::next_token_is_function_keyword_on_same_line) {
                    return self.parse_function_expression();
                }
            }
            SyntaxKind::AtToken => return self.parse_decorated_expression(),
            SyntaxKind::ClassKeyword => return self.parse_class_expression(),
            SyntaxKind::FunctionKeyword => return self.parse_function_expression(),
            SyntaxKind::NewKeyword => return self.parse_new_expression_or_new_dot_target(),
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => {
                if self.rescan_slash_token() == SyntaxKind::RegularExpressionLiteral {
                    return self.parse_literal_expression();
                }
            }
            SyntaxKind::TemplateHead => return self.parse_template_expression(false),
            SyntaxKind::PrivateIdentifier => return self.parse_private_identifier(),
            _ => {}
        }
        self.parse_identifier_with_diagnostic(Some(Message::e1109_expression_expected()), None)
    }

    fn parse_parenthesized_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let node = self
            .nodes
            .create(SyntaxKind::ParenthesizedExpression, ParenthesizedExpression { expression });
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_array_literal_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let open_bracket_pos = self.scanner.token_start();
        let open_bracket_parsed = self.parse_expected(SyntaxKind::OpenBracketToken);
        let multiline = self.has_preceding_line_break();
        let elements = self
            .parse_delimited_list(ParsingContext::ArrayLiteralMembers, |p| {
                Some(p.parse_argument_or_array_literal_element())
            })
            .unwrap();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
            open_bracket_parsed,
            open_bracket_pos,
        );
        let node = self.nodes.create(
            SyntaxKind::ArrayLiteralExpression,
            ArrayLiteralExpression { elements, multiline },
        );
        self.finish_node(node, pos)
    }

    fn parse_argument_or_array_literal_element(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::DotDotDotToken => self.parse_spread_element(),
            SyntaxKind::CommaToken => {
                let node = self.nodes.create(SyntaxKind::OmittedExpression, ());
                self.finish_node(node, self.node_pos())
            }
            _ => self.parse_assignment_expression_or_higher(),
        }
    }

    fn parse_spread_element(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::DotDotDotToken);
        let expression = self.parse_assignment_expression_or_higher();
        let node = self.nodes.create(SyntaxKind::SpreadElement, SpreadElement { expression });
        self.finish_node(node, pos)
    }

    fn parse_object_literal_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let open_brace_pos = self.scanner.token_start();
        let open_brace_parsed = self.parse_expected(SyntaxKind::OpenBraceToken);
        let multiline = self.has_preceding_line_break();
        let properties = self
            .parse_delimited_list(ParsingContext::ObjectLiteralMembers, |p| {
                Some(p.parse_object_literal_element())
            })
            .unwrap();
        self.parse_expected_matching_brackets(
            SyntaxKind::OpenBraceToken,
            SyntaxKind::CloseBraceToken,
            open_brace_parsed,
            open_brace_pos,
        );
        let node = self.nodes.create(
            SyntaxKind::ObjectLiteralExpression,
            ObjectLiteralExpression { properties, multiline },
        );
        self.finish_node(node, pos)
    }

    fn parse_object_literal_element(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if self.parse_optional(SyntaxKind::DotDotDotToken) {
            let expression = self.parse_assignment_expression_or_higher();
            let node =
                self.nodes.create(SyntaxKind::SpreadAssignment, SpreadAssignment { expression });
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            return node;
        }

        let modifiers = self.parse_modifiers_ex(true, false, false);
        if self.parse_contextual_modifier(SyntaxKind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::GetAccessor,
                ParseFlags::empty(),
            );
        }
        if self.parse_contextual_modifier(SyntaxKind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::SetAccessor,
                ParseFlags::empty(),
            );
        }

        let asterisk_token = self.parse_optional_token(SyntaxKind::AsteriskToken);
        let token_is_identifier = self.is_identifier();
        let name = self.parse_property_name();
        // Disallowing of optional property assignments and definite assignment assertion happens in the grammar checker.
        let mut postfix_token = self.parse_optional_token(SyntaxKind::QuestionToken);
        // Decorators, Modifiers, questionToken, and exclamationToken are not supported by property assignments and are reported in the grammar checker
        if postfix_token.is_none() {
            postfix_token = self.parse_optional_token(SyntaxKind::ExclamationToken);
        }

        if asterisk_token.is_some()
            || matches!(self.token, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken)
        {
            return self.parse_method_declaration(
                pos,
                jsdoc,
                modifiers,
                asterisk_token,
                name,
                postfix_token,
                None,
            );
        }

        // check if it is short-hand property assignment or normal property assignment
        // NOTE: if token is EqualsToken it is interpreted as CoverInitializedName production
        // CoverInitializedName[Yield] :
        //     IdentifierReference[?Yield] Initializer[In, ?Yield]
        // this is necessary because ObjectLiteral productions are also used to cover grammar for ObjectAssignmentPattern
        let is_shorthand_property_assignment =
            token_is_identifier && self.token != SyntaxKind::ColonToken;
        let node = if is_shorthand_property_assignment {
            let equals_token = self.parse_optional_token(SyntaxKind::EqualsToken);
            let mut initializer = None;
            if equals_token.is_some() {
                initializer = Some(self.in_context(
                    NodeFlags::DisallowInContext,
                    false,
                    Self::parse_assignment_expression_or_higher,
                ));
            }
            self.nodes.create(
                SyntaxKind::ShorthandPropertyAssignment,
                ShorthandPropertyAssignment {
                    modifiers,
                    name,
                    postfix_token,
                    type_node: None,
                    equals_token,
                    initializer,
                },
            )
        } else {
            self.parse_expected(SyntaxKind::ColonToken);
            let initializer = Some(self.in_context(
                NodeFlags::DisallowInContext,
                false,
                Self::parse_assignment_expression_or_higher,
            ));
            self.nodes.create(
                SyntaxKind::PropertyAssignment,
                PropertyAssignment { modifiers, name, postfix_token, type_node: None, initializer },
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_accessor_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        kind: SyntaxKind,
        flags: ParseFlags,
    ) -> NodeId {
        let name = self.parse_property_name();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::empty());
        let return_type = self.parse_return_type(SyntaxKind::ColonToken, false);
        let body = self.parse_function_block_or_semicolon(flags, None);
        let node = if kind == SyntaxKind::GetAccessor {
            self.nodes.create(
                kind,
                GetAccessor {
                    modifiers,
                    name,
                    type_parameters,
                    parameters,
                    return_type,
                    full_signature: None,
                    body,
                },
            )
        } else {
            self.nodes.create(
                kind,
                SetAccessor {
                    modifiers,
                    name,
                    type_parameters,
                    parameters,
                    return_type,
                    full_signature: None,
                    body,
                },
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        if !flags.contains(ParseFlags::Type) {
            self.check_js_syntax(node);
        }
        node
    }

    fn parse_function_block_or_semicolon(
        &mut self,
        flags: ParseFlags,
        diagnostic_message: Option<&'static Message>,
    ) -> Option<NodeId> {
        if self.token != SyntaxKind::OpenBraceToken {
            if flags.contains(ParseFlags::Type) {
                self.parse_type_member_semicolon();
                return None;
            }

            if self.can_parse_semicolon() {
                self.parse_semicolon();
                return None;
            }
        }
        Some(self.parse_function_block(flags, diagnostic_message))
    }

    fn parse_type_member_semicolon(&mut self) {
        // We allow type members to be separated by commas or (possibly ASI) semicolons.
        // First check if it was a comma.  If so, we're done with the member.
        if self.parse_optional(SyntaxKind::CommaToken) {
            return;
        }
        // Didn't have a comma.  We must have a (possible ASI) semicolon.
        self.parse_semicolon();
    }

    fn parse_method_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        asterisk_token: Option<NodeId>,
        name: NodeId,
        question_token: Option<NodeId>,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let mut signature_flags = ParseFlags::empty();
        signature_flags.set(ParseFlags::Yield, asterisk_token.is_some());
        signature_flags
            .set(ParseFlags::Await, self.nodes.has_modifier(&modifiers, ModifierFlags::Async));

        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(signature_flags);
        let type_node = self.parse_return_type(SyntaxKind::ColonToken, false);
        let body = self.parse_function_block_or_semicolon(signature_flags, diagnostic_message);
        let node = self.nodes.create(
            SyntaxKind::MethodDeclaration,
            MethodDeclaration {
                modifiers,
                asterisk_token,
                name,
                question_token,
                type_parameters,
                parameters,
                type_node,
                full_signature: None,
                body,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_function_expression(&mut self) -> NodeId {
        // GeneratorExpression:
        //      function* BindingIdentifier [Yield][opt](FormalParameters[Yield]){ GeneratorBody }
        //
        // FunctionExpression:
        //      function BindingIdentifier[opt](FormalParameters){ FunctionBody }
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::DecoratorContext, false);
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers();
        self.parse_expected(SyntaxKind::FunctionKeyword);
        let asterisk_token = self.parse_optional_token(SyntaxKind::AsteriskToken);
        let is_generator = asterisk_token.is_some();
        let is_async = self.nodes.has_modifier(&modifiers, ModifierFlags::Async);

        let mut signature_flags = ParseFlags::empty();
        signature_flags.set(ParseFlags::Yield, is_generator);
        signature_flags.set(ParseFlags::Await, is_async);

        let name = if is_generator && is_async {
            self.in_context(
                NodeFlags::YieldContext | NodeFlags::AwaitContext,
                true,
                Self::parse_optional_binding_identifier,
            )
        } else if is_generator {
            self.in_context(NodeFlags::YieldContext, true, Self::parse_optional_binding_identifier)
        } else if is_async {
            self.in_context(NodeFlags::AwaitContext, true, Self::parse_optional_binding_identifier)
        } else {
            self.parse_optional_binding_identifier()
        };
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(signature_flags);
        let return_type = self.parse_return_type(SyntaxKind::ColonToken, false);
        let body = self.parse_function_block(signature_flags, None);
        self.context_flags = save_context_flags;
        let result = self.nodes.create(
            SyntaxKind::FunctionExpression,
            FunctionExpression {
                modifiers,
                asterisk_token,
                name,
                type_parameters,
                parameters,
                return_type,
                full_signature: None, /*fullSignature*/
                body: Some(body),
            },
        );
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_decorated_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_ex(true, false, false);
        if self.token == SyntaxKind::ClassKeyword {
            return self.parse_class_declaration_or_expression(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::ClassExpression,
            );
        }
        self.parse_error_at_range(
            TextRange::new(self.node_pos(), self.node_pos()),
            Message::e1109_expression_expected(),
            [],
        );
        let node =
            self.nodes.create(SyntaxKind::MissingDeclaration, MissingDeclaration { modifiers });
        self.finish_node(node, pos)
    }

    fn parse_class_expression(&mut self) -> NodeId {
        self.parse_class_declaration_or_expression(
            self.node_pos(),
            self.jsdoc_scanner_info(),
            None,
            SyntaxKind::ClassExpression,
        )
    }

    fn parse_class_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_class_declaration_or_expression(
            pos,
            jsdoc,
            modifiers,
            SyntaxKind::ClassDeclaration,
        )
    }

    fn parse_class_declaration_or_expression(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        kind: SyntaxKind,
    ) -> NodeId {
        let save_context_flags = self.context_flags;
        let save_has_await_identifier = self.statement_has_await_identifier;
        self.parse_expected(SyntaxKind::ClassKeyword);
        // We don't parse the name here in await context, instead we will report a grammar error in the checker.
        let name = self.parse_name_of_class_declaration_or_expression();
        let type_parameters = self.parse_type_parameters();
        if self.nodes.has_modifier(&modifiers, ModifierFlags::Export) {
            self.set_context_flags(NodeFlags::AwaitContext, true);
        }
        let heritage_clauses = self.parse_heritage_clauses();
        let members;
        if self.parse_expected(SyntaxKind::OpenBraceToken) {
            // ClassTail[Yield,Await] : (Modified) See 14.5
            //      ClassHeritage[?Yield,?Await]opt { ClassBody[?Yield,?Await]opt }
            members = self.parse_list(ParsingContext::ClassMembers, Self::parse_class_element);
            self.parse_expected(SyntaxKind::CloseBraceToken);
        } else {
            members = NodeList::missing();
        }
        self.context_flags = save_context_flags;
        if self.nodes.has_modifier(&modifiers, ModifierFlags::Ambient) {
            self.statement_has_await_identifier = save_has_await_identifier;
        }
        let node = if kind == SyntaxKind::ClassDeclaration {
            self.nodes.create(
                SyntaxKind::ClassDeclaration,
                ClassDeclaration {
                    modifiers,
                    name,
                    type_parameters,
                    heritage_clauses: heritage_clauses.clone(),
                    members,
                },
            )
        } else {
            self.nodes.create(
                SyntaxKind::ClassExpression,
                ClassExpression {
                    modifiers,
                    name,
                    type_parameters,
                    heritage_clauses: heritage_clauses.clone(),
                    members,
                },
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        if self.nodes[node].flags.contains(NodeFlags::JavaScriptFile) {
            self.check_js_syntax(node);
            if let Some(heritage_clauses) = heritage_clauses {
                for clause in heritage_clauses.nodes {
                    let heritage_clause = self.nodes[clause].data_ref::<HeritageClause>();
                    if heritage_clause.token == SyntaxKind::ExtendsKeyword {
                        for &expr in heritage_clause.types.nodes.iter() {
                            self.check_js_syntax(expr);
                        }
                    }
                }
            }
        }
        node
    }

    fn parse_name_of_class_declaration_or_expression(&mut self) -> Option<NodeId> {
        // implements is a future reserved word so
        // 'class implements' might mean either
        // - class expression with omitted name, 'implements' starts heritage clause
        // - class with name 'implements'
        // 'isImplementsClause' helps to disambiguate between these two cases
        if self.is_binding_identifier() && !self.is_implements_clause() {
            let save_has_await_identifier = self.statement_has_await_identifier;
            let id = self.create_identifier(self.is_binding_identifier());
            self.statement_has_await_identifier = save_has_await_identifier;
            Some(id)
        } else {
            None
        }
    }

    fn is_implements_clause(&mut self) -> bool {
        self.token == SyntaxKind::ImplementsKeyword
            && self.look_ahead(Self::next_token_is_identifier_or_keyword)
    }

    fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
        // ClassTail[Yield,Await] : (Modified) See 14.5
        //      ClassHeritage[?Yield,?Await]opt { ClassBody[?Yield,?Await]opt }
        if self.is_heritage_clause() {
            Some(self.parse_list(ParsingContext::HeritageClauses, Self::parse_heritage_clause))
        } else {
            None
        }
    }

    fn parse_heritage_clause(&mut self) -> NodeId {
        let pos = self.node_pos();
        let token = self.token;
        self.next_token();
        let types = self
            .parse_delimited_list(ParsingContext::HeritageClauseElement, |p| {
                Some(p.parse_expression_with_type_arguments())
            })
            .unwrap();
        let node = self.nodes.create(SyntaxKind::HeritageClause, HeritageClause { token, types });
        self.finish_node(node, pos);
        self.check_js_syntax(node);
        node
    }

    fn parse_expression_with_type_arguments(&mut self) -> NodeId {
        let pos = self.node_pos();
        let expression = self.parse_left_hand_side_expression_or_higher();
        if self.nodes.is(expression, SyntaxKind::ExpressionWithTypeArguments) {
            return expression;
        }
        let type_arguments = self.parse_type_arguments();
        let node = self.nodes.create(
            SyntaxKind::ExpressionWithTypeArguments,
            ExpressionWithTypeArguments { expression, type_arguments },
        );
        self.finish_node(node, pos)
    }

    fn parse_class_element(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if self.token == SyntaxKind::SemicolonToken {
            self.next_token();
            let node = self.nodes.create(SyntaxKind::SemicolonClassElement, ());
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            return node;
        }

        let modifiers = self.parse_modifiers_ex(true, true, true);
        if self.token == SyntaxKind::StaticKeyword
            && self.look_ahead(Self::next_token_is_open_brace)
        {
            return self.parse_class_static_block_declaration(pos, jsdoc, modifiers);
        }
        if self.parse_contextual_modifier(SyntaxKind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::GetAccessor,
                ParseFlags::empty(),
            );
        }
        if self.parse_contextual_modifier(SyntaxKind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::SetAccessor,
                ParseFlags::empty(),
            );
        }
        if self.token == SyntaxKind::ConstructorKeyword || self.token == SyntaxKind::StringLiteral {
            let constructor_declaration =
                self.try_parse_constructor_declaration(pos, jsdoc, modifiers.clone());
            if let Some(constructor_declaration) = constructor_declaration {
                return constructor_declaration;
            }
        }
        if self.is_index_signature() {
            let node = self.parse_index_signature_declaration(pos, jsdoc, modifiers);
            self.check_js_syntax(node);
            return node;
        }
        // It is very important that we check this *after* checking indexers because
        // the [ token can start an index signature or a computed property name
        if self.token.is_identifier_or_keyword()
            || matches!(
                self.token,
                SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
                    | SyntaxKind::BigIntLiteral
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::OpenBracketToken
            )
        {
            let is_ambient = self.nodes.has_modifier(&modifiers, ModifierFlags::Ambient);
            return if is_ambient {
                for &m in modifiers.iter().flat_map(|x| x.list.nodes.iter()) {
                    self.nodes[m].flags.insert(NodeFlags::Ambient);
                }
                let save_context_flags = self.context_flags;
                self.set_context_flags(NodeFlags::Ambient, true);
                let node = self.parse_property_or_method_declaration(pos, jsdoc, modifiers);
                self.context_flags = save_context_flags;
                node
            } else {
                self.parse_property_or_method_declaration(pos, jsdoc, modifiers)
            };
        }
        if modifiers.is_some() {
            // treat this as a property declaration with a missing name.
            self.parse_error_at_range(
                TextRange::new(self.node_pos(), self.node_pos()),
                Message::e1146_declaration_expected(),
                [],
            );
            let name = self.create_missing_identifier();
            return self.parse_property_declaration(pos, jsdoc, modifiers, name, None);
        }

        // 'isClassMemberStart' should have hinted not to attempt parsing.
        unreachable!("Should not have attempted to parse class member declaration.")
    }

    fn try_parse_constructor_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> Option<NodeId> {
        let state = self.mark();
        if self.token == SyntaxKind::ConstructorKeyword
            || self.token == SyntaxKind::StringLiteral
                && self.scanner.token_value() == "constructor"
                && self.look_ahead(Self::next_token_is_open_paren)
        {
            self.next_token();
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameters(ParseFlags::empty());
            let return_type = self.parse_return_type(SyntaxKind::ColonToken, false);
            let body = self.parse_function_block_or_semicolon(
                ParseFlags::empty(),
                Some(Message::e1144_or_expected()),
            );
            let node = self.nodes.create(
                SyntaxKind::Constructor,
                Constructor {
                    modifiers,
                    type_parameters,
                    parameters,
                    return_type,
                    full_signature: None,
                    body,
                },
            );
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            self.check_js_syntax(node);
            Some(node)
        } else {
            self.rewind(state);
            None
        }
    }

    fn next_token_is_open_paren(&mut self) -> bool {
        self.next_token() == SyntaxKind::OpenParenToken
    }

    fn parse_property_or_method_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let asterisk_token = self.parse_optional_token(SyntaxKind::AsteriskToken);
        let name = self.parse_property_name();
        // Note: this is not legal as per the grammar.  But we allow it in the parser and
        // report an error in the grammar checker.
        let question_token = self.parse_optional_token(SyntaxKind::QuestionToken);
        if asterisk_token.is_some()
            || self.token == SyntaxKind::OpenParenToken
            || self.token == SyntaxKind::LessThanToken
        {
            self.parse_method_declaration(
                pos,
                jsdoc,
                modifiers,
                asterisk_token,
                name,
                question_token,
                Some(Message::e1144_or_expected()),
            )
        } else {
            self.parse_property_declaration(pos, jsdoc, modifiers, name, question_token)
        }
    }

    fn parse_property_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
        name: NodeId,
        question_token: Option<NodeId>,
    ) -> NodeId {
        let mut postfix_token = question_token;
        if postfix_token.is_none() && !self.has_preceding_line_break() {
            postfix_token = self.parse_optional_token(SyntaxKind::ExclamationToken);
        }
        let type_node = self.parse_type_annotation();
        let initializer = self.in_context(
            NodeFlags::YieldContext | NodeFlags::AwaitContext | NodeFlags::DisallowInContext,
            false,
            Self::parse_initializer,
        );
        self.parse_semicolon_after_property_name(name, type_node, initializer);
        let node = self.nodes.create(
            SyntaxKind::PropertyDeclaration,
            PropertyDeclaration { modifiers, name, postfix_token, type_node, initializer },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        self.check_js_syntax(node);
        node
    }

    fn parse_class_static_block_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        self.parse_expected(SyntaxKind::StaticKeyword);
        let body = self.parse_class_static_block_body();
        let node = self.nodes.create(
            SyntaxKind::ClassStaticBlockDeclaration,
            ClassStaticBlockDeclaration { modifiers, body },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_class_static_block_body(&mut self) -> NodeId {
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::YieldContext, false);
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let body = self.parse_block(false, None);
        self.context_flags = save_context_flags;
        body
    }

    fn parse_semicolon_after_property_name(
        &mut self,
        name: NodeId,
        type_node: Option<NodeId>,
        initializer: Option<NodeId>,
    ) {
        if self.token == SyntaxKind::AtToken && !self.has_preceding_line_break() {
            self.parse_error_at_current_token(Message::e1436_decorators_must_precede_the_name_and_all_keywords_of_property_declarations(), []);
            return;
        }

        if self.token == SyntaxKind::OpenParenToken {
            self.parse_error_at_current_token(
                Message::e1441_cannot_start_a_function_call_in_a_type_annotation(),
                [],
            );
            self.next_token();
            return;
        }
        if type_node.is_some() && !self.can_parse_semicolon() {
            if initializer.is_some() {
                self.parse_error_at_current_token(
                    Message::e1005_0_expected(),
                    [token_to_text(SyntaxKind::SemicolonToken).to_string()],
                );
            } else {
                self.parse_error_at_current_token(
                    Message::e1442_expected_for_property_initializer(),
                    [],
                );
            }
            return;
        }
        if self.try_parse_semicolon() {
            return;
        }
        if initializer.is_some() {
            self.parse_error_at_current_token(
                Message::e1005_0_expected(),
                [token_to_text(SyntaxKind::SemicolonToken).to_string()],
            );
            return;
        }
        self.parse_error_for_missing_semicolon_after(name);
    }

    fn parse_new_expression_or_new_dot_target(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::NewKeyword);
        if self.parse_optional(SyntaxKind::DotToken) {
            let name = self.parse_identifier_name();
            let node = self.nodes.create(
                SyntaxKind::MetaProperty,
                MetaProperty { keyword_token: SyntaxKind::NewKeyword, name },
            );
            return self.finish_node(node, pos);
        }
        let expression_pos = self.node_pos();
        let expression = self.parse_primary_expression();
        let mut expression = self.parse_member_expression_rest(expression_pos, expression, false);
        let mut type_arguments = None;
        // Absorb type arguments into NewExpression when preceding expression is ExpressionWithTypeArguments
        if self.nodes.is(expression, SyntaxKind::ExpressionWithTypeArguments) {
            let expr = self.nodes[expression].data_ref::<ExpressionWithTypeArguments>();
            type_arguments = expr.type_arguments.clone();
            expression = expr.expression;
        }
        if self.token == SyntaxKind::QuestionDotToken {
            self.parse_error_at_current_token(
                Message::e1209_invalid_optional_chain_from_new_expression_did_you_mean_to_call_0(),
                [Scanner::get_text_of_node_from_source_text(
                    &self.scanner.text,
                    &self.nodes[expression],
                    false,
                )],
            );
        }
        let mut argument_list = None;
        if self.token == SyntaxKind::OpenParenToken {
            argument_list = Some(self.parse_argument_list());
        }
        let node = self.nodes.create(
            SyntaxKind::NewExpression,
            NewExpression { expression, type_arguments: type_arguments.clone(), argument_list },
        );
        self.finish_node(node, pos);
        self.check_js_syntax(node);
        self.unparse_expression_with_type_arguments(Some(expression), type_arguments, node);
        node
    }

    fn parse_template_expression(&mut self, is_tagged_template: bool) -> NodeId {
        let pos = self.node_pos();
        let head = self.parse_template_head(is_tagged_template);
        let template_spans = self.parse_template_spans(is_tagged_template);
        let node = self
            .nodes
            .create(SyntaxKind::TemplateExpression, TemplateExpression { head, template_spans });
        self.finish_node(node, pos)
    }

    fn parse_template_head(&mut self, is_tagged_template: bool) -> NodeId {
        if !is_tagged_template && self.scanner.token_flags().contains(TokenFlags::IsInvalid) {
            self.rescan_template_token(false);
        }
        let pos = self.node_pos();
        let text = self.scanner.token_value().to_string();
        let raw_text = self.get_template_literal_raw_text(2);
        let template_flags = self.scanner.token_flags();
        self.next_token();
        let node = self
            .nodes
            .create(SyntaxKind::TemplateHead, TemplateHead { text, raw_text, template_flags });
        self.finish_node(node, pos)
    }

    fn parse_template_spans(&mut self, is_tagged_template: bool) -> NodeList {
        let pos = self.node_pos();
        let mut nodes = Vec::new();
        loop {
            let span = self.parse_template_span(is_tagged_template);
            nodes.push(span);
            let literal = self.nodes[span].data_ref::<TemplateSpan>().literal;
            if self.nodes[literal].kind != SyntaxKind::TemplateMiddle {
                break;
            }
        }
        NodeList { loc: TextRange::new(pos, self.node_pos()), nodes }
    }

    fn parse_template_span(&mut self, is_tagged_template: bool) -> NodeId {
        let pos = self.node_pos();
        let expression = self.parse_expression_allow_in();
        let literal = self.parse_literal_of_template_span(is_tagged_template);
        let node =
            self.nodes.create(SyntaxKind::TemplateSpan, TemplateSpan { expression, literal });
        self.finish_node(node, pos)
    }

    fn parse_literal_of_template_span(&mut self, is_tagged_template: bool) -> NodeId {
        if self.token == SyntaxKind::CloseBraceToken {
            self.rescan_template_token(is_tagged_template);
            return self.parse_template_middle_or_tail();
        }

        self.parse_error_at_current_token(
            Message::e1005_0_expected(),
            [token_to_text(SyntaxKind::CloseBraceToken).to_string()],
        );
        let node = self.nodes.create(
            SyntaxKind::TemplateTail,
            TemplateTail {
                text: String::new(),
                raw_text: String::new(),
                template_flags: TokenFlags::empty(),
            },
        );
        self.finish_node(node, self.node_pos())
    }

    fn parse_template_middle_or_tail(&mut self) -> NodeId {
        let pos = self.node_pos();
        let node = if self.token == SyntaxKind::TemplateMiddle {
            let text = self.scanner.token_value().to_string();
            let raw_text = self.get_template_literal_raw_text(2);
            let template_flags = self.scanner.token_flags();
            self.nodes.create(
                SyntaxKind::TemplateMiddle,
                TemplateMiddle { text, raw_text, template_flags },
            )
        } else {
            let text = self.scanner.token_value().to_string();
            let raw_text = self.get_template_literal_raw_text(1);
            let template_flags = self.scanner.token_flags();
            self.nodes
                .create(SyntaxKind::TemplateTail, TemplateTail { text, raw_text, template_flags })
        };
        self.next_token();
        self.finish_node(node, pos)
    }

    fn get_template_literal_raw_text(&mut self, mut end_length: usize) -> String {
        let text = self.scanner.token_text();
        if self.scanner.token_flags().contains(TokenFlags::Unterminated) {
            end_length = 0;
        }
        text[1..text.len() - end_length].to_string()
    }

    fn parse_member_expression_rest(
        &mut self,
        pos: usize,
        mut expression: NodeId,
        allow_optional_chain: bool,
    ) -> NodeId {
        loop {
            let mut question_dot_token = None;
            let is_property_access;
            if allow_optional_chain && self.is_start_of_optional_property_or_element_access_chain()
            {
                question_dot_token = Some(self.parse_expected_token(SyntaxKind::QuestionDotToken));
                is_property_access = self.token.is_identifier_or_keyword();
            } else {
                is_property_access = self.parse_optional(SyntaxKind::DotToken);
            }
            if is_property_access {
                expression =
                    self.parse_property_access_expression_rest(pos, expression, question_dot_token);
                continue;
            }
            // when in the [Decorator] context, we do not parse ElementAccess as it could be part of a ComputedPropertyName
            if (question_dot_token.is_some() || !self.in_decorator_context())
                && self.parse_optional(SyntaxKind::OpenBracketToken)
            {
                expression =
                    self.parse_element_access_expression_rest(pos, expression, question_dot_token);
                continue;
            }
            if self.is_template_start_of_tagged_template() {
                // Absorb type arguments into TemplateExpression when preceding expression is ExpressionWithTypeArguments
                if question_dot_token.is_none()
                    && self.nodes.is(expression, SyntaxKind::ExpressionWithTypeArguments)
                {
                    let original = self.nodes[expression].data::<ExpressionWithTypeArguments>();
                    expression = self.parse_tagged_template_rest(
                        pos,
                        original.expression,
                        question_dot_token,
                        original.type_arguments.clone(),
                    );
                    self.unparse_expression_with_type_arguments(
                        Some(original.expression),
                        original.type_arguments.clone(),
                        expression,
                    );
                } else {
                    expression =
                        self.parse_tagged_template_rest(pos, expression, question_dot_token, None);
                }
                continue;
            }
            if question_dot_token.is_none() {
                if self.token == SyntaxKind::ExclamationToken && !self.has_preceding_line_break() {
                    self.next_token();
                    expression = self.nodes.new_non_null_expression(expression, NodeFlags::empty());
                    self.finish_node(expression, pos);
                    self.check_js_syntax(expression);
                    continue;
                }
                let type_arguments = self.try_parse_type_arguments_in_expression();
                if type_arguments.is_some() {
                    expression = self.nodes.create(
                        SyntaxKind::ExpressionWithTypeArguments,
                        ExpressionWithTypeArguments { expression, type_arguments },
                    );
                    self.finish_node(expression, pos);
                    continue;
                }
            }
            return expression;
        }
    }

    fn is_start_of_optional_property_or_element_access_chain(&mut self) -> bool {
        self.token == SyntaxKind::QuestionDotToken
            && self
                .look_ahead(Self::next_token_is_identifier_or_keyword_or_open_bracket_or_template)
    }

    fn parse_property_access_expression_rest(
        &mut self,
        pos: usize,
        expression: NodeId,
        question_dot_token: Option<NodeId>,
    ) -> NodeId {
        let name = self.parse_right_side_of_dot(true, true, true);
        let is_optional_chain =
            question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
        let property_access = self.nodes.new_property_access_expression(
            expression,
            question_dot_token,
            name,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::empty() },
        );
        if is_optional_chain && self.nodes.is(name, SyntaxKind::PrivateIdentifier) {
            let loc = self.skip_range_trivia(self.nodes[name].loc);
            self.parse_error_at_range(
                loc,
                Message::e18030_an_optional_chain_cannot_contain_private_identifiers(),
                [],
            );
        }
        if self.nodes.is(expression, SyntaxKind::ExpressionWithTypeArguments) {
            let type_arguments =
                &self.nodes[expression].data_ref::<ExpressionWithTypeArguments>().type_arguments;
            if let Some(type_arguments) = type_arguments {
                let loc = TextRange::new(
                    type_arguments.loc.pos as usize - 1,
                    Scanner::skip_trivia(&self.scanner.text, type_arguments.loc.end as usize) + 1,
                );
                self.parse_error_at_range(
                    loc,
                    Message::e1477_an_instantiation_expression_cannot_be_followed_by_a_property_access(),
                    [],
                );
            }
        }
        self.finish_node(property_access, pos)
    }

    fn parse_element_access_expression_rest(
        &mut self,
        pos: usize,
        expression: NodeId,
        question_dot_token: Option<NodeId>,
    ) -> NodeId {
        let argument_expression = if self.token == SyntaxKind::CloseBracketToken {
            self.parse_error_at_range(
                TextRange::new(self.node_pos(), self.node_pos()),
                Message::e1011_an_element_access_expression_should_take_an_argument(),
                [],
            );
            self.create_missing_identifier()
        } else {
            self.parse_expression_allow_in()
        };
        self.parse_expected(SyntaxKind::CloseBracketToken);
        let is_optional_chain =
            question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
        let node = self.nodes.new_element_access_expression(
            expression,
            question_dot_token,
            argument_expression,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::empty() },
        );
        self.finish_node(node, pos)
    }

    fn parse_super_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let mut expression = self.parse_keyword_expression();
        if self.token == SyntaxKind::LessThanToken {
            let start_pos = self.node_pos();
            let type_arguments = self.try_parse_type_arguments_in_expression();
            if type_arguments.is_some() {
                self.parse_error_at_range(
                    TextRange::new(start_pos, self.node_pos()),
                    Message::e2754_super_may_not_use_type_arguments(),
                    [],
                );
                if !self.is_template_start_of_tagged_template() {
                    expression = self.nodes.create(
                        SyntaxKind::ExpressionWithTypeArguments,
                        ExpressionWithTypeArguments { expression, type_arguments },
                    );
                    self.finish_node(expression, pos);
                }
            }
        }

        if matches!(
            self.token,
            SyntaxKind::OpenParenToken | SyntaxKind::DotToken | SyntaxKind::OpenBracketToken
        ) {
            return expression;
        }

        // If we have seen "super" it must be followed by '(' or '.'.
        // If it wasn't then just try to parse out a '.' and report an error.
        self.parse_error_at_current_token(
            Message::e1034_super_must_be_followed_by_an_argument_list_or_member_access(),
            [],
        );
        // private names will never work with `super` (`super.#foo`), but that's a semantic error, not syntactic
        let name = self.parse_right_side_of_dot(true, true, true);
        let node =
            self.nodes.new_property_access_expression(expression, None, name, NodeFlags::empty());
        self.finish_node(node, pos)
    }

    fn is_template_start_of_tagged_template(&mut self) -> bool {
        matches!(self.token, SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead)
    }

    fn try_parse_type_arguments_in_expression(&mut self) -> Option<NodeList> {
        // TypeArguments must not be parsed in JavaScript files to avoid ambiguity with binary operators.
        // Check the cheap preconditions before saving the parser state: unless the current token is `<`
        // (or `<<`, which reScanLessThanToken would split), there is nothing to speculatively parse and
        // the mark/rewind would be a no-op.
        if self.context_flags.contains(NodeFlags::JavaScriptFile)
            || (self.token != SyntaxKind::LessThanToken
                && self.token != SyntaxKind::LessThanLessThanToken)
        {
            return None;
        }
        let state = self.mark();
        if self.rescan_less_than_token() == SyntaxKind::LessThanToken {
            self.next_token();
            let type_arguments =
                self.parse_delimited_list(ParsingContext::TypeArguments, |p| Some(p.parse_type()));
            // If it doesn't have the closing `>` then it's definitely not an type argument list.
            if self.rescan_greater_than_token() == SyntaxKind::GreaterThanToken {
                self.next_token();
                // We successfully parsed a type argument list. The next token determines whether we want to
                // treat it as such. If the type argument list is followed by `(` or a template literal, as in
                // `f<number>(42)`, we favor the type argument interpretation even though JavaScript would view
                // it as a relational expression.
                if self.can_follow_type_arguments_in_expression() {
                    return type_arguments;
                }
            }
        }
        self.rewind(state);
        None
    }

    fn can_follow_type_arguments_in_expression(&mut self) -> bool {
        match self.token {
            // These tokens can follow a type argument list in a call expression:
            // foo<x>(
            // foo<T> `...`
            // foo<T> `...${100}...`
            SyntaxKind::OpenParenToken
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,

            // A type argument list followed by `<` never makes sense, and a type argument list followed
            // by `>` is ambiguous with a (re-scanned) `>>` operator, so we disqualify both. Also, in
            // this context, `+` and `-` are unary operators, not binary operators.
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken => false,

            // We favor the type argument list interpretation when it is immediately followed by
            // a line break, a binary operator, or something that can't start an expression.
            _ => {
                self.has_preceding_line_break()
                    || self.is_binary_operator()
                    || !self.is_start_of_expression()
            }
        }
    }

    fn parse_right_side_of_dot(
        &mut self,
        allow_identifier_names: bool,
        allow_private_identifiers: bool,
        allow_unicode_escape_sequence_in_identifier_name: bool,
    ) -> NodeId {
        // Technically a keyword is valid here as all identifiers and keywords are identifier names.
        // However, often we'll encounter this in error situations when the identifier or keyword
        // is actually starting another valid construct.
        //
        // So, we check for the following specific case:
        //
        //      name.
        //      identifierOrKeyword identifierNameOrKeyword
        //
        // Note: the newlines are important here.  For example, if that above code
        // were rewritten into:
        //
        //      name.identifierOrKeyword
        //      identifierNameOrKeyword
        //
        // Then we would consider it valid.  That's because ASI would take effect and
        // the code would be implicitly: "name.identifierOrKeyword; identifierNameOrKeyword".
        // In the first case though, ASI will not take effect because there is not a
        // line terminator after the identifier or keyword.
        if self.has_preceding_line_break()
            && self.token.is_identifier_or_keyword()
            && self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line)
        {
            // Report that we need an identifier.  However, report it right after the dot,
            // and not on the next token.  This is because the next token might actually
            // be an identifier and the error would be quite confusing.
            self.parse_error_at_range(
                TextRange::new(self.node_pos(), self.node_pos()),
                Message::e1003_identifier_expected(),
                [],
            );
            return self.create_missing_identifier();
        }

        if self.token == SyntaxKind::PrivateIdentifier {
            let node = self.parse_private_identifier();
            if allow_private_identifiers {
                return node;
            }
            self.parse_error_at_range(
                TextRange::new(self.node_pos(), self.node_pos()),
                Message::e1003_identifier_expected(),
                [],
            );
            return self.create_missing_identifier();
        }

        if allow_identifier_names {
            return if allow_unicode_escape_sequence_in_identifier_name {
                self.parse_identifier_name()
            } else {
                self.parse_identifier_name_error_on_unicode_escape_sequence()
            };
        }

        let save_has_await_identifier = self.statement_has_await_identifier;
        let id = self.parse_identifier();
        self.statement_has_await_identifier = save_has_await_identifier;
        id
    }

    fn parse_call_expression_rest(&mut self, pos: usize, mut expression: NodeId) -> NodeId {
        loop {
            expression = self.parse_member_expression_rest(pos, expression, true);
            let question_dot_token = self.parse_optional_token(SyntaxKind::QuestionDotToken);
            let mut type_arguments = None;
            if question_dot_token.is_some() {
                type_arguments = self.try_parse_type_arguments_in_expression();
                if self.is_template_start_of_tagged_template() {
                    expression = self.parse_tagged_template_rest(
                        pos,
                        expression,
                        question_dot_token,
                        type_arguments,
                    );
                    continue;
                }
            }

            if type_arguments.is_some() || self.token == SyntaxKind::OpenParenToken {
                // Absorb type arguments into CallExpression when preceding expression is ExpressionWithTypeArguments
                if question_dot_token.is_none()
                    && self.nodes[expression].kind == SyntaxKind::ExpressionWithTypeArguments
                {
                    let expression_with_type_args =
                        self.nodes[expression].data_ref::<ExpressionWithTypeArguments>();
                    type_arguments = expression_with_type_args.type_arguments.clone();
                    expression = expression_with_type_args.expression;
                }
                let inner = expression;
                let argument_list = self.parse_argument_list();
                let is_optional_chain =
                    question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
                expression = self.nodes.new_call_expression(
                    expression,
                    question_dot_token,
                    type_arguments.clone(),
                    argument_list,
                    if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::empty() },
                );
                self.finish_node(expression, pos);
                self.check_js_syntax(expression);
                self.unparse_expression_with_type_arguments(
                    Some(inner),
                    type_arguments,
                    expression,
                );
                continue;
            }

            if question_dot_token.is_some() {
                // We parsed `?.` but then failed to parse anything, so report a missing identifier here.
                self.parse_error_at_current_token(Message::e1003_identifier_expected(), []);
                let name = self.create_missing_identifier();
                expression = self.nodes.new_property_access_expression(
                    expression,
                    question_dot_token,
                    name,
                    NodeFlags::OptionalChain,
                );
                self.finish_node(expression, pos);
            }
            break;
        }
        expression
    }

    fn parse_tagged_template_rest(
        &mut self,
        pos: usize,
        tag: NodeId,
        question_dot_token: Option<NodeId>,
        type_arguments: Option<NodeList>,
    ) -> NodeId {
        let template = if self.token == SyntaxKind::NoSubstitutionTemplateLiteral {
            self.rescan_template_token(true);
            self.parse_literal_expression()
        } else {
            self.parse_template_expression(true)
        };
        let is_optional_chain = question_dot_token.is_some()
            || self.nodes[tag].flags.contains(NodeFlags::OptionalChain);
        let node = self.nodes.new_tagged_template_expression(
            tag,
            question_dot_token,
            type_arguments,
            template,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::empty() },
        );
        self.finish_node(node, pos)
    }

    fn try_reparse_optional_chain(&mut self, mut node: NodeId) -> bool {
        if self.nodes[node].flags.contains(NodeFlags::OptionalChain) {
            return true;
        }

        // check for an optional chain in a non-null expression
        if self.nodes.is(node, SyntaxKind::NonNullExpression) {
            let mut expr = self.nodes[node].data_ref::<NonNullExpression>().expression;
            while self.nodes.is(node, SyntaxKind::NonNullExpression)
                && !self.nodes[expr].flags.contains(NodeFlags::OptionalChain)
            {
                expr = self.nodes[expr].data_ref::<NonNullExpression>().expression;
            }

            if self.nodes[expr].flags.contains(NodeFlags::OptionalChain) {
                // this is part of an optional chain. Walk down from `node` to `expression` and set the flag.
                while self.nodes.is(node, SyntaxKind::NonNullExpression) {
                    self.nodes[node].flags.insert(NodeFlags::OptionalChain);
                    node = self.nodes[node].data_ref::<NonNullExpression>().expression;
                }
                return true;
            }
        }

        false
    }

    fn unparse_expression_with_type_arguments(
        &mut self,
        expression: Option<NodeId>,
        type_arguments: Option<NodeList>,
        result: NodeId,
    ) {
        // force overwrite the `.Parent` of the expression and type arguments to erase the fact that they may have originally been parsed as an ExpressionWithTypeArguments and be parented to such
        if let Some(expr) = expression {
            self.nodes[expr].parent = Some(result);
        }
        if let Some(type_arguments) = type_arguments {
            for node in type_arguments.nodes {
                self.nodes[node].parent = Some(result);
            }
        }
    }

    fn parse_argument_list(&mut self) -> NodeList {
        self.parse_expected(SyntaxKind::OpenParenToken);
        let result = self
            .parse_delimited_list(ParsingContext::ArgumentExpressions, |p| {
                Some(p.parse_argument_expression())
            })
            .unwrap();
        self.parse_expected(SyntaxKind::CloseParenToken);
        result
    }

    fn parse_argument_expression(&mut self) -> NodeId {
        self.in_context(
            NodeFlags::DisallowInContext | NodeFlags::DecoratorContext,
            false,
            Self::parse_argument_or_array_literal_element,
        )
    }

    fn next_token_is_identifier_or_keyword_or_greater_than(&mut self) -> bool {
        self.next_token();
        self.token.is_identifier_or_keyword() || self.token == SyntaxKind::GreaterThanToken
    }

    fn next_token_is_open_paren_or_less_than(&mut self) -> bool {
        self.next_token();
        matches!(self.token, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken)
    }

    fn next_token_is_dot(&mut self) -> bool {
        self.next_token() == SyntaxKind::DotToken
    }

    fn is_update_expression(&self) -> bool {
        match self.token {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::DeleteKeyword
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::AwaitKeyword => false,
            SyntaxKind::LessThanToken => self.language_variant == LanguageVariant::JSX,
            _ => true,
        }
    }

    fn parse_type(&mut self) -> NodeId {
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::TypeExcludesFlags, false);
        let mut type_node;
        if self.is_start_of_function_type_or_constructor_type() {
            type_node = self.parse_function_or_constructor_type();
        } else {
            let pos = self.node_pos();
            type_node = self.parse_union_type_or_higher();
            if !self.in_disallow_conditional_types_context()
                && !self.has_preceding_line_break()
                && self.parse_optional(SyntaxKind::ExtendsKeyword)
            {
                // The type following 'extends' is not permitted to be another conditional type
                let extends_type = self.in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    true,
                    Self::parse_type,
                );
                self.parse_expected(SyntaxKind::QuestionToken);
                let true_type = self.in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    false,
                    Self::parse_type,
                );
                self.parse_expected(SyntaxKind::ColonToken);
                let false_type = self.in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    false,
                    Self::parse_type,
                );
                let conditional_type = self.nodes.create(
                    SyntaxKind::ConditionalType,
                    ConditionalType { type_node, extends_type, true_type, false_type },
                );
                self.finish_node(conditional_type, pos);
                type_node = conditional_type;
            }
        }
        self.context_flags = save_context_flags;
        type_node
    }

    fn make_binary_expression(
        &mut self,
        left: NodeId,
        operator_token: NodeId,
        right: NodeId,
        pos: usize,
    ) -> NodeId {
        let node = self.nodes.create(
            SyntaxKind::BinaryExpression,
            BinaryExpression { left, operator_token, right, modifiers: None, type_node: None },
        );
        self.finish_node(node, pos)
    }

    fn make_satisfies_expression(&mut self, expression: NodeId, type_node: NodeId) -> NodeId {
        let node = self
            .nodes
            .create(SyntaxKind::SatisfiesExpression, SatisfiesExpression { expression, type_node });
        self.finish_node(node, self.nodes[expression].loc.pos as usize);
        self.check_js_syntax(node);
        node
    }

    fn make_as_expression(&mut self, expression: NodeId, type_node: NodeId) -> NodeId {
        let node =
            self.nodes.create(SyntaxKind::AsExpression, AsExpression { expression, type_node });
        self.finish_node(node, self.nodes[expression].loc.pos as usize);
        self.check_js_syntax(node);
        node
    }

    fn is_start_of_function_type_or_constructor_type(&mut self) -> bool {
        self.token == SyntaxKind::LessThanToken
            || self.token == SyntaxKind::OpenParenToken
                && self.look_ahead(Self::next_is_unambiguously_start_of_function_type)
            || self.token == SyntaxKind::NewKeyword
            || self.token == SyntaxKind::AbstractKeyword
                && self.look_ahead(Self::next_token_is_new_keyword)
    }

    fn parse_function_or_constructor_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_for_constructor_type();
        let is_constructor_type = self.parse_optional(SyntaxKind::NewKeyword);
        debug_assert!(
            modifiers.is_none() || is_constructor_type,
            "Per isStartOfFunctionOrConstructorType, a function type cannot have modifiers."
        );
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::Type);
        let return_type = self.parse_return_type(SyntaxKind::EqualsGreaterThanToken, false);
        let result = if is_constructor_type {
            self.nodes.create(
                SyntaxKind::ConstructorType,
                ConstructorType { modifiers, type_parameters, parameters, return_type },
            )
        } else {
            self.nodes.create(
                SyntaxKind::FunctionType,
                FunctionType { type_parameters, parameters, return_type },
            )
        };
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_type_parameters(&mut self) -> Option<NodeList> {
        if self.token == SyntaxKind::LessThanToken {
            self.parse_bracketed_list(
                ParsingContext::TypeParameters,
                |p| Some(p.parse_type_parameter()),
                SyntaxKind::LessThanToken,
                SyntaxKind::GreaterThanToken,
            )
        } else {
            None
        }
    }

    fn parse_type_parameter(&mut self) -> NodeId {
        let pos = self.node_pos();
        let modifiers = self.parse_modifiers_ex(false, true, false);
        let name = self.parse_identifier();
        let mut constraint = None;
        let mut expression = None;
        if self.parse_optional(SyntaxKind::ExtendsKeyword) {
            // It's not uncommon for people to write improper constraints to a generic.  If the
            // user writes a constraint that is an expression and not an actual type, then parse
            // it out as an expression (so we can recover well), but report that a type is needed
            // instead.
            if self.is_start_of_type(false) || !self.is_start_of_expression() {
                constraint = Some(self.parse_type());
            } else {
                // It was not a type, and it looked like an expression.  Parse out an expression
                // here so we recover well.  Note: it is important that we call parseUnaryExpression
                // and not parseExpression here.  If the user has:
                //
                //      <T extends "">
                //
                // We do *not* want to consume the `>` as we're consuming the expression for "".
                expression = Some(self.parse_unary_expression_or_higher());
            }
        }
        let mut default_type = None;
        if self.parse_optional(SyntaxKind::EqualsToken) {
            default_type = Some(self.parse_type());
        }
        let node = self.nodes.create(
            SyntaxKind::TypeParameter,
            TypeParameter { modifiers, name, constraint, expression, default_type },
        );
        self.finish_node(node, pos)
    }

    fn parse_parameters(&mut self, flags: ParseFlags) -> Option<NodeList> {
        // FormalParameters [Yield,Await]: (modified)
        //      [empty]
        //      FormalParameterList[?Yield,Await]
        //
        // FormalParameter[Yield,Await]: (modified)
        //      BindingElement[?Yield,Await]
        //
        // BindingElement [Yield,Await]: (modified)
        //      SingleNameBinding[?Yield,?Await]
        //      BindingPattern[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        //
        // SingleNameBinding [Yield,Await]:
        //      BindingIdentifier[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        if self.parse_expected(SyntaxKind::OpenParenToken) {
            let parameters = self.parse_parameters_worker(flags, true);
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            Some(NodeList::missing())
        }
    }

    fn parse_parameters_worker(
        &mut self,
        flags: ParseFlags,
        allow_ambiguity: bool,
    ) -> Option<NodeList> {
        // FormalParameters [Yield,Await]: (modified)
        //      [empty]
        //      FormalParameterList[?Yield,Await]
        //
        // FormalParameter[Yield,Await]: (modified)
        //      BindingElement[?Yield,Await]
        //
        // BindingElement [Yield,Await]: (modified)
        //      SingleNameBinding[?Yield,?Await]
        //      BindingPattern[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        //
        // SingleNameBinding [Yield,Await]:
        //      BindingIdentifier[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        let in_await_context = self.context_flags.contains(NodeFlags::AwaitContext);
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::YieldContext, flags.contains(ParseFlags::Yield));
        self.set_context_flags(NodeFlags::AwaitContext, flags.contains(ParseFlags::Await));
        let parameters = self.parse_delimited_list(ParsingContext::Parameters, |p| {
            let parameter = p.parse_parameter_ex(in_await_context, allow_ambiguity);
            if let Some(parameter) = parameter
                && !flags.contains(ParseFlags::Type)
            {
                p.check_js_syntax(parameter);
            }
            parameter
        });
        self.context_flags = save_context_flags;
        parameters
    }

    fn parse_parameter(&mut self) -> Option<NodeId> {
        self.parse_parameter_ex(false, true)
    }

    fn parse_parameter_ex(
        &mut self,
        in_outer_await_context: bool,
        allow_ambiguity: bool,
    ) -> Option<NodeId> {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        // FormalParameter [Yield,Await]:
        //      BindingElement[?Yield,?Await]
        // Decorators are parsed in the outer [Await] context, the rest of the parameter is parsed in the function's [Await] context.
        let save_context_flags = self.context_flags;
        self.set_context_flags(NodeFlags::AwaitContext, in_outer_await_context);
        let modifiers = self.parse_modifiers_ex(true, false, false);
        self.context_flags = save_context_flags;
        if self.token == SyntaxKind::ThisKeyword {
            let name = self.create_identifier(true);
            let type_node = self.parse_type_annotation();
            if let Some(modifiers) = &modifiers {
                self.parse_error_at_range(self.nodes[modifiers.list.nodes[0]].loc, Message::e1433_neither_decorators_nor_modifiers_may_be_applied_to_this_parameters(), []);
            }
            let node = self.nodes.create(
                SyntaxKind::Parameter,
                Parameter {
                    modifiers,
                    dot_dot_dot_token: None,
                    name,
                    question_token: None,
                    type_node,
                    initializer: None,
                },
            );
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            return Some(node);
        }

        let dot_dot_dot_token = self.parse_optional_token(SyntaxKind::DotDotDotToken);
        if !allow_ambiguity && !self.is_parameter_name_start() {
            return None;
        }
        let name = self.parse_name_of_parameter(modifiers.as_ref());
        let question_token = self.parse_optional_token(SyntaxKind::QuestionToken);
        let type_node = self.parse_type_annotation();
        let initializer = self.parse_initializer();
        let node = self.nodes.create(
            SyntaxKind::Parameter,
            Parameter {
                modifiers,
                dot_dot_dot_token,
                name,
                question_token,
                type_node,
                initializer,
            },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        Some(node)
    }

    fn parse_name_of_parameter(&mut self, modifiers: Option<&ModifierList>) -> NodeId {
        // FormalParameter [Yield,Await]:
        //      BindingElement[?Yield,?Await]
        let name = self.parse_identifier_or_pattern_with_diagnostic(Some(
            Message::e18009_private_identifiers_cannot_be_used_as_parameters(),
        ));
        if self.nodes[name].loc.len() == 0 && modifiers.is_none() && self.token.is_modifier() {
            // in cases like
            // 'use strict'
            // function foo(static)
            // isParameter('static') == true, because of isModifier('static')
            // however 'static' is not a legal identifier in a strict mode.
            // so result of this function will be Parameter (flags = 0, name = missing, type = undefined, initializer = undefined)
            // and current token will not change => parsing of the enclosing parameter list will last till the end of time (or OOM)
            // to avoid this we'll advance cursor to the next token.
            self.next_token();
        }
        name
    }

    fn is_parameter_name_start(&self) -> bool {
        // Be permissive about await and yield by calling isBindingIdentifier instead of isIdentifier; disallowing
        // them during a speculative parse leads to many more follow-on errors than allowing the function to parse then later
        // complaining about the use of the keywords.
        self.is_binding_identifier()
            || matches!(self.token, SyntaxKind::OpenBracketToken | SyntaxKind::OpenBraceToken)
    }

    fn parse_return_type(&mut self, return_token: SyntaxKind, is_type: bool) -> Option<NodeId> {
        if self.should_parse_return_type(return_token, is_type) {
            let type_node = self.in_context(
                NodeFlags::DisallowConditionalTypesContext,
                false,
                Self::parse_type_or_type_predicate,
            );
            Some(type_node)
        } else {
            None
        }
    }

    fn parse_type_or_type_predicate(&mut self) -> NodeId {
        if self.is_identifier() {
            let state = self.mark();
            let pos = self.node_pos();
            let identifier = self.parse_identifier();
            if self.token == SyntaxKind::IsKeyword && !self.has_preceding_line_break() {
                self.next_token();
                let type_node = self.parse_type();
                let node = self.nodes.create(
                    SyntaxKind::TypePredicate,
                    TypePredicate {
                        asserts_modifier: None,
                        parameter_name: identifier,
                        type_node: Some(type_node),
                    },
                );
                return self.finish_node(node, pos);
            }
            self.rewind(state);
        }
        self.parse_type()
    }

    fn should_parse_return_type(&mut self, return_token: SyntaxKind, is_type: bool) -> bool {
        if return_token == SyntaxKind::EqualsGreaterThanToken {
            self.parse_expected(return_token);
            true
        } else if self.parse_optional(SyntaxKind::ColonToken) {
            true
        } else if is_type && self.token == SyntaxKind::EqualsGreaterThanToken {
            // This is easy to get backward, especially in type contexts, so parse the type anyway
            self.parse_error_at_current_token(
                Message::e1005_0_expected(),
                [token_to_text(SyntaxKind::ColonToken).to_string()],
            );
            self.next_token();
            true
        } else {
            false
        }
    }

    fn parse_modifiers_for_constructor_type(&mut self) -> Option<ModifierList> {
        if self.token == SyntaxKind::AbstractKeyword {
            let pos = self.node_pos();
            let modifier = self.nodes.create(SyntaxKind::AbstractKeyword, ());
            self.next_token();
            self.finish_node(modifier, pos);
            Some(self.nodes.new_modifier_list(vec![modifier], self.nodes[modifier].loc))
        } else {
            None
        }
    }

    fn parse_modifiers_for_arrow_function(&mut self) -> Option<ModifierList> {
        if self.token == SyntaxKind::AsyncKeyword {
            let pos = self.node_pos();
            let modifier = self.nodes.create(SyntaxKind::AsyncKeyword, ());
            self.next_token();
            self.finish_node(modifier, pos);
            Some(self.nodes.new_modifier_list(vec![modifier], self.nodes[modifier].loc))
        } else {
            None
        }
    }

    fn parse_union_type_or_higher(&mut self) -> NodeId {
        self.parse_union_or_intersection_type(
            SyntaxKind::BarToken,
            Self::parse_intersection_or_higher,
        )
    }

    fn parse_intersection_or_higher(&mut self) -> NodeId {
        self.parse_union_or_intersection_type(
            SyntaxKind::AmpersandToken,
            Self::parse_type_operator_or_higher,
        )
    }

    fn parse_union_or_intersection_type(
        &mut self,
        operator: SyntaxKind,
        mut parse_constituent_type: impl FnMut(&mut Parser) -> NodeId,
    ) -> NodeId {
        let pos = self.node_pos();
        let is_union_type = operator == SyntaxKind::BarToken;
        let has_leading_operator = self.parse_optional(operator);
        let mut type_node = if has_leading_operator {
            self.parse_function_or_constructor_type_to_error(
                is_union_type,
                &mut parse_constituent_type,
            )
        } else {
            parse_constituent_type(self)
        };
        if self.token == operator || has_leading_operator {
            let mut types = Vec::new();
            types.push(type_node);
            while self.parse_optional(operator) {
                types.push(self.parse_function_or_constructor_type_to_error(
                    is_union_type,
                    &mut parse_constituent_type,
                ));
            }
            type_node = self.create_union_or_intersection_type_node(
                operator,
                NodeList { loc: TextRange::new(pos, self.node_pos()), nodes: types },
            );
            self.finish_node(type_node, pos);
        }
        type_node
    }

    fn create_union_or_intersection_type_node(
        &mut self,
        operator: SyntaxKind,
        types: NodeList,
    ) -> NodeId {
        match operator {
            SyntaxKind::BarToken => self.nodes.create(SyntaxKind::UnionType, UnionType { types }),
            SyntaxKind::AmpersandToken => {
                self.nodes.create(SyntaxKind::IntersectionType, IntersectionType { types })
            }
            _ => unreachable!("Unhandled case in create_union_or_intersection_type_node"),
        }
    }

    fn parse_type_operator_or_higher(&mut self) -> NodeId {
        let operator = self.token;
        match operator {
            SyntaxKind::KeyOfKeyword | SyntaxKind::UniqueKeyword | SyntaxKind::ReadonlyKeyword => {
                self.parse_type_operator(operator)
            }
            SyntaxKind::InferKeyword => self.parse_infer_type(),
            _ => self.in_context(
                NodeFlags::DisallowConditionalTypesContext,
                false,
                Self::parse_postfix_type_or_higher,
            ),
        }
    }

    fn parse_type_operator(&mut self, operator: SyntaxKind) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(operator);
        let type_node = self.parse_type_operator_or_higher();
        let node =
            self.nodes.create(SyntaxKind::TypeOperator, TypeOperator { operator, type_node });
        self.finish_node(node, pos)
    }

    fn parse_infer_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::InferKeyword);
        let type_parameter = self.parse_type_parameter_of_infer_type();
        let node = self.nodes.create(SyntaxKind::InferType, InferType { type_parameter });
        self.finish_node(node, pos)
    }

    fn parse_type_parameter_of_infer_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        let name = self.parse_identifier();
        let constraint = self.try_parse_constraint_of_infer_type();
        let node = self.nodes.create(
            SyntaxKind::TypeParameter,
            TypeParameter {
                modifiers: None,
                name,
                constraint,
                expression: None,
                default_type: None,
            },
        );
        self.finish_node(node, pos)
    }

    fn try_parse_constraint_of_infer_type(&mut self) -> Option<NodeId> {
        let state = self.mark();
        if self.parse_optional(SyntaxKind::ExtendsKeyword) {
            let constraint =
                self.in_context(NodeFlags::DisallowConditionalTypesContext, true, Self::parse_type);
            if self.in_disallow_conditional_types_context()
                || self.token != SyntaxKind::QuestionToken
            {
                return Some(constraint);
            }
        }
        self.rewind(state);
        None
    }

    fn parse_postfix_type_or_higher(&mut self) -> NodeId {
        let pos = self.node_pos();
        let mut type_node = self.parse_non_array_type();
        while !self.has_preceding_line_break() {
            match self.token {
                SyntaxKind::ExclamationToken => {
                    self.next_token();
                    type_node = self.nodes.create(
                        SyntaxKind::JSDocNonNullableType,
                        JSDocNonNullableType { type_node },
                    );
                    self.finish_node(type_node, pos);
                }
                SyntaxKind::QuestionToken => {
                    // If next token is start of a type we have a conditional type
                    if self.look_ahead(Self::next_is_start_of_type) {
                        return type_node;
                    }
                    self.next_token();
                    type_node = self
                        .nodes
                        .create(SyntaxKind::JSDocNullableType, JSDocNullableType { type_node });
                    self.finish_node(type_node, pos);
                }
                SyntaxKind::OpenBracketToken => {
                    self.parse_expected(SyntaxKind::OpenBracketToken);
                    type_node = if self.is_start_of_type(false) {
                        let index_type = self.parse_type();
                        self.parse_expected(SyntaxKind::CloseBracketToken);
                        self.nodes.create(
                            SyntaxKind::IndexedAccessType,
                            IndexedAccessType { type_node, index_type },
                        )
                    } else {
                        self.parse_expected(SyntaxKind::CloseBracketToken);
                        self.nodes.create(SyntaxKind::ArrayType, ArrayType { type_node })
                    };
                    self.finish_node(type_node, pos);
                }
                _ => break,
            }
        }

        type_node
    }

    fn parse_non_array_type(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::ObjectKeyword => {
                let state = self.mark();
                let keyword_type_node = self.parse_keyword_type_node();
                // If these are followed by a dot then parse these out as a dotted type reference instead
                if self.token != SyntaxKind::DotToken {
                    return keyword_type_node;
                }
                self.rewind(state);
                self.parse_type_reference()
            }
            SyntaxKind::AsteriskEqualsToken | SyntaxKind::AsteriskToken => {
                if self.token == SyntaxKind::AsteriskEqualsToken {
                    // If there is '*=', treat it as * followed by postfix =
                    self.scanner.rescan_asterisk_equals_token();
                }
                self.parse_jsdoc_all_type()
            }
            SyntaxKind::QuestionQuestionToken | SyntaxKind::QuestionToken => {
                if self.token == SyntaxKind::QuestionQuestionToken {
                    self.scanner.rescan_question_token();
                }
                self.parse_jsdoc_nullable_type()
            }
            SyntaxKind::ExclamationToken => self.parse_jsdoc_non_nullable_type(),
            SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword => self.parse_literal_type_node(false),
            SyntaxKind::MinusToken => {
                if self.look_ahead(Self::next_token_is_numeric_or_big_int_literal) {
                    self.parse_literal_type_node(true)
                } else {
                    self.parse_type_reference()
                }
            }
            SyntaxKind::VoidKeyword => self.parse_keyword_type_node(),
            SyntaxKind::ThisKeyword => {
                let this_keyword = self.parse_this_type_node();
                if self.token == SyntaxKind::IsKeyword && !self.has_preceding_line_break() {
                    self.parse_this_type_predicate(this_keyword)
                } else {
                    this_keyword
                }
            }
            SyntaxKind::TypeOfKeyword => {
                if self.look_ahead(Self::next_is_start_of_type_of_import_type) {
                    self.parse_import_type()
                } else {
                    self.parse_type_query()
                }
            }
            SyntaxKind::OpenBraceToken => {
                if self.look_ahead(Self::next_is_start_of_mapped_type) {
                    self.parse_mapped_type()
                } else {
                    self.parse_type_literal()
                }
            }
            SyntaxKind::OpenBracketToken => self.parse_tuple_type(),
            SyntaxKind::OpenParenToken => self.parse_parenthesized_type(),
            SyntaxKind::ImportKeyword => self.parse_import_type(),
            SyntaxKind::AssertsKeyword => {
                if self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line) {
                    self.parse_asserts_type_predicate()
                } else {
                    self.parse_type_reference()
                }
            }
            SyntaxKind::TemplateHead => self.parse_template_type(),
            _ => self.parse_type_reference(),
        }
    }

    fn parse_keyword_type_node(&mut self) -> NodeId {
        let pos = self.node_pos();
        let node = self.nodes.create(self.token, ());
        self.next_token();
        self.finish_node(node, pos)
    }

    fn parse_this_type_node(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let node = self.nodes.create(SyntaxKind::ThisKeyword, ());
        self.finish_node(node, pos)
    }

    fn parse_type_reference(&mut self) -> NodeId {
        let pos = self.node_pos();
        let type_name = self.parse_entity_name_of_type_reference();
        let type_arguments = self.parse_type_arguments_of_type_reference();
        let node = self
            .nodes
            .create(SyntaxKind::TypeReference, TypeReference { type_name, type_arguments });
        self.finish_node(node, pos)
    }

    fn parse_entity_name_of_type_reference(&mut self) -> NodeId {
        self.parse_entity_name(true, Some(Message::e1110_type_expected()))
    }

    fn parse_entity_name(
        &mut self,
        allow_reserved_words: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> NodeId {
        let pos = self.node_pos();
        let mut entity = if allow_reserved_words {
            self.parse_identifier_name_with_diagnostic(diagnostic_message)
        } else {
            self.parse_identifier_with_diagnostic(diagnostic_message, None)
        };
        while self.parse_optional(SyntaxKind::DotToken) {
            if self.token == SyntaxKind::LessThanToken {
                // The entity is part of a JSDoc-style generic. We will use the gap between `typeName` and
                // `typeArguments` to report it as a grammar error in the checker.
                break;
            }
            let right = self.parse_right_side_of_dot(allow_reserved_words, false, true);
            entity =
                self.nodes.create(SyntaxKind::QualifiedName, QualifiedName { left: entity, right });
            self.finish_node(entity, pos);
        }
        entity
    }

    fn parse_type_arguments_of_type_reference(&mut self) -> Option<NodeList> {
        if !self.has_preceding_line_break()
            && self.rescan_less_than_token() == SyntaxKind::LessThanToken
        {
            self.parse_type_arguments()
        } else {
            None
        }
    }

    fn parse_type_arguments(&mut self) -> Option<NodeList> {
        if self.token == SyntaxKind::LessThanToken {
            self.parse_bracketed_list(
                ParsingContext::TypeArguments,
                |p| Some(p.parse_type()),
                SyntaxKind::LessThanToken,
                SyntaxKind::GreaterThanToken,
            )
        } else {
            None
        }
    }

    fn parse_jsdoc_all_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let node = self.nodes.create(SyntaxKind::JSDocAllType, ());
        self.finish_node(node, pos)
    }

    fn parse_jsdoc_nullable_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        // skip the ?
        self.next_token();
        let type_node = self.parse_type_operator_or_higher();
        let node =
            self.nodes.create(SyntaxKind::JSDocNullableType, JSDocNullableType { type_node });
        self.finish_node(node, pos)
    }

    fn parse_jsdoc_non_nullable_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.next_token();
        let type_node = self.parse_type_operator_or_higher();
        let node =
            self.nodes.create(SyntaxKind::JSDocNonNullableType, JSDocNonNullableType { type_node });
        self.finish_node(node, pos)
    }

    fn parse_literal_type_node(&mut self, negative: bool) -> NodeId {
        let pos = self.node_pos();
        if negative {
            self.next_token();
        }
        let mut expression = if matches!(
            self.token,
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword
        ) {
            self.parse_keyword_expression()
        } else {
            self.parse_literal_expression()
        };
        if negative {
            expression = self.nodes.create(
                SyntaxKind::PrefixUnaryExpression,
                PrefixUnaryExpression { operator: SyntaxKind::MinusToken, expression },
            )
        }
        let node = self.nodes.create(SyntaxKind::LiteralType, LiteralType { expression });
        self.finish_node(node, pos)
    }

    fn parse_this_type_predicate(&mut self, lhs: NodeId) -> NodeId {
        self.next_token();
        let type_node = self.parse_type();
        let node = self.nodes.create(
            SyntaxKind::TypePredicate,
            TypePredicate {
                asserts_modifier: None,
                parameter_name: lhs,
                type_node: Some(type_node),
            },
        );
        self.finish_node(node, self.nodes[lhs].loc.pos as usize)
    }

    fn parse_import_type(&mut self) -> NodeId {
        self.source_flags.insert(NodeFlags::PossiblyContainsDynamicImport);
        let pos = self.node_pos();
        let is_typeof = self.parse_optional(SyntaxKind::TypeOfKeyword);
        self.parse_expected(SyntaxKind::ImportKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let type_node = self.parse_type();
        let mut attributes = None;
        if self.parse_optional(SyntaxKind::CommaToken) {
            let open_brace_position = self.scanner.token_start();
            self.parse_expected(SyntaxKind::OpenBraceToken);
            let current_token = self.token;
            if current_token == SyntaxKind::WithKeyword
                || current_token == SyntaxKind::AssertKeyword
            {
                if current_token == SyntaxKind::AssertKeyword {
                    self.parse_error_at_current_token(Message::e2880_import_assertions_have_been_replaced_by_import_attributes_use_with_instead_of_assert(), []);
                }
                self.next_token();
            } else {
                self.parse_error_at_current_token(
                    Message::e1005_0_expected(),
                    [token_to_text(SyntaxKind::WithKeyword).to_string()],
                );
            }
            self.parse_expected(SyntaxKind::ColonToken);
            attributes = Some(self.parse_import_attributes(current_token, true));
            self.parse_optional(SyntaxKind::CommaToken);
            if !self.parse_expected(SyntaxKind::CloseBraceToken) {
                if let Some(last) = self
                    .diagnostics
                    .last_and(|d| d.message.code == Message::e1005_0_expected().code)
                {
                    self.diagnostics.add_related_info(
                        last,
                        Message::e1007_the_parser_expected_to_find_a_1_to_match_the_0_token_here(),
                        TextRange::new(open_brace_position, open_brace_position),
                        [String::from("{"), String::from("}")],
                    );
                }
            }
        }
        self.parse_expected(SyntaxKind::CloseParenToken);
        let qualifier = if self.parse_optional(SyntaxKind::DotToken) {
            Some(self.parse_entity_name_of_type_reference())
        } else {
            None
        };
        let type_arguments = self.parse_type_arguments_of_type_reference();
        let node = self.nodes.create(
            SyntaxKind::ImportType,
            ImportType { is_typeof, type_node, attributes, qualifier, type_arguments },
        );
        self.finish_node(node, pos)
    }

    fn parse_import_attributes(&mut self, token: SyntaxKind, skip_keyword: bool) -> NodeId {
        let pos = self.node_pos();
        if !skip_keyword {
            self.parse_expected(token);
        }
        let elements;
        let mut multiline = false;
        let open_brace_position = self.scanner.token_start();
        if self.parse_expected(SyntaxKind::OpenBraceToken) {
            multiline = self.has_preceding_line_break();
            elements = self
                .parse_delimited_list(ParsingContext::ImportAttributes, |p| {
                    Some(p.parse_import_attribute())
                })
                .unwrap();
            if !self.parse_expected(SyntaxKind::CloseBraceToken) {
                if let Some(last) = self
                    .diagnostics
                    .last_and(|d| d.message.code == Message::e1005_0_expected().code)
                {
                    self.diagnostics.add_related_info(
                        last,
                        Message::e1007_the_parser_expected_to_find_a_1_to_match_the_0_token_here(),
                        TextRange::new(open_brace_position, open_brace_position),
                        [String::from("{"), String::from("}")],
                    );
                }
            }
        } else {
            elements = self.parse_empty_node_list();
        }
        let node = self
            .nodes
            .create(SyntaxKind::ImportAttributes, ImportAttributes { token, elements, multiline });
        self.finish_node(node, pos)
    }

    fn parse_import_attribute(&mut self) -> NodeId {
        let pos = self.node_pos();
        let name = if self.token.is_identifier_or_keyword() {
            Some(self.parse_identifier_name())
        } else if self.token == SyntaxKind::StringLiteral {
            Some(self.parse_literal_expression())
        } else {
            None
        };
        if name.is_some() {
            self.parse_expected(SyntaxKind::ColonToken);
        } else {
            self.parse_error_at_current_token(
                Message::e1478_identifier_or_string_literal_expected(),
                [],
            );
        }
        let value = self.parse_assignment_expression_or_higher();
        let node = self.nodes.create(SyntaxKind::ImportAttribute, ImportAttribute { name, value });
        self.finish_node(node, pos)
    }

    fn parse_empty_node_list(&mut self) -> NodeList {
        NodeList { loc: TextRange::new(self.node_pos(), self.node_pos()), nodes: Vec::new() }
    }

    fn parse_type_literal(&mut self) -> NodeId {
        let pos = self.node_pos();
        let members = self.parse_object_type_members();
        let node = self.nodes.create(SyntaxKind::TypeLiteral, TypeLiteral { members });
        self.finish_node(node, pos)
    }

    fn parse_object_type_members(&mut self) -> NodeList {
        if self.parse_expected(SyntaxKind::OpenBraceToken) {
            let members = self.parse_list(ParsingContext::TypeMembers, Self::parse_type_member);
            self.parse_expected(SyntaxKind::CloseBraceToken);
            members
        } else {
            NodeList::missing()
        }
    }

    fn parse_type_member(&mut self) -> NodeId {
        if matches!(self.token, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken) {
            return self.parse_signature_member(SyntaxKind::CallSignature);
        }
        if self.token == SyntaxKind::NewKeyword
            && self.look_ahead(Self::next_token_is_open_paren_or_less_than)
        {
            return self.parse_signature_member(SyntaxKind::ConstructSignature);
        }
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers();
        if self.parse_contextual_modifier(SyntaxKind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::GetAccessor,
                ParseFlags::Type,
            );
        }
        if self.parse_contextual_modifier(SyntaxKind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                SyntaxKind::SetAccessor,
                ParseFlags::Type,
            );
        }
        if self.is_index_signature() {
            return self.parse_index_signature_declaration(pos, jsdoc, modifiers);
        }
        self.parse_property_or_method_signature(pos, jsdoc, modifiers)
    }

    fn parse_signature_member(&mut self, kind: SyntaxKind) -> NodeId {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if kind == SyntaxKind::ConstructSignature {
            self.parse_expected(SyntaxKind::NewKeyword);
        }
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::Type);
        let type_node = self.parse_return_type(SyntaxKind::ColonToken, true);
        self.parse_type_member_semicolon();
        let node = if kind == SyntaxKind::CallSignature {
            self.nodes.create(
                SyntaxKind::CallSignature,
                CallSignature { type_parameters, parameters, type_node },
            )
        } else {
            self.nodes.create(
                SyntaxKind::ConstructSignature,
                ConstructSignature { type_parameters, parameters, type_node },
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_index_signature_declaration(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let parameters = self.parse_bracketed_list(
            ParsingContext::Parameters,
            Self::parse_parameter,
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
        );
        let type_node = self.parse_type_annotation();
        self.parse_type_member_semicolon();
        let node = self.nodes.create(
            SyntaxKind::IndexSignature,
            IndexSignature { modifiers, parameters, type_node },
        );
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_property_or_method_signature(
        &mut self,
        pos: usize,
        jsdoc: JSDocScannerInfo,
        modifiers: Option<ModifierList>,
    ) -> NodeId {
        let name = self.parse_property_name();
        let question_token = self.parse_optional_token(SyntaxKind::QuestionToken);
        let node = if matches!(self.token, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken) {
            // Method signatures don't exist in expression contexts.  So they have neither
            // [Yield] nor [Await]
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameters(ParseFlags::Type);
            let return_type = self.parse_return_type(SyntaxKind::ColonToken, true);
            self.nodes.create(
                SyntaxKind::MethodSignature,
                MethodSignature {
                    modifiers,
                    name,
                    question_token,
                    type_parameters,
                    parameters,
                    return_type,
                },
            )
        } else {
            let type_node = self.parse_type_annotation();
            // Although type literal properties cannot not have initializers, we attempt
            // to parse an initializer so we can report in the checker that an interface
            // property or type literal property cannot have an initializer.
            let initializer =
                if self.token == SyntaxKind::EqualsToken { self.parse_initializer() } else { None };
            self.nodes.create(
                SyntaxKind::PropertySignature,
                PropertySignature { modifiers, name, question_token, type_node, initializer },
            )
        };
        self.parse_type_member_semicolon();
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn is_index_signature(&mut self) -> bool {
        self.token == SyntaxKind::OpenBracketToken
            && self.look_ahead(Self::next_is_unambiguously_index_signature)
    }

    fn next_is_unambiguously_index_signature(&mut self) -> bool {
        // The only allowed sequence is:
        //
        //   [id:
        //
        // However, for error recovery, we also check the following cases:
        //
        //   [...
        //   [id,
        //   [id?,
        //   [id?:
        //   [id?]
        //   [public id
        //   [private id
        //   [protected id
        //   []
        //
        self.next_token();
        if self.token == SyntaxKind::DotDotDotToken || self.token == SyntaxKind::CloseBracketToken {
            return true;
        }
        if self.token.is_modifier() {
            self.next_token();
            if self.is_identifier() {
                return true;
            }
        } else if !self.is_identifier() {
            return false;
        } else {
            // Skip the identifier
            self.next_token();
        }

        // A colon signifies a well formed indexer
        // A comma should be a badly formed indexer because comma expressions are not allowed
        // in computed properties.
        if matches!(self.token, SyntaxKind::ColonToken | SyntaxKind::CommaToken) {
            return true;
        }
        // Question mark could be an indexer with an optional property,
        // or it could be a conditional expression in a computed property.
        if self.token != SyntaxKind::QuestionToken {
            return false;
        }
        // If any of the following tokens are after the question mark, it cannot
        // be a conditional expression, so treat it as an indexer.
        self.next_token();
        matches!(
            self.token,
            SyntaxKind::ColonToken | SyntaxKind::CommaToken | SyntaxKind::CloseBracketToken
        )
    }

    fn parse_type_query(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::TypeOfKeyword);
        let entity_name = self.parse_entity_name(true, None);
        // Make sure we perform ASI to prevent parsing the next line's type arguments as part of an instantiation expression
        let type_arguments =
            if !self.has_preceding_line_break() { self.parse_type_arguments() } else { None };
        let node =
            self.nodes.create(SyntaxKind::TypeQuery, TypeQuery { entity_name, type_arguments });
        self.finish_node(node, pos)
    }

    fn parse_mapped_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let mut readonly_token = None;
        if matches!(
            self.token,
            SyntaxKind::ReadonlyKeyword | SyntaxKind::PlusToken | SyntaxKind::MinusToken
        ) {
            let token = self.parse_token_node();
            if !self.nodes.is(token, SyntaxKind::ReadonlyKeyword) {
                self.parse_expected(SyntaxKind::ReadonlyKeyword);
            }
            readonly_token = Some(token);
        }
        self.parse_expected(SyntaxKind::OpenBracketToken);
        let type_parameter = self.parse_mapped_type_parameter();
        let mut name_type = None;
        if self.parse_optional(SyntaxKind::AsKeyword) {
            name_type = Some(self.parse_type());
        }
        self.parse_expected(SyntaxKind::CloseBracketToken);
        let mut question_token = None;
        if matches!(
            self.token,
            SyntaxKind::QuestionToken | SyntaxKind::PlusToken | SyntaxKind::MinusToken
        ) {
            let token = self.parse_token_node();
            if !self.nodes.is(token, SyntaxKind::QuestionToken) {
                self.parse_expected(SyntaxKind::QuestionToken);
            }
            question_token = Some(token);
        }
        let type_node = self.parse_type_annotation();
        self.parse_semicolon();
        let members = self.parse_list(ParsingContext::TypeMembers, Self::parse_type_member);
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let node = self.nodes.create(
            SyntaxKind::MappedType,
            MappedType {
                readonly_token,
                type_parameter,
                name_type,
                question_token,
                type_node,
                members,
            },
        );
        self.finish_node(node, pos)
    }

    fn parse_mapped_type_parameter(&mut self) -> NodeId {
        let pos = self.node_pos();
        let name = self.parse_identifier_name();
        self.parse_expected(SyntaxKind::InKeyword);
        let type_node = self.parse_type();
        let node = self.nodes.create(
            SyntaxKind::TypeParameter,
            TypeParameter {
                modifiers: None,
                name,
                constraint: Some(type_node),
                expression: None,
                default_type: None,
            },
        );
        self.finish_node(node, pos)
    }

    fn parse_tuple_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        let elements = self
            .parse_bracketed_list(
                ParsingContext::TupleElementTypes,
                |p| Some(p.parse_tuple_element_name_or_tuple_element_type()),
                SyntaxKind::OpenBracketToken,
                SyntaxKind::CloseBracketToken,
            )
            .unwrap();
        let node = self.nodes.create(SyntaxKind::TupleType, TupleType { elements });
        self.finish_node(node, pos)
    }

    fn parse_tuple_element_name_or_tuple_element_type(&mut self) -> NodeId {
        if self.look_ahead(Self::scan_start_of_named_tuple_element) {
            let pos = self.node_pos();
            let jsdoc = self.jsdoc_scanner_info();
            let dot_dot_dot_token = self.parse_optional_token(SyntaxKind::DotDotDotToken);
            let name = self.parse_identifier_name();
            let question_token = self.parse_optional_token(SyntaxKind::QuestionToken);
            self.parse_expected(SyntaxKind::ColonToken);
            let type_node = self.parse_tuple_element_type();
            let node = self.nodes.create(
                SyntaxKind::NamedTupleMember,
                NamedTupleMember { dot_dot_dot_token, name, question_token, type_node },
            );
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            return node;
        }

        self.parse_tuple_element_type()
    }

    fn scan_start_of_named_tuple_element(&mut self) -> bool {
        if self.token == SyntaxKind::DotDotDotToken {
            self.next_token();
        }
        self.token.is_identifier_or_keyword() && self.next_token_is_colon_or_question_colon()
    }

    fn next_token_is_colon_or_question_colon(&mut self) -> bool {
        match self.next_token() {
            SyntaxKind::ColonToken => true,
            SyntaxKind::QuestionToken => self.next_token() == SyntaxKind::ColonToken,
            _ => false,
        }
    }

    fn parse_tuple_element_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        if self.parse_optional(SyntaxKind::DotDotDotToken) {
            let type_node = self.parse_type();
            let node = self.nodes.create(SyntaxKind::RestType, RestType { type_node });
            return self.finish_node(node, pos);
        }
        let type_node = self.parse_type();
        if self.nodes.is(type_node, SyntaxKind::JSDocNullableType) {
            let inner_type = self.nodes[type_node].data_ref::<JSDocNullableType>().type_node;
            if self.nodes[type_node].loc.pos == self.nodes[inner_type].loc.pos {
                let node = self
                    .nodes
                    .create(SyntaxKind::OptionalType, OptionalType { type_node: inner_type });
                self.nodes[node].flags = self.nodes[type_node].flags;
                self.nodes[node].loc = self.nodes[type_node].loc;
                self.nodes[inner_type].parent = Some(node);
                return node;
            }
        }
        type_node
    }

    fn parse_parenthesized_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::OpenParenToken);
        let type_node = self.parse_type();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let node =
            self.nodes.create(SyntaxKind::ParenthesizedType, ParenthesizedType { type_node });
        self.finish_node(node, pos)
    }

    fn parse_asserts_type_predicate(&mut self) -> NodeId {
        let pos = self.node_pos();
        let asserts_modifier = self.parse_expected_token(SyntaxKind::AssertsKeyword);
        let parameter_name = if self.token == SyntaxKind::ThisKeyword {
            self.parse_this_type_node()
        } else {
            self.parse_identifier()
        };
        let mut type_node = None;
        if self.parse_optional(SyntaxKind::IsKeyword) {
            type_node = Some(self.parse_type());
        }
        let node = self.nodes.create(
            SyntaxKind::TypePredicate,
            TypePredicate { asserts_modifier: Some(asserts_modifier), parameter_name, type_node },
        );
        self.finish_node(node, pos)
    }

    fn parse_template_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        let head = self.parse_template_head(false);
        let template_spans = self.parse_template_type_spans();
        let node = self
            .nodes
            .create(SyntaxKind::TemplateLiteralType, TemplateLiteralType { head, template_spans });
        self.finish_node(node, pos)
    }

    fn parse_template_type_spans(&mut self) -> NodeList {
        let pos = self.node_pos();
        let mut nodes = Vec::new();
        loop {
            let span = self.parse_template_type_span();
            nodes.push(span);

            let literal = self.nodes[span].data_ref::<TemplateLiteralTypeSpan>().literal;
            if self.nodes[literal].kind != SyntaxKind::TemplateMiddle {
                break;
            }
        }
        NodeList { loc: TextRange::new(pos, self.node_pos()), nodes }
    }

    fn parse_template_type_span(&mut self) -> NodeId {
        let pos = self.node_pos();
        let type_node = self.parse_type();
        let literal = self.parse_literal_of_template_span(false);
        let node = self.nodes.create(
            SyntaxKind::TemplateLiteralTypeSpan,
            TemplateLiteralTypeSpan { type_node, literal },
        );
        self.finish_node(node, pos)
    }

    fn next_is_start_of_type_of_import_type(&mut self) -> bool {
        self.next_token();
        self.token == SyntaxKind::ImportKeyword
    }

    fn next_is_start_of_mapped_type(&mut self) -> bool {
        self.next_token();
        if matches!(self.token, SyntaxKind::PlusToken | SyntaxKind::MinusToken) {
            return self.next_token() == SyntaxKind::ReadonlyKeyword;
        }
        if self.token == SyntaxKind::ReadonlyKeyword {
            self.next_token();
        }
        self.token == SyntaxKind::OpenBracketToken
            && self.next_token_is_identifier()
            && self.next_token() == SyntaxKind::InKeyword
    }

    fn parse_function_or_constructor_type_to_error(
        &mut self,
        is_in_union_type: bool,
        mut parse_constituent_type: impl FnMut(&mut Parser) -> NodeId,
    ) -> NodeId {
        // the function type and constructor type shorthand notation
        // are not allowed directly in unions and intersections, but we'll
        // try to parse them gracefully and issue a helpful message.
        if self.is_start_of_function_type_or_constructor_type() {
            let type_node = self.parse_function_or_constructor_type();
            let diagnostic = if self.nodes.is(type_node, SyntaxKind::FunctionType) {
                if is_in_union_type {
                    Message::e1385_function_type_notation_must_be_parenthesized_when_used_in_a_union_type()
                } else {
                    Message::e1387_function_type_notation_must_be_parenthesized_when_used_in_an_intersection_type()
                }
            } else {
                if is_in_union_type {
                    Message::e1386_constructor_type_notation_must_be_parenthesized_when_used_in_a_union_type()
                } else {
                    Message::e1388_constructor_type_notation_must_be_parenthesized_when_used_in_an_intersection_type()
                }
            };
            self.parse_error_at_range(self.nodes[type_node].loc, diagnostic, []);
            return type_node;
        }

        parse_constituent_type(self)
    }

    fn next_is_unambiguously_start_of_function_type(&mut self) -> bool {
        self.next_token();
        if self.token == SyntaxKind::CloseParenToken || self.token == SyntaxKind::DotDotDotToken {
            // ( )
            // ( ...
            return true;
        }
        if self.skip_parameter_start() {
            // We successfully skipped modifiers (if any) and an identifier or binding pattern,
            // now see if we have something that indicates a parameter declaration
            if matches!(
                self.token,
                SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::EqualsToken
            ) {
                // ( xxx :
                // ( xxx ,
                // ( xxx ?
                // ( xxx =
                return true;
            }
            if self.token == SyntaxKind::CloseParenToken
                && self.next_token() == SyntaxKind::EqualsGreaterThanToken
            {
                // ( xxx ) =>
                return true;
            }
        }
        return false;
    }

    fn next_token_is_new_keyword(&mut self) -> bool {
        self.next_token() == SyntaxKind::NewKeyword
    }

    fn skip_parameter_start(&mut self) -> bool {
        if self.token.is_modifier() {
            // Skip modifiers
            self.parse_modifiers();
        }
        self.parse_optional(SyntaxKind::DotDotDotToken);
        if self.is_identifier() || self.token == SyntaxKind::ThisKeyword {
            self.next_token();
            return true;
        }
        if self.token == SyntaxKind::OpenBracketToken || self.token == SyntaxKind::OpenBraceToken {
            // Return true if we can parse an array or object binding pattern with no errors
            let previous_error_count = self.diagnostics.len();
            self.parse_identifier_or_pattern();
            return previous_error_count == self.diagnostics.len();
        }
        return false;
    }

    fn parse_modifiers(&mut self) -> Option<ModifierList> {
        self.parse_modifiers_ex(false, false, false)
    }

    fn parse_modifiers_ex(
        &mut self,
        allow_decorators: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<ModifierList> {
        let mut has_leading_modifier = false;
        let mut has_trailing_decorator = false;
        let mut has_trailing_modifier = false;
        let mut has_static_modifier = false;
        // Decorators should be contiguous in a list of modifiers but can potentially appear in two places (i.e., `[...leadingDecorators, ...leadingModifiers, ...trailingDecorators, ...trailingModifiers]`).
        // The leading modifiers *should* only contain `export` and `default` when trailingDecorators are present, but we'll handle errors for any other leading modifiers in the checker.
        // It is illegal to have both leadingDecorators and trailingDecorators, but we will report that as a grammar check in the checker.
        // parse leading decorators
        let pos = self.node_pos();
        let mut list = Vec::new();
        loop {
            if allow_decorators && self.token == SyntaxKind::AtToken && !has_trailing_modifier {
                let decorator = self.parse_decorator();
                list.push(decorator);
                if has_leading_modifier {
                    has_trailing_decorator = true
                }
            } else {
                let Some(modifier) = self.try_parse_modifier(
                    has_static_modifier,
                    permit_const_as_modifier,
                    stop_on_start_of_class_static_block,
                ) else {
                    break;
                };
                if self.nodes.is(modifier, SyntaxKind::StaticKeyword) {
                    has_static_modifier = true
                }
                list.push(modifier);
                if has_trailing_decorator {
                    has_trailing_modifier = true
                } else {
                    has_leading_modifier = true
                }
            }
        }
        if !list.is_empty() {
            return Some(self.nodes.new_modifier_list(list, TextRange::new(pos, self.node_pos())));
        }
        None
    }

    fn try_parse_modifier(
        &mut self,
        has_static_modifier: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<NodeId> {
        let pos = self.node_pos();
        let kind = self.token;
        if self.token == SyntaxKind::ConstKeyword && permit_const_as_modifier {
            // We need to ensure that any subsequent modifiers appear on the same line
            // so that when 'const' is a standalone declaration, we don't issue an error.
            if !self.look_ahead(Self::next_token_is_on_same_line_and_can_follow_modifier) {
                return None;
            } else {
                self.next_token();
            }
        } else if stop_on_start_of_class_static_block
            && self.token == SyntaxKind::StaticKeyword
            && self.look_ahead(Self::next_token_is_open_brace)
        {
            return None;
        } else if has_static_modifier && self.token == SyntaxKind::StaticKeyword {
            return None;
        } else {
            if !self.parse_any_contextual_modifier() {
                return None;
            }
        }
        let node = self.nodes.create(kind, ());
        Some(self.finish_node(node, pos))
    }

    fn parse_decorator(&self) -> NodeId {
        todo!()
    }

    fn next_token_is_open_brace(&mut self) -> bool {
        self.next_token() == SyntaxKind::OpenBraceToken
    }

    fn parse_contextual_modifier(&mut self, kind: SyntaxKind) -> bool {
        let state = self.mark();
        if self.token == kind && self.next_token_can_follow_modifier() {
            return true;
        }
        self.rewind(state);
        false
    }

    fn parse_any_contextual_modifier(&mut self) -> bool {
        let state = self.mark();
        if self.token.is_modifier() && self.next_token_can_follow_modifier() {
            return true;
        }
        self.rewind(state);
        false
    }

    fn can_follow_modifier(&self) -> bool {
        matches!(
            self.token,
            SyntaxKind::OpenBracketToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::DotDotDotToken
        ) || self.is_literal_property_name()
    }

    fn next_token_can_follow_modifier(&mut self) -> bool {
        match self.token {
            SyntaxKind::ConstKeyword => {
                // 'const' is only a modifier if followed by 'enum'.
                self.next_token() == SyntaxKind::EnumKeyword
            }
            SyntaxKind::ExportKeyword => match self.next_token() {
                SyntaxKind::DefaultKeyword => {
                    self.look_ahead(Self::next_token_can_follow_default_keyword)
                }
                SyntaxKind::TypeKeyword => {
                    self.look_ahead(Self::next_token_can_follow_export_modifier)
                }
                _ => self.can_follow_export_modifier(),
            },
            SyntaxKind::DefaultKeyword => self.next_token_can_follow_default_keyword(),
            SyntaxKind::StaticKeyword => {
                self.next_token();
                self.can_follow_modifier()
            }
            SyntaxKind::GetKeyword | SyntaxKind::SetKeyword => {
                self.next_token();
                self.can_follow_get_or_set_keyword()
            }
            _ => self.next_token_is_on_same_line_and_can_follow_modifier(),
        }
    }

    fn next_token_can_follow_default_keyword(&mut self) -> bool {
        match self.next_token() {
            SyntaxKind::ClassKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::AtToken => true,
            SyntaxKind::AbstractKeyword => {
                self.look_ahead(Self::next_token_is_class_keyword_on_same_line)
            }
            SyntaxKind::AsyncKeyword => {
                self.look_ahead(Self::next_token_is_function_keyword_on_same_line)
            }
            _ => false,
        }
    }

    fn next_token_can_follow_export_modifier(&mut self) -> bool {
        self.next_token();
        self.can_follow_export_modifier()
    }

    fn can_follow_export_modifier(&mut self) -> bool {
        self.token == SyntaxKind::AtToken
            || !matches!(
                self.token,
                SyntaxKind::AsteriskToken | SyntaxKind::AsKeyword | SyntaxKind::OpenBraceToken
            ) && self.can_follow_modifier()
    }

    fn can_follow_get_or_set_keyword(&mut self) -> bool {
        self.token == SyntaxKind::OpenBracketToken || self.is_literal_property_name()
    }

    fn next_token_is_on_same_line_and_can_follow_modifier(&mut self) -> bool {
        self.next_token();
        !self.has_preceding_line_break() && self.can_follow_modifier()
    }

    fn next_token_is_class_keyword_on_same_line(&mut self) -> bool {
        self.next_token() == SyntaxKind::ClassKeyword && !self.has_preceding_line_break()
    }

    fn next_token_is_function_keyword_on_same_line(&mut self) -> bool {
        self.next_token() == SyntaxKind::FunctionKeyword && !self.has_preceding_line_break()
    }

    fn rescan_greater_than_token(&mut self) -> SyntaxKind {
        self.token = self.scanner.rescan_greater_than_token();
        self.token
    }

    fn rescan_less_than_token(&mut self) -> SyntaxKind {
        self.token = self.scanner.rescan_less_than_token();
        self.token
    }

    fn rescan_template_token(&mut self, is_tagged_template: bool) -> SyntaxKind {
        self.token = self.scanner.rescan_template_token(is_tagged_template);
        self.token
    }

    fn rescan_slash_token(&mut self) -> SyntaxKind {
        self.token = self.scanner.rescan_slash_token(false);
        self.token
    }

    fn skip_range_trivia(&mut self, range: TextRange) -> TextRange {
        TextRange::new(
            Scanner::skip_trivia(&self.scanner.text, range.pos as usize),
            range.end as usize,
        )
    }
}

fn get_jsdoc_comment_ranges(
    node_factory: &mut NodeFactory,
    node: NodeId,
    text: &str,
) -> Vec<CommentRange> {
    todo!()
}
