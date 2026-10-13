//! Expression values: numbers, strings, booleans, and the reference values
//! (lists, dictionaries, coordinates, world objects, stopwatches, UI
//! controls). Lists and dictionaries are shared and changed in place, as
//! the C# objects are; a copy of a value is a copy of the reference.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use mosstank_dotnet::compat::number::{MidpointRounding, format_f64, round_digits, to_string_f64};

use crate::dictionary::DotNetDictionary;
use crate::error::{EvaluationError, Fault};

/// Which kind a value is (the C# `ExpressionValueKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ExpressionValueKind {
    Number,
    String,
    Boolean,
    List,
    Dictionary,
    Coordinates,
    WorldObject,
    Stopwatch,
    UiControl,
}

impl ExpressionValueKind {
    /// The C# member name, as messages print it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::String => "String",
            Self::Boolean => "Boolean",
            Self::List => "List",
            Self::Dictionary => "Dictionary",
            Self::Coordinates => "Coordinates",
            Self::WorldObject => "WorldObject",
            Self::Stopwatch => "Stopwatch",
            Self::UiControl => "UiControl",
        }
    }
}

/// A map position: east-west, north-south and height, in map units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpressionCoordinates {
    /// East (positive) or west.
    pub east_west: f64,
    /// North (positive) or south.
    pub north_south: f64,
    /// Height.
    pub elevation: f64,
}

impl ExpressionCoordinates {
    /// A position.
    #[must_use]
    pub fn new(east_west: f64, north_south: f64, elevation: f64) -> Self {
        Self {
            east_west,
            north_south,
            elevation,
        }
    }

    /// The short compass form the position report prints: each half rounded
    /// to one decimal, trailing ".0" dropped, no height.
    #[must_use]
    pub fn to_compass_text(&self) -> String {
        let ns = if self.north_south < 0.0 { "S" } else { "N" };
        let ew = if self.east_west < 0.0 { "W" } else { "E" };
        let round =
            |value: f64| to_string_f64(round_digits(value.abs(), 1, MidpointRounding::ToEven));
        format!(
            "{}{ns}, {}{ew}",
            round(self.north_south),
            round(self.east_west)
        )
    }
}

impl fmt::Display for ExpressionCoordinates {
    /// The form an expression reads and writes: north/south then
    /// east/west, each with two decimals, then the height with two decimals
    /// and a Z ("36.50S, 28.90E, 0.24Z").
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ns = if self.north_south >= 0.0 { "N" } else { "S" };
        let ew = if self.east_west >= 0.0 { "E" } else { "W" };
        let f2 = |value: f64| format_f64(value, "F2").expect("F2 is supported");
        write!(
            f,
            "{}{ns}, {}{ew}, {}Z",
            f2(self.north_south.abs()),
            f2(self.east_west.abs()),
            f2(self.elevation)
        )
    }
}

/// Counts every in-place change of the lists and dictionaries that share
/// it, so a saver can tell cheaply whether anything it holds may have
/// changed since it last saved. One per expression state.
#[derive(Debug, Clone, Default)]
pub struct ChangeCounter(Rc<Cell<u64>>);

impl ChangeCounter {
    /// A counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The count.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.0.get()
    }

    fn record(&self) {
        self.0.set(self.0.get().wrapping_add(1));
    }
}

#[derive(Debug)]
struct ListData {
    items: Vec<ExpressionValue>,
    has_changes: bool,
    counter: ChangeCounter,
}

/// A list, shared and changed in place.
#[derive(Debug, Clone)]
pub struct ExpressionList(Rc<RefCell<ListData>>);

impl ExpressionList {
    /// A list of `items` whose changes `counter` counts.
    #[must_use]
    pub fn new(items: Vec<ExpressionValue>, counter: &ChangeCounter) -> Self {
        Self(Rc::new(RefCell::new(ListData {
            items,
            has_changes: false,
            counter: counter.clone(),
        })))
    }

    /// A copy of the items.
    #[must_use]
    pub fn items(&self) -> Vec<ExpressionValue> {
        self.0.borrow().items.clone()
    }

    /// How many items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.borrow().items.len()
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The item at `index`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<ExpressionValue> {
        self.0.borrow().items.get(index).cloned()
    }

    /// Changes the items (the change is not recorded; see
    /// [`Self::mark_changed`]).
    pub fn with_items<R>(&self, change: impl FnOnce(&mut Vec<ExpressionValue>) -> R) -> R {
        change(&mut self.0.borrow_mut().items)
    }

    /// Whether an expression function added, inserted, removed or cleared
    /// an item of this list itself. Clearing counts even when the list was
    /// already empty; removing an item that is not there does not.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.0.borrow().has_changes
    }

    /// Records a change.
    pub fn mark_changed(&self) {
        let mut data = self.0.borrow_mut();
        data.has_changes = true;
        data.counter.record();
    }

    /// Whether two handles name the same list.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    fn address(&self) -> usize {
        Rc::as_ptr(&self.0).cast::<()>() as usize
    }
}

#[derive(Debug)]
struct DictionaryData {
    items: DotNetDictionary<ExpressionValue>,
    has_changes: bool,
    counter: ChangeCounter,
}

/// A dictionary (string keys, case counts), shared and changed in place.
#[derive(Debug, Clone)]
pub struct ExpressionDictionary(Rc<RefCell<DictionaryData>>);

impl ExpressionDictionary {
    /// An empty dictionary whose changes `counter` counts.
    #[must_use]
    pub fn new(counter: &ChangeCounter) -> Self {
        Self(Rc::new(RefCell::new(DictionaryData {
            items: DotNetDictionary::new(),
            has_changes: false,
            counter: counter.clone(),
        })))
    }

    /// The entries in enumeration order.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, ExpressionValue)> {
        self.0
            .borrow()
            .items
            .iter()
            .map(|(key, value)| (key.to_owned(), value.clone()))
            .collect()
    }

    /// The value under `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<ExpressionValue> {
        self.0.borrow().items.get(key).cloned()
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.borrow().items.len()
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Changes the entries (the change is not recorded).
    pub fn with_items<R>(
        &self,
        change: impl FnOnce(&mut DotNetDictionary<ExpressionValue>) -> R,
    ) -> R {
        change(&mut self.0.borrow_mut().items)
    }

    /// Whether an expression function added, replaced or removed a key of
    /// this dictionary itself.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.0.borrow().has_changes
    }

    /// Records a change.
    pub fn mark_changed(&self) {
        let mut data = self.0.borrow_mut();
        data.has_changes = true;
        data.counter.record();
    }

    /// Whether two handles name the same dictionary.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    fn address(&self) -> usize {
        Rc::as_ptr(&self.0).cast::<()>() as usize
    }
}

/// A monotonic clock in milliseconds.
pub type MonotonicClock = Rc<dyn Fn() -> u64>;

struct StopwatchData {
    elapsed_ms: u64,
    started_at: Option<u64>,
    clock: MonotonicClock,
}

/// A stopwatch on the session's monotonic clock.
#[derive(Clone)]
pub struct ExpressionStopwatch(Rc<RefCell<StopwatchData>>);

impl fmt::Debug for ExpressionStopwatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExpressionStopwatch")
            .field("elapsed_seconds", &self.elapsed_seconds())
            .field("running", &self.is_running())
            .finish()
    }
}

impl ExpressionStopwatch {
    /// A stopped stopwatch at zero.
    #[must_use]
    pub fn new(clock: MonotonicClock) -> Self {
        Self(Rc::new(RefCell::new(StopwatchData {
            elapsed_ms: 0,
            started_at: None,
            clock,
        })))
    }

    /// Whether it runs.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.0.borrow().started_at.is_some()
    }

    fn elapsed_ms(&self) -> u64 {
        let data = self.0.borrow();
        let running = data
            .started_at
            .map_or(0, |start| (data.clock)().saturating_sub(start));
        data.elapsed_ms + running
    }

    /// Whole milliseconds over 1000, so an expression sees the same
    /// quantised figure a profile was written against.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed_ms() as f64 / 1000.0
    }

    /// Starts (a running stopwatch goes on).
    pub fn start(&self) {
        let mut data = self.0.borrow_mut();
        if data.started_at.is_none() {
            data.started_at = Some((data.clock)());
        }
    }

    /// Stops, keeping the time.
    pub fn stop(&self) {
        let now_elapsed = self.elapsed_ms();
        let mut data = self.0.borrow_mut();
        if data.started_at.is_some() {
            data.elapsed_ms = now_elapsed;
            data.started_at = None;
        }
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The names the client knows objects by right now.
pub type ObjectNames = Rc<dyn Fn(u32) -> Option<String>>;

/// A world object: its id, and how to find its name when it is printed.
#[derive(Clone)]
pub struct ExpressionWorldObject {
    /// The object id.
    pub id: u32,
    names: Option<ObjectNames>,
}

impl fmt::Debug for ExpressionWorldObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WorldObject(0x{:08X})", self.id)
    }
}

/// A view's control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionUiControl {
    /// The view.
    pub view: String,
    /// The control.
    pub control: String,
}

/// A value.
#[derive(Clone)]
pub enum ExpressionValue {
    /// A number.
    Number(f64),
    /// A string.
    String(Rc<str>),
    /// True or false.
    Boolean(bool),
    /// A list.
    List(ExpressionList),
    /// A dictionary.
    Dictionary(ExpressionDictionary),
    /// A position (shared: two reads of one value are the same one).
    Coordinates(Rc<ExpressionCoordinates>),
    /// A world object.
    WorldObject(ExpressionWorldObject),
    /// A stopwatch.
    Stopwatch(ExpressionStopwatch),
    /// A view's control (shared).
    UiControl(Rc<ExpressionUiControl>),
}

impl fmt::Debug for ExpressionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.kind().name(), self.to_display_string())
    }
}

impl Default for ExpressionValue {
    fn default() -> Self {
        Self::Number(0.0)
    }
}

fn type_error(operation: &str, expected: &str, kind: ExpressionValueKind) -> Fault {
    EvaluationError {
        reason: format!(
            "{operation} expects {expected}, but received {}.",
            kind.name()
        ),
        is_argument_error: true,
        ..EvaluationError::default()
    }
    .into()
}

impl ExpressionValue {
    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self::Number(0.0)
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self::Number(1.0)
    }

    /// A string.
    #[must_use]
    pub fn string(text: &str) -> Self {
        Self::String(Rc::from(text))
    }

    /// A position.
    #[must_use]
    pub fn coordinates(value: ExpressionCoordinates) -> Self {
        Self::Coordinates(Rc::new(value))
    }

    /// A world object; `names` finds its name when it is printed (none
    /// prints every object as lost).
    #[must_use]
    pub fn world_object(id: u32, names: Option<ObjectNames>) -> Self {
        Self::WorldObject(ExpressionWorldObject { id, names })
    }

    /// A view's control.
    #[must_use]
    pub fn ui_control(view: &str, control: &str) -> Self {
        Self::UiControl(Rc::new(ExpressionUiControl {
            view: view.to_owned(),
            control: control.to_owned(),
        }))
    }

    /// An object id as a number, in the signed 32-bit form ids are handed
    /// out in (0x800008B5 is -2147481419).
    #[must_use]
    pub fn object_id_number(object_id: u32) -> Self {
        Self::Number(f64::from(object_id.cast_signed()))
    }

    /// The kind.
    #[must_use]
    pub fn kind(&self) -> ExpressionValueKind {
        match self {
            Self::Number(_) => ExpressionValueKind::Number,
            Self::String(_) => ExpressionValueKind::String,
            Self::Boolean(_) => ExpressionValueKind::Boolean,
            Self::List(_) => ExpressionValueKind::List,
            Self::Dictionary(_) => ExpressionValueKind::Dictionary,
            Self::Coordinates(_) => ExpressionValueKind::Coordinates,
            Self::WorldObject(_) => ExpressionValueKind::WorldObject,
            Self::Stopwatch(_) => ExpressionValueKind::Stopwatch,
            Self::UiControl(_) => ExpressionValueKind::UiControl,
        }
    }

    /// The number (a true/false is 1/0).
    ///
    /// # Errors
    /// Any other kind: "{operation} expects number, ...".
    pub fn as_number(&self, operation: &str) -> Result<f64, Fault> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Boolean(value) => Ok(if *value { 1.0 } else { 0.0 }),
            other => Err(type_error(operation, "number", other.kind())),
        }
    }

    /// The number as `Convert.ToInt32` reads it: to the nearest whole
    /// number, halves to even.
    ///
    /// # Errors
    /// Not a number, or past the 32-bit range.
    pub fn as_int32(&self, operation: &str) -> Result<i32, Fault> {
        let number = self.as_number(operation)?;
        mosstank_dotnet::compat::number::convert_to_int32(number).ok_or_else(Fault::int32_overflow)
    }

    /// The string.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_str(&self, operation: &str) -> Result<&str, Fault> {
        match self {
            Self::String(text) => Ok(text),
            other => Err(type_error(operation, "string", other.kind())),
        }
    }

    /// The list.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_list(&self, operation: &str) -> Result<&ExpressionList, Fault> {
        match self {
            Self::List(list) => Ok(list),
            other => Err(type_error(operation, "list", other.kind())),
        }
    }

    /// The dictionary.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_dictionary(&self, operation: &str) -> Result<&ExpressionDictionary, Fault> {
        match self {
            Self::Dictionary(dictionary) => Ok(dictionary),
            other => Err(type_error(operation, "dictionary", other.kind())),
        }
    }

    /// The position.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_coordinates(&self, operation: &str) -> Result<ExpressionCoordinates, Fault> {
        match self {
            Self::Coordinates(value) => Ok(**value),
            other => Err(type_error(operation, "coordinates", other.kind())),
        }
    }

    /// The stopwatch.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_stopwatch(&self, operation: &str) -> Result<&ExpressionStopwatch, Fault> {
        match self {
            Self::Stopwatch(value) => Ok(value),
            other => Err(type_error(operation, "stopwatch", other.kind())),
        }
    }

    /// The control.
    ///
    /// # Errors
    /// Any other kind.
    pub fn as_ui_control(&self, operation: &str) -> Result<&ExpressionUiControl, Fault> {
        match self {
            Self::UiControl(value) => Ok(value),
            other => Err(type_error(operation, "UI control", other.kind())),
        }
    }

    /// The object id a world object or a number names: a number in the
    /// signed form (or the unsigned form older profiles saved).
    ///
    /// # Errors
    /// Any other kind, or a number past both ranges.
    pub fn as_object_id(&self, operation: &str) -> Result<u32, Fault> {
        match self {
            Self::WorldObject(object) => Ok(object.id),
            Self::Number(value) => signed_object_id(*value),
            other => Err(type_error(operation, "world object", other.kind())),
        }
    }

    /// Whether the value counts as true: a non-zero number, a non-empty
    /// string, any reference value.
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Number(value) => *value != 0.0,
            Self::Boolean(value) => *value,
            Self::String(text) => !text.is_empty(),
            _ => true,
        }
    }

    /// The value as it prints.
    #[must_use]
    pub fn to_display_string(&self) -> String {
        self.to_display_string_with(&mut |_| None)
    }

    /// The value as it prints, a world object with the name `names` gives:
    /// `Some(None)` for an object the client has lost, `None` to use the
    /// object's own name source.
    #[must_use]
    pub fn to_display_string_with(
        &self,
        names: &mut dyn FnMut(u32) -> Option<Option<String>>,
    ) -> String {
        match self {
            Self::Number(value) => mosstank_dotnet::format_g15(*value),
            Self::Boolean(value) => if *value { "True" } else { "False" }.to_owned(),
            Self::String(text) => text.to_string(),
            Self::List(list) => {
                let items: Vec<String> = list
                    .items()
                    .iter()
                    .map(|item| item.to_display_string_with(names))
                    .collect();
                format!("[{}]", items.join(","))
            }
            Self::Dictionary(dictionary) => {
                let items: Vec<String> = dictionary
                    .entries()
                    .iter()
                    .map(|(key, value)| format!("{key}=>{}", value.to_display_string_with(names)))
                    .collect();
                format!("[{}]", items.join(","))
            }
            Self::Coordinates(value) => value.to_string(),
            Self::WorldObject(object) => {
                let hex = format!("0x{:08X}", object.id);
                let name = names(object.id)
                    .unwrap_or_else(|| object.names.as_ref().and_then(|names| names(object.id)));
                match name {
                    Some(name) => format!("{hex}: {name}"),
                    None => format!("{hex} (Invalid)"),
                }
            }
            Self::Stopwatch(stopwatch) => {
                format_f64(stopwatch.elapsed_seconds(), "0.###").expect("a supported format")
            }
            Self::UiControl(control) => format!(
                "ExpressionUiControl {{ View = {}, Control = {} }}",
                control.view, control.control
            ),
        }
    }

    /// Whether two values are the same value, as the list functions find
    /// an item: the same kind, a string exactly, a number by value (NaN
    /// equals NaN), a world object by id, a reference value only to itself.
    #[must_use]
    pub fn same_value(&self, other: &Self) -> bool {
        let number_equal = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
        match (self, other) {
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => number_equal(*a, *b),
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::WorldObject(a), Self::WorldObject(b)) => a.id == b.id,
            (Self::List(a), Self::List(b)) => a.same(b),
            (Self::Dictionary(a), Self::Dictionary(b)) => a.same(b),
            (Self::Coordinates(a), Self::Coordinates(b)) => Rc::ptr_eq(a, b),
            (Self::Stopwatch(a), Self::Stopwatch(b)) => a.same(b),
            (Self::UiControl(a), Self::UiControl(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// The identity of a list or dictionary (for cycle checks).
    pub(crate) fn collection_address(&self) -> Option<usize> {
        match self {
            Self::List(list) => Some(list.address()),
            Self::Dictionary(dictionary) => Some(dictionary.address()),
            _ => None,
        }
    }
}

/// Reads a number as an object id: rounded to the nearest whole number
/// (halves to even), then the same 32 bits. A whole number above the signed
/// range but within 32 unsigned bits is the id it names (MossTank 0.4.0
/// and earlier saved ids unsigned).
///
/// # Errors
/// A number past both ranges.
pub fn signed_object_id(number: f64) -> Result<u32, Fault> {
    let whole = mosstank_dotnet::compat::number::round(number);
    if whole > f64::from(i32::MAX) && whole <= f64::from(u32::MAX) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        return Ok(whole as u32);
    }
    mosstank_dotnet::compat::number::convert_to_int32(number)
        .map(i32::cast_unsigned)
        .ok_or_else(Fault::int32_overflow)
}
