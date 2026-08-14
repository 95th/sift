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

pub type FlowLabel = FlowNodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActiveLabelId(usize);

pub struct ActiveLabel {
    next: Option<ActiveLabelId>,
    break_target: Option<FlowLabel>,
    continue_target: Option<FlowLabel>,
    name: String,
    referenced: bool,
}

pub struct FlowFactory {
    flows: Vec<FlowNode>,
    active_labels: Vec<ActiveLabel>,
}

impl FlowFactory {
    pub fn new() -> Self {
        Self { flows: Vec::new(), active_labels: Vec::new() }
    }

    pub fn create_flow(&mut self, node: NodeId, flags: FlowFlags) -> FlowNodeId {
        let id = FlowNodeId(self.flows.len());
        self.flows.push(FlowNode { flags, node, antecedent: None, antecedents: None });
        id
    }

    pub fn create_active_label(&mut self, label: ActiveLabel) -> ActiveLabelId {
        let id = ActiveLabelId(self.active_labels.len());
        self.active_labels.push(label);
        id
    }
}

impl Index<FlowNodeId> for FlowFactory {
    type Output = FlowNode;

    fn index(&self, index: FlowNodeId) -> &Self::Output {
        &self.flows[index.0]
    }
}

impl IndexMut<FlowNodeId> for FlowFactory {
    fn index_mut(&mut self, index: FlowNodeId) -> &mut Self::Output {
        &mut self.flows[index.0]
    }
}

impl Index<ActiveLabelId> for FlowFactory {
    type Output = ActiveLabel;

    fn index(&self, index: ActiveLabelId) -> &Self::Output {
        &self.active_labels[index.0]
    }
}

impl IndexMut<ActiveLabelId> for FlowFactory {
    fn index_mut(&mut self, index: ActiveLabelId) -> &mut Self::Output {
        &mut self.active_labels[index.0]
    }
}
