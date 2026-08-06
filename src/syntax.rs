use crate::flags::ModifierFlags;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    #[default]
    Unknown,
    EndOfFile,
    SingleLineCommentTrivia,
    MultiLineCommentTrivia,
    NewLineTrivia,
    WhitespaceTrivia,
    ConflictMarkerTrivia,
    NonTextFileMarkerTrivia,
    NumericLiteral,
    BigIntLiteral,
    StringLiteral,
    JsxText,
    JsxTextAllWhiteSpaces,
    RegularExpressionLiteral,
    NoSubstitutionTemplateLiteral,
    // Pseudo-literals
    TemplateHead,
    TemplateMiddle,
    TemplateTail,
    // Punctuation
    OpenBraceToken,
    CloseBraceToken,
    OpenParenToken,
    CloseParenToken,
    OpenBracketToken,
    CloseBracketToken,
    DotToken,
    DotDotDotToken,
    SemicolonToken,
    CommaToken,
    QuestionDotToken,
    LessThanToken,
    LessThanSlashToken,
    GreaterThanToken,
    LessThanEqualsToken,
    GreaterThanEqualsToken,
    EqualsEqualsToken,
    ExclamationEqualsToken,
    EqualsEqualsEqualsToken,
    ExclamationEqualsEqualsToken,
    EqualsGreaterThanToken,
    PlusToken,
    MinusToken,
    AsteriskToken,
    AsteriskAsteriskToken,
    SlashToken,
    PercentToken,
    PlusPlusToken,
    MinusMinusToken,
    LessThanLessThanToken,
    GreaterThanGreaterThanToken,
    GreaterThanGreaterThanGreaterThanToken,
    AmpersandToken,
    BarToken,
    CaretToken,
    ExclamationToken,
    TildeToken,
    AmpersandAmpersandToken,
    BarBarToken,
    QuestionToken,
    ColonToken,
    AtToken,
    QuestionQuestionToken,
    // Only the JSDoc scanner produces BacktickToken. The normal scanner produces NoSubstitutionTemplateLiteral and related kinds.
    BacktickToken,
    // Only the JSDoc scanner produces HashToken. The normal scanner produces PrivateIdentifier.
    HashToken,
    // Assignments
    EqualsToken,
    PlusEqualsToken,
    MinusEqualsToken,
    AsteriskEqualsToken,
    AsteriskAsteriskEqualsToken,
    SlashEqualsToken,
    PercentEqualsToken,
    LessThanLessThanEqualsToken,
    GreaterThanGreaterThanEqualsToken,
    GreaterThanGreaterThanGreaterThanEqualsToken,
    AmpersandEqualsToken,
    BarEqualsToken,
    BarBarEqualsToken,
    AmpersandAmpersandEqualsToken,
    QuestionQuestionEqualsToken,
    CaretEqualsToken,
    // Identifiers and PrivateIdentifier
    Identifier,
    PrivateIdentifier,
    JSDocCommentTextToken,
    // Reserved words
    BreakKeyword,
    CaseKeyword,
    CatchKeyword,
    ClassKeyword,
    ConstKeyword,
    ContinueKeyword,
    DebuggerKeyword,
    DefaultKeyword,
    DeleteKeyword,
    DoKeyword,
    ElseKeyword,
    EnumKeyword,
    ExportKeyword,
    ExtendsKeyword,
    FalseKeyword,
    FinallyKeyword,
    ForKeyword,
    FunctionKeyword,
    IfKeyword,
    ImportKeyword,
    InKeyword,
    InstanceOfKeyword,
    NewKeyword,
    NullKeyword,
    ReturnKeyword,
    SuperKeyword,
    SwitchKeyword,
    ThisKeyword,
    ThrowKeyword,
    TrueKeyword,
    TryKeyword,
    TypeOfKeyword,
    VarKeyword,
    VoidKeyword,
    WhileKeyword,
    WithKeyword,
    // Strict mode reserved words
    ImplementsKeyword,
    InterfaceKeyword,
    LetKeyword,
    PackageKeyword,
    PrivateKeyword,
    ProtectedKeyword,
    PublicKeyword,
    StaticKeyword,
    YieldKeyword,
    // Contextual keywords
    AbstractKeyword,
    AccessorKeyword,
    AsKeyword,
    AssertsKeyword,
    AssertKeyword,
    AnyKeyword,
    AsyncKeyword,
    AwaitKeyword,
    BooleanKeyword,
    ConstructorKeyword,
    DeclareKeyword,
    GetKeyword,
    ImmediateKeyword,
    InferKeyword,
    IntrinsicKeyword,
    IsKeyword,
    KeyOfKeyword,
    ModuleKeyword,
    NamespaceKeyword,
    NeverKeyword,
    OutKeyword,
    ReadonlyKeyword,
    RequireKeyword,
    NumberKeyword,
    ObjectKeyword,
    SatisfiesKeyword,
    SetKeyword,
    StringKeyword,
    SymbolKeyword,
    TypeKeyword,
    UndefinedKeyword,
    UniqueKeyword,
    UnknownKeyword,
    UsingKeyword,
    FromKeyword,
    GlobalKeyword,
    BigIntKeyword,
    OverrideKeyword,
    OfKeyword,
    DeferKeyword,

    // Parse tree nodes
    // Names
    QualifiedName,
    ComputedPropertyName,
    // Signature elements
    TypeParameter,
    Parameter,
    Decorator,
    // TypeMember
    PropertySignature,
    PropertyDeclaration,
    MethodSignature,
    MethodDeclaration,
    ClassStaticBlockDeclaration,
    Constructor,
    GetAccessor,
    SetAccessor,
    CallSignature,
    ConstructSignature,
    IndexSignature,
    // Type
    TypePredicate,
    TypeReference,
    FunctionType,
    ConstructorType,
    TypeQuery,
    TypeLiteral,
    ArrayType,
    TupleType,
    OptionalType,
    RestType,
    UnionType,
    IntersectionType,
    ConditionalType,
    InferType,
    ParenthesizedType,
    ThisType,
    TypeOperator,
    IndexedAccessType,
    MappedType,
    LiteralType,
    NamedTupleMember,
    TemplateLiteralType,
    TemplateLiteralTypeSpan,
    ImportType,
    // Binding patterns
    ObjectBindingPattern,
    ArrayBindingPattern,
    BindingElement,
    // Expression
    ArrayLiteralExpression,
    ObjectLiteralExpression,
    PropertyAccessExpression,
    ElementAccessExpression,
    CallExpression,
    NewExpression,
    TaggedTemplateExpression,
    TypeAssertionExpression,
    ParenthesizedExpression,
    FunctionExpression,
    ArrowFunction,
    DeleteExpression,
    TypeOfExpression,
    VoidExpression,
    AwaitExpression,
    PrefixUnaryExpression,
    PostfixUnaryExpression,
    BinaryExpression,
    ConditionalExpression,
    TemplateExpression,
    YieldExpression,
    SpreadElement,
    ClassExpression,
    OmittedExpression,
    ExpressionWithTypeArguments,
    AsExpression,
    NonNullExpression,
    MetaProperty,
    SyntheticExpression,
    SatisfiesExpression,
    // Misc
    TemplateSpan,
    SemicolonClassElement,
    // Element
    Block,
    EmptyStatement,
    VariableStatement,
    ExpressionStatement,
    IfStatement,
    DoStatement,
    WhileStatement,
    ForStatement,
    ForInStatement,
    ForOfStatement,
    ContinueStatement,
    BreakStatement,
    ReturnStatement,
    WithStatement,
    SwitchStatement,
    LabeledStatement,
    ThrowStatement,
    TryStatement,
    DebuggerStatement,
    VariableDeclaration,
    VariableDeclarationList,
    FunctionDeclaration,
    ClassDeclaration,
    InterfaceDeclaration,
    TypeAliasDeclaration,
    EnumDeclaration,
    ModuleDeclaration,
    ModuleBlock,
    CaseBlock,
    NamespaceExportDeclaration,
    ImportEqualsDeclaration,
    ImportDeclaration,
    ImportClause,
    NamespaceImport,
    NamedImports,
    ImportSpecifier,
    ExportAssignment,
    ExportDeclaration,
    NamedExports,
    NamespaceExport,
    ExportSpecifier,
    MissingDeclaration,
    // Module references
    ExternalModuleReference,
    // JSX
    JsxElement,
    JsxSelfClosingElement,
    JsxOpeningElement,
    JsxClosingElement,
    JsxFragment,
    JsxOpeningFragment,
    JsxClosingFragment,
    JsxAttribute,
    JsxAttributes,
    JsxSpreadAttribute,
    JsxExpression,
    JsxNamespacedName,
    // Clauses
    CaseClause,
    DefaultClause,
    HeritageClause,
    CatchClause,
    // Import attributes
    ImportAttributes,
    ImportAttribute,
    // Property assignments
    PropertyAssignment,
    ShorthandPropertyAssignment,
    SpreadAssignment,
    // Enum
    EnumMember,
    // Top-level nodes
    SourceFile,
    // JSDoc nodes
    JSDocTypeExpression,
    JSDocNameReference,
    JSDocAllType, // The * type
    JSDocNullableType,
    JSDocNonNullableType,
    JSDocOptionalType,
    JSDocVariadicType,
    JSDoc,
    JSDocText,
    JSDocTypeLiteral,
    JSDocSignature,
    JSDocLink,
    JSDocLinkCode,
    JSDocLinkPlain,
    JSDocUnknownTag,
    JSDocAugmentsTag,
    JSDocImplementsTag,
    JSDocDeprecatedTag,
    JSDocPublicTag,
    JSDocPrivateTag,
    JSDocProtectedTag,
    JSDocReadonlyTag,
    JSDocOverrideTag,
    JSDocCallbackTag,
    JSDocOverloadTag,
    JSDocParameterTag,
    JSDocReturnTag,
    JSDocThisTag,
    JSDocTypeTag,
    JSDocTemplateTag,
    JSDocTypedefTag,
    JSDocSeeTag,
    JSDocPropertyTag,
    JSDocThrowsTag,
    JSDocSatisfiesTag,
    JSDocImportTag,
    // Synthesized list
    SyntaxList,
    // Reparsed JS nodes
    JSTypeAliasDeclaration,
    JSImportDeclaration,
    // Transformation nodes
    NotEmittedStatement,
    PartiallyEmittedExpression,
    SyntheticReferenceExpression,
    NotEmittedTypeElement,
    Count,
}

#[rustfmt::skip]
impl SyntaxKind {
    pub const FIRST_ASSIGNMENT           : Self = Self::EqualsToken;
    pub const LAST_ASSIGNMENT            : Self = Self::CaretEqualsToken;
    pub const FIRST_COMPOUND_ASSIGNMENT  : Self = Self::PlusEqualsToken;
    pub const LAST_COMPOUND_ASSIGNMENT   : Self = Self::CaretEqualsToken;
    pub const FIRST_RESERVED_WORD        : Self = Self::BreakKeyword;
    pub const LAST_RESERVED_WORD         : Self = Self::WithKeyword;
    pub const FIRST_KEYWORD              : Self = Self::BreakKeyword;
    pub const LAST_KEYWORD               : Self = Self::DeferKeyword;
    pub const FIRST_FUTURE_RESERVED_WORD : Self = Self::ImplementsKeyword;
    pub const LAST_FUTURE_RESERVED_WORD  : Self = Self::YieldKeyword;
    pub const FIRST_TYPE_NODE            : Self = Self::TypePredicate;
    pub const LAST_TYPE_NODE             : Self = Self::ImportType;
    pub const FIRST_PUNCTUATION          : Self = Self::OpenBraceToken;
    pub const LAST_PUNCTUATION           : Self = Self::CaretEqualsToken;
    pub const FIRST_TOKEN                : Self = Self::Unknown;
    pub const LAST_TOKEN                 : Self = Self::LAST_KEYWORD;
    pub const FIRST_LITERAL_TOKEN        : Self = Self::NumericLiteral;
    pub const LAST_LITERAL_TOKEN         : Self = Self::NoSubstitutionTemplateLiteral;
    pub const FIRST_TEMPLATE_TOKEN       : Self = Self::NoSubstitutionTemplateLiteral;
    pub const LAST_TEMPLATE_TOKEN        : Self = Self::TemplateTail;
    pub const FIRST_BINARY_OPERATOR      : Self = Self::LessThanToken;
    pub const LAST_BINARY_OPERATOR       : Self = Self::CaretEqualsToken;
    pub const FIRST_STATEMENT            : Self = Self::VariableStatement;
    pub const LAST_STATEMENT             : Self = Self::DebuggerStatement;
    pub const FIRST_NODE                 : Self = Self::QualifiedName;
    pub const FIRST_JSDOC_NODE           : Self = Self::JSDocTypeExpression;
    pub const LAST_JSDOC_NODE            : Self = Self::JSDocImportTag;
    pub const FIRST_JSDOC_TAG_NODE       : Self = Self::JSDocUnknownTag;
    pub const LAST_JSDOC_TAG_NODE        : Self = Self::JSDocImportTag;
    pub const FIRST_CONTEXTUAL_KEYWORD   : Self = Self::AbstractKeyword;
    pub const LAST_CONTEXTUAL_KEYWORD    : Self = Self::DeferKeyword;
    pub const LAST_UNARY_OPERATOR        : Self = Self::TildeToken;
    pub const FIRST_TRIVIA_TOKEN         : Self = Self::SingleLineCommentTrivia;
    pub const LAST_TRIVIA_TOKEN          : Self = Self::ConflictMarkerTrivia;
}

impl SyntaxKind {
    pub fn is_keyword(self) -> bool {
        Self::FIRST_KEYWORD <= self && self <= Self::LAST_KEYWORD
    }

    pub fn is_identifier_or_keyword(self) -> bool {
        self >= Self::Identifier
    }

    pub fn is_reserved_word(self) -> bool {
        Self::FIRST_RESERVED_WORD <= self && self <= Self::LAST_RESERVED_WORD
    }

    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::AbstractKeyword
                | Self::AccessorKeyword
                | Self::AsyncKeyword
                | Self::ConstKeyword
                | Self::DeclareKeyword
                | Self::DefaultKeyword
                | Self::ExportKeyword
                | Self::InKeyword
                | Self::PrivateKeyword
                | Self::ProtectedKeyword
                | Self::PublicKeyword
                | Self::ReadonlyKeyword
                | Self::OutKeyword
                | Self::OverrideKeyword
                | Self::StaticKeyword
        )
    }

    pub fn is_class_member_modifier(self) -> bool {
        self.is_parameter_property_modifier()
            || matches!(
                self,
                Self::StaticKeyword | Self::OverrideKeyword | Self::AccessorKeyword
            )
    }

    pub fn is_parameter_property_modifier(self) -> bool {
        self.modifier_to_flag()
            .contains(ModifierFlags::ParameterPropertyModifier)
    }

    pub fn binary_operator_precedence(self) -> OperatorPrecedence {
        match self {
            Self::QuestionQuestionToken => OperatorPrecedence::COALESCE,
            Self::BarBarToken => OperatorPrecedence::LogicalOR,
            Self::AmpersandAmpersandToken => OperatorPrecedence::LogicalAND,
            Self::BarToken => OperatorPrecedence::BitwiseOR,
            Self::CaretToken => OperatorPrecedence::BitwiseXOR,
            Self::AmpersandToken => OperatorPrecedence::BitwiseAND,
            Self::EqualsEqualsToken
            | Self::ExclamationEqualsToken
            | Self::EqualsEqualsEqualsToken
            | Self::ExclamationEqualsEqualsToken => OperatorPrecedence::Equality,
            Self::LessThanToken
            | Self::GreaterThanToken
            | Self::LessThanEqualsToken
            | Self::GreaterThanEqualsToken
            | Self::InstanceOfKeyword
            | Self::InKeyword
            | Self::AsKeyword
            | Self::SatisfiesKeyword => OperatorPrecedence::Relational,
            Self::LessThanLessThanToken
            | Self::GreaterThanGreaterThanToken
            | Self::GreaterThanGreaterThanGreaterThanToken => OperatorPrecedence::Shift,
            Self::PlusToken | Self::MinusToken => OperatorPrecedence::Additive,
            Self::AsteriskToken | Self::SlashToken | Self::PercentToken => {
                OperatorPrecedence::Multiplicative
            }
            Self::AsteriskAsteriskToken => OperatorPrecedence::Exponentiation,
            // This is lower than all other precedences.  Returning it will cause binary expression
            // parsing to stop.
            _ => OperatorPrecedence::Invalid,
        }
    }

    pub fn modifier_to_flag(self) -> ModifierFlags {
        match self {
            Self::StaticKeyword => ModifierFlags::Static,
            Self::PublicKeyword => ModifierFlags::Public,
            Self::ProtectedKeyword => ModifierFlags::Protected,
            Self::PrivateKeyword => ModifierFlags::Private,
            Self::AbstractKeyword => ModifierFlags::Abstract,
            Self::AccessorKeyword => ModifierFlags::Accessor,
            Self::ExportKeyword => ModifierFlags::Export,
            Self::DeclareKeyword => ModifierFlags::Ambient,
            Self::ConstKeyword => ModifierFlags::Const,
            Self::DefaultKeyword => ModifierFlags::Default,
            Self::AsyncKeyword => ModifierFlags::Async,
            Self::ReadonlyKeyword => ModifierFlags::Readonly,
            Self::OverrideKeyword => ModifierFlags::Override,
            Self::InKeyword => ModifierFlags::In,
            Self::OutKeyword => ModifierFlags::Out,
            Self::Decorator => ModifierFlags::Decorator,
            _ => ModifierFlags::empty(),
        }
    }
}

pub fn text_to_keyword(text: &str) -> Option<SyntaxKind> {
    use SyntaxKind::*;
    Some(match text {
        "abstract" => AbstractKeyword,
        "accessor" => AccessorKeyword,
        "any" => AnyKeyword,
        "as" => AsKeyword,
        "asserts" => AssertsKeyword,
        "assert" => AssertKeyword,
        "bigint" => BigIntKeyword,
        "boolean" => BooleanKeyword,
        "break" => BreakKeyword,
        "case" => CaseKeyword,
        "catch" => CatchKeyword,
        "class" => ClassKeyword,
        "continue" => ContinueKeyword,
        "const" => ConstKeyword,
        "constructor" => ConstructorKeyword,
        "debugger" => DebuggerKeyword,
        "declare" => DeclareKeyword,
        "default" => DefaultKeyword,
        "defer" => DeferKeyword,
        "delete" => DeleteKeyword,
        "do" => DoKeyword,
        "else" => ElseKeyword,
        "enum" => EnumKeyword,
        "export" => ExportKeyword,
        "extends" => ExtendsKeyword,
        "false" => FalseKeyword,
        "finally" => FinallyKeyword,
        "for" => ForKeyword,
        "from" => FromKeyword,
        "function" => FunctionKeyword,
        "get" => GetKeyword,
        "if" => IfKeyword,
        "immediate" => ImmediateKeyword,
        "implements" => ImplementsKeyword,
        "import" => ImportKeyword,
        "in" => InKeyword,
        "infer" => InferKeyword,
        "instanceof" => InstanceOfKeyword,
        "interface" => InterfaceKeyword,
        "intrinsic" => IntrinsicKeyword,
        "is" => IsKeyword,
        "keyof" => KeyOfKeyword,
        "let" => LetKeyword,
        "module" => ModuleKeyword,
        "namespace" => NamespaceKeyword,
        "never" => NeverKeyword,
        "new" => NewKeyword,
        "null" => NullKeyword,
        "number" => NumberKeyword,
        "object" => ObjectKeyword,
        "package" => PackageKeyword,
        "private" => PrivateKeyword,
        "protected" => ProtectedKeyword,
        "public" => PublicKeyword,
        "override" => OverrideKeyword,
        "out" => OutKeyword,
        "readonly" => ReadonlyKeyword,
        "require" => RequireKeyword,
        "global" => GlobalKeyword,
        "return" => ReturnKeyword,
        "satisfies" => SatisfiesKeyword,
        "set" => SetKeyword,
        "static" => StaticKeyword,
        "string" => StringKeyword,
        "super" => SuperKeyword,
        "switch" => SwitchKeyword,
        "symbol" => SymbolKeyword,
        "this" => ThisKeyword,
        "throw" => ThrowKeyword,
        "true" => TrueKeyword,
        "try" => TryKeyword,
        "type" => TypeKeyword,
        "typeof" => TypeOfKeyword,
        "undefined" => UndefinedKeyword,
        "unique" => UniqueKeyword,
        "unknown" => UnknownKeyword,
        "using" => UsingKeyword,
        "var" => VarKeyword,
        "void" => VoidKeyword,
        "while" => WhileKeyword,
        "with" => WithKeyword,
        "yield" => YieldKeyword,
        "async" => AsyncKeyword,
        "await" => AwaitKeyword,
        "of" => OfKeyword,
        _ => return None,
    })
}

pub fn text_to_token(text: &str) -> Option<SyntaxKind> {
    use SyntaxKind::*;
    Some(match text {
        "{" => OpenBraceToken,
        "}" => CloseBraceToken,
        "(" => OpenParenToken,
        ")" => CloseParenToken,
        "[" => OpenBracketToken,
        "]" => CloseBracketToken,
        "." => DotToken,
        "..." => DotDotDotToken,
        ";" => SemicolonToken,
        "," => CommaToken,
        "<" => LessThanToken,
        ">" => GreaterThanToken,
        "<=" => LessThanEqualsToken,
        ">=" => GreaterThanEqualsToken,
        "==" => EqualsEqualsToken,
        "!=" => ExclamationEqualsToken,
        "===" => EqualsEqualsEqualsToken,
        "!==" => ExclamationEqualsEqualsToken,
        "=>" => EqualsGreaterThanToken,
        "+" => PlusToken,
        "-" => MinusToken,
        "**" => AsteriskAsteriskToken,
        "*" => AsteriskToken,
        "/" => SlashToken,
        "%" => PercentToken,
        "++" => PlusPlusToken,
        "--" => MinusMinusToken,
        "<<" => LessThanLessThanToken,
        "</" => LessThanSlashToken,
        ">>" => GreaterThanGreaterThanToken,
        ">>>" => GreaterThanGreaterThanGreaterThanToken,
        "&" => AmpersandToken,
        "|" => BarToken,
        "^" => CaretToken,
        "!" => ExclamationToken,
        "~" => TildeToken,
        "&&" => AmpersandAmpersandToken,
        "||" => BarBarToken,
        "?" => QuestionToken,
        "??" => QuestionQuestionToken,
        "?." => QuestionDotToken,
        ":" => ColonToken,
        "=" => EqualsToken,
        "+=" => PlusEqualsToken,
        "-=" => MinusEqualsToken,
        "*=" => AsteriskEqualsToken,
        "**=" => AsteriskAsteriskEqualsToken,
        "/=" => SlashEqualsToken,
        "%=" => PercentEqualsToken,
        "<<=" => LessThanLessThanEqualsToken,
        ">>=" => GreaterThanGreaterThanEqualsToken,
        ">>>=" => GreaterThanGreaterThanGreaterThanEqualsToken,
        "&=" => AmpersandEqualsToken,
        "|=" => BarEqualsToken,
        "^=" => CaretEqualsToken,
        "||=" => BarBarEqualsToken,
        "&&=" => AmpersandAmpersandEqualsToken,
        "??=" => QuestionQuestionEqualsToken,
        "@" => AtToken,
        "#" => HashToken,
        "`" => BacktickToken,
        _ => return text_to_keyword(text),
    })
}

pub fn token_to_text(token: SyntaxKind) -> &'static str {
    use SyntaxKind::*;
    match token {
        AbstractKeyword => "abstract",
        AccessorKeyword => "accessor",
        AnyKeyword => "any",
        AsKeyword => "as",
        AssertsKeyword => "asserts",
        AssertKeyword => "assert",
        BigIntKeyword => "bigint",
        BooleanKeyword => "boolean",
        BreakKeyword => "break",
        CaseKeyword => "case",
        CatchKeyword => "catch",
        ClassKeyword => "class",
        ContinueKeyword => "continue",
        ConstKeyword => "const",
        ConstructorKeyword => "constructor",
        DebuggerKeyword => "debugger",
        DeclareKeyword => "declare",
        DefaultKeyword => "default",
        DeferKeyword => "defer",
        DeleteKeyword => "delete",
        DoKeyword => "do",
        ElseKeyword => "else",
        EnumKeyword => "enum",
        ExportKeyword => "export",
        ExtendsKeyword => "extends",
        FalseKeyword => "false",
        FinallyKeyword => "finally",
        ForKeyword => "for",
        FromKeyword => "from",
        FunctionKeyword => "function",
        GetKeyword => "get",
        IfKeyword => "if",
        ImmediateKeyword => "immediate",
        ImplementsKeyword => "implements",
        ImportKeyword => "import",
        InKeyword => "in",
        InferKeyword => "infer",
        InstanceOfKeyword => "instanceof",
        InterfaceKeyword => "interface",
        IntrinsicKeyword => "intrinsic",
        IsKeyword => "is",
        KeyOfKeyword => "keyof",
        LetKeyword => "let",
        ModuleKeyword => "module",
        NamespaceKeyword => "namespace",
        NeverKeyword => "never",
        NewKeyword => "new",
        NullKeyword => "null",
        NumberKeyword => "number",
        ObjectKeyword => "object",
        PackageKeyword => "package",
        PrivateKeyword => "private",
        ProtectedKeyword => "protected",
        PublicKeyword => "public",
        OverrideKeyword => "override",
        OutKeyword => "out",
        ReadonlyKeyword => "readonly",
        RequireKeyword => "require",
        GlobalKeyword => "global",
        ReturnKeyword => "return",
        SatisfiesKeyword => "satisfies",
        SetKeyword => "set",
        StaticKeyword => "static",
        StringKeyword => "string",
        SuperKeyword => "super",
        SwitchKeyword => "switch",
        SymbolKeyword => "symbol",
        ThisKeyword => "this",
        ThrowKeyword => "throw",
        TrueKeyword => "true",
        TryKeyword => "try",
        TypeKeyword => "type",
        TypeOfKeyword => "typeof",
        UndefinedKeyword => "undefined",
        UniqueKeyword => "unique",
        UnknownKeyword => "unknown",
        UsingKeyword => "using",
        VarKeyword => "var",
        VoidKeyword => "void",
        WhileKeyword => "while",
        WithKeyword => "with",
        YieldKeyword => "yield",
        AsyncKeyword => "async",
        AwaitKeyword => "await",
        OfKeyword => "of",
        OpenBraceToken => "{",
        CloseBraceToken => "}",
        OpenParenToken => "(",
        CloseParenToken => ")",
        OpenBracketToken => "[",
        CloseBracketToken => "]",
        DotToken => ".",
        DotDotDotToken => "...",
        SemicolonToken => ";",
        CommaToken => ",",
        LessThanToken => "<",
        GreaterThanToken => ">",
        LessThanEqualsToken => "<=",
        GreaterThanEqualsToken => ">=",
        EqualsEqualsToken => "==",
        ExclamationEqualsToken => "!=",
        EqualsEqualsEqualsToken => "===",
        ExclamationEqualsEqualsToken => "!==",
        EqualsGreaterThanToken => "=>",
        PlusToken => "+",
        MinusToken => "-",
        AsteriskAsteriskToken => "**",
        AsteriskToken => "*",
        SlashToken => "/",
        PercentToken => "%",
        PlusPlusToken => "++",
        MinusMinusToken => "--",
        LessThanLessThanToken => "<<",
        LessThanSlashToken => "</",
        GreaterThanGreaterThanToken => ">>",
        GreaterThanGreaterThanGreaterThanToken => ">>>",
        AmpersandToken => "&",
        BarToken => "|",
        CaretToken => "^",
        ExclamationToken => "!",
        TildeToken => "~",
        AmpersandAmpersandToken => "&&",
        BarBarToken => "||",
        QuestionToken => "?",
        QuestionQuestionToken => "??",
        QuestionDotToken => "?.",
        ColonToken => ":",
        EqualsToken => "=",
        PlusEqualsToken => "+=",
        MinusEqualsToken => "-=",
        AsteriskEqualsToken => "*=",
        AsteriskAsteriskEqualsToken => "**=",
        SlashEqualsToken => "/=",
        PercentEqualsToken => "%=",
        LessThanLessThanEqualsToken => "<<=",
        GreaterThanGreaterThanEqualsToken => ">>=",
        GreaterThanGreaterThanGreaterThanEqualsToken => ">>>=",
        AmpersandEqualsToken => "&=",
        BarEqualsToken => "|=",
        CaretEqualsToken => "^=",
        BarBarEqualsToken => "||=",
        AmpersandAmpersandEqualsToken => "&&=",
        QuestionQuestionEqualsToken => "??=",
        AtToken => "@",
        HashToken => "#",
        BacktickToken => "`",
        _ => "",
    }
}

#[derive(Debug, Clone)]
pub struct CommentDirective {
    pub loc: TextRange,
    pub kind: CommentDirectiveKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentDirectiveKind {
    Unknown,
    ExpectError,
    Ignore,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub pos: TextPos,
    pub end: TextPos,
}

pub type TextPos = u32;

impl TextRange {
    pub fn new(pos: usize, end: usize) -> Self {
        Self {
            pos: pos as TextPos,
            end: end as TextPos,
        }
    }

    pub fn invalid() -> Self {
        Self { pos: 1, end: 0 }
    }

    pub fn is_invalid(&self) -> bool {
        self.pos > self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorPrecedence {
    // This is lower than all other precedences. Returning it will cause binary expression
    // parsing to stop.
    Invalid,

    // Expression:
    //     AssignmentExpression
    //     Expression `,` AssignmentExpression
    Comma,
    // NOTE: `Spread` is higher than `Comma` due to how it is parsed in |ElementList|
    // SpreadElement:
    //     `...` AssignmentExpression
    Spread,
    // AssignmentExpression:
    //     ConditionalExpression
    //     YieldExpression
    //     ArrowFunction
    //     AsyncArrowFunction
    //     LeftHandSideExpression `=` AssignmentExpression
    //     LeftHandSideExpression AssignmentOperator AssignmentExpression
    //
    // NOTE: AssignmentExpression is broken down into several precedences due to the requirements
    //       of the parenthesizer rules.
    // AssignmentExpression: YieldExpression
    // YieldExpression:
    //     `yield`
    //     `yield` AssignmentExpression
    //     `yield` `*` AssignmentExpression
    Yield,
    // AssignmentExpression: LeftHandSideExpression `=` AssignmentExpression
    // AssignmentExpression: LeftHandSideExpression AssignmentOperator AssignmentExpression
    // AssignmentOperator: one of
    //     `*=` `/=` `%=` `+=` `-=` `<<=` `>>=` `>>>=` `&=` `^=` `|=` `**=`
    Assignment,
    // NOTE: `Conditional` is considered higher than `Assignment` here, but in reality they have
    //       the same precedence.
    // AssignmentExpression: ConditionalExpression
    // ConditionalExpression:
    //     ShortCircuitExpression
    //     ShortCircuitExpression `?` AssignmentExpression `:` AssignmentExpression
    Conditional,
    // LogicalORExpression:
    //     LogicalANDExpression
    //     LogicalORExpression `||` LogicalANDExpression
    LogicalOR,
    // LogicalANDExpression:
    //     BitwiseORExpression
    //     LogicalANDExprerssion `&&` BitwiseORExpression
    LogicalAND,
    // BitwiseORExpression:
    //     BitwiseXORExpression
    //     BitwiseORExpression `|` BitwiseXORExpression
    BitwiseOR,
    // BitwiseXORExpression:
    //     BitwiseANDExpression
    //     BitwiseXORExpression `^` BitwiseANDExpression
    BitwiseXOR,
    // BitwiseANDExpression:
    //     EqualityExpression
    //     BitwiseANDExpression `&` EqualityExpression
    BitwiseAND,
    // EqualityExpression:
    //     RelationalExpression
    //     EqualityExpression `==` RelationalExpression
    //     EqualityExpression `!=` RelationalExpression
    //     EqualityExpression `===` RelationalExpression
    //     EqualityExpression `!==` RelationalExpression
    Equality,
    // RelationalExpression:
    //     ShiftExpression
    //     RelationalExpression `<` ShiftExpression
    //     RelationalExpression `>` ShiftExpression
    //     RelationalExpression `<=` ShiftExpression
    //     RelationalExpression `>=` ShiftExpression
    //     RelationalExpression `instanceof` ShiftExpression
    //     RelationalExpression `in` ShiftExpression
    //     [+TypeScript] RelationalExpression `as` Type
    Relational,
    // ShiftExpression:
    //     AdditiveExpression
    //     ShiftExpression `<<` AdditiveExpression
    //     ShiftExpression `>>` AdditiveExpression
    //     ShiftExpression `>>>` AdditiveExpression
    Shift,
    // AdditiveExpression:
    //     MultiplicativeExpression
    //     AdditiveExpression `+` MultiplicativeExpression
    //     AdditiveExpression `-` MultiplicativeExpression
    Additive,
    // MultiplicativeExpression:
    //     ExponentiationExpression
    //     MultiplicativeExpression MultiplicativeOperator ExponentiationExpression
    // MultiplicativeOperator: one of `*`, `/`, `%`
    Multiplicative,
    // ExponentiationExpression:
    //     UnaryExpression
    //     UpdateExpression `**` ExponentiationExpression
    Exponentiation,
    // UnaryExpression:
    //     UpdateExpression
    //     `delete` UnaryExpression
    //     `void` UnaryExpression
    //     `typeof` UnaryExpression
    //     `+` UnaryExpression
    //     `-` UnaryExpression
    //     `~` UnaryExpression
    //     `!` UnaryExpression
    //     AwaitExpression
    // UpdateExpression:            // TODO: Do we need to investigate the precedence here?
    //     `++` UnaryExpression
    //     `--` UnaryExpression
    Unary,
    // UpdateExpression:
    //     LeftHandSideExpression
    //     LeftHandSideExpression `++`
    //     LeftHandSideExpression `--`
    Update,
    // LeftHandSideExpression:
    //     NewExpression
    // NewExpression:
    //     MemberExpression
    //     `new` NewExpression
    LeftHandSide,
    // LeftHandSideExpression:
    //     OptionalExpression
    // OptionalExpression:
    //     MemberExpression OptionalChain
    //     CallExpression OptionalChain
    //     OptionalExpression OptionalChain
    OptionalChain,
    // LeftHandSideExpression:
    //     CallExpression
    // CallExpression:
    //     CoverCallExpressionAndAsyncArrowHead
    //     SuperCall
    //     ImportCall
    //     CallExpression Arguments
    //     CallExpression `[` Expression `]`
    //     CallExpression `.` IdentifierName
    //     CallExpression TemplateLiteral
    // MemberExpression:
    //     PrimaryExpression
    //     MemberExpression `[` Expression `]`
    //     MemberExpression `.` IdentifierName
    //     MemberExpression TemplateLiteral
    //     SuperProperty
    //     MetaProperty
    //     `new` MemberExpression Arguments
    Member,
    // TODO: JSXElement?
    // PrimaryExpression:
    //     `this`
    //     IdentifierReference
    //     Literal
    //     ArrayLiteral
    //     ObjectLiteral
    //     FunctionExpression
    //     ClassExpression
    //     GeneratorExpression
    //     AsyncFunctionExpression
    //     AsyncGeneratorExpression
    //     RegularExpressionLiteral
    //     TemplateLiteral
    Primary,
    // PrimaryExpression:
    //     CoverParenthesizedExpressionAndArrowParameterList
    Parentheses,
}

impl OperatorPrecedence {
    pub const LOWEST: Self = Self::Comma;
    pub const HIGHEST: Self = Self::Parentheses;
    pub const DISALLOW_COMMA: Self = Self::Yield;
    // ShortCircuitExpression:
    //     LogicalORExpression
    //     CoalesceExpression
    // CoalesceExpression:
    //     CoalesceExpressionHead `??` BitwiseORExpression
    // CoalesceExpressionHead:
    //     CoalesceExpression
    //     BitwiseORExpression
    pub const COALESCE: Self = Self::LogicalOR;
}
