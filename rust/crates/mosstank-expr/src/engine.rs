//! The expression language: the reference's lexer and parser (with its
//! silent recovery, which profiles are written against) and the evaluator.

use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use mosstank_dotnet::text::casing;
use openac_plugin_compat::text::utf16;

use crate::error::{ExpressionError, Fault};
use crate::runtime::{EvaluationContext, ExpressionVariableScope};
use crate::value::{ExpressionList, ExpressionValue};

/// A compiled expression.
#[derive(Debug, Clone)]
pub struct ExpressionProgram {
    statements: Vec<Node>,
    repairs: Vec<String>,
    text: String,
}

impl ExpressionProgram {
    /// Reads an expression.
    ///
    /// # Errors
    /// The text is empty or not an expression.
    pub fn compile(source: &str) -> Result<Self, ExpressionError> {
        if casing::is_white_space_only(source) {
            return Err(ExpressionError::parse("Expression is empty", 0));
        }
        let mut parser = Parser::new(source);
        let statements = parser.parse_program()?;
        let text = format!("{}<EOF>", parser.text);
        Ok(Self {
            statements,
            repairs: parser.lexer.repairs,
            text,
        })
    }

    /// What the reader dropped, skipped or supplied to make sense of the
    /// text, as the reference parser does without a word; empty when the
    /// text was read as written.
    #[must_use]
    pub fn repairs(&self) -> &[String] {
        &self.repairs
    }

    /// The expression as the reference's parser gives it back when it names
    /// it in an error: every token it read, as written, with nothing between
    /// them, a `<missing ']'>` where it supplied a closer, and `<EOF>`.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Runs the expression; its last statement's value.
    ///
    /// # Errors
    /// Any error ends the run; one the runtime raises (an overflow, a bad
    /// pattern) carries the runtime's message.
    pub fn evaluate(
        &self,
        context: &mut EvaluationContext<'_>,
    ) -> Result<ExpressionValue, ExpressionError> {
        let mut result = ExpressionValue::zero();
        for statement in &self.statements {
            result = evaluate(statement, context).map_err(Fault::into_expression_error)?;
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Node {
    Literal {
        value: ExpressionValue,
        offset: usize,
    },
    Variable {
        scope: ExpressionVariableScope,
        name: Box<Node>,
        offset: usize,
    },
    Assignment {
        scope: ExpressionVariableScope,
        name: Box<Node>,
        value: Box<Node>,
        offset: usize,
    },
    Function {
        name: String,
        arguments: Vec<Node>,
        offset: usize,
    },
    Unary {
        operation: TokenKind,
        operand: Box<Node>,
        offset: usize,
    },
    Binary {
        operation: TokenKind,
        left: Box<Node>,
        right: Box<Node>,
        offset: usize,
        source: Option<String>,
    },
    Index {
        source: Box<Node>,
        start: Option<Box<Node>>,
        end: Option<Box<Node>>,
        slice: bool,
        offset: usize,
    },
}

/// What a node evaluates to, with a true/false turned into the number 1 or
/// 0: the reference does that to every value an expression produces.
fn evaluate(node: &Node, context: &mut EvaluationContext<'_>) -> Result<ExpressionValue, Fault> {
    let value = evaluate_core(node, context)?;
    Ok(match value {
        ExpressionValue::Boolean(truth) => ExpressionValue::Number(if truth { 1.0 } else { 0.0 }),
        other => other,
    })
}

fn variable_name(name: &Node, context: &mut EvaluationContext<'_>) -> Result<String, Fault> {
    let value = evaluate(name, context)?;
    Ok(context.display(&value))
}

fn evaluate_core(
    node: &Node,
    context: &mut EvaluationContext<'_>,
) -> Result<ExpressionValue, Fault> {
    match node {
        Node::Literal { value, offset } => {
            context.step(*offset)?;
            Ok(value.clone())
        }
        Node::Variable {
            scope,
            name,
            offset,
        } => {
            context.step(*offset)?;
            let name = variable_name(name, context)?;
            Ok(context.variable(*scope, &name)?)
        }
        Node::Assignment {
            scope,
            name,
            value,
            offset,
        } => {
            context.step(*offset)?;
            let result = evaluate(value, context)?;
            let name = variable_name(name, context)?;
            Ok(context.set_variable(*scope, &name, result)?)
        }
        Node::Function {
            name,
            arguments,
            offset,
        } => {
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                values.push(evaluate(argument, context)?);
            }
            context.invoke(name, &values, *offset)
        }
        Node::Unary {
            operation,
            operand,
            offset,
        } => {
            context.step(*offset)?;
            let value = evaluate(operand, context)?;
            match operation {
                TokenKind::Minus => Ok(ExpressionValue::Number(-value.as_number("unary '-'")?)),
                TokenKind::Tilde => Ok(ExpressionValue::Number(f64::from(
                    !value.as_int32("bitwise complement")?,
                ))),
                other => Err(ExpressionError::evaluation_at(
                    format!("Unsupported unary operator {other:?}"),
                    *offset,
                )
                .into()),
            }
        }
        Node::Binary {
            operation,
            left,
            right,
            offset,
            source,
        } => evaluate_binary(*operation, left, right, *offset, source.as_deref(), context),
        Node::Index {
            source,
            start,
            end,
            slice,
            offset,
        } => evaluate_index(
            source,
            start.as_deref(),
            end.as_deref(),
            *slice,
            *offset,
            context,
        ),
    }
}

fn number_result(value: f64) -> ExpressionValue {
    ExpressionValue::Number(value)
}

fn int_result(value: i32) -> ExpressionValue {
    ExpressionValue::Number(f64::from(value))
}

fn evaluate_binary(
    operation: TokenKind,
    left: &Node,
    right: &Node,
    offset: usize,
    source: Option<&str>,
    context: &mut EvaluationContext<'_>,
) -> Result<ExpressionValue, Fault> {
    context.step(offset)?;
    let lhs = evaluate(left, context)?;
    if operation == TokenKind::AndAnd {
        // Either side false answers false (0); only a true right side is
        // handed back itself.
        if !lhs.is_truthy() {
            return Ok(ExpressionValue::zero());
        }
        let last = evaluate(right, context)?;
        return Ok(if last.is_truthy() {
            last
        } else {
            ExpressionValue::zero()
        });
    }
    if operation == TokenKind::OrOr {
        return if lhs.is_truthy() {
            Ok(lhs)
        } else {
            evaluate(right, context)
        };
    }
    let rhs = evaluate(right, context)?;
    use TokenKind as T;
    Ok(match operation {
        T::Plus => add(context, &lhs, &rhs)?,
        T::Minus => subtract(&lhs, &rhs, source)?,
        T::Star => {
            number_result(lhs.as_number("multiplication")? * rhs.as_number("multiplication")?)
        }
        T::Slash => number_result(lhs.as_number("division")? / rhs.as_number("division")?),
        T::Percent => number_result(lhs.as_number("modulo")? % rhs.as_number("modulo")?),
        T::Caret => int_result(lhs.as_int32("bitwise xor")? ^ rhs.as_int32("bitwise xor")?),
        T::ShiftLeft => {
            let value = lhs.as_int32("left shift")?;
            let count = rhs.as_int32("left shift")?;
            int_result(value.wrapping_shl(count.cast_unsigned()))
        }
        T::ShiftRight => {
            let value = lhs.as_int32("right shift")?;
            let count = rhs.as_int32("right shift")?;
            int_result(value.wrapping_shr(count.cast_unsigned()))
        }
        T::Ampersand => int_result(lhs.as_int32("bitwise and")? & rhs.as_int32("bitwise and")?),
        T::Pipe => int_result(lhs.as_int32("bitwise or")? | rhs.as_int32("bitwise or")?),
        T::Hash => regex_match(context, &lhs, &rhs)?,
        T::EqualEqual => ExpressionValue::Boolean(are_equal(context, &lhs, &rhs)),
        T::BangEqual => ExpressionValue::Boolean(!are_equal(context, &lhs, &rhs)),
        T::Less => ExpressionValue::Boolean(compare(&lhs, &rhs)? < 0),
        T::LessEqual => ExpressionValue::Boolean(compare(&lhs, &rhs)? <= 0),
        T::Greater => ExpressionValue::Boolean(compare(&lhs, &rhs)? > 0),
        T::GreaterEqual => ExpressionValue::Boolean(compare(&lhs, &rhs)? >= 0),
        other => {
            return Err(ExpressionError::evaluation_at(
                format!("Unsupported binary operator {other:?}"),
                offset,
            )
            .into());
        }
    })
}

fn add(
    context: &mut EvaluationContext<'_>,
    left: &ExpressionValue,
    right: &ExpressionValue,
) -> Result<ExpressionValue, Fault> {
    match left {
        ExpressionValue::Number(_) | ExpressionValue::Boolean(_) => Ok(number_result(
            left.as_number("addition")? + right.as_number("addition")?,
        )),
        ExpressionValue::String(text) => Ok(ExpressionValue::string(&format!(
            "{text}{}",
            context.display(right)
        ))),
        _ => Err(ExpressionError::evaluation(format!(
            "Unable to add {} to {}.",
            left.kind().name(),
            right.kind().name()
        ))
        .into()),
    }
}

fn subtract(
    left: &ExpressionValue,
    right: &ExpressionValue,
    source: Option<&str>,
) -> Result<ExpressionValue, Fault> {
    let numeric = |value: &ExpressionValue| {
        matches!(
            value,
            ExpressionValue::Number(_) | ExpressionValue::Boolean(_)
        )
    };
    if numeric(left) && numeric(right) {
        return Ok(number_result(
            left.as_number("subtraction")? - right.as_number("subtraction")?,
        ));
    }
    // A string minus a string answers the subtraction's own source text,
    // the way a dash inside an unquoted name reads back.
    if let (ExpressionValue::String(a), ExpressionValue::String(b)) = (left, right) {
        return Ok(ExpressionValue::string(
            &source.map_or_else(|| format!("{a}-{b}"), str::to_owned),
        ));
    }
    Err(ExpressionError::evaluation(format!(
        "Unable to subtract {} from {}.",
        right.kind().name(),
        left.kind().name()
    ))
    .into())
}

/// `==` as the reference decides it: a string on the left is compared,
/// ignoring case, with the right side as it prints; anything else equals
/// only a value of its own kind equal to it.
fn are_equal(
    context: &mut EvaluationContext<'_>,
    left: &ExpressionValue,
    right: &ExpressionValue,
) -> bool {
    match left {
        ExpressionValue::String(text) => {
            casing::to_lower_invariant(text) == casing::to_lower_invariant(&context.display(right))
        }
        _ => left.same_value(right),
    }
}

/// `double.CompareTo`: NaN is below everything and equal to itself.
pub(crate) fn compare_doubles(left: f64, right: f64) -> i32 {
    if left < right {
        -1
    } else if left > right {
        1
    } else if left == right {
        0
    } else if left.is_nan() {
        if right.is_nan() { 0 } else { -1 }
    } else {
        1
    }
}

fn compare(left: &ExpressionValue, right: &ExpressionValue) -> Result<i32, Fault> {
    Ok(compare_doubles(
        left.as_number("comparison")?,
        right.as_number("comparison")?,
    ))
}

fn regex_match(
    context: &mut EvaluationContext<'_>,
    left: &ExpressionValue,
    right: &ExpressionValue,
) -> Result<ExpressionValue, Fault> {
    let pattern = context.display(right);
    let regex = DotNetRegex::new(
        &pattern,
        RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
    )
    .map_err(|error| Fault::argument(error.to_string()))?;
    let input = context.display(left);
    let found = regex
        .find(&input)
        .map_err(|error| Fault::runtime(error.to_string()))?;
    // Only a match sets the capture groups; a failed one leaves the last
    // match's groups readable.
    let Some(found) = found else {
        return Ok(ExpressionValue::Boolean(false));
    };
    for name in regex.group_names() {
        let variable = format!("capturegroup_{name}");
        let group = found.group(name);
        if group.success {
            context.state.set(
                ExpressionVariableScope::Session,
                &variable,
                ExpressionValue::string(group.value),
            )?;
        } else {
            context
                .state
                .clear(ExpressionVariableScope::Session, &variable)?;
        }
    }
    Ok(ExpressionValue::Boolean(true))
}

fn utf16_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn evaluate_index(
    source: &Node,
    start: Option<&Node>,
    end: Option<&Node>,
    slice: bool,
    offset: usize,
    context: &mut EvaluationContext<'_>,
) -> Result<ExpressionValue, Fault> {
    context.step(offset)?;
    let value = evaluate(source, context)?;
    if let ExpressionValue::Dictionary(dictionary) = &value {
        if slice {
            return Err(ExpressionError::evaluation_at(
                "Range indices are not supported with dictionaries",
                offset,
            )
            .into());
        }
        let key = match start {
            Some(start) => evaluate(start, context)?,
            None => ExpressionValue::zero(),
        };
        let key = key.as_str("dictionary index")?;
        return Ok(dictionary.get(key).unwrap_or_default());
    }
    let length = match &value {
        ExpressionValue::List(list) => list.len(),
        ExpressionValue::String(text) => utf16::len(text),
        other => {
            return Err(ExpressionError::evaluation_at(
                format!("{} does not support index access", other.kind().name()),
                offset,
            )
            .into());
        }
    };
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    let first = resolve_index(context, start, length, 0, slice, offset)?;
    if !slice {
        return Ok(match &value {
            ExpressionValue::List(list) => list.get(first).unwrap_or_default(),
            ExpressionValue::String(text) => {
                let units = utf16_units(text);
                ExpressionValue::string(&String::from_utf16_lossy(&units[first..=first]))
            }
            _ => unreachable!("checked above"),
        });
    }
    let last = resolve_index(context, end, length, length, true, offset)?;
    let count = last.saturating_sub(first);
    Ok(match &value {
        ExpressionValue::List(list) => {
            let items: Vec<ExpressionValue> =
                list.items().into_iter().skip(first).take(count).collect();
            ExpressionValue::List(ExpressionList::new(items, context.state.changes()))
        }
        ExpressionValue::String(text) => {
            let units = utf16_units(text);
            ExpressionValue::string(&String::from_utf16_lossy(&units[first..first + count]))
        }
        _ => unreachable!("checked above"),
    })
}

fn resolve_index(
    context: &mut EvaluationContext<'_>,
    expression: Option<&Node>,
    length: i64,
    default: i64,
    allow_end: bool,
    offset: usize,
) -> Result<usize, Fault> {
    let mut index = match expression {
        None => default,
        Some(expression) => i64::from(truncate(evaluate(expression, context)?.as_number("index")?)),
    };
    if index < 0 {
        index += length;
    }
    let maximum = if allow_end { length } else { length - 1 };
    if index < 0 || index > maximum {
        return Err(ExpressionError::evaluation_at(
            format!("Index {index} is outside 0..{maximum}"),
            offset,
        )
        .into());
    }
    Ok(usize::try_from(index).expect("checked non-negative"))
}

/// An index as the reference reads it: cut toward zero; a number with no
/// 32-bit whole part reads as the lowest 32-bit number (out of range).
#[allow(clippy::cast_possible_truncation)]
fn truncate(number: f64) -> i32 {
    if number > -2_147_483_649.0 && number < 2_147_483_648.0 {
        number as i32
    } else {
        i32::MIN
    }
}

// ---------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    End,
    Number,
    HexNumber,
    String,
    True,
    False,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Semicolon,
    Colon,
    Dollar,
    At,
    Ampersand,
    Pipe,
    Tilde,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Hash,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    AndAnd,
    OrOr,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    /// The token as a name, with trailing spaces taken off.
    text: String,
    /// Where it starts (UTF-16 offset).
    offset: usize,
    /// An unquoted string exactly as written, trailing spaces and all.
    literal: Option<String>,
    /// The token exactly as the source spells it.
    raw: String,
}

const OPERATORS: [(&str, TokenKind); 31] = [
    (">>", TokenKind::ShiftRight),
    ("<<", TokenKind::ShiftLeft),
    (">=", TokenKind::GreaterEqual),
    ("<=", TokenKind::LessEqual),
    ("==", TokenKind::EqualEqual),
    ("!=", TokenKind::BangEqual),
    ("&&", TokenKind::AndAnd),
    ("||", TokenKind::OrOr),
    (";", TokenKind::Semicolon),
    ("(", TokenKind::LeftParen),
    (")", TokenKind::RightParen),
    ("$", TokenKind::Dollar),
    ("@", TokenKind::At),
    ("&", TokenKind::Ampersand),
    ("{", TokenKind::LeftBrace),
    (":", TokenKind::Colon),
    ("}", TokenKind::RightBrace),
    ("~", TokenKind::Tilde),
    ("^", TokenKind::Caret),
    ("|", TokenKind::Pipe),
    ("*", TokenKind::Star),
    ("/", TokenKind::Slash),
    ("%", TokenKind::Percent),
    ("+", TokenKind::Plus),
    ("#", TokenKind::Hash),
    ("=", TokenKind::Equal),
    (">", TokenKind::Greater),
    ("<", TokenKind::Less),
    ("[", TokenKind::LeftBracket),
    ("]", TokenKind::RightBracket),
    (",", TokenKind::Comma),
];

/// The reference lexer, rule for rule: at each position every token rule
/// is tried, the longest match wins and a tie goes to the rule listed first
/// (the operators, then true/false, '-', a number, a hexadecimal number, a
/// string). A position no rule matches is not an error: the characters the
/// rules had taken before they all failed are dropped, together with the
/// one they failed on, and lexing goes on.
struct Lexer {
    chars: Vec<char>,
    /// UTF-16 offset of each char (and of the end).
    units: Vec<usize>,
    position: usize,
    repairs: Vec<String>,
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_string_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '\'' | '"')
}

fn unescape(value: &[char]) -> String {
    let mut built = String::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        let mut current = value[index];
        if current == '\\' && index + 1 < value.len() {
            index += 1;
            current = value[index];
        }
        built.push(current);
        index += 1;
    }
    built
}

impl Lexer {
    fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let mut units = Vec::with_capacity(chars.len() + 1);
        let mut offset = 0;
        for c in &chars {
            units.push(offset);
            offset += c.len_utf16();
        }
        units.push(offset);
        Self {
            chars,
            units,
            position: 0,
            repairs: Vec::new(),
        }
    }

    fn text(&self, from: usize, to: usize) -> String {
        self.chars[from..to].iter().collect()
    }

    fn next(&mut self) -> Token {
        loop {
            while self.position < self.chars.len() && self.chars[self.position].is_whitespace() {
                self.position += 1;
            }
            if self.position >= self.chars.len() {
                return Token {
                    kind: TokenKind::End,
                    text: String::new(),
                    offset: self.units[self.position],
                    literal: None,
                    raw: String::new(),
                };
            }
            let start = self.position;
            let mut viable = 0;
            let mut best = 0;
            let mut kind = TokenKind::End;
            let quoted = self.chars[start] == '`';
            let mut consider = |accepted: usize, reached: usize, rule: TokenKind| {
                viable = viable.max(reached);
                if accepted > best {
                    best = accepted;
                    kind = rule;
                }
            };
            let (length, reach, candidate) = self.match_operator(start);
            consider(length, reach, candidate);
            let (length, reach, truth) = self.match_boolean(start);
            consider(
                length,
                reach,
                if truth {
                    TokenKind::True
                } else {
                    TokenKind::False
                },
            );
            let minus = usize::from(self.chars[start] == '-');
            consider(minus, minus, TokenKind::Minus);
            let (length, reach) = self.match_number(start);
            consider(length, reach, TokenKind::Number);
            let (length, reach) = self.match_hex_number(start);
            consider(length, reach, TokenKind::HexNumber);
            let (length, reach) = if quoted {
                self.match_quoted(start)
            } else {
                self.match_unquoted(start)
            };
            consider(length, reach, TokenKind::String);

            if best == 0 {
                // Nothing matched: drop what the rules had taken, and the
                // character they failed on.
                let failed_at = start + viable;
                self.position = if failed_at < self.chars.len() {
                    failed_at + 1
                } else {
                    failed_at
                };
                let dropped = self.text(start, self.position);
                self.repairs
                    .push(format!("dropped '{dropped}' at {}", self.units[start]));
                continue;
            }

            self.position = start + best;
            let raw_chars = &self.chars[start..self.position];
            let raw: String = raw_chars.iter().collect();
            let offset = self.units[start];
            let (text, literal) = match kind {
                TokenKind::String if quoted => {
                    let inner = if raw_chars.len() >= 2 {
                        &raw_chars[1..raw_chars.len() - 1]
                    } else {
                        &[][..]
                    };
                    (unescape(inner), None)
                }
                TokenKind::String => {
                    let mut trimmed = raw_chars.len();
                    while trimmed > 0 && raw_chars[trimmed - 1].is_whitespace() {
                        trimmed -= 1;
                    }
                    (unescape(&raw_chars[..trimmed]), Some(unescape(raw_chars)))
                }
                TokenKind::HexNumber => (raw_chars[2..].iter().collect(), None),
                _ => (raw.clone(), None),
            };
            return Token {
                kind,
                text,
                offset,
                literal,
                raw,
            };
        }
    }

    fn prefix(&self, start: usize, word: &str, ignore_case: bool) -> usize {
        let mut matched = 0;
        for expected in word.chars() {
            let Some(actual) = self.chars.get(start + matched) else {
                break;
            };
            let actual = if ignore_case {
                casing::to_lower_char(*actual)
            } else {
                *actual
            };
            if actual != expected {
                break;
            }
            matched += 1;
        }
        matched
    }

    fn match_operator(&self, start: usize) -> (usize, usize, TokenKind) {
        let mut reach = 0;
        for (candidate, kind) in OPERATORS {
            let matched = self.prefix(start, candidate, false);
            if matched == candidate.len() {
                return (matched, matched, kind);
            }
            reach = reach.max(matched);
        }
        (0, reach, TokenKind::End)
    }

    fn match_boolean(&self, start: usize) -> (usize, usize, bool) {
        let mut reach = 0;
        for word in ["true", "false"] {
            let matched = self.prefix(start, word, true);
            if matched == word.len() {
                return (matched, matched, word == "true");
            }
            reach = reach.max(matched);
        }
        (0, reach, false)
    }

    /// Digits, an optional '.', and at least one digit after it.
    fn match_number(&self, start: usize) -> (usize, usize) {
        let mut index = start;
        while index < self.chars.len() && is_digit(self.chars[index]) {
            index += 1;
        }
        let whole = index - start;
        if index >= self.chars.len() || self.chars[index] != '.' {
            return (whole, whole);
        }
        let mut fraction = index + 1;
        while fraction < self.chars.len() && is_digit(self.chars[fraction]) {
            fraction += 1;
        }
        if fraction > index + 1 {
            (fraction - start, fraction - start)
        } else {
            (whole, index + 1 - start)
        }
    }

    /// "0x" (lower case) and at least one hexadecimal digit.
    fn match_hex_number(&self, start: usize) -> (usize, usize) {
        if self.chars[start] != '0' {
            return (0, 0);
        }
        if start + 1 >= self.chars.len() || self.chars[start + 1] != 'x' {
            return (0, 1);
        }
        let mut index = start + 2;
        while index < self.chars.len() && self.chars[index].is_ascii_hexdigit() {
            index += 1;
        }
        (
            if index > start + 2 { index - start } else { 0 },
            index - start,
        )
    }

    /// A backtick string. A backslash before a backtick may escape it or
    /// be a character of its own before the closing backtick; the longer
    /// reading wins.
    fn match_quoted(&self, start: usize) -> (usize, usize) {
        let mut accepted = 0;
        let mut index = start + 1;
        while index < self.chars.len() {
            if self.chars[index] == '`' {
                return (index + 1 - start, index + 1 - start);
            }
            if self.chars[index] == '\\'
                && index + 1 < self.chars.len()
                && self.chars[index + 1] == '`'
            {
                accepted = index + 2 - start;
                index += 2;
                continue;
            }
            index += 1;
        }
        (accepted, self.chars.len() - start)
    }

    fn match_unquoted(&self, start: usize) -> (usize, usize) {
        let mut index = start;
        let mut accepted = 0;
        while index < self.chars.len() {
            let current = self.chars[index];
            if current == '\\' {
                if index + 1 >= self.chars.len() {
                    return (accepted, index + 1 - start);
                }
                index += 2;
                accepted = index - start;
                continue;
            }
            let continues = if index == start {
                is_string_start(current)
            } else {
                is_string_start(current) || is_digit(current) || current == ' '
            };
            if !continues {
                break;
            }
            index += 1;
            accepted = index - start;
        }
        (accepted, index - start)
    }
}

// ---------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------

const STATEMENT_ENDS: &[TokenKind] = &[TokenKind::Semicolon, TokenKind::End];
const ARGUMENT_ENDS: &[TokenKind] = &[TokenKind::Comma, TokenKind::RightBracket];
const GROUP_ENDS: &[TokenKind] = &[TokenKind::RightParen];
const INDEX_ENDS: &[TokenKind] = &[TokenKind::Colon, TokenKind::RightBrace];

/// Every token that may directly follow a complete operand wherever it
/// sits: a binary operator or the '{' of an index.
const OPERAND_FOLLOWERS: &[TokenKind] = &[
    TokenKind::OrOr,
    TokenKind::AndAnd,
    TokenKind::EqualEqual,
    TokenKind::BangEqual,
    TokenKind::Less,
    TokenKind::LessEqual,
    TokenKind::Greater,
    TokenKind::GreaterEqual,
    TokenKind::Hash,
    TokenKind::Plus,
    TokenKind::Minus,
    TokenKind::Star,
    TokenKind::Slash,
    TokenKind::Percent,
    TokenKind::Ampersand,
    TokenKind::Caret,
    TokenKind::Pipe,
    TokenKind::ShiftLeft,
    TokenKind::ShiftRight,
    TokenKind::LeftBrace,
];

/// The reference parser. It recovers from missing and extra tokens
/// without reporting it, and profiles are written against that:
///  - where a closer (a group's ')', a call's ']') is due, one extra token
///    before it is dropped; failing that, the closer is taken as read when
///    the token in its place may follow a closed construct anywhere up the
///    nesting;
///  - after the first statement, or the first argument of a call, one extra
///    token is dropped when the token after it ends the statement or
///    argument; anything else is an error;
///  - after the second or any later statement, every token up to the next
///    ';' or the end is skipped; after the second or any later argument,
///    every token up to one that may follow there;
///  - once it has skipped or supplied a token it is "recovering" until it
///    next reads a token it expected, and while recovering it skips nothing
///    more, and supplies every closer it has to.
///
/// Precedence, loosest first: `|| &&`; `== != < <= > >=`; `=` (right to
/// left); `#`; `+ -`; `* / %`; `& ^ |` (one level, tighter than
/// multiplication); `<< >>`; `~` and a negative number; `{index}`.
struct Parser {
    lexer: Lexer,
    current: Token,
    next: Option<Token>,
    recovering: bool,
    enclosing: Vec<&'static [TokenKind]>,
    text: String,
}

type ParseResult<T> = Result<T, ExpressionError>;

impl Parser {
    fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next();
        Self {
            lexer,
            current,
            next: None,
            recovering: false,
            enclosing: vec![STATEMENT_ENDS],
            text: String::new(),
        }
    }

    fn unexpected(&self) -> ExpressionError {
        ExpressionError::parse(
            format!("Unexpected token '{}'", self.current.text),
            self.current.offset,
        )
    }

    fn parse_program(&mut self) -> ParseResult<Vec<Node>> {
        let mut statements = vec![self.parse_expression()?];
        self.sync_after_item(STATEMENT_ENDS, false, STATEMENT_ENDS)?;
        while self.current.kind == TokenKind::Semicolon {
            self.advance();
            if self.current.kind == TokenKind::End {
                break;
            }
            statements.push(self.parse_expression()?);
            self.sync_after_item(STATEMENT_ENDS, true, STATEMENT_ENDS)?;
        }
        if self.current.kind != TokenKind::End {
            if self.peek().kind != TokenKind::End {
                return Err(self.unexpected());
            }
            self.skip();
        }
        Ok(statements)
    }

    fn parse_expression(&mut self) -> ParseResult<Node> {
        self.parse_logical()
    }

    fn parse_logical(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_comparison,
            &[TokenKind::AndAnd, TokenKind::OrOr],
        )
    }

    fn parse_comparison(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_assignment,
            &[
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::Less,
                TokenKind::LessEqual,
                TokenKind::Greater,
                TokenKind::GreaterEqual,
            ],
        )
    }

    /// Right to left, and takes everything tighter than a comparison.
    fn parse_assignment(&mut self) -> ParseResult<Node> {
        let left = self.parse_regex()?;
        if self.current.kind != TokenKind::Equal {
            return Ok(left);
        }
        let operation = self.current.offset;
        self.advance();
        let Node::Variable { scope, name, .. } = left else {
            return Err(ExpressionError::parse(
                "Only variables may appear on the left of '='",
                operation,
            ));
        };
        let value = self.parse_assignment()?;
        Ok(Node::Assignment {
            scope,
            name,
            value: Box::new(value),
            offset: operation,
        })
    }

    fn parse_regex(&mut self) -> ParseResult<Node> {
        self.parse_left(Self::parse_additive, &[TokenKind::Hash])
    }

    fn parse_additive(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_multiplicative,
            &[TokenKind::Plus, TokenKind::Minus],
        )
    }

    fn parse_multiplicative(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_bitwise,
            &[TokenKind::Star, TokenKind::Slash, TokenKind::Percent],
        )
    }

    /// One level for all three bitwise operators, binding tighter than
    /// multiplication: "2 & 3 * 2" is (2 & 3) * 2.
    fn parse_bitwise(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_shift,
            &[TokenKind::Ampersand, TokenKind::Caret, TokenKind::Pipe],
        )
    }

    fn parse_shift(&mut self) -> ParseResult<Node> {
        self.parse_left(
            Self::parse_unary,
            &[TokenKind::ShiftLeft, TokenKind::ShiftRight],
        )
    }

    fn parse_unary(&mut self) -> ParseResult<Node> {
        if !matches!(self.current.kind, TokenKind::Minus | TokenKind::Tilde) {
            return self.parse_postfix();
        }
        let operation = self.current.kind;
        let offset = self.current.offset;
        self.advance();
        let operand = self.parse_unary()?;
        Ok(Node::Unary {
            operation,
            operand: Box::new(operand),
            offset,
        })
    }

    fn parse_postfix(&mut self) -> ParseResult<Node> {
        let mut source = self.parse_primary(false)?;
        while self.current.kind == TokenKind::LeftBrace {
            let opening = self.current.offset;
            self.advance();
            let mut start = None;
            let mut end = None;
            let mut slice = false;
            self.enclosing.push(INDEX_ENDS);
            if self.current.kind != TokenKind::Colon && self.current.kind != TokenKind::RightBrace {
                start = Some(Box::new(self.parse_expression()?));
            }
            if self.current.kind == TokenKind::Colon {
                slice = true;
                self.advance();
                if self.current.kind != TokenKind::RightBrace {
                    end = Some(Box::new(self.parse_expression()?));
                }
            }
            self.enclosing.pop();
            self.require(TokenKind::RightBrace, "Closing '}' expected")?;
            source = Node::Index {
                source: Box::new(source),
                start,
                end,
                slice,
                offset: opening,
            };
        }
        Ok(source)
    }

    /// `as_name`: what is read names something (a variable), so the spaces
    /// that separate it from the next token are not part of it.
    fn parse_primary(&mut self, as_name: bool) -> ParseResult<Node> {
        let token = self.current.clone();
        match token.kind {
            TokenKind::Number => {
                self.advance();
                let value = mosstank_dotnet::parse_double(&token.text)
                    .map_err(|message| ExpressionError::parse(message, token.offset))?;
                Ok(Node::Literal {
                    value: ExpressionValue::Number(value),
                    offset: token.offset,
                })
            }
            TokenKind::HexNumber => {
                self.advance();
                Ok(Node::Literal {
                    value: ExpressionValue::Number(read_hex_literal(&token)?),
                    offset: token.offset,
                })
            }
            TokenKind::True | TokenKind::False => {
                self.advance();
                Ok(Node::Literal {
                    value: ExpressionValue::Boolean(token.kind == TokenKind::True),
                    offset: token.offset,
                })
            }
            TokenKind::String => {
                self.advance();
                if self.current.kind != TokenKind::LeftBracket {
                    self.require_string_follower(as_name)?;
                    // Read as a string, it keeps its trailing spaces; a name
                    // (a function's or a variable's) is read without them.
                    let text = if as_name {
                        token.text.clone()
                    } else {
                        token.literal.clone().unwrap_or_else(|| token.text.clone())
                    };
                    return Ok(Node::Literal {
                        value: ExpressionValue::string(&text),
                        offset: token.offset,
                    });
                }
                self.parse_function(&token)
            }
            TokenKind::Dollar | TokenKind::At | TokenKind::Ampersand => self.parse_variable(),
            TokenKind::LeftParen => {
                self.advance();
                self.enclosing.push(GROUP_ENDS);
                let nested = self.parse_expression()?;
                self.enclosing.pop();
                self.require_closer(TokenKind::RightParen, "Closing ')' expected")?;
                Ok(nested)
            }
            _ => Err(ExpressionError::parse(
                format!("Expression expected; found '{}'", token.text),
                token.offset,
            )),
        }
    }

    fn parse_function(&mut self, name: &Token) -> ParseResult<Node> {
        self.require(TokenKind::LeftBracket, "Opening '[' expected")?;
        let mut arguments = Vec::new();
        if self.current.kind != TokenKind::RightBracket {
            self.enclosing.push(ARGUMENT_ENDS);
            arguments.push(self.parse_expression()?);
            let stops = self.argument_later_stops();
            self.sync_after_item(ARGUMENT_ENDS, false, &stops)?;
            while self.current.kind == TokenKind::Comma {
                self.advance();
                arguments.push(self.parse_expression()?);
                let stops = self.argument_later_stops();
                self.sync_after_item(ARGUMENT_ENDS, true, &stops)?;
            }
            self.enclosing.pop();
        }
        self.require_closer(TokenKind::RightBracket, "Closing ']' expected")?;
        Ok(Node::Function {
            name: name.text.clone(),
            arguments,
            offset: name.offset,
        })
    }

    fn parse_variable(&mut self) -> ParseResult<Node> {
        let prefix = self.current.kind;
        let offset = self.current.offset;
        self.advance();
        if matches!(
            self.current.kind,
            TokenKind::End
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::RightBracket
                | TokenKind::RightBrace
                | TokenKind::RightParen
        ) {
            return Err(ExpressionError::parse(
                "Variable name expected",
                self.current.offset,
            ));
        }
        let name = self.parse_primary(true)?;
        let scope = match prefix {
            TokenKind::Dollar => ExpressionVariableScope::Session,
            TokenKind::At => ExpressionVariableScope::Persistent,
            _ => ExpressionVariableScope::Global,
        };
        Ok(Node::Variable {
            scope,
            name: Box::new(name),
            offset,
        })
    }

    fn parse_left(
        &mut self,
        operand: fn(&mut Self) -> ParseResult<Node>,
        operations: &[TokenKind],
    ) -> ParseResult<Node> {
        let start = self.text.len();
        let mut left = operand(self)?;
        while operations.contains(&self.current.kind) {
            let operation = self.current.kind;
            let offset = self.current.offset;
            self.advance();
            let right = operand(self)?;
            // A subtraction of two strings answers its own source text.
            let source = (operation == TokenKind::Minus).then(|| self.text[start..].to_owned());
            left = Node::Binary {
                operation,
                left: Box::new(left),
                right: Box::new(right),
                offset,
                source,
            };
        }
        Ok(left)
    }

    fn require(&mut self, expected: TokenKind, message: &str) -> ParseResult<()> {
        if self.current.kind != expected {
            return Err(ExpressionError::parse(message, self.current.offset));
        }
        self.advance();
        Ok(())
    }

    /// A group's ')' or a call's ']': one extra token before it is dropped;
    /// failing that, a missing closer is taken as read when the token in its
    /// place may follow a closed construct here.
    fn require_closer(&mut self, closer: TokenKind, message: &str) -> ParseResult<()> {
        if self.current.kind == closer {
            self.advance();
            return Ok(());
        }
        if self.peek().kind == closer {
            self.skip();
            self.advance();
            return Ok(());
        }
        if self.may_follow_a_closed_construct(self.current.kind) {
            let missing = if closer == TokenKind::RightParen {
                ")"
            } else {
                "]"
            };
            self.lexer.repairs.push(format!(
                "supplied a missing '{missing}' at {}",
                self.current.offset
            ));
            self.text.push_str(&format!("<missing '{missing}'>"));
            self.recovering = true;
            return Ok(());
        }
        Err(ExpressionError::parse(message, self.current.offset))
    }

    /// After a string value, a token that ends constructs has to end this
    /// one ('=' only after a variable's name), or the expression is refused.
    fn require_string_follower(&self, as_name: bool) -> ParseResult<()> {
        let next = self.current.kind;
        if !matches!(
            next,
            TokenKind::End
                | TokenKind::Semicolon
                | TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::RightBrace
                | TokenKind::Equal
        ) {
            return Ok(());
        }
        let innermost = self.enclosing.last().copied().unwrap_or(STATEMENT_ENDS);
        if innermost.contains(&next) || (as_name && next == TokenKind::Equal) {
            return Ok(());
        }
        Err(if next == TokenKind::End {
            ExpressionError::parse("Unexpected end of expression", self.current.offset)
        } else {
            self.unexpected()
        })
    }

    fn may_follow_a_closed_construct(&self, kind: TokenKind) -> bool {
        OPERAND_FOLLOWERS.contains(&kind) || self.enclosing.iter().any(|ends| ends.contains(&kind))
    }

    /// After a statement or a call argument: the token must end it. After
    /// the first one, one extra token before an end is dropped; after a
    /// later one, every token up to one of `stops` is skipped.
    fn sync_after_item(
        &mut self,
        ends: &[TokenKind],
        later_item: bool,
        stops: &[TokenKind],
    ) -> ParseResult<()> {
        if self.recovering || ends.contains(&self.current.kind) {
            return Ok(());
        }
        if !later_item {
            if !ends.contains(&self.peek().kind) {
                return Err(if ends == ARGUMENT_ENDS {
                    ExpressionError::parse("Closing ']' expected", self.current.offset)
                } else {
                    self.unexpected()
                });
            }
            self.skip();
            return Ok(());
        }
        self.recovering = true;
        while self.current.kind != TokenKind::End && !stops.contains(&self.current.kind) {
            self.skip();
        }
        Ok(())
    }

    /// Where skipping stops after a later call argument: its ',' or ']',
    /// an operator, or what ends any construct it sits in.
    fn argument_later_stops(&self) -> Vec<TokenKind> {
        let mut stops: Vec<TokenKind> = ARGUMENT_ENDS.to_vec();
        stops.extend_from_slice(OPERAND_FOLLOWERS);
        for ends in self.enclosing.iter().rev() {
            stops.extend_from_slice(ends);
        }
        stops
    }

    fn peek(&mut self) -> &Token {
        if self.next.is_none() {
            self.next = Some(self.lexer.next());
        }
        self.next.as_ref().expect("just filled")
    }

    /// Reads the token the parser expected; any recovery is over.
    fn advance(&mut self) {
        self.recovering = false;
        self.move_next();
    }

    /// Moves past a stray token without treating it as expected.
    fn skip(&mut self) {
        let repair = if self.current.kind == TokenKind::End {
            format!("skipped the end at {}", self.current.offset)
        } else {
            format!(
                "skipped '{}' at {}",
                self.current
                    .literal
                    .as_deref()
                    .unwrap_or(&self.current.text),
                self.current.offset
            )
        };
        self.lexer.repairs.push(repair);
        self.move_next();
    }

    fn move_next(&mut self) {
        self.text.push_str(&self.current.raw);
        self.current = match self.next.take() {
            Some(next) => next,
            None => self.lexer.next(),
        };
    }
}

/// A hexadecimal literal is a 32-bit pattern read as a signed number:
/// 0x80000000 is -2147483648 and 0xFFFFFFFF is -1. A wider pattern is a
/// literal the language cannot hold.
fn read_hex_literal(token: &Token) -> ParseResult<f64> {
    let digits = token.text.trim_start_matches('0');
    let wide =
        || ExpressionError::parse("Hexadecimal literal does not fit in 32 bits", token.offset);
    if digits.len() > 8 {
        return Err(wide());
    }
    let value = if digits.is_empty() {
        0
    } else {
        u32::from_str_radix(digits, 16).map_err(|_| wide())?
    };
    Ok(f64::from(value.cast_signed()))
}
