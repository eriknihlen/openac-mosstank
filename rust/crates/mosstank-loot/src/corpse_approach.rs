//! Walking to a corpse the looter cannot reach (`CorpseApproachController`).
//!
//! It is a navigation goal like a waypoint: the shared close-in mover,
//! aimed at wherever the chosen corpse is, re-asked every mover frame. The
//! rule sits one place above the open, so the character arrives first and
//! the open then finds the corpse inside its own reach.
//!
//! Two radii from the profile bound it. Outside the outer one there is no
//! goal (a corpse further than the profile's reach is not walked to), and
//! inside the inner one there is none either, which is what ends the walk:
//! the rule stops being valid, loses the turn, and the mover lets go.
//!
//! MossTank's one exception to the outer radius, with the own-rare walk
//! option on: a corpse holding this character's own rare is walked to out
//! to the rare reach, and a walk to it that stops covering ground or runs
//! out of time is given up (see [`crate::own_rare`]).

use ac_plugin_abi::world::Position;
use mosstank_automation::Automation;
use mosstank_combat::gate::SharedCombatModeGate;
use mosstank_combat::mover::{
    CREEP_DISTANCE_METERS, CreepSettings, NavigationMover, STUCK_SECONDS,
};
use openac_plugin_compat::number;

use crate::controller::LootController;
use crate::host::{LootCorpse, LootHost};

/// How long a walk to this character's own rare corpse may go on before it
/// is given up: long enough to cover the whole rare reach at a walk.
pub const OWN_RARE_WALK_GIVE_UP_SECONDS: f64 = 60.0;

/// What one call of the walk works with: the client, the looter whose pick
/// it walks at (and whose settings bound it), and the creep band's
/// settings once the shared stance gate is bound.
#[allow(missing_debug_implementations)]
pub struct ApproachTurn<'a> {
    /// The client.
    pub host: &'a mut dyn LootHost,
    /// The looter.
    pub loot: &'a mut LootController,
    /// The combat and vital settings the mover's creep band pushes into
    /// magic with; `None` while no gate is bound.
    pub creep: Option<CreepSettings<'a>>,
}

/// The corpse walk.
#[allow(missing_debug_implementations)]
pub struct CorpseApproachController {
    mover: NavigationMover,
    goal_corpse: u32,
    goal_distance: f64,
    goal_position: Position,
    status: String,
    /// The own-rare corpse the give-up clock is timing, or zero.
    own_rare_walk_corpse: u32,
    /// How long the mover has been armed walking to that corpse.
    own_rare_walk_seconds: f64,
}

impl Default for CorpseApproachController {
    fn default() -> Self {
        Self::new()
    }
}

fn format(value: f64, pattern: &str) -> String {
    number::format_f64(value, pattern).expect("a supported format")
}

impl CorpseApproachController {
    /// A walk with no goal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mover: NavigationMover::new(),
            goal_corpse: 0,
            goal_distance: 0.0,
            goal_position: Position::default(),
            status: "No corpse to walk to.".to_owned(),
            own_rare_walk_corpse: 0,
            own_rare_walk_seconds: 0.0,
        }
    }

    /// Shares the stance gate the mover's creep band pushes into magic
    /// through.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.mover.bind_combat_mode_gate(gate);
    }

    /// The walk's status line.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The mover, for the owner's low-stop-distance warning.
    pub fn mover_mut(&mut self) -> &mut NavigationMover {
        &mut self.mover
    }

    /// What the rule appends to its "Running" line: the range to the corpse
    /// and where it is.
    #[must_use]
    pub fn running_detail(&self) -> String {
        if self.goal_corpse == 0 {
            return String::new();
        }
        format!(
            "[targ range {}, targ loc {}, {}, {} ]",
            format(self.goal_distance, "0.###"),
            format(self.goal_position.east_west, "0.######"),
            format(self.goal_position.north_south, "0.######"),
            format(self.goal_position.elevation, "0.######"),
        )
    }

    /// Whether the walk's pick is still outside the creep band.
    pub fn is_outside_creep_distance(&mut self, turn: &mut ApproachTurn<'_>) -> bool {
        let range = turn.loot.settings.corpse_approach_range;
        turn.loot
            .try_select_approach_corpse(turn.host, range)
            .is_some_and(|corpse| f64::from(corpse.distance) >= CREEP_DISTANCE_METERS)
    }

    /// The walk's own turn: it answers the pass and arms the mover.
    pub fn claim_from_rule_pass(&mut self, turn: &mut ApproachTurn<'_>, can_act: bool) -> bool {
        // The pass consumes whatever the mover was owed, so a rule turn and
        // a mover frame never both spend the same time.
        let _ = self.mover.take_pending_seconds();
        let claimed = self.tick(turn, can_act);
        self.mover.arm(claimed);
        claimed
    }

    /// One host frame of the armed mover. The corpse moves nowhere but the
    /// character does, so the bearing and range are asked again on the
    /// mover's own interval rather than once per rule pass.
    pub fn step_armed_mover(&mut self, turn: &mut ApproachTurn<'_>, elapsed_seconds: f64) {
        if self.mover.is_armed()
            && self.goal_corpse != 0
            && self.goal_corpse == self.own_rare_walk_corpse
            && elapsed_seconds.is_finite()
            && elapsed_seconds > 0.0
        {
            self.own_rare_walk_seconds += elapsed_seconds;
        }
        if self.mover.try_take_mover_frame(elapsed_seconds).is_some() {
            let _ = self.tick(turn, true);
        }
    }

    /// Losing the turn lets go of the keys, as the reference's navigate
    /// rule does when the pass goes to somebody else.
    pub fn stop_for_lost_turn(&mut self, automation: &mut dyn Automation) {
        self.mover.stop_for_lost_turn(automation);
        self.goal_corpse = 0;
    }

    fn tick(&mut self, turn: &mut ApproachTurn<'_>, can_act: bool) -> bool {
        let snapshot = turn.host.navigation_snapshot();
        if !can_act {
            return self.decline(turn.host, "Corpse walk paused.".to_owned());
        }
        if !turn.loot.settings.enabled {
            return self.decline(turn.host, "Looting disabled.".to_owned());
        }
        if !snapshot.is_available || snapshot.is_portal_space {
            return self.decline(turn.host, "Waiting for the world.".to_owned());
        }
        let Some(corpse) = Self::resolve_goal(turn) else {
            return self.decline(turn.host, "No corpse to walk to.".to_owned());
        };

        // The outer radius is the profile's reach and the inner one is where
        // the walk ends: both are exclusions, not a band the mover clamps
        // to. The one exception to the outer radius is this character's own
        // rare, behind its option.
        let range = turn.loot.settings.corpse_approach_range;
        let distance = f64::from(corpse.distance);
        let reach = turn.loot.approach_reach_for(turn.host, &corpse, range);
        if distance > reach {
            return self.decline(
                turn.host,
                format!(
                    "{} is outside the corpse approach range ({}m).",
                    corpse.name,
                    format(distance, "0.0")
                ),
            );
        }
        if distance <= turn.loot.settings.corpse_minimum_approach_range {
            return self.decline(turn.host, format!("Standing at {}.", corpse.name));
        }

        let own_rare_walk = turn.loot.is_own_rare_walk_goal(turn.host, &corpse);
        if own_rare_walk && self.own_rare_walk_corpse != corpse.object_id {
            self.own_rare_walk_corpse = corpse.object_id;
            self.own_rare_walk_seconds = 0.0;
        }

        let goal = corpse.position.clone().unwrap_or_default();
        self.goal_corpse = corpse.object_id;
        self.goal_distance = distance;
        self.goal_position = goal.clone();
        self.status = if distance > range {
            format!(
                "Walking to {} ({}m), this character's rare, past the corpse approach range.",
                corpse.name,
                format(distance, "0.0")
            )
        } else {
            format!("Walking to {} ({}m).", corpse.name, format(distance, "0.0"))
        };
        let claimed = self.mover.steer(
            turn.host,
            turn.creep.as_ref(),
            &snapshot.position,
            &goal,
            distance,
        );
        if let Some(status) = self.mover.take_status() {
            self.status = status;
        }
        if own_rare_walk
            && (self.mover.is_stuck()
                || self.own_rare_walk_seconds >= OWN_RARE_WALK_GIVE_UP_SECONDS)
        {
            let reason = if self.mover.is_stuck() {
                format!(
                    "no ground covered for {} seconds",
                    format(STUCK_SECONDS, "0")
                )
            } else {
                format!(
                    "not reached in {} seconds",
                    format(OWN_RARE_WALK_GIVE_UP_SECONDS, "0")
                )
            };
            turn.loot.give_up_own_rare_walk(corpse.object_id, &reason);
            self.own_rare_walk_corpse = 0;
            self.own_rare_walk_seconds = 0.0;
            return self.decline(
                turn.host,
                format!("Gave up walking to {}: {reason}.", corpse.name),
            );
        }
        claimed
    }

    /// No goal this pass: say why, let go of whatever the mover held, and
    /// forget the corpse so the running line does not describe a walk that
    /// is no longer happening.
    fn decline(&mut self, host: &mut dyn LootHost, status: String) -> bool {
        self.mover.stop_movement(host);
        self.goal_corpse = 0;
        self.status = status;
        false
    }

    /// Which corpse, and where it is now. The pick is the looter's, so a
    /// corpse the looter would refuse is never walked to; one the client
    /// cannot place is skipped, as there is nowhere to walk.
    fn resolve_goal(turn: &mut ApproachTurn<'_>) -> Option<LootCorpse> {
        let range = turn.loot.settings.corpse_approach_range;
        turn.loot
            .try_select_approach_corpse(turn.host, range)
            .filter(LootCorpse::has_position)
    }
}
