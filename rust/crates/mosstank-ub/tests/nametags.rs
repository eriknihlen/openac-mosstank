//! The name tags, against `NametagControllerTests`.

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::ui::WorldLabel;
use ac_plugin_abi::world::{
    ItemProperties, ObjectChange, ObjectChangeKind, ObjectClass, Position, WorldObject,
};
use mosstank_ub::testing::FakeUbHost;
use mosstank_ub::tools::nametags::{
    ASSESS_ATTEMPT_LIMIT, MAXIMUM_LABELS, NametagController, NametagSettings,
};

const SELF_ID: u32 = 0x5000_0001;
const LEVEL_PROPERTY: u32 = 25;
const MONARCH_PROPERTY: u32 = 26;
const PET_OWNER_PROPERTY: u32 = 44;
const PORTAL_DESTINATION_PROPERTY: u32 = 38;
const ALLEGIANCE_NAME_PROPERTY: u32 = 47;

fn at(east_west: f64) -> Option<Position> {
    Some(Position {
        east_west,
        is_outdoor: true,
        ..Position::default()
    })
}

fn world(id: u32, name: &str, object_class: ObjectClass, east_west: f64) -> WorldObject {
    WorldObject {
        object_id: id,
        weenie_class_id: 1,
        name: name.to_owned(),
        object_class,
        is_landscape: true,
        position: at(east_west),
        ..WorldObject::default()
    }
}

fn properties(
    ints: &[(u32, i32)],
    strings: &[(u32, &str)],
    instance_ids: &[(u32, u32)],
) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        strings: strings
            .iter()
            .map(|(key, value)| (*key, (*value).to_owned()))
            .collect(),
        instance_ids: instance_ids.to_vec(),
        ..ItemProperties::default()
    }
}

/// The colour a label carries, as the C# reads it (`Vector4`).
fn color(label: &WorldLabel) -> [f32; 4] {
    [label.r, label.g, label.b, label.a]
}

struct Rig {
    host: FakeUbHost,
    settings: NametagSettings,
    controller: NametagController,
}

impl Rig {
    fn new() -> Self {
        Self::with(NametagSettings::default(), 0)
    }

    fn with(settings: NametagSettings, monarch: u32) -> Self {
        let mut host = FakeUbHost::new();
        host.meta.character_mut().id = SELF_ID;
        host.meta.navigation = NavigationSnapshot {
            is_available: true,
            position: Position {
                is_outdoor: true,
                ..Position::default()
            },
            ..NavigationSnapshot::default()
        };
        host.allegiance.monarch_id = monarch;
        Self {
            host,
            settings,
            controller: NametagController::new(&settings),
        }
    }

    fn change(&mut self, object_id: u32, kind: ObjectChangeKind) {
        let change = ObjectChange {
            object_id,
            kind,
            revision: 1,
            changed_fields: 0,
            current: None,
        };
        self.controller.on_object_changed(&mut self.host, &change);
    }

    fn put(&mut self, object: WorldObject) {
        self.host
            .meta
            .objects
            .retain(|known| known.object_id != object.object_id);
        self.host.meta.objects.push(object);
    }

    fn create(&mut self, object: WorldObject) {
        let id = object.object_id;
        self.put(object);
        self.change(id, ObjectChangeKind::Created);
    }

    fn appraise(&mut self, object_id: u32, appraisal: ItemProperties) {
        self.host.meta.properties.insert(object_id, appraisal);
        for known in &mut self.host.meta.objects {
            if known.object_id == object_id {
                known.has_appraisal_data = true;
            }
        }
        self.change(object_id, ObjectChangeKind::IdentReceived);
    }

    fn release(&mut self, object_id: u32) {
        self.host
            .meta
            .objects
            .retain(|known| known.object_id != object_id);
        self.change(object_id, ObjectChangeKind::Released);
    }

    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.controller.on_tick(&mut self.host, &self.settings, 0.1);
        }
    }

    fn current(&self) -> Vec<WorldLabel> {
        self.host.label_sets.last().cloned().unwrap_or_default()
    }

    fn single(&self) -> WorldLabel {
        let current = self.current();
        assert_eq!(1, current.len(), "{current:?}");
        current[0].clone()
    }

    fn pushes(&self) -> usize {
        self.host.label_sets.len()
    }

    fn identified(&self) -> Vec<u32> {
        self.host.identified.clone()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: ACreatedMonsterIsHandedToTheHostAndAReleasedOneIsTakenBack
#[test]
fn a_created_monster_is_handed_to_the_host_and_a_released_one_is_taken_back() {
    let mut rig = Rig::new();
    rig.create(world(
        0x8000_0001,
        "Drudge Slinker",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);

    let label = rig.single();
    assert_eq!(0x8000_0001, label.object_id);
    assert_eq!("Drudge Slinker", label.text);
    assert_eq!(0, label.line);
    assert!((label.max_range - 35.0).abs() < f32::EPSILON);
    assert!(label.outline);
    assert_eq!([1.0, 0.0, 0.0, 1.0], color(&label));

    rig.release(0x8000_0001);
    rig.ticks(1);
    assert!(rig.current().is_empty());
    assert_eq!(2, rig.pushes());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AFrameInWhichNothingChangedHandsNothingOver
#[test]
fn a_frame_in_which_nothing_changed_hands_nothing_over() {
    let mut rig = Rig::new();
    rig.create(world(
        0x8000_0001,
        "Drudge Slinker",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);
    assert_eq!(1, rig.pushes());

    rig.ticks(30);
    assert_eq!(1, rig.pushes());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: ATagsTextIsRefreshedAtMostOnceASecond
#[test]
fn a_tags_text_is_refreshed_at_most_once_a_second() {
    let mut rig = Rig::new();
    rig.create(world(
        0x8000_0001,
        "Drudge Slinker",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);
    assert_eq!("Drudge Slinker", rig.single().text);

    rig.put(world(
        0x8000_0001,
        "Drudge Prowler",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(5);
    assert_eq!("Drudge Slinker", rig.single().text);

    rig.ticks(6);
    assert_eq!("Drudge Prowler", rig.single().text);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AnAppraisalIsAskedForOnlyWhenCloserThanLastTimeAndAtMostEveryFiveSeconds
#[test]
fn an_appraisal_is_asked_for_only_when_closer_than_last_time_and_at_most_every_five_seconds() {
    let mut rig = Rig::new();
    rig.create(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.05));
    rig.ticks(1);
    assert_eq!(vec![0x5000_0002], rig.identified());

    rig.ticks(40);
    assert_eq!(1, rig.identified().len());

    rig.ticks(20);
    assert_eq!(1, rig.identified().len());

    rig.put(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.02));
    rig.ticks(11);
    assert_eq!(2, rig.identified().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: ComingCloserTwiceInsideFiveSecondsAsksOnce
#[test]
fn coming_closer_twice_inside_five_seconds_asks_once() {
    let mut rig = Rig::new();
    rig.create(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.05));
    rig.ticks(1);
    assert_eq!(vec![0x5000_0002], rig.identified());

    rig.put(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.04));
    rig.ticks(10);
    assert_eq!(1, rig.identified().len());

    rig.put(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.03));
    rig.ticks(10);
    assert_eq!(1, rig.identified().len());

    // Past the five seconds, still closer than the first request.
    rig.ticks(40);
    assert_eq!(2, rig.identified().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AnObjectThatNeverAnswersIsGivenUpOnAfterTheTryLimit
#[test]
fn an_object_that_never_answers_is_given_up_on_after_the_try_limit() {
    let mut rig = Rig::new();
    let mut east_west = 0.1;
    rig.create(world(0x5000_0002, "Sigrid", ObjectClass::Player, east_west));
    for _ in 0..30 {
        east_west -= 0.002;
        rig.put(world(0x5000_0002, "Sigrid", ObjectClass::Player, east_west));
        rig.ticks(60);
    }
    assert_eq!(
        usize::try_from(ASSESS_ATTEMPT_LIMIT + 1).unwrap(),
        rig.identified().len()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: APlayersAppraisalAddsTheLevelAndHangsTheAllegianceUnderneath
#[test]
fn a_players_appraisal_adds_the_level_and_hangs_the_allegiance_underneath() {
    let mut rig = Rig::new();
    rig.create(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.05));
    rig.ticks(1);
    assert_eq!("Sigrid", rig.single().text);

    rig.appraise(
        0x5000_0002,
        properties(
            &[(LEVEL_PROPERTY, 126)],
            &[(ALLEGIANCE_NAME_PROPERTY, "The Iron Hand")],
            &[(MONARCH_PROPERTY, 0x5000_0009)],
        ),
    );
    rig.ticks(1);

    let current = rig.current();
    assert_eq!(2, current.len());
    assert_eq!("Sigrid [126]", current[0].text);
    assert_eq!(1, current[0].line);
    assert_eq!("<The Iron Hand>", current[1].text);
    assert_eq!(0, current[1].line);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: APlayerUnderTheSameMonarchTakesTheAllegianceColour
#[test]
fn a_player_under_the_same_monarch_takes_the_allegiance_colour() {
    let mut rig = Rig::with(NametagSettings::default(), 0x5000_0009);
    rig.create(world(0x5000_0002, "Sigrid", ObjectClass::Player, 0.05));
    rig.ticks(1);
    assert_eq!([0.0, 1.0, 1.0, 1.0], color(&rig.single()));

    rig.appraise(
        0x5000_0002,
        properties(
            &[(LEVEL_PROPERTY, 50)],
            &[],
            &[(MONARCH_PROPERTY, 0x5000_0009)],
        ),
    );
    rig.ticks(1);
    assert_eq!([0.0, 1.0, 0.0, 1.0], color(&rig.current()[0]));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: APortalHangsItsDestinationUnderneathOnceAppraised
#[test]
fn a_portal_hangs_its_destination_underneath_once_appraised() {
    let mut rig = Rig::new();
    rig.create(world(
        0x7000_0001,
        "Portal to Holtburg",
        ObjectClass::Portal,
        0.05,
    ));
    rig.ticks(1);
    assert_eq!(vec![0x7000_0001], rig.identified());

    rig.appraise(
        0x7000_0001,
        properties(&[], &[(PORTAL_DESTINATION_PROPERTY, "Holtburg")], &[]),
    );
    rig.ticks(1);
    let current = rig.current();
    assert_eq!(2, current.len());
    assert_eq!("<Holtburg>", current[1].text);
    assert_eq!([0.0, 1.0, 0.0, 1.0], color(&current[1]));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AMonsterWithAnOwnerTakesThePetColour
#[test]
fn a_monster_with_an_owner_takes_the_pet_colour() {
    let mut rig = Rig::new();
    rig.host.meta.properties.insert(
        0x8000_0001,
        properties(&[], &[], &[(PET_OWNER_PROPERTY, 0x5000_0002)]),
    );
    rig.create(world(
        0x8000_0001,
        "Angel of Death",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);
    assert_eq!([0.0, 1.0, 1.0, 1.0], color(&rig.single()));
    assert!(rig.identified().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AChangeOnThePageIsPickedUpWithinAQuarterSecondAndReHanded
#[test]
fn a_change_on_the_page_is_picked_up_within_a_quarter_second_and_re_handed() {
    let mut rig = Rig::new();
    rig.create(world(
        0x8000_0001,
        "Drudge Slinker",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);
    assert_eq!(1, rig.pushes());

    rig.settings.monster.tag_color = 0xFF00_00FF;
    rig.ticks(3);
    assert_eq!(2, rig.pushes());
    assert_eq!([0.0, 0.0, 1.0, 1.0], color(&rig.single()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: ADisabledGroupIsNotTaggedAndTheMasterSwitchClearsEverything
#[test]
fn a_disabled_group_is_not_tagged_and_the_master_switch_clears_everything() {
    let mut settings = NametagSettings::default();
    settings.vendor.enabled = false;
    let mut rig = Rig::with(settings, 0);
    rig.create(world(0x6000_0001, "Aun Tanua", ObjectClass::Vendor, 0.01));
    rig.create(world(
        0x8000_0001,
        "Drudge Slinker",
        ObjectClass::Monster,
        0.01,
    ));
    rig.ticks(1);
    assert_eq!(0x8000_0001, rig.single().object_id);

    rig.settings.enabled = false;
    rig.ticks(3);
    assert!(rig.current().is_empty());

    rig.settings.enabled = true;
    rig.ticks(3);
    assert_eq!(0x8000_0001, rig.single().object_id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AnObjectPastTheMaximumRangeIsLeftOutOfTheSet
#[test]
fn an_object_past_the_maximum_range_is_left_out_of_the_set() {
    let mut rig = Rig::new();
    rig.create(world(0x8000_0001, "Near", ObjectClass::Monster, 0.05));
    rig.create(world(0x8000_0002, "Far", ObjectClass::Monster, 1.0));
    rig.ticks(1);
    assert_eq!("Near", rig.single().text);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: ASetPastTheHostsCapIsTrimmedToTheNearestObjects
#[test]
fn a_set_past_the_hosts_cap_is_trimmed_to_the_nearest_objects() {
    let mut rig = Rig::new();
    for i in 1..=300_u32 {
        rig.create(world(
            0x8000_0000 + i,
            &format!("M{i}"),
            ObjectClass::Monster,
            0.0001 * f64::from(i),
        ));
    }
    rig.ticks(1);
    let current = rig.current();
    assert_eq!(MAXIMUM_LABELS, current.len());
    assert_eq!("M1", current[0].text);
    assert!(!current.iter().any(|label| label.text == "M300"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: AnObjectGoneFromTheTableIsDroppedAfterTheWalkNotDuringIt
#[test]
fn an_object_gone_from_the_table_is_dropped_after_the_walk_not_during_it() {
    let mut rig = Rig::new();
    rig.create(world(0x8000_0001, "One", ObjectClass::Monster, 0.01));
    rig.create(world(0x8000_0002, "Two", ObjectClass::Monster, 0.01));
    rig.create(world(0x8000_0003, "Three", ObjectClass::Monster, 0.01));
    rig.ticks(1);
    assert_eq!(3, rig.current().len());

    rig.host
        .meta
        .objects
        .retain(|known| known.object_id != 0x8000_0002);
    rig.ticks(11);
    assert_eq!(2, rig.current().len());
    assert_eq!(
        vec![0x8000_0001, 0x8000_0003],
        rig.controller.tagged_object_ids()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NametagControllerTests.cs :: TheWorldIsScannedOnTheFirstFrameAndAfterASessionReset
#[test]
fn the_world_is_scanned_on_the_first_frame_and_after_a_session_reset() {
    let mut rig = Rig::new();
    rig.put(world(
        0x8000_0001,
        "Already here",
        ObjectClass::Monster,
        0.01,
    ));
    rig.put(world(SELF_ID, "Me", ObjectClass::Player, 0.01));
    rig.ticks(1);
    assert_eq!("Already here", rig.single().text);

    rig.controller.reset(&mut rig.host);
    assert!(rig.current().is_empty());
    rig.ticks(1);
    assert_eq!("Already here", rig.single().text);
}
