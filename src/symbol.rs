use rustc_hash::FxHashMap;

use crate::{
    ast::NodeId,
    flags::{CheckFlags, SymbolFlags},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(usize);

pub struct Symbol {
    pub flags: SymbolFlags,
    pub check_flags: CheckFlags,
    pub name: String,
    pub declaration: Vec<NodeId>,
    pub value_declaration: NodeId,
    pub members: SymbolTable,
    pub exports: SymbolTable,
    pub id: u64,
    pub parent: Option<SymbolId>,
    pub export_symbol: Option<SymbolId>,
}

pub type SymbolTable = FxHashMap<String, SymbolId>;

pub struct InternalSymbolName;

#[rustfmt::skip]
impl InternalSymbolName {
	// Prefix \xFE: Invalid UTF8 sequence, will never occur as IdentifierName
	pub const CALL                     : &[u8] = b"\xFEcall";                    // Call signatures
	pub const CONSTRUCTOR              : &[u8] = b"\xFEconstructor";             // Constructor implementations
	pub const NEW                      : &[u8] = b"\xFEnew";                     // Constructor signatures
	pub const INDEX                    : &[u8] = b"\xFEindex";                   // Index signatures
	pub const EXPORT_STAR              : &[u8] = b"\xFEexport";                  // Module export * declarations
	pub const GLOBAL                   : &[u8] = b"\xFEglobal";                  // Global self-reference
	pub const MISSING                  : &[u8] = b"\xFEmissing";                 // Indicates missing symbol
	pub const TYPE                     : &[u8] = b"\xFEtype";                    // Anonymous type literal symbol
	pub const OBJECT                   : &[u8] = b"\xFEobject";                  // Anonymous object literal declaration
	pub const JSXATTRIBUTES            : &[u8] = b"\xFEjsxAttributes";           // Anonymous JSX attributes object literal declaration
	pub const CLASS                    : &[u8] = b"\xFEclass";                   // Unnamed class expression
	pub const FUNCTION                 : &[u8] = b"\xFEfunction";                // Unnamed function expression
	pub const COMPUTED                 : &[u8] = b"\xFEcomputed";                // Computed property name declaration with dynamic name
	pub const ASSIGNMENT_DECLARATION   : &[u8] = b"\xFEassignment";              // Assignment declarations
	pub const INSTANTIATION_EXPRESSION : &[u8] = b"\xFEinstantiationExpression"; // Instantiation expressions
	pub const IMPORT_ATTRIBUTES        : &[u8] = b"\xFEimportAttributes";

	pub const EXPORT_EQUALS            : &[u8] = b"export=";                 // Export assignment symbol
	pub const DEFAULT                  : &[u8] = b"default";                 // Default export symbol (technically not wholly internal, but included here for usability)
	pub const THIS                     : &[u8] = b"this";
	pub const MODULE_EXPORTS           : &[u8] = b"module.exports";
}
