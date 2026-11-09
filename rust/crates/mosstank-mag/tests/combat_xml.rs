//! The combat stats files: Mag-Tools' format byte for byte, the import
//! and its failures, their keys, and when the plugin reads and writes
//! them (login, logoff, the 10-minute save, the export button).

use ac_plugin_abi::{ChatLine, LogLevel};
use mosstank_mag::combat::info::{AttackType, CombatEvent, DamageElement, SurgeEvent};
use mosstank_mag::combat::xml::{self, ImportError, Imported, StagedImport};
use mosstank_mag::combat::{
    Combat, CombatSettings, CombatTracker, SAVE_INTERVAL_MS, TrackerUpdate,
};
use mosstank_mag::testing::{FakeMagHost, SharedStorage};
use mosstank_meta::host::MetaHost;
use openac_plugin_compat::datetime::DateTime;

const ME: &str = "Acdream";

fn slash(failed: bool, points: i32) -> CombatEvent {
    if failed {
        CombatEvent::failed(ME, "Drudge Skulker", AttackType::MeleeMissle)
    } else {
        CombatEvent::damage(
            ME,
            "Drudge Skulker",
            AttackType::MeleeMissle,
            DamageElement::Slash,
            false,
            points,
        )
    }
}

/// Slash: 3 attacks, 1 evaded, two hits of 25 (the evade in the Slash
/// bucket, as the plan's example writes it).
fn one_info() -> CombatTracker {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &slash(false, 25));
    tracker.add_combat(ME, &slash(false, 25));
    tracker.add_combat(
        ME,
        &CombatEvent {
            is_failed: true,
            ..slash(false, 0)
        },
    );
    tracker
}

#[test]
fn one_info_is_written_byte_for_byte() {
    assert_eq!(
        xml::export(&one_info()),
        "<CombatTracker>\r\n  <CombatInfos>\r\n    <CombatInfo SourceName=\"Acdream\" TargetName=\"Drudge Skulker\">\r\n      <MeleeMissle>\r\n        <Slash TotalAttacks=\"3\" FailedAttacks=\"1\" TotalNormalDamage=\"50\" MaxNormalDamage=\"25\" />\r\n      </MeleeMissle>\r\n    </CombatInfo>\r\n  </CombatInfos>\r\n</CombatTracker>"
    );
}

/// Every section and attribute: a kill, crits, an aetheria and a cloak
/// surge, names to escape.
fn full() -> CombatTracker {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &CombatEvent::killing_blow(ME, "Tom & \"Jerry\" <1>'s"));
    tracker.add_combat(
        ME,
        &CombatEvent::damage(
            "Tom & \"Jerry\" <1>'s",
            ME,
            AttackType::Magic,
            DamageElement::Fire,
            true,
            90,
        ),
    );
    tracker.add_combat(
        ME,
        &CombatEvent::damage(
            "Tom & \"Jerry\" <1>'s",
            ME,
            AttackType::Magic,
            DamageElement::Cold,
            false,
            7,
        ),
    );
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    tracker.add_cloak(ME, &SurgeEvent::new(ME, ""));
    tracker.add_cloak(ME, &SurgeEvent::new(ME, ""));
    tracker
}

#[test]
fn every_section_and_attribute_is_written() {
    let name = "Tom &amp; &quot;Jerry&quot; &lt;1&gt;'s";
    let expected = [
        "<CombatTracker>".to_owned(),
        "  <CombatInfos>".to_owned(),
        format!("    <CombatInfo SourceName=\"Acdream\" TargetName=\"{name}\" KillingBlows=\"1\">"),
        "      <Unknown>".to_owned(),
        "        <Unknown TotalAttacks=\"1\" />".to_owned(),
        "      </Unknown>".to_owned(),
        "    </CombatInfo>".to_owned(),
        format!("    <CombatInfo SourceName=\"{name}\" TargetName=\"Acdream\">"),
        "      <Magic>".to_owned(),
        "        <Fire TotalAttacks=\"1\" Crits=\"1\" TotalCritDamage=\"90\" MaxCritDamage=\"90\" />"
            .to_owned(),
        "        <Cold TotalAttacks=\"1\" TotalNormalDamage=\"7\" MaxNormalDamage=\"7\" />".to_owned(),
        "      </Magic>".to_owned(),
        "    </CombatInfo>".to_owned(),
        "  </CombatInfos>".to_owned(),
        "  <AetheriaInfos>".to_owned(),
        "    <AetheriaInfo SourceName=\"Acdream\" TargetName=\"Acdream\" TotalSurges=\"1\" />"
            .to_owned(),
        "  </AetheriaInfos>".to_owned(),
        "  <CloakInfos>".to_owned(),
        "    <CloakInfo SourceName=\"Acdream\" TargetName=\"\" TotalSurges=\"2\" />".to_owned(),
        "  </CloakInfos>".to_owned(),
        "</CombatTracker>".to_owned(),
    ]
    .join("\r\n");
    assert_eq!(xml::export(&full()), expected);
}

#[test]
fn an_info_without_attacks_and_a_zero_surge_count_are_empty_elements() {
    let text = "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\"><Magic /></CombatInfo><CombatInfo SourceName=\"c\" TargetName=\"d\" /></CombatInfos><CloakInfos><CloakInfo SourceName=\"a\" TargetName=\"\" /></CloakInfos></CombatTracker>";
    let mut tracker = CombatTracker::default();
    assert_eq!(xml::import(text, &mut tracker), Ok(Imported::Read));
    assert_eq!(
        xml::export(&tracker),
        "<CombatTracker>\r\n  <CombatInfos>\r\n    <CombatInfo SourceName=\"a\" TargetName=\"b\">\r\n      <Magic />\r\n    </CombatInfo>\r\n    <CombatInfo SourceName=\"c\" TargetName=\"d\" />\r\n  </CombatInfos>\r\n  <CloakInfos>\r\n    <CloakInfo SourceName=\"a\" TargetName=\"\" />\r\n  </CloakInfos>\r\n</CombatTracker>"
    );
}

#[test]
fn import_round_trips() {
    for tracker in [one_info(), full()] {
        let text = xml::export(&tracker);
        let mut read = CombatTracker::default();
        read.add_combat(ME, &CombatEvent::killing_blow(ME, "Someone before"));
        assert_eq!(xml::import(&text, &mut read), Ok(Imported::Read));
        assert_eq!(read.combat_infos(), tracker.combat_infos());
        assert_eq!(read.aetheria_infos(), tracker.aetheria_infos());
        assert_eq!(read.cloak_infos(), tracker.cloak_infos());
        assert_eq!(xml::export(&read), text);
        // Read in, it counts on: the same info.
        let first = read.combat_infos()[0].clone();
        read.add_combat(ME, &CombatEvent::killing_blow(&first.source, &first.target));
        assert_eq!(read.combat_infos().len(), tracker.combat_infos().len());
        assert_eq!(
            read.combat_infos()[0].killing_blows,
            first.killing_blows + 1
        );
    }
}

#[test]
fn import_reads_what_xml_document_reads() {
    // A byte-order mark, a declaration, comments, nodes without attributes,
    // numbers with blanks and signs, numbers that do not read (0).
    let text = "\u{feff}<?xml version=\"1.0\"?>\r\n<CombatTracker>\r\n  <!-- a comment -->\r\n  <CombatInfos>\r\n    <!-- skipped -->\r\n    <CombatInfo />\r\n    <CombatInfo SourceName=\"Acdream\" TargetName=\"X\" KillingBlows=\" +2 \">\r\n      <MeleeMissle>\r\n        <Slash />\r\n        <Pierce TotalAttacks=\"4\" Crits=\"many\" />\r\n      </MeleeMissle>\r\n    </CombatInfo>\r\n  </CombatInfos>\r\n</CombatTracker>\r\n";
    let mut tracker = CombatTracker::default();
    assert_eq!(xml::import(text, &mut tracker), Ok(Imported::Read));
    let infos = tracker.combat_infos();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].killing_blows, 2);
    let melee = infos[0]
        .by_attack_type
        .get(AttackType::MeleeMissle)
        .unwrap();
    assert_eq!(melee.by_element.len(), 1);
    let pierce = melee.by_element.get(DamageElement::Pierce).unwrap();
    assert_eq!((pierce.total_attacks, pierce.crits), (4, 0));
}

#[test]
fn an_unknown_element_name_aborts_the_import_keeping_what_came_before() {
    let text = "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\" KillingBlows=\"1\" /><CombatInfo SourceName=\"c\" TargetName=\"d\"><Magic><Nether TotalAttacks=\"1\" /></Magic></CombatInfo><CombatInfo SourceName=\"e\" TargetName=\"f\" /></CombatInfos></CombatTracker>";
    let mut tracker = one_info();
    assert_eq!(
        xml::import(text, &mut tracker),
        Err(ImportError::UnknownElement("Nether".to_owned()))
    );
    assert_eq!(tracker.combat_infos().len(), 1);
    assert_eq!(tracker.combat_infos()[0].source, "a");
}

#[test]
fn an_unknown_attack_type_or_a_missing_name_aborts_the_import() {
    let mut tracker = CombatTracker::default();
    assert_eq!(
        xml::import(
            "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\"><melee /></CombatInfo></CombatInfos></CombatTracker>",
            &mut tracker
        ),
        Err(ImportError::UnknownAttackType("melee".to_owned()))
    );
    assert_eq!(
        xml::import(
            "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" /></CombatInfos></CombatTracker>",
            &mut tracker
        ),
        Err(ImportError::MissingName("TargetName"))
    );
    assert_eq!(
        xml::import(
            "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\"><Magic /><Magic /></CombatInfo></CombatInfos></CombatTracker>",
            &mut tracker
        ),
        Err(ImportError::Duplicate("Magic".to_owned()))
    );
}

#[test]
fn a_file_that_is_not_stats_leaves_the_tracker_as_it_was() {
    let mut tracker = one_info();
    assert_eq!(
        xml::import("<Mag-Tools />", &mut tracker),
        Ok(Imported::NotStats)
    );
    assert!(matches!(
        xml::import("<CombatTracker>", &mut tracker),
        Err(ImportError::Xml(_))
    ));
    assert_eq!(tracker.combat_infos(), one_info().combat_infos());
}

#[test]
fn the_keys_are_mag_tools_file_names() {
    assert_eq!(
        xml::persistent_key("Coldeve", "Acdream"),
        "Mag-Tools/Coldeve/Acdream.CombatTracker.xml"
    );
    // 2025-11-08 00:00 UTC, two hours east: local 02:00.
    let local = DateTime::local_from_unix_millis(1_762_560_000_000, 120);
    assert_eq!(
        xml::export_key("Coldeve", "Mr.", local),
        "Mag-Tools/Coldeve/Mr_.CombatTracker.2025-11-08 02-00.xml"
    );
    assert_eq!(
        xml::persistent_key("a/b:c", "x\\y"),
        "Mag-Tools/a_b_c/x_y.CombatTracker.xml"
    );
}

// The files in the session

fn settings(persistent: bool, export_on_logoff: bool) -> CombatSettings {
    CombatSettings {
        persistent,
        export_on_logoff,
    }
}

/// The fake's character is "Meta Tester" on Coldeve.
const PERSISTENT: &str = "Mag-Tools/Coldeve/Meta Tester.CombatTracker.xml";

fn hit_line(text: &str) -> ChatLine {
    ChatLine {
        kind: 8,
        combat_kind: 1,
        text: text.to_owned(),
        display_text: text.to_owned(),
        ..ChatLine::default()
    }
}

fn ready(host: &mut FakeMagHost) -> Combat {
    let mut combat = Combat::default();
    while !combat.is_ready() {
        combat.tick(host, CombatSettings::default());
    }
    combat
}

fn read(host: &mut FakeMagHost, key: &str) -> Option<String> {
    host.storage().read_text(key).expect("readable")
}

#[test]
fn a_missing_file_is_not_an_error() {
    let mut host = FakeMagHost::new();
    let mut tracker = one_info();
    let mut import = StagedImport::new(PERSISTENT.to_owned());
    assert!(import.step(&mut host, &mut tracker));
    assert_eq!(tracker.combat_infos(), one_info().combat_infos());
    assert!(host.meta.logged(LogLevel::Warn).is_empty());
    assert_eq!(tracker.take_updates().last(), Some(&TrackerUpdate::Loaded));
}

#[test]
fn a_broken_file_is_logged_and_not_loaded() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(PERSISTENT, "<CombatTracker><CombatInfos><Oops")
        .unwrap();
    let mut tracker = one_info();
    tracker.take_updates();
    let mut import = StagedImport::new(PERSISTENT.to_owned());
    assert!(!import.step(&mut host, &mut tracker));
    assert!(import.step(&mut host, &mut tracker));
    assert_eq!(tracker.combat_infos(), one_info().combat_infos());
    assert_eq!(host.meta.logged(LogLevel::Warn).len(), 1);
    assert!(tracker.take_updates().is_empty());
}

#[test]
fn the_login_reads_the_persistent_file_and_the_logoff_writes_it() {
    let mut host = FakeMagHost::new();
    let saved = xml::export(&full());
    host.storage().write_text(PERSISTENT, &saved).unwrap();
    let mut combat = ready(&mut host);
    // Before the login completed: discarded by the import.
    combat.on_chat(&mut host, &hit_line("You obliterate Early Bird!"));
    combat.on_login_complete(&mut host, settings(true, false));
    assert!(combat.is_importing());
    // Heard while it reads: kept, after the file's.
    combat.on_chat(&mut host, &hit_line("You obliterate Late Bird!"));
    while combat.is_importing() {
        combat.tick(&mut host, settings(true, false));
    }
    let targets: Vec<&str> = combat
        .persistent
        .combat_infos()
        .iter()
        .map(|info| info.target.as_str())
        .collect();
    assert_eq!(targets, vec!["Tom & \"Jerry\" <1>'s", ME, "Late Bird"]);
    assert_eq!(combat.current.combat_infos().len(), 2);
    host.storage.writes.clear();
    combat.on_logoff(&mut host, settings(true, false));
    assert_eq!(host.storage.writes, vec![PERSISTENT.to_owned()]);
    let written = read(&mut host, PERSISTENT).unwrap();
    assert!(written.contains("TargetName=\"Late Bird\""));
    assert!(!written.contains("Early Bird"));

    // The next session reads it back.
    combat.reset();
    combat.on_login_complete(&mut host, settings(true, false));
    while combat.is_importing() {
        combat.tick(&mut host, settings(true, false));
    }
    assert_eq!(combat.persistent.combat_infos().len(), 3);
    assert!(combat.current.is_empty());
}

#[test]
fn without_persistent_stats_no_file_is_read_or_written() {
    let mut host = FakeMagHost::new();
    host.storage()
        .write_text(PERSISTENT, &xml::export(&full()))
        .unwrap();
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings(false, false));
    assert!(!combat.is_importing());
    combat.on_chat(&mut host, &hit_line("You obliterate Drudge Skulker!"));
    // The persistent tracker still counts (Mag-Tools: it always parses).
    assert_eq!(combat.persistent.combat_infos().len(), 1);
    host.storage.writes.clear();
    combat.on_logoff(&mut host, settings(false, false));
    assert!(host.storage.writes.is_empty());
}

#[test]
fn empty_stats_write_nothing() {
    let mut host = FakeMagHost::new();
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings(true, true));
    while combat.is_importing() {
        combat.tick(&mut host, settings(true, true));
    }
    combat.on_logoff(&mut host, settings(true, true));
    assert!(host.storage.writes.is_empty());
}

#[test]
fn the_logoff_exports_the_current_stats_when_asked() {
    let mut host = FakeMagHost::new();
    host.local_offset_minutes = 60;
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings(false, true));
    combat.on_chat(&mut host, &hit_line("You obliterate Drudge Skulker!"));
    combat.on_logoff(&mut host, settings(false, true));
    let key = "Mag-Tools/Coldeve/Meta Tester.CombatTracker.2025-11-08 01-00.xml";
    assert_eq!(host.storage.writes, vec![key.to_owned()]);
    assert!(read(&mut host, key).unwrap().contains("KillingBlows=\"1\""));
    // No message at the logoff.
    assert!(host.posted_text().is_empty());
}

#[test]
fn the_export_button_says_where_it_wrote() {
    let mut host = FakeMagHost::new();
    host.storage.inner = SharedStorage::folder();
    let mut combat = ready(&mut host);
    assert_eq!(combat.export_current(&mut host, true), None);
    combat.on_chat(&mut host, &hit_line("You obliterate Drudge Skulker!"));
    let key = combat.export_current(&mut host, true).unwrap();
    assert_eq!(
        key,
        "Mag-Tools/Coldeve/Meta Tester.CombatTracker.2025-11-08 00-00.xml"
    );
    assert_eq!(
        host.posted_text(),
        vec![
            "<{Mag-Tools}>: Stats exported to: C:\\storage\\Mag-Tools\\Coldeve\\Meta Tester.CombatTracker.2025-11-08 00-00.xml"
                .to_owned()
        ]
    );
    // Heard back, the message is not a combat line.
    let posted = host.posted_text()[0].clone();
    let infos = combat.current.combat_infos().to_vec();
    combat.on_chat(&mut host, &hit_line(&posted));
    assert_eq!(combat.current.combat_infos(), infos.as_slice());
}

#[test]
fn the_persistent_stats_are_written_every_ten_minutes() {
    let mut host = FakeMagHost::new();
    let mut combat = ready(&mut host);
    combat.on_login_complete(&mut host, settings(true, false));
    while combat.is_importing() {
        combat.tick(&mut host, settings(true, false));
    }
    combat.on_chat(&mut host, &hit_line("You obliterate Drudge Skulker!"));
    host.now_ms += SAVE_INTERVAL_MS - 1.0;
    combat.tick(&mut host, settings(true, false));
    assert!(host.storage.writes.is_empty());
    host.now_ms += 1.0;
    combat.tick(&mut host, settings(true, false));
    assert_eq!(host.storage.writes, vec![PERSISTENT.to_owned()]);
    combat.tick(&mut host, settings(true, false));
    assert_eq!(host.storage.writes.len(), 1);
}

/// A file of `count` infos, about 300 bytes each.
fn many(count: usize) -> String {
    let mut tracker = CombatTracker::default();
    for index in 0..count {
        let monster = format!("Monster {index}");
        for points in [25, 40] {
            tracker.add_combat(
                ME,
                &CombatEvent::damage(
                    ME,
                    &monster,
                    AttackType::MeleeMissle,
                    DamageElement::Slash,
                    false,
                    points,
                ),
            );
        }
        tracker.add_combat(ME, &CombatEvent::killing_blow(ME, &monster));
    }
    xml::export(&tracker)
}

#[test]
fn a_large_file_is_read_in_over_several_ticks() {
    let mut host = FakeMagHost::new();
    let text = many(2000);
    assert!(text.len() > 3 * xml::IMPORT_BYTES_PER_TICK);
    host.storage().write_text(PERSISTENT, &text).unwrap();
    let mut tracker = one_info();
    let mut import = StagedImport::new(PERSISTENT.to_owned());
    let mut steps = 1;
    while !import.step(&mut host, &mut tracker) {
        steps += 1;
        if steps < 4 {
            // Nothing replaced until the whole file is read.
            assert_eq!(tracker.combat_infos(), one_info().combat_infos());
        }
    }
    assert!(steps > 4, "{steps}");
    assert_eq!(tracker.combat_infos().len(), 2000);
    assert_eq!(xml::export(&tracker), text);
}

#[test]
fn a_file_broken_near_its_end_leaves_the_tracker_as_it_was() {
    // An unknown element first, then an info that is not XML: XmlDocument
    // would not have loaded the file at all.
    let text = many(1000).replacen("<Slash ", "<Nether ", 1).replacen(
        "TargetName=\"Monster 999\"",
        "TargetName=\"Monster 999\" TargetName=\"twice\"",
        1,
    );
    let mut tracker = one_info();
    assert!(matches!(
        xml::import(&text, &mut tracker),
        Err(ImportError::Xml(_))
    ));
    assert_eq!(tracker.combat_infos(), one_info().combat_infos());
}

#[test]
fn only_the_first_section_of_each_name_is_read() {
    let text = "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\" /></CombatInfos><Other><CombatInfo SourceName=\"x\" TargetName=\"y\" /></Other><CombatInfos><CombatInfo SourceName=\"c\" TargetName=\"d\" /></CombatInfos></CombatTracker>";
    let mut tracker = CombatTracker::default();
    assert_eq!(xml::import(text, &mut tracker), Ok(Imported::Read));
    let sources: Vec<&str> = tracker
        .combat_infos()
        .iter()
        .map(|info| info.source.as_str())
        .collect();
    assert_eq!(sources, vec!["a"]);
}

#[test]
fn line_breaks_in_a_name_are_written_as_character_references() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(ME, &CombatEvent::killing_blow(ME, "Two\r\nLines\n"));
    let text = xml::export(&tracker);
    assert!(
        text.contains("TargetName=\"Two&#xD;&#xA;Lines&#xA;\""),
        "{text}"
    );
    let mut read = CombatTracker::default();
    assert_eq!(xml::import(&text, &mut read), Ok(Imported::Read));
    assert_eq!(read.combat_infos()[0].target, "Two\r\nLines\n");
}

#[test]
fn the_text_kept_follows_every_change() {
    let mut tracker = CombatTracker::default();
    let check = |tracker: &CombatTracker| {
        assert_eq!(xml::export(tracker), xml::export_fresh(tracker));
    };
    for round in 0..5 {
        for index in 0..40 {
            let monster = format!("Monster {}", (index * 7 + round) % 30);
            tracker.add_combat(ME, &slash(index % 3 == 0, 10 + index));
            tracker.add_combat(ME, &CombatEvent::killing_blow(ME, &monster));
            tracker.add_combat(
                ME,
                &CombatEvent::damage(
                    &monster,
                    ME,
                    AttackType::Magic,
                    DamageElement::Cold,
                    true,
                    5,
                ),
            );
        }
        check(&tracker);
        tracker.refresh_texts(25);
        check(&tracker);
    }
    while tracker.refresh_texts(25) > 0 {}
    assert_eq!(tracker.stale_texts(), 0);
    check(&tracker);
    // Read in, the infos come with their text.
    let mut read = CombatTracker::default();
    assert_eq!(
        xml::import(&xml::export(&tracker), &mut read),
        Ok(Imported::Read)
    );
    assert_eq!(read.stale_texts(), 0);
    check(&read);
    read.clear();
    check(&read);
}

/// The broken copy's key for the fake's character.
const BROKEN: &str = "Mag-Tools/Coldeve/Meta Tester.CombatTracker.broken.xml";

/// The login reads `file` in, a kill is heard, and the character logs off.
fn session_over(host: &mut FakeMagHost) -> Combat {
    let mut combat = ready(host);
    combat.on_login_complete(host, settings(true, false));
    while combat.is_importing() {
        combat.tick(host, settings(true, false));
    }
    combat.on_chat(host, &hit_line("You obliterate Drudge Skulker!"));
    combat.on_logoff(host, settings(true, false));
    combat
}

#[test]
fn a_broken_persistent_file_is_kept_aside_before_it_is_written_over() {
    // F16
    for broken in [
        // Cut short.
        "<CombatTracker>\r\n  <CombatInfos>\r\n    <CombatInfo SourceName=\"Meta Tester\" Targ",
        // Edited by hand into something else.
        "<CombatTracker><CombatInfos><CombatInfo SourceName=\"a\" TargetName=\"b\"><Melee /></CombatInfo></CombatInfos></CombatTracker>",
        "<Stats />",
    ] {
        let mut host = FakeMagHost::new();
        host.storage.inner.insert(PERSISTENT, broken);
        session_over(&mut host);
        assert_eq!(host.storage.inner.get(BROKEN).as_deref(), Some(broken));
        let warnings = host.meta.logged(LogLevel::Warn);
        assert!(
            warnings.iter().any(|line| line.contains(BROKEN)),
            "{warnings:?}"
        );
        let written = host.storage.inner.get(PERSISTENT).unwrap();
        let mut read = CombatTracker::default();
        assert_eq!(xml::import(&written, &mut read), Ok(Imported::Read));
        assert!(written.contains("Drudge Skulker"));
    }
}

#[test]
fn a_persistent_file_that_cannot_be_kept_aside_is_not_written_over() {
    let broken = "<CombatTracker><CombatInfos>";
    let mut host = FakeMagHost::new();
    host.storage.inner.insert(PERSISTENT, broken);
    host.storage.inner.refuse_writes(BROKEN);
    session_over(&mut host);
    assert_eq!(host.storage.inner.get(PERSISTENT).as_deref(), Some(broken));

    // Nor one that could not be read.
    let mut host = FakeMagHost::new();
    host.storage.inner.insert(PERSISTENT, "<CombatTracker />");
    host.storage.inner.fail("read", "locked");
    session_over(&mut host);
    assert_eq!(
        host.storage.inner.get(PERSISTENT).as_deref(),
        Some("<CombatTracker />")
    );
    assert!(host.storage.inner.get(BROKEN).is_none());
}
