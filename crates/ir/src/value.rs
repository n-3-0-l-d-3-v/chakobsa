use std::fmt;

/// An opaque, typed SSA value id. Never reassigned once defined — the
/// defining property of SSA form. Unique within a single `Function`, not
/// across a whole `Module`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

impl fmt::Display for ValueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// A basic block id — an index into `Function::blocks`, mirroring
/// mentat's own "block index is the only notion of address" design
/// (`docs/design/LANGUAGE.md` codegen target).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b{}", self.0)
    }
}
