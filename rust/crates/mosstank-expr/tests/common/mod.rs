//! What the expression tests share: the C# tests' `Evaluate` helper.

#![allow(dead_code)]

use mosstank_expr::{
    CoreOptions, EvaluationContext, ExpressionError, ExpressionProgram, ExpressionState,
    ExpressionValue, create_default,
};

/// The C# tests' `Evaluate(source, state)`: the core functions with a
/// random source seeded 1234, the default budget.
pub fn evaluate_in(
    source: &str,
    state: &mut ExpressionState,
) -> Result<ExpressionValue, ExpressionError> {
    let functions = create_default(&CoreOptions::seeded(1234));
    let mut context = EvaluationContext::new(state, &functions);
    ExpressionProgram::compile(source)?.evaluate(&mut context)
}

/// `Evaluate(source)` on a fresh state.
pub fn evaluate(source: &str) -> Result<ExpressionValue, ExpressionError> {
    evaluate_in(source, &mut ExpressionState::new())
}

/// The number an expression answers; panics on an error.
pub fn number(source: &str) -> f64 {
    evaluate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .as_number("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
}

/// The string an expression answers; panics on an error.
pub fn string(source: &str) -> String {
    evaluate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .as_str("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
        .to_owned()
}

/// What an expression prints as; panics on an error.
pub fn display(source: &str) -> String {
    evaluate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .to_display_string()
}

/// The value in a state; panics on an error.
pub fn value(source: &str, state: &mut ExpressionState) -> ExpressionValue {
    evaluate_in(source, state).unwrap_or_else(|error| panic!("{source}: {error}"))
}

/// xUnit's `Assert.Equal(expected, actual, precision)`: both rounded to
/// that many decimals (halves to even) are equal.
pub fn assert_close(expected: f64, actual: f64, precision: i32) {
    let scale = 10f64.powi(precision);
    let round = |value: f64| (value * scale).round_ties_even() / scale;
    assert_eq!(
        round(expected),
        round(actual),
        "expected {expected}, got {actual} (to {precision} decimals)"
    );
}
