//! The built-in functions every expression has, whatever the host: the
//! variables, conversions and math, lists, dictionaries, coordinates, time.

use std::cell::RefCell;
use std::rc::Rc;

use mosstank_dotnet::compat::DotNetRandom;
use mosstank_dotnet::compat::datetime::{DateFormatError, DateTime};
use mosstank_dotnet::compat::number::{self, NumberStyles, convert_to_int32, format_u32};
use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use openac_plugin_compat::text::{collation, ordinal, utf16};

use crate::engine::compare_doubles;
use crate::error::{ExpressionError, Fault};
use crate::reference_sort;
use crate::runtime::{EvaluationContext, ExpressionVariableScope, FunctionRegistry, KnownObject};
use crate::value::{
    ExpressionCoordinates, ExpressionDictionary, ExpressionList, ExpressionStopwatch,
    ExpressionValue,
};

/// The clocks the time functions read.
pub trait ExpressionClock {
    /// Wall time, Unix milliseconds (UTC).
    fn utc_now_ms(&self) -> i64;
    /// Minutes the local time is ahead of UTC.
    fn local_offset_minutes(&self) -> i32;
    /// A monotonic clock for stopwatches, milliseconds.
    fn monotonic_ms(&self) -> u64;
}

/// Prints a line to chat.
pub type ChatWriter = Rc<dyn Fn(&str)>;

/// What the core functions need from outside.
#[derive(Clone)]
pub struct CoreOptions {
    /// The random source (`randint`).
    pub random: Rc<RefCell<DotNetRandom>>,
    /// Whether the client still knows an object, for the one core function
    /// that tells a lost object apart; `None` takes every object as known.
    pub is_known_object: Option<KnownObject>,
    /// Prints a line the reference writes to chat itself (a date format it
    /// cannot use); `None` prints nothing.
    pub write_to_chat: Option<ChatWriter>,
    /// The clocks.
    pub clock: Rc<dyn ExpressionClock>,
}

impl std::fmt::Debug for CoreOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoreOptions")
    }
}

/// A clock that stands still unless moved: for tests.
#[derive(Debug, Default)]
pub struct FixedClock {
    /// Wall time, Unix milliseconds.
    pub utc_ms: std::cell::Cell<i64>,
    /// Local offset, minutes.
    pub offset_minutes: i32,
    /// Monotonic time, milliseconds.
    pub monotonic: std::cell::Cell<u64>,
}

impl ExpressionClock for FixedClock {
    fn utc_now_ms(&self) -> i64 {
        self.utc_ms.get()
    }
    fn local_offset_minutes(&self) -> i32 {
        self.offset_minutes
    }
    fn monotonic_ms(&self) -> u64 {
        self.monotonic.get()
    }
}

impl CoreOptions {
    /// Options with a seeded random source and a still clock, nothing else.
    #[must_use]
    pub fn seeded(seed: i32) -> Self {
        Self {
            random: Rc::new(RefCell::new(DotNetRandom::new(seed))),
            is_known_object: None,
            write_to_chat: None,
            clock: Rc::new(FixedClock::default()),
        }
    }
}

/// A registry with every core function.
#[must_use]
pub fn create_default(options: &CoreOptions) -> FunctionRegistry {
    let mut registry = FunctionRegistry::new();
    register(&mut registry, options);
    registry
}

/// Adds every core function to `registry`.
pub fn register(registry: &mut FunctionRegistry, options: &CoreOptions) {
    register_variables(registry, ExpressionVariableScope::Session, "");
    register_variables(registry, ExpressionVariableScope::Persistent, "p");
    register_variables(registry, ExpressionVariableScope::Global, "g");
    register_conversions_and_math(registry, options);
    register_lists(registry);
    register_dictionaries(registry);
    register_coordinates(registry);
    register_time(registry, options);
}

type Result<T> = std::result::Result<T, Fault>;

fn register_variables(
    registry: &mut FunctionRegistry,
    scope: ExpressionVariableScope,
    infix: &str,
) {
    let get = format!("get{infix}var");
    let set = format!("set{infix}var");
    let test = format!("test{infix}var");
    let touch = format!("touch{infix}var");
    let clear = format!("clear{infix}var");
    let clear_all = format!("clearall{infix}vars");

    let name = get.clone();
    registry.register(
        &get,
        1,
        1,
        move |context, args| Ok(context.variable(scope, args[0].as_str(&name)?)?),
        Some(&format!("{get}[name]")),
        "",
    );
    let name = set.clone();
    registry.register(
        &set,
        2,
        2,
        move |context, args| {
            Ok(context.set_variable(scope, args[0].as_str(&name)?, args[1].clone())?)
        },
        Some(&format!("{set}[name,value]")),
        "",
    );
    let name = test.clone();
    registry.register(
        &test,
        1,
        1,
        move |context, args| {
            Ok(ExpressionValue::Boolean(
                context.has_variable(scope, args[0].as_str(&name)?)?,
            ))
        },
        Some(&format!("{test}[name]")),
        "",
    );
    let name = touch.clone();
    registry.register(
        &touch,
        1,
        1,
        move |context, args| {
            let variable = args[0].as_str(&name)?;
            let existed = context.has_variable(scope, variable)?;
            if !existed {
                context.set_variable(scope, variable, ExpressionValue::zero())?;
            }
            Ok(ExpressionValue::Boolean(existed))
        },
        Some(&format!("{touch}[name]")),
        "",
    );
    let name = clear.clone();
    registry.register(
        &clear,
        1,
        1,
        move |context, args| {
            Ok(ExpressionValue::Boolean(
                context.clear_variable(scope, args[0].as_str(&name)?)?,
            ))
        },
        Some(&format!("{clear}[name]")),
        "",
    );
    registry.register(
        &clear_all,
        0,
        0,
        move |context, _| {
            context.clear_variables(scope)?;
            Ok(ExpressionValue::one())
        },
        Some(&format!("{clear_all}[]")),
        "",
    );
}

fn register_unary_math(
    registry: &mut FunctionRegistry,
    name: &'static str,
    operation: fn(f64) -> f64,
) {
    registry.register(
        name,
        1,
        1,
        move |_, args| Ok(ExpressionValue::Number(operation(args[0].as_number(name)?))),
        Some(&format!("{name}[number]")),
        "",
    );
}

/// `checked((int)value)`.
fn checked_int(value: f64) -> Result<i32> {
    mosstank_dotnet::checked_double_to_int(value).map_err(Fault::runtime)
}

/// The type tags an expression exposes: 0 none, 1 number, 3 string,
/// 7 object. Booleans ride a number.
fn internal_type_tag(value: &ExpressionValue) -> f64 {
    match value {
        ExpressionValue::Number(_) | ExpressionValue::Boolean(_) => 1.0,
        ExpressionValue::String(_) => 3.0,
        _ => 7.0,
    }
}

/// A whole-string hex specifier ("X", "x", "X4"), nothing else.
fn is_standard_hex_format(format: &str) -> bool {
    let mut chars = format.chars();
    matches!(chars.next(), Some('X' | 'x')) && chars.all(|c| c.is_ascii_digit())
}

/// The number as the 32 bits a hex format shows: truncated through a
/// 64-bit integer, cut to the low 32 bits; no 64-bit form gives 0.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn wrap_to_u32(number: f64) -> u32 {
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&number) {
        (number as i64) as u32
    } else {
        0
    }
}

fn format_error(error: &number::FormatError) -> Fault {
    match error {
        number::FormatError::Invalid(_) => Fault::runtime("Format specifier was invalid."),
        other => Fault::runtime(other.to_string()),
    }
}

fn register_conversions_and_math(registry: &mut FunctionRegistry, options: &CoreOptions) {
    register_unary_math(registry, "abs", f64::abs);
    register_unary_math(registry, "acos", f64::acos);
    register_unary_math(registry, "asin", f64::asin);
    register_unary_math(registry, "atan", f64::atan);
    register_unary_math(registry, "ceiling", f64::ceil);
    register_unary_math(registry, "cos", f64::cos);
    register_unary_math(registry, "cosh", f64::cosh);
    register_unary_math(registry, "floor", f64::floor);
    register_unary_math(registry, "round", number::round);
    register_unary_math(registry, "sin", f64::sin);
    register_unary_math(registry, "sinh", f64::sinh);
    register_unary_math(registry, "sqrt", f64::sqrt);
    register_unary_math(registry, "tan", f64::tan);
    register_unary_math(registry, "tanh", f64::tanh);
    registry.register(
        "atan2",
        2,
        2,
        |_, args| {
            Ok(ExpressionValue::Number(
                args[0]
                    .as_number("atan2")?
                    .atan2(args[1].as_number("atan2")?),
            ))
        },
        Some("atan2[y,x]"),
        "",
    );
    registry.register(
        "chr",
        1,
        1,
        |_, args| {
            let code = checked_int(args[0].as_number("chr")?)?;
            let c = u32::try_from(code).ok().and_then(char::from_u32).ok_or_else(|| {
                Fault::argument(
                    "A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')",
                )
            })?;
            Ok(ExpressionValue::string(&c.to_string()))
        },
        Some("chr[codepoint]"),
        "",
    );
    registry.register(
        "ord",
        1,
        1,
        |_, args| {
            let value = args[0].as_str("ord")?;
            let Some(first) = value.chars().next() else {
                return Err(ExpressionError::evaluation("ord expects a non-empty string").into());
            };
            Ok(ExpressionValue::Number(f64::from(u32::from(first))))
        },
        Some("ord[text]"),
        "",
    );
    // Group separators are accepted, and an unparsable string is 0.
    registry.register(
        "cnumber",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::Number(
                number::parse_f64(
                    args[0].as_str("cnumber")?,
                    NumberStyles::FLOAT | NumberStyles::ALLOW_THOUSANDS,
                )
                .unwrap_or(0.0),
            ))
        },
        Some("cnumber[text]"),
        "",
    );
    // Type introspection over any value; an object the client has lost
    // answers 0, as nothing does.
    let known = options.is_known_object.clone();
    registry.register(
        "getobjectinternaltype",
        1,
        1,
        move |context, args| {
            if let ExpressionValue::WorldObject(object) = &args[0]
                && !known
                    .as_ref()
                    .map_or_else(|| context.knows_object(object.id), |known| known(object.id))
            {
                return Ok(ExpressionValue::zero());
            }
            Ok(ExpressionValue::Number(internal_type_tag(&args[0])))
        },
        Some("getobjectinternaltype[value]"),
        "",
    );
    registry.register(
        "cstr",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::string(&mosstank_dotnet::format_g15(
                args[0].as_number("cstr")?,
            )))
        },
        Some("cstr[number]"),
        "",
    );
    registry.register(
        "cstrf",
        2,
        2,
        |_, args| {
            let value = args[0].as_number("cstrf")?;
            let format = args[1].as_str("cstrf")?;
            let text = if is_standard_hex_format(format) {
                format_u32(wrap_to_u32(value), format)
            } else {
                number::format_f64(value, format)
            }
            .map_err(|error| format_error(&error))?;
            Ok(ExpressionValue::string(&text))
        },
        Some("cstrf[number,format]"),
        "",
    );
    registry.register(
        "hexstr",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::string(&format!(
                "0x{:X}",
                checked_int(args[0].as_number("hexstr")?)?
            )))
        },
        Some("hexstr[number]"),
        "",
    );
    registry.register(
        "strlen",
        1,
        1,
        |_, args| {
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(
                utf16::len(args[0].as_str("strlen")?) as f64,
            ))
        },
        Some("strlen[text]"),
        "",
    );
    registry.register(
        "tostring",
        1,
        1,
        |context, args| Ok(ExpressionValue::string(&context.display(&args[0]))),
        Some("tostring[value]"),
        "",
    );
    registry.register(
        "istrue",
        1,
        1,
        |_, args| Ok(ExpressionValue::Boolean(args[0].is_truthy())),
        Some("istrue[value]"),
        "",
    );
    registry.register(
        "isfalse",
        1,
        1,
        |_, args| Ok(ExpressionValue::Boolean(!args[0].is_truthy())),
        Some("isfalse[value]"),
        "",
    );
    registry.register(
        "iif",
        3,
        3,
        |_, args| {
            Ok(if args[0].is_truthy() {
                args[1].clone()
            } else {
                args[2].clone()
            })
        },
        Some("iif[test,trueValue,falseValue]"),
        "",
    );
    registry.register(
        "ifthen",
        2,
        3,
        |context, args| {
            let source = if args[0].is_truthy() {
                Some(args[1].as_str("ifthen")?.to_owned())
            } else if args.len() == 3 {
                Some(args[2].as_str("ifthen")?.to_owned())
            } else {
                None
            };
            let Some(source) = source else {
                return Ok(ExpressionValue::zero());
            };
            let program = context.compile(&source)?;
            Ok(context.run_separately(&program)?.0)
        },
        Some("ifthen[test,trueExpression,falseExpression?]"),
        "",
    );
    let random = options.random.clone();
    registry.register(
        "randint",
        2,
        2,
        move |_, args| {
            let minimum = checked_int(args[0].as_number("randint")?)?;
            let maximum = checked_int(args[1].as_number("randint")?)?;
            if minimum > maximum {
                return Err(Fault::argument(format!(
                    "'minValue' cannot be greater than maxValue. (Parameter 'minValue')\r\nActual value was {minimum}."
                )));
            }
            Ok(ExpressionValue::Number(f64::from(
                random.borrow_mut().next_range(minimum, maximum),
            )))
        },
        Some("randint[min,maxExclusive]"),
        "",
    );
    registry.register(
        "getregexmatch",
        2,
        2,
        |_, args| {
            let regex = DotNetRegex::new(
                args[1].as_str("getregexmatch")?,
                RegexOptions::CULTURE_INVARIANT,
            )
            .map_err(|error| Fault::argument(error.to_string()))?;
            let text = args[0].as_str("getregexmatch")?;
            let found = regex
                .find(text)
                .map_err(|error| Fault::runtime(error.to_string()))?;
            Ok(found.map_or_else(ExpressionValue::zero, |found| {
                ExpressionValue::string(found.value())
            }))
        },
        Some("getregexmatch[text,pattern]"),
        "",
    );
}

fn to_truncated_int(value: &ExpressionValue, operation: &str) -> Result<i32> {
    checked_int(value.as_number(operation)?)
}

fn bad_index(operation: &str, index: i64, count: usize, allow_end: bool) -> Fault {
    let count = i64::try_from(count).unwrap_or(i64::MAX);
    ExpressionError::evaluation(format!(
        "Unable to {operation} index {index}; valid range is 0..{}",
        if allow_end { count } else { count - 1 }
    ))
    .into()
}

fn require_list_index(
    list: &ExpressionList,
    value: &ExpressionValue,
    operation: &str,
) -> Result<usize> {
    let index = to_truncated_int(value, operation)?;
    match usize::try_from(index) {
        Ok(index) if index < list.len() => Ok(index),
        _ => Err(bad_index(operation, i64::from(index), list.len(), false)),
    }
}

fn contains_reference(
    value: &ExpressionValue,
    destination: usize,
    visited: &mut Vec<usize>,
) -> bool {
    let Some(address) = value.collection_address() else {
        return false;
    };
    if address == destination {
        return true;
    }
    if visited.contains(&address) {
        return false;
    }
    visited.push(address);
    let children = match value {
        ExpressionValue::List(list) => list.items(),
        ExpressionValue::Dictionary(dictionary) => {
            dictionary.entries().into_iter().map(|(_, v)| v).collect()
        }
        _ => Vec::new(),
    };
    children
        .iter()
        .any(|child| contains_reference(child, destination, visited))
}

fn guard_no_cycle(
    destination: &ExpressionValue,
    value: &ExpressionValue,
    operation: &str,
) -> Result<()> {
    let destination = destination.collection_address().expect("a collection");
    if contains_reference(value, destination, &mut Vec::new()) {
        return Err(ExpressionError::evaluation(format!(
            "{operation} cannot create a cyclic collection"
        ))
        .into());
    }
    Ok(())
}

fn set_session(
    context: &mut EvaluationContext<'_>,
    name: &str,
    value: ExpressionValue,
) -> Result<()> {
    context
        .state
        .set(ExpressionVariableScope::Session, name, value)?;
    Ok(())
}

const ITERATION_VARIABLES: [&str; 3] = ["0", "1", "2"];

fn save_iteration_variables(context: &mut EvaluationContext<'_>) -> Result<Vec<ExpressionValue>> {
    ITERATION_VARIABLES
        .iter()
        .map(|name| {
            context
                .state
                .get(ExpressionVariableScope::Session, name)
                .map_err(Fault::from)
        })
        .collect()
}

/// Sets `$0`, `$1` and `$2` back; one that was undefined is set to 0.
fn restore_iteration_variables(
    context: &mut EvaluationContext<'_>,
    saved: &[ExpressionValue],
) -> Result<()> {
    for (name, value) in ITERATION_VARIABLES.iter().zip(saved) {
        set_session(context, name, value.clone())?;
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn set_iteration(
    context: &mut EvaluationContext<'_>,
    index: usize,
    item: &ExpressionValue,
    accumulator: Option<&ExpressionValue>,
) -> Result<()> {
    set_session(context, "0", ExpressionValue::Number(index as f64))?;
    set_session(context, "1", item.clone())?;
    if let Some(value) = accumulator {
        set_session(context, "2", value.clone())?;
    }
    Ok(())
}

/// A listsort comparison's answer as a whole number: a number rounds
/// (halves to even) and must fit in 32 bits, a string must be a whole
/// number written out, anything else cannot be converted.
fn comparison_answer(answer: &ExpressionValue) -> Result<i32> {
    match answer {
        ExpressionValue::Number(_) | ExpressionValue::Boolean(_) => {
            convert_to_int32(answer.as_number("")?).ok_or_else(Fault::int32_overflow)
        }
        ExpressionValue::String(text) => mosstank_dotnet::parse_int(text).map_err(Fault::runtime),
        other => Err(Fault::runtime(format!(
            "A {} cannot be read as a comparison result.",
            other.kind().name()
        ))),
    }
}

/// listsort's order without an expression: numbers by value, strings in
/// the invariant culture's order, a reference value as equal to itself;
/// anything else cannot be compared.
fn default_comparison(left: &ExpressionValue, right: &ExpressionValue) -> Result<i32> {
    use ExpressionValue as V;
    match (left, right) {
        (V::String(a), V::String(b)) => Ok(match collation::compare(a, b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }),
        (V::Number(a), V::Number(b)) => Ok(compare_doubles(*a, *b)),
        (V::Boolean(a), V::Boolean(b)) => Ok(i32::from(*a) - i32::from(*b)),
        _ => {
            if matches!(
                left,
                V::List(_)
                    | V::Dictionary(_)
                    | V::Stopwatch(_)
                    | V::UiControl(_)
                    | V::Coordinates(_)
            ) && left.same_value(right)
            {
                return Ok(0);
            }
            let comparable = |value: &ExpressionValue| match value {
                V::Number(_) => Some("Double"),
                V::Boolean(_) => Some("Boolean"),
                V::String(_) => Some("String"),
                _ => None,
            };
            Err(Fault::argument(
                match (comparable(left), comparable(right)) {
                    (Some(kind), _) | (None, Some(kind)) => {
                        format!("Object must be of type {kind}.")
                    }
                    (None, None) => "At least one object must implement IComparable.".to_owned(),
                },
            ))
        }
    }
}

#[allow(clippy::too_many_lines)]
fn register_lists(registry: &mut FunctionRegistry) {
    registry.register(
        "listcreate",
        0,
        usize::MAX,
        |context, args| {
            Ok(ExpressionValue::List(ExpressionList::new(
                args.to_vec(),
                context.state.changes(),
            )))
        },
        Some("listcreate[items...]"),
        "",
    );
    registry.register(
        "listadd",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listadd")?;
            guard_no_cycle(&args[0], &args[1], "listadd")?;
            list.with_items(|items| items.push(args[1].clone()));
            list.mark_changed();
            Ok(args[0].clone())
        },
        Some("listadd[list,item]"),
        "",
    );
    registry.register(
        "listinsert",
        3,
        3,
        |_, args| {
            let list = args[0].as_list("listinsert")?;
            guard_no_cycle(&args[0], &args[1], "listinsert")?;
            let index = to_truncated_int(&args[2], "listinsert")?;
            let Some(position) = usize::try_from(index).ok().filter(|i| *i <= list.len()) else {
                return Err(bad_index("insert", i64::from(index), list.len(), true));
            };
            list.with_items(|items| items.insert(position, args[1].clone()));
            list.mark_changed();
            Ok(args[0].clone())
        },
        Some("listinsert[list,item,index]"),
        "",
    );
    registry.register(
        "listremove",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listremove")?;
            let removed = list.with_items(|items| {
                items
                    .iter()
                    .position(|item| item.same_value(&args[1]))
                    .map(|index| items.remove(index))
                    .is_some()
            });
            if removed {
                list.mark_changed();
            }
            Ok(args[0].clone())
        },
        Some("listremove[list,item]"),
        "",
    );
    registry.register(
        "listremoveat",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listremoveat")?;
            let index = require_list_index(list, &args[1], "listremoveat")?;
            list.with_items(|items| items.remove(index));
            list.mark_changed();
            Ok(args[0].clone())
        },
        Some("listremoveat[list,index]"),
        "",
    );
    registry.register(
        "listgetitem",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listgetitem")?;
            let index = require_list_index(list, &args[1], "listgetitem")?;
            Ok(list.get(index).unwrap_or_default())
        },
        Some("listgetitem[list,index]"),
        "",
    );
    registry.register(
        "listcontains",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listcontains")?;
            Ok(ExpressionValue::Boolean(
                list.items().iter().any(|item| item.same_value(&args[1])),
            ))
        },
        Some("listcontains[list,item]"),
        "",
    );
    registry.register(
        "listindexof",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listindexof")?;
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(
                list.items()
                    .iter()
                    .position(|item| item.same_value(&args[1]))
                    .map_or(-1.0, |index| index as f64),
            ))
        },
        Some("listindexof[list,item]"),
        "",
    );
    registry.register(
        "listlastindexof",
        2,
        2,
        |_, args| {
            let list = args[0].as_list("listlastindexof")?;
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(
                list.items()
                    .iter()
                    .rposition(|item| item.same_value(&args[1]))
                    .map_or(-1.0, |index| index as f64),
            ))
        },
        Some("listlastindexof[list,item]"),
        "",
    );
    registry.register(
        "listcopy",
        1,
        1,
        |context, args| {
            let list = args[0].as_list("listcopy")?;
            Ok(ExpressionValue::List(ExpressionList::new(
                list.items(),
                context.state.changes(),
            )))
        },
        Some("listcopy[list]"),
        "",
    );
    registry.register(
        "listreverse",
        1,
        1,
        |context, args| {
            let mut items = args[0].as_list("listreverse")?.items();
            items.reverse();
            Ok(ExpressionValue::List(ExpressionList::new(
                items,
                context.state.changes(),
            )))
        },
        Some("listreverse[list]"),
        "",
    );
    registry.register(
        "listpop",
        1,
        2,
        |_, args| {
            let list = args[0].as_list("listpop")?;
            let last = i64::try_from(list.len()).unwrap_or(i64::MAX) - 1;
            let index = if args.len() == 1 || args[1].as_number("listpop")? == -1.0 {
                last
            } else {
                i64::try_from(require_list_index(list, &args[1], "listpop")?).unwrap_or(i64::MAX)
            };
            let Ok(index) = usize::try_from(index) else {
                return Err(bad_index("pop", index, list.len(), false));
            };
            let result = list.with_items(|items| items.remove(index));
            list.mark_changed();
            Ok(result)
        },
        Some("listpop[list,index?]"),
        "",
    );
    registry.register(
        "listcount",
        1,
        1,
        |_, args| {
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(
                args[0].as_list("listcount")?.len() as f64
            ))
        },
        Some("listcount[list]"),
        "",
    );
    registry.register(
        "listclear",
        1,
        1,
        |_, args| {
            let list = args[0].as_list("listclear")?;
            list.with_items(Vec::clear);
            list.mark_changed();
            Ok(args[0].clone())
        },
        Some("listclear[list]"),
        "",
    );
    // The list functions run their code string once per item (per
    // comparison for listsort), each time as a run of its own. $0 is the
    // item's index and $1 the item.
    registry.register(
        "listfilter",
        2,
        2,
        |context, args| {
            let source = args[0].as_list("listfilter")?.items();
            let code = args[1].as_str("listfilter")?;
            let program = context.compile(code)?;
            let result = ExpressionList::new(Vec::new(), context.state.changes());
            let saved = save_iteration_variables(context)?;
            for (index, item) in source.iter().enumerate() {
                set_iteration(context, index, item, None)?;
                let (answer, failed) = context.run_separately(&program)?;
                // A failed run answers a whole-number 0, which the
                // reference's truth test counts as true, so the item is
                // kept. The variables go back only after a kept item.
                if failed || answer.is_truthy() {
                    result.with_items(|items| items.push(item.clone()));
                    restore_iteration_variables(context, &saved)?;
                }
            }
            Ok(ExpressionValue::List(result))
        },
        Some("listfilter[list,expression]"),
        "",
    );
    registry.register(
        "listmap",
        2,
        2,
        |context, args| {
            let source = args[0].as_list("listmap")?.items();
            let code = args[1].as_str("listmap")?;
            let program = context.compile(code)?;
            let result = ExpressionList::new(Vec::new(), context.state.changes());
            let saved = save_iteration_variables(context)?;
            for (index, item) in source.iter().enumerate() {
                set_iteration(context, index, item, None)?;
                let (answer, _) = context.run_separately(&program)?;
                result.with_items(|items| items.push(answer));
                restore_iteration_variables(context, &saved)?;
            }
            Ok(ExpressionValue::List(result))
        },
        Some("listmap[list,expression]"),
        "",
    );
    registry.register(
        "listreduce",
        2,
        2,
        |context, args| {
            let source = args[0].as_list("listreduce")?.items();
            let code = args[1].as_str("listreduce")?;
            let program = context.compile(code)?;
            let mut result = ExpressionValue::zero();
            let saved = save_iteration_variables(context)?;
            for (index, item) in source.iter().enumerate() {
                set_iteration(context, index, item, Some(&result))?;
                result = context.run_separately(&program)?.0;
                restore_iteration_variables(context, &saved)?;
            }
            Ok(result)
        },
        Some("listreduce[list,expression]"),
        "",
    );
    // $1 and $2 are the two items being compared and stay set to the last
    // pair. The answer is rounded to a whole number.
    registry.register(
        "listsort",
        1,
        2,
        |context, args| {
            let mut items = args[0].as_list("listsort")?.items();
            if args.len() == 1 || args[1].as_str("listsort")?.is_empty() {
                reference_sort::sort(&mut items, default_comparison)?;
                return Ok(ExpressionValue::List(ExpressionList::new(
                    items,
                    context.state.changes(),
                )));
            }
            let code = args[1].as_str("listsort")?;
            let program = context.compile(code)?;
            reference_sort::sort(&mut items, |left, right| {
                set_session(context, "1", left.clone())?;
                set_session(context, "2", right.clone())?;
                let (answer, _) = context.run_separately(&program)?;
                comparison_answer(&answer)
            })?;
            Ok(ExpressionValue::List(ExpressionList::new(
                items,
                context.state.changes(),
            )))
        },
        Some("listsort[list,expression?]"),
        "",
    );
    // From the start up to the end, both included; a start past the end
    // gives an empty list.
    registry.register(
        "listfromrange",
        2,
        2,
        |context, args| {
            let start = to_truncated_int(&args[0], "listfromrange")?;
            let end = to_truncated_int(&args[1], "listfromrange")?;
            let result = ExpressionList::new(Vec::new(), context.state.changes());
            if start > end {
                return Ok(ExpressionValue::List(result));
            }
            if i64::from(end) - i64::from(start) + 1 > 100_000 {
                return Err(ExpressionError::evaluation(
                    "listfromrange is limited to 100000 entries",
                )
                .into());
            }
            result.with_items(|items| {
                for value in start..=end {
                    items.push(ExpressionValue::Number(f64::from(value)));
                }
            });
            Ok(ExpressionValue::List(result))
        },
        Some("listfromrange[start,end]"),
        "",
    );
}

#[allow(clippy::too_many_lines)]
fn register_dictionaries(registry: &mut FunctionRegistry) {
    registry.register(
        "dictcreate",
        0,
        usize::MAX,
        |context, args| {
            if args.len() % 2 != 0 {
                return Err(
                    ExpressionError::evaluation("dictcreate expects key/value pairs").into(),
                );
            }
            let dictionary = ExpressionDictionary::new(context.state.changes());
            for pair in args.chunks(2) {
                let key = pair[0].as_str("dictcreate key")?;
                if !dictionary.with_items(|items| items.try_add(key, pair[1].clone())) {
                    return Err(ExpressionError::evaluation(format!(
                        "dictcreate received duplicate key '{key}'"
                    ))
                    .into());
                }
            }
            Ok(ExpressionValue::Dictionary(dictionary))
        },
        Some("dictcreate[key,value,...]"),
        "",
    );
    registry.register(
        "dictgetitem",
        2,
        2,
        |_, args| {
            let dictionary = args[0].as_dictionary("dictgetitem")?;
            let key = args[1].as_str("dictgetitem key")?;
            dictionary.get(key).ok_or_else(|| {
                ExpressionError::evaluation(format!("Dictionary key '{key}' was not found")).into()
            })
        },
        Some("dictgetitem[dictionary,key]"),
        "",
    );
    registry.register(
        "dictadditem",
        3,
        3,
        |_, args| {
            let dictionary = args[0].as_dictionary("dictadditem")?;
            let key = args[1].as_str("dictadditem key")?;
            guard_no_cycle(&args[0], &args[2], "dictadditem")?;
            let replaced = dictionary.with_items(|items| {
                let replaced = items.contains_key(key);
                items.insert(key, args[2].clone());
                replaced
            });
            dictionary.mark_changed();
            Ok(ExpressionValue::Boolean(replaced))
        },
        Some("dictadditem[dictionary,key,value]"),
        "",
    );
    registry.register(
        "dicthaskey",
        2,
        2,
        |_, args| {
            let dictionary = args[0].as_dictionary("dicthaskey")?;
            Ok(ExpressionValue::Boolean(
                dictionary.get(args[1].as_str("dicthaskey key")?).is_some(),
            ))
        },
        Some("dicthaskey[dictionary,key]"),
        "",
    );
    registry.register(
        "dictremovekey",
        2,
        2,
        |_, args| {
            let dictionary = args[0].as_dictionary("dictremovekey")?;
            let key = args[1].as_str("dictremovekey key")?;
            let removed = dictionary.with_items(|items| items.remove(key));
            if removed {
                dictionary.mark_changed();
            }
            Ok(ExpressionValue::Boolean(removed))
        },
        Some("dictremovekey[dictionary,key]"),
        "",
    );
    registry.register(
        "dictkeys",
        1,
        1,
        |context, args| {
            let keys = args[0]
                .as_dictionary("dictkeys")?
                .entries()
                .into_iter()
                .map(|(key, _)| ExpressionValue::string(&key))
                .collect();
            Ok(ExpressionValue::List(ExpressionList::new(
                keys,
                context.state.changes(),
            )))
        },
        Some("dictkeys[dictionary]"),
        "",
    );
    registry.register(
        "dictvalues",
        1,
        1,
        |context, args| {
            let values = args[0]
                .as_dictionary("dictvalues")?
                .entries()
                .into_iter()
                .map(|(_, value)| value)
                .collect();
            Ok(ExpressionValue::List(ExpressionList::new(
                values,
                context.state.changes(),
            )))
        },
        Some("dictvalues[dictionary]"),
        "",
    );
    registry.register(
        "dictsize",
        1,
        1,
        |_, args| {
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(
                args[0].as_dictionary("dictsize")?.len() as f64,
            ))
        },
        Some("dictsize[dictionary]"),
        "",
    );
    registry.register(
        "dictclear",
        1,
        1,
        |_, args| {
            // The reference removes the keys one by one, so an empty
            // dictionary sees no removal and is not changed.
            let dictionary = args[0].as_dictionary("dictclear")?;
            if !dictionary.is_empty() {
                dictionary.with_items(crate::dictionary::DotNetDictionary::clear);
                dictionary.mark_changed();
            }
            Ok(args[0].clone())
        },
        Some("dictclear[dictionary]"),
        "",
    );
    registry.register(
        "dictcopy",
        1,
        1,
        |context, args| {
            let result = ExpressionDictionary::new(context.state.changes());
            for (key, value) in args[0].as_dictionary("dictcopy")?.entries() {
                result.with_items(|items| items.insert(&key, value));
            }
            Ok(ExpressionValue::Dictionary(result))
        },
        Some("dictcopy[dictionary]"),
        "",
    );
}

// The reference's coordinate pattern, searched for anywhere in the text.
// The height's "." is any character, as it is there.
const COORDINATE_PATTERN: &str = r"(?<NSval>[0-9]{1,3}(?:\.[0-9]{1,3})?)(?<NSchr>(?:[ns]))(?:[,\s]+)?(?<EWval>[0-9]{1,3}(?:\.[0-9]{1,3})?)(?<EWchr>(?:[ew]))?(,?\s*(?<Zval>\-?\d+.?\d+)z)?";

fn coordinate_distance(
    first: &ExpressionValue,
    second: &ExpressionValue,
    with_height: bool,
) -> Result<f64> {
    let left = first.as_coordinates("coordinate distance")?;
    let right = second.as_coordinates("coordinate distance")?;
    let east_west = (left.east_west - right.east_west) * 240.0;
    let north_south = (left.north_south - right.north_south) * 240.0;
    let elevation = if with_height {
        (left.elevation - right.elevation) * 240.0
    } else {
        0.0
    };
    Ok((east_west * east_west + north_south * north_south + elevation * elevation).sqrt())
}

fn parse_coordinate_number(text: &str) -> Result<f64> {
    mosstank_dotnet::compat::number::parse_f64(
        text,
        NumberStyles::FLOAT | NumberStyles::ALLOW_THOUSANDS,
    )
    .ok_or_else(|| Fault::runtime(mosstank_dotnet::format_exception_message(text)))
}

fn register_coordinates(registry: &mut FunctionRegistry) {
    // Text with no coordinate in it reads as 0N, 0E. The height is a raw
    // height, so its reading is the written number over 240.
    registry.register(
        "coordinateparse",
        1,
        1,
        |_, args| {
            let source = args[0].as_str("coordinateparse")?;
            let regex = DotNetRegex::new(
                COORDINATE_PATTERN,
                RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
            )
            .map_err(|error| Fault::runtime(error.to_string()))?;
            let found = regex
                .find(source)
                .map_err(|error| Fault::runtime(error.to_string()))?;
            let Some(found) = found else {
                return Ok(ExpressionValue::coordinates(ExpressionCoordinates::new(
                    0.0, 0.0, 0.0,
                )));
            };
            let mut north_south = parse_coordinate_number(found.group("NSval").value)?;
            if !ordinal::equals_ignore_case(found.group("NSchr").value, "n") {
                north_south = -north_south;
            }
            let mut east_west = parse_coordinate_number(found.group("EWval").value)?;
            let letter = found.group("EWchr").value;
            if !letter.is_empty() && !ordinal::equals_ignore_case(letter, "e") {
                east_west = -east_west;
            }
            let height = found.group("Zval").value;
            let elevation = if height.is_empty() {
                0.0
            } else {
                parse_coordinate_number(height)? / 240.0
            };
            Ok(ExpressionValue::coordinates(ExpressionCoordinates::new(
                east_west,
                north_south,
                elevation,
            )))
        },
        Some("coordinateparse[text]"),
        "",
    );
    registry.register(
        "coordinategetns",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::Number(
                args[0].as_coordinates("coordinategetns")?.north_south,
            ))
        },
        Some("coordinategetns[coordinates]"),
        "",
    );
    registry.register(
        "coordinategetwe",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::Number(
                args[0].as_coordinates("coordinategetwe")?.east_west,
            ))
        },
        Some("coordinategetwe[coordinates]"),
        "",
    );
    registry.register(
        "coordinategetz",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::Number(
                args[0].as_coordinates("coordinategetz")?.elevation,
            ))
        },
        Some("coordinategetz[coordinates]"),
        "",
    );
    registry.register(
        "coordinatetostring",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::string(
                &args[0].as_coordinates("coordinatetostring")?.to_string(),
            ))
        },
        Some("coordinatetostring[coordinates]"),
        "",
    );
    registry.register(
        "coordinatedistanceflat",
        2,
        2,
        |_, args| {
            Ok(ExpressionValue::Number(coordinate_distance(
                &args[0], &args[1], false,
            )?))
        },
        Some("coordinatedistanceflat[first,second]"),
        "",
    );
    registry.register(
        "coordinatedistancewithz",
        2,
        2,
        |_, args| {
            Ok(ExpressionValue::Number(coordinate_distance(
                &args[0], &args[1], true,
            )?))
        },
        Some("coordinatedistancewithz[first,second]"),
        "",
    );
}

fn date_text(
    context: &mut EvaluationContext<'_>,
    time: &DateTime,
    format: &str,
    write_to_chat: Option<&ChatWriter>,
) -> Result<String> {
    match time.format(format) {
        Ok(text) => Ok(text),
        // A format the date cannot be written in is not an error: the
        // reference prints why to chat and answers the empty string.
        Err(DateFormatError::Invalid(_)) => {
            context.write_to_chat(
                "Input string was not in a correct format.",
                write_to_chat.map(|chat| &**chat as &dyn Fn(&str)),
            );
            Ok(String::new())
        }
        Err(other) => Err(Fault::runtime(other.to_string())),
    }
}

fn register_time(registry: &mut FunctionRegistry, options: &CoreOptions) {
    let clock = options.clock.clone();
    let chat = options.write_to_chat.clone();
    registry.register(
        "getdatetimelocal",
        0,
        1,
        move |context, args| {
            let format = if args.is_empty() {
                "hh:mm:ss tt"
            } else {
                args[0].as_str("getdatetimelocal")?
            };
            let time =
                DateTime::local_from_unix_millis(clock.utc_now_ms(), clock.local_offset_minutes());
            Ok(ExpressionValue::string(&date_text(
                context,
                &time,
                format,
                chat.as_ref(),
            )?))
        },
        Some("getdatetimelocal[format?]"),
        "",
    );
    let clock = options.clock.clone();
    let chat = options.write_to_chat.clone();
    registry.register(
        "getdatetimeutc",
        0,
        1,
        move |context, args| {
            let format = if args.is_empty() {
                "hh:mm:ss tt"
            } else {
                args[0].as_str("getdatetimeutc")?
            };
            let time = DateTime::utc_from_unix_millis(clock.utc_now_ms());
            Ok(ExpressionValue::string(&date_text(
                context,
                &time,
                format,
                chat.as_ref(),
            )?))
        },
        Some("getdatetimeutc[format?]"),
        "",
    );
    let clock = options.clock.clone();
    registry.register(
        "getunixtime",
        0,
        0,
        move |_, _| {
            #[allow(clippy::cast_precision_loss)]
            Ok(ExpressionValue::Number(clock.utc_now_ms() as f64 / 1000.0))
        },
        Some("getunixtime[]"),
        "",
    );
    let clock = options.clock.clone();
    registry.register(
        "stopwatchcreate",
        0,
        0,
        move |_, _| {
            let clock = clock.clone();
            Ok(ExpressionValue::Stopwatch(ExpressionStopwatch::new(
                Rc::new(move || clock.monotonic_ms()),
            )))
        },
        Some("stopwatchcreate[]"),
        "",
    );
    registry.register(
        "stopwatchstart",
        1,
        1,
        |_, args| {
            args[0].as_stopwatch("stopwatchstart")?.start();
            Ok(args[0].clone())
        },
        Some("stopwatchstart[stopwatch]"),
        "",
    );
    registry.register(
        "stopwatchstop",
        1,
        1,
        |_, args| {
            args[0].as_stopwatch("stopwatchstop")?.stop();
            Ok(args[0].clone())
        },
        Some("stopwatchstop[stopwatch]"),
        "",
    );
    registry.register(
        "stopwatchelapsedseconds",
        1,
        1,
        |_, args| {
            Ok(ExpressionValue::Number(
                args[0]
                    .as_stopwatch("stopwatchelapsedseconds")?
                    .elapsed_seconds(),
            ))
        },
        Some("stopwatchelapsedseconds[stopwatch]"),
        "",
    );
}
