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

pub const BUILTINS: BuiltinSymbols = include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/builtins.gen.rs"
));
