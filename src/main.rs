use crate::{scanner::Scanner, syntax::SyntaxKind};

mod ast;
mod diagnostics;
mod number;
mod options;
mod parser;
mod scanner;
mod syntax;

fn main() {
    let mut scanner = Scanner::new();
    scanner.set_text(String::from("  + - % ^&  (  )"));
    loop {
        let token = scanner.scan();
        if token == SyntaxKind::EndOfFile {
            break;
        }

        println!(
            "token = {token:?}, value = {}, pos = {}",
            scanner.token_value(),
            scanner.pos()
        )
    }
}
