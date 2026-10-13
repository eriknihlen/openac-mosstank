//! The movement keys a macro holds down on its own (`HeldMotions`): one
//! switch per key, and the one place that turns them into the client's
//! movement intent. The setmotion command and the setmotion[] expression
//! share it, so a key pressed by one and released by the other is the same.

use ac_plugin_abi::navigation::{MovementIntent, NavigationStatus};
use mosstank_dotnet::text::ordinal;

use crate::host::MetaHost;

/// A movement key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldMotion {
    /// Forward.
    Forward,
    /// Backward.
    Backward,
    /// Turn right.
    TurnRight,
    /// Turn left.
    TurnLeft,
    /// Strafe right.
    StrafeRight,
    /// Strafe left.
    StrafeLeft,
    /// The run key let go.
    Walk,
}

impl HeldMotion {
    /// Every key, in the order the usage line lists them.
    pub const ALL: [Self; 7] = [
        Self::Forward,
        Self::Backward,
        Self::TurnRight,
        Self::TurnLeft,
        Self::StrafeRight,
        Self::StrafeLeft,
        Self::Walk,
    ];

    /// The key's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Backward => "Backward",
            Self::TurnRight => "TurnRight",
            Self::TurnLeft => "TurnLeft",
            Self::StrafeRight => "StrafeRight",
            Self::StrafeLeft => "StrafeLeft",
            Self::Walk => "Walk",
        }
    }

    /// A key by its name, ignoring letter case; a number is not a name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let name = text.trim();
        Self::ALL
            .into_iter()
            .find(|motion| ordinal::equals_ignore_case(motion.name(), name))
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The usage line of the setmotion command.
pub const USAGE: &str =
    "/ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>";

/// The motion names, comma separated, as an error lists them.
#[must_use]
pub fn valid_names() -> String {
    HeldMotion::ALL
        .iter()
        .map(|motion| motion.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The keys the macro wants held.
#[derive(Debug, Clone, Default)]
pub struct HeldMotions {
    held: [bool; 7],
}

impl HeldMotions {
    /// Whether the macro asked for this key to be held.
    #[must_use]
    pub fn is_held(&self, motion: HeldMotion) -> bool {
        self.held[motion.index()]
    }

    /// The intent the held keys add up to.
    #[must_use]
    pub fn intent(&self) -> MovementIntent {
        MovementIntent {
            forward: self.is_held(HeldMotion::Forward),
            backward: self.is_held(HeldMotion::Backward),
            strafe_left: self.is_held(HeldMotion::StrafeLeft),
            strafe_right: self.is_held(HeldMotion::StrafeRight),
            turn_left: self.is_held(HeldMotion::TurnLeft),
            turn_right: self.is_held(HeldMotion::TurnRight),
            run: !self.is_held(HeldMotion::Walk),
            jump: false,
        }
    }

    fn any_key_held(&self) -> bool {
        HeldMotion::ALL
            .iter()
            .any(|&motion| motion != HeldMotion::Walk && self.is_held(motion))
    }

    /// Presses or releases one key and hands the client what is held now.
    /// Walk with nothing else held is only remembered for the next key.
    pub fn set(
        &mut self,
        host: &mut dyn MetaHost,
        motion: HeldMotion,
        on: bool,
    ) -> NavigationStatus {
        self.held[motion.index()] = on;
        if motion == HeldMotion::Walk && !self.any_key_held() {
            return NavigationStatus::Accepted;
        }
        self.apply(host)
    }

    /// Releases every key, walk included.
    pub fn clear(&mut self, host: &mut dyn MetaHost) -> NavigationStatus {
        self.held = [false; 7];
        host.clear_movement_intent()
    }

    /// Forgets the held keys without telling the client.
    pub fn reset(&mut self) {
        self.held = [false; 7];
    }

    /// Lets go of the held keys when there are any, and leaves the client's
    /// movement alone when the macro was holding nothing.
    pub fn release_if_held(&mut self, host: &mut dyn MetaHost) {
        if self.any_key_held() {
            self.clear(host);
        } else {
            self.reset();
        }
    }

    fn apply(&self, host: &mut dyn MetaHost) -> NavigationStatus {
        if !self.any_key_held() {
            return host.clear_movement_intent();
        }
        host.set_movement_intent(self.intent())
    }
}
