//! Incremental SSA construction without dominance frontiers, per Braun,
//! Buchwald, Hack, Leißa, Mallon & Zwinkau (CC 2013), "Simple and
//! Efficient Construction of Static Single Assignment Form." This is
//! the piece that makes `docs/design/LANGUAGE.md`'s central claim real:
//! the parser (`crate::parser`) drives this builder directly while
//! walking tokens, so no AST is ever built in between.
//!
//! Core idea: a variable read in a block resolves against whatever
//! definition currently reaches that block. If the block isn't
//! "sealed" yet (not every predecessor is known — true for a loop
//! header until its body has been parsed and the back edge added), the
//! read gets a placeholder `Phi` with no operands yet, recorded as
//! *incomplete*; `seal_block` later fills in every incomplete phi's
//! operands once the block's predecessor set is finally complete.
//!
//! **Known simplification**: this does not implement the paper's
//! trivial-phi removal optimization (collapsing a phi whose operands
//! are all identical, or all-but-self, back to that one value). Skipping
//! it produces slightly less minimal IR — an extra `Phi` where a plain
//! value would do — but never incorrect IR: every phi this builder
//! creates still has exactly one incoming entry per actual predecessor,
//! which is all `ir::validate` requires. Tracked here as a real,
//! deliberately deferred optimization, not silently forgotten.

use std::collections::{HashMap, HashSet};

use ir::{BasicBlock, BlockId, Function, InstKind, Instruction, Terminator, Type, ValueId};

pub struct SsaBuilder {
    func: Function,
    next_value: u32,
    /// Per block, the current value bound to each variable name.
    current_def: HashMap<BlockId, HashMap<String, ValueId>>,
    value_types: HashMap<ValueId, Type>,
    sealed: HashSet<BlockId>,
    /// Per not-yet-sealed block, the phis created for a variable read
    /// before the block's full predecessor set was known — resolved by
    /// `seal_block`.
    incomplete_phis: HashMap<BlockId, Vec<(String, ValueId)>>,
    preds: HashMap<BlockId, Vec<BlockId>>,
}

impl SsaBuilder {
    /// Starts building `name`'s IR. The entry block always exists,
    /// always has zero predecessors for the lifetime of the function
    /// (nothing in this language can jump backward into a function's
    /// entry), and is therefore sealed immediately. Parameters occupy
    /// `ValueId(0)..ValueId(params.len())`, matching the convention
    /// `ir::interp` and codegen (ticket 004) both rely on.
    pub fn new(name: impl Into<String>, params: Vec<(String, Type)>, ret_ty: Option<Type>) -> Self {
        let next_value = params.len() as u32;
        let mut func = Function::new(name, params.clone(), ret_ty);
        let entry = BlockId(0);
        func.blocks.push(BasicBlock::new(entry));
        func.entry = entry;

        let mut value_types = HashMap::new();
        let mut entry_defs = HashMap::new();
        for (i, (pname, ty)) in params.into_iter().enumerate() {
            let id = ValueId(i as u32);
            entry_defs.insert(pname, id);
            value_types.insert(id, ty);
        }

        Self {
            func,
            next_value,
            current_def: HashMap::from([(entry, entry_defs)]),
            value_types,
            sealed: HashSet::from([entry]),
            incomplete_phis: HashMap::new(),
            preds: HashMap::from([(entry, Vec::new())]),
        }
    }

    pub fn entry(&self) -> BlockId {
        self.func.entry
    }

    /// True once `block` has a terminator — used by the parser to reject
    /// statements textually following one that already ended the block
    /// (unreachable code), and to decide whether an if/while branch's
    /// tail needs an explicit `Jump` added to reach a join/loop-header
    /// block.
    pub fn is_terminated(&self, block: BlockId) -> bool {
        self.func
            .block(block)
            .map(|b| b.terminator.is_some())
            .unwrap_or(false)
    }

    /// Emits a `Phi` with already-known incoming values — for merges the
    /// parser builds directly (short-circuit `and`/`or`) rather than
    /// through `read_variable`'s named-variable resolution, since both
    /// incoming values are already in hand and the merge block's
    /// predecessors are already final by construction.
    pub fn emit_phi(
        &mut self,
        block: BlockId,
        ty: Type,
        incoming: Vec<(BlockId, ValueId)>,
    ) -> ValueId {
        self.emit(block, ty, InstKind::Phi(incoming))
    }

    /// Allocates a fresh, empty, not-yet-sealed block with no known
    /// predecessors yet.
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.func.blocks.len() as u32);
        self.func.blocks.push(BasicBlock::new(id));
        self.current_def.insert(id, HashMap::new());
        self.preds.insert(id, Vec::new());
        id
    }

    fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        self.func
            .blocks
            .iter_mut()
            .find(|b| b.id == id)
            .expect("block id always resolves within its own builder")
    }

    /// Appends a new instruction to `block` and returns its value id.
    pub fn emit(&mut self, block: BlockId, ty: Type, kind: InstKind) -> ValueId {
        let id = self.fresh_value();
        self.value_types.insert(id, ty);
        self.block_mut(block)
            .instructions
            .push(Instruction { id, ty, kind });
        id
    }

    /// Sets `block`'s terminator and records the CFG edge(s) it creates
    /// — the single place edges enter `self.preds`, so it can never be
    /// done inconsistently by a caller forgetting to also update preds.
    pub fn terminate(&mut self, block: BlockId, term: Terminator) {
        for succ in term.successors() {
            self.preds.entry(succ).or_default().push(block);
        }
        self.block_mut(block).terminator = Some(term);
    }

    /// Binds `var` to `value` in `block` — the SSA "current definition"
    /// at this point in the block being built.
    pub fn write_variable(&mut self, var: &str, block: BlockId, value: ValueId) {
        self.current_def
            .entry(block)
            .or_default()
            .insert(var.to_string(), value);
    }

    /// Resolves `var`'s current value as seen from `block`, per Braun et
    /// al.: a local definition if one exists, otherwise recursively from
    /// predecessors, inserting a phi wherever the value could come from
    /// more than one place (or isn't fully known yet because `block`
    /// isn't sealed).
    pub fn read_variable(&mut self, var: &str, block: BlockId, ty: Type) -> ValueId {
        if let Some(&v) = self.current_def.get(&block).and_then(|m| m.get(var)) {
            return v;
        }
        self.read_variable_recursive(var, block, ty)
    }

    fn read_variable_recursive(&mut self, var: &str, block: BlockId, ty: Type) -> ValueId {
        let value = if !self.sealed.contains(&block) {
            // Predecessors aren't all known yet — park an incomplete
            // phi to be resolved when `block` is sealed.
            let phi = self.emit(block, ty, InstKind::Phi(Vec::new()));
            self.incomplete_phis
                .entry(block)
                .or_default()
                .push((var.to_string(), phi));
            phi
        } else {
            let preds = self.preds.get(&block).cloned().unwrap_or_default();
            match preds.as_slice() {
                [] => {
                    // No predecessor ever reaches this block (either the
                    // entry block, or genuinely dead code). The parser
                    // only ever calls read_variable for names already
                    // confirmed live by its own scope tracking, so this
                    // path is not expected to be hit for a real program;
                    // an empty phi is the closest honest answer if it
                    // ever is.
                    self.emit(block, ty, InstKind::Phi(Vec::new()))
                }
                [only] => self.read_variable(var, *only, ty),
                _ => {
                    // Multiple predecessors: create the phi and bind it
                    // *before* recursing into predecessors, so a cycle
                    // (a loop reading its own loop-carried variable)
                    // terminates instead of recursing forever.
                    let phi = self.emit(block, ty, InstKind::Phi(Vec::new()));
                    self.write_variable(var, block, phi);
                    self.add_phi_operands(var, phi, block, ty);
                    phi
                }
            }
        };
        self.write_variable(var, block, value);
        value
    }

    fn add_phi_operands(&mut self, var: &str, phi: ValueId, block: BlockId, ty: Type) {
        let preds = self.preds.get(&block).cloned().unwrap_or_default();
        let mut incoming = Vec::with_capacity(preds.len());
        for pred in preds {
            let value = self.read_variable(var, pred, ty);
            incoming.push((pred, value));
        }
        let inst = self
            .block_mut(block)
            .instructions
            .iter_mut()
            .find(|i| i.id == phi)
            .expect("phi id was just created in this block");
        inst.kind = InstKind::Phi(incoming);
    }

    /// Marks `block`'s predecessor set as final and resolves every phi
    /// that was parked there as incomplete while it wasn't. Must be
    /// called exactly once every predecessor edge into `block` has been
    /// added via `terminate`.
    pub fn seal_block(&mut self, block: BlockId) {
        let pending = self.incomplete_phis.remove(&block).unwrap_or_default();
        for (var, phi) in pending {
            let ty = self.value_types[&phi];
            self.add_phi_operands(&var, phi, block, ty);
        }
        self.sealed.insert(block);
    }

    pub fn finish(self) -> Function {
        self.func
    }
}
