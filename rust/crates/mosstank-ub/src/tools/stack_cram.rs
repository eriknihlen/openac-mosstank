//! Stacking and cramming (`InventoryMaintenance`, `StackCramCommandRun`):
//! the planner that picks the one move that tidies the packs most, the
//! controller that makes one move at a time and waits for the client's
//! receipt before planning the next, and the pass `/ub autostack` and
//! `/ub autocram` run whatever the macro's own switches say.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{InventoryCompletion, InventoryItem, ItemStatus};
use mosstank_settings::model::InventorySettings;

use crate::commands::UbCall;
use crate::host::UbHost;

/// Public weenie flag of a focus (a spell component pouch and the like):
/// never crammed.
const PUBLIC_WEENIE_FOCI: u32 = 0x0080_0000;

/// How many refused moves of one pair before the pair is given up on.
const AUTHENTIC_ABANDON_ATTEMPTS: i32 = 80;

/// Which kind of move a plan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryMaintenanceKind {
    /// One stack onto another of the same thing.
    Merge,
    /// A loose item into a side pack.
    Cram,
}

/// One move (`InventoryMaintenancePlan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryMaintenancePlan {
    /// Which kind.
    pub kind: InventoryMaintenanceKind,
    /// What moves.
    pub source_object_id: u32,
    /// Where to.
    pub target_object_id: u32,
    /// How many.
    pub amount: u32,
}

fn is_success(completion: &InventoryCompletion) -> bool {
    completion.revision != 0 && completion.weenie_error == 0
}

/// The next move: a stack first when stacking is on, otherwise (or when
/// nothing stacks) a cram when cramming is on (`InventoryMaintenancePlanner.Plan`).
#[must_use]
pub fn plan(
    items: &[InventoryItem],
    player_object_id: u32,
    settings: &InventorySettings,
    ignored: &HashSet<u32>,
) -> Option<InventoryMaintenancePlan> {
    if settings.auto_stack
        && let Some(stack) = plan_stack(items, ignored)
    {
        return Some(stack);
    }
    if settings.auto_cram {
        plan_cram(items, player_object_id, ignored)
    } else {
        None
    }
}

/// The lightest stack of each kind poured into the heaviest one with room,
/// kinds in weenie class order.
fn plan_stack(items: &[InventoryItem], ignored: &HashSet<u32>) -> Option<InventoryMaintenancePlan> {
    let by_id: HashMap<u32, &InventoryItem> =
        items.iter().map(|item| (item.object_id, item)).collect();
    let mut groups: Vec<(u32, Vec<&InventoryItem>)> = Vec::new();
    for item in items.iter().filter(|item| {
        item.object_id != 0
            && item.weenie_class_id != 0
            && item.maximum_stack_size > 1
            && item.stack_size > 0
            && item.equipped_location == 0
            && !ignored.contains(&item.object_id)
    }) {
        match groups
            .iter_mut()
            .find(|(class, _)| *class == item.weenie_class_id)
        {
            Some((_, members)) => members.push(item),
            None => groups.push((item.weenie_class_id, vec![item])),
        }
    }
    groups.sort_by_key(|(class, _)| *class);

    for (_, mut ordered) in groups {
        ordered.sort_by(|a, b| {
            burden_rank(a, &by_id)
                .cmp(&burden_rank(b, &by_id))
                .then(a.container_slot.cmp(&b.container_slot))
                .then(a.object_id.cmp(&b.object_id))
        });
        if ordered.len() < 2 {
            continue;
        }
        let source = ordered[0];
        for target in ordered[1..].iter().rev() {
            let free = target.maximum_stack_size - target.stack_size.max(1);
            if free <= 0 {
                continue;
            }
            let amount = source.stack_size.max(1).min(free);
            return Some(InventoryMaintenancePlan {
                kind: InventoryMaintenanceKind::Merge,
                source_object_id: source.object_id,
                target_object_id: target.object_id,
                amount: amount.cast_unsigned(),
            });
        }
    }
    None
}

/// The first loose item in the main pack into the first side pack with
/// room.
fn plan_cram(
    items: &[InventoryItem],
    player_object_id: u32,
    ignored: &HashSet<u32>,
) -> Option<InventoryMaintenancePlan> {
    if player_object_id == 0 {
        return None;
    }
    let by_slot = |a: &&InventoryItem, b: &&InventoryItem| {
        a.container_slot
            .cmp(&b.container_slot)
            .then(a.object_id.cmp(&b.object_id))
    };
    let source = items
        .iter()
        .filter(|item| {
            item.container_id == player_object_id
                && item.wielder_id == 0
                && item.items_capacity <= 0
                && item.containers_capacity <= 0
                && item.public_flags & PUBLIC_WEENIE_FOCI == 0
                && !ignored.contains(&item.object_id)
        })
        .min_by(by_slot)?;
    if source.object_id == 0 {
        return None;
    }

    let mut contained_counts: HashMap<u32, i32> = HashMap::new();
    for item in items.iter().filter(|item| item.container_id != 0) {
        *contained_counts.entry(item.container_id).or_default() += 1;
    }
    let destination = items
        .iter()
        .filter(|item| {
            item.container_id == player_object_id
                && item.items_capacity > 0
                && !ignored.contains(&item.object_id)
                && contained_counts
                    .get(&item.object_id)
                    .copied()
                    .unwrap_or_default()
                    < item.items_capacity
        })
        .min_by(by_slot)?;
    (destination.object_id != 0).then(|| InventoryMaintenancePlan {
        kind: InventoryMaintenanceKind::Cram,
        source_object_id: source.object_id,
        target_object_id: destination.object_id,
        amount: source.stack_size.max(1).cast_unsigned(),
    })
}

/// Lighter first, and anything in a side pack after everything loose, by
/// the pack's own burden.
fn burden_rank(item: &InventoryItem, by_id: &HashMap<u32, &InventoryItem>) -> i64 {
    let parent = if item.container_id == 0 {
        0
    } else {
        by_id
            .get(&item.container_id)
            .map_or(0, |container| i64::from(container.burden.max(0)) + 1)
    };
    i64::from(item.burden.max(0)) + 10_000 * parent
}

/// One stack-or-cram move at a time, each waiting for the client's
/// authoritative receipt before the next is planned
/// (`InventoryMaintenanceController`).
#[derive(Debug, Clone)]
pub struct InventoryMaintenanceController {
    attempts: HashMap<(u32, u32), i32>,
    ignored: HashSet<u32>,
    pending: Option<InventoryMaintenancePlan>,
    observed_revision: i64,
    until_scan: f64,
    status: String,
    found_nothing_to_do: bool,
}

impl Default for InventoryMaintenanceController {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryMaintenanceController {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            attempts: HashMap::new(),
            ignored: HashSet::new(),
            pending: None,
            observed_revision: 0,
            until_scan: 0.0,
            status: "Stack/Cram idle".to_owned(),
            found_nothing_to_do: false,
        }
    }

    /// What it is doing, in words.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether the last look at the packs found nothing to stack or cram;
    /// false until a look has been taken, and after every move it starts.
    #[must_use]
    pub fn found_nothing_to_do(&self) -> bool {
        self.found_nothing_to_do
    }

    /// One frame; true only while it owns the character this frame.
    pub fn tick(
        &mut self,
        host: &mut dyn UbHost,
        settings: &InventorySettings,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        let state = host.item_state();
        self.observe_completion(host, &state.last_inventory_completion);
        if self.pending.is_some() {
            if state.is_busy {
                return true;
            }
            self.pending = None;
        }
        if !can_act || host.character().is_none() || !state.is_available {
            return false;
        }
        if !settings.auto_stack && !settings.auto_cram {
            "Stack/Cram disabled".clone_into(&mut self.status);
            return false;
        }
        if state.is_busy {
            return false;
        }

        self.until_scan -= elapsed_seconds.max(0.0);
        if self.until_scan > 0.0 {
            return false;
        }
        self.until_scan = settings.scan_interval_seconds.max(0.05);

        let inventory = host.owned_items();
        self.ignored
            .retain(|id| inventory.iter().any(|item| item.object_id == *id));
        let player = host.character().map_or(0, |character| character.id);
        let Some(next) = plan(&inventory, player, settings, &self.ignored) else {
            "Stack/Cram idle".clone_into(&mut self.status);
            self.found_nothing_to_do = true;
            return false;
        };
        self.found_nothing_to_do = false;

        let result = match next.kind {
            InventoryMaintenanceKind::Merge => {
                host.merge_items(next.source_object_id, next.target_object_id, next.amount)
            }
            InventoryMaintenanceKind::Cram => host.move_item(
                next.source_object_id,
                next.target_object_id,
                next.amount,
                0,
                false,
            ),
        };
        if !result.accepted() {
            self.status = format!("Stack/Cram waiting: {:?}", result.status);
            return result.status == ItemStatus::Busy;
        }

        self.pending = Some(next);
        match next.kind {
            InventoryMaintenanceKind::Merge => "Stacking items",
            InventoryMaintenanceKind::Cram => "Moving an item to a side pack",
        }
        .clone_into(&mut self.status);
        true
    }

    /// Puts down whatever was in flight.
    pub fn reset(&mut self) {
        self.pending = None;
        self.attempts.clear();
        self.ignored.clear();
        self.until_scan = 0.0;
        self.found_nothing_to_do = false;
        "Stack/Cram idle".clone_into(&mut self.status);
    }

    fn observe_completion(&mut self, host: &mut dyn UbHost, completion: &InventoryCompletion) {
        if completion.revision == 0 || completion.revision == self.observed_revision {
            return;
        }
        self.observed_revision = completion.revision;
        let Some(pending) = self.pending else {
            return;
        };
        if completion.source_id != pending.source_object_id {
            return;
        }

        let key = (pending.source_object_id, pending.target_object_id);
        if is_success(completion) {
            self.attempts.remove(&key);
        } else {
            let attempts = self.attempts.get(&key).copied().unwrap_or_default() + 1;
            self.attempts.insert(key, attempts);
            self.status = format!("Stack/Cram failed (0x{:X})", completion.weenie_error);
            if attempts > AUTHENTIC_ABANDON_ATTEMPTS {
                self.ignored.insert(pending.source_object_id);
                self.ignored.insert(pending.target_object_id);
                mosstank_meta::chat::post_mosstank(
                    host,
                    "[MossTank] Abandoned trying to stack/cram two bugged items.",
                );
            }
        }
        self.pending = None;
        self.until_scan = 0.0;
    }
}

/// One stack or cram pass asked for from the command line, whether or not
/// the macro's own switches are on (`StackCramCommandRun`): it says it is
/// running, works one move at a time, and says it is complete once nothing
/// is left; with nothing to do at the start it says so and does not start.
#[derive(Debug, Clone, Default)]
pub struct StackCramCommandRun {
    run: Option<(InventoryMaintenanceController, InventorySettings)>,
    name: String,
}

impl StackCramCommandRun {
    /// Nothing under way.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a pass is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// Starts a stack pass, or a cram pass into the side packs, replacing
    /// any pass under way.
    pub fn start(&mut self, call: &mut UbCall<'_>, stack: bool) {
        let name = if stack { "AutoStack" } else { "AutoCram" };
        let settings = InventorySettings {
            auto_stack: stack,
            auto_cram: !stack,
            ..InventorySettings::default()
        };
        self.run = None;
        let character = call.host.character();
        let nothing = match character {
            None => true,
            Some(character) => {
                let owned = call.host.owned_items();
                plan(&owned, character.id, &settings, &HashSet::new()).is_none()
            }
        };
        if nothing {
            call.write_ub(&format!("{name} - nothing to do"));
            return;
        }
        self.run = Some((InventoryMaintenanceController::new(), settings));
        name.clone_into(&mut self.name);
        call.write_ub(&format!("{name} running"));
    }

    /// Drives the pass; true while it owns the character this frame.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        let Some((run, settings)) = &mut self.run else {
            return false;
        };
        let owns = run.tick(call.host, settings, elapsed_seconds, can_act);
        if !owns && run.found_nothing_to_do() {
            self.run = None;
            let line = format!("{} complete.", self.name);
            call.write_ub(&line);
        }
        owns
    }

    /// Drops a pass without a word, as a new session does.
    pub fn reset(&mut self) {
        self.run = None;
    }
}
