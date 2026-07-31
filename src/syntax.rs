#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
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
}

impl SyntaxKind {
    const FIRST_KEYWORD: Self = Self::BreakKeyword;
    const LAST_KEYWORD: Self = Self::DeferKeyword;

    pub fn is_keyword(self) -> bool {
        Self::FIRST_KEYWORD <= self && self <= Self::LAST_KEYWORD
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EscapeSequenceScanningFlags: u8 {
        const String                     = 1 << 0;
        const ReportErrors               = 1 << 1;
        const RegularExpression          = 1 << 2;
        const AnnexB                     = 1 << 3;
        const AnyUnicodeMode             = 1 << 4;
        const AtomEscape                 = 1 << 5;
        const ReportInvalidEscapeErrors  = Self::RegularExpression.bits() | Self::ReportErrors.bits();
        const AllowExtendedUnicodeEscape = Self::String.bits() | Self::AnyUnicodeMode.bits();
    }
}

bitflags::bitflags! {

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct TokenFlags: u32 {
        const None                           = 0;
        const PrecedingLineBreak             = 1 << 0;
        const PrecedingJSDocComment          = 1 << 1;
        const Unterminated                   = 1 << 2;
        const ExtendedUnicodeEscape          = 1 << 3 ; // e.g. `\u{10ffff}`
        const Scientific                     = 1 << 4 ; // e.g. `10e2`
        const Octal                          = 1 << 5 ; // e.g. `0777`
        const HexSpecifier                   = 1 << 6 ; // e.g. `0x00000000`
        const BinarySpecifier                = 1 << 7 ; // e.g. `0b0110010000000000`
        const OctalSpecifier                 = 1 << 8 ; // e.g. `0o777`
        const ContainsSeparator              = 1 << 9 ; // e.g. `0b1100_0101`
        const UnicodeEscape                  = 1 << 10; // e.g. `\u00a0`
        const ContainsInvalidEscape          = 1 << 11; // e.g. `\uhello`
        const HexEscape                      = 1 << 12; // e.g. `\xa0`
        const ContainsLeadingZero            = 1 << 13; // e.g. `0888`
        const ContainsInvalidSeparator       = 1 << 14; // e.g. `0_1`
        const PrecedingJSDocLeadingAsterisks = 1 << 15;
        const SingleQuote                    = 1 << 16; // e.g. `'abc'`
        const PrecedingJSDocWithDeprecated   = 1 << 17; // Preceding JSDoc comment contains @deprecated
        const PrecedingJSDocWithSeeOrLink    = 1 << 18; // Preceding JSDoc comment contains @see or @link
        const BinaryOrOctalSpecifier         = Self::BinarySpecifier.bits() | Self::OctalSpecifier.bits();
        const WithSpecifier                  = Self::HexSpecifier.bits() | Self::BinaryOrOctalSpecifier.bits();
        const StringLiteralFlags             = Self::Unterminated.bits() | Self::HexEscape.bits() | Self::UnicodeEscape.bits() | Self::ExtendedUnicodeEscape.bits() | Self::ContainsInvalidEscape.bits() | Self::SingleQuote.bits();
        const NumericLiteralFlags            = Self::Scientific.bits() | Self::Octal.bits() | Self::ContainsLeadingZero.bits() | Self::WithSpecifier.bits() | Self::ContainsSeparator.bits() | Self::ContainsInvalidSeparator.bits();
        const TemplateLiteralLikeFlags       = Self::Unterminated.bits() | Self::HexEscape.bits() | Self::UnicodeEscape.bits() | Self::ExtendedUnicodeEscape.bits() | Self::ContainsInvalidEscape.bits();
        const RegularExpressionLiteralFlags  = Self::Unterminated.bits();
        const IsInvalid                      = Self::Octal.bits() | Self::ContainsLeadingZero.bits() | Self::ContainsInvalidSeparator.bits() | Self::ContainsInvalidEscape.bits();
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

#[derive(Debug)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}
