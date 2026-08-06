use crate::{options::ScriptKind, parser::Parser};

mod ast;
mod diagnostics;
mod flags;
mod number;
mod options;
mod parser;
mod scanner;
mod syntax;

fn main() {
    let mut parser = Parser::new();
    parser.parse_source_file(String::from("{;}{;;;};;"), ScriptKind::JS);
}
