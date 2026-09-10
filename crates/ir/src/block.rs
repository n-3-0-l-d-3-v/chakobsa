use crate::inst::Instruction;
use crate::value::{BlockId, ValueId};

/// How control leaves a block. Exactly one of these ends every valid
/// block — mirroring mentat's "a block is a sequence of instructions
/// followed by exactly one terminator" rule (`Program::validate`),
/// applied one layer up the pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jump(BlockId),
    /// `cond` must be `Type::Bool` — checked at validation time.
    Branch {
        cond: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// `Some` iff the enclosing function has a non-`None` return type;
    /// checked at validation time against the function's signature.
    Return(Option<ValueId>),
}

impl Terminator {
    /// Every block this terminator can transfer control to.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(b) => vec![*b],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Terminator::Return(_) => vec![],
        }
    }
}

/// A basic block: an ordered list of instructions plus exactly one
/// terminator. Predecessors are deliberately **not** stored here — they
/// can only be known once every block that might jump to this one has
/// been added, which isn't true yet while ticket 003's parser is still
/// incrementally building a function. `Function::predecessors` computes
/// them on demand instead, so there's no cached field that could drift
/// out of sync with the CFG's actual shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

impl BasicBlock {
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            instructions: Vec::new(),
            terminator: None,
        }
    }
}
