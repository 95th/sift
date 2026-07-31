use crate::scanner::LanguageVariant;

bitflags::bitflags! {
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

        const BlockScoped = Self::Let.bits() | Self::Const.bits() | Self::Using.bits();
        const Constant    = Self::Const.bits() | Self::Using.bits();
        const AwaitUsing  = Self::Const.bits() | Self::Using.bits(); // Variable declaration (NOTE: on a single node these flags would otherwise be mutually exclusive)

        const ReachabilityCheckFlags   = Self::HasImplicitReturn.bits() | Self::HasExplicitReturn.bits();
        const ReachabilityAndEmitFlags = Self::ReachabilityCheckFlags.bits() | Self::HasAsyncFunctions.bits();

        // Parsing context flags
        const ContextFlags  = Self::DisallowInContext.bits() | Self::DisallowConditionalTypesContext.bits() | Self::YieldContext.bits() | Self::DecoratorContext.bits() | Self::AwaitContext.bits() | Self::JavaScriptFile.bits() | Self::InWithStatement.bits() | Self::Ambient.bits();

        // Exclude these flags when parsing a Type
        const TypeExcludesFlags  = Self::YieldContext.bits() | Self::AwaitContext.bits();

        // Represents all flags that are potentially set once and
        // never cleared on SourceFiles which get re-used in between incremental parses.
        // See the comment above on `PossiblyContainsDynamicImport` and `PossiblyContainsImportMeta`.
        const PermanentlySetIncrementalFlags  = Self::PossiblyContainsDynamicImport.bits() | Self::PossiblyContainsImportMeta.bits();

        // The following flags repurpose other  as different meanings for Identifier nodes
        const IdentifierHasExtendedUnicodeEscape  = Self::ContainsThis.bits()  ;    // Indicates whether the identifier contains an extended unicode escape sequence
        const IdentifierIsInJSDocNamespace        = Self::HasAsyncFunctions.bits(); // Indicates the identifier is the innermost name of a JSDoc namespace declaration

        // The following flag repurposes other  for ModuleDeclaration nodes
        const NestedNamespace  = Self::OptionalChain.bits(); // If ModuleDeclaration is a nested namespace (e.g. inner part of A.B.C)
    }
}

bitflags::bitflags! {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    Unknown,
    JS,
    JSX,
    TS,
    TSX,
    External,
    JSON,
    /**
     * Used on extensions that doesn't define the ScriptKind but the content defines it.
     * Deferred extensions are going to be included in all project contexts.
     */
    Deferred,
}

impl ScriptKind {
    pub fn language_variant(self) -> LanguageVariant {
        match self {
            Self::TSX | Self::JSX | Self::JS | Self::JSON => LanguageVariant::JSX,
            _ => LanguageVariant::Standard,
        }
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
