//! MossTank's own loot rule language (`LootRuleExpression`): `||` groups of
//! `&&` clauses, each a field, a comparison and a value. `*` (or `DEFAULT`)
//! matches everything.
//!
//! Fields are the item's own numbers (`name`, `wcid`, `value`, ...) or an
//! appraised property written as `table[key]` (`int[105]`). A clause that
//! cannot be read is an error, which the rule reads as no match.

use std::cmp::Ordering;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_dotnet::{parse_bool, try_parse_double, try_parse_uint};

/// Why an expression or a clause did not read (the C#'s `FormatException`).
pub type ExpressionError = String;

/// A compiled expression.
#[derive(Debug, Clone, PartialEq)]
pub struct LootRuleExpression {
    groups: Vec<Vec<Clause>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Clause {
    field: String,
    operation: &'static str,
    expected: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Number(f64),
    Text(String),
    Boolean(bool),
}

const OPERATIONS: [&str; 7] = [">=", "<=", "!=", "==", "~=", ">", "<"];

/// `string.Trim()`.
fn trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

/// Whether the expression is the catch-all: `*` or `DEFAULT` in any case.
#[must_use]
pub fn is_catch_all(expression: &str) -> bool {
    expression == "*" || ordinal::equals_ignore_case(expression, "DEFAULT")
}

impl LootRuleExpression {
    /// Compiles `source`.
    ///
    /// # Errors
    /// A blank source, an empty condition, or a clause without a comparison.
    pub fn compile(source: &str) -> Result<Self, ExpressionError> {
        if casing::is_white_space_only(source) {
            return Err("The value cannot be an empty string or composed entirely of whitespace. (Parameter 'source')".to_owned());
        }
        let normalized = trim(source);
        if is_catch_all(normalized) {
            return Ok(Self {
                groups: vec![Vec::new()],
            });
        }
        let mut groups = Vec::new();
        for group in split(normalized, "||") {
            let clauses = split(group, "&&")
                .into_iter()
                .map(parse_clause)
                .collect::<Result<Vec<_>, _>>()?;
            groups.push(clauses);
        }
        if groups.is_empty() || groups.iter().any(Vec::is_empty) {
            return Err("Loot expression contains an empty condition.".to_owned());
        }
        Ok(Self { groups })
    }

    /// Whether the item matches.
    ///
    /// # Errors
    /// A clause names an unknown field or table, compares text with a
    /// number, or holds a value that is not one.
    pub fn is_match(
        &self,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> Result<bool, ExpressionError> {
        if self.groups.len() == 1 && self.groups[0].is_empty() {
            return Ok(true);
        }
        for group in &self.groups {
            let mut all = true;
            for clause in group {
                if !clause.is_match(item, properties)? {
                    all = false;
                    break;
                }
            }
            if all {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn parse_clause(text: &str) -> Result<Clause, ExpressionError> {
    for operation in OPERATIONS {
        let Some(offset) = find_outside_quotes(text, operation) else {
            continue;
        };
        let field = trim(&text[..offset]);
        let expected = unquote(trim(&text[offset + operation.len()..]));
        if field.is_empty() || expected.is_empty() {
            return Err(format!("Invalid loot condition '{}'.", trim(text)));
        }
        return Ok(Clause {
            field: field.to_owned(),
            operation,
            expected: expected.to_owned(),
        });
    }
    Err(format!(
        "Loot condition '{}' needs a comparison operator.",
        trim(text)
    ))
}

/// Toggles the quote state on a quote character, as the C# scan does.
fn toggle_quote(quote: u8, current: u8) -> u8 {
    if current == b'\'' || current == b'"' {
        if quote == 0 {
            current
        } else if quote == current {
            0
        } else {
            quote
        }
    } else {
        quote
    }
}

/// The parts of `source` between the delimiters outside quotes, trimmed.
/// Quotes and delimiters are ASCII, so scanning bytes finds exactly the
/// places scanning characters would.
fn split<'a>(source: &'a str, delimiter: &str) -> Vec<&'a str> {
    let bytes = source.as_bytes();
    let delimiter = delimiter.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = 0u8;
    let mut index = 0;
    while bytes.len() >= delimiter.len() && index <= bytes.len() - delimiter.len() {
        quote = toggle_quote(quote, bytes[index]);
        if quote == 0 && bytes[index..].starts_with(delimiter) {
            result.push(trim(&source[start..index]));
            start = index + delimiter.len();
            index += delimiter.len();
            continue;
        }
        index += 1;
    }
    result.push(trim(&source[start..]));
    result
}

fn find_outside_quotes(source: &str, operation: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let operation = operation.as_bytes();
    let mut quote = 0u8;
    if bytes.len() < operation.len() {
        return None;
    }
    for index in 0..=bytes.len() - operation.len() {
        quote = toggle_quote(quote, bytes[index]);
        if quote == 0 && bytes[index..].starts_with(operation) {
            return Some(index);
        }
    }
    None
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// `double.CompareTo`: NaN sorts below every number and equals itself.
fn compare_double(left: f64, right: f64) -> Ordering {
    match left.partial_cmp(&right) {
        Some(order) => order,
        None => match (left.is_nan(), right.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            _ => Ordering::Greater,
        },
    }
}

impl Clause {
    fn is_match(
        &self,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> Result<bool, ExpressionError> {
        let actual = resolve(item, properties, &self.field)?;
        if self.operation == "~=" {
            let Value::Text(text) = actual else {
                return Err("~= is only valid for text fields.".to_owned());
            };
            return Ok(ordinal::contains_ignore_case(&text, &self.expected));
        }
        let comparison = match actual {
            Value::Number(number) => compare_double(number, parse_number(&self.expected)?),
            Value::Boolean(flag) => flag.cmp(&parse_boolean(&self.expected)?),
            Value::Text(text) => ordinal::compare_ignore_case(&text, &self.expected),
        };
        Ok(match self.operation {
            "==" => comparison == Ordering::Equal,
            "!=" => comparison != Ordering::Equal,
            ">" => comparison == Ordering::Greater,
            "<" => comparison == Ordering::Less,
            ">=" => comparison != Ordering::Less,
            "<=" => comparison != Ordering::Greater,
            _ => false,
        })
    }
}

fn resolve(
    item: &InventoryItem,
    properties: &ItemProperties,
    field: &str,
) -> Result<Value, ExpressionError> {
    let key = casing::to_lower_invariant(trim(field));
    Ok(match key.as_str() {
        "name" => Value::Text(item.name.clone()),
        "wcid" | "typeid" => Value::Number(f64::from(item.weenie_class_id)),
        "itemtype" | "type" => Value::Number(f64::from(item.item_type)),
        "stack" | "stacksize" => Value::Number(f64::from(item.stack_size)),
        "maxstack" => Value::Number(f64::from(item.maximum_stack_size)),
        "value" => Value::Number(f64::from(item.value)),
        "burden" => Value::Number(f64::from(item.burden)),
        "workmanship" => Value::Number(f64::from(item.workmanship)),
        "material" => Value::Number(f64::from(item.material_type)),
        _ => resolve_raw(&key, properties)?,
    })
}

fn lookup<T: Copy>(table: &[(u32, T)], key: u32) -> Option<T> {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, value)| *value)
}

#[allow(clippy::cast_precision_loss)]
fn resolve_raw(field: &str, properties: &ItemProperties) -> Result<Value, ExpressionError> {
    let Some((table, key)) = try_raw_key(field) else {
        return Err(format!("Unknown loot field '{field}'."));
    };
    Ok(match table {
        "int" => Value::Number(f64::from(lookup(&properties.ints, key).unwrap_or(0))),
        "int64" => Value::Number(lookup(&properties.int64s, key).unwrap_or(0) as f64),
        "bool" => Value::Boolean(lookup(&properties.bools, key).unwrap_or(false)),
        "float" => Value::Number(lookup(&properties.floats, key).unwrap_or(0.0)),
        "string" => Value::Text(
            properties
                .strings
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, text)| text.clone())
                .unwrap_or_default(),
        ),
        "did" => Value::Number(f64::from(lookup(&properties.data_ids, key).unwrap_or(0))),
        "iid" => Value::Number(f64::from(
            lookup(&properties.instance_ids, key).unwrap_or(0),
        )),
        _ => return Err(format!("Unknown raw table '{table}'.")),
    })
}

/// `table[key]`: the table's name before the first `[`, the key a whole
/// number up to a `]` that ends the field.
fn try_raw_key(field: &str) -> Option<(&str, u32)> {
    let open = field.find('[')?;
    let close = field.rfind(']')?;
    if open == 0 || close != field.len() - 1 || close < open + 1 {
        return None;
    }
    let key = try_parse_uint(&field[open + 1..close])?;
    Some((&field[..open], key))
}

fn parse_number(value: &str) -> Result<f64, ExpressionError> {
    try_parse_double(value).ok_or_else(|| format!("'{value}' is not a number."))
}

fn parse_boolean(value: &str) -> Result<bool, ExpressionError> {
    parse_bool(value).map_err(|_| format!("'{value}' is not true or false."))
}
