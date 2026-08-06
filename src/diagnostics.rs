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
    pub related_information: Vec<Diagnostic>,
}

impl Diagnostic {
    pub fn new(
        message: &'static Message,
        loc: TextRange,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            message,
            loc,
            args: args.into_iter().collect(),
            related_information: Vec::new(),
        }
    }
}

#[allow(unused)]
mod generated {
    use super::{Message, MessageCategory};

    include!(concat!(env!("OUT_DIR"), "/generated_diagnostics.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticId(usize);

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

    pub fn report(
        &self,
        message: &'static Message,
        loc: TextRange,
        args: impl IntoIterator<Item = String>,
    ) -> DiagnosticId {
        let list = &mut *self.list.lock().unwrap();
        let id = DiagnosticId(list.len());
        list.push(Diagnostic::new(message, loc, args));
        id
    }

    pub fn truncate(&self, len: usize) {
        self.list.lock().unwrap().truncate(len);
    }

    pub fn len(&self) -> usize {
        self.list.lock().unwrap().len()
    }

    pub fn with<T>(&self, f: impl FnOnce(&[Diagnostic]) -> T) -> T {
        let list = self.list.lock().unwrap();
        f(&list)
    }

    pub fn add_related_info(
        &self,
        id: DiagnosticId,
        message: &'static Message,
        loc: TextRange,
        args: impl IntoIterator<Item = String>,
    ) {
        let list = &mut *self.list.lock().unwrap();
        list[id.0]
            .related_information
            .push(Diagnostic::new(message, loc, args));
    }
}
