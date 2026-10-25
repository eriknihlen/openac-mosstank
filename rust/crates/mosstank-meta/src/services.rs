//! What the meta engine and its functions ask of the rest of the plugin
//! (the C#'s `MetaServices` and `ExpressionHostPolicy`): the route, the
//! buffs, the options, the macro's caster, the profile's skill margins and
//! the functions the plugin adds. Every method has the C#'s default answer,
//! so a plugin (or a test) overrides only what it has.

use mosstank_expr::{EvaluationContext, ExpressionValue, Fault};
use mosstank_formats::meta::MetaProfile;
use mosstank_formats::route::NavigationSettings;

use crate::engine::MetaEngine;
use crate::host::MetaHost;

/// What a peer function needs the other clients to share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerNeed {
    /// Their state (`netclients`).
    ClientState,
    /// Their casts (`netcasts`).
    Casts,
}

/// The rest of the plugin, as the meta sees it.
pub trait MetaServices {
    /// Submits a chat line for a meta (a chat command action, a chat
    /// expression's result, `chatbox[]`): the plugin takes its own verbs,
    /// the way the client runs a command before the submit returns, and
    /// hands the rest to the client. A line that loaded a meta profile
    /// answers it, and the engine takes it at once; a verb that needs the
    /// engine itself waits for [`MetaServices::run_pending_lines`].
    fn submit_chat(&mut self, host: &mut dyn MetaHost, line: &str) -> Option<MetaProfile> {
        host.chat_submit(line);
        None
    }

    /// Runs the meta's own verbs that waited for the engine (register
    /// META-9): the engine lends itself as soon as the action or expression
    /// that submitted them is over, so their work is done before the meta
    /// looks at its next rule, as the reference's submit runs them before
    /// it returns.
    fn run_pending_lines(&mut self, engine: &mut MetaEngine, host: &mut dyn MetaHost) {
        let _ = (engine, host);
    }

    /// Whether the loaded route has nothing left to walk.
    fn is_navigation_route_empty(&mut self, host: &mut dyn MetaHost) -> bool {
        let _ = host;
        true
    }

    /// Whether a buff is due.
    fn needs_buff(&mut self, host: &mut dyn MetaHost) -> bool {
        let _ = host;
        false
    }

    /// How far the nearest route point is, metres.
    fn distance_from_any_route_point(&mut self, host: &mut dyn MetaHost) -> f64 {
        let _ = host;
        f64::INFINITY
    }

    /// How many monsters of a priority are within `distance` metres.
    fn count_monsters_by_priority(
        &mut self,
        host: &mut dyn MetaHost,
        priority: i32,
        distance: f64,
    ) -> i32 {
        let _ = (host, priority, distance);
        0
    }

    /// Loads a route a meta carries.
    fn load_embedded_navigation_route(
        &mut self,
        host: &mut dyn MetaHost,
        route: Option<&NavigationSettings>,
        name: &str,
    ) {
        let _ = (host, route, name);
    }

    /// An option's value (`GetOpt`, `vtgetsetting`).
    fn get_option(&mut self, host: &mut dyn MetaHost, name: &str) -> ExpressionValue {
        let _ = (host, name);
        ExpressionValue::zero()
    }

    /// Sets an option; false when there is no such option.
    ///
    /// # Errors
    /// A value of the wrong type for the option.
    fn set_option(
        &mut self,
        host: &mut dyn MetaHost,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        let _ = (host, name, value);
        Ok(false)
    }

    /// The settings page's switch for the event handlers.
    fn game_event_handlers_enabled(&mut self) -> bool {
        true
    }

    /// Told when the meta changes state: the line the C# announces
    /// ("Meta state: A -> B.") and the new state.
    fn state_changed(&mut self, host: &mut dyn MetaHost, message: &str, state: &str) {
        let _ = (host, message, state);
    }

    /// `vtsetmetastate[]` moved the meta to `state`: the fight's copy of
    /// the state is set at once (the C# function sets
    /// `_combatSettings.MetaState` itself).
    fn meta_state_set(&mut self, state: &str) {
        let _ = state;
    }

    /// A peer function needs the other clients to share something.
    fn require_peers(&mut self, host: &mut dyn MetaHost, need: PeerNeed) {
        let _ = (host, need);
    }

    /// Skill required above a spell's difficulty before it counts as
    /// castable: the hunting margin, or the buffing one.
    fn skill_margin(&mut self, hunting: bool) -> i32 {
        let _ = hunting;
        0
    }

    /// Hands an expression's cast to the macro's own caster; false when
    /// there is none and the cast goes straight to the client.
    fn begin_cast(&mut self, host: &mut dyn MetaHost, spell_id: u32, target: Option<u32>) -> bool {
        let _ = (host, spell_id, target);
        false
    }

    /// Whether the UtilityBelt debug setting is on.
    fn debug(&mut self) -> bool {
        false
    }

    /// Runs a function the plugin registered with
    /// [`crate::runtime::ExpressionRuntime::register_service_function`];
    /// `None` for a name it does not have.
    fn call_function(
        &mut self,
        host: &mut dyn MetaHost,
        function: &str,
        context: &mut EvaluationContext<'_>,
        arguments: &[ExpressionValue],
    ) -> Option<Result<ExpressionValue, Fault>> {
        let _ = (host, function, context, arguments);
        None
    }
}

/// The C#'s default services: nothing to say.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoServices;

impl MetaServices for NoServices {}
