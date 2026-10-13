//! The variables, the function table and the context one evaluation runs
//! in.

use std::rc::Rc;

use crate::dictionary::DotNetDictionary;
use crate::engine::ExpressionProgram;
use crate::error::{EvaluationError, ExpressionError, Fault};
use crate::reference_arguments;
use crate::value::{ChangeCounter, ExpressionValue};

/// Which variables: `$` session, `@` persistent (saved with the
/// character), `&` global (shared by every character on the server).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressionVariableScope {
    /// `$`.
    Session,
    /// `@`.
    Persistent,
    /// `&`.
    Global,
}

/// Where the global variables live when they are shared (the plugin's
/// storage). Every call reaches the shared store.
pub trait GlobalVariableStore {
    /// One variable.
    ///
    /// # Errors
    /// The variable's file cannot be read back.
    fn try_get(&self, name: &str) -> Result<Option<ExpressionValue>, ExpressionError>;
    /// Whether it exists (a damaged one still does).
    ///
    /// # Errors
    /// The store cannot be read.
    fn contains(&self, name: &str) -> Result<bool, ExpressionError>;
    /// Writes one.
    ///
    /// # Errors
    /// The value cannot be saved, or the store cannot be written.
    fn set(&self, name: &str, value: &ExpressionValue) -> Result<(), ExpressionError>;
    /// Removes one; whether it existed.
    ///
    /// # Errors
    /// The store cannot be written.
    fn remove(&self, name: &str) -> Result<bool, ExpressionError>;
    /// Removes all.
    ///
    /// # Errors
    /// The store cannot be written.
    fn clear(&self) -> Result<(), ExpressionError>;
    /// Every variable.
    ///
    /// # Errors
    /// A file cannot be read back.
    fn capture(&self) -> Result<Vec<(String, ExpressionValue)>, ExpressionError>;
}

/// The variables of one session.
pub struct ExpressionState {
    // Variable names of every scope are case-sensitive: `R` and `r` are two
    // variables.
    session: DotNetDictionary<ExpressionValue>,
    persistent: DotNetDictionary<ExpressionValue>,
    global: DotNetDictionary<ExpressionValue>,
    global_reads: Vec<DotNetDictionary<ExpressionValue>>,
    global_store: Option<Rc<dyn GlobalVariableStore>>,
    persistent_writes: u64,
    changes: ChangeCounter,
}

impl std::fmt::Debug for ExpressionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionState")
            .field("session", &self.session.len())
            .field("persistent", &self.persistent.len())
            .field("global", &self.global.len())
            .finish_non_exhaustive()
    }
}

impl Default for ExpressionState {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpressionState {
    /// No variables.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: DotNetDictionary::new(),
            persistent: DotNetDictionary::new(),
            global: DotNetDictionary::new(),
            global_reads: Vec::new(),
            global_store: None,
            persistent_writes: 0,
            changes: ChangeCounter::new(),
        }
    }

    /// The counter every list and dictionary made in this state's runs
    /// records its changes on.
    #[must_use]
    pub fn changes(&self) -> &ChangeCounter {
        &self.changes
    }

    /// Sends the global scope to a shared store instead of this state's own
    /// table; `None` goes back to the table.
    pub fn bind_global_store(&mut self, store: Option<Rc<dyn GlobalVariableStore>>) {
        self.global_store = store;
        for reads in &mut self.global_reads {
            reads.clear();
        }
    }

    /// Opens one top-level evaluation: within it a global variable is read
    /// from the shared store once, and the same value is handed back after.
    pub fn begin_evaluation(&mut self) {
        self.global_reads.push(DotNetDictionary::new());
    }

    /// Closes the evaluation [`Self::begin_evaluation`] opened. When it
    /// completed, every list or dictionary it read from the shared store
    /// and then changed is written back.
    ///
    /// # Errors
    /// The store cannot be written.
    pub fn end_evaluation(&mut self, completed: bool) -> Result<(), ExpressionError> {
        let reads = self.global_reads.pop().unwrap_or_default();
        let Some(store) = self.global_store.clone().filter(|_| completed) else {
            return Ok(());
        };
        for (name, value) in reads.iter() {
            let changed = match &value {
                ExpressionValue::List(list) => list.has_changes(),
                ExpressionValue::Dictionary(dictionary) => dictionary.has_changes(),
                _ => false,
            };
            if changed {
                store.set(name, value)?;
            }
        }
        Ok(())
    }

    fn table(&self, scope: ExpressionVariableScope) -> &DotNetDictionary<ExpressionValue> {
        match scope {
            ExpressionVariableScope::Session => &self.session,
            ExpressionVariableScope::Persistent => &self.persistent,
            ExpressionVariableScope::Global => &self.global,
        }
    }

    fn table_mut(
        &mut self,
        scope: ExpressionVariableScope,
    ) -> &mut DotNetDictionary<ExpressionValue> {
        match scope {
            ExpressionVariableScope::Session => &mut self.session,
            ExpressionVariableScope::Persistent => &mut self.persistent,
            ExpressionVariableScope::Global => &mut self.global,
        }
    }

    fn store_for(&self, scope: ExpressionVariableScope) -> Option<Rc<dyn GlobalVariableStore>> {
        if scope == ExpressionVariableScope::Global {
            self.global_store.clone()
        } else {
            None
        }
    }

    /// A variable's value; 0 when it does not exist.
    ///
    /// # Errors
    /// A shared global variable cannot be read.
    pub fn get(
        &mut self,
        scope: ExpressionVariableScope,
        name: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        if let Some(store) = self.store_for(scope) {
            if let Some(cached) = self.global_reads.last().and_then(|reads| reads.get(name)) {
                return Ok(cached.clone());
            }
            let value = store.try_get(name)?.unwrap_or_default();
            if let Some(reads) = self.global_reads.last_mut() {
                reads.insert(name, value.clone());
            }
            return Ok(value);
        }
        Ok(self.table(scope).get(name).cloned().unwrap_or_default())
    }

    /// Whether a variable exists.
    ///
    /// # Errors
    /// The shared store cannot be read.
    pub fn contains(
        &self,
        scope: ExpressionVariableScope,
        name: &str,
    ) -> Result<bool, ExpressionError> {
        match self.store_for(scope) {
            Some(store) => store.contains(name),
            None => Ok(self.table(scope).contains_key(name)),
        }
    }

    /// Sets a variable; the value set.
    ///
    /// # Errors
    /// A persistent value that cannot be saved (a stopwatch, a control);
    /// the shared store cannot be written.
    pub fn set(
        &mut self,
        scope: ExpressionVariableScope,
        name: &str,
        value: ExpressionValue,
    ) -> Result<ExpressionValue, ExpressionError> {
        if let Some(store) = self.store_for(scope) {
            store.set(name, &value)?;
            return Ok(value);
        }
        // A persistent variable is saved as it is set, so a value that
        // cannot be saved is refused here and nothing is stored.
        if scope == ExpressionVariableScope::Persistent {
            crate::json::require_savable(&value)?;
        }
        self.table_mut(scope).insert(name, value.clone());
        self.written(scope);
        Ok(value)
    }

    /// Removes a variable; whether it existed.
    ///
    /// # Errors
    /// The shared store cannot be written.
    pub fn clear(
        &mut self,
        scope: ExpressionVariableScope,
        name: &str,
    ) -> Result<bool, ExpressionError> {
        if let Some(store) = self.store_for(scope) {
            return store.remove(name);
        }
        self.written(scope);
        Ok(self.table_mut(scope).remove(name))
    }

    /// Removes every variable of a scope.
    ///
    /// # Errors
    /// The shared store cannot be written.
    pub fn clear_all(&mut self, scope: ExpressionVariableScope) -> Result<(), ExpressionError> {
        if let Some(store) = self.store_for(scope) {
            return store.clear();
        }
        self.table_mut(scope).clear();
        self.written(scope);
        Ok(())
    }

    /// Every variable of a scope, in enumeration order.
    ///
    /// # Errors
    /// The shared store cannot be read.
    pub fn capture(
        &self,
        scope: ExpressionVariableScope,
    ) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        match self.store_for(scope) {
            Some(store) => store.capture(),
            None => Ok(self
                .table(scope)
                .iter()
                .map(|(name, value)| (name.to_owned(), value.clone()))
                .collect()),
        }
    }

    /// Replaces every variable of a scope.
    ///
    /// # Errors
    /// The shared store cannot be written.
    pub fn replace(
        &mut self,
        scope: ExpressionVariableScope,
        values: impl IntoIterator<Item = (String, ExpressionValue)>,
    ) -> Result<(), ExpressionError> {
        if let Some(store) = self.store_for(scope) {
            store.clear()?;
            for (name, value) in values {
                store.set(&name, &value)?;
            }
            return Ok(());
        }
        let table = self.table_mut(scope);
        table.clear();
        for (name, value) in values {
            table.insert(&name, value);
        }
        self.written(scope);
        Ok(())
    }

    /// How many sets and clears the persistent variables have had, so their
    /// saver can tell whether anything changed since it saved.
    #[must_use]
    pub fn persistent_writes(&self) -> u64 {
        self.persistent_writes
    }

    /// Whether a variable of the scope holds a list or a dictionary.
    #[must_use]
    pub fn holds_collection(&self, scope: ExpressionVariableScope) -> bool {
        self.table(scope).values().any(|value| {
            matches!(
                value,
                ExpressionValue::List(_) | ExpressionValue::Dictionary(_)
            )
        })
    }

    fn written(&mut self, scope: ExpressionVariableScope) {
        if scope == ExpressionVariableScope::Persistent {
            self.persistent_writes += 1;
        }
    }
}

/// A function's body.
pub type FunctionHandler =
    Rc<dyn Fn(&mut EvaluationContext<'_>, &[ExpressionValue]) -> Result<ExpressionValue, Fault>>;

/// One function an expression may call.
#[derive(Clone)]
pub struct ExpressionFunction {
    /// Its name (case counts).
    pub name: String,
    /// The fewest arguments.
    pub minimum_arguments: usize,
    /// The most arguments.
    pub maximum_arguments: usize,
    /// Its body.
    pub handler: FunctionHandler,
    /// How errors name it (`sqrt[number]`).
    pub signature: String,
    /// What it does.
    pub description: String,
}

impl std::fmt::Debug for ExpressionFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.signature)
    }
}

/// The functions expressions may call.
#[derive(Debug, Clone, Default)]
pub struct FunctionRegistry {
    functions: DotNetDictionary<ExpressionFunction>,
}

impl FunctionRegistry {
    /// No functions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every function, in registration order.
    pub fn functions(&self) -> impl Iterator<Item = &ExpressionFunction> {
        self.functions.values()
    }

    /// Adds a function.
    ///
    /// # Panics
    /// The name is taken, empty, or the counts are backwards (a programming
    /// error, as in the C#).
    pub fn register(
        &mut self,
        name: &str,
        minimum_arguments: usize,
        maximum_arguments: usize,
        handler: impl Fn(
            &mut EvaluationContext<'_>,
            &[ExpressionValue],
        ) -> Result<ExpressionValue, Fault>
        + 'static,
        signature: Option<&str>,
        description: &str,
    ) {
        self.register_handler(
            name,
            minimum_arguments,
            maximum_arguments,
            Rc::new(handler),
            signature,
            description,
        );
    }

    /// [`Self::register`] with a shared handler.
    ///
    /// # Panics
    /// As [`Self::register`].
    pub fn register_handler(
        &mut self,
        name: &str,
        minimum_arguments: usize,
        maximum_arguments: usize,
        handler: FunctionHandler,
        signature: Option<&str>,
        description: &str,
    ) {
        assert!(!name.trim().is_empty(), "a function needs a name");
        assert!(
            maximum_arguments >= minimum_arguments,
            "argument counts are backwards for {name}"
        );
        let function = ExpressionFunction {
            name: name.to_owned(),
            minimum_arguments,
            maximum_arguments,
            handler,
            signature: signature.map_or_else(|| format!("{name}[...]"), str::to_owned),
            description: description.to_owned(),
        };
        assert!(
            self.functions.try_add(name, function),
            "Expression function '{name}' is already registered."
        );
    }

    /// Registers `alias` as another name for `existing`.
    ///
    /// # Panics
    /// `existing` is not registered or `alias` is taken.
    pub fn alias(&mut self, alias: &str, existing: &str) {
        let function = self
            .functions
            .get(existing)
            .cloned()
            .unwrap_or_else(|| panic!("Expression function '{existing}' is not registered."));
        let signature = function.signature.replace(existing, alias);
        self.register_handler(
            alias,
            function.minimum_arguments,
            function.maximum_arguments,
            function.handler,
            Some(&signature),
            &function.description,
        );
    }

    /// A function by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ExpressionFunction> {
        self.functions.get(name)
    }

    fn resolve(&self, name: &str, offset: usize) -> Result<ExpressionFunction, Fault> {
        self.functions.get(name).cloned().ok_or_else(|| {
            ExpressionError::evaluation_at(format!("Unknown expression method: {name}"), offset)
                .into()
        })
    }
}

/// Whether the client knows an object id.
pub type KnownObject = Rc<dyn Fn(u32) -> bool>;

/// Told about an expression the reader had to repair, with what it did.
pub type RepairObserver = Rc<dyn Fn(&str, &[String])>;

/// Told about a code string whose separate run failed.
pub type RunFailedObserver = Rc<dyn Fn(&str, &ExpressionError)>;

/// One evaluation: the variables, the functions, the instruction budget.
pub struct EvaluationContext<'a> {
    /// The variables.
    pub state: &'a mut ExpressionState,
    /// The functions.
    pub functions: &'a FunctionRegistry,
    remaining_instructions: i64,
    budget_exhausted: bool,
    cancelled: Option<Rc<std::cell::Cell<bool>>>,
    /// Told about an expression the reader had to repair, with what it did.
    pub repairs_observed: Option<RepairObserver>,
    /// Told about a code string whose separate run failed: the code as the
    /// reader gives it back, and the error.
    pub run_failed: Option<RunFailedObserver>,
    /// Whether the client knows an object id, for the reference's argument
    /// check; `None` takes every id as known.
    pub is_known_object: Option<KnownObject>,
}

impl std::fmt::Debug for EvaluationContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvaluationContext")
            .field("remaining_instructions", &self.remaining_instructions)
            .finish_non_exhaustive()
    }
}

/// The default instruction budget of one evaluation.
pub const DEFAULT_INSTRUCTION_BUDGET: i64 = 10_000;

impl<'a> EvaluationContext<'a> {
    /// A context with the default budget.
    #[must_use]
    pub fn new(state: &'a mut ExpressionState, functions: &'a FunctionRegistry) -> Self {
        Self::with_budget(state, functions, DEFAULT_INSTRUCTION_BUDGET)
    }

    /// A context with an instruction budget.
    ///
    /// # Panics
    /// A budget that is not positive.
    #[must_use]
    pub fn with_budget(
        state: &'a mut ExpressionState,
        functions: &'a FunctionRegistry,
        instruction_budget: i64,
    ) -> Self {
        assert!(
            instruction_budget > 0,
            "the instruction budget must be positive"
        );
        Self {
            state,
            functions,
            remaining_instructions: instruction_budget,
            budget_exhausted: false,
            cancelled: None,
            repairs_observed: None,
            run_failed: None,
            is_known_object: None,
        }
    }

    /// Cancels the run when the flag is set.
    #[must_use]
    pub fn with_cancellation(mut self, cancelled: Rc<std::cell::Cell<bool>>) -> Self {
        self.cancelled = Some(cancelled);
        self
    }

    /// The instructions left.
    #[must_use]
    pub fn remaining_instructions(&self) -> i64 {
        self.remaining_instructions
    }

    /// Compiles an expression met while evaluating (a code string handed to
    /// ifthen, exec or a list function), reporting any repair.
    ///
    /// # Errors
    /// The text is not an expression.
    pub fn compile(&self, source: &str) -> Result<ExpressionProgram, ExpressionError> {
        let program = ExpressionProgram::compile(source)?;
        if !program.repairs().is_empty()
            && let Some(observer) = &self.repairs_observed
        {
            observer(source, program.repairs());
        }
        Ok(program)
    }

    /// One instruction.
    ///
    /// # Errors
    /// The run was cancelled, or the budget ran out.
    pub fn step(&mut self, offset: usize) -> Result<(), Fault> {
        if self.cancelled.as_ref().is_some_and(|flag| flag.get()) {
            return Err(ExpressionError::Cancelled.into());
        }
        self.remaining_instructions -= 1;
        if self.remaining_instructions < 0 {
            self.budget_exhausted = true;
            return Err(ExpressionError::evaluation_at(
                "Expression instruction budget exceeded",
                offset,
            )
            .into());
        }
        Ok(())
    }

    /// Runs a code string handed to exec, ifthen or a list function as a
    /// run of its own: it reads the global variables fresh and writes back
    /// the collections it changed. An error ends only this run: it is
    /// reported, and the run answers 0 with `failed` set. Only running out
    /// of the budget, or being cancelled, ends the holding expression.
    ///
    /// # Errors
    /// The budget ran out, or the run was cancelled.
    pub fn run_separately(
        &mut self,
        program: &ExpressionProgram,
    ) -> Result<(ExpressionValue, bool), Fault> {
        self.state.begin_evaluation();
        let outcome = match program.evaluate(self) {
            // The write-back belongs to the run: its failure fails the run.
            Ok(value) => self.state.end_evaluation(true).map(|()| value),
            Err(error) => {
                // Nothing is written back from a run that did not complete.
                let _ = self.state.end_evaluation(false);
                Err(error)
            }
        };
        match outcome {
            Ok(value) => Ok((value, false)),
            Err(error) => {
                if self.budget_exhausted || error == ExpressionError::Cancelled {
                    return Err(error.into());
                }
                if let Some(observer) = &self.run_failed {
                    observer(program.text(), &error);
                }
                Ok((ExpressionValue::zero(), true))
            }
        }
    }

    /// Calls a function: the reference's argument check, the count check,
    /// then the body, its errors worded as the reference words them.
    ///
    /// # Errors
    /// The function is unknown, the arguments are wrong, or the body failed.
    pub fn invoke(
        &mut self,
        name: &str,
        arguments: &[ExpressionValue],
        offset: usize,
    ) -> Result<ExpressionValue, Fault> {
        self.step(offset)?;
        let function = self.functions.resolve(name, offset)?;
        reference_arguments::check(
            name,
            arguments,
            self.is_known_object.as_deref(),
            Some(offset),
        )?;
        if arguments.len() < function.minimum_arguments
            || arguments.len() > function.maximum_arguments
        {
            let expected = if function.minimum_arguments == function.maximum_arguments {
                function.minimum_arguments.to_string()
            } else {
                format!(
                    "{}..{}",
                    function.minimum_arguments,
                    if function.maximum_arguments == usize::MAX {
                        i32::MAX.to_string()
                    } else {
                        function.maximum_arguments.to_string()
                    }
                )
            };
            return Err(ExpressionError::evaluation_at(
                format!(
                    "{} expects {expected} arguments; {} were passed",
                    function.signature,
                    arguments.len()
                ),
                offset,
            )
            .into());
        }
        match (function.handler)(self, arguments) {
            Ok(value) => Ok(value),
            Err(Fault::Expression(ExpressionError::Evaluation(error)))
                if error.is_argument_error || error.raised_in_function =>
            {
                Err(Fault::Expression(ExpressionError::Evaluation(error)))
            }
            Err(Fault::Expression(ExpressionError::Evaluation(error))) => Err(EvaluationError {
                reason: error.reason,
                offset: error.offset,
                raised_in_function: true,
                ..EvaluationError::default()
            }
            .into()),
            Err(Fault::Expression(ExpressionError::Cancelled)) => {
                Err(ExpressionError::Cancelled.into())
            }
            Err(other) => Err(EvaluationError {
                reason: format!("{} failed: {}", function.signature, other.message()),
                offset: Some(offset),
                raised_in_function: true,
                ..EvaluationError::default()
            }
            .into()),
        }
    }
}
