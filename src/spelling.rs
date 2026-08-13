pub fn get_spelling_suggestion_for_strings(
    name: &str,
    candidates: impl Iterator<Item = &'static str>,
) -> String {
    todo!()
}

pub fn get_space_suggestion(name: &str, candidates: impl Iterator<Item = &'static str>) -> String {
    for keyword in candidates {
        if name.len() > keyword.len() + 2 && name.starts_with(keyword) {
            let rest = &name[keyword.len()..];
            return format!("{keyword} {rest}");
        }
    }
    String::new()
}
