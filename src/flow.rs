use std::ops::{Index, IndexMut};

use crate::{ast::NodeId, flags::FlowFlags};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlowNodeId(usize);

pub struct FlowNode {
    pub flags: FlowFlags,
    pub node: NodeId,                   // Associated AST node
    pub antecedent: Option<FlowNodeId>, // Antecedent for all but FlowLabel
    pub antecedents: Option<FlowList>,  // Linked list of antecedents for FlowLabel
}

pub struct FlowList {
    pub flow: FlowNodeId,
    pub next: Option<FlowNodeId>,
}

pub struct FlowNodeFactory {
    store: Vec<FlowNode>,
}

pub type FlowLabel = FlowNodeId;

impl FlowNodeFactory {
    pub fn new() -> Self {
        Self { store: Vec::new() }
    }

    pub fn create(&mut self, node: NodeId, flags: FlowFlags) -> FlowNodeId {
        let id = FlowNodeId(self.store.len());
        self.store.push(FlowNode { flags, node, antecedent: None, antecedents: None });
        id
    }
}

impl Index<FlowNodeId> for FlowNodeFactory {
    type Output = FlowNode;

    fn index(&self, index: FlowNodeId) -> &Self::Output {
        &self.store[index.0]
    }
}

impl IndexMut<FlowNodeId> for FlowNodeFactory {
    fn index_mut(&mut self, index: FlowNodeId) -> &mut Self::Output {
        &mut self.store[index.0]
    }
}
