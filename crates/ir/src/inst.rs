use crate::types::Type;
use crate::value::{BlockId, ValueId};

/// A binary operation. Grouped by the type discipline it enforces —
/// arithmetic and comparison take `I64` operands, `And`/`Or` take `Bool`
/// operands — rather than one opcode per Rust variant, mirroring the way
/// mentat's `Opcode::reads`/`writes` centralize per-opcode shape instead
/// of scattering it across match arms everywhere it's needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    CmpEq,
    CmpNe,
    CmpLt,
    CmpLe,
    CmpGt,
    CmpGe,
    And,
    Or,
}

impl BinOp {
    /// The type both operands must have.
    pub fn operand_type(self) -> Type {
        use BinOp::*;
        match self {
            Add | Sub | Mul | Div | Mod | CmpEq | CmpNe | CmpLt | CmpLe | CmpGt | CmpGe => {
                Type::I64
            }
            And | Or => Type::Bool,
        }
    }

    /// The type of the value this instruction produces.
    pub fn result_type(self) -> Type {
        use BinOp::*;
        match self {
            Add | Sub | Mul | Div | Mod => Type::I64,
            CmpEq | CmpNe | CmpLt | CmpLe | CmpGt | CmpGe | And | Or => Type::Bool,
        }
    }

    pub fn mnemonic(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "add",
            Sub => "sub",
            Mul => "mul",
            Div => "div",
            Mod => "mod",
            CmpEq => "cmpeq",
            CmpNe => "cmpne",
            CmpLt => "cmplt",
            CmpLe => "cmple",
            CmpGt => "cmpgt",
            CmpGe => "cmpge",
            And => "and",
            Or => "or",
        }
    }
}

/// A unary operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

impl UnOp {
    pub fn operand_type(self) -> Type {
        match self {
            UnOp::Neg => Type::I64,
            UnOp::Not => Type::Bool,
        }
    }

    pub fn result_type(self) -> Type {
        match self {
            UnOp::Neg => Type::I64,
            UnOp::Not => Type::Bool,
        }
    }

    pub fn mnemonic(self) -> &'static str {
        match self {
            UnOp::Neg => "neg",
            UnOp::Not => "not",
        }
    }
}

/// What one SSA value is defined as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstKind {
    ConstI64(i64),
    ConstBool(bool),
    Bin(BinOp, ValueId, ValueId),
    Un(UnOp, ValueId),
    /// A call to another function in the same `Module`. Argument count
    /// and types are checked against the callee's declared signature at
    /// module-validation time (a single function can't self-validate a
    /// call without seeing the rest of the module).
    Call {
        func: String,
        args: Vec<ValueId>,
    },
    /// One incoming value per predecessor of the block this instruction
    /// lives in. `Function::validate` checks the incoming count matches
    /// the block's actual predecessor count, and every incoming value's
    /// type matches this instruction's declared type.
    Phi(Vec<(BlockId, ValueId)>),
}

/// One instruction: defines exactly one new, typed SSA value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub id: ValueId,
    pub ty: Type,
    pub kind: InstKind,
}
