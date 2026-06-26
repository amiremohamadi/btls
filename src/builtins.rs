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

macro_rules! keyword {
    ($name:expr) => {
        BuiltinSymbol {
            name: $name,
            detail: "",
            documentation: "",
        }
    };
}

pub const SYNTAX_KEYWORDS: &[BuiltinSymbol] = &[
    keyword!("break"),
    keyword!("config"),
    keyword!("continue"),
    keyword!("else"),
    keyword!("for"),
    keyword!("if"),
    keyword!("import"),
    keyword!("let"),
    keyword!("macro"),
    keyword!("offsetof"),
    keyword!("sizeof"),
    keyword!("unroll"),
    keyword!("while"),
];

pub const DATA_TYPES: &[BuiltinSymbol] = &[
    keyword!("bool"),
    keyword!("uint8"),
    keyword!("int8"),
    keyword!("uint16"),
    keyword!("int16"),
    keyword!("uint32"),
    keyword!("int32"),
    keyword!("uint64"),
    keyword!("int64"),
];

pub const BUILTINS: BuiltinSymbols = include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/builtins.gen.rs"
));
