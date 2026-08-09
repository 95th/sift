use crate::{
    options::ScriptKind,
    parser::{Parser, SourceFileParseOptions},
};

mod ast;
mod diagnostics;
mod flags;
mod number;
mod options;
mod parser;
mod printer;
mod regexp_parser;
mod scanner;
mod syntax;

fn main() {
    let parser = Parser::new(SourceFileParseOptions { file_name: String::from("test.js") });
    let (source_file, nodes) =
        parser.parse_source_file(String::from("class Foo {}"), ScriptKind::JS);
    println!("{}", nodes.print(source_file));
}
