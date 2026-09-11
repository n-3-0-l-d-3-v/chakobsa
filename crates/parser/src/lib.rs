//! CHAKOBSA's parser: consumes `lexer`'s token stream and builds
//! `ir`'s typed SSA form directly via `ssa_builder`, with no AST stage
//! in between. See `docs/design/LANGUAGE.md`.

pub mod parser;
pub mod ssa_builder;

pub use parser::{parse, ParseError};
