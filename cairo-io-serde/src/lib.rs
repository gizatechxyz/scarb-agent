use cairo_vm::Felt252;

pub mod cairo_input;
pub mod cairo_output;
pub mod schema;
pub(crate) mod utils;

#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
pub enum FuncArg {
    Array(Vec<Felt252>),
    Single(Felt252),
}

#[derive(Debug, Clone, Default)]
pub struct FuncArgs(pub Vec<FuncArg>);
