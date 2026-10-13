//! `/ub xp` (`ExperienceSpendController`): spends banked experience by the
//! weight policy, one request per interval, stopping on any refusal. It is
//! not a macro rule and takes no lock: it refuses to run at all while the
//! macro is enabled, and says so.

use ac_plugin_abi::character::{AdvancementStatus, Training};
use mosstank_data::experience::{
    self, DEFAULT_MAX_STEPS, ExperiencePlan, ExperienceStep, ExperienceTarget,
    ExperienceTargetKind, ExperienceTargetState, TARGETS, policy,
};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_settings::ub::UbSettingValue;
use openac_plugin_compat::number;

use crate::chat;
use crate::commands::UbCall;

/// The character's own 64-bit property holding the experience not yet
/// spent. Row 6 beside it is unspent luminance, which this must not read.
const AVAILABLE_EXPERIENCE_PROPERTY: u32 = 2;

/// The rows the spender reads, live (`ExperienceSpendSettings`).
const STOP_BEFORE_MAX: &str = "AutoXp.StopBeforeMax";
const TRIES_TIME: &str = "AutoXp.TriesTime";
const MAX_XP_CHUNK: &str = "AutoXp.MaxXpChunk";
const POLICY: &str = "AutoXp.Policy";

/// The experience spender.
#[derive(Debug, Clone)]
pub struct ExperienceSpendController {
    is_running: bool,
    status: String,
    plan: Vec<ExperienceStep>,
    index: usize,
    since_spend: f64,
}

impl Default for ExperienceSpendController {
    fn default() -> Self {
        Self::new()
    }
}

impl ExperienceSpendController {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_running: false,
            status: "Experience spending idle.".to_owned(),
            plan: Vec::new(),
            index: 0,
            since_spend: 0.0,
        }
    }

    /// Whether a run is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// What it last did, in words.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// `/ub xp [level|slow|test|export|import <Target=Weight;...>]`: no verb
    /// lists the weights in use; `level` spends in chunks and `slow` one rank
    /// at a time, either halting a run already going; `test` shows the plan;
    /// `export` writes the policy in the form `import` reads. The lines to
    /// say back, each under the tag; `None` when the line does not parse.
    pub fn command(&mut self, call: &mut UbCall<'_>, arguments: &str) -> Option<Vec<String>> {
        self.command_lines(call, arguments).map(|lines| {
            lines
                .into_iter()
                .filter(|line| !line.is_empty())
                .map(|line| chat::line(&line))
                .collect()
        })
    }

    fn command_lines(&mut self, call: &mut UbCall<'_>, arguments: &str) -> Option<Vec<String>> {
        let text = arguments.trim();
        let (verb, rest) = match text.find(' ') {
            None => (text, ""),
            Some(at) => (&text[..at], text[at + 1..].trim()),
        };
        match casing::to_lower_invariant(verb).as_str() {
            "level" => Some(vec![self.start(call, true)]),
            "slow" => Some(vec![self.start(call, false)]),
            "test" => Some(Self::print_plan(call)),
            "export" => Some(vec![policy::export(&policy::parse(&policy_lines(call)))]),
            "import" => {
                let mut notices = Vec::new();
                let imported = policy::import(&policy_lines(call), rest, &mut notices);
                let row = call.settings.require(POLICY);
                call.settings
                    .set(&row, UbSettingValue::from_collection(&imported));
                notices.push(format!(
                    "Experience policy now holds {} weight(s).",
                    policy::parse(&imported).len()
                ));
                Some(notices)
            }
            "" => Some(Self::print_policy(call)),
            _ => None,
        }
    }

    /// Walks the plan, one request each interval; true while running.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) -> bool {
        if !self.is_running {
            return false;
        }
        if call.services.macro_enabled() {
            self.halt(
                call,
                Some("Experience spending stopped: the macro was enabled.".to_owned()),
            );
            return false;
        }
        if self.index >= self.plan.len() {
            self.halt(call, None);
            return false;
        }
        self.since_spend += elapsed_seconds.max(0.0);
        let tries = call.settings.value(TRIES_TIME).as_int32().max(0);
        if self.since_spend * 1000.0 < f64::from(tries) {
            return true;
        }
        self.since_spend = 0.0;

        let step = self.plan[self.index];
        let Some(stat_id) = try_stat_id(call, &step.target) else {
            self.halt(
                call,
                Some(format!(
                    "Experience spending stopped: the client has no record of {}.",
                    step.target.name
                )),
            );
            return false;
        };
        let result = call.host.advance(
            step.target.advancement_kind(),
            stat_id,
            step.cost.cast_unsigned(),
        );
        if result.status != AdvancementStatus::Sent {
            let notice = match result.notice.as_deref() {
                Some(notice) if !casing::is_white_space_only(notice) => format!(", {notice}"),
                _ => ".".to_owned(),
            };
            self.halt(
                call,
                Some(format!(
                    "Experience spending stopped at {}: {:?}{notice}",
                    step.target.name, result.status
                )),
            );
            return false;
        }
        self.index += 1;
        self.status = format!(
            "Spent {} on {} ({}/{}).",
            number::format_i64(step.cost, "n0").expect("a number format the kit prints"),
            step.target.name,
            self.index,
            self.plan.len()
        );
        true
    }

    /// Puts down a run without a word.
    pub fn reset(&mut self) {
        self.is_running = false;
        self.plan.clear();
        self.index = 0;
        self.since_spend = 0.0;
        "Experience spending idle.".clone_into(&mut self.status);
    }

    fn start(&mut self, call: &mut UbCall<'_>, batch: bool) -> String {
        if self.is_running {
            // The reference says it is stopping, then the stop says how far
            // it got.
            call.post_ub(&chat::line("Stopping AutoXp."));
            self.halt(call, None);
            return String::new();
        }
        if call.host.character().is_none() {
            "Experience spending needs a live session.".clone_into(&mut self.status);
            return chat::error(&self.status);
        }
        if call.services.macro_enabled() {
            "Experience spending refuses to run while the macro is enabled; stop it first."
                .clone_into(&mut self.status);
            return chat::error(&self.status);
        }

        let unassigned = unassigned_experience(call);
        let plan = plan(call, unassigned, batch);
        // Nothing to level is not worth a word to the reference.
        if plan.steps.is_empty() {
            "Nothing to level: no affordable rank under the policy.".clone_into(&mut self.status);
            return String::new();
        }
        self.plan = plan.steps;
        self.index = 0;
        // The first request goes out on the next tick, not an interval later.
        self.since_spend = f64::MAX / 2.0;
        self.is_running = true;
        self.status = format!(
            "Spending on a plan consisting of {} steps with {unassigned} available exp.",
            self.plan.len()
        );
        self.status.clone()
    }

    /// Ends a run: a reason it stopped for first, as an error, then how many
    /// targets it levelled.
    fn halt(&mut self, call: &mut UbCall<'_>, reason: Option<String>) {
        self.is_running = false;
        self.plan.clear();
        if let Some(reason) = &reason {
            call.post_ub(&chat::error(reason));
        }
        let finished = format!("Finished leveling {} targets.", self.index);
        self.status = reason.unwrap_or_else(|| finished.clone());
        call.post_ub(&chat::line(&finished));
    }

    fn print_plan(call: &mut UbCall<'_>) -> Vec<String> {
        let unassigned = unassigned_experience(call);
        let plan = plan(call, unassigned, false);
        let mut lines = vec![format!("Experience plan for {unassigned} exp:")];
        for target in &plan.by_target {
            lines.push(format!(
                "{}: {} levels for {} xp",
                target.target.name,
                target.costs.len(),
                target.costs.iter().sum::<i64>()
            ));
        }
        if plan.by_target.is_empty() {
            lines.push("(nothing can be levelled under the policy)".to_owned());
        }
        lines
    }

    fn print_policy(call: &mut UbCall<'_>) -> Vec<String> {
        let mut lines = vec!["Current experience policy weights:".to_owned()];
        for (name, weight) in policy::parse(&policy_lines(call)) {
            if weight > 0.0 {
                lines.push(format!(
                    "{name}: {}",
                    mosstank_dotnet::double_to_string(weight)
                ));
            }
        }
        lines
    }
}

fn policy_lines(call: &UbCall<'_>) -> Vec<String> {
    call.settings.value(POLICY).items
}

fn plan(call: &mut UbCall<'_>, unassigned: i64, batch: bool) -> ExperiencePlan {
    let policy = policy::parse(&policy_lines(call));
    let stop_before_max = call.settings.value(STOP_BEFORE_MAX).as_int32();
    // A double row read as a whole number, as `(long)` reads it.
    #[allow(clippy::cast_possible_truncation)]
    let max_chunk = call.settings.value(MAX_XP_CHUNK).number as i64;
    let states = states(call);
    experience::plan(
        &policy,
        &states,
        unassigned,
        stop_before_max,
        batch,
        max_chunk,
        DEFAULT_MAX_STEPS,
    )
}

/// Health, stamina and mana are pools 0, 1 and 2 to the lookup.
fn vital_kind(target: &ExperienceTarget) -> i32 {
    match target.name {
        "Health" => 0,
        "Stamina" => 1,
        _ => 2,
    }
}

/// Where every target stands, off the character sheet: attributes by name,
/// pools by kind, skills by id. A target the client has no record of is
/// left out and so never planned.
fn states(call: &mut UbCall<'_>) -> Vec<ExperienceTargetState> {
    let attributes = call.host.attributes();
    let vitals = call.host.vitals();
    let mut states = Vec::new();
    for target in TARGETS {
        let (ranks, training) = match target.kind {
            ExperienceTargetKind::Attribute => {
                match attributes
                    .iter()
                    .find(|attribute| ordinal::equals_ignore_case(&attribute.name, target.name))
                {
                    Some(attribute) => (attribute.ranks, Training::Unknown),
                    None => continue,
                }
            }
            ExperienceTargetKind::Vital => {
                match vitals
                    .iter()
                    .find(|vital| vital.kind == vital_kind(&target))
                {
                    Some(vital) => (vital.ranks, Training::Unknown),
                    None => continue,
                }
            }
            ExperienceTargetKind::Skill => match call.host.skill(target.stat_id) {
                Some(skill) => (skill.ranks, skill.training),
                None => continue,
            },
        };
        states.push(ExperienceTargetState {
            target,
            ranks: ranks.cast_signed(),
            training,
        });
    }
    states
}

/// The number a request names the target by: the client's own record
/// first, the catalogue's when the record carries none.
fn try_stat_id(call: &mut UbCall<'_>, target: &ExperienceTarget) -> Option<u32> {
    let pick = |own: u32| if own != 0 { own } else { target.stat_id };
    match target.kind {
        ExperienceTargetKind::Attribute => call
            .host
            .attributes()
            .iter()
            .find(|attribute| ordinal::equals_ignore_case(&attribute.name, target.name))
            .map(|attribute| pick(attribute.stat_id)),
        ExperienceTargetKind::Vital => call
            .host
            .vitals()
            .iter()
            .find(|vital| vital.kind == vital_kind(target))
            .map(|vital| pick(vital.stat_id)),
        ExperienceTargetKind::Skill => call.host.skill(target.stat_id).map(|_| target.stat_id),
    }
}

fn unassigned_experience(call: &mut UbCall<'_>) -> i64 {
    let Some(character) = call.host.character() else {
        return 0;
    };
    call.host
        .properties(character.id)
        .and_then(|properties| {
            properties
                .int64s
                .iter()
                .find(|(key, _)| *key == AVAILABLE_EXPERIENCE_PROPERTY)
                .map(|(_, value)| (*value).max(0))
        })
        .unwrap_or(0)
}
