use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hash::FxHashMap;

use crate::{
    ast::NodeId,
    flags::{CheckFlags, SymbolFlags},
};

static NEXT_SYMBOL_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(u64);

impl SymbolId {
    pub fn new() -> Self {
        Self(NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub struct Symbol {
    pub flags: SymbolFlags,
    pub check_flags: CheckFlags,
    pub name: String,
    pub declaration: Vec<NodeId>,
    pub value_declaration: NodeId,
    pub members: SymbolTable,
    pub exports: SymbolTable,
    pub id: SymbolId,
    pub parent: Option<SymbolId>,
    pub export_symbol: Option<SymbolId>,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: FxHashMap<String, Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { symbols: FxHashMap::default() }
    }

    pub fn insert(&mut self, name: String) -> &mut Symbol {
        self.symbols.entry(name).or_default()
    }
}

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
