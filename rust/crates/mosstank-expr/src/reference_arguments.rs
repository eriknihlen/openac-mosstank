//! The reference's check of a function's arguments, made before the
//! function runs and worded exactly as it words it. It walks the function's
//! own parameters in order: the expression state it is handed takes a place
//! in the argument list, a "rest" parameter takes every remaining argument,
//! a missing optional parameter gets its default, a parameter that takes
//! anything is not checked, a number handed to a world-object parameter is
//! an object id the client must know, and any other argument must be
//! exactly the declared type. Then the count must match.

use crate::error::{EvaluationError, Fault};
use crate::reference_declared::DECLARED;
use crate::value::{ExpressionValue, signed_object_id};

fn declared(name: &str) -> Option<(&'static str, &'static str)> {
    DECLARED
        .binary_search_by(|(entry, _, _)| entry.cmp(&name))
        .ok()
        .map(|index| (DECLARED[index].1, DECLARED[index].2))
}

/// Whether the reference declares this function.
#[must_use]
pub fn declares(name: &str) -> bool {
    declared(name).is_some()
}

/// The count of arguments the reference accepts for a declared function,
/// the fewest and the most (`None`: no upper bound).
#[must_use]
pub fn accepted(name: &str) -> Option<(usize, Option<usize>)> {
    let (types, parameters) = declared(name)?;
    if types.ends_with('P') {
        return Some((0, None));
    }
    let own = parameters.chars().filter(|p| *p != 's').count();
    let required = parameters.chars().filter(|p| *p == 'r').count();
    Some((required, Some(own)))
}

fn type_of(value: &ExpressionValue) -> char {
    match value {
        ExpressionValue::Number(_) | ExpressionValue::Boolean(_) => 'N',
        ExpressionValue::String(_) => 'S',
        ExpressionValue::List(_) => 'L',
        ExpressionValue::Dictionary(_) => 'D',
        ExpressionValue::WorldObject(_) => 'W',
        ExpressionValue::Coordinates(_) => 'C',
        ExpressionValue::Stopwatch(_) => 'T',
        ExpressionValue::UiControl(_) => 'U',
    }
}

/// A type as the reference names it to the user.
fn friendly(kind: char) -> &'static str {
    match kind {
        'N' => "number",
        'S' => "string",
        'L' => "List",
        'D' => "Dictionary",
        'W' => "WorldObject",
        'C' => "Coordinates",
        'T' => "Stopwatch",
        'U' => "UIControl",
        'P' => "...items",
        _ => "Object",
    }
}

/// A value's type as the reference names it: "number", "string", or the
/// object's type name such as "List" or "UIControl".
#[must_use]
pub fn friendly_type_name(value: &ExpressionValue) -> &'static str {
    friendly(type_of(value))
}

/// The function as the reference names it in an error, e.g.
/// "wobjectgetintprop[WorldObject, number]".
#[must_use]
pub fn signature(name: &str, types: &str) -> String {
    let names: Vec<&str> = types.chars().map(friendly).collect();
    format!("{name}[{}]", names.join(", "))
}

fn error(offset: Option<usize>, message: String) -> Fault {
    EvaluationError {
        reason: message,
        offset,
        is_argument_error: true,
        ..EvaluationError::default()
    }
    .into()
}

/// Checks the arguments of a declared function; the reference's error when
/// it would raise one. `is_known_object` tells whether the client knows an
/// object id (`None`: every id is known).
///
/// # Errors
/// The reference's argument error.
pub fn check(
    name: &str,
    values: &[ExpressionValue],
    is_known_object: Option<&dyn Fn(u32) -> bool>,
    offset: Option<usize>,
) -> Result<(), Fault> {
    let Some((types, parameters)) = declared(name) else {
        return Ok(());
    };
    let types: Vec<char> = types.chars().collect();
    let parameters: Vec<char> = parameters.chars().collect();
    let rest = types.last() == Some(&'P');

    // The argument list as the reference builds it: the values, with the
    // state and defaults put in (`None`).
    let mut arguments: Vec<Option<&ExpressionValue>> = values.iter().map(Some).collect();

    for index in 0..parameters.len() {
        if parameters[index] == 's' {
            if index > arguments.len() {
                return Err(error(
                    offset,
                    "Index must be within the bounds of the List.\r\nParameter name: index"
                        .to_owned(),
                ));
            }
            arguments.insert(index, None);
            continue;
        }
        if index >= types.len() {
            return Err(error(
                offset,
                "Index was outside the bounds of the array.".to_owned(),
            ));
        }
        let kind = types[index];
        if kind == 'P' {
            arguments.truncate(index);
            arguments.push(None);
            break;
        }
        if parameters[index] == 'o' && index + 1 > arguments.len() {
            arguments.push(None);
            continue;
        }
        if kind == 'O' {
            continue;
        }
        if kind == 'W' {
            if index >= arguments.len() {
                return Err(error(
                    offset,
                    "Index was out of range. Must be non-negative and less than the size of the collection.\r\nParameter name: index"
                        .to_owned(),
                ));
            }
            if let Some(id @ ExpressionValue::Number(number)) = arguments[index] {
                let known = match signed_object_id(*number) {
                    Ok(object_id) => is_known_object.is_none_or(|known| known(object_id)),
                    Err(fault) => return Err(fault),
                };
                if known {
                    continue;
                }
                return Err(error(
                    offset,
                    format!(
                        "{} expects argument #{}/{} to be a {} but an invalid (number)id was passed instead. Passed value: {}",
                        signature(name, &types.iter().collect::<String>()),
                        index + 1,
                        types.len(),
                        friendly(kind),
                        id.to_display_string()
                    ),
                ));
            }
        }
        if let Some(Some(argument)) = arguments.get(index)
            && type_of(argument) != kind
        {
            return Err(error(
                offset,
                format!(
                    "{} expects argument #{}/{} to be a {} but a {} was passed instead. Passed value: {}",
                    signature(name, &types.iter().collect::<String>()),
                    index + 1,
                    types.len(),
                    friendly(kind),
                    friendly(type_of(argument)),
                    argument.to_display_string()
                ),
            ));
        }
    }

    let count = if arguments.is_empty() && rest {
        1
    } else {
        arguments.len()
    };
    if count != parameters.len() && !rest {
        return Err(error(
            offset,
            format!(
                "{} expects {} arguments. {count} arguments were passed.",
                signature(name, &types.iter().collect::<String>()),
                types.len()
            ),
        ));
    }
    Ok(())
}
