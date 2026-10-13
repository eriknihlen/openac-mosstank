//! The monster-spec language VTank's monster rules are written in: a match
//! such as `range>5 && species==drudge`, or bare text naming a monster.
//!
//! Its precedence is VTank's own and not the usual one: from loosest to
//! tightest, `||`, `&&`, the comparisons, `#` (regex), `-`, `+`, `*`, `/`,
//! `%`. Digits and operator characters in text must be escaped with `\`.

use std::cmp::Ordering;
use std::sync::Arc;

use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use mosstank_dotnet::{
    compare_ordinal_ignore_case, double_to_string, eq_ignore_case, is_decimal_digit,
    try_parse_double,
};

/// What a [`MonsterValue`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonsterValueKind {
    /// A number.
    Number,
    /// Text.
    Text,
    /// True or false.
    Boolean,
}

impl MonsterValueKind {
    fn name(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Text => "Text",
            Self::Boolean => "Boolean",
        }
    }
}

/// A value in a monster spec, and the value of an option.
#[derive(Debug, Clone, PartialEq)]
pub struct MonsterValue {
    /// Which of the three it is.
    pub kind: MonsterValueKind,
    /// The number (0 unless a number).
    pub number: f64,
    /// The text (empty unless text).
    pub text: String,
    /// The boolean (false unless a boolean).
    pub boolean: bool,
}

impl MonsterValue {
    /// A number.
    #[must_use]
    pub fn from_number(value: f64) -> Self {
        Self {
            kind: MonsterValueKind::Number,
            number: value,
            text: String::new(),
            boolean: false,
        }
    }

    /// Text.
    #[must_use]
    pub fn from_text(value: &str) -> Self {
        Self {
            kind: MonsterValueKind::Text,
            number: 0.0,
            text: value.to_owned(),
            boolean: false,
        }
    }

    /// A boolean.
    #[must_use]
    pub fn from_boolean(value: bool) -> Self {
        Self {
            kind: MonsterValueKind::Boolean,
            number: 0.0,
            text: String::new(),
            boolean: value,
        }
    }
}

impl std::fmt::Display for MonsterValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            MonsterValueKind::Number => f.write_str(&double_to_string(self.number)),
            MonsterValueKind::Text => f.write_str(&self.text),
            MonsterValueKind::Boolean => f.write_str(if self.boolean { "true" } else { "false" }),
        }
    }
}

/// Looks a setting up by its exact name.
pub type SettingLookup<'a> = &'a dyn Fn(&str) -> Option<MonsterValue>;

/// The live values a spec reads (`/vt listmonstervariables`).
#[derive(Clone, Copy)]
pub struct MonsterExpressionContext<'a> {
    /// The monster's name.
    pub name: &'a str,
    /// Its weenie class.
    pub type_id: u32,
    /// Its species word (empty when not known).
    pub species: &'a str,
    /// Its maximum health (-1 when not known).
    pub maximum_health: i32,
    /// How far away it is, metres.
    pub range: f32,
    /// Whether it carries a shield.
    pub has_shield: bool,
    /// The meta's state.
    pub meta_state: &'a str,
    /// A setting by its exact name (`setting_<name>`).
    pub setting: Option<SettingLookup<'a>>,
}

impl std::fmt::Debug for MonsterExpressionContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonsterExpressionContext")
            .field("name", &self.name)
            .field("type_id", &self.type_id)
            .field("species", &self.species)
            .field("maximum_health", &self.maximum_health)
            .field("range", &self.range)
            .field("has_shield", &self.has_shield)
            .field("meta_state", &self.meta_state)
            .finish_non_exhaustive()
    }
}

impl MonsterExpressionContext<'_> {
    /// The value a word names, if it names one.
    #[must_use]
    pub fn try_resolve(&self, token: &str) -> Option<MonsterValue> {
        let word = |name: &str| eq_ignore_case(token, name);
        if word("true") {
            return Some(MonsterValue::from_boolean(true));
        }
        if word("false") {
            return Some(MonsterValue::from_boolean(false));
        }
        if word("name") {
            return Some(MonsterValue::from_text(self.name));
        }
        if word("typeid") {
            return Some(MonsterValue::from_number(f64::from(self.type_id)));
        }
        if word("species") {
            return Some(MonsterValue::from_text(self.species));
        }
        if word("maxhp") {
            return Some(MonsterValue::from_number(f64::from(self.maximum_health)));
        }
        if word("range") {
            return Some(MonsterValue::from_number(f64::from(self.range)));
        }
        if word("hasshield") {
            return Some(MonsterValue::from_boolean(self.has_shield));
        }
        if word("metastate") {
            return Some(MonsterValue::from_text(self.meta_state));
        }
        const SETTING_PREFIX: &str = "setting_";
        let name = token.strip_prefix(SETTING_PREFIX)?;
        self.setting.and_then(|setting| setting(name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    End,
    Atom,
    Number,
    LeftParen,
    RightParen,
    Modulo,
    Divide,
    Multiply,
    Add,
    Subtract,
    Regex,
    NotEqual,
    Equal,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
    And,
    Or,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
    /// In UTF-16 code units, as the messages count.
    offset: usize,
}

#[derive(Debug)]
enum Node {
    Number(f64),
    Atom(String),
    Binary(TokenKind, Box<Node>, Box<Node>),
}

impl Node {
    fn is_dynamic(&self) -> bool {
        match self {
            Self::Number(_) => false,
            Self::Atom(token) => {
                eq_ignore_case(token, "range")
                    || eq_ignore_case(token, "hasshield")
                    || eq_ignore_case(token, "metastate")
                    || token.starts_with("setting_")
            }
            Self::Binary(_, left, right) => left.is_dynamic() || right.is_dynamic(),
        }
    }

    fn evaluate(&self, context: &MonsterExpressionContext<'_>) -> Result<MonsterValue, String> {
        match self {
            Self::Number(value) => Ok(MonsterValue::from_number(*value)),
            Self::Atom(token) => Ok(context
                .try_resolve(token)
                .unwrap_or_else(|| MonsterValue::from_text(token.trim()))),
            Self::Binary(operation, left, right) => {
                evaluate_binary(*operation, left, right, context)
            }
        }
    }
}

fn evaluate_binary(
    operation: TokenKind,
    left: &Node,
    right: &Node,
    context: &MonsterExpressionContext<'_>,
) -> Result<MonsterValue, String> {
    let lhs = left.evaluate(context)?;
    match operation {
        TokenKind::And => {
            let l = require_boolean(&lhs, "&&")?;
            return Ok(MonsterValue::from_boolean(
                l && require_boolean(&right.evaluate(context)?, "&&")?,
            ));
        }
        TokenKind::Or => {
            let l = require_boolean(&lhs, "||")?;
            return Ok(MonsterValue::from_boolean(
                l || require_boolean(&right.evaluate(context)?, "||")?,
            ));
        }
        _ => {}
    }
    let rhs = right.evaluate(context)?;
    match operation {
        TokenKind::Modulo => {
            let dividend = require_number(&lhs, "%")?;
            let divisor = require_non_zero(&rhs, "%")?;
            // `(long)a % (long)b`: the casts truncate (and saturate).
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let (a, b) = (dividend as i64, divisor as i64);
            match a.checked_rem(b) {
                #[allow(clippy::cast_precision_loss)]
                Some(remainder) => Ok(MonsterValue::from_number(remainder as f64)),
                None if b == 0 => Err("Attempted to divide by zero.".to_owned()),
                None => Err(mosstank_dotnet::ARITHMETIC_OVERFLOW.to_owned()),
            }
        }
        TokenKind::Divide => {
            let a = require_number(&lhs, "/")?;
            let b = require_non_zero(&rhs, "/")?;
            Ok(MonsterValue::from_number(a / b))
        }
        TokenKind::Multiply => {
            let a = require_number(&lhs, "*")?;
            let b = require_number(&rhs, "*")?;
            Ok(MonsterValue::from_number(a * b))
        }
        TokenKind::Add => {
            require_same_type(&lhs, &rhs, "+")?;
            match lhs.kind {
                MonsterValueKind::Number => Ok(MonsterValue::from_number(lhs.number + rhs.number)),
                MonsterValueKind::Text => Ok(MonsterValue::from_text(&(lhs.text + &rhs.text))),
                MonsterValueKind::Boolean => Err(type_error("+", lhs.kind)),
            }
        }
        TokenKind::Subtract => {
            let a = require_number(&lhs, "-")?;
            let b = require_number(&rhs, "-")?;
            Ok(MonsterValue::from_number(a - b))
        }
        TokenKind::Regex => {
            require_same_type(&lhs, &rhs, "#")?;
            if lhs.kind != MonsterValueKind::Text {
                return Err(type_error("#", lhs.kind));
            }
            let regex = DotNetRegex::new(
                &rhs.text,
                RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
            )
            .map_err(|error| error.to_string())?;
            let matched = regex
                .is_match(&lhs.text)
                .map_err(|error| error.to_string())?;
            Ok(MonsterValue::from_boolean(matched))
        }
        TokenKind::Equal => compare(&lhs, &rhs, Ordering::is_eq),
        TokenKind::NotEqual => compare(&lhs, &rhs, Ordering::is_ne),
        TokenKind::Greater => compare(&lhs, &rhs, Ordering::is_gt),
        TokenKind::Less => compare(&lhs, &rhs, Ordering::is_lt),
        TokenKind::GreaterOrEqual => compare(&lhs, &rhs, Ordering::is_ge),
        TokenKind::LessOrEqual => compare(&lhs, &rhs, Ordering::is_le),
        other => Err(format!(
            "Unsupported monster-expression operator {other:?}."
        )),
    }
}

/// `double.CompareTo`: NaN equals NaN and is below every number.
fn compare_double(a: f64, b: f64) -> Ordering {
    match a.partial_cmp(&b) {
        Some(order) => order,
        None if a.is_nan() && b.is_nan() => Ordering::Equal,
        None if a.is_nan() => Ordering::Less,
        None => Ordering::Greater,
    }
}

fn compare(
    left: &MonsterValue,
    right: &MonsterValue,
    predicate: fn(Ordering) -> bool,
) -> Result<MonsterValue, String> {
    require_same_type(left, right, "comparison")?;
    let comparison = match left.kind {
        MonsterValueKind::Number => compare_double(left.number, right.number),
        MonsterValueKind::Text => compare_ordinal_ignore_case(&left.text, &right.text),
        MonsterValueKind::Boolean => left.boolean.cmp(&right.boolean),
    };
    Ok(MonsterValue::from_boolean(predicate(comparison)))
}

fn require_same_type(
    left: &MonsterValue,
    right: &MonsterValue,
    operation: &str,
) -> Result<(), String> {
    if left.kind == right.kind {
        return Ok(());
    }
    Err(format!(
        "Operator {operation} requires matching operand types; received {} and {}.",
        left.kind.name(),
        right.kind.name()
    ))
}

fn require_number(value: &MonsterValue, operation: &str) -> Result<f64, String> {
    if value.kind == MonsterValueKind::Number {
        Ok(value.number)
    } else {
        Err(type_error(operation, value.kind))
    }
}

fn require_non_zero(value: &MonsterValue, operation: &str) -> Result<f64, String> {
    let number = require_number(value, operation)?;
    if number == 0.0 {
        return Err(format!("Operator {operation} divided by zero."));
    }
    Ok(number)
}

fn require_boolean(value: &MonsterValue, operation: &str) -> Result<bool, String> {
    if value.kind == MonsterValueKind::Boolean {
        Ok(value.boolean)
    } else {
        Err(type_error(operation, value.kind))
    }
}

fn type_error(operation: &str, actual: MonsterValueKind) -> String {
    format!(
        "Operator {operation} cannot be applied to {}.",
        actual.name()
    )
}

fn is_operator_start(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '%' | '/' | '*' | '+' | '-' | '#' | '!' | '=' | '>' | '<' | '&' | '|'
    )
}

struct Lexer {
    chars: Vec<char>,
    /// The UTF-16 offset of each character, and of the end.
    offsets: Vec<usize>,
    position: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let mut offsets = Vec::with_capacity(chars.len() + 1);
        let mut offset = 0;
        for c in &chars {
            offsets.push(offset);
            offset += c.len_utf16();
        }
        offsets.push(offset);
        Self {
            chars,
            offsets,
            position: 0,
        }
    }

    fn token(&self, kind: TokenKind, text: String, start: usize) -> Token {
        Token {
            kind,
            text,
            offset: self.offsets[start],
        }
    }

    fn next(&mut self) -> Result<Token, String> {
        while self.position < self.chars.len() && self.chars[self.position].is_whitespace() {
            self.position += 1;
        }
        if self.position >= self.chars.len() {
            return Ok(self.token(TokenKind::End, String::new(), self.position));
        }
        let start = self.position;
        if let Some(operation) = self.try_operator() {
            return Ok(operation);
        }
        let current = self.chars[self.position];
        if is_decimal_digit(current)
            || (current == '.'
                && self
                    .chars
                    .get(self.position + 1)
                    .is_some_and(|&c| is_decimal_digit(c)))
        {
            self.position += 1;
            while self
                .chars
                .get(self.position)
                .is_some_and(|&c| is_decimal_digit(c) || c == '.')
            {
                self.position += 1;
            }
            let number: String = self.chars[start..self.position].iter().collect();
            if try_parse_double(&number).is_none() {
                return Err(format!(
                    "Invalid number '{number}' at offset {}.",
                    self.offsets[start]
                ));
            }
            return Ok(self.token(TokenKind::Number, number, start));
        }

        let mut text = String::new();
        while let Some(&c) = self.chars.get(self.position) {
            if c == '\\' {
                let Some(&escaped) = self.chars.get(self.position + 1) else {
                    return Err(format!(
                        "Trailing escape at offset {}.",
                        self.offsets[self.position]
                    ));
                };
                text.push(escaped);
                self.position += 2;
                continue;
            }
            if is_operator_start(c) || is_decimal_digit(c) {
                break;
            }
            text.push(c);
            self.position += 1;
        }
        let atom = text.trim();
        if atom.is_empty() {
            // The C# indexes the character here; past the end (which a
            // trimmed source cannot reach) it would throw, so say "end".
            let found = self
                .chars
                .get(self.position)
                .map_or_else(String::new, char::to_string);
            return Err(format!(
                "Unexpected character '{found}' at offset {}; digits and punctuation in VTank strings must be escaped.",
                self.offsets[self.position]
            ));
        }
        Ok(self.token(TokenKind::Atom, atom.to_owned(), start))
    }

    fn try_operator(&mut self) -> Option<Token> {
        let start = self.position;
        let c = self.chars[start];
        if let Some(&next) = self.chars.get(start + 1) {
            let kind = match (c, next) {
                ('!', '=') => Some(TokenKind::NotEqual),
                ('=', '=') => Some(TokenKind::Equal),
                ('>', '=') => Some(TokenKind::GreaterOrEqual),
                ('<', '=') => Some(TokenKind::LessOrEqual),
                ('&', '&') => Some(TokenKind::And),
                ('|', '|') => Some(TokenKind::Or),
                _ => None,
            };
            if let Some(kind) = kind {
                self.position += 2;
                return Some(self.token(kind, [c, next].iter().collect(), start));
            }
        }
        let kind = match c {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '%' => TokenKind::Modulo,
            '/' => TokenKind::Divide,
            '*' => TokenKind::Multiply,
            '+' => TokenKind::Add,
            '-' => TokenKind::Subtract,
            '#' => TokenKind::Regex,
            '>' => TokenKind::Greater,
            '<' => TokenKind::Less,
            _ => return None,
        };
        self.position += 1;
        Some(self.token(kind, c.to_string(), start))
    }
}

struct Parser {
    lexer: Lexer,
    current: Token,
}

/// The levels of the grammar, loosest first, each with its operators.
const LEVELS: [&[TokenKind]; 9] = [
    &[TokenKind::Or],
    &[TokenKind::And],
    &[
        TokenKind::NotEqual,
        TokenKind::Equal,
        TokenKind::Greater,
        TokenKind::Less,
        TokenKind::GreaterOrEqual,
        TokenKind::LessOrEqual,
    ],
    &[TokenKind::Regex],
    &[TokenKind::Subtract],
    &[TokenKind::Add],
    &[TokenKind::Multiply],
    &[TokenKind::Divide],
    &[TokenKind::Modulo],
];

impl Parser {
    fn new(source: &str) -> Result<Self, String> {
        let mut lexer = Lexer::new(source);
        let current = lexer.next()?;
        Ok(Self { lexer, current })
    }

    fn parse(&mut self) -> Result<Node, String> {
        let result = self.parse_level(0)?;
        if self.current.kind != TokenKind::End {
            return Err(format!(
                "Unexpected token '{}' at offset {}.",
                self.current.text, self.current.offset
            ));
        }
        Ok(result)
    }

    fn advance(&mut self) -> Result<(), String> {
        self.current = self.lexer.next()?;
        Ok(())
    }

    fn parse_level(&mut self, level: usize) -> Result<Node, String> {
        let Some(operations) = LEVELS.get(level) else {
            return self.parse_primary();
        };
        let mut left = self.parse_level(level + 1)?;
        while operations.contains(&self.current.kind) {
            let operation = self.current.kind;
            self.advance()?;
            let right = self.parse_level(level + 1)?;
            left = Node::Binary(operation, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Node, String> {
        let token = self.current.clone();
        match token.kind {
            TokenKind::Number => {
                self.advance()?;
                let value = try_parse_double(&token.text)
                    .ok_or_else(|| mosstank_dotnet::format_exception_message(&token.text))?;
                Ok(Node::Number(value))
            }
            TokenKind::Atom => {
                self.advance()?;
                Ok(Node::Atom(token.text))
            }
            TokenKind::LeftParen => {
                self.advance()?;
                let nested = self.parse_level(0)?;
                if self.current.kind != TokenKind::RightParen {
                    return Err(format!(
                        "Closing ')' expected at offset {}.",
                        self.current.offset
                    ));
                }
                self.advance()?;
                Ok(nested)
            }
            _ => Err(format!(
                "Operand expected at offset {}; found '{}'.",
                token.offset, token.text
            )),
        }
    }
}

/// A compiled monster spec.
#[derive(Debug, Clone)]
pub struct MonsterExpression {
    source: String,
    root: Arc<Node>,
}

impl MonsterExpression {
    /// Compiles a spec (trimmed first).
    ///
    /// # Errors
    /// The spec does not parse; the message says where.
    pub fn compile(source: &str) -> Result<Self, String> {
        let normalized = source.trim();
        if normalized.is_empty() {
            return Err("Monster expression is empty.".to_owned());
        }
        let root = Parser::new(normalized)?.parse()?;
        Ok(Self {
            source: normalized.to_owned(),
            root: Arc::new(root),
        })
    }

    /// The spec as compiled.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether the spec reads something that changes during a fight (the
    /// range, the shield, the meta state or a setting).
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.root.is_dynamic()
    }

    /// The spec's value.
    ///
    /// # Errors
    /// A type mismatch, a division by zero, a bad regex: the message.
    pub fn try_evaluate(
        &self,
        context: &MonsterExpressionContext<'_>,
    ) -> Result<MonsterValue, String> {
        self.root.evaluate(context)
    }

    /// Whether the spec matches: a boolean result as it is, a text result
    /// when it is the monster's name (any case, trimmed), a number never.
    ///
    /// # Errors
    /// The evaluation failed (no match): the message.
    pub fn is_match(&self, context: &MonsterExpressionContext<'_>) -> Result<bool, String> {
        let result = self.try_evaluate(context)?;
        Ok(match result.kind {
            MonsterValueKind::Boolean => result.boolean,
            MonsterValueKind::Text => eq_ignore_case(result.text.trim(), context.name.trim()),
            MonsterValueKind::Number => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn context<'a>(
        name: &'a str,
        setting: Option<SettingLookup<'a>>,
    ) -> MonsterExpressionContext<'a> {
        MonsterExpressionContext {
            name,
            type_id: 1234,
            species: "Drudge",
            maximum_health: 450,
            range: 8.0,
            has_shield: true,
            meta_state: "Hunting",
            setting,
        }
    }

    const LURKER: &str = "Drudge Lurker";

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: DocumentedVariablesAndOperatorsEvaluateVerbatim
    #[test]
    fn documented_variables_and_operators_evaluate_verbatim() {
        for (source, expected) in [
            ("range>5", true),
            ("range<=5", false),
            ("range>5 && species==drudge", true),
            ("range>5 && species==drudge && name!=drudge ravener", true),
            ("hasshield && metastate==hunting", true),
            ("typeid==1234", true),
            ("maxhp>=450", true),
        ] {
            let expression = MonsterExpression::compile(source).unwrap();
            let value = expression.try_evaluate(&context(LURKER, None)).unwrap();
            assert_eq!(value.kind, MonsterValueKind::Boolean, "{source}");
            assert_eq!(value.boolean, expected, "{source}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: BareTextMatchesTheMonsterNameCaseInsensitively
    #[test]
    fn bare_text_matches_the_monster_name_case_insensitively() {
        let expression = MonsterExpression::compile("Drudge Ravener").unwrap();
        assert!(
            expression
                .is_match(&context("drudge ravener", None))
                .unwrap()
        );
        assert!(
            !expression
                .is_match(&context("Drudge Lurker", None))
                .unwrap()
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: RegexUsesLeftAsInputAndRightAsPattern
    #[test]
    fn regex_uses_left_as_input_and_right_as_pattern() {
        let expression = MonsterExpression::compile("name#^drudge .\\+er$").unwrap();
        assert_eq!(expression.is_match(&context(LURKER, None)), Ok(true));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: EscapedOperatorAndDigitRemainLiteralStringCharacters
    #[test]
    fn escaped_operator_and_digit_remain_literal_string_characters() {
        let expression = MonsterExpression::compile("name==Prototype \\#\\2").unwrap();
        assert!(expression.is_match(&context("Prototype #2", None)).unwrap());
        assert!(MonsterExpression::compile("name==Prototype 2").is_err());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: UsesVtankDocumentedNonstandardPrecedence
    #[test]
    fn uses_vtank_documented_nonstandard_precedence() {
        let subtraction = MonsterExpression::compile("10-3+1==6").unwrap();
        let modulo = MonsterExpression::compile("20/6%4==10").unwrap();
        assert!(subtraction.is_match(&context(LURKER, None)).unwrap());
        assert!(modulo.is_match(&context(LURKER, None)).unwrap());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: BooleanOperatorsShortCircuitInvalidRightBranch
    #[test]
    fn boolean_operators_short_circuit_invalid_right_branch() {
        let expression = MonsterExpression::compile("false && 1/0==0").unwrap();
        let value = expression.try_evaluate(&context(LURKER, None)).unwrap();
        assert!(!value.boolean);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: SettingNamesAreCaseSensitiveAndMakeExpressionDynamic
    #[test]
    fn setting_names_are_case_sensitive_and_make_expression_dynamic() {
        let settings: HashMap<&str, MonsterValue> =
            HashMap::from([("DoJiggle", MonsterValue::from_boolean(true))]);
        let lookup = |name: &str| settings.get(name).cloned();
        let correct = MonsterExpression::compile("setting_DoJiggle").unwrap();
        let wrong = MonsterExpression::compile("setting_dojiggle").unwrap();
        assert!(correct.is_dynamic());
        assert!(correct.is_match(&context(LURKER, Some(&lookup))).unwrap());
        assert!(!wrong.is_match(&context(LURKER, Some(&lookup))).unwrap());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: TypeMismatchIsAReportedNonMatchRatherThanAPluginCrash
    #[test]
    fn type_mismatch_is_a_reported_non_match_rather_than_a_plugin_crash() {
        let expression = MonsterExpression::compile("name==5").unwrap();
        let error = expression.is_match(&context(LURKER, None)).unwrap_err();
        assert!(error.contains("matching operand types"), "{error}");
    }

    #[test]
    fn messages_name_the_place_in_utf16_units() {
        assert_eq!(
            MonsterExpression::compile("(((").unwrap_err(),
            "Operand expected at offset 3; found ''."
        );
        assert_eq!(
            MonsterExpression::compile("\u{1F600}==a)").unwrap_err(),
            "Unexpected token ')' at offset 5."
        );
        assert_eq!(
            MonsterExpression::compile("a = b").unwrap_err(),
            "Unexpected character '=' at offset 2; digits and punctuation in VTank strings must be escaped."
        );
        assert_eq!(
            MonsterExpression::compile("1.2.3").unwrap_err(),
            "Invalid number '1.2.3' at offset 0."
        );
        let modulo = MonsterExpression::compile("5%0.5").unwrap();
        assert_eq!(
            modulo.try_evaluate(&context(LURKER, None)).unwrap_err(),
            "Attempted to divide by zero."
        );
    }
}
