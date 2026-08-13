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
