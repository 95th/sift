use rustc_hash::FxHashMap;

use crate::{flags::RegexpFlags, scanner::Scanner};

pub struct RegexpParser<'a> {
    pub scanner: &'a mut Scanner,
    pub end: usize,
    pub regexp_flags: RegexpFlags,
    pub any_unicode_mode: bool,
    pub unicode_sets_mode: bool,
    pub annex_b: bool,
    pub named_capture_groups: bool,
    pub group_specifiers: FxHashMap<String, bool>,
}

impl RegexpParser<'_> {
    pub fn run(&mut self) {
        todo!()
    }
}
