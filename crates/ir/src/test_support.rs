//! Tiny hand-construction helpers for building IR fixtures in tests —
//! deliberately not the ticket 003 SSA-construction builder (which
//! needs to support incremental, not-yet-sealed blocks); this crate's
//! own tests just need to assemble already-finished, valid or
//! deliberately-invalid functions directly.
#![cfg(test)]

use crate::block::{BasicBlock, Terminator};
use crate::function::Function;
use crate::inst::{InstKind, Instruction};
use crate::types::Type;
use crate::value::{BlockId, ValueId};

pub struct FnBuilder {
    func: Function,
    next_value: u32,
}

impl FnBuilder {
    pub fn new(name: &str, params: Vec<(&str, Type)>, ret_ty: Option<Type>) -> Self {
        let next_value = params.len() as u32;
        Self {
            func: Function::new(
                name,
                params
                    .into_iter()
                    .map(|(n, t)| (n.to_string(), t))
                    .collect(),
                ret_ty,
            ),
            next_value,
        }
    }

    pub fn param_value(&self, index: u32) -> ValueId {
        ValueId(index)
    }

    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.func.blocks.len() as u32);
        self.func.blocks.push(BasicBlock::new(id));
        id
    }

    pub fn push(&mut self, block: BlockId, ty: Type, kind: InstKind) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.func
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .unwrap()
            .instructions
            .push(Instruction { id, ty, kind });
        id
    }

    /// Pushes an instruction with an explicit `ValueId` — for tests that
    /// need to construct a deliberately-broken IR (e.g. a duplicate
    /// value id) that `push`'s auto-increment would never produce.
    pub fn push_with_id(&mut self, block: BlockId, id: ValueId, ty: Type, kind: InstKind) {
        self.func
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .unwrap()
            .instructions
            .push(Instruction { id, ty, kind });
    }

    pub fn terminate(&mut self, block: BlockId, term: Terminator) {
        self.func
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .unwrap()
            .terminator = Some(term);
    }

    pub fn set_entry(&mut self, block: BlockId) {
        self.func.entry = block;
    }

    pub fn finish(self) -> Function {
        self.func
    }
}
