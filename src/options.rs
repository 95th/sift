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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LanguageVariant {
    #[default]
    Standard,
    JSX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    Unknown,
    JS,
    JSX,
    TS,
    TSX,
    External,
    JSON,
    /**
     * Used on extensions that doesn't define the ScriptKind but the content defines it.
     * Deferred extensions are going to be included in all project contexts.
     */
    Deferred,
}

impl ScriptKind {
    pub fn language_variant(self) -> LanguageVariant {
        match self {
            Self::TSX | Self::JSX | Self::JS | Self::JSON => LanguageVariant::JSX,
            _ => LanguageVariant::Standard,
        }
    }
}
