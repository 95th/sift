#[derive(Debug)]
pub struct Message {
    code: u32,
    category: MessageCategory,
    text: &'static str,
    reports_unnecessary: bool,
    elided_in_compatability_pyramid: bool,
    reports_deprecated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCategory {
    Error,
    Message,
    Suggestion,
}

#[allow(unused)]
mod generated {
    use super::{Message, MessageCategory};

    include!(concat!(env!("OUT_DIR"), "/generated_diagnostics.rs"));
}

pub use generated::*;
