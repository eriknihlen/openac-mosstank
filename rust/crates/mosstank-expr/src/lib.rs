//! The meta expression language: values, the parser and evaluator, the
//! variable state, the function registry and the core functions every
//! expression has. Pure: the host's functions are registered by the caller.

pub mod core;
pub mod dictionary;
pub mod engine;
pub mod error;
pub mod json;
pub mod reference_arguments;
mod reference_declared;
pub mod reference_sort;
pub mod runtime;
pub mod value;

pub use crate::core::{CoreOptions, ExpressionClock, FixedClock, create_default};
pub use engine::ExpressionProgram;
pub use error::{EvaluationError, ExpressionError, Fault};
pub use runtime::{
    DEFAULT_INSTRUCTION_BUDGET, EvaluationContext, ExpressionState, ExpressionVariableScope,
    FunctionRegistry, GlobalVariableStore,
};
pub use value::{ChangeCounter, ExpressionCoordinates, ExpressionValue, ExpressionValueKind};
