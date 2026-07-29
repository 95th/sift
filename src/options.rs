#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTarget {
    None = 0,
    // Deprecated: Do not use outside of options parsing and validation.
    ES5 = 1,
    ES2015 = 2,
    ES2016 = 3,
    ES2017 = 4,
    ES2018 = 5,
    ES2019 = 6,
    ES2020 = 7,
    ES2021 = 8,
    ES2022 = 9,
    ES2023 = 10,
    ES2024 = 11,
    ES2025 = 12,
    ESNext = 99,
    JSON = 100,
}

impl ScriptTarget {
    pub const LATEST: Self = Self::ESNext;
    pub const LATEST_STANDARD: Self = Self::ES2025;
}
