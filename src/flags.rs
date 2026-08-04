bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeFlags: u32 {
        const None                             = 0;
        const Let                              = 1 << 0 ; // Variable declaration
        const Const                            = 1 << 1 ; // Variable declaration
        const Using                            = 1 << 2 ; // Variable declaration
        const Reparsed                         = 1 << 3 ; // Node was synthesized during parsing
        const Synthesized                      = 1 << 4 ; // Node was synthesized during transformation
        const OptionalChain                    = 1 << 5 ; // Chained MemberExpression rooted to a pseudo-OptionalExpression
        const ExportContext                    = 1 << 6 ; // Export context (initialized by binding)
        const ContainsThis                     = 1 << 7 ; // Interface contains references to "this"
        const HasImplicitReturn                = 1 << 8 ; // If function implicitly returns on one of codepaths (initialized by binding)
        const HasExplicitReturn                = 1 << 9 ; // If function has explicit reachable return on one of codepaths (initialized by binding)
        const DisallowInContext                = 1 << 10; // If node was parsed in a context where 'in-expressions' are not allowed
        const YieldContext                     = 1 << 11; // If node was parsed in the 'yield' context created when parsing a generator
        const DecoratorContext                 = 1 << 12; // If node was parsed as part of a decorator
        const AwaitContext                     = 1 << 13; // If node was parsed in the 'await' context created when parsing an async function
        const DisallowConditionalTypesContext  = 1 << 14; // If node was parsed in a context where conditional types are not allowed
        const ThisNodeHasError                 = 1 << 15; // If the parser encountered an error when parsing the code that created this node
        const JavaScriptFile                   = 1 << 16; // If node was parsed in a JavaScript
        const ThisNodeOrAnySubNodesHasError    = 1 << 17; // If this node or any of its children had an error
        const HasAsyncFunctions                = 1 << 18; // If the file has async functions (initialized by binding)
        // HasAggregatedChildData is deprecated. Use `subtreeFacts` instead.

        // These flags will be set when the parser encounters a dynamic import expression or 'import.meta' to avoid
        // walking the tree if the flags are not set. However, these flags are just a approximation
        // (hence why it's named "PossiblyContainsDynamicImport") because once set, the flags never get cleared.
        // During editing, if a dynamic import is removed, incremental parsing will *NOT* clear this flag.
        // This means that the tree will always be traversed during module resolution, or when looking for external module indicators.
        // However, the removal operation should not occur often and in the case of the
        // removal, it is likely that users will add the import anyway.
        // The advantage of this approach is its simplicity. For the case of batch compilation,
        // we guarantee that users won't have to pay the price of walking the tree if a dynamic import isn't used.
        const PossiblyContainsDynamicImport  = 1 << 19;
        const PossiblyContainsImportMeta     = 1 << 20;

        const HasJSDoc                       = 1 << 21; // If node has preceding JSDoc comment(s)
        const JSDoc                          = 1 << 22; // If node was parsed inside jsdoc
        const Ambient                        = 1 << 23; // If node was inside an ambient context -- a declaration file, or inside something with the `declare` modifier.
        const InWithStatement                = 1 << 24; // If any ancestor of node was the `statement` of a WithStatement (not the `expression`)
        const JsonFile                       = 1 << 25; // If node was parsed in a Json
        const PossiblyContainsDeprecatedTag  = 1 << 26; // Set during parse if comment text contains '@deprecated'; must confirm via JSDoc lookup
        const Unreachable                    = 1 << 27; // If node is unreachable according to the binder
        const ReparserTransformedLiteral     = 1 << 28; // If node was transformed during parsing, making its' naive text source not match the AST

        const BlockScoped                    = Self::Let.bits() | Self::Const.bits() | Self::Using.bits();
        const Constant                       = Self::Const.bits() | Self::Using.bits();
        const AwaitUsing                     = Self::Const.bits() | Self::Using.bits(); // Variable declaration (NOTE: on a single node these flags would otherwise be mutually exclusive)

        const ReachabilityCheckFlags         = Self::HasImplicitReturn.bits() | Self::HasExplicitReturn.bits();
        const ReachabilityAndEmitFlags       = Self::ReachabilityCheckFlags.bits() | Self::HasAsyncFunctions.bits();

        // Parsing context flags
        const ContextFlags                   = Self::DisallowInContext.bits() | Self::DisallowConditionalTypesContext.bits() | Self::YieldContext.bits() | Self::DecoratorContext.bits() | Self::AwaitContext.bits() | Self::JavaScriptFile.bits() | Self::InWithStatement.bits() | Self::Ambient.bits();

        // Exclude these flags when parsing a Type
        const TypeExcludesFlags              = Self::YieldContext.bits() | Self::AwaitContext.bits();

        // Represents all flags that are potentially set once and
        // never cleared on SourceFiles which get re-used in between incremental parses.
        // See the comment above on `PossiblyContainsDynamicImport` and `PossiblyContainsImportMeta`.
        const PermanentlySetIncrementalFlags = Self::PossiblyContainsDynamicImport.bits() | Self::PossiblyContainsImportMeta.bits();

        // The following flags repurpose other  as different meanings for Identifier nodes
        const IdentifierHasExtendedUnicodeEscape = Self::ContainsThis.bits()  ;    // Indicates whether the identifier contains an extended unicode escape sequence
        const IdentifierIsInJSDocNamespace       = Self::HasAsyncFunctions.bits(); // Indicates the identifier is the innermost name of a JSDoc namespace declaration

        // The following flag repurposes other  for ModuleDeclaration nodes
        const NestedNamespace                = Self::OptionalChain.bits(); // If ModuleDeclaration is a nested namespace (e.g. inner part of A.B.C)
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ParsingContext: u32 {
        const SourceElements           = 1 << 0;   // Elements in source file
        const BlockStatements          = 1 << 1;   // Statements in block
        const SwitchClauses            = 1 << 2;   // Clauses in switch statement
        const SwitchClauseStatements   = 1 << 3;   // Statements in switch clause
        const TypeMembers              = 1 << 4;   // Members in interface or type literal
        const ClassMembers             = 1 << 5;   // Members in class declaration
        const EnumMembers              = 1 << 6;   // Members in enum declaration
        const HeritageClauseElement    = 1 << 7;   // Elements in a heritage clause
        const VariableDeclarations     = 1 << 8;   // Variable declarations in variable statement
        const ObjectBindingElements    = 1 << 9;   // Binding elements in object binding list
        const ArrayBindingElements     = 1 << 10;  // Binding elements in array binding list
        const ArgumentExpressions      = 1 << 11;  // Expressions in argument list
        const ObjectLiteralMembers     = 1 << 12;  // Members in object literal
        const JsxAttributes            = 1 << 13;  // Attributes in jsx element
        const JsxChildren              = 1 << 14;  // Things between opening and closing JSX tags
        const ArrayLiteralMembers      = 1 << 15;  // Members in array literal
        const Parameters               = 1 << 16;  // Parameters in parameter list
        const JSDocParameters          = 1 << 17;  // JSDoc parameters in parameter list of JSDoc function type
        const RestProperties           = 1 << 18;  // Property names in a rest type list
        const TypeParameters           = 1 << 19;  // Type parameters in type parameter list
        const TypeArguments            = 1 << 20;  // Type arguments in type argument list
        const TupleElementTypes        = 1 << 21;  // Element types in tuple element type list
        const HeritageClauses          = 1 << 22;  // Heritage clauses for a class or interface declaration.
        const ImportOrExportSpecifiers = 1 << 23;  // Named import clause's import specifier list
        const ImportAttributes         = 1 << 24;  // Import attributes
        const JSDocComment             = 1 << 25;  // Parsing via JSDocParser
        const Count                    = 1 << 26;  // Number of parsing contexts
    }
}

bitflags::bitflags! {
    pub struct ParseFlags: u8 {
        const None                   = 0;
        const Yield                  = 1 << 0;
        const Await                  = 1 << 1;
        const Type                   = 1 << 2;
        const IgnoreMissingOpenBrace = 1 << 4;
        const JSDoc                  = 1 << 5;
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

bitflags::bitflags! {
    pub struct JsdDocScannerInfo: u8 {
        const HasJSDoc      = 1 << 0;
        const HasDeprecated = 1 << 1;
        const HasSeeOrLink  = 1 << 2;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModifierFlags: u32 {
        // Syntactic/JSDoc modifiers
        const Public                         = 1 << 0; // Property/Method
        const Private                        = 1 << 1; // Property/Method
        const Protected                      = 1 << 2; // Property/Method
        const Readonly                       = 1 << 3; // Property/Method
        const Override                       = 1 << 4; // Override method
        // Syntactic-only modifiers
        const Export                         = 1 << 5;  // Declarations
        const Abstract                       = 1 << 6;  // Class/Method/ConstructSignature
        const Ambient                        = 1 << 7;  // Declarations (declare keyword)
        const Static                         = 1 << 8;  // Property/Method
        const Accessor                       = 1 << 9;  // Property
        const Async                          = 1 << 10; // Property/Method/Function
        const Default                        = 1 << 11; // Function/Class (export default declaration)
        const Const                          = 1 << 12; // Const enum
        const In                             = 1 << 13; // Contravariance modifier
        const Out                            = 1 << 14; // Covariance modifier
        const Decorator                      = 1 << 15; // Contains a decorator
        // JSDoc-only modifiers
        const Deprecated                     = 1 << 16; // Deprecated tag
        // Cache-only JSDoc-modifiers. Should match order of Syntactic/JSDoc modifiers, above.
        const JSDocPublic                    = 1 << 23; // if this value changes, `selectEffectiveModifierFlags` must change accordingly
        const JSDocPrivate                   = 1 << 24;
        const JSDocProtected                 = 1 << 25;
        const JSDocReadonly                  = 1 << 26;
        const JSDocOverride                  = 1 << 27;
        const HasComputedJSDocModifiers      = 1 << 28; // Indicates the computed modifier flags include modifiers from JSDoc.
        const HasComputedFlags               = 1 << 29; // Modifier flags have been computed

        const SyntacticOrJSDocModifiers      = Self::Public.bits() | Self::Private.bits() | Self::Protected.bits() | Self::Readonly.bits() | Self::Override.bits();
        const SyntacticOnlyModifiers         = Self::Export.bits() | Self::Ambient.bits() | Self::Abstract.bits() | Self::Static.bits() | Self::Accessor.bits() | Self::Async.bits() | Self::Default.bits() | Self::Const.bits() | Self::In.bits() | Self::Out.bits() | Self::Decorator.bits();
        const SyntacticModifiers             = Self::SyntacticOrJSDocModifiers.bits() | Self::SyntacticOnlyModifiers.bits();
        const JSDocCacheOnlyModifiers        = Self::JSDocPublic.bits() | Self::JSDocPrivate.bits() | Self::JSDocProtected.bits() | Self::JSDocReadonly.bits() | Self::JSDocOverride.bits();
        const JSDocOnlyModifiers             = Self::Deprecated.bits();
        const NonCacheOnlyModifiers          = Self::SyntacticOrJSDocModifiers.bits() | Self::SyntacticOnlyModifiers.bits() | Self::JSDocOnlyModifiers.bits();

        const AccessibilityModifier          = Self::Public.bits() | Self::Private.bits() | Self::Protected.bits();
        // Accessibility modifiers and 'readonly' can be attached to a parameter in a constructor to make it a property.
        const ParameterPropertyModifier      = Self::AccessibilityModifier.bits() | Self::Readonly.bits() | Self::Override.bits();
        const NonPublicAccessibilityModifier = Self::Private.bits() | Self::Protected.bits();

        const TypeScriptModifier             = Self::Ambient.bits() | Self::Public.bits() | Self::Private.bits() | Self::Protected.bits() | Self::Readonly.bits() | Self::Abstract.bits() | Self::Const.bits() | Self::Override.bits() | Self::In.bits() | Self::Out.bits();
        const ExportDefault                  = Self::Export.bits() | Self::Default.bits();
        const All                            = Self::Export.bits() | Self::Ambient.bits() | Self::Public.bits() | Self::Private.bits() | Self::Protected.bits() | Self::Static.bits() | Self::Readonly.bits() | Self::Abstract.bits() | Self::Accessor.bits() | Self::Async.bits() | Self::Default.bits() | Self::Const.bits() | Self::Deprecated.bits() | Self::Override.bits() | Self::In.bits() | Self::Out.bits() | Self::Decorator.bits();
        const Modifier                       = Self::All.bits() & !Self::Decorator.bits();
        const JavaScript                     = Self::Export.bits() | Self::Static.bits() | Self::Accessor.bits() | Self::Async.bits() | Self::Default.bits();
    }
}
