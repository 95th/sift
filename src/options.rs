#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScriptTarget {
    #[default]
    None,
    // Deprecated: Do not use outside of options parsing and validation.
    ES5,
    ES2015,
    ES2016,
    ES2017,
    ES2018,
    ES2019,
    ES2020,
    ES2021,
    ES2022,
    ES2023,
    ES2024,
    ES2025,
    ESNext = 99,
    JSON = 100,
}

impl ScriptTarget {
    pub const LATEST: Self = Self::ESNext;
    pub const LATEST_STANDARD: Self = Self::ES2025;
}
