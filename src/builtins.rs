pub struct BuiltinSymbols {
    pub keywords: &'static [BuiltinSymbol],
    pub functions: &'static [BuiltinSymbol],
    pub config_vars: &'static [ConfigVar],
}

pub struct BuiltinSymbol {
    pub name: &'static str,
    pub detail: &'static str,
    pub documentation: &'static str,
}

pub struct ConfigVar {
    pub name: &'static str,
    pub detail: &'static str,
    pub documentation: &'static str,
    pub values: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Bool,
    Uint8,
    Int8,
    Uint16,
    Int16,
    Uint32,
    Int32,
    Uint64,
    Int64,
}

impl DataType {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Uint8 => "uint8",
            Self::Int8 => "int8",
            Self::Uint16 => "uint16",
            Self::Int16 => "int16",
            Self::Uint32 => "uint32",
            Self::Int32 => "int32",
            Self::Uint64 => "uint64",
            Self::Int64 => "int64",
        }
    }

    #[allow(dead_code)]
    pub fn from_name(name: &str) -> Option<DataType> {
        match name {
            "bool" => Some(DataType::Bool),
            "uint8" => Some(DataType::Uint8),
            "int8" => Some(DataType::Int8),
            "uint16" => Some(DataType::Uint16),
            "int16" => Some(DataType::Int16),
            "uint32" => Some(DataType::Uint32),
            "int32" => Some(DataType::Int32),
            "uint64" => Some(DataType::Uint64),
            "int64" => Some(DataType::Int64),
            _ => None,
        }
    }
}

macro_rules! keyword {
    ($dt:expr) => {
        BuiltinSymbol {
            name: $dt.name(),
            detail: "",
            documentation: "",
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxKeyword {
    Break,
    Config,
    Continue,
    Else,
    For,
    If,
    Import,
    Let,
    Macro,
    Offsetof,
    Sizeof,
    Unroll,
    While,
}

impl SyntaxKeyword {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Break => "break",
            Self::Config => "config",
            Self::Continue => "continue",
            Self::Else => "else",
            Self::For => "for",
            Self::If => "if",
            Self::Import => "import",
            Self::Let => "let",
            Self::Macro => "macro",
            Self::Offsetof => "offsetof",
            Self::Sizeof => "sizeof",
            Self::Unroll => "unroll",
            Self::While => "while",
        }
    }
}

pub const SYNTAX_KEYWORDS: &[BuiltinSymbol] = &[
    keyword!(SyntaxKeyword::Break),
    keyword!(SyntaxKeyword::Config),
    keyword!(SyntaxKeyword::Continue),
    keyword!(SyntaxKeyword::Else),
    keyword!(SyntaxKeyword::For),
    keyword!(SyntaxKeyword::If),
    keyword!(SyntaxKeyword::Import),
    keyword!(SyntaxKeyword::Let),
    keyword!(SyntaxKeyword::Macro),
    keyword!(SyntaxKeyword::Offsetof),
    keyword!(SyntaxKeyword::Sizeof),
    keyword!(SyntaxKeyword::Unroll),
    keyword!(SyntaxKeyword::While),
];

pub const DATA_TYPES: &[BuiltinSymbol] = &[
    keyword!(DataType::Bool),
    keyword!(DataType::Uint8),
    keyword!(DataType::Int8),
    keyword!(DataType::Uint16),
    keyword!(DataType::Int16),
    keyword!(DataType::Uint32),
    keyword!(DataType::Int32),
    keyword!(DataType::Uint64),
    keyword!(DataType::Int64),
];

pub const BUILTINS: BuiltinSymbols = include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/builtins.gen.rs"
));
