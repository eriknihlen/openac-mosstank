//! The mana list: the worn items, their mana, the time it lasts and
//! whether their spells are up.
//!
//! Source: Mag-Tools `Mag-Tools/Trackers/Equipment/EquipmentTracker.cs`,
//! `EquipmentTrackedItem.cs` and `Views/ManaTrackerGUI.cs` at 43ee2ec. The
//! values are worked out from `SecondsPerBurn`, `CalculatedCurrentMana`,
//! `ManaTimeRemaining`, `RecaclulteState`, `Item_Changed` and `SortList`.

use std::time::Duration;

use ac_plugin_abi::ChatLine;
use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::items::{EquipmentObservation, InventoryItem};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ItemProperties, ObjectChange, ObjectChangeKind, WorldObject};
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::mana::{
    ICON_ACTIVE, ICON_NONE, ICON_NOT_ACTIVE, ICON_UNKNOWN, ManaState, ManaTracker, NO_BURN,
    mana_needed_text, seconds_per_burn, time_text, unretained_text,
};
use mosstank_mag::testing::FakeMagHost;
use mosstank_meta::host::MetaHost;

/// The meta fake's character.
const ME: u32 = 1;
/// A ring slot.
const RING_SLOT: u32 = 0x0004_0000;
/// The item's spell, family 5, difficulty 100.
const ITEM_SPELL: u32 = 100;
/// A stronger timed buff of the same family, difficulty 200.
const STRONG_BUFF: u32 = 200;

fn spell(spell_id: u32, family: u32, difficulty: i32) -> SpellInfo {
    SpellInfo {
        spell_id,
        family,
        difficulty,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

fn enchantment(spell_id: u32, family: u32, seconds_remaining: f64) -> ActiveEnchantment {
    ActiveEnchantment {
        spell_id,
        family,
        tier: 1,
        seconds_remaining,
    }
}

/// A worn item: `mana` of `maximum`, appraised with `spells`, burning
/// `rate` mana a second.
struct Worn {
    id: u32,
    name: &'static str,
    mana: i32,
    maximum: i32,
    spells: Vec<u32>,
    rate: f64,
    appraised: bool,
    retained: bool,
    valid_locations: u32,
    equipped_location: u32,
}

fn worn(id: u32, name: &'static str, mana: i32, maximum: i32) -> Worn {
    Worn {
        id,
        name,
        mana,
        maximum,
        spells: vec![ITEM_SPELL],
        rate: -0.025,
        appraised: true,
        retained: false,
        valid_locations: RING_SLOT,
        equipped_location: RING_SLOT,
    }
}

struct Rig {
    host: FakeMagHost,
    queue: IdentifyQueue,
    tracker: ManaTracker,
}

impl Rig {
    /// The item spell is in the table, its item-cast enchantment on the
    /// character.
    fn new() -> Self {
        let mut host = FakeMagHost::new();
        host.meta
            .spells
            .insert(ITEM_SPELL, spell(ITEM_SPELL, 5, 100));
        host.meta
            .spells
            .insert(STRONG_BUFF, spell(STRONG_BUFF, 5, 200));
        host.all_enchantments = vec![enchantment(ITEM_SPELL, 5, f64::INFINITY)];
        Self {
            host,
            queue: IdentifyQueue::default(),
            tracker: ManaTracker::default(),
        }
    }

    fn wear(&mut self, item: &Worn) {
        self.host.meta.owned_items.push(InventoryItem {
            object_id: item.id,
            name: item.name.to_owned(),
            wielder_id: ME,
            valid_locations: item.valid_locations,
            equipped_location: item.equipped_location,
            item_current_mana: item.mana,
            item_maximum_mana: item.maximum,
            retained: item.retained,
            ..InventoryItem::default()
        });
        self.host.meta.objects.push(WorldObject {
            object_id: item.id,
            name: item.name.to_owned(),
            wielder_id: ME,
            is_owned: true,
            has_appraisal_data: item.appraised,
            spell_ids: item.spells.clone(),
            ..WorldObject::default()
        });
        let mut properties = ItemProperties::default();
        properties.ints.push((107, item.mana));
        properties.ints.push((108, item.maximum));
        properties.floats.push((5, item.rate));
        self.host.meta.properties.insert(item.id, properties);
    }

    fn login(&mut self) {
        self.tracker.on_login(&mut self.host, &mut self.queue);
    }

    fn change(&mut self, object_id: u32, kind: ObjectChangeKind) {
        let current = self.host.meta.object(object_id);
        let change = ObjectChange {
            object_id,
            kind,
            revision: 1,
            changed_fields: 0,
            current,
        };
        self.tracker
            .on_object_changed(&mut self.host, &change, &mut self.queue);
    }

    fn ident(&mut self, object_id: u32) {
        self.change(object_id, ObjectChangeKind::IdentReceived);
    }

    fn states(&mut self) -> Vec<(String, ManaState)> {
        let now = self.host.now_ms;
        self.tracker
            .rows(&mut self.host, now)
            .into_iter()
            .map(|row| (row.name, row.state))
            .collect()
    }

    fn state(&mut self) -> ManaState {
        let states = self.states();
        assert_eq!(states.len(), 1, "{states:?}");
        states[0].1
    }

    fn chat(&mut self, kind: u32, text: &str) {
        let line = ChatLine {
            kind,
            text: text.to_owned(),
            ..ChatLine::default()
        };
        self.tracker.on_chat(&line, &mut self.queue);
    }
}

#[test]
fn burn_period() {
    // Source: EquipmentTrackedItem.SecondsPerBurn: (int)ceil(-0.2 / rate) * 5.
    assert_eq!(seconds_per_burn(-0.025), 40);
    assert_eq!(seconds_per_burn(-0.033_333_3), 35);
    assert_eq!(seconds_per_burn(0.0), 0);
}

#[test]
fn time_left_text() {
    // Source: ManaTrackerGUI.Item_Changed: "{0:d}h{1:d2}m" of
    // (int)TotalHours and Minutes.
    assert_eq!(time_text(Duration::from_secs(3 * 3600 + 7 * 60)), "3h07m");
    assert_eq!(time_text(Duration::from_secs(0)), "0h00m");
    // new TimeSpan(99, 99, 0), for an item that does not burn.
    assert_eq!(time_text(NO_BURN), "100h39m");
}

#[test]
fn the_state_icons_are_mag_tools() {
    assert_eq!(ManaState::Active.icon(), ICON_ACTIVE);
    assert_eq!(ManaState::NotActive.icon(), ICON_NOT_ACTIVE);
    assert_eq!(ManaState::Unknown.icon(), ICON_UNKNOWN);
    assert_eq!(ManaState::NotActivatable.icon(), ICON_NONE);
    assert_eq!(
        (ICON_ACTIVE, ICON_NOT_ACTIVE, ICON_UNKNOWN, ICON_NONE),
        (0x0600_11F9, 0x0600_11F8, 0x0600_20B5, 0x0600_287A)
    );
}

#[test]
fn an_item_spell_surpassed_by_a_stronger_buff_is_active() {
    // The stronger timed buff is the one in effect; the item's own cast is
    // still on the character (every enchantment, API 1.10), with no end.
    let mut rig = Rig::new();
    rig.host.all_enchantments = vec![
        enchantment(STRONG_BUFF, 5, 1800.0),
        enchantment(ITEM_SPELL, 5, f64::INFINITY),
    ];
    rig.host.meta.enchantments = vec![enchantment(STRONG_BUFF, 5, 1800.0)];
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::Active);
}

#[test]
fn without_the_item_cast_enchantment_it_is_not_active() {
    // Only the timed buff: a stronger spell of the family, but it runs out,
    // so it is no item's cast.
    let mut rig = Rig::new();
    rig.host.all_enchantments = vec![enchantment(STRONG_BUFF, 5, 1800.0)];
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::NotActive);
}

#[test]
fn an_expired_enchantment_counts_as_an_items_cast() {
    // TimeRemaining <= 0: a timed one that ran out reads 0 seconds.
    let mut rig = Rig::new();
    rig.host.all_enchantments = vec![enchantment(STRONG_BUFF, 5, 0.0)];
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::Active);
}

#[test]
fn a_spell_on_the_item_itself_makes_it_active() {
    // The appraisal's spellbook entries with the high bit are the spells
    // active on the item.
    let mut rig = Rig::new();
    rig.host.all_enchantments.clear();
    let mut ring = worn(10, "Test Ring", 100, 200);
    ring.spells = vec![ITEM_SPELL, 0x8000_0000 | STRONG_BUFF];
    rig.wear(&ring);
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::Active);
}

#[test]
fn a_weaker_spell_of_the_family_does_not_count() {
    let mut rig = Rig::new();
    rig.host.meta.spells.insert(50, spell(50, 5, 50));
    rig.host.all_enchantments = vec![enchantment(50, 5, f64::INFINITY)];
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::NotActive);
}

#[test]
fn cast_on_strike_and_the_items_own_spell_are_not_required() {
    let mut rig = Rig::new();
    let mut offensive = spell(300, 9, 10);
    offensive.is_offensive = true;
    let mut debuff = spell(301, 10, 10);
    debuff.is_debuff = true;
    rig.host.meta.spells.insert(300, offensive);
    rig.host.meta.spells.insert(301, debuff);
    rig.host.meta.spells.insert(400, spell(400, 11, 10));
    let mut ring = worn(10, "Test Ring", 100, 200);
    ring.spells = vec![ITEM_SPELL, 300, 301, 400];
    rig.wear(&ring);
    // 400 is the spell the item casts when used (its Spell DID).
    rig.host.meta.owned_items[0].spell_id = 400;
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::Active);

    // Without the Spell DID, 400 must be up too.
    rig.host.meta.owned_items[0].spell_id = 0;
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::NotActive);
}

#[test]
fn unappraised_spell_less_and_empty_items() {
    let mut rig = Rig::new();
    let mut unknown = worn(10, "Unknown Ring", 100, 200);
    unknown.appraised = false;
    let mut plain = worn(11, "Plain Ring", 0, 0);
    plain.spells.clear();
    let empty = worn(12, "Empty Ring", 0, 200);
    rig.wear(&unknown);
    rig.wear(&plain);
    rig.wear(&empty);
    rig.login();
    for id in [10, 11, 12] {
        rig.ident(id);
    }
    assert_eq!(
        rig.states(),
        vec![
            ("Empty Ring".to_owned(), ManaState::NotActive),
            ("Unknown Ring".to_owned(), ManaState::Unknown),
            ("Plain Ring".to_owned(), ManaState::NotActivatable),
        ]
    );
}

#[test]
fn the_state_is_cached_until_the_next_ident() {
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::Active);
    // The item's spell is gone: the row still says Active.
    rig.host.all_enchantments.clear();
    rig.host.now_ms += 10_000.0;
    assert_eq!(rig.state(), ManaState::Active);
    // An appraisal: worked out again.
    rig.ident(10);
    assert_eq!(rig.state(), ManaState::NotActive);
}

#[test]
fn before_its_first_ident_the_state_is_worked_out_when_the_item_changes() {
    // Appraised before it was tracked, none since: the state is worked out
    // when it is tracked, kept while only the character's enchantments
    // change, and worked out again when the item itself changes.
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    assert_eq!(rig.state(), ManaState::Active);
    rig.host.all_enchantments.clear();
    assert_eq!(rig.state(), ManaState::Active);
    rig.change(10, ObjectChangeKind::Updated);
    assert_eq!(rig.state(), ManaState::NotActive);
}

#[test]
fn an_arrived_appraisal_answers_the_queued_request() {
    // Someone else's appraisal (the recharger's, the player's) arrived
    // first: the item is not appraised again.
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    assert!(rig.queue.contains(10));
    rig.ident(10);
    assert!(!rig.queue.contains(10));
}

#[test]
fn the_login_waits_for_the_whole_inventory() {
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.host.complete = false;
    rig.login();
    assert!(rig.tracker.needs_tick());
    rig.tracker.tick(&mut rig.host, &mut rig.queue);
    assert!(rig.states().is_empty());
    assert!(rig.queue.is_empty());
    rig.host.complete = true;
    rig.tracker.tick(&mut rig.host, &mut rig.queue);
    assert_eq!(rig.states().len(), 1);
    assert!(rig.queue.contains(10));
    assert!(!rig.tracker.needs_tick());
}

#[test]
fn a_second_appraisal_restarts_the_burn_clock() {
    // Appraised at 0 s, again at 80 s with the mana still at 100: nothing
    // burned since the second.
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    rig.host.now_ms = 80_000.0;
    rig.ident(10);
    let rows = rig.tracker.rows(&mut rig.host, 80_000.0);
    assert_eq!(rows[0].mana_text, "100 / 200");
}

#[test]
fn mana_burns_down_on_the_burn_grid() {
    // 100 mana, one every 40 s; 85 s after the appraisal two have burned.
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    rig.host.now_ms += 85_000.0;
    let rows = rig.tracker.rows(&mut rig.host, 85_000.0);
    assert_eq!(rows[0].mana_text, "98 / 200");
    // 98 * 40 s = 3920 s.
    assert_eq!(rows[0].time_text, "1h05m");
    assert!((rows[0].sort_key - 3920.0).abs() < f64::EPSILON);
    // Burning never goes below 0.
    let rows = rig.tracker.rows(&mut rig.host, 100_000_000.0);
    assert_eq!(rows[0].mana_text, "0 / 200");
    assert_eq!(rows[0].time_text, "0h00m");
}

#[test]
fn an_item_that_does_not_burn_lasts_100h39m() {
    let mut rig = Rig::new();
    let mut ring = worn(10, "Test Ring", 100, 200);
    ring.rate = 0.0;
    rig.wear(&ring);
    rig.login();
    rig.ident(10);
    let rows = rig.tracker.rows(&mut rig.host, 1_000_000.0);
    assert_eq!(rows[0].mana_text, "100 / 200");
    assert_eq!(rows[0].time_text, "100h39m");
}

#[test]
fn rows_sort_not_active_first_then_by_time_left_then_unknown() {
    let mut rig = Rig::new();
    let mut unknown = worn(10, "Unknown", 100, 200);
    unknown.appraised = false;
    let long = worn(11, "Long", 150, 200);
    let mut none = worn(12, "None", 0, 0);
    none.spells.clear();
    let not_active = worn(13, "Not Active", 0, 200);
    let short = worn(14, "Short", 10, 200);
    for item in [&unknown, &long, &none, &not_active, &short] {
        rig.wear(item);
    }
    rig.login();
    for id in 10..=14 {
        rig.ident(id);
    }
    let rows = rig.tracker.rows(&mut rig.host, 0.0);
    let order: Vec<(&str, &str, &str, u32)> = rows
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                row.mana_text.as_str(),
                row.time_text.as_str(),
                row.state.icon(),
            )
        })
        .collect();
    assert_eq!(
        order,
        vec![
            ("Not Active", "0 / 200", "0h00m", ICON_NOT_ACTIVE),
            ("Short", "10 / 200", "0h06m", ICON_ACTIVE),
            ("Long", "150 / 200", "1h40m", ICON_ACTIVE),
            ("Unknown", "-", "-", ICON_UNKNOWN),
            ("None", "-", "-", ICON_NONE),
        ]
    );
    assert_eq!(rows[0].object_id, 13);
}

#[test]
fn labels() {
    let mut rig = Rig::new();
    let first = worn(10, "First", 100, 200);
    let mut second = worn(11, "Second", 50, 200);
    second.retained = true;
    let third = worn(12, "Third", 150, 200);
    for item in [&first, &second, &third] {
        rig.wear(item);
    }
    rig.login();
    for id in 10..=12 {
        rig.ident(id);
    }
    // 100 + 150 + 50 short of full; the second is retained.
    let needed = rig.tracker.mana_needed(&mut rig.host, 0.0);
    assert_eq!(mana_needed_text(needed), "Mana needed: 300");
    assert_eq!(
        unretained_text(rig.tracker.unretained()),
        "Unretained Items: 2"
    );
}

#[test]
fn only_system_lines_cue_an_identify() {
    // F12: Mag-Tools skipped "You say, " / "says, \"" lines, which an
    // OpenAC line's words never carry; only system lines are read.
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    assert!(rig.queue.contains(10), "identified when tracked");
    rig.queue.clear();

    rig.chat(0, "Your Test Ring is low on Mana.");
    rig.chat(3, "Your Test Ring is out of Mana.");
    assert!(rig.queue.is_empty());
    rig.chat(4, "Your Other Ring is low on Mana.");
    assert!(rig.queue.is_empty());

    rig.chat(4, "Your Test Ring is low on Mana.");
    assert!(rig.queue.contains(10));
    rig.queue.clear();
    rig.chat(4, "Your Test Ring is out of Mana.");
    assert!(rig.queue.contains(10));
    rig.queue.clear();
    rig.chat(
        4,
        "The Mana Stone gives 1,234 points of mana to the following items: Test Ring.\nYour items are fully charged.",
    );
    assert!(rig.queue.contains(10));
    rig.queue.clear();
    rig.chat(4, "The Mana Stone is destroyed.");
    assert!(rig.queue.is_empty());
}

#[test]
fn aetheria_cloaks_and_ammo_are_hidden_but_counted() {
    let mut rig = Rig::new();
    let ring = worn(10, "Test Ring", 100, 200);
    let aetheria = worn(11, "Coalesced Aetheria", 100, 200);
    let mut cloak = worn(12, "Cloak", 100, 200);
    cloak.valid_locations = 0x0800_0000;
    cloak.equipped_location = 0x0800_0000;
    let mut arrows = worn(13, "Arrows", 100, 200);
    arrows.valid_locations = 0x0080_0000;
    arrows.equipped_location = 0x0080_0000;
    for item in [&ring, &aetheria, &cloak, &arrows] {
        rig.wear(item);
    }
    rig.login();
    // Each was asked for an appraisal.
    for id in 10..=13 {
        assert!(rig.queue.contains(id), "{id}");
    }
    for id in 10..=13 {
        rig.ident(id);
    }
    assert_eq!(
        rig.states(),
        vec![("Test Ring".to_owned(), ManaState::Active)]
    );
    // Every one counts in the mana needed; the ammunition is not counted
    // as unretained.
    assert_eq!(rig.tracker.mana_needed(&mut rig.host, 0.0), 400);
    assert_eq!(rig.tracker.unretained(), 3);
}

#[test]
fn only_items_the_character_wears_are_tracked() {
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Worn Ring", 100, 200));
    rig.wear(&worn(11, "Packed Ring", 100, 200));
    rig.host.meta.owned_items[1].equipped_location = 0;
    rig.wear(&worn(12, "Someone's Ring", 100, 200));
    rig.host.meta.owned_items[2].wielder_id = 99;
    rig.login();
    assert_eq!(
        rig.states()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["Worn Ring"]
    );
}

#[test]
fn equipping_releasing_and_logging_off_change_the_list() {
    let mut rig = Rig::new();
    rig.login();
    assert!(rig.states().is_empty());

    // Put on: looked at on the next tick, appraised.
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.tracker.on_equipment(&EquipmentObservation {
        object_id: 10,
        equipped_location: RING_SLOT,
        is_removal: false,
        is_initial_placement: false,
    });
    rig.tracker.tick(&mut rig.host, &mut rig.queue);
    assert_eq!(rig.states().len(), 1);
    assert!(rig.queue.contains(10));

    // Taken off.
    rig.tracker.on_equipment(&EquipmentObservation {
        object_id: 10,
        equipped_location: RING_SLOT,
        is_removal: true,
        is_initial_placement: false,
    });
    assert!(rig.states().is_empty());

    // Created already worn; then released.
    rig.change(10, ObjectChangeKind::Created);
    assert_eq!(rig.states().len(), 1);
    rig.change(10, ObjectChangeKind::Released);
    assert!(rig.states().is_empty());

    rig.change(10, ObjectChangeKind::Created);
    assert_eq!(rig.states().len(), 1);
    rig.tracker.reset();
    assert!(rig.states().is_empty());
}

#[test]
fn the_view_is_built_once_a_moment() {
    let mut rig = Rig::new();
    rig.wear(&worn(10, "Test Ring", 100, 200));
    rig.login();
    rig.ident(10);
    rig.tracker.refresh(&mut rig.host);
    let view = rig.tracker.view();
    assert_eq!(view.names(), ["Test Ring"]);
    assert_eq!(view.object_ids(), [10]);
    assert_eq!(view.state_icons(), [ICON_ACTIVE]);
    assert_eq!(view.mana_texts(), ["100 / 200"]);
    assert_eq!(view.time_texts(), ["1h06m"]);
    assert_eq!(view.mana_needed_text(), "Mana needed: 100");
    assert_eq!(view.unretained_text(), "Unretained Items: 1");
    // 40 s later one mana burned.
    rig.host.now_ms += 40_000.0;
    rig.tracker.refresh(&mut rig.host);
    assert_eq!(rig.tracker.view().mana_texts(), ["99 / 200"]);
}
