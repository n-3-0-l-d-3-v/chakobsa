use std::collections::HashMap;

use crate::block::BasicBlock;
use crate::types::Type;
use crate::value::BlockId;

/// One typed SSA function: parameters, an optional return type (`None`
/// means the surface-language function has no `->` clause), a CFG of
/// blocks, and the entry block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret_ty: Option<Type>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

impl Function {
    pub fn new(name: impl Into<String>, params: Vec<(String, Type)>, ret_ty: Option<Type>) -> Self {
        Self {
            name: name.into(),
            params,
            ret_ty,
            blocks: Vec::new(),
            entry: BlockId(0),
        }
    }

    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Every block's predecessors, derived from every other block's
    /// terminator successors — computed fresh each call rather than
    /// cached (see `BasicBlock`'s doc comment for why). A block with no
    /// entry in the returned map has zero predecessors (this includes
    /// the entry block itself, unless the CFG has a back-edge into it).
    pub fn predecessors(&self) -> HashMap<BlockId, Vec<BlockId>> {
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for block in &self.blocks {
            let Some(term) = &block.terminator else {
                continue;
            };
            for succ in term.successors() {
                preds.entry(succ).or_default().push(block.id);
            }
        }
        preds
    }
}
