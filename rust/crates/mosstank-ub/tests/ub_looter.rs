//! The looter (UtilityBelt's `Looter`): a chest or the own corpse that
//! opens, `/ub use` on a container, the key looter and `/ub keyloot`.

mod common;

use std::collections::VecDeque;

use ac_plugin_abi::items::{InventoryItem, ItemUseCompletion};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use common::Rig;
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_ub::tools::looter::{ChestLooter, ItemState, LOCK_ALREADY_UNLOCKED, LooterState};

const CHEST: u32 = 0x7000_0001;
const CORPSE: u32 = 0x7000_0002;
const KEY_ONE: u32 = 0x5000_0101;
const KEY_TWO: u32 = 0x5000_0102;
const KEY_NAME: &str = "Legendary Key";
/// The server's answer to a key that does not fit.
const KEY_DOESNT_FIT: u32 = 0x04B2;

/// "Tester" (id 1) at the origin.
fn rig() -> Rig {
    let mut rig = Rig::new().at_heading(0.0);
    let character = rig.host.meta.character_mut();
    character.id = 1;
    "Tester".clone_into(&mut character.name);
    rig.host.meta.submitted.clear();
    rig
}

fn set(rig: &mut Rig, name: &str, value: &str) {
    rig.ub(&format!("opt set {name} {value}"));
    rig.clear_messages();
}

/// The looter on, for chests and the own corpse.
fn enabled() -> Rig {
    let mut rig = rig();
    set(&mut rig, "Looter.Enabled", "True");
    set(&mut rig, "Looter.EnableChests", "True");
    set(&mut rig, "Looter.EnableMyCorpse", "True");
    rig
}

fn container(id: u32, name: &str, class: ObjectClass, capacity: i32) -> WorldObject {
    WorldObject {
        items_capacity: capacity,
        ..common::landscape(id, name, class, 0.0)
    }
}

/// A chest as ACE describes most of them: no item limit (-1).
fn chest() -> WorldObject {
    container(CHEST, "Chest", ObjectClass::Container, -1)
}

fn loose(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: CHEST,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn carried(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        container_id: 1,
        ..loose(id, name)
    }
}

fn rule(name: &str, expression: &str, action: LootAction) -> LootRule {
    let mut rule = LootRule::default();
    name.clone_into(&mut rule.name);
    rule.action = action;
    rule.set_expression(expression);
    rule
}

/// Gems kept, salvage salvaged, the rest left.
fn profile(rig: &mut Rig) {
    rig.services.active_loot_rules = Some(vec![
        rule("Gems", "name ~= gem", LootAction::Keep),
        rule("Salvage", "name ~= salvage", LootAction::Salvage),
        rule("Rest", "*", LootAction::NoLoot),
    ]);
}

/// Keeps up to `count` gems.
fn keep_up_to(rig: &mut Rig, count: i32) {
    let mut gems = rule("Gems", "name ~= gem", LootAction::KeepUpTo);
    gems.keep_count = count;
    rig.services.active_loot_rules = Some(vec![gems, rule("Rest", "*", LootAction::NoLoot)]);
}

/// A container in the world holding `items`, each described; not open.
fn place(rig: &mut Rig, object: WorldObject, items: Vec<InventoryItem>, appraised: bool) {
    rig.host.meta.objects.push(object);
    for item in &items {
        rig.host.meta.objects.push(WorldObject {
            object_id: item.object_id,
            name: item.name.clone(),
            object_class: ObjectClass::Gem,
            container_id: item.container_id,
            has_appraisal_data: appraised,
            ..WorldObject::default()
        });
    }
    rig.host.loot_contents = items;
    rig.host.loot_contents_ready = true;
}

fn open(rig: &mut Rig, id: u32) {
    rig.host.meta.open_container = id;
}

fn set_locked(rig: &mut Rig, id: u32, locked: bool) {
    rig.host.meta.properties.insert(
        id,
        ItemProperties {
            bools: vec![(3, locked)],
            ..ItemProperties::default()
        },
    );
}

fn looter(rig: &Rig) -> &ChestLooter {
    &rig.commands.tools().looter
}

/// One frame of the looter.
fn tick(rig: &mut Rig, seconds: f64) {
    rig.locks.borrow_mut().advance(seconds);
    rig.with_call(|call, commands| commands.tools_mut().looter.tick(call, seconds));
}

/// Every item picked out of the container arrives in the packs.
fn arrive(rig: &mut Rig) {
    let picked = rig.host.pickups.clone();
    let mut arrived = Vec::new();
    rig.host.loot_contents.retain(|item| {
        let taken = picked.contains(&item.object_id);
        if taken {
            arrived.push(item.clone());
        }
        !taken
    });
    for item in arrived {
        rig.host.meta.owned_items.push(InventoryItem {
            container_id: 1,
            ..item
        });
    }
}

/// Frames of 0.1 s with every pickup arriving, a fixed count.
fn frames(rig: &mut Rig, count: usize) {
    for _ in 0..count {
        tick(rig, 0.1);
        arrive(rig);
    }
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

// ---- looting what opens ----------------------------------------------------

#[test]
fn an_opened_chest_is_judged_item_by_item_and_what_the_profile_keeps_is_taken() {
    let mut rig = enabled();
    profile(&mut rig);
    place(
        &mut rig,
        chest(),
        vec![
            loose(10, "Red Gem"),
            loose(11, "Rusty Junk"),
            loose(12, "Steel Salvage"),
        ],
        true,
    );
    open(&mut rig, CHEST);

    frames(&mut rig, 10);

    // Keep and Salvage are taken, the rest is left; a chest the player
    // opened is left open.
    assert_eq!(vec![10, 12], rig.host.pickups);
    assert!(rig.host.closed_loot.is_empty());
    assert!(!looter(&rig).is_running());
    assert_eq!(
        strings(&["[UB] Looter completed in 0s Scanned 3 items and looted 2 items"]),
        rig.messages()
    );
    // Opened once, looted once: the open chest is not looted again.
    frames(&mut rig, 10);
    assert_eq!(vec![10, 12], rig.host.pickups);
}

#[test]
fn an_item_whose_verdict_needs_an_appraisal_is_appraised_first_one_at_a_time() {
    let mut rig = enabled();
    profile(&mut rig);
    place(
        &mut rig,
        chest(),
        vec![loose(10, "Red Gem"), loose(11, "Blue Gem")],
        false,
    );
    open(&mut rig, CHEST);

    frames(&mut rig, 4);
    // The first is asked about; the second waits for the slot.
    assert_eq!(vec![10], rig.host.identified);
    assert!(rig.host.pickups.is_empty());
    assert_eq!(Some(ItemState::RequestingInfo), looter(&rig).item_state(10));

    for object in &mut rig.host.meta.objects {
        if object.object_id == 10 {
            object.has_appraisal_data = true;
        }
    }
    frames(&mut rig, 1);
    // Its answer decides it; the next is asked about.
    assert_eq!(vec![10], rig.host.pickups);
    assert_eq!(vec![10, 11], rig.host.identified);
}

#[test]
fn keep_up_to_counts_what_is_owned() {
    let mut rig = enabled();
    keep_up_to(&mut rig, 2);
    rig.host.meta.owned_items = vec![carried(50, "Red Gem"), carried(51, "Red Gem")];
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    open(&mut rig, CHEST);

    frames(&mut rig, 10);

    assert!(rig.host.pickups.is_empty());
    assert_eq!(
        strings(&["[UB] Looter completed in 0s Scanned 1 items and looted 0 items"]),
        rig.messages()
    );
}

#[test]
fn keep_up_to_counts_what_is_on_its_way() {
    let mut rig = enabled();
    keep_up_to(&mut rig, 2);
    rig.host.meta.owned_items = vec![carried(50, "Red Gem")];
    place(
        &mut rig,
        chest(),
        vec![
            loose(10, "Red Gem"),
            loose(11, "Red Gem"),
            loose(12, "Red Gem"),
        ],
        true,
    );
    open(&mut rig, CHEST);

    // Nothing arrives while the three are judged: the one being taken
    // counts towards the two.
    for _ in 0..6 {
        tick(&mut rig, 0.1);
    }
    arrive(&mut rig);
    frames(&mut rig, 4);

    assert!(rig.host.pickups.iter().all(|id| *id == 10));
    assert!(!rig.host.pickups.is_empty());
}

#[test]
fn a_container_of_forty_seven_slots_is_left_and_one_of_forty_eight_is_looted() {
    for (slots, looted) in [(47, false), (48, true)] {
        let mut rig = enabled();
        profile(&mut rig);
        place(
            &mut rig,
            container(CHEST, "Chest", ObjectClass::Container, slots),
            vec![loose(10, "Red Gem")],
            true,
        );
        open(&mut rig, CHEST);
        frames(&mut rig, 10);
        assert_eq!(looted, !rig.host.pickups.is_empty(), "{slots} slots");
    }
}

#[test]
fn a_chest_without_an_item_limit_is_looted_and_a_pack_is_not() {
    // The server sends the slot count as a byte: -1 is 255, no limit.
    for (slots, looted) in [(-1, true), (24, false)] {
        let mut rig = enabled();
        profile(&mut rig);
        place(
            &mut rig,
            container(CHEST, "Chest", ObjectClass::Container, slots),
            vec![loose(10, "Red Gem")],
            true,
        );
        open(&mut rig, CHEST);
        frames(&mut rig, 10);
        assert_eq!(looted, !rig.host.pickups.is_empty(), "{slots} slots");
    }
}

#[test]
fn the_own_corpse_gives_back_everything_and_a_monsters_corpse_is_left() {
    // The own corpse: everything, whatever the profile says.
    let mut rig = enabled();
    profile(&mut rig);
    place(
        &mut rig,
        container(CORPSE, "Corpse of Tester", ObjectClass::Corpse, 120),
        vec![loose(10, "Rusty Junk"), loose(11, "Red Gem")],
        true,
    );
    open(&mut rig, CORPSE);
    frames(&mut rig, 10);
    assert_eq!(vec![10, 11], rig.host.pickups);

    // Anyone else's corpse is the macro's looter's.
    let mut rig = enabled();
    profile(&mut rig);
    place(
        &mut rig,
        container(CORPSE, "Corpse of Drudge Skulker", ObjectClass::Corpse, 120),
        vec![loose(11, "Red Gem")],
        true,
    );
    open(&mut rig, CORPSE);
    frames(&mut rig, 10);
    assert!(rig.host.pickups.is_empty());
    assert!(rig.messages().is_empty());
}

#[test]
fn each_kind_of_container_waits_for_its_own_switch() {
    // A chest with chests off.
    let mut rig = enabled();
    set(&mut rig, "Looter.EnableChests", "False");
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    open(&mut rig, CHEST);
    frames(&mut rig, 10);
    assert!(rig.host.pickups.is_empty());

    // The own corpse with the own corpse off.
    let mut rig = enabled();
    set(&mut rig, "Looter.EnableMyCorpse", "False");
    profile(&mut rig);
    place(
        &mut rig,
        container(CORPSE, "Corpse of Tester", ObjectClass::Corpse, 120),
        vec![loose(10, "Red Gem")],
        true,
    );
    open(&mut rig, CORPSE);
    frames(&mut rig, 10);
    assert!(rig.host.pickups.is_empty());
}

#[test]
fn with_the_looter_off_nothing_happens() {
    let mut rig = enabled();
    set(&mut rig, "Looter.Enabled", "False");
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    open(&mut rig, CHEST);

    frames(&mut rig, 20);

    assert!(rig.host.pickups.is_empty());
    assert!(rig.host.identified.is_empty());
    assert!(rig.messages().is_empty());
    assert!(!looter(&rig).is_running());
}

#[test]
fn a_container_the_macros_own_looter_is_working_is_left_to_it() {
    let mut rig = enabled();
    profile(&mut rig);
    rig.services.macro_loot_container = CORPSE;
    place(
        &mut rig,
        container(CORPSE, "Corpse of Tester", ObjectClass::Corpse, 120),
        vec![loose(10, "Red Gem")],
        true,
    );
    open(&mut rig, CORPSE);

    frames(&mut rig, 10);

    assert!(rig.host.pickups.is_empty());
    assert!(!looter(&rig).is_running());
}

#[test]
fn looting_the_own_corpse_holds_the_macro_and_block_melee_holds_its_attacks() {
    for block in [true, false] {
        let mut rig = enabled();
        set(
            &mut rig,
            "Looter.BlockVtankMelee",
            if block { "True" } else { "False" },
        );
        profile(&mut rig);
        place(
            &mut rig,
            container(CORPSE, "Corpse of Tester", ObjectClass::Corpse, 120),
            vec![loose(10, "Red Gem")],
            true,
        );
        open(&mut rig, CORPSE);

        // The item is asked for and has not arrived yet.
        tick(&mut rig, 0.1);
        tick(&mut rig, 0.1);
        {
            let mut locks = rig.locks.borrow_mut();
            assert!(locks.is_locked(ActionLockKind::Navigation), "{block}");
            assert!(locks.is_locked(ActionLockKind::ItemUse), "{block}");
            assert_eq!(
                block,
                locks.is_locked(ActionLockKind::MeleeAttackShot),
                "{block}"
            );
        }

        // Done: every slot it held is let go.
        arrive(&mut rig);
        frames(&mut rig, 5);
        assert!(!looter(&rig).is_running());
        let mut locks = rig.locks.borrow_mut();
        assert!(!locks.is_locked(ActionLockKind::Navigation), "{block}");
        assert!(!locks.is_locked(ActionLockKind::MeleeAttackShot), "{block}");
    }
}

#[test]
fn an_item_that_will_not_come_is_given_up_on_after_its_attempts() {
    let mut rig = enabled();
    set(&mut rig, "Looter.AttemptsBeforeBlacklisting", "5");
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    open(&mut rig, CHEST);

    // Nothing ever arrives.
    for _ in 0..20 {
        tick(&mut rig, 0.1);
    }

    assert_eq!(vec![10; 5], rig.host.pickups);
    assert_eq!(
        strings(&[
            "[UB] Looter: Failed to loot Red Gem",
            "[UB] Looter completed in 0s --- Scanned 1 items and looted 0 items --- Failed to loot 1 items",
        ]),
        rig.messages()
    );
}

// ---- /ub use ---------------------------------------------------------------

/// A server that opens what is used and closes what is closed.
fn serve_use(rig: &mut Rig) {
    if !rig.host.closed_loot.is_empty() {
        open(rig, 0);
    } else if let Some(&last) = rig.host.opened_loot.last() {
        open(rig, last);
    }
}

#[test]
fn ub_use_on_a_chest_opens_it_loots_it_and_closes_it_whatever_the_switches_say() {
    let mut rig = rig();
    profile(&mut rig);
    place(
        &mut rig,
        chest(),
        vec![loose(10, "Red Gem"), loose(11, "Rusty Junk")],
        true,
    );

    rig.ub("use Chest");
    for _ in 0..30 {
        tick(&mut rig, 0.1);
        arrive(&mut rig);
        serve_use(&mut rig);
    }

    assert_eq!(vec![CHEST], rig.host.opened_loot);
    assert_eq!(vec![10], rig.host.pickups);
    assert_eq!(vec![CHEST], rig.host.closed_loot);
    assert!(!looter(&rig).is_running());
    let messages = rig.messages();
    assert_eq!(2, messages.len(), "{messages:?}");
    assert_eq!("[UB] using Chest", messages[0]);
    assert!(
        messages[1].starts_with("[UB] Looter completed in ")
            && messages[1].ends_with(" Scanned 2 items and looted 1 items"),
        "{messages:?}"
    );
}

#[test]
fn ub_use_on_a_monsters_corpse_loots_it_too() {
    let mut rig = rig();
    profile(&mut rig);
    place(
        &mut rig,
        container(CORPSE, "Corpse of Drudge Skulker", ObjectClass::Corpse, 120),
        vec![loose(10, "Red Gem")],
        true,
    );

    rig.ub("use Corpse of Drudge Skulker");
    for _ in 0..30 {
        tick(&mut rig, 0.1);
        arrive(&mut rig);
        serve_use(&mut rig);
    }

    assert_eq!(vec![10], rig.host.pickups);
    assert_eq!(vec![CORPSE], rig.host.closed_loot);
}

#[test]
fn ub_use_on_a_locked_chest_bails() {
    let mut rig = rig();
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    set_locked(&mut rig, CHEST, true);

    rig.ub("use Chest");
    frames(&mut rig, 10);

    assert!(rig.host.opened_loot.is_empty());
    assert!(!looter(&rig).is_running());
    assert_eq!(
        strings(&[
            "[UB] using Chest",
            "[UB] Looter: Chest is locked... bailing"
        ]),
        rig.messages()
    );
}

#[test]
fn ub_use_tries_to_open_up_to_max_open_attempts_paced_by_the_delay() {
    let mut rig = rig();
    set(&mut rig, "Looter.MaxOpenAttempts", "3");
    set(&mut rig, "Looter.DelaySpeed", "1000");
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);

    rig.ub("use Chest");
    rig.clear_messages();
    // The container never opens.
    frames(&mut rig, 5);
    assert_eq!(1, rig.host.opened_loot.len());
    frames(&mut rig, 7);
    assert_eq!(2, rig.host.opened_loot.len());
    frames(&mut rig, 40);

    assert_eq!(vec![CHEST; 3], rig.host.opened_loot);
    assert!(!looter(&rig).is_running());
    assert_eq!(
        strings(&["[UB] Reached max number of opening attempts"]),
        rig.messages()
    );
}

#[test]
fn ub_use_on_a_pack_only_opens_it() {
    let mut rig = rig();
    place(
        &mut rig,
        container(CHEST, "Pack", ObjectClass::Container, 24),
        Vec::new(),
        true,
    );
    rig.ub("use Pack");
    assert_eq!(vec![CHEST], rig.host.opened_loot);
    assert!(!looter(&rig).is_running());
}

// ---- the key looter --------------------------------------------------------

/// A server for key looting: a key that fits turns the lock and is used
/// up, an unlocked chest opens, a closed chest locks again and refills from
/// `refills`.
struct KeyServer {
    applied: usize,
    opened: usize,
    closed: usize,
    revision: i64,
    /// The error every key use is answered with; 0 turns the lock.
    answer: u32,
    /// The key is used up when it turns the lock.
    consumes: bool,
    /// The chest locks again when it closes.
    relocks: bool,
    refills: VecDeque<Vec<InventoryItem>>,
}

impl KeyServer {
    fn new(answer: u32) -> Self {
        Self {
            applied: 0,
            opened: 0,
            closed: 0,
            revision: 0,
            answer,
            consumes: true,
            relocks: true,
            refills: VecDeque::new(),
        }
    }

    fn serve(&mut self, rig: &mut Rig) {
        while self.applied < rig.host.applied.len() {
            let (key, target) = rig.host.applied[self.applied];
            self.applied += 1;
            self.revision += 1;
            rig.host.meta.item_state.last_completion = ItemUseCompletion {
                revision: self.revision,
                source_id: key,
                target_id: target,
                weenie_error: self.answer,
            };
            if self.answer == 0 {
                set_locked(rig, target, false);
                if self.consumes {
                    rig.host
                        .meta
                        .owned_items
                        .retain(|item| item.object_id != key);
                }
            }
        }
        while self.opened < rig.host.opened_loot.len() {
            let id = rig.host.opened_loot[self.opened];
            self.opened += 1;
            let locked = rig
                .host
                .meta
                .properties
                .get(&id)
                .is_some_and(|properties| properties.bools.contains(&(3, true)));
            if !locked {
                open(rig, id);
            }
        }
        while self.closed < rig.host.closed_loot.len() {
            self.closed += 1;
            open(rig, 0);
            if self.relocks {
                set_locked(rig, CHEST, true);
            }
            let refill = self.refills.pop_front().unwrap_or_default();
            for item in &refill {
                rig.host.meta.objects.push(WorldObject {
                    object_id: item.object_id,
                    name: item.name.clone(),
                    object_class: ObjectClass::Gem,
                    has_appraisal_data: true,
                    ..WorldObject::default()
                });
            }
            rig.host.loot_contents = refill;
        }
        arrive(rig);
    }
}

/// A locked chest, selected, holding a gem; two keys carried.
fn key_rig() -> Rig {
    let mut rig = rig();
    profile(&mut rig);
    place(&mut rig, chest(), vec![loose(10, "Red Gem")], true);
    set_locked(&mut rig, CHEST, true);
    rig.host.meta.selection.selected = Some(CHEST);
    rig.host.meta.owned_items = vec![carried(KEY_ONE, KEY_NAME), carried(KEY_TWO, KEY_NAME)];
    rig
}

fn serve(rig: &mut Rig, server: &mut KeyServer, frames: usize) {
    for _ in 0..frames {
        tick(rig, 0.1);
        server.serve(rig);
    }
}

#[test]
fn the_key_looter_goes_round_by_round_until_the_keys_run_out() {
    let mut rig = key_rig();
    let mut server = KeyServer::new(0);
    server.refills.push_back(vec![loose(20, "Blue Gem")]);

    rig.ub("keyloot Legendary Key");
    assert!(looter(&rig).is_key_looting());
    serve(&mut rig, &mut server, 80);

    assert_eq!(vec![(KEY_ONE, CHEST), (KEY_TWO, CHEST)], rig.host.applied);
    assert_eq!(vec![CHEST, CHEST], rig.host.opened_loot);
    assert_eq!(vec![CHEST, CHEST], rig.host.closed_loot);
    assert_eq!(vec![10, 20], rig.host.pickups);
    assert!(!looter(&rig).is_running());
    let messages = rig.messages();
    assert_eq!(3, messages.len(), "{messages:?}");
    assert!(messages[0].ends_with(" Scanned 1 items and looted 1 items"));
    assert!(messages[1].ends_with(" Scanned 1 items and looted 1 items"));
    assert_eq!("[UB] Looter: Out of keys to use", messages[2]);
}

#[test]
fn a_key_that_does_not_fit_is_tried_up_to_max_unlock_attempts() {
    let mut rig = key_rig();
    set(&mut rig, "Looter.MaxUnlockAttempts", "3");
    set(&mut rig, "Looter.DelaySpeed", "1000");
    let mut server = KeyServer::new(KEY_DOESNT_FIT);

    rig.ub("keyloot Legendary Key");
    serve(&mut rig, &mut server, 5);
    assert_eq!(1, rig.host.applied.len());
    serve(&mut rig, &mut server, 7);
    assert_eq!(2, rig.host.applied.len());
    serve(&mut rig, &mut server, 40);

    assert_eq!(3, rig.host.applied.len());
    assert!(rig.host.opened_loot.is_empty());
    assert!(!looter(&rig).is_running());
    assert_eq!(
        strings(&["[UB] Reached max number of unlock attempts"]),
        rig.messages()
    );
}

#[test]
fn a_chest_already_unlocked_is_opened_and_an_empty_one_ends_the_run() {
    let mut rig = key_rig();
    set_locked(&mut rig, CHEST, false);
    let mut server = KeyServer::new(LOCK_ALREADY_UNLOCKED);
    server.relocks = false;

    rig.ub("keyloot Legendary Key");
    serve(&mut rig, &mut server, 80);

    // The first round loots the gem; the second finds the chest empty, and
    // a key that is never used up would go round for ever.
    assert_eq!(vec![CHEST, CHEST], rig.host.opened_loot);
    assert_eq!(vec![10], rig.host.pickups);
    assert!(!looter(&rig).is_running());
    let messages = rig.messages();
    assert_eq!(2, messages.len(), "{messages:?}");
    assert!(messages[1].ends_with(" Scanned 0 items and looted 0 items"));
}

#[test]
fn keyloot_refuses_what_it_cannot_loot() {
    // No key name.
    let mut rig = key_rig();
    rig.ub("keyloot");
    assert_eq!("[UB] Error: Bad command syntax", rig.messages()[0]);
    assert!(!looter(&rig).is_running());

    // Nothing selected.
    let mut rig = key_rig();
    rig.host.meta.selection.selected = None;
    rig.ub("keyloot Legendary Key");
    assert_eq!(
        strings(&["[UB] Error: Looter: nothing selected"]),
        rig.messages()
    );

    // Something that is not a chest.
    let mut rig = key_rig();
    rig.host.meta.objects.push(common::player(77, "Bob"));
    rig.host.meta.selection.selected = Some(77);
    rig.ub("keyloot Legendary Key");
    assert_eq!(
        strings(&["[UB] Error: Looter: target is not a chest"]),
        rig.messages()
    );

    // No key of that name.
    let mut rig = key_rig();
    rig.ub("keyloot Rusty Key");
    assert_eq!(
        strings(&["[UB] Error: Looter: no Rusty Key in your inventory"]),
        rig.messages()
    );
    assert!(!looter(&rig).is_running());
    assert!(rig.host.applied.is_empty());
}

#[test]
fn keyloot_stop_stops_a_run_and_says_when_there_is_none() {
    let mut rig = key_rig();
    rig.ub("keyloot stop");
    assert_eq!(
        strings(&["[UB] Error: Looter is not running."]),
        rig.messages()
    );

    rig.clear_messages();
    rig.ub("keyloot Legendary Key");
    assert!(looter(&rig).is_key_looting());
    rig.ub("keyloot stop");
    assert!(!looter(&rig).is_running());
    assert_eq!(LooterState::Done, looter(&rig).state());
    assert_eq!(strings(&["[UB] Looter stopped"]), rig.messages());
}

#[test]
fn the_page_sets_the_key_and_the_chest_from_the_selection_and_starts_and_stops() {
    let mut rig = key_rig();
    rig.host.meta.objects.push(WorldObject {
        object_id: KEY_ONE,
        name: KEY_NAME.to_owned(),
        object_class: ObjectClass::Key,
        is_owned: true,
        ..WorldObject::default()
    });

    // A chest is not a key.
    rig.with_call(|call, commands| commands.tools_mut().looter.set_page_key(call));
    assert_eq!(
        strings(&["[UB] Error: Looter: target is not a key"]),
        rig.messages()
    );
    rig.with_call(|call, commands| commands.tools_mut().looter.set_page_chest(call));
    rig.host.meta.selection.selected = Some(KEY_ONE);
    rig.with_call(|call, commands| commands.tools_mut().looter.set_page_key(call));
    assert_eq!(KEY_NAME, looter(&rig).page_key());
    assert_eq!("Chest", looter(&rig).page_chest());
    assert_eq!("Start", looter(&rig).page_button_text());

    rig.clear_messages();
    rig.with_call(|call, commands| commands.tools_mut().looter.press_page_button(call));
    assert!(looter(&rig).is_key_looting());
    assert_eq!("Stop", looter(&rig).page_button_text());
    rig.with_call(|call, commands| commands.tools_mut().looter.press_page_button(call));
    assert!(!looter(&rig).is_running());
    assert_eq!("Start", looter(&rig).page_button_text());
    assert_eq!(strings(&["[UB] Looter stopped"]), rig.messages());
}
