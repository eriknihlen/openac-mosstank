//! The expression runtime the meta and the commands run expressions in
//! (`MossTankExpressionRuntime`): the core functions and the host's,
//! delayed runs, the experience meter, the quest flags, the salvage
//! staging and the status window, and the variables saved per character
//! and shared per server.
//!
//! The client is handed to every call; the runtime keeps nothing of it but
//! what it learned (the names of objects it read, for printing them).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::world::WorldObject;
use mosstank_dotnet::compat::DotNetRandom;
use mosstank_dotnet::compat::json::{self, JsonValue, system_text_json};
use mosstank_expr::ChangeCounter;
use mosstank_expr::json as value_json;
use mosstank_expr::value::ObjectNames;
use mosstank_expr::{
    CoreOptions, DEFAULT_INSTRUCTION_BUDGET, EvaluationContext, ExpressionClock, ExpressionError,
    ExpressionHost, ExpressionProgram, ExpressionState, ExpressionValue, ExpressionVariableScope,
    Fault, FunctionRegistry, GlobalVariableStore, Globals,
};
use mosstank_formats::meta::MetaProfile;
use mosstank_stores::global_variables::{GlobalVariables, StorageGlobalVariableStore, hash};

use crate::chat;
use crate::experience::ExperienceMeter;
use crate::functions;
use crate::held_motions::HeldMotions;
use crate::host::{Character, MetaHost};
use crate::quests::QuestTracker;
use crate::salvage::SalvageStaging;
use crate::services::MetaServices;
use crate::status_hud::StatusHud;
use crate::view_manager::MetaViewControls;

/// How many repaired texts are remembered as already reported; the record
/// is emptied when it is full.
pub const REPORTED_REPAIR_LIMIT: usize = 256;

/// One capture of the objects the client knows, shared by everything a meta
/// pass evaluates (`ObjectCapture`). A rule's action can change what the
/// client holds, so the capture is dropped after every action, and outside
/// a pass every call captures afresh.
#[derive(Debug, Default)]
pub struct ObjectCapture {
    captured: Option<Rc<Vec<WorldObject>>>,
    pass_depth: u32,
    capture_count: usize,
}

impl ObjectCapture {
    /// How many times the client was asked for a capture.
    #[must_use]
    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    /// The objects: the pass's capture inside a pass, a fresh one outside.
    pub fn objects(&mut self, host: &mut dyn MetaHost) -> Rc<Vec<WorldObject>> {
        if self.pass_depth == 0 {
            return self.capture(host);
        }
        if let Some(captured) = &self.captured {
            return captured.clone();
        }
        let captured = self.capture(host);
        self.captured = Some(captured.clone());
        captured
    }

    /// Opens a pass.
    pub fn begin_pass(&mut self) {
        self.pass_depth += 1;
    }

    /// Closes a pass.
    pub fn end_pass(&mut self) {
        if self.pass_depth > 0 {
            self.pass_depth -= 1;
            if self.pass_depth == 0 {
                self.captured = None;
            }
        }
    }

    /// Drops the pass capture; the next read captures again.
    pub fn invalidate(&mut self) {
        self.captured = None;
    }

    fn capture(&mut self, host: &mut dyn MetaHost) -> Rc<Vec<WorldObject>> {
        self.capture_count += 1;
        Rc::new(host.objects())
    }
}

/// The names of the objects the runtime read, for printing a world object
/// value outside an evaluation (inside one, the client is asked).
#[derive(Debug, Clone, Default)]
pub(crate) struct NameCache(Rc<RefCell<HashMap<u32, String>>>);

impl NameCache {
    pub(crate) fn names(&self) -> ObjectNames {
        let map = self.0.clone();
        Rc::new(move |id| map.borrow().get(&id).cloned())
    }

    pub(crate) fn learn(&self, id: u32, object: Option<&WorldObject>) {
        let mut map = self.0.borrow_mut();
        match object {
            Some(object) => {
                map.insert(id, object.name.clone());
            }
            None => {
                map.remove(&id);
            }
        }
    }

    pub(crate) fn learn_all(&self, objects: &[WorldObject]) {
        let mut map = self.0.borrow_mut();
        map.clear();
        for object in objects {
            map.insert(object.object_id, object.name.clone());
        }
    }
}

#[derive(Debug, Clone)]
struct DelayedExpression {
    id: i32,
    remaining_milliseconds: f64,
    source: String,
}

/// Everything the runtime keeps beside the variables and the functions.
pub(crate) struct Parts {
    pub(crate) experience: ExperienceMeter,
    pub(crate) quests: QuestTracker,
    pub(crate) salvage: SalvageStaging,
    pub(crate) status_hud: StatusHud,
    pub(crate) held_motions: HeldMotions,
    pub(crate) capture: ObjectCapture,
    pub(crate) names: NameCache,
    pub(crate) clock: Rc<dyn ExpressionClock>,
    delayed: Vec<DelayedExpression>,
    next_delay_id: i32,
    reported_repairs: HashSet<String>,
    service_functions: HashSet<String>,
    /// The global variables' store, while they are shared.
    globals: Option<BoundGlobals>,
}

/// The shared global variables of the server the character is on: the
/// store, and the counter the values read back count their changes on.
#[derive(Debug)]
struct BoundGlobals {
    store: StorageGlobalVariableStore,
    counter: ChangeCounter,
}

/// The shared store lent the plugin's storage for one use.
fn lend_globals<'s>(
    globals: &'s Option<BoundGlobals>,
    storage: &'s mut dyn mosstank_stores::storage::PluginStorage,
    names: &'s ObjectNames,
) -> Option<GlobalVariables<'s>> {
    globals.as_ref().map(|bound| GlobalVariables {
        store: &bound.store,
        storage,
        counter: &bound.counter,
        names: Some(names),
    })
}

/// The expression runtime.
pub struct ExpressionRuntime {
    state: ExpressionState,
    functions: FunctionRegistry,
    pub(crate) parts: Parts,
    identity: String,
    persistent_json: Option<String>,
    saved_writes: Option<u64>,
    saved_collection_changes: Option<u64>,
    saved_document: Vec<(String, JsonValue)>,
    disposed: bool,
}

impl std::fmt::Debug for ExpressionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionRuntime")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// A function the runtime's owner adds that needs more than the runtime
/// holds (the meta engine's own state): the runtime hands it every call of
/// a name the owner registered with
/// [`ExpressionRuntime::register_extension_function`].
pub trait FunctionExtension {
    /// Runs `function`; `None` for a name this extension does not have.
    fn call(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        function: &str,
        context: &mut EvaluationContext<'_>,
        arguments: &[ExpressionValue],
    ) -> Option<Result<ExpressionValue, Fault>>;

    /// A line an expression submitted loaded a meta profile.
    fn profile_loaded(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        profile: MetaProfile,
    ) {
        let _ = (host, services, profile);
    }
}

/// Who an evaluation can reach beyond the runtime itself.
pub struct Reach<'r> {
    /// The rest of the plugin.
    pub services: &'r mut dyn MetaServices,
    /// The views metas created.
    pub views: Option<&'r mut dyn MetaViewControls>,
    /// The owner's own functions.
    pub extension: Option<&'r mut dyn FunctionExtension>,
}

impl std::fmt::Debug for Reach<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reach")
    }
}

impl<'r> Reach<'r> {
    /// Only the services.
    pub fn services(services: &'r mut dyn MetaServices) -> Self {
        Self {
            services,
            views: None,
            extension: None,
        }
    }
}

/// The client and the rest, for the length of one evaluation: what the
/// host's functions run against.
pub(crate) struct Call<'c> {
    pub(crate) host: &'c mut dyn MetaHost,
    pub(crate) services: &'c mut dyn MetaServices,
    pub(crate) views: Option<&'c mut dyn MetaViewControls>,
    pub(crate) extension: Option<&'c mut dyn FunctionExtension>,
    pub(crate) parts: &'c mut Parts,
}

impl Call<'_> {
    /// Runs `action` on the shared global variables, over the storage of
    /// this call.
    fn with_globals<T>(
        &mut self,
        action: impl FnOnce(&mut GlobalVariables<'_>) -> Result<T, ExpressionError>,
    ) -> Result<T, ExpressionError> {
        let names = self.parts.names.names();
        let storage = self.host.storage();
        let mut lent = lend_globals(&self.parts.globals, storage, &names).ok_or_else(|| {
            ExpressionError::evaluation(
                "The global variables could not be read or written: they are not shared",
            )
        })?;
        action(&mut lent)
    }

    /// One object, its name learned.
    pub(crate) fn object(&mut self, id: u32) -> Option<WorldObject> {
        let object = self.host.object(id);
        self.parts.names.learn(id, object.as_ref());
        object
    }

    /// Every object, from the pass's capture inside a pass.
    pub(crate) fn objects(&mut self) -> Rc<Vec<WorldObject>> {
        let objects = self.parts.capture.objects(self.host);
        self.parts.names.learn_all(&objects);
        objects
    }

    /// A world object value that prints with its current name.
    pub(crate) fn object_value(&self, id: u32) -> ExpressionValue {
        ExpressionValue::world_object(id, Some(self.parts.names.names()))
    }

    fn report_repairs(&mut self, source: &str, repairs: &[String]) {
        report_repairs(self.host, &mut self.parts.reported_repairs, source, repairs);
    }
}

fn report_repairs(
    host: &mut dyn MetaHost,
    reported: &mut HashSet<String>,
    source: &str,
    repairs: &[String],
) {
    if reported.contains(source) {
        return;
    }
    if reported.len() >= REPORTED_REPAIR_LIMIT {
        reported.clear();
    }
    reported.insert(source.to_owned());
    host.log(
        LogLevel::Warn,
        &format!(
            "Expression read the way UtilityBelt reads it: {}. Expression: {source}",
            repairs.join("; ")
        ),
    );
}

impl ExpressionHost for Call<'_> {
    fn call(
        &mut self,
        function: &str,
        context: &mut EvaluationContext<'_>,
        arguments: &[ExpressionValue],
    ) -> Result<ExpressionValue, Fault> {
        if let Some(result) = functions::call(self, function, context, arguments) {
            return result;
        }
        if let Some(extension) = self.extension.as_deref_mut()
            && let Some(result) =
                extension.call(self.host, self.services, function, context, arguments)
        {
            return result;
        }
        if self.parts.service_functions.contains(function)
            && let Some(result) = self
                .services
                .call_function(self.host, function, context, arguments)
        {
            return result;
        }
        Err(ExpressionError::evaluation(format!("Unknown expression method: {function}")).into())
    }

    fn knows_object(&mut self, object_id: u32) -> bool {
        self.object(object_id).is_some()
    }

    fn object_name(&mut self, object_id: u32) -> Option<Option<String>> {
        Some(self.object(object_id).map(|object| object.name))
    }

    fn write_to_chat(&mut self, line: &str) {
        chat::ub_post(self.host, &chat::ub_line(line));
    }

    fn global_variables(&mut self) -> Option<&mut dyn GlobalVariableStore> {
        if self.parts.globals.is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn repairs_observed(&mut self, source: &str, repairs: &[String]) {
        self.report_repairs(source, repairs);
    }

    fn run_failed(&mut self, text: &str, error: &ExpressionError) {
        // The reference reports a failed code string as two UtilityBelt
        // error lines: the code as its parser gives it back, then the
        // reason. With the debug setting on the reason is the whole
        // exception; off, an error raised inside a function reaches it
        // wrapped by the reflection call.
        const WRAPPER: &str = "Exception has been thrown by the target of an invocation.";
        let reason = if self.services.debug() {
            format!(
                "AcDream.Plugins.MossTank.Expressions.ExpressionEvaluationException: {}",
                error.message()
            )
        } else {
            match error {
                ExpressionError::Evaluation(evaluation) if !evaluation.raised_in_function => {
                    evaluation.reason.clone()
                }
                _ => WRAPPER.to_owned(),
            }
        };
        for line in [format!("Error running string expression: {text}"), reason] {
            let message = chat::ub_error(&line);
            chat::ub_post(self.host, &message);
            self.host.log(LogLevel::Info, &message);
        }
    }
}

/// The shared global variables as a run reaches them: through the storage
/// of the call the run is part of.
impl GlobalVariableStore for Call<'_> {
    fn try_get(&mut self, name: &str) -> Result<Option<ExpressionValue>, ExpressionError> {
        self.with_globals(|globals| globals.try_get(name))
    }

    fn contains(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.with_globals(|globals| globals.contains(name))
    }

    fn set(&mut self, name: &str, value: &ExpressionValue) -> Result<(), ExpressionError> {
        self.with_globals(|globals| globals.set(name, value))
    }

    fn remove(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.with_globals(|globals| globals.remove(name))
    }

    fn clear(&mut self) -> Result<(), ExpressionError> {
        self.with_globals(|globals| globals.clear())
    }

    fn capture(&mut self) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        self.with_globals(|globals| globals.capture())
    }
}

impl ExpressionRuntime {
    /// A runtime bound to the character the client is on now. `seed` seeds
    /// `randint` (`None`: from the clock); `clock` is the time the date and
    /// stopwatch functions read.
    pub fn new(host: &mut dyn MetaHost, seed: Option<i32>, clock: Rc<dyn ExpressionClock>) -> Self {
        let seed = seed.unwrap_or_else(|| {
            #[allow(clippy::cast_possible_truncation)]
            let low = clock.monotonic_ms() as i32;
            low
        });
        let options = CoreOptions {
            random: Rc::new(RefCell::new(DotNetRandom::new(seed))),
            is_known_object: None,
            write_to_chat: None,
            clock: clock.clone(),
        };
        let mut functions = mosstank_expr::create_default(&options);
        functions::register(&mut functions);
        let parts = Parts {
            experience: ExperienceMeter::default(),
            quests: QuestTracker::new(host),
            salvage: SalvageStaging::default(),
            status_hud: StatusHud::default(),
            held_motions: HeldMotions::default(),
            capture: ObjectCapture::default(),
            names: NameCache::default(),
            clock,
            delayed: Vec::new(),
            next_delay_id: 1,
            reported_repairs: HashSet::new(),
            service_functions: HashSet::new(),
            globals: None,
        };
        let mut runtime = Self {
            state: ExpressionState::new(),
            functions,
            parts,
            identity: String::new(),
            persistent_json: None,
            saved_writes: None,
            saved_collection_changes: None,
            saved_document: Vec::new(),
            disposed: false,
        };
        runtime.bind_identity(host, true);
        runtime
    }

    /// The variables.
    pub fn state(&mut self) -> &mut ExpressionState {
        &mut self.state
    }

    /// The functions, in registration order.
    #[must_use]
    pub fn functions(&self) -> &FunctionRegistry {
        &self.functions
    }

    /// The movement keys the setmotion command and function share.
    pub fn held_motions(&mut self) -> &mut HeldMotions {
        &mut self.parts.held_motions
    }

    /// The object capture a meta pass shares.
    pub fn capture(&mut self) -> &mut ObjectCapture {
        &mut self.parts.capture
    }

    /// The status window's rows.
    #[must_use]
    pub fn status_hud(&self) -> &StatusHud {
        &self.parts.status_hud
    }

    /// How many delayed runs are waiting.
    #[must_use]
    pub fn pending_execution_count(&self) -> usize {
        self.parts.delayed.len()
    }

    /// Registers a function the runtime's owner runs
    /// ([`FunctionExtension`]).
    ///
    /// # Panics
    /// The name is taken.
    pub fn register_extension_function(
        &mut self,
        name: &str,
        minimum_arguments: usize,
        maximum_arguments: usize,
        signature: &str,
    ) {
        let function = name.to_owned();
        self.functions.register(
            name,
            minimum_arguments,
            maximum_arguments,
            move |context, args| context.call_host(&function, args),
            Some(signature),
            "",
        );
    }

    /// Registers a function the plugin's services run
    /// ([`MetaServices::call_function`]).
    ///
    /// # Panics
    /// The name is taken.
    pub fn register_service_function(
        &mut self,
        name: &str,
        minimum_arguments: usize,
        maximum_arguments: usize,
        signature: &str,
    ) {
        self.parts.service_functions.insert(name.to_owned());
        self.register_extension_function(name, minimum_arguments, maximum_arguments, signature);
    }

    /// Runs an expression with the default budget.
    ///
    /// # Errors
    /// The text is not an expression, or the run failed.
    pub fn evaluate(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        source: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        self.evaluate_with(
            host,
            Reach::services(services),
            source,
            DEFAULT_INSTRUCTION_BUDGET,
        )
    }

    /// Runs an expression reaching `reach`, with a budget.
    ///
    /// # Errors
    /// The text is not an expression, or the run failed.
    pub fn evaluate_with(
        &mut self,
        host: &mut dyn MetaHost,
        reach: Reach<'_>,
        source: &str,
        instruction_budget: i64,
    ) -> Result<ExpressionValue, ExpressionError> {
        if self.disposed {
            return Err(ExpressionError::evaluation(
                "Cannot access a disposed object.\r\nObject name: 'AcDream.Plugins.MossTank.Expressions.MossTankExpressionRuntime'.",
            ));
        }
        self.bind_identity(host, false);
        let program = ExpressionProgram::compile(source)?;
        if !program.repairs().is_empty() {
            report_repairs(
                host,
                &mut self.parts.reported_repairs,
                source,
                program.repairs(),
            );
        }
        let Reach {
            services,
            views,
            extension,
        } = reach;
        self.state.begin_evaluation();
        let result = {
            let mut call = Call {
                host: &mut *host,
                services,
                views: views.map(|views| -> &mut dyn MetaViewControls { views }),
                extension: extension.map(|extension| -> &mut dyn FunctionExtension { extension }),
                parts: &mut self.parts,
            };
            let mut context = EvaluationContext::with_budget(
                &mut self.state,
                &self.functions,
                instruction_budget.max(1),
            )
            .with_host(&mut call);
            program.evaluate(&mut context)
        };
        // The write-back of the shared lists the expression changed belongs
        // to it: its failure fails the expression, before anything is saved.
        let names = self.parts.names.names();
        let mut globals = lend_globals(&self.parts.globals, host.storage(), &names);
        self.state.end_evaluation(
            result.is_ok(),
            globals
                .as_mut()
                .map(|globals| -> &mut dyn GlobalVariableStore { globals }),
        )?;
        // What the expression set before an error stands, and the
        // reference saves a persistent variable as it is set, so the save
        // happens whether or not the expression completed.
        self.flush_variables(host);
        result
    }

    /// What a value prints as, a world object with the name the client
    /// knows it by now.
    pub fn display(&mut self, host: &mut dyn MetaHost, value: &ExpressionValue) -> String {
        let names = self.parts.names.clone();
        value.to_display_string_with(&mut |id| {
            let object = host.object(id);
            names.learn(id, object.as_ref());
            Some(object.map(|object| object.name))
        })
    }

    /// Every variable of a scope, the shared global variables read through
    /// the plugin's storage.
    ///
    /// # Errors
    /// A shared global variable cannot be read.
    pub fn capture_variables(
        &mut self,
        host: &mut dyn MetaHost,
        scope: ExpressionVariableScope,
    ) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        let names = self.parts.names.names();
        let mut globals = lend_globals(&self.parts.globals, host.storage(), &names);
        let globals: Globals<'_> = globals
            .as_mut()
            .map(|globals| -> &mut dyn GlobalVariableStore { globals });
        self.state.capture_with(scope, globals)
    }

    /// The client asks the runtime's chat filter about a line it is about
    /// to show; true drops it (a quest line of the runtime's own refresh).
    pub fn filter_chat(&mut self, text: &str) -> bool {
        self.parts.quests.filter(text)
    }

    /// One tick: the meter, the quests and the delayed runs.
    pub fn on_tick(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        elapsed_seconds: f64,
    ) {
        self.on_tick_with(host, services, None, None, elapsed_seconds);
    }

    /// [`Self::on_tick`], the delayed runs reaching the views and the
    /// owner's own functions.
    pub fn on_tick_with(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        mut views: Option<&mut dyn MetaViewControls>,
        mut extension: Option<&mut dyn FunctionExtension>,
        elapsed_seconds: f64,
    ) {
        if self.disposed {
            return;
        }
        self.bind_identity(host, false);
        if elapsed_seconds < 0.0 || !elapsed_seconds.is_finite() {
            return;
        }
        self.parts.experience.on_tick(host, elapsed_seconds);
        self.parts.quests.on_tick(host, elapsed_seconds);
        if self.parts.delayed.is_empty() {
            return;
        }
        let elapsed_milliseconds = elapsed_seconds * 1000.0;
        for delayed in &mut self.parts.delayed {
            delayed.remaining_milliseconds -= elapsed_milliseconds;
        }
        let mut ready: Vec<DelayedExpression> = self
            .parts
            .delayed
            .iter()
            .filter(|delayed| delayed.remaining_milliseconds <= 0.0)
            .cloned()
            .collect();
        if ready.is_empty() {
            return;
        }
        ready.sort_by_key(|delayed| delayed.id);
        self.parts
            .delayed
            .retain(|delayed| delayed.remaining_milliseconds > 0.0);
        for delayed in ready {
            let reach = Reach {
                services: &mut *services,
                views: views
                    .as_deref_mut()
                    .map(|views| -> &mut dyn MetaViewControls { views }),
                extension: extension
                    .as_deref_mut()
                    .map(|extension| -> &mut dyn FunctionExtension { extension }),
            };
            if let Err(error) =
                self.evaluate_with(host, reach, &delayed.source, DEFAULT_INSTRUCTION_BUDGET)
            {
                host.log(
                    LogLevel::Error,
                    &format!(
                        "Delayed expression {} failed: {}",
                        delayed.id,
                        error.message()
                    ),
                );
            }
        }
    }

    /// Forgets the session: its variables and delayed runs.
    pub fn clear_session(&mut self) {
        let _ = self.state.clear_all(ExpressionVariableScope::Session);
        self.parts.delayed.clear();
    }

    /// Takes the status window down.
    pub fn destroy_auxiliary_views(&mut self, host: &mut dyn MetaHost) {
        self.parts.status_hud.destroy(host);
    }

    /// Asks the server for the quest list again; a refresh under way starts
    /// over.
    pub fn refresh_quests(&mut self, host: &mut dyn MetaHost) {
        self.parts.quests.refresh(host, true);
    }

    /// Saves, lets go of the client's resources and stops.
    pub fn dispose(&mut self, host: &mut dyn MetaHost) {
        if self.disposed {
            return;
        }
        self.flush_variables(host);
        self.parts.delayed.clear();
        self.parts.status_hud.destroy(host);
        self.unbind_global_store();
        self.parts.quests.dispose(host);
        self.disposed = true;
    }

    fn bind_identity(&mut self, host: &mut dyn MetaHost, force: bool) {
        let character = Character::of(host);
        let identity = format!(
            "{}\n{}\n{}",
            character.world, character.account, character.name
        );
        if !force && identity == self.identity {
            return;
        }
        if !self.identity.is_empty() {
            self.flush_variables(host);
        }
        self.identity = identity;
        let identity = self.identity.clone();
        self.parts.quests.bind_identity(host, &identity);
        self.parts.salvage.clear();
        let _ = self.state.clear_all(ExpressionVariableScope::Session);
        self.parts.delayed.clear();
        self.parts.experience.reset();
        self.persistent_json = self.load_persistent(host);
        self.mark_persistent_saved();
        self.bind_global_store(host, &character);
    }

    /// Global variables belong to the server, not to one account: every
    /// client on the same server reads and writes one shared set.
    fn bind_global_store(&mut self, host: &mut dyn MetaHost, character: &Character) {
        self.unbind_global_store();
        let _ = self.state.clear_all(ExpressionVariableScope::Global);
        if !host.storage().is_available() {
            return;
        }
        self.parts.globals = Some(BoundGlobals {
            store: StorageGlobalVariableStore::new(&character.world),
            counter: self.state.changes().clone(),
        });
        self.state.share_globals(true);
        self.import_account_globals(host, character);
    }

    /// The global scope back in the state's own table.
    fn unbind_global_store(&mut self) {
        self.state.share_globals(false);
        self.parts.globals = None;
    }

    /// Earlier versions kept the global variables in one file per server
    /// and account. Their values join the shared set without replacing a
    /// value it already has, and the old file goes.
    fn import_account_globals(&mut self, host: &mut dyn MetaHost, character: &Character) {
        let key = storage_key(
            "global",
            &format!("{}\n{}", character.world, character.account),
        );
        let outcome = (|| -> Result<(), String> {
            let Some(text) = host.storage().read_text(&key)? else {
                return Ok(());
            };
            if !text.trim().is_empty() {
                let document = json::parse(&text).map_err(|error| error.to_string())?;
                if let JsonValue::Object(members) = document {
                    let names = self.parts.names.names();
                    for (name, value) in members {
                        let mut store = lend_globals(&self.parts.globals, host.storage(), &names)
                            .ok_or("the global variables are not shared")?;
                        if !store.contains(&name).map_err(|error| error.message())? {
                            let value =
                                value_json::restore(&value, Some(&names), self.state.changes())?;
                            store.set(&name, &value).map_err(|error| error.message())?;
                        }
                    }
                }
            }
            host.storage().delete(&key)?;
            Ok(())
        })();
        if let Err(error) = outcome {
            host.log(
                LogLevel::Error,
                &format!("Unable to move the old global expression variables: {error}"),
            );
        }
    }

    fn load_persistent(&mut self, host: &mut dyn MetaHost) -> Option<String> {
        let _ = self.state.clear_all(ExpressionVariableScope::Persistent);
        self.saved_document.clear();
        if !host.storage().is_available() || self.identity.is_empty() {
            return None;
        }
        let key = storage_key("persistent", &self.identity);
        let outcome = (|| -> Result<Option<String>, String> {
            let Some(text) = host.storage().read_text(&key)? else {
                return Ok(None);
            };
            if text.trim().is_empty() {
                return Ok(None);
            }
            let document = json::parse(&text).map_err(|error| error.to_string())?;
            match document {
                JsonValue::Null => {}
                JsonValue::Object(members) => {
                    let mut values = Vec::with_capacity(members.len());
                    for (name, value) in &members {
                        values.push((
                            name.clone(),
                            value_json::restore(
                                value,
                                Some(&self.parts.names.names()),
                                self.state.changes(),
                            )?,
                        ));
                    }
                    self.saved_document = members;
                    self.state
                        .replace(ExpressionVariableScope::Persistent, values)
                        .map_err(|error| error.message())?;
                }
                _ => {
                    return Err(
                        "The JSON value could not be converted to System.Collections.Generic.Dictionary`2[System.String,ExpressionValueJson+StoredValue]."
                            .to_owned(),
                    );
                }
            }
            Ok(Some(text))
        })();
        match outcome {
            Ok(text) => text,
            Err(error) => {
                host.log(
                    LogLevel::Error,
                    &format!("Unable to load Persistent expression variables: {error}"),
                );
                None
            }
        }
    }

    /// Saves the persistent variables when something may have changed since
    /// the last save.
    fn flush_variables(&mut self, host: &mut dyn MetaHost) {
        if !host.storage().is_available() || self.identity.is_empty() {
            return;
        }
        let writes = self.state.persistent_writes();
        let changes = self.state.changes().count();
        if Some(writes) == self.saved_writes
            && (Some(changes) == self.saved_collection_changes
                || !self
                    .state
                    .holds_collection(ExpressionVariableScope::Persistent))
        {
            return;
        }
        if let Some(json) = self.flush_persistent(host) {
            self.persistent_json = Some(json);
            self.saved_writes = Some(writes);
            self.saved_collection_changes = Some(changes);
        }
    }

    fn mark_persistent_saved(&mut self) {
        self.saved_writes = Some(self.state.persistent_writes());
        self.saved_collection_changes = Some(self.state.changes().count());
    }

    fn flush_persistent(&mut self, host: &mut dyn MetaHost) -> Option<String> {
        let captured = match self.state.capture(ExpressionVariableScope::Persistent) {
            Ok(captured) => captured,
            Err(error) => {
                host.log(
                    LogLevel::Error,
                    &format!(
                        "Unable to save Persistent expression variables: {}",
                        error.message()
                    ),
                );
                return None;
            }
        };
        let mut document: Vec<(String, JsonValue)> = Vec::with_capacity(captured.len());
        for (name, value) in captured {
            match value_json::store(&value) {
                Ok(stored) => document.push((name, stored)),
                Err(error) => {
                    // A list changed in place can come to hold what cannot be
                    // saved: that one variable keeps the form it was last
                    // saved in, and the others are saved all the same.
                    if let Some((_, saved)) =
                        self.saved_document.iter().find(|(saved, _)| *saved == name)
                    {
                        document.push((name.clone(), saved.clone()));
                    }
                    host.log(
                        LogLevel::Error,
                        &format!(
                            "Unable to save Persistent expression variable {name}: {}",
                            error.message()
                        ),
                    );
                }
            }
        }
        let json = match system_text_json::to_string(
            &JsonValue::Object(document.clone()),
            &value_json::OPTIONS,
        ) {
            Ok(json) => json,
            Err(error) => {
                host.log(
                    LogLevel::Error,
                    &format!("Unable to save Persistent expression variables: {error}"),
                );
                return None;
            }
        };
        if self.persistent_json.as_deref() != Some(json.as_str()) {
            let key = storage_key("persistent", &self.identity);
            if let Err(error) = host.storage().write_text(&key, &json) {
                host.log(
                    LogLevel::Error,
                    &format!("Unable to save Persistent expression variables: {error}"),
                );
                return None;
            }
        }
        self.saved_document = document;
        Some(json)
    }
}

fn next_delay_id(parts: &mut Parts) -> Result<i32, Fault> {
    let initial = parts.next_delay_id;
    loop {
        let candidate = parts.next_delay_id;
        parts.next_delay_id = parts.next_delay_id.wrapping_add(1);
        if parts.next_delay_id <= 0 {
            parts.next_delay_id = 1;
        }
        if parts.delayed.iter().all(|delayed| delayed.id != candidate) {
            return Ok(candidate);
        }
        if parts.next_delay_id == initial {
            return Err(ExpressionError::evaluation("No delayed-expression ids remain").into());
        }
    }
}

/// The storage key of a scope's file: `expressions/<scope>/` and twelve
/// bytes of SHA-256 of its owner, in lower-case hex.
#[must_use]
pub fn storage_key(scope: &str, owner: &str) -> String {
    format!("expressions/{scope}/{}.json", hash(owner))
}

impl Parts {
    pub(crate) fn delayed_remove(&mut self, id: i32) -> bool {
        let before = self.delayed.len();
        self.delayed.retain(|delayed| delayed.id != id);
        before != self.delayed.len()
    }

    pub(crate) fn delay(&mut self, milliseconds: f64, source: &str) -> Result<i32, Fault> {
        let id = next_delay_id(self)?;
        self.delayed.push(DelayedExpression {
            id,
            remaining_milliseconds: milliseconds,
            source: source.to_owned(),
        });
        Ok(id)
    }
}
