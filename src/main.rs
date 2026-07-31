use crate::{ast::ScriptKind, parser::Parser};

mod ast;
mod diagnostics;
mod number;
mod options;
mod parser;
mod scanner;
mod syntax;

fn main() {
    let mut parser = Parser::new();
    parser.parse(String::from("  + - % ^&  (  )"), ScriptKind::JS);
}
