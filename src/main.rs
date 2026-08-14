use crate::{
    binder::Binder,
    options::ScriptKind,
    parser::{Parser, SourceFileParseOptions},
};

mod ast;
mod binder;
mod diagnostics;
mod flags;
mod flow;
mod number;
mod options;
mod parser;
mod printer;
mod regexp_parser;
mod scanner;
mod spelling;
mod symbol;
mod syntax;

fn main() {
    let parser = Parser::new(SourceFileParseOptions { file_name: String::from("test.ts") });
    let (source_file, nodes) =
        parser.parse_source_file(String::from("interface Foo { a: string }"), ScriptKind::TS);
    let mut binder = Binder::new(source_file, nodes);
    let x = binder.bind(source_file);
    println!("{x}");
}
