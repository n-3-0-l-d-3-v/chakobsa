//! CHAKOBSA's typed SSA intermediate representation — the thing the
//! parser (ticket 003) builds directly, with no AST stage in between.
//! See `docs/design/LANGUAGE.md` for the full pipeline and why.

pub mod block;
pub mod function;
pub mod inst;
pub mod interp;
pub mod module;
#[cfg(test)]
pub mod test_support;
pub mod types;
pub mod validate;
pub mod value;

pub use block::{BasicBlock, Terminator};
pub use function::Function;
pub use inst::{BinOp, InstKind, Instruction, UnOp};
pub use interp::{run, InterpError, RtValue};
pub use module::Module;
pub use types::Type;
pub use validate::{validate_function, validate_module, IrError};
pub use value::{BlockId, ValueId};
