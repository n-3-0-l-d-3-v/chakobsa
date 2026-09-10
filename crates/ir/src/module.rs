use crate::function::Function;

/// A whole compiled program: every function it defines. Module-level
/// validation (`validate::validate_module`) is what checks a `Call`
/// instruction's callee actually exists with a matching signature — a
/// single `Function` can't check that against itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    pub functions: Vec<Function>,
}

impl Module {
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}
