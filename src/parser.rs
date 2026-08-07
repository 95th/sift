use crate::{
    ast::{
        ArrayBindingPattern, BigIntLiteral, BinaryExpression, BindingElement, Block, CommentRange,
        ComputedPropertyName, ConditionalType, Identifier, InferType, IntersectionType, JSDocInfo,
        ModifierList, NoSubstitutionTemplateLiteral, NodeFactory, NodeId, NodeList, NumericLiteral,
        ObjectBindingPattern, PrivateIdentifier, RegularExpressionLiteral, SourceFile,
        StringLiteral, TypeOperator, TypeParameter, UnionType, VariableDeclaration,
        VariableDeclarationList, VariableStatement,
    },
    diagnostics::{DiagnosticId, Diagnostics, Message},
    flags::{JSDocScannerInfo, ModifierFlags, NodeFlags, ParsingContext},
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

    identifier_count: usize,
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
            identifier_count: 0,
        }
    }

    pub fn parse_source_file(&mut self, contents: String, script_kind: ScriptKind) {
        self.init(contents, script_kind);
        self.next_token();
        self.parse_source_file_worker()
    }

    fn parse_source_file_worker(&mut self) {
        let pos = self.node_pos();
        let mut statements = self.parse_list_index(
            ParsingContext::SourceElements,
            Self::parse_top_level_statement,
        );
        let end = self.node_pos();
        let end_jsdoc = self.jsdoc_scanner_info();
        let eof = self.parse_token_node();
        self.with_jsdoc(eof, end_jsdoc);
        if self.nodes[eof].kind != SyntaxKind::EndOfFile {
            panic!("Expected end of file token from scanner.");
        }
        if !self.reparse_list.is_empty() {
            statements.extend(std::mem::take(&mut self.reparse_list));
        }
        let node = self.nodes.create(
            SyntaxKind::SourceFile,
            SourceFile {
                statements: NodeList {
                    loc: TextRange::new(pos, end),
                    nodes: statements,
                },
                source_text: self.scanner.text.clone(),
                eof_token: eof,
            },
        );
        self.finish_node(node, pos);
        todo!()
    }

    fn parse_list(
        &mut self,
        context: ParsingContext,
        mut parse_element: impl FnMut(&mut Parser) -> NodeId,
    ) -> NodeList {
        let pos = self.node_pos();
        let nodes = self.parse_list_index(context, |parser, _index| parse_element(parser));
        NodeList {
            loc: TextRange::new(pos, self.node_pos()),
            nodes,
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
        Some(NodeList {
            loc: TextRange::new(pos, self.node_pos()),
            nodes,
        })
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
            && !self.nodes[statement]
                .flags
                .contains(NodeFlags::AwaitContext)
        {
            if self
                .possible_await_spans
                .last()
                .is_none_or(|&last| last != i)
            {
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
                matches!(
                    self.token,
                    SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken
                )
            }
            ParsingContext::ArrayLiteralMembers
            | ParsingContext::TupleElementTypes
            | ParsingContext::ArrayBindingElements => self.token == SyntaxKind::CloseBracketToken,
            ParsingContext::JSDocParameters
            | ParsingContext::Parameters
            | ParsingContext::RestProperties => {
                // Tokens other than ')' and ']' (the latter for index signatures) are here for better error recovery
                matches!(
                    self.token,
                    SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                )
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
                matches!(
                    self.token,
                    SyntaxKind::GreaterThanToken | SyntaxKind::SlashToken
                )
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
                self.context_flags
                    .insert(NodeFlags::JavaScriptFile | NodeFlags::JsonFile);
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
                    || !self.look_ahead(Self::next_token_is_identifier_or_keyword_on_sameline)
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
        matches!(
            self.token,
            SyntaxKind::ExtendsKeyword | SyntaxKind::ImplementsKeyword
        )
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

    fn next_token_is_identifier_or_keyword_on_sameline(&mut self) -> bool {
        self.next_token_is_identifier_or_keyword() && !self.has_preceding_line_break()
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
        matches!(
            self.next_token(),
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
        )
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
        self.context_flags
            .contains(NodeFlags::DisallowConditionalTypesContext)
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
        if self.diagnostics.len() == 0
            || self
                .diagnostics
                .with(|d| d.last().unwrap().loc.pos != loc.pos)
        {
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
        self.nodes
            .for_each_child(node, |child| child.parent = self.current_parent);
        self.current_parent = None;
    }

    fn with_jsdoc(&mut self, node: NodeId, info: JSDocScannerInfo) -> Vec<NodeId> {
        if !info.contains(JSDocScannerInfo::HasJSDoc) {
            return Vec::new();
        }

        // For TS/TSX files, defer JSDoc parsing to first access, unless the comment
        // contains @see/@link (needed for unused-identifier checks).
        // @deprecated is detected via cheap text scan to set PossiblyContainsDeprecatedTag;
        // callers must confirm via JSDoc lookup.
        if !self.is_javascript() {
            self.nodes[node].flags.insert(NodeFlags::HasJSDoc);
            if info.contains(JSDocScannerInfo::HasDeprecated) {
                self.nodes[node]
                    .flags
                    .insert(NodeFlags::PossiblyContainsDeprecatedTag);
            }
            if !info.contains(JSDocScannerInfo::HasSeeOrLink) {
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
                self.nodes[node]
                    .flags
                    .insert(NodeFlags::PossiblyContainsDeprecatedTag);
            }
            if self.is_javascript() {
                self.reparse_tags(node, &jsdoc);
            }
            self.jsdoc_infos.push(JSDocInfo {
                parent: node,
                jsdocs: jsdoc.clone(),
            });
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
                [
                    token_to_text(open_token).to_string(),
                    token_to_text(close_token).to_string(),
                ],
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
        if self.token == token {
            Some(self.parse_token_node())
        } else {
            None
        }
    }

    pub fn parse_statement(&mut self) -> NodeId {
        match self.token {
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::OpenBraceToken => self.parse_block(false, None),
            SyntaxKind::VarKeyword => {
                self.parse_variable_statement(self.node_pos(), self.jsdoc_scanner_info(), None)
            }
            _ => todo!(),
        }
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
            let node = self.nodes.create(
                SyntaxKind::Block,
                Block {
                    statements,
                    multiline,
                },
            );
            self.finish_node(node, pos);
            self.with_jsdoc(node, jsdoc);
            if self.token == SyntaxKind::EqualsToken {
                self.parse_error_at_current_token(Message::e2809_declaration_or_statement_expected_this_follows_a_block_of_statements_so_if_you_intended_to_write_a_destructuring_assignment_you_might_need_to_wrap_the_whole_assignment_in_parentheses(), []);
                self.next_token();
            }
            return node;
        }

        let node = self.nodes.create(
            SyntaxKind::Block,
            Block {
                statements: NodeList::missing(),
                multiline: false,
            },
        );
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
            VariableStatement {
                modifiers,
                declaration_list,
            },
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
            VariableDeclarationList {
                declarations,
                flags,
            },
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
            && self.nodes[name].kind == SyntaxKind::Identifier
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
            || node
                .flags
                .intersects(NodeFlags::JSDoc | NodeFlags::Reparsed)
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
        if self.parse_optional(SyntaxKind::ColonToken) {
            Some(self.parse_type())
        } else {
            None
        }
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
        let node = self.nodes.create(
            SyntaxKind::ArrayBindingPattern,
            ArrayBindingPattern { elements },
        );
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
        let node = self.nodes.create(
            SyntaxKind::ObjectBindingPattern,
            ObjectBindingPattern { elements },
        );
        self.finish_node(node, pos)
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
            BindingElement {
                dot_dot_dot_token,
                property_name: None,
                name,
                initializer,
            },
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
            BindingElement {
                dot_dot_dot_token,
                property_name,
                name,
                initializer,
            },
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
            let identifier = self.new_identifier(text);
            let node = self.nodes.create(SyntaxKind::Identifier, identifier);
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
        let identifier = self.new_identifier(String::new());
        let node = self.nodes.create(SyntaxKind::Identifier, identifier);
        self.finish_node(node, self.node_pos());
        node
    }

    fn new_identifier(&mut self, text: String) -> Identifier {
        self.identifier_count += 1;
        if text == "await" {
            self.statement_has_await_identifier = true;
        }
        Identifier { text }
    }

    fn parse_literal_expression(&mut self) -> NodeId {
        let pos = self.node_pos();
        let text = self.scanner.token_value().to_string();
        let token_flags = self.scanner.token_flags();
        let node = match self.token {
            SyntaxKind::StringLiteral => self
                .nodes
                .create(self.token, StringLiteral { text, token_flags }),
            SyntaxKind::NumericLiteral => self
                .nodes
                .create(self.token, NumericLiteral { text, token_flags }),
            SyntaxKind::BigIntLiteral => self
                .nodes
                .create(self.token, BigIntLiteral { text, token_flags }),
            SyntaxKind::RegularExpressionLiteral => self
                .nodes
                .create(self.token, RegularExpressionLiteral { text, token_flags }),
            SyntaxKind::NoSubstitutionTemplateLiteral => self.nodes.create(
                self.token,
                NoSubstitutionTemplateLiteral { text, token_flags },
            ),
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
        let node = self.nodes.create(
            SyntaxKind::ComputedPropertyName,
            ComputedPropertyName { expression },
        );
        self.finish_node(node, pos)
    }

    fn parse_private_identifier(&mut self) -> NodeId {
        let pos = self.node_pos();
        let text = self.scanner.token_value().to_string();
        self.next_token();
        let node = self
            .nodes
            .create(SyntaxKind::PrivateIdentifier, PrivateIdentifier { text });
        self.finish_node(node, pos)
    }

    fn parse_identifier_name(&mut self) -> NodeId {
        self.parse_identifier_name_with_diagnostic(None)
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

    fn parse_assignment_expression_or_higher(&self) -> NodeId {
        todo!()
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
                    ConditionalType {
                        type_node,
                        extends_type,
                        true_type,
                        false_type,
                    },
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
            BinaryExpression {
                left,
                operator_token,
                right,
                modifiers: None,
                type_node: None,
            },
        );
        self.finish_node(node, pos)
    }

    fn is_start_of_function_type_or_constructor_type(&mut self) -> bool {
        self.token == SyntaxKind::LessThanToken
            || self.token == SyntaxKind::OpenParenToken
                && self.look_ahead(Self::next_is_unambiguously_start_of_function_type)
            || self.token == SyntaxKind::NewKeyword
            || self.token == SyntaxKind::AbstractKeyword
                && self.look_ahead(Self::next_token_is_new_keyword)
    }

    fn parse_function_or_constructor_type(&self) -> NodeId {
        todo!()
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
                NodeList {
                    loc: TextRange::new(pos, self.node_pos()),
                    nodes: types,
                },
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
            SyntaxKind::BarToken => self
                .nodes
                .create(SyntaxKind::UnionType, UnionType { types }),
            SyntaxKind::AmpersandToken => self
                .nodes
                .create(SyntaxKind::IntersectionType, IntersectionType { types }),
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
        let node = self.nodes.create(
            SyntaxKind::TypeOperator,
            TypeOperator {
                operator,
                type_node,
            },
        );
        self.finish_node(node, pos)
    }

    fn parse_infer_type(&mut self) -> NodeId {
        let pos = self.node_pos();
        self.parse_expected(SyntaxKind::InferKeyword);
        let type_parameter = self.parse_type_parameter_of_infer_type();
        let node = self
            .nodes
            .create(SyntaxKind::InferType, InferType { type_parameter });
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
            let constraint = self.in_context(
                NodeFlags::DisallowConditionalTypesContext,
                true,
                Self::parse_type,
            );
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
        todo!()
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
            let diagnostic = if self.nodes[type_node].kind == SyntaxKind::FunctionType {
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
                if self.nodes[modifier].kind == SyntaxKind::StaticKeyword {
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
            return Some(
                self.nodes
                    .new_modifier_list(list, TextRange::new(pos, self.node_pos())),
            );
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
}

fn get_jsdoc_comment_ranges(
    node_factory: &mut NodeFactory,
    node: NodeId,
    text: &str,
) -> Vec<CommentRange> {
    todo!()
}
