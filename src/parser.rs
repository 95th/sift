use crate::{
    ast::{JSDocInfo, Node},
    diagnostics::{Diagnostic, Diagnostics, Message},
    flags::{NodeFlags, ParsingContext},
    options::{LanguageVariant, ScriptKind},
    scanner::{Scanner, ScannerState},
    syntax::{OperatorPrecedence, SyntaxKind, TextRange},
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
    reparsed_clones: Vec<Node>,
    reparse_list: Vec<Node>,
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
        }
    }

    pub fn parse(&mut self, contents: String, script_kind: ScriptKind) {
        self.init(contents, script_kind);
        self.next_token();
        self.parse_worker()
    }

    fn parse_worker(&mut self) {
        let pos = self.node_pos();
        let statements = self.parse_list_index(
            ParsingContext::SourceElements,
            Self::parse_top_level_statement,
        );

        todo!()
    }

    fn parse_list_index(
        &mut self,
        context: ParsingContext,
        mut parse_element: impl FnMut(&mut Parser, usize) -> Node,
    ) -> Vec<Node> {
        let save_parsing_context = self.parsing_context;
        self.parsing_context.insert(context);
        let mut outer_reparse_list = std::mem::take(&mut self.reparse_list);

        let mut list = Vec::new();
        while !self.is_list_terminator(context) {
            if self.is_list_element(context, false) {
                let elt = parse_element(self, list.len());
                for e in self.reparse_list.drain(..) {
                    // Propagate @typedef type alias declarations outwards to a context that permits them.
                    if (e.is_js_type_alias_declaration() || e.is_js_import_declaration())
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

    fn abort_parsing_list_or_move_to_next_token(&self, context: ParsingContext) -> bool {
        todo!()
    }

    fn parse_top_level_statement(&mut self, index: usize) -> Node {
        todo!()
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
                    && self.next_token_and(Self::is_string_literal)
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
            ParsingContext::JSDocComment => return true,
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
                self.token == SyntaxKind::LessThanToken && self.next_token_and(Self::is_slash)
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

    fn next_token_and(&mut self, predicate: impl FnOnce(&mut Parser) -> bool) -> bool {
        self.look_ahead(|p| {
            p.next_token();
            predicate(p)
        })
    }

    fn node_pos(&self) -> usize {
        self.scanner.full_token_start()
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
            | SyntaxKind::FinallyKeyword => return true,
            SyntaxKind::ImportKeyword => {
                self.is_start_of_declaration()
                    || self.next_token_and(Self::is_open_paren_or_less_than_or_dot)
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
                    || !self.next_token_and(Self::is_identifier_or_keyword_on_sameline)
            }

            _ => self.is_start_of_expression(),
        }
    }

    fn is_start_of_declaration(&mut self) -> bool {
        self.look_ahead(Self::scan_start_of_declaration)
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
        return self.is_identifier();
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
            | SyntaxKind::TemplateHead => return true,
            SyntaxKind::FunctionKeyword => return !in_start_of_parameter,
            SyntaxKind::MinusToken => {
                return !in_start_of_parameter
                    && self.next_token_and(Self::is_numeric_or_big_int_literal);
            }
            SyntaxKind::OpenParenToken => {
                // Only consider '(' the start of a type if followed by ')', '...', an identifier, a modifier,
                // or something that starts a type. We don't want to consider things like '(1)' a type.
                return !in_start_of_parameter
                    && self.next_token_and(Self::is_parenthesized_or_function_type);
            }
            _ => self.is_identifier(),
        }
    }

    fn is_heritage_clause_extends_or_implements_keyword(&mut self) -> bool {
        self.is_heritage_clause() && self.next_token_and(Self::is_start_of_expression)
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
            | SyntaxKind::Identifier => return true,
            SyntaxKind::ImportKeyword => {
                return self.next_token_and(Self::is_open_paren_or_less_than_or_dot);
            }
            _ => return self.is_identifier(),
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
        return false;
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
        return false;
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
                    return self.next_token_and(Self::is_identifier_on_same_line);
                }
                SyntaxKind::ModuleKeyword | SyntaxKind::NamespaceKeyword => {
                    return self.next_token_and(Self::is_identifier_or_string_literal_on_same_line);
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

    fn is_string_literal(&mut self) -> bool {
        self.token == SyntaxKind::StringLiteral
    }

    fn is_slash(&mut self) -> bool {
        self.token == SyntaxKind::SlashToken
    }

    fn is_identifier_or_keyword_on_sameline(&mut self) -> bool {
        self.token.is_identifier_or_keyword() && !self.has_preceding_line_break()
    }

    fn is_open_paren_or_less_than_or_dot(&mut self) -> bool {
        matches!(
            self.token,
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
        self.next_token_and(|p| {
            if p.token == SyntaxKind::CloseBraceToken {
                // if we see "extends {}" then only treat the {} as what we're extending (and not
                // the class body) if we have:
                //
                //      extends {} {
                //      extends {},
                //      extends {} extends
                //      extends {} implements
                matches!(
                    p.next_token(),
                    SyntaxKind::CommaToken
                        | SyntaxKind::OpenBraceToken
                        | SyntaxKind::ExtendsKeyword
                        | SyntaxKind::ImplementsKeyword
                )
            } else {
                true
            }
        })
    }

    fn is_numeric_or_big_int_literal(&mut self) -> bool {
        matches!(
            self.token,
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
        )
    }

    fn is_parenthesized_or_function_type(&mut self) -> bool {
        self.token == SyntaxKind::CloseParenToken || self.is_start_of_parameter(false)
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
        self.next_token_and(|p| {
            p.is_binding_identifier_or_start_of_destructuring_on_same_line(false)
        })
    }

    fn is_await_using_declaration(&mut self) -> bool {
        self.next_token_and(
            Self::is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line,
        )
    }

    fn is_identifier_on_same_line(&mut self) -> bool {
        self.is_identifier() && !self.has_preceding_line_break()
    }

    fn is_identifier_or_string_literal_on_same_line(&mut self) -> bool {
        (self.is_identifier() || self.token == SyntaxKind::StringLiteral)
            && !self.has_preceding_line_break()
    }

    fn has_preceding_line_break(&self) -> bool {
        self.scanner.has_preceding_line_break()
    }

    fn is_binding_identifier_or_start_of_destructuring_on_same_line(
        &mut self,
        disallow_of: bool,
    ) -> bool {
        if disallow_of && self.token == SyntaxKind::OfKeyword {
            return self.next_token_and(Self::is_equals_or_semicolon_or_colon_token);
        }
        (self.is_binding_identifier() || self.token == SyntaxKind::OpenBraceToken)
            && !self.has_preceding_line_break()
    }

    fn is_equals_or_semicolon_or_colon_token(&mut self) -> bool {
        matches!(
            self.token,
            SyntaxKind::EqualsToken | SyntaxKind::SemicolonToken | SyntaxKind::ColonToken
        )
    }

    fn is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line(
        &mut self,
    ) -> bool {
        self.token == SyntaxKind::UsingKeyword
            && self.next_token_and(|p| {
                p.is_binding_identifier_or_start_of_destructuring_on_same_line(false)
            })
    }

    fn parse_error_at_current_token(
        &mut self,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) {
        self.parse_error_at_range(self.scanner.token_range(), message, args)
    }

    fn parse_error_at_range(
        &mut self,
        loc: TextRange,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) {
        // Don't report another error if it would just be at the same location as the last error
        if self.diagnostics.len() == 0
            || self
                .diagnostics
                .with(|d| d.last().unwrap().loc.pos != loc.pos)
        {
            self.diagnostics.report(message, loc, args);
        }
        self.has_parse_error = true;
    }
}
