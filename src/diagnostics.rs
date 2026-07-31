use std::sync::{Arc, Mutex};

use crate::syntax::TextRange;

#[derive(Debug)]
pub struct Message {
    pub code: u32,
    pub category: MessageCategory,
    pub text: &'static str,
    pub reports_unnecessary: bool,
    pub elided_in_compatability_pyramid: bool,
    pub reports_deprecated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCategory {
    Error,
    Message,
    Suggestion,
}

#[derive(Debug)]
pub struct Diagnostic {
    pub message: &'static Message,
    pub loc: TextRange,
    pub args: Vec<String>,
}

#[allow(unused)]
mod generated {
    use super::{Message, MessageCategory};

    include!(concat!(env!("OUT_DIR"), "/generated_diagnostics.rs"));
}

#[derive(Debug, Clone)]
pub struct Diagnostics {
    list: Arc<Mutex<Vec<Diagnostic>>>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self {
            list: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn scan_error<I>(&self, message: &'static Message, pos: usize, len: usize, args: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.report(
            message,
            TextRange::new(pos, pos + len),
            args.into_iter().collect(),
        );
    }

    pub fn report(&self, message: &'static Message, loc: TextRange, args: Vec<String>) {
        self.list
            .lock()
            .unwrap()
            .push(Diagnostic { message, loc, args });
    }

    pub fn has_errors(&self) -> bool {
        !self.list.lock().unwrap().is_empty()
    }
}
