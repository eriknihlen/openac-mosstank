//! The combat tracker's two lists (Mag-Tools' `CombatTrackerGUI` and
//! `CombatTrackerGUIInfo` at 43ee2ec) as rows of strings, with the fixes
//! F7 (ordinal sort, the selection follows its name), F8 (each Typeless
//! cell its own total) and F9 (a zero divisor prints `(0%)`).

use mosstank_mag::combat::gui::{
    CombatGui, DETAIL_ALIGN_RIGHT, DETAIL_WIDTHS, MONSTER_ALIGN_RIGHT, MONSTER_WIDTHS,
};
use mosstank_mag::combat::info::{AttackType, CombatEvent, DamageElement, SurgeEvent};
use mosstank_mag::combat::xml::{self, StagedImport};
use mosstank_mag::combat::{Combat, CombatSettings, CombatTracker};
use mosstank_mag::testing::FakeMagHost;
use mosstank_meta::host::MetaHost;

const ME: &str = "Acdream";

fn row(cells: [&str; 5]) -> [String; 5] {
    cells.map(str::to_owned)
}

fn hit(
    source: &str,
    target: &str,
    kind: AttackType,
    element: DamageElement,
    points: i32,
) -> CombatEvent {
    CombatEvent::damage(source, target, kind, element, false, points)
}

fn melee(target: &str, points: i32) -> CombatEvent {
    hit(
        ME,
        target,
        AttackType::MeleeMissle,
        DamageElement::Slash,
        points,
    )
}

fn crit(target: &str, points: i32) -> CombatEvent {
    CombatEvent::damage(
        ME,
        target,
        AttackType::MeleeMissle,
        DamageElement::Slash,
        true,
        points,
    )
}

/// The lists after `events`, sorting off, "All" selected.
fn gui_after(events: &[CombatEvent]) -> (CombatGui, CombatTracker) {
    let mut tracker = CombatTracker::default();
    for event in events {
        tracker.add_combat(ME, event);
    }
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    (gui, tracker)
}

/// The detail list's value at (`row`, `column`).
fn detail(gui: &CombatGui, row: usize, column: usize) -> String {
    gui.detail_rows()[row][column].clone()
}

#[test]
fn the_columns_are_mag_tools_widths_and_alignments() {
    assert_eq!(MONSTER_WIDTHS, [5, 111, 37, 55, 77]);
    assert_eq!(DETAIL_WIDTHS, [40, 52, 52, 45, 96]);
    assert_eq!(MONSTER_ALIGN_RIGHT, [false, false, true, true, true]);
    assert_eq!(DETAIL_ALIGN_RIGHT, [false, true, true, false, true]);
}

#[test]
fn an_empty_list_has_the_header_and_the_all_row_selected() {
    let gui = CombatGui::default();
    assert_eq!(
        gui.rows(),
        vec![
            row(["", "", "KB's", "Dmg Rcvd", "Dmg Givn"]),
            row(["*", "All", "", "", ""]),
        ]
    );
    assert_eq!(gui.selected_row(), 1);
}

#[test]
fn the_detail_list_has_its_labels_and_no_values() {
    let gui = CombatGui::default();
    assert_eq!(
        gui.detail_rows(),
        vec![
            row(["", "Mel/Msl", "Magic", "Attacks", ""]),
            row(["Typeless", "", "", "Evades", ""]),
            row(["Slash", "", "", "Resists", ""]),
            row(["Pierce", "", "", "A.Surges", ""]),
            row(["Bludge", "", "", "C.Surges", ""]),
            row(["Fire", "", "", "", ""]),
            row(["Cold", "", "", "Av/Mx", ""]),
            row(["Acid", "", "", "Crits", ""]),
            row(["Electric", "", "", "Av/Mx", ""]),
            row(["", "", "", "", ""]),
            row(["Total", "", "", "Total", ""]),
        ]
    );
}

#[test]
fn a_monster_row_counts_kills_damage_received_and_given_through_the_number_formatter() {
    let (gui, _) = gui_after(&[
        melee("Drudge Skulker", 60_000_000),
        melee("Drudge Skulker", 60_000_000),
        CombatEvent::killing_blow(ME, "Drudge Skulker"),
        hit(
            "Drudge Skulker",
            ME,
            AttackType::MeleeMissle,
            DamageElement::Pierce,
            12_345,
        ),
    ]);
    // 120,000,000 is past 99,999,999: in thousands.
    assert_eq!(
        gui.rows(),
        vec![
            row(["", "", "KB's", "Dmg Rcvd", "Dmg Givn"]),
            row(["*", "All", "1", "12,345", "120,000k"]),
            row(["", "Drudge Skulker", "1", "12,345", "120,000k"]),
        ]
    );
}

#[test]
fn a_cell_stays_empty_while_its_value_is_zero() {
    let (gui, _) = gui_after(&[CombatEvent::failed(
        ME,
        "Drudge Skulker",
        AttackType::MeleeMissle,
    )]);
    assert_eq!(gui.rows()[1], row(["*", "All", "", "", ""]));
    assert_eq!(gui.rows()[2], row(["", "Drudge Skulker", "", "", ""]));
    // Attacks are counted, nothing else.
    assert_eq!(detail(&gui, 0, 4), "1 (0%)");
    assert_eq!(detail(&gui, 10, 4), "");
}

#[test]
fn we_are_never_a_row_of_our_own() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(
        ME,
        &hit(ME, ME, AttackType::Magic, DamageElement::Typeless, 5),
    );
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    tracker.add_cloak(ME, &SurgeEvent::new(ME, ""));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    assert_eq!(gui.rows().len(), 2);
    // (me, me) counts as given only.
    assert_eq!(gui.rows()[1], row(["*", "All", "", "", "5"]));
}

#[test]
fn the_attacks_cell_shows_the_hit_percentage() {
    // 10 attacks, 3 evaded: 70% hit.
    let mut events: Vec<CombatEvent> = [40, 20, 20, 20, 20, 20, 21]
        .iter()
        .map(|points| melee("Drudge Skulker", *points))
        .collect();
    for _ in 0..3 {
        events.push(CombatEvent::failed(
            ME,
            "Drudge Skulker",
            AttackType::MeleeMissle,
        ));
    }
    let (gui, _) = gui_after(&events);
    assert_eq!(detail(&gui, 0, 4), "10 (70%)");
    // 161 over 7 hits, truncated: 23; the largest 40.
    assert_eq!(detail(&gui, 6, 4), "23 / 40");
    assert_eq!(detail(&gui, 2, 1), "");
    assert_eq!(detail(&gui, 10, 4), "161");
}

#[test]
fn crits_show_their_share_of_the_hits_and_their_average() {
    let mut events: Vec<CombatEvent> = (0..5).map(|_| melee("Drudge Skulker", 10)).collect();
    events.push(crit("Drudge Skulker", 30));
    events.push(crit("Drudge Skulker", 45));
    events.push(CombatEvent::killing_blow(ME, "Drudge Skulker"));
    let (gui, _) = gui_after(&events);
    // 2 of the 7 hits (the kill not counted): 28.571... -> 28.6.
    assert_eq!(detail(&gui, 7, 4), "2 (28.6%)");
    // 75 over 2: 37; the largest 45.
    assert_eq!(detail(&gui, 8, 4), "37 / 45");
    assert_eq!(detail(&gui, 6, 4), "10 / 10");
    assert_eq!(detail(&gui, 0, 4), "8 (100%)");
}

#[test]
fn the_percentages_round_half_to_even() {
    // 5 of 8 hit: 62.5% -> 62.
    let mut events: Vec<CombatEvent> = (0..5).map(|_| melee("Drudge Skulker", 10)).collect();
    for _ in 0..3 {
        events.push(CombatEvent::failed(
            ME,
            "Drudge Skulker",
            AttackType::MeleeMissle,
        ));
    }
    let (gui, _) = gui_after(&events);
    assert_eq!(detail(&gui, 0, 4), "8 (62%)");
}

#[test]
fn the_received_block_is_by_attack_type_and_element() {
    let (gui, _) = gui_after(&[
        hit(
            "Drudge Skulker",
            ME,
            AttackType::MeleeMissle,
            DamageElement::Slash,
            1_000,
        ),
        hit(
            "Drudge Skulker",
            ME,
            AttackType::MeleeMissle,
            DamageElement::Slash,
            234,
        ),
        hit(
            "Drudge Skulker",
            ME,
            AttackType::Magic,
            DamageElement::Fire,
            50,
        ),
        hit(
            "Drudge Skulker",
            ME,
            AttackType::Magic,
            DamageElement::Electric,
            7,
        ),
        CombatEvent::failed("Drudge Skulker", ME, AttackType::MeleeMissle),
        CombatEvent::failed("Drudge Skulker", ME, AttackType::Magic),
    ]);
    assert_eq!(detail(&gui, 2, 1), "1,234");
    assert_eq!(detail(&gui, 5, 2), "50");
    assert_eq!(detail(&gui, 8, 2), "7");
    assert_eq!(detail(&gui, 10, 1), "1,234");
    assert_eq!(detail(&gui, 10, 2), "57");
    // Three melee attacks on me, one evaded; three spells, one resisted.
    assert_eq!(detail(&gui, 1, 4), "3 (33%)");
    assert_eq!(detail(&gui, 2, 4), "3 (33%)");
    // Nothing given.
    assert_eq!(detail(&gui, 0, 4), "");
    assert_eq!(detail(&gui, 10, 4), "");
}

#[test]
fn each_typeless_cell_shows_its_own_total() {
    // F8: Mag-Tools showed the magic total under Mel/Msl and nothing under
    // Magic.
    let (gui, _) = gui_after(&[
        hit(
            "Drudge Skulker",
            ME,
            AttackType::MeleeMissle,
            DamageElement::Typeless,
            15,
        ),
        hit(
            "Drudge Skulker",
            ME,
            AttackType::Magic,
            DamageElement::Typeless,
            30,
        ),
    ]);
    assert_eq!(detail(&gui, 1, 1), "15");
    assert_eq!(detail(&gui, 1, 2), "30");
}

#[test]
fn a_surge_with_no_attacks_shows_zero_percent() {
    // F9: Mag-Tools printed "1 (Infinity%)".
    let mut tracker = CombatTracker::default();
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    tracker.add_cloak(ME, &SurgeEvent::new(ME, ME));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    assert_eq!(detail(&gui, 3, 4), "1 (0.0%)");
    assert_eq!(detail(&gui, 4, 4), "1 (0.0%)");
}

#[test]
fn surges_count_against_the_attacks_and_the_hits_received() {
    let mut tracker = CombatTracker::default();
    for _ in 0..3 {
        tracker.add_combat(ME, &melee("Drudge Skulker", 10));
    }
    for _ in 0..4 {
        tracker.add_combat(
            ME,
            &hit(
                "Drudge Skulker",
                ME,
                AttackType::MeleeMissle,
                DamageElement::Slash,
                5,
            ),
        );
    }
    tracker.add_combat(
        ME,
        &CombatEvent::failed("Drudge Skulker", ME, AttackType::MeleeMissle),
    );
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    tracker.add_cloak(ME, &SurgeEvent::new(ME, ME));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    // 1 of 3 attacks: 33.3%; 1 of 4 hits received (the evade not
    // counted): 25.0%.
    assert_eq!(detail(&gui, 3, 4), "1 (33.3%)");
    assert_eq!(detail(&gui, 4, 4), "1 (25.0%)");
}

#[test]
fn sorting_is_ordinal_and_the_selection_follows_its_name() {
    // F7: Mag-Tools swapped only when CompareOrdinal returned exactly 1
    // ("Zombie" against "Drudge" is 22: no swap) and kept the row number
    // selected.
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &melee("Zombie", 10));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, true);
    gui.click(&mut tracker, ME, true, 2);
    assert_eq!(gui.selected_name(), "Zombie");
    tracker.add_combat(ME, &melee("Drudge", 7));
    gui.update(&mut tracker, ME, true);
    let names: Vec<String> = gui
        .rows()
        .into_iter()
        .map(|cells| cells[1].clone())
        .collect();
    assert_eq!(names, ["", "All", "Drudge", "Zombie"]);
    assert_eq!(gui.rows()[3], row(["*", "Zombie", "", "", "10"]));
    assert_eq!(gui.rows()[2][0], "");
    assert_eq!(gui.selected_row(), 3);
    assert_eq!(gui.selected_name(), "Zombie");
    // The detail list is still Zombie's.
    assert_eq!(detail(&gui, 10, 4), "10");
}

#[test]
fn sorting_is_by_code_unit_not_by_case() {
    let mut tracker = CombatTracker::default();
    for name in ["drudge", "Zombie", "Banderling", "Áerfalle"] {
        tracker.add_combat(ME, &melee(name, 1));
    }
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, true);
    let names: Vec<String> = gui
        .rows()
        .into_iter()
        .skip(2)
        .map(|cells| cells[1].clone())
        .collect();
    assert_eq!(names, ["Banderling", "Zombie", "drudge", "Áerfalle"]);
}

#[test]
fn without_sorting_rows_stay_in_the_order_first_seen() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &melee("Zombie", 10));
    tracker.add_combat(ME, &melee("Drudge", 7));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    let names: Vec<String> = gui
        .rows()
        .into_iter()
        .skip(2)
        .map(|cells| cells[1].clone())
        .collect();
    assert_eq!(names, ["Zombie", "Drudge"]);
    // Turning the option on sorts at the next new row, not before.
    tracker.add_combat(ME, &melee("Zombie", 1));
    gui.update(&mut tracker, ME, true);
    let names: Vec<String> = gui
        .rows()
        .into_iter()
        .skip(2)
        .map(|cells| cells[1].clone())
        .collect();
    assert_eq!(names, ["Zombie", "Drudge"]);
    tracker.add_combat(ME, &melee("Mosswart", 1));
    gui.update(&mut tracker, ME, true);
    let names: Vec<String> = gui
        .rows()
        .into_iter()
        .skip(2)
        .map(|cells| cells[1].clone())
        .collect();
    assert_eq!(names, ["Drudge", "Mosswart", "Zombie"]);
}

#[test]
fn a_click_selects_a_monster_and_the_header_selects_all() {
    let (mut gui, mut tracker) = gui_after(&[melee("Zombie", 10), melee("Drudge", 7)]);
    assert_eq!(detail(&gui, 10, 4), "17");
    gui.click(&mut tracker, ME, false, 3);
    assert_eq!(gui.rows()[1][0], "");
    assert_eq!(gui.rows()[3][0], "*");
    assert_eq!(detail(&gui, 10, 4), "7");
    gui.click(&mut tracker, ME, false, 0);
    assert_eq!(gui.selected_row(), 1);
    assert_eq!(gui.rows()[1][0], "*");
    assert_eq!(gui.rows()[3][0], "");
    assert_eq!(detail(&gui, 10, 4), "17");
    // A row past the list is no row.
    gui.click(&mut tracker, ME, false, 9);
    assert_eq!(gui.selected_row(), 1);
}

#[test]
fn clear_current_stats_empties_the_list_but_the_all_row() {
    let (mut gui, mut tracker) = gui_after(&[
        melee("Zombie", 10),
        hit("Zombie", ME, AttackType::Magic, DamageElement::Cold, 3),
    ]);
    gui.click(&mut tracker, ME, false, 2);
    tracker.clear();
    gui.update(&mut tracker, ME, false);
    assert_eq!(
        gui.rows(),
        vec![
            row(["", "", "KB's", "Dmg Rcvd", "Dmg Givn"]),
            row(["*", "All", "", "", ""]),
        ]
    );
    assert_eq!(gui.detail_rows(), CombatGui::default().detail_rows());
    assert_eq!(gui.selected_row(), 1);
}

#[test]
fn the_guarded_cells_keep_their_text_until_the_next_clear() {
    // Mag-Tools set Av/Mx, Crits and the crits' Av/Mx only when their
    // divisor is not zero: a file read in with fewer hits leaves them.
    let mut host = FakeMagHost::new();
    let key = "Mag-Tools/Coldeve/Acdream.CombatTracker.xml";
    let mut file = CombatTracker::default();
    file.add_combat(
        ME,
        &CombatEvent::failed(ME, "Zombie", AttackType::MeleeMissle),
    );
    host.storage().write_text(key, &xml::export(&file)).unwrap();

    let (mut gui, mut tracker) = gui_after(&[melee("Zombie", 25), crit("Zombie", 60)]);
    assert_eq!(detail(&gui, 6, 4), "25 / 25");
    assert_eq!(detail(&gui, 7, 4), "1 (50.0%)");
    let mut import = StagedImport::new(key.to_owned());
    while !import.step(&mut host, &mut tracker) {}
    gui.update(&mut tracker, ME, false);
    assert_eq!(detail(&gui, 0, 4), "1 (0%)");
    assert_eq!(detail(&gui, 10, 4), "");
    assert_eq!(detail(&gui, 6, 4), "25 / 25");
    assert_eq!(detail(&gui, 7, 4), "1 (50.0%)");
    assert_eq!(detail(&gui, 8, 4), "60 / 60");
    // And the monster row keeps its damage (written only when not zero).
    assert_eq!(gui.rows()[2], row(["", "Zombie", "", "", "85"]));
    // A click clears them.
    gui.click(&mut tracker, ME, false, 2);
    assert_eq!(detail(&gui, 6, 4), "");
    assert_eq!(detail(&gui, 7, 4), "");
}

#[test]
fn a_file_read_in_adds_its_names_and_sorts_once() {
    let mut host = FakeMagHost::new();
    let key = "Mag-Tools/Coldeve/Acdream.CombatTracker.xml";
    let mut file = CombatTracker::default();
    file.add_combat(ME, &melee("Zombie", 4));
    file.add_combat(
        ME,
        &hit(
            "Banderling",
            ME,
            AttackType::MeleeMissle,
            DamageElement::Acid,
            9,
        ),
    );
    host.storage().write_text(key, &xml::export(&file)).unwrap();

    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &melee("Mosswart", 2));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, true);
    let mut import = StagedImport::new(key.to_owned());
    while !import.step(&mut host, &mut tracker) {}
    gui.update(&mut tracker, ME, true);
    // The row of before stays (with its text), the file's are added, all
    // sorted.
    assert_eq!(
        gui.rows(),
        vec![
            row(["", "", "KB's", "Dmg Rcvd", "Dmg Givn"]),
            row(["*", "All", "", "9", "4"]),
            row(["", "Banderling", "", "9", ""]),
            row(["", "Mosswart", "", "", "2"]),
            row(["", "Zombie", "", "", "4"]),
        ]
    );
}

#[test]
fn the_values_are_worked_out_only_when_asked() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &melee("Zombie", 10));
    let mut gui = CombatGui::default();
    gui.follow(&mut tracker, ME, false);
    // The row is there; its numbers wait for the refresh.
    assert_eq!(gui.rows()[2], row(["", "Zombie", "", "", ""]));
    assert!(gui.needs_refresh());
    gui.refresh(&tracker, ME);
    assert_eq!(gui.rows()[2], row(["", "Zombie", "", "", "10"]));
    assert!(!gui.needs_refresh());
}

// Clear Persistent Stats

fn ready(host: &mut FakeMagHost) -> Combat {
    let mut combat = Combat::default();
    while !combat.is_ready() {
        combat.tick(host, CombatSettings::default());
    }
    combat
}

fn hit_line(text: &str) -> ac_plugin_abi::ChatLine {
    ac_plugin_abi::ChatLine {
        kind: 8,
        combat_kind: 1,
        text: text.to_owned(),
        display_text: text.to_owned(),
        ..ac_plugin_abi::ChatLine::default()
    }
}

/// The fake's character is "Meta Tester" on Coldeve.
const PERSISTENT: &str = "Mag-Tools/Coldeve/Meta Tester.CombatTracker.xml";

#[test]
fn clear_persistent_stats_clears_them_and_deletes_their_file() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(
            PERSISTENT,
            &xml::export(&{
                let mut file = CombatTracker::default();
                file.add_combat(
                    "Meta Tester",
                    &CombatEvent::killing_blow("Meta Tester", "Zombie"),
                );
                file
            }),
        )
        .unwrap();
    let settings = CombatSettings {
        persistent: true,
        ..CombatSettings::default()
    };
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings);
    while combat.is_importing() {
        combat.tick(&mut host, settings);
    }
    combat.on_chat(
        &mut host,
        &hit_line("You hit Drudge Skulker for 25 points of slashing damage!"),
    );
    assert_eq!(combat.persistent.combat_infos().len(), 2);

    combat.clear_persistent(&mut host);
    assert!(combat.persistent.is_empty());
    assert_eq!(combat.current.combat_infos().len(), 1);
    assert_eq!(host.storage().read_text(PERSISTENT).unwrap(), None);
    assert_eq!(
        host.posted_text().last().map(String::as_str),
        Some(format!("<{{Mag-Tools}}>: File deleted: {PERSISTENT}").as_str())
    );
    // No file: nothing said.
    let said = host.posted_text().len();
    combat.clear_persistent(&mut host);
    assert_eq!(host.posted_text().len(), said);
    // The persistent page follows.
    combat.update_pages(&mut host, false);
    assert_eq!(combat.persistent_gui().rows().len(), 2);
}

#[test]
fn clear_current_stats_leaves_the_persistent_ones() {
    let mut host = FakeMagHost::new();
    let mut combat = ready(&mut host);
    combat.on_chat(
        &mut host,
        &hit_line("You hit Drudge Skulker for 25 points of slashing damage!"),
    );
    combat.update_pages(&mut host, false);
    assert_eq!(combat.current_gui().rows().len(), 3);
    combat.clear_current();
    combat.update_pages(&mut host, false);
    assert_eq!(combat.current_gui().rows().len(), 2);
    assert_eq!(combat.persistent_gui().rows().len(), 3);
    assert_eq!(combat.persistent.combat_infos().len(), 1);
}

#[test]
fn clearing_while_the_file_is_read_in_stops_the_read() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(
            PERSISTENT,
            &xml::export(&{
                let mut file = CombatTracker::default();
                file.add_combat(
                    "Meta Tester",
                    &CombatEvent::killing_blow("Meta Tester", "Zombie"),
                );
                file
            }),
        )
        .unwrap();
    let settings = CombatSettings {
        persistent: true,
        ..CombatSettings::default()
    };
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings);
    assert!(combat.is_importing());
    combat.clear_persistent(&mut host);
    assert!(!combat.is_importing());
    for _ in 0..5 {
        combat.tick(&mut host, settings);
    }
    assert!(combat.persistent.is_empty());
}

#[test]
fn a_files_rows_are_worked_out_a_part_at_a_time() {
    let mut host = FakeMagHost::new();
    let key = "Mag-Tools/Coldeve/Acdream.CombatTracker.xml";
    let mut file = CombatTracker::default();
    for name in ["Zombie", "Drudge", "Mosswart"] {
        file.add_combat(ME, &melee(name, 4));
    }
    host.storage().write_text(key, &xml::export(&file)).unwrap();
    let mut tracker = CombatTracker::default();
    let mut import = StagedImport::new(key.to_owned());
    while !import.step(&mut host, &mut tracker) {}
    let mut gui = CombatGui::default();
    gui.follow(&mut tracker, ME, false);
    gui.refresh_some(&tracker, ME, 2);
    let given: Vec<String> = gui
        .rows()
        .into_iter()
        .map(|cells| cells[4].clone())
        .collect();
    assert_eq!(given, ["Dmg Givn", "", "4", "4", ""]);
    assert!(gui.needs_refresh());
    gui.refresh_some(&tracker, ME, 2);
    let given: Vec<String> = gui
        .rows()
        .into_iter()
        .map(|cells| cells[4].clone())
        .collect();
    assert_eq!(given, ["Dmg Givn", "12", "4", "4", "4"]);
    assert_eq!(detail(&gui, 10, 4), "12");
    assert!(!gui.needs_refresh());
}

// The review's fixes (Phase 8b)

#[test]
fn a_sort_while_a_files_rows_are_worked_out_starts_them_again() {
    let mut host = FakeMagHost::new();
    let key = "Mag-Tools/Coldeve/Acdream.CombatTracker.xml";
    let mut file = CombatTracker::default();
    for name in ["Zombie", "Yeti", "Drudge"] {
        file.add_combat(ME, &melee(name, 4));
    }
    host.storage().write_text(key, &xml::export(&file)).unwrap();
    let mut tracker = CombatTracker::default();
    let mut import = StagedImport::new(key.to_owned());
    while !import.step(&mut host, &mut tracker) {}
    let mut gui = CombatGui::default();
    gui.follow(&mut tracker, ME, false);
    // Zombie and Yeti worked out; Drudge waits.
    gui.refresh_some(&tracker, ME, 2);
    // A new row sorts Drudge above the rows still waiting.
    tracker.add_combat(ME, &melee("Abc", 1));
    gui.follow(&mut tracker, ME, true);
    gui.refresh(&tracker, ME);
    assert_eq!(
        gui.rows()[2..],
        [
            row(["", "Abc", "", "", "1"]),
            row(["", "Drudge", "", "", "4"]),
            row(["", "Yeti", "", "", "4"]),
            row(["", "Zombie", "", "", "4"]),
        ]
    );
}

#[test]
fn a_clear_or_a_load_after_an_overflow_is_kept() {
    let mut tracker = CombatTracker::default();
    for _ in 0..=mosstank_mag::combat::UPDATES_MAX {
        tracker.add_combat(ME, &melee("Drudge", 1));
    }
    tracker.clear();
    use mosstank_mag::combat::TrackerUpdate;
    assert_eq!(
        tracker.take_updates(),
        [TrackerUpdate::Cleared, TrackerUpdate::Overflowed]
    );

    let mut host = FakeMagHost::new();
    for _ in 0..=mosstank_mag::combat::UPDATES_MAX {
        tracker.add_combat(ME, &melee("Drudge", 1));
    }
    let mut import = StagedImport::new("none".to_owned());
    while !import.step(&mut host, &mut tracker) {}
    assert_eq!(
        tracker.take_updates(),
        [TrackerUpdate::Loaded, TrackerUpdate::Overflowed]
    );

    // A load after a clear after an overflow: both.
    for _ in 0..=mosstank_mag::combat::UPDATES_MAX {
        tracker.add_combat(ME, &melee("Drudge", 1));
    }
    tracker.clear();
    let mut import = StagedImport::new("none".to_owned());
    while !import.step(&mut host, &mut tracker) {}
    assert_eq!(
        tracker.take_updates(),
        [
            TrackerUpdate::Cleared,
            TrackerUpdate::Loaded,
            TrackerUpdate::Overflowed
        ]
    );
}

#[test]
fn a_clear_after_an_overflow_empties_the_list() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &melee("Zombie", 1));
    let mut gui = CombatGui::default();
    gui.update(&mut tracker, ME, false);
    for _ in 0..=mosstank_mag::combat::UPDATES_MAX {
        tracker.add_combat(ME, &melee("Drudge", 1));
    }
    tracker.clear();
    gui.update(&mut tracker, ME, false);
    assert_eq!(gui.rows().len(), 2);
}

/// A combat with its patterns not yet compiled.
fn compiling() -> Combat {
    Combat::default()
}

fn drain(combat: &mut Combat, host: &mut FakeMagHost, settings: CombatSettings) {
    for _ in 0..200 {
        combat.tick(host, settings);
    }
}

const HIT: &str = "You hit Drudge Skulker for 25 points of slashing damage!";

#[test]
fn a_line_waiting_for_the_patterns_is_not_counted_in_a_tracker_cleared_since() {
    let mut host = FakeMagHost::new();
    let mut combat = compiling();
    combat.on_chat(&mut host, &hit_line(HIT));
    combat.clear_current();
    drain(&mut combat, &mut host, CombatSettings::default());
    assert!(combat.current.is_empty());
    assert_eq!(combat.persistent.combat_infos().len(), 1);

    let mut combat = compiling();
    combat.on_chat(&mut host, &hit_line(HIT));
    combat.clear_persistent(&mut host);
    drain(&mut combat, &mut host, CombatSettings::default());
    assert!(combat.persistent.is_empty());
    assert_eq!(combat.current.combat_infos().len(), 1);
    // A line heard after the clear is counted.
    combat.on_chat(&mut host, &hit_line(HIT));
    assert_eq!(combat.persistent.combat_infos().len(), 1);
}

#[test]
fn a_line_heard_before_the_login_is_not_counted_when_the_import_is_stopped() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(
            PERSISTENT,
            &xml::export(&{
                let mut file = CombatTracker::default();
                file.add_combat(
                    "Meta Tester",
                    &CombatEvent::killing_blow("Meta Tester", "Zombie"),
                );
                file
            }),
        )
        .unwrap();
    let settings = CombatSettings {
        persistent: true,
        ..CombatSettings::default()
    };
    let mut combat = compiling();
    combat.on_chat(&mut host, &hit_line(HIT));
    combat.on_login_complete(&mut host, settings);
    combat.clear_persistent(&mut host);
    drain(&mut combat, &mut host, settings);
    // The import would have replaced it; the clear stopped the import.
    assert!(combat.persistent.is_empty());
    assert_eq!(combat.current.combat_infos().len(), 1);
}

#[test]
fn a_row_heard_before_the_login_keeps_its_numbers_after_the_file_is_read_in() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(
            PERSISTENT,
            &xml::export(&{
                let mut file = CombatTracker::default();
                file.add_combat("Meta Tester", &melee_of("Meta Tester", "Zombie", 4));
                file
            }),
        )
        .unwrap();
    let settings = CombatSettings {
        persistent: true,
        ..CombatSettings::default()
    };
    let mut combat = ready(&mut host);
    combat.on_chat(&mut host, &hit_line(HIT));
    combat.on_login_complete(&mut host, settings);
    while combat.is_importing() {
        combat.tick(&mut host, settings);
    }
    combat.update_pages(&mut host, false);
    // Mag-Tools drew the row as the line came; the file, which lacks it,
    // writes no zeros over it.
    assert_eq!(
        combat.persistent_gui().rows()[2..],
        [
            row(["", "Drudge Skulker", "", "", "25"]),
            row(["", "Zombie", "", "", "4"]),
        ]
    );
}

fn melee_of(source: &str, target: &str, points: i32) -> CombatEvent {
    hit(
        source,
        target,
        AttackType::MeleeMissle,
        DamageElement::Slash,
        points,
    )
}
