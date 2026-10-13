//! The expression runtime as the UB tab reaches it ([`UbExpressions`]): run
//! an expression, print a value, read and write the variables, the held
//! movement keys and the quest list. The plugin answers it with the meta
//! engine's runtime; [`RuntimeExpressions`] answers it with a runtime and
//! the plugin's services.

use mosstank_expr::{ExpressionError, ExpressionState, ExpressionValue};
use mosstank_meta::held_motions::HeldMotions;
use mosstank_meta::host::MetaHost;
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::services::MetaServices;

/// The expression runtime.
pub trait UbExpressions {
    /// Runs an expression.
    ///
    /// # Errors
    /// The text is not an expression, or the run failed.
    fn evaluate(
        &mut self,
        host: &mut dyn MetaHost,
        source: &str,
    ) -> Result<ExpressionValue, ExpressionError>;
    /// What a value prints as.
    fn display(&mut self, host: &mut dyn MetaHost, value: &ExpressionValue) -> String;
    /// The variables.
    fn state(&mut self) -> &mut ExpressionState;
    /// The movement keys `setmotion` holds.
    fn held_motions(&mut self) -> &mut HeldMotions;
    /// Asks the server for the quest list again.
    fn refresh_quests(&mut self, host: &mut dyn MetaHost);
}

/// A runtime with the plugin's services.
pub struct RuntimeExpressions<'a> {
    /// The runtime.
    pub runtime: &'a mut ExpressionRuntime,
    /// The rest of the plugin, as the runtime's functions reach it.
    pub services: &'a mut dyn MetaServices,
}

impl std::fmt::Debug for RuntimeExpressions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeExpressions")
            .field("runtime", &self.runtime)
            .finish_non_exhaustive()
    }
}

impl UbExpressions for RuntimeExpressions<'_> {
    fn evaluate(
        &mut self,
        host: &mut dyn MetaHost,
        source: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        self.runtime.evaluate(host, self.services, source)
    }

    fn display(&mut self, host: &mut dyn MetaHost, value: &ExpressionValue) -> String {
        self.runtime.display(host, value)
    }

    fn state(&mut self) -> &mut ExpressionState {
        self.runtime.state()
    }

    fn held_motions(&mut self) -> &mut HeldMotions {
        self.runtime.held_motions()
    }

    fn refresh_quests(&mut self, host: &mut dyn MetaHost) {
        self.runtime.refresh_quests(host);
    }
}
