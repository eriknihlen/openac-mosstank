//! What can go wrong reading or running an expression.

use std::fmt;

/// An expression that does not read, or a run that failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionError {
    /// The text is not an expression (the C# `ExpressionParseException`).
    Parse {
        /// What is wrong.
        message: String,
        /// Where (UTF-16 offset into the text).
        offset: usize,
    },
    /// The run failed (the C# `ExpressionEvaluationException`).
    Evaluation(EvaluationError),
    /// The run was cancelled.
    Cancelled,
}

/// A failed run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvaluationError {
    /// The error as the reference words it, without the offset.
    pub reason: String,
    /// Where it happened, when known.
    pub offset: Option<usize>,
    /// Raised while a function ran (not while its arguments were checked).
    pub raised_in_function: bool,
    /// An argument of the wrong kind.
    pub is_argument_error: bool,
    /// The runtime's own error this one carries (an overflow, a pattern that
    /// does not parse), as its message.
    pub inner: Option<String>,
}

impl EvaluationError {
    /// An error with this reason.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            ..Self::default()
        }
    }

    /// The same error at an offset.
    #[must_use]
    pub fn at(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// The message: the reason, then " at offset N." when there is one.
    #[must_use]
    pub fn message(&self) -> String {
        match self.offset {
            Some(offset) => format!("{} at offset {offset}.", self.reason),
            None => self.reason.clone(),
        }
    }
}

impl ExpressionError {
    /// An evaluation error with this reason.
    #[must_use]
    pub fn evaluation(reason: impl Into<String>) -> Self {
        Self::Evaluation(EvaluationError::new(reason))
    }

    /// An evaluation error with this reason at an offset.
    #[must_use]
    pub fn evaluation_at(reason: impl Into<String>, offset: usize) -> Self {
        Self::Evaluation(EvaluationError::new(reason).at(offset))
    }

    /// A parse error.
    #[must_use]
    pub fn parse(message: impl Into<String>, offset: usize) -> Self {
        Self::Parse {
            message: message.into(),
            offset,
        }
    }

    /// The message as the C# exception carries it.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Parse { message, offset } => format!("{message} at offset {offset}."),
            Self::Evaluation(error) => error.message(),
            Self::Cancelled => "The operation was canceled.".to_owned(),
        }
    }

    /// The reason an evaluation error gives (its message without the
    /// offset); the whole message for the others.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::Evaluation(error) => error.reason.clone(),
            other => other.message(),
        }
    }
}

impl fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ExpressionError {}

/// What a function or an operator can fail with inside a run: an
/// expression error, or an error the runtime itself raised (an overflow, a
/// pattern that does not parse), with its message.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    /// An expression error.
    Expression(ExpressionError),
    /// A runtime error. `argument` marks .NET's `ArgumentException` family,
    /// which a few callers let through where they wrap the others.
    Runtime {
        /// Its message.
        message: String,
        /// An argument error.
        argument: bool,
    },
}

impl Fault {
    /// A runtime error.
    #[must_use]
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            argument: false,
        }
    }

    /// A runtime argument error.
    #[must_use]
    pub fn argument(message: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            argument: true,
        }
    }

    /// .NET's `OverflowException` from a checked conversion.
    #[must_use]
    pub fn overflow() -> Self {
        Self::runtime("Arithmetic operation resulted in an overflow.")
    }

    /// .NET's `OverflowException` from `Convert.ToInt32`.
    #[must_use]
    pub fn int32_overflow() -> Self {
        Self::runtime("Value was either too large or too small for an Int32.")
    }

    /// The message as the exception carries it.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Expression(error) => error.message(),
            Self::Runtime { message, .. } => message.clone(),
        }
    }

    /// The error a whole run ends with: a runtime error becomes an
    /// evaluation error carrying the runtime's message.
    #[must_use]
    pub fn into_expression_error(self) -> ExpressionError {
        match self {
            Self::Expression(error) => error,
            Self::Runtime { message, .. } => ExpressionError::Evaluation(EvaluationError {
                reason: message.clone(),
                inner: Some(message),
                ..EvaluationError::default()
            }),
        }
    }
}

impl From<ExpressionError> for Fault {
    fn from(error: ExpressionError) -> Self {
        Self::Expression(error)
    }
}

impl From<EvaluationError> for Fault {
    fn from(error: EvaluationError) -> Self {
        Self::Expression(ExpressionError::Evaluation(error))
    }
}
