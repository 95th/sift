use std::{collections::HashMap, env, fs, io::Write, path::Path};

use serde::Deserialize;

fn main() {
    println!("cargo::rerun-if-changed=diagnostics.json");

    let file = fs::File::open("diagnostics.json").expect("Expect 'diagnostics.json' to exist");
    let contents: HashMap<String, Diagnostic> =
        serde_json::from_reader(file).expect("Expected JSON file");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_diagnostics.rs");
    let mut out = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dest_path)
        .expect("Unable to create output file");
    for (message, diag) in contents {
        let name = format!("E{}", diag.code);
        write!(out, "pub static {name}: &'static Message = &Message {{ ").unwrap();
        write!(out, "code: {}, ", diag.code).unwrap();
        write!(
            out,
            "category: {}, ",
            match diag.category {
                DiagnosticCategory::Error => "MessageCategory::Error",
                DiagnosticCategory::Message => "MessageCategory::Message",
                DiagnosticCategory::Suggestion => "MessageCategory::Suggestion",
            }
        )
        .unwrap();
        write!(out, "text: {:?}, ", message).unwrap();
        write!(out, "reports_unnecessary: {}, ", diag.reports_unnecessary).unwrap();
        write!(
            out,
            "elided_in_compatability_pyramid: {}, ",
            diag.elided_in_compatability_pyramid
        )
        .unwrap();
        write!(out, "reports_deprecated: {}, ", diag.reports_deprecated).unwrap();
        writeln!(out, "}};").unwrap();
    }
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
