use std::fmt;

/// The v1 surface language's only two types — see `docs/design/LANGUAGE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    I64,
    Bool,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::Bool => write!(f, "bool"),
        }
    }
}
