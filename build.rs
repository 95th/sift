use std::{collections::HashMap, env, fs, io::Write, path::Path};

use serde::Deserialize;

fn main() {
    println!("cargo::rerun-if-changed=diagnostics.json");
    println!("cargo::rerun-if-changed=astgen/ts.ungram");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let ast_dest_path = Path::new(&out_dir).join("generated_ast.rs");
    fs::write(&ast_dest_path, astgen::generate()).expect("Unable to write generated AST");

    let file = fs::File::open("diagnostics.json").expect("Expect 'diagnostics.json' to exist");
    let contents: HashMap<String, Diagnostic> =
        serde_json::from_reader(file).expect("Expected JSON file");

    let dest_path = Path::new(&out_dir).join("generated_diagnostics.rs");
    let mut out = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dest_path)
        .expect("Unable to create output file");
    writeln!(out, "impl Message {{").unwrap();
    for (message, diag) in contents {
        write!(
            out,
            "    pub const fn {}() -> &'static Message {{ &Message {{ ",
            as_name(diag.code, &message)
        )
        .unwrap();
        write!(out, "code: {}, ", diag.code).unwrap();
        write!(out, "category: {}, ", category_str(diag.category)).unwrap();
        write!(out, "text: {:?}, ", message).unwrap();
        write!(out, "reports_unnecessary: {}, ", diag.reports_unnecessary).unwrap();
        write!(
            out,
            "elided_in_compatability_pyramid: {}, ",
            diag.elided_in_compatability_pyramid
        )
        .unwrap();
        write!(out, "reports_deprecated: {}, ", diag.reports_deprecated).unwrap();
        writeln!(out, "}} }}").unwrap();
    }
    writeln!(out, "}}").unwrap();
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostic {
    category: DiagnosticCategory,
    code: u32,
    #[serde(default)]
    reports_unnecessary: bool,
    #[serde(default)]
    reports_deprecated: bool,
    #[serde(default)]
    elided_in_compatability_pyramid: bool,
}

#[derive(Deserialize)]
enum DiagnosticCategory {
    Error,
    Message,
    Suggestion,
}

fn category_str(c: DiagnosticCategory) -> &'static str {
    match c {
        DiagnosticCategory::Error => "MessageCategory::Error",
        DiagnosticCategory::Message => "MessageCategory::Message",
        DiagnosticCategory::Suggestion => "MessageCategory::Suggestion",
    }
}

fn as_name(code: u32, s: &str) -> String {
    let mut out = format!("e{code}_");
    for c in s.chars() {
        match c {
            '*' => out.push_str("_asterisk"),
            '/' => out.push_str("_slash"),
            ':' => out.push_str("_colon"),
            '_' | '0'..='9' | 'a'..='z' | 'A'..='Z' => out.extend(c.to_lowercase()),
            _ => out.push('_'),
        }
    }

    let mut out2 = String::new();
    let mut add_underscore = false;
    for c in out.chars() {
        match c {
            '_' => add_underscore = true,
            c => {
                if add_underscore {
                    out2.push('_');
                    add_underscore = false;
                }
                out2.push(c);
            }
        }
    }

    out2
}
