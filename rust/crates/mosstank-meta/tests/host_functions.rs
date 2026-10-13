//! The host's expression functions and the runtime around them: the
//! character, objects, finders, actions, casting, login, chat, windows,
//! peers, delayed runs, persistent and global variables, fellowship, the
//! world clock, experience, quests, corpses, components, salvage. Ported
//! from the C#'s `HostExpressionFunctionsTests`.

mod common;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::{EquipmentItem, InventoryItem, LootContainer};
use ac_plugin_abi::magic::ComponentInfo;
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::social::{FellowMember, FellowshipState, LoginCharacter};
use ac_plugin_abi::world::{ObjectClass, Position, WorldTime};
use common::*;
use mosstank_expr::{ExpressionError, ExpressionValue};
use mosstank_meta::chat::{UB_ERROR_CHAT_TYPE, UB_GENERIC_CHAT_TYPE};
use mosstank_meta::runtime::{ExpressionRuntime, storage_key};
use mosstank_meta::testing::{FakeHost, SharedStorage};
use mosstank_stores::global_variables::hash;

fn new(host: &mut FakeHost) -> ExpressionRuntime {
    runtime(host)
}

fn wand(equipped: u32) -> EquipmentItem {
    EquipmentItem {
        object_id: 50,
        name: "Wand".to_owned(),
        valid_locations: 0x0100_0000,
        equipped_location: equipped,
        container_id: 1,
        ..EquipmentItem::default()
    }
}

fn reason(error: &ExpressionError) -> String {
    error.reason()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CharacterAndNavigationFunctionsReadCanonicalAutomationFacts
#[test]
fn character_and_navigation_functions_read_canonical_automation_facts() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(eval_string(&mut rt, &mut host, "getworldname[]"), "Coldeve");
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[``]"),
        2.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharvital_current[1]"),
        90.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharvital_buffedmax[1]"),
        100.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharattribute_base[1]"),
        100.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharattribute_buffed[1]"),
        110.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharskill_base[34]"),
        275.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharskill_buffed[34]"),
        300.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharskill_traininglevel[34]"),
        2.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getplayerlandblock[]"),
        f64::from(0x7F7F_0000_u32)
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getheading[wobjectgetplayer[]]"),
        90.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: PlayerLandblockKeepsTheHighWordInPlace
#[test]
fn player_landblock_keeps_the_high_word_in_place() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "getplayerlandblock[]"),
        2_139_029_504.0
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "cstrf[getplayerlandblock[],`X8`]"),
        "7F7F0000"
    );
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "getplayerlandblock[]==2139029504"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "getplayerlandcell[]"),
        f64::from(0x7F7F_0001_u32)
    );
}

fn tome(host: &mut FakeHost) {
    let mut book = object(0x8000_08B5, 105, "Tome", ObjectClass::Book, 0x2, 1, 0);
    book.is_owned = true;
    host.objects.push(book);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: HexFormatWrapsASignedObjectId
#[test]
fn hex_format_wraps_a_signed_object_id() {
    let mut host = create_automation();
    tome(&mut host);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "cstrf[wobjectgetid[wobjectfindbyid[-2147481419]],`X8`]"
        ),
        "800008B5"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "cstrf[-1,`X8`]"),
        "FFFFFFFF"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "cstrf[-2.7,`X`]"),
        "FFFFFFFE"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ObjectIdsAreSignedInExpressions
#[test]
fn object_ids_are_signed_in_expressions() {
    let mut host = create_automation();
    tome(&mut host);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetname[wobjectfindbyid[-2147481419]]"
        ),
        "Tome"
    );
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetname[wobjectfindbyid[-2147481419.4]]"
        ),
        "Tome"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindbyid[-2147481419]]"
        ),
        -2_147_481_419.0
    );
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetname[wobjectfindbyid[wobjectgetid[wobjectfindbyid[-2147481419]]]]"
        ),
        "Tome"
    );
    // A bare number stands for the object wherever an object is expected.
    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[-2147481419]"),
        "Tome"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "wobjectfindbyid[-5]"), 0.0);
    assert!(eval(&mut rt, &mut host, "wobjectfindbyid[4294967296]").is_err());
    assert!(eval(&mut rt, &mut host, "wobjectfindbyid[-2147483649]").is_err());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AnObjectArgumentThatIsNoIdIsAnEvaluationErrorAsInUtilityBelt
#[test]
fn an_object_argument_that_is_no_id_is_an_evaluation_error_as_in_utility_belt() {
    for source in [
        "wobjectgetname[0/0]",
        "wobjectgetname[99999999999*99999999999]",
    ] {
        let mut host = create_automation();
        let mut rt = new(&mut host);
        match eval_error(&mut rt, &mut host, source) {
            ExpressionError::Evaluation(error) => assert_eq!(
                error.inner.as_deref(),
                Some("Value was either too large or too small for an Int32."),
                "{source}"
            ),
            other => panic!("{source}: {other:?}"),
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ADateFormatThatCannotBeWrittenAnswersEmptyAsInUtilityBelt
#[test]
fn a_date_format_that_cannot_be_written_answers_empty_as_in_utility_belt() {
    // The C#'s format is "%", which .NET refuses; the SDK's compatibility
    // kit counts it among the formats it does not reproduce (an error, not
    // a refusal), so the same refusal is reached here with another format
    // .NET refuses: more than seven fraction digits.
    for source in ["getdatetimelocal[`ffffffff`]", "getdatetimeutc[`ffffffff`]"] {
        let mut host = create_automation();
        let mut rt = new(&mut host);

        assert_eq!(
            eval_string(&mut rt, &mut host, &format!("{source}+`after`")),
            "after"
        );
        assert_eq!(host.posted.len(), 1);
        let (text, chat_type) = &host.posted[0];
        assert!(text.starts_with("[UB] "));
        assert!(text.len() > "[UB] ".len());
        assert_eq!(*chat_type, UB_GENERIC_CHAT_TYPE);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: UnsignedIdsSavedByEarlierVersionsStillNameTheObject
#[test]
fn unsigned_ids_saved_by_earlier_versions_still_name_the_object() {
    let storage = SharedStorage::new();
    {
        let mut host = create_automation();
        host.storage = storage.clone();
        let mut earlier = new(&mut host);
        eval(
            &mut earlier,
            &mut host,
            "setpvar[`pack`,2147485877];setgvar[`tome`,2147485877]",
        )
        .unwrap();
        earlier.dispose(&mut host);
    }

    let mut host = create_automation();
    host.storage = storage;
    tome(&mut host);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetname[wobjectfindbyid[getpvar[`pack`]]]"
        ),
        "Tome"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[getgvar[`tome`]]"),
        "Tome"
    );
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "actiontryuseitem[getpvar[`pack`]]"
    ));
    assert_eq!(host.used_object, 0x8000_08B5);
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindbyid[getpvar[`pack`]]]"
        ),
        -2_147_481_419.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NetworkCastIdsCompareEqualToObjectIds
#[test]
fn network_cast_ids_compare_equal_to_object_ids() {
    let mut host = create_automation();
    host.objects.push(object(
        0x8000_1234,
        105,
        "Drudge",
        ObjectClass::Monster,
        0x2,
        1,
        0,
    ));
    host.peer_casts = vec![PeerCast {
        sequence: 3,
        client_id: 7,
        caster_id: 0x5000_00AA,
        target_id: 0x8000_1234,
        spell_id: 2074,
        effective_skill: 310,
        seconds_remaining: 42.5,
        landed: true,
        is_remote: false,
    }];
    let mut rt = new(&mut host);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "dictgetitem[listgetitem[netcasts[],0],`TargetId`]==wobjectgetid[wobjectfindbyid[-2147478988]]"
    ));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "dictgetitem[listgetitem[netcasts[],0],`TargetId`]"
        ),
        -2_147_478_988.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "dictgetitem[listgetitem[netcasts[],0],`CasterId`]"
        ),
        f64::from(0x5000_00AA_u32)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ObjectPropertiesAnswerFromTheObjectBeforeAppraisal
#[test]
fn object_properties_answer_from_the_object_before_appraisal() {
    let mut host = create_automation();
    host.objects.push(object(
        30,
        7001,
        "Insatiable Eater Jaw",
        ObjectClass::Misc,
        0x80,
        40,
        0,
    ));
    let mut crate_object = object(31, 7002, "Create Name", ObjectClass::Container, 0x200, 0, 0);
    crate_object.items_capacity = 24;
    crate_object.containers_capacity = 1;
    crate_object.is_landscape = true;
    crate_object.position = Some(Position {
        cell_id: 0xA9B4_0021,
        ..automation_position()
    });
    host.objects.push(crate_object);
    host.objects.push(object(
        32,
        7003,
        "Wand",
        ObjectClass::WandStaffOrb,
        0x8000,
        0,
        50001,
    ));
    host.properties
        .insert(31, properties(&[], &[], &[(1, "Appraised Name")]));
    let mut rt = new(&mut host);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "wobjectgetstringprop[wobjectfindbyid[30],1]==`Insatiable Eater Jaw`"
    ));
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetstringprop[wobjectfindbyid[30],16]"
        ),
        ""
    );
    for (key, expected) in [
        (218_103_808, 7001.0),
        (218_103_810, 40.0),
        (218_103_818, 0.0),
        (218_103_811, 0.0),
        (19, 0.0),
    ] {
        assert_eq!(
            eval_number(
                &mut rt,
                &mut host,
                &format!("wobjectgetintprop[wobjectfindbyid[30],{key}]")
            ),
            expected,
            "{key}"
        );
    }

    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetstringprop[wobjectfindbyid[31],1]"
        ),
        "Appraised Name"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[31],218103812]"
        ),
        24.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[31],218103813]"
        ),
        1.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[31],218103811]"
        ),
        f64::from(0xA9B4_0021_u32.cast_signed())
    );

    // A wielded object's container is the one wielding it.
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[32],218103810]"
        ),
        50001.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[32],218103818]"
        ),
        50001.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ObjectDescriptionIdsAreSignedLikeObjectIds
#[test]
fn object_description_ids_are_signed_like_object_ids() {
    let mut host = create_automation();
    let mut pack = object(
        0x8000_0100,
        7004,
        "Pack",
        ObjectClass::Container,
        0x200,
        1,
        0,
    );
    pack.is_owned = true;
    host.objects.push(pack);
    let mut gem = object(
        0x8000_0101,
        7005,
        "Gem",
        ObjectClass::Gem,
        0x800,
        0x8000_0100,
        0,
    );
    gem.is_owned = true;
    host.objects.push(gem);
    host.objects.push(object(
        0x8000_0102,
        7006,
        "Bow",
        ObjectClass::MissileWeapon,
        0x100,
        0,
        0x8000_0200,
    ));
    host.objects.push(object(
        0x8000_0200,
        7007,
        "Guard",
        ObjectClass::Npc,
        0x1,
        0,
        0,
    ));
    let mut rt = new(&mut host);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "wobjectgetintprop[wobjectfindbyid[-2147483391],218103810]==wobjectgetid[wobjectfindbyid[-2147483392]]"
    ));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[-2147483391],218103810]"
        ),
        -2_147_483_392.0
    );
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "wobjectgetintprop[wobjectfindbyid[-2147483390],218103818]==wobjectgetid[wobjectfindbyid[-2147483136]]"
    ));
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "wobjectgetintprop[wobjectfindbyid[-2147483390],218103810]==wobjectgetid[wobjectfindbyid[-2147483136]]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: VitalReadsSeparateBaseCurrentAndBuffedMaximumAndFloorAtOne
#[test]
fn vital_reads_separate_base_current_and_buffed_maximum_and_floor_at_one() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    for (source, expected) in [
        ("getcharvital_base[1]", 80.0),
        ("getcharvital_current[1]", 90.0),
        ("getcharvital_buffedmax[1]", 100.0),
        ("getcharvital_base[2]", 70.0),
        ("getcharvital_base[3]", 60.0),
        ("getcharvital_current[9]", 1.0),
        ("getcharvital_base[9]", 1.0),
        ("getcharvital_buffedmax[9]", 1.0),
    ] {
        assert_eq!(
            eval_number(&mut rt, &mut host, source),
            expected,
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: EchoPrintsInTheColourItIsGiven
#[test]
fn echo_prints_in_the_colour_it_is_given() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "echo[\\[RynCMD\\] Clearing All Vars,13]"
    ));
    // Without the colour it is the reference's argument-count error.
    assert!(eval(&mut rt, &mut host, "echo[plain]").is_err());

    assert_eq!(host.posted, [("[RynCMD] Clearing All Vars".to_owned(), 13)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ObjectDiscoveryCountsPropertiesAndNearestMatchUtilityBeltShape
#[test]
fn object_discovery_counts_properties_and_nearest_match_utility_belt_shape() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[wobjectfindallinventory[]]"),
        2.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "getitemcountininventorybyname[`Health Elixir`]"
        ),
        10.0
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getfreeitemslots[]"), 9.0);
    assert_eq!(
        eval_number(&mut rt, &mut host, "getfreecontainerslots[]"),
        1.0
    );
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "wobjectgetname[wobjectfindnearestmonster[]]"
        ),
        "Drudge"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetintprop[wobjectfindbyid[20],25]"
        ),
        77.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listcount[wobjectfindallbynamerx[`(?i)elixir|drudge`]]"
        ),
        2.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetobjectclass[wobjectfindbyid[20]]"
        ),
        f64::from(ObjectClass::Monster.tag())
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "wobjectlastidtime[wobjectfindbyid[20]]"),
        54321.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: InventoryNameLookupsAreCaseSensitive
#[test]
fn inventory_name_lookups_are_case_sensitive() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybyname[`Health Elixir`]]"
        ),
        10.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectfindininventorybyname[`health elixir`]"
        ),
        0.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybynamerx[`^Health`]]"
        ),
        10.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectfindininventorybynamerx[`^health`]"
        ),
        0.0
    );
}

fn owned(host: &mut FakeHost, id: u32, wcid: u32, name: &str, class: ObjectClass, item_type: u32) {
    let mut item = object(id, wcid, name, class, item_type, 1, 0);
    item.is_owned = true;
    host.objects.push(item);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NamePatternsMatchTheMaterialPrefixedDisplayName
#[test]
fn name_patterns_match_the_material_prefixed_display_name() {
    let mut host = create_automation();
    owned(
        &mut host,
        12,
        102,
        "Long Sword",
        ObjectClass::MeleeWeapon,
        0x1,
    );
    owned(&mut host, 13, 103, "Shard", ObjectClass::Gem, 0x2);
    host.properties
        .insert(12, properties(&[(131, 63)], &[], &[]));
    host.properties
        .insert(13, properties(&[(131, 9)], &[], &[]));
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[wobjectfindbyid[12]]"),
        "Silver Long Sword"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybynamerx[`^Silver `]]"
        ),
        12.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybyname[`Silver Long Sword`]]"
        ),
        12.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectfindininventorybyname[`Long Sword`]"
        ),
        0.0
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[wobjectfindbyid[13]]"),
        "Shard"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybynamerx[`^Shard$`]]"
        ),
        13.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: DisplayNameSkipsAMaterialTheNameAlreadyCarries
#[test]
fn display_name_skips_a_material_the_name_already_carries() {
    let mut host = create_automation();
    owned(&mut host, 14, 106, "Jet", ObjectClass::Gem, 0x2);
    owned(&mut host, 15, 107, "Silver Ring", ObjectClass::Jewelry, 0x8);
    // 27 is Jet, 63 is Silver.
    host.properties
        .insert(14, properties(&[(131, 27)], &[], &[]));
    host.properties
        .insert(15, properties(&[(131, 63)], &[], &[]));
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[wobjectfindbyid[14]]"),
        "Jet"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[wobjectfindbyid[15]]"),
        "Silver Ring"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybyname[`Jet`]]"
        ),
        14.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindininventorybyname[`Silver Ring`]]"
        ),
        15.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: InventoryCountsByNameReadTheDisplayNameInAnyCase
#[test]
fn inventory_counts_by_name_read_the_display_name_in_any_case() {
    let mut host = create_automation();
    owned(
        &mut host,
        12,
        102,
        "Long Sword",
        ObjectClass::MeleeWeapon,
        0x1,
    );
    object_mut(&mut host, 12).stack_size = 3;
    host.properties
        .insert(12, properties(&[(131, 63)], &[], &[]));
    let mut rt = new(&mut host);

    for (source, expected) in [
        ("getitemcountininventorybyname[`silver long sword`]", 3.0),
        ("getitemcountininventorybynamerx[`^silver `]", 3.0),
        ("getitemcountininventorybyname[`Long Sword`]", 0.0),
        ("getitemcountininventorybynamerx[`^Long`]", 0.0),
    ] {
        assert_eq!(
            eval_number(&mut rt, &mut host, source),
            expected,
            "{source}"
        );
    }
}

fn landscape(
    host: &mut FakeHost,
    id: u32,
    wcid: u32,
    name: &str,
    class: ObjectClass,
    item_type: u32,
    position: Position,
) {
    let mut item = object(id, wcid, name, class, item_type, 0, 0);
    item.is_landscape = true;
    item.position = Some(position);
    host.objects.push(item);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestByNameAndObjectClassMatchesTheBareName
#[test]
fn nearest_by_name_and_object_class_matches_the_bare_name() {
    let mut host = create_automation();
    landscape(
        &mut host,
        22,
        104,
        "Chest",
        ObjectClass::Container,
        0x200,
        Position {
            east_west: 10.2,
            ..automation_position()
        },
    );
    // 75 is Oak.
    host.properties
        .insert(22, properties(&[(131, 75)], &[], &[]));
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[wobjectfindbyid[22]]"),
        "Oak Chest"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[10,`^Chest$`]]"
        ),
        22.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectfindnearestbynameandobjectclass[10,`^Oak `]"
        ),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ListPatternFindersMatchTheDisplayNameCaseSensitively
#[test]
fn list_pattern_finders_match_the_display_name_case_sensitively() {
    let mut host = create_automation();
    owned(
        &mut host,
        12,
        102,
        "Long Sword",
        ObjectClass::MeleeWeapon,
        0x1,
    );
    landscape(
        &mut host,
        22,
        104,
        "Long Sword",
        ObjectClass::MeleeWeapon,
        0x1,
        Position {
            east_west: 10.2,
            ..automation_position()
        },
    );
    // 63 is Silver, 75 is Oak.
    host.properties
        .insert(12, properties(&[(131, 63)], &[], &[]));
    host.properties
        .insert(22, properties(&[(131, 75)], &[], &[]));
    let mut rt = new(&mut host);

    for (source, expected) in [
        ("listcount[wobjectfindallbynamerx[`Long Sword`]]", 2.0),
        ("listcount[wobjectfindallbynamerx[`^Silver `]]", 1.0),
        ("listcount[wobjectfindallbynamerx[`^Oak `]]", 1.0),
        (
            "listcount[wobjectfindallinventorybynamerx[`^Silver `]]",
            1.0,
        ),
        ("listcount[wobjectfindallinventorybynamerx[`^Oak `]]", 0.0),
        ("listcount[wobjectfindalllandscapebynamerx[`^Oak `]]", 1.0),
        (
            "listcount[wobjectfindalllandscapebynamerx[`^Silver `]]",
            0.0,
        ),
        ("listcount[wobjectfindallbynamerx[`long sword`]]", 0.0),
        (
            "listcount[wobjectfindallinventorybynamerx[`^silver `]]",
            0.0,
        ),
        ("listcount[wobjectfindalllandscapebynamerx[`^oak `]]", 0.0),
    ] {
        assert_eq!(
            eval_number(&mut rt, &mut host, source),
            expected,
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestByNameAndObjectClassTakesClassThenCaseSensitiveRegex
#[test]
fn nearest_by_name_and_object_class_takes_class_then_case_sensitive_regex() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    for (source, expected) in [
        (
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,`^Dru`]]",
            20.0,
        ),
        (
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,`Drudge`]]",
            20.0,
        ),
        ("wobjectfindnearestbynameandobjectclass[5,`^dru`]", 0.0),
        ("wobjectfindnearestbynameandobjectclass[6,`^Dru`]", 0.0),
    ] {
        assert_eq!(
            eval_number(&mut rt, &mut host, source),
            expected,
            "{source}"
        );
    }
}

fn assert_close(expected: f64, actual: f64, precision: i32) {
    let scale = 10f64.powi(precision);
    assert_eq!(
        (expected * scale).round_ties_even(),
        (actual * scale).round_ties_even(),
        "expected {expected}, got {actual}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: PositionsKeepTheHostsHeightReading
#[test]
fn positions_keep_the_hosts_height_reading() {
    let mut host = create_automation();
    host.navigation.position.elevation = 97.5 / 240.0;
    let below = Position {
        elevation: 94.0 / 240.0,
        ..host.navigation.position.clone()
    };
    landscape(
        &mut host,
        40,
        400,
        "Lower Chest",
        ObjectClass::Container,
        0x200,
        below,
    );
    let mut rt = new(&mut host);

    assert_close(
        97.5 / 240.0,
        eval_number(&mut rt, &mut host, "coordinategetz[getplayercoordinates[]]"),
        12,
    );
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "coordinatetostring[getplayercoordinates[]]"
        ),
        "20.00N, 10.00E, 0.41Z"
    );
    assert_close(
        3.5,
        eval_number(
            &mut rt,
            &mut host,
            "coordinatedistancewithz[getplayercoordinates[],wobjectgetphysicscoordinates[wobjectfindbyid[40]]]",
        ),
        9,
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestLookupCountsHeightInMetres
#[test]
fn nearest_lookup_counts_height_in_metres() {
    let mut host = create_automation();
    host.navigation.position.elevation = 97.5 / 240.0;
    let player = host.navigation.position.clone();
    landscape(
        &mut host,
        40,
        400,
        "Chest Below",
        ObjectClass::Container,
        0x200,
        Position {
            east_west: player.east_west + 0.8 / 240.0,
            elevation: 94.0 / 240.0,
            ..player.clone()
        },
    );
    landscape(
        &mut host,
        41,
        401,
        "Chest Beside",
        ObjectClass::Container,
        0x200,
        Position {
            east_west: player.east_west + 1.5 / 240.0,
            ..player
        },
    );
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindnearestbyobjectclass[10]]"
        ),
        41.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestLookupsExcludeSelfAndRankInThreeDimensions
#[test]
fn nearest_lookups_exclude_self_and_rank_in_three_dimensions() {
    let mut host = create_automation();
    let player = automation_position();
    landscape(
        &mut host,
        21,
        200,
        "Skywards Drudge",
        ObjectClass::Monster,
        0x10,
        Position {
            east_west: 10.05,
            elevation: player.elevation + 100.0,
            ..player
        },
    );
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "wobjectfindnearestbyobjectclass[24]"),
        0.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindnearestmonster[]]"
        ),
        20.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestByObjectClassSearchesEveryKnownObject
#[test]
fn nearest_by_object_class_searches_every_known_object() {
    let mut host = create_automation();
    let mut wand = object(
        30,
        300,
        "Wielded Wand",
        ObjectClass::WandStaffOrb,
        0x1,
        0,
        1,
    );
    wand.is_owned = true;
    host.objects.push(wand);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindnearestbyobjectclass[31]]"
        ),
        30.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NearestByTemplateTypeSearchesOnlyTheGround
#[test]
fn nearest_by_template_type_searches_only_the_ground() {
    let mut host = create_automation();
    let mut wand = object(
        30,
        300,
        "Wielded Wand",
        ObjectClass::WandStaffOrb,
        0x1,
        0,
        1,
    );
    wand.is_owned = true;
    host.objects.push(wand);
    let mut pick = object(
        31,
        301,
        "Packed Lockpick",
        ObjectClass::Lockpick,
        0x1,
        11,
        0,
    );
    pick.is_owned = true;
    host.objects.push(pick);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "wobjectfindnearestbytemplatetype[301]"),
        0.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "wobjectfindnearestbytemplatetype[300]"),
        0.0
    );
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "wobjectfindnearestbytemplatetype[301]!=0"
    ));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgettemplatetype[wobjectfindnearestbytemplatetype[200]]"
        ),
        200.0
    );

    landscape(
        &mut host,
        32,
        301,
        "Lockpick",
        ObjectClass::Lockpick,
        0x1,
        Position {
            east_west: 11.0,
            ..automation_position()
        },
    );

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetid[wobjectfindnearestbytemplatetype[301]]"
        ),
        32.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ActionFunctionsUseSharedSelectionInventoryMagicAndMovementCommands
#[test]
fn action_functions_use_shared_selection_inventory_magic_and_movement_commands() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    eval(&mut rt, &mut host, "actiontryselect[20]").unwrap();
    assert_eq!(host.selection.selected, Some(20));
    assert!(eval_truthy(&mut rt, &mut host, "actiontryuseitem[10]"));
    assert_eq!(host.used_object, 10);
    assert!(eval_truthy(&mut rt, &mut host, "setmotion[`Forward`,1]"));
    assert!(host.intents.last().unwrap().forward);
    assert!(eval_truthy(&mut rt, &mut host, "clearmotion[]"));
    assert_eq!(host.clear_movement_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: SelectingAlwaysReportsFalseEvenWhenItSucceeded
#[test]
fn selecting_always_reports_false_even_when_it_succeeded() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert!(!eval_truthy(&mut rt, &mut host, "actiontryselect[20]"));
    assert_eq!(host.selection.selected, Some(20));
}

fn with_margins(
    rt: &mut ExpressionRuntime,
    host: &mut FakeHost,
    margins: (i32, i32),
    source: &str,
) -> bool {
    let mut services = TestServices {
        margins: Some(margins),
        ..TestServices::default()
    };
    rt.evaluate(host, &mut services, source)
        .unwrap()
        .is_truthy()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastabilityAppliesTheHuntingAndBuffingSkillMarginsSeparately
#[test]
fn castability_applies_the_hunting_and_buffing_skill_margins_separately() {
    let mut host = create_automation();
    host.spells.insert(1001, spell(1001, 34, 250));
    let mut rt = new(&mut host);
    // The fake's War Magic is 300 buffed.
    let margins = (75, 25);

    assert!(!with_margins(
        &mut rt,
        &mut host,
        margins,
        "getcancastspell_hunt[1001]"
    ));
    assert!(with_margins(
        &mut rt,
        &mut host,
        margins,
        "getcancastspell_buff[1001]"
    ));

    host.has_components = false;
    assert!(!with_margins(
        &mut rt,
        &mut host,
        margins,
        "getcancastspell_buff[1001]"
    ));
    host.has_components = true;
    assert!(!with_margins(
        &mut rt,
        &mut host,
        margins,
        "getcancastspell_buff[2002]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastabilityFailsClosedWhenTheSchoolSkillIsUnknown
#[test]
fn castability_fails_closed_when_the_school_skill_is_unknown() {
    let mut host = create_automation();
    // The fake reports War Magic (34) only; Life Magic (33) is unknown.
    host.spells.insert(1001, spell(1001, 33, 1));
    let mut rt = new(&mut host);

    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_hunt[1001]"
    ));
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_buff[1001]"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1001]"),
        2.0
    );

    host.skills.push(ac_plugin_abi::character::Skill {
        skill_id: 33,
        name: "Life Magic".to_owned(),
        training: ac_plugin_abi::character::Training::Trained,
        current: 300,
        ..ac_plugin_abi::character::Skill::default()
    });
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_hunt[1001]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastabilityIsClosedForASpellWithNoSchool
#[test]
fn castability_is_closed_for_a_spell_with_no_school() {
    let mut host = create_automation();
    host.spells.insert(1003, spell(1003, 0, 0));
    let mut rt = new(&mut host);

    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_hunt[1003]"
    ));
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_buff[1003]"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1003]"),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastingReportsImpossibleNotYetAndBegunAndTakesOneStepFirst
#[test]
fn casting_reports_impossible_not_yet_and_begun_and_takes_one_step_first() {
    let mut host = create_automation();
    host.spells.insert(1001, spell(1001, 34, 10));
    let mut untargeted = spell(1002, 34, 10);
    untargeted.is_untargeted = true;
    host.spells.insert(1002, untargeted);
    host.equipment = vec![wand(0)];
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[9999]"),
        2.0
    );

    // Step one: the wand is not wielded yet.
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1002]"),
        0.0
    );
    assert!(!eval_truthy(&mut rt, &mut host, "actiontryequipanywand[]"));
    assert_eq!(host.equipped_object, 50);

    // Step two: wielded, but still in peace mode.
    host.equipment = vec![wand(0x0100_0000)];
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1002]"),
        0.0
    );
    assert_eq!(host.requested_mode, Some(CombatMode::Magic));

    // Ready: the cast begins.
    host.combat_mode = CombatMode::Magic;
    assert!(eval_truthy(&mut rt, &mut host, "actiontryequipanywand[]"));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1002]"),
        1.0
    );
    assert_eq!(host.last_cast, (1002, 0));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyidontarget[1001,20]"),
        1.0
    );
    assert_eq!(host.last_cast, (1001, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastByIdCastsAnyKnownSkilledSpellWithNoTarget
#[test]
fn cast_by_id_casts_any_known_skilled_spell_with_no_target() {
    let mut host = create_automation();
    // A self-targeted recall: it needs no target, but it is not an
    // "untargeted" spell either.
    let mut recall = spell(1004, 34, 10);
    "Recall".clone_into(&mut recall.name);
    recall.duration_seconds = 0.0;
    recall.is_self_targeted = true;
    host.spells.insert(1004, recall);
    host.spells.insert(1001, spell(1001, 34, 10));
    host.equipment = vec![wand(0x0100_0000)];
    host.combat_mode = CombatMode::Magic;
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1004]"),
        1.0
    );
    assert_eq!(host.last_cast, (1004, 0));

    host.has_components = false;
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getcancastspell_hunt[1004]"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1004]"),
        1.0
    );

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1001]"),
        1.0
    );
    assert_eq!(host.last_cast, (1001, 0));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyidontarget[1004,20]"),
        1.0
    );
    assert_eq!(host.last_cast, (1004, 20));

    host.unknown_spells.insert(1004);
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1004]"),
        2.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyidontarget[1004,20]"),
        1.0
    );

    host.cast_sent = false;
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1001]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CastByIdGoesThroughTheMacrosCasterWhenOneIsBound
#[test]
fn cast_by_id_goes_through_the_macros_caster_when_one_is_bound() {
    let mut host = create_automation();
    host.spells.insert(1001, spell(1001, 34, 10));
    host.equipment = vec![wand(0x0100_0000)];
    host.combat_mode = CombatMode::Magic;
    let mut rt = new(&mut host);
    let mut services = TestServices {
        casts: Some(Vec::new()),
        ..TestServices::default()
    };

    let mut number = |source: &str, host: &mut FakeHost| {
        rt.evaluate(host, &mut services, source)
            .unwrap()
            .as_number("")
            .unwrap()
    };
    assert_eq!(number("actiontrycastbyid[1001]", &mut host), 1.0);
    assert_eq!(number("actiontrycastbyidontarget[1001,20]", &mut host), 1.0);

    assert_eq!(services.casts.unwrap(), [(1001, None), (1001, Some(20))]);
    assert_eq!(host.last_cast, (0, 0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: MagicModeStepTakesNoStepWhileTheHostIsBusy
#[test]
fn magic_mode_step_takes_no_step_while_the_host_is_busy() {
    let mut host = create_automation();
    let mut untargeted = spell(1002, 34, 10);
    untargeted.is_untargeted = true;
    host.spells.insert(1002, untargeted);
    host.equipment = vec![wand(0)];
    set_busy(&mut host, true);
    let mut rt = new(&mut host);

    assert_eq!(eval_number(&mut rt, &mut host, "getbusystate[]"), 1.0);
    assert!(!eval_truthy(&mut rt, &mut host, "actiontryequipanywand[]"));
    assert_eq!(host.equipped_object, 0);
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrycastbyid[1002]"),
        0.0
    );
    assert_eq!(host.equipped_object, 0);
    assert_eq!(host.last_cast, (0, 0));

    set_busy(&mut host, false);
    assert!(!eval_truthy(&mut rt, &mut host, "actiontryequipanywand[]"));
    assert_eq!(host.equipped_object, 50);
}

fn login(id: u32, name: &str, active_index: i32) -> LoginCharacter {
    LoginCharacter {
        object_id: id,
        name: name.to_owned(),
        active_index,
        is_pending_delete: false,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: LoginFunctionsUseTheAuthoritativeSortedRosterAndOneShotOwner
#[test]
fn login_functions_use_the_authoritative_sorted_roster_and_one_shot_owner() {
    let mut host = create_automation();
    host.login.roster = vec![
        login(10, "Beta", 2),
        login(1, "Expression Tester", 0),
        login(30, "Mule", 1),
    ];
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[``]"),
        1.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[`bet`]"),
        0.0
    );
    assert!(eval_truthy(&mut rt, &mut host, "setnextlogin[`1`]"));
    assert_eq!(host.login.next_login_id, 30);
    assert!(eval_truthy(&mut rt, &mut host, "setnextlogin[`bet`]"));
    assert_eq!(host.login.next_login_id, 10);
    assert!(eval_truthy(&mut rt, &mut host, "clearnextlogin[]"));
    assert_eq!(host.login.next_login_id, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: LoginIndicesAreAlphabeticalWhateverOrderTheRosterArrivesIn
#[test]
fn login_indices_are_alphabetical_whatever_order_the_roster_arrives_in() {
    let mut host = create_automation();
    host.login.roster = vec![
        login(1, "Expression Tester", 0),
        login(30, "Zeke", 1),
        login(10, "Aardvark", 2),
    ];
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[`aard`]"),
        0.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[``]"),
        1.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[`zek`]"),
        2.0
    );

    assert!(eval_truthy(&mut rt, &mut host, "setnextlogin[`1`]"));
    assert_eq!(host.login.next_login_id, 30);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ChatboxSendsAndChatboxPasteOnlyStagesTheText
#[test]
fn chatbox_sends_and_chatbox_paste_only_stages_the_text() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    host.submitted.clear();

    assert_eq!(
        eval_string(&mut rt, &mut host, "chatbox[`/say hi`]"),
        "/say hi"
    );
    assert_eq!(host.submitted, ["/say hi"]);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "chatboxpaste[`/tell Bob, `]"
    ));
    assert_eq!(host.composed, "/tell Bob, ");
    assert_eq!(host.submitted, ["/say hi"]);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "chatboxpaste[chr[9]+`a`+chr[10]+`b`]"
    ));
    assert_eq!(host.composed, "ab");
    assert!(!eval_truthy(&mut rt, &mut host, "chatboxpaste[chr[9]]"));

    host.can_compose = false;
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "chatboxpaste[`/tell Bob, `]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ChatVerbsRequireStringsAndTheStatusColourAcceptsNegatives
#[test]
fn chat_verbs_require_strings_and_the_status_colour_accepts_negatives() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    host.submitted.clear();

    assert!(eval(&mut rt, &mut host, "chatbox[5]").is_err());
    assert!(eval(&mut rt, &mut host, "chatboxpaste[6]").is_err());
    assert!(host.submitted.is_empty());
    assert_eq!(host.composed, "");

    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "statushudcolored[`k`,`v`,0-1]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: UiVisibilityUsesNonZeroTruthAndLabelFailureIsAnError
#[test]
fn ui_visibility_uses_non_zero_truth_and_label_failure_is_an_error() {
    let mut host = create_automation();
    host.has_ui = true;
    host.ui.everything = true;
    host.ui.label_accepted = true;
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "uisetvisible[uigetcontrol[`view`,`control`],0.5]"
        ),
        0.5
    );
    assert_eq!(host.ui.last_visible, Some(true));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "uisetvisible[uigetcontrol[`view`,`control`],0-1]"
        ),
        -1.0
    );
    assert_eq!(host.ui.last_visible, Some(true));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "uisetvisible[uigetcontrol[`view`,`control`],0]"
        ),
        0.0
    );
    assert_eq!(host.ui.last_visible, Some(false));

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "uisetlabel[uigetcontrol[`view`,`control`],`Go`]"
        ),
        1.0
    );
    host.ui.label_accepted = false;
    assert!(
        eval(
            &mut rt,
            &mut host,
            "uisetlabel[uigetcontrol[`view`,`control`],`Go`]"
        )
        .is_err()
    );
}

fn remote_mule() -> NetworkClient {
    NetworkClient {
        client_id: 7,
        player_id: 70,
        name: "Remote Mule".to_owned(),
        world_name: "Coldeve".to_owned(),
        position: automation_position(),
        tags: vec!["mules".to_owned(), "trade".to_owned()],
        current_health: 90,
        current_mana: 70,
        current_stamina: 80,
        max_health: 100,
        max_mana: 100,
        max_stamina: 100,
        heading: 90.0,
        is_remote: false,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NetworkClientsReturnUtilityBeltDictionariesAndTagFiltering
#[test]
fn network_clients_return_utility_belt_dictionaries_and_tag_filtering() {
    let mut host = create_automation();
    host.peers = vec![remote_mule()];
    let mut rt = new(&mut host);

    let clients = eval(&mut rt, &mut host, "netclients[`mules`]").unwrap();
    let ExpressionValue::List(clients) = clients else {
        panic!("not a list");
    };
    assert_eq!(clients.len(), 1);
    let ExpressionValue::Dictionary(client) = clients.get(0).unwrap() else {
        panic!("not a dictionary");
    };
    assert_eq!(
        client.get("Name").unwrap().as_str("").unwrap(),
        "Remote Mule"
    );
    assert_eq!(client.get("PlayerId").unwrap().as_number("").unwrap(), 70.0);
    assert_eq!(client.get("Tags").unwrap().as_list("").unwrap().len(), 2);
    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[netclients[`combat`]]"),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NetworkCastsReturnWhatPeersReportedWithTagFiltering
#[test]
fn network_casts_return_what_peers_reported_with_tag_filtering() {
    let mut host = create_automation();
    host.peers = vec![remote_mule()];
    host.peer_casts = vec![
        PeerCast {
            sequence: 3,
            client_id: 7,
            caster_id: 70,
            target_id: 500,
            spell_id: 2074,
            effective_skill: 310,
            seconds_remaining: 42.5,
            landed: true,
            is_remote: false,
        },
        PeerCast {
            sequence: 4,
            client_id: 8,
            caster_id: 80,
            target_id: 500,
            spell_id: 2074,
            effective_skill: 280,
            seconds_remaining: 0.0,
            landed: false,
            is_remote: false,
        },
    ];
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[netcasts[]]"),
        2.0
    );
    for (key, expected) in [
        ("Sequence", 3.0),
        ("ClientId", 7.0),
        ("CasterId", 70.0),
        ("TargetId", 500.0),
        ("SpellId", 2074.0),
        ("EffectiveSkill", 310.0),
        ("SecondsRemaining", 42.5),
    ] {
        assert_eq!(
            eval_number(
                &mut rt,
                &mut host,
                &format!("dictgetitem[listgetitem[netcasts[],0],`{key}`]")
            ),
            expected,
            "{key}"
        );
    }
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "dictgetitem[listgetitem[netcasts[],0],`Landed`]"
    ));
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "dictgetitem[listgetitem[netcasts[],1],`Landed`]"
    ));

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[netcasts[`mules`]]"),
        1.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "dictgetitem[listgetitem[netcasts[`mules`],0],`ClientId`]"
        ),
        7.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[netcasts[`combat`]]"),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: DelayedExecutionUsesTickAndCanBeCancelled
#[test]
fn delayed_execution_uses_tick_and_can_be_cancelled() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    let mut services = TestServices::default();

    let id = eval_number(&mut rt, &mut host, "delayexec[100,`setvar[done,1]`]");
    rt.on_tick(&mut host, &mut services, 0.099);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`done`]"), 0.0);
    rt.on_tick(&mut host, &mut services, 0.001);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`done`]"), 1.0);

    let cancelled = eval_number(&mut rt, &mut host, "delayexec[1,`setvar[bad,1]`]");
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        &format!("clearexec[{cancelled}]")
    ));
    rt.on_tick(&mut host, &mut services, 1.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`bad`]"), 0.0);
    assert!((id - cancelled).abs() > f64::EPSILON);
}

fn host_on(storage: &SharedStorage) -> FakeHost {
    let mut host = create_automation();
    host.storage = storage.clone();
    host
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: PersistentAndGlobalVariablesRoundTripThroughPluginStorage
#[test]
fn persistent_and_global_variables_round_trip_through_plugin_storage() {
    let storage = SharedStorage::new();
    {
        let mut host = host_on(&storage);
        let mut first = new(&mut host);
        eval(
            &mut first,
            &mut host,
            "setpvar[`count`,42];setgvar[`names`,listcreate[`a`,`b`]]",
        )
        .unwrap();
        first.dispose(&mut host);
    }

    let mut host = host_on(&storage);
    let mut second = new(&mut host);
    assert_eq!(
        eval_number(&mut second, &mut host, "getpvar[`count`]"),
        42.0
    );
    assert_eq!(
        eval_number(&mut second, &mut host, "listcount[getgvar[`names`]]"),
        2.0
    );
    let keys = storage.keys();
    assert!(
        keys.iter()
            .any(|key| key.starts_with("expressions/persistent/"))
    );
    assert!(
        keys.iter()
            .any(|key| key.starts_with("expressions/global/"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ACharacterChangeSavesTheOldSetToTheOldCharacterAsUtilityBeltKeepsThemApart
#[test]
fn a_character_change_saves_the_old_set_to_the_old_character_as_utility_belt_keeps_them_apart() {
    let storage = SharedStorage::new();
    {
        let mut second = host_on(&storage);
        "Second".clone_into(&mut second.character_mut().name);
        let mut earlier = new(&mut second);
        eval(&mut earlier, &mut second, "@kept=2").unwrap();
        earlier.dispose(&mut second);
    }

    let mut host = host_on(&storage);
    "First".clone_into(&mut host.character_mut().name);
    let mut rt = new(&mut host);
    "Second".clone_into(&mut host.character_mut().name);

    assert_eq!(eval_number(&mut rt, &mut host, "@kept"), 2.0);
    eval(&mut rt, &mut host, "@mine=3").unwrap();
    "First".clone_into(&mut host.character_mut().name);
    assert!(!eval_truthy(&mut rt, &mut host, "testpvar[`kept`]"));
    assert!(!eval_truthy(&mut rt, &mut host, "testpvar[`mine`]"));
    "Second".clone_into(&mut host.character_mut().name);
    assert_eq!(eval_number(&mut rt, &mut host, "@mine"), 3.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: APersistentSetBeforeAnErrorIsSavedAsUtilityBeltSavesIt
#[test]
fn a_persistent_set_before_an_error_is_saved_as_utility_belt_saves_it() {
    let storage = SharedStorage::new();
    let mut host = host_on(&storage);
    let mut first = new(&mut host);

    assert!(eval(&mut first, &mut host, "@early=4;nosuchfunction[]").is_err());

    let mut other = host_on(&storage);
    let mut second = new(&mut other);
    assert_eq!(eval_number(&mut second, &mut other, "@early"), 4.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: APersistentListChangedInPlaceIsSavedAsUtilityBeltSavesIt
#[test]
fn a_persistent_list_changed_in_place_is_saved_as_utility_belt_saves_it() {
    let storage = SharedStorage::new();
    let mut host = host_on(&storage);
    let mut first = new(&mut host);
    eval(&mut first, &mut host, "@l=listcreate[1]").unwrap();
    eval(&mut first, &mut host, "listadd[@l,2]").unwrap();

    let mut other = host_on(&storage);
    let mut second = new(&mut other);
    assert_eq!(eval_display(&mut second, &mut other, "@l"), "[1,2]");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ThePersistentSetIsSavedOnlyWhenSomethingInItChanged
#[test]
fn the_persistent_set_is_saved_only_when_something_in_it_changed() {
    let storage = SharedStorage::new();
    let mut host = host_on(&storage);
    let mut rt = new(&mut host);
    eval(&mut rt, &mut host, "@l=listcreate[1]").unwrap();
    eval(&mut rt, &mut host, "listadd[@l,stopwatchcreate[]]").unwrap();
    assert_eq!(host.logged(ac_plugin_abi::LogLevel::Error).len(), 1);

    eval(&mut rt, &mut host, "listcount[@l]").unwrap();
    eval(&mut rt, &mut host, "1+1").unwrap();
    eval(&mut rt, &mut host, "@l{0}").unwrap();

    assert_eq!(host.logged(ac_plugin_abi::LogLevel::Error).len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: NumbersThatAreNotFiniteAreSavedAsUtilityBeltSavesThem
#[test]
fn numbers_that_are_not_finite_are_saved_as_utility_belt_saves_them() {
    let storage = SharedStorage::new();
    {
        let mut host = host_on(&storage);
        let mut first = new(&mut host);
        eval(
            &mut first,
            &mut host,
            "@avg=0/0;@fast=1/0;&low=-1/0;@after=5",
        )
        .unwrap();
        eval(&mut first, &mut host, "@later=6").unwrap();
        first.dispose(&mut host);
    }

    let mut host = host_on(&storage);
    let mut second = new(&mut host);
    assert!(eval_number(&mut second, &mut host, "@avg").is_nan());
    assert_eq!(eval_number(&mut second, &mut host, "@fast"), f64::INFINITY);
    assert_eq!(
        eval_number(&mut second, &mut host, "&low"),
        f64::NEG_INFINITY
    );
    assert_eq!(eval_number(&mut second, &mut host, "@after"), 5.0);
    assert_eq!(eval_number(&mut second, &mut host, "@later"), 6.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AValueThatCannotBeSavedIsRefusedWhenSetAsUtilityBeltRefusesIt
#[test]
fn a_value_that_cannot_be_saved_is_refused_when_set_as_utility_belt_refuses_it() {
    for source in ["@sw=stopwatchcreate[]", "setpvar[`sw`,stopwatchcreate[]]"] {
        let storage = SharedStorage::new();
        {
            let mut host = host_on(&storage);
            let mut first = new(&mut host);
            let error = eval_error(&mut first, &mut host, source);
            assert_eq!(
                reason(&error),
                "There is currently no support for serializing Stopwatch types",
                "{source}"
            );
            assert!(!eval_truthy(&mut first, &mut host, "testpvar[`sw`]"));
            eval(&mut first, &mut host, "@after=7").unwrap();
            first.dispose(&mut host);
        }

        let mut host = host_on(&storage);
        let mut second = new(&mut host);
        assert_eq!(eval_number(&mut second, &mut host, "@after"), 7.0);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AListThatCameToHoldAnUnsavableValueKeepsItsSavedFormAndTheRestSave
#[test]
fn a_list_that_came_to_hold_an_unsavable_value_keeps_its_saved_form_and_the_rest_save() {
    let storage = SharedStorage::new();
    {
        let mut host = host_on(&storage);
        let mut first = new(&mut host);
        eval(&mut first, &mut host, "@l=listcreate[1]").unwrap();
        eval(
            &mut first,
            &mut host,
            "listadd[@l,stopwatchcreate[]];@after=8",
        )
        .unwrap();
        first.dispose(&mut host);
    }

    let mut host = host_on(&storage);
    let mut second = new(&mut host);
    assert_eq!(eval_display(&mut second, &mut host, "@l"), "[1]");
    assert_eq!(eval_number(&mut second, &mut host, "@after"), 8.0);
}

fn on_account(storage: &SharedStorage, account: &str, name: &str, world: &str) -> FakeHost {
    let mut host = host_on(storage);
    let character = host.character_mut();
    account.clone_into(&mut character.account);
    name.clone_into(&mut character.name);
    world.clone_into(&mut character.world);
    host
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: GlobalVariablesAreSharedLiveByEveryClientOnTheSameServer
#[test]
fn global_variables_are_shared_live_by_every_client_on_the_same_server() {
    let storage = SharedStorage::folder();
    let mut leader_host = on_account(&storage, "leader-account", "Leader", "Coldeve");
    let mut follower_host = on_account(&storage, "follower-account", "Follower", "Coldeve");
    let mut elsewhere_host = on_account(
        &storage,
        "example-account",
        "Expression Tester",
        "Frostfell",
    );
    let mut leader = new(&mut leader_host);
    let mut follower = new(&mut follower_host);
    let mut elsewhere = new(&mut elsewhere_host);

    eval(
        &mut leader,
        &mut leader_host,
        "setgvar[`roster`,listcreate[`Leader`]]",
    )
    .unwrap();
    assert_eq!(
        eval_number(
            &mut follower,
            &mut follower_host,
            "listcount[getgvar[`roster`]]"
        ),
        1.0
    );
    assert!(!eval_truthy(
        &mut elsewhere,
        &mut elsewhere_host,
        "testgvar[`roster`]"
    ));

    eval(&mut follower, &mut follower_host, "&heartbeat=5").unwrap();
    assert_eq!(
        eval_number(&mut leader, &mut leader_host, "getgvar[`heartbeat`]"),
        5.0
    );

    eval(&mut follower, &mut follower_host, "cleargvar[`roster`]").unwrap();
    assert!(!eval_truthy(
        &mut leader,
        &mut leader_host,
        "testgvar[`roster`]"
    ));
    eval(&mut leader, &mut leader_host, "clearallgvars[]").unwrap();
    assert!(!eval_truthy(
        &mut follower,
        &mut follower_host,
        "testgvar[`heartbeat`]"
    ));
}

/// The C# runs the two writers on two threads; a plugin's session is one
/// thread, so here their runs interleave one by one, the order two clients
/// meeting on the storage lock would see.
// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ConcurrentClientsKeepEachOthersGlobalVariables
#[test]
fn concurrent_clients_keep_each_others_global_variables() {
    const PER_CLIENT: usize = 150;
    let storage = SharedStorage::folder();
    let mut first_host = on_account(&storage, "first-account", "Expression Tester", "Coldeve");
    let mut second_host = on_account(&storage, "second-account", "Expression Tester", "Coldeve");
    let mut first = new(&mut first_host);
    let mut second = new(&mut second_host);

    for index in 0..PER_CLIENT {
        for (runtime, host, prefix, other) in [
            (&mut first, &mut first_host, "a", "b"),
            (&mut second, &mut second_host, "b", "a"),
        ] {
            eval(
                runtime,
                host,
                &format!("setgvar[`{prefix}{index}`,{index}]"),
            )
            .unwrap();
            eval(runtime, host, &format!("getgvar[`{other}{index}`]")).unwrap();
            eval(runtime, host, "testgvar[`shared`]").unwrap();
            eval(runtime, host, &format!("setgvar[`shared`,`{prefix}`]")).unwrap();
        }
    }

    let mut reader_host = host_on(&storage);
    let mut reader = new(&mut reader_host);
    for index in 0..PER_CLIENT {
        #[allow(clippy::cast_precision_loss)]
        let expected = index as f64;
        assert_eq!(
            eval_number(
                &mut reader,
                &mut reader_host,
                &format!("getgvar[`a{index}`]")
            ),
            expected
        );
        assert_eq!(
            eval_number(
                &mut reader,
                &mut reader_host,
                &format!("getgvar[`b{index}`]")
            ),
            expected
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AGlobalListChangedInPlaceIsSavedForTheNextClient
#[test]
fn a_global_list_changed_in_place_is_saved_for_the_next_client() {
    let storage = SharedStorage::folder();
    let mut first_host = host_on(&storage);
    let mut first = new(&mut first_host);
    let mut second_host = on_account(&storage, "other-account", "Expression Tester", "Coldeve");
    let mut second = new(&mut second_host);

    eval(
        &mut first,
        &mut first_host,
        "setgvar[`roster`,listcreate[`Leader`]]",
    )
    .unwrap();
    assert_eq!(
        eval_number(
            &mut second,
            &mut second_host,
            "listadd[getgvar[`roster`],`Follower`];listcount[getgvar[`roster`]]"
        ),
        2.0
    );

    assert_eq!(
        eval_string(
            &mut first,
            &mut first_host,
            "listgetitem[getgvar[`roster`],1]"
        ),
        "Follower"
    );
}

fn memory_runtime() -> (FakeHost, ExpressionRuntime) {
    let mut host = host_on(&SharedStorage::new());
    let rt = new(&mut host);
    (host, rt)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AGlobalChangedInPlaceThroughTheShorthandIsSaved
#[test]
fn a_global_changed_in_place_through_the_shorthand_is_saved() {
    let (mut host, mut rt) = memory_runtime();
    eval(
        &mut rt,
        &mut host,
        "setgvar[`roster`,listcreate[`a`]];setgvar[`bag`,dictcreate[]]",
    )
    .unwrap();

    eval(
        &mut rt,
        &mut host,
        "listadd[&roster,`b`];dictadditem[&bag,`k`,1]",
    )
    .unwrap();

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[getgvar[`roster`]]"),
        2.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "dictsize[getgvar[`bag`]]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ABareRemovalFromAGlobalListIsSaved
#[test]
fn a_bare_removal_from_a_global_list_is_saved() {
    let (mut host, mut rt) = memory_runtime();
    eval(
        &mut rt,
        &mut host,
        "setgvar[`running`,listcreate[`Alpha`,`Beta`]]",
    )
    .unwrap();

    eval(&mut rt, &mut host, "listremove[getgvar[`running`],`Alpha`]").unwrap();

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[getgvar[`running`]]"),
        1.0
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "listgetitem[getgvar[`running`],0]"),
        "Beta"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: TheWriteBackOverwritesASetOrClearInTheSameExpression
#[test]
fn the_write_back_overwrites_a_set_or_clear_in_the_same_expression() {
    for source in [
        "listadd[getgvar[`r`],`x`];setgvar[`r`,listcreate[]]",
        "listadd[&r,`x`];&r=listcreate[]",
        "listadd[getgvar[`r`],`x`];cleargvar[`r`]",
        "listadd[&r,`x`];clearallgvars[]",
    ] {
        let (mut host, mut rt) = memory_runtime();
        eval(&mut rt, &mut host, "setgvar[`r`,listcreate[`a`]]").unwrap();

        eval(&mut rt, &mut host, source).unwrap();

        assert!(eval_truthy(&mut rt, &mut host, "testgvar[`r`]"), "{source}");
        assert_eq!(
            eval_number(&mut rt, &mut host, "listcount[getgvar[`r`]]"),
            2.0,
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AnyChangeIsWrittenBackEvenWhenTheContentsEndTheSame
#[test]
fn any_change_is_written_back_even_when_the_contents_end_the_same() {
    for (source, count) in [
        ("listadd[&r,`x`];listpop[&r];cleargvar[`r`]", 1.0),
        ("listclear[&e];cleargvar[`e`]", 0.0),
        (
            "dictadditem[&d,`k`,1];dictremovekey[&d,`k`];cleargvar[`d`]",
            0.0,
        ),
    ] {
        let (mut host, mut rt) = memory_runtime();
        eval(
            &mut rt,
            &mut host,
            "setgvar[`r`,listcreate[`a`]];setgvar[`e`,listcreate[]];setgvar[`d`,dictcreate[]]",
        )
        .unwrap();

        eval(&mut rt, &mut host, source).unwrap();

        let name = source[source.find("clear").unwrap()..]
            .split('`')
            .nth(1)
            .unwrap()
            .to_owned();
        assert!(
            eval_truthy(&mut rt, &mut host, &format!("testgvar[`{name}`]")),
            "{source}"
        );
        let probe = if name == "d" {
            "dictsize[getgvar[`d`]]".to_owned()
        } else {
            format!("listcount[getgvar[`{name}`]]")
        };
        assert_eq!(eval_number(&mut rt, &mut host, &probe), count, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AGlobalReadButNotChangedIsNotWrittenBack
#[test]
fn a_global_read_but_not_changed_is_not_written_back() {
    for (source, name) in [
        ("listcount[getgvar[`r`]];setgvar[`r`,5]", "r"),
        ("listremove[&r,`absent`];setgvar[`r`,5]", "r"),
        ("dictremovekey[&d,`absent`];setgvar[`d`,5]", "d"),
        ("dictclear[&z];setgvar[`z`,5]", "z"),
    ] {
        let (mut host, mut rt) = memory_runtime();
        eval(
            &mut rt,
            &mut host,
            "setgvar[`r`,listcreate[`a`]];setgvar[`d`,dictcreate[`k`,1]];setgvar[`z`,dictcreate[]]",
        )
        .unwrap();

        eval(&mut rt, &mut host, source).unwrap();

        assert_eq!(
            eval_number(&mut rt, &mut host, &format!("getgvar[`{name}`]")),
            5.0,
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ANestedOrUntrackedChangeIsNotSaved
#[test]
fn a_nested_or_untracked_change_is_not_saved() {
    let (mut host, mut rt) = memory_runtime();
    eval(
        &mut rt,
        &mut host,
        "setgvar[`nest`,listcreate[listcreate[1]]]",
    )
    .unwrap();

    eval(&mut rt, &mut host, "listadd[listgetitem[&nest,0],2]").unwrap();
    eval(
        &mut rt,
        &mut host,
        "setvar[`l`,listcreate[1]];setgvar[`copy`,getvar[`l`]];listadd[getvar[`l`],2]",
    )
    .unwrap();

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listcount[listgetitem[getgvar[`nest`],0]]"
        ),
        1.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[getgvar[`copy`]]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AFailedExpressionWritesNothingBack
#[test]
fn a_failed_expression_writes_nothing_back() {
    let (mut host, mut rt) = memory_runtime();
    eval(&mut rt, &mut host, "setgvar[`r`,listcreate[`a`]]").unwrap();

    assert!(eval(&mut rt, &mut host, "listadd[&r,`x`];nosuchfunction[]").is_err());

    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[getgvar[`r`]]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ADamagedGlobalVariableIsAnErrorOfTheExpressionThatReadsIt
#[test]
fn a_damaged_global_variable_is_an_error_of_the_expression_that_reads_it() {
    for damaged in [
        "{ not json",
        "   ",
        "null",
        "{\"Name\":\"r\",\"Value\":null}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":null}}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":\"list\",\"List\":[null]}}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":\"worldobject\",\"Number\":-1}}",
    ] {
        let storage = SharedStorage::new();
        let mut host = host_on(&storage);
        let mut rt = new(&mut host);
        eval(&mut rt, &mut host, "setgvar[`r`,1]").unwrap();
        let globals: Vec<String> = storage
            .keys()
            .into_iter()
            .filter(|key| key.starts_with("expressions/global/"))
            .collect();
        assert_eq!(globals.len(), 1);
        let key = globals[0].clone();
        eval(&mut rt, &mut host, "setgvar[`ok`,2]").unwrap();
        storage.insert(&key, damaged);

        assert!(eval(&mut rt, &mut host, "&r").is_err(), "{damaged}");
        assert!(
            eval(&mut rt, &mut host, "getgvar[`r`]").is_err(),
            "{damaged}"
        );
        assert!(
            eval(&mut rt, &mut host, "listadd[&r,1]").is_err(),
            "{damaged}"
        );
        assert_eq!(eval_number(&mut rt, &mut host, "&ok"), 2.0);
        assert!(eval_truthy(&mut rt, &mut host, "testgvar[`r`]"));
        assert!(eval_truthy(&mut rt, &mut host, "touchgvar[`r`]"));
        assert!(eval_truthy(&mut rt, &mut host, "cleargvar[`r`]"));
        assert!(!eval_truthy(&mut rt, &mut host, "testgvar[`r`]"));

        storage.insert(&key, damaged);
        assert_eq!(eval_number(&mut rt, &mut host, "setgvar[`r`,3]"), 3.0);
        assert_eq!(eval_number(&mut rt, &mut host, "&r"), 3.0);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AStorageFailureIsAnErrorOfTheExpression
#[test]
fn a_storage_failure_is_an_error_of_the_expression() {
    for (operation, source) in [
        ("read", "&r"),
        ("read", "testgvar[`r`]"),
        ("write", "setgvar[`r`,2]"),
        ("write", "&r=2"),
        ("write", "listadd[&l,1]"),
        ("delete", "cleargvar[`r`]"),
        ("delete", "clearallgvars[]"),
        ("list", "clearallgvars[]"),
    ] {
        for failure in ["the file is in use", "access denied"] {
            let storage = SharedStorage::new();
            let mut host = host_on(&storage);
            let mut rt = new(&mut host);
            eval(
                &mut rt,
                &mut host,
                "setgvar[`r`,1];setgvar[`l`,listcreate[]]",
            )
            .unwrap();
            storage.fail(operation, failure);

            let error = eval_error(&mut rt, &mut host, source);
            assert!(
                error.message().contains(failure),
                "{operation} {source}: {}",
                error.message()
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: TheOldPerAccountGlobalFileJoinsTheSharedSet
#[test]
fn the_old_per_account_global_file_joins_the_shared_set() {
    let storage = SharedStorage::new();
    {
        let mut host = host_on(&storage);
        let mut old = new(&mut host);
        old.dispose(&mut host);
    }
    let legacy_key = format!(
        "expressions/global/{}.json",
        hash("Coldeve\nexample-account")
    );
    assert_eq!(
        legacy_key,
        storage_key("global", "Coldeve\nexample-account")
    );
    storage.insert(
        &legacy_key,
        "{\"carried\":{\"Kind\":\"number\",\"Number\":7}}",
    );

    let mut host = host_on(&storage);
    let mut rt = new(&mut host);

    assert_eq!(eval_number(&mut rt, &mut host, "getgvar[`carried`]"), 7.0);
    assert!(storage.get(&legacy_key).is_none());
}

fn fellow(id: u32, name: &str, vitals: [u32; 6], distance: f32) -> FellowMember {
    FellowMember {
        object_id: id,
        name: name.to_owned(),
        current_health: vitals[0],
        max_health: vitals[1],
        current_stamina: vitals[2],
        max_stamina: vitals[3],
        current_mana: vitals[4],
        max_mana: vitals[5],
        distance,
        ..FellowMember::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: FellowshipFunctionsExposeTheAuthoritativeCompleteRoster
#[test]
fn fellowship_functions_expose_the_authoritative_complete_roster() {
    let mut host = create_automation();
    host.fellows = vec![
        fellow(1, "Expression Tester", [90, 100, 80, 100, 70, 100], 0.0),
        fellow(22, "Fellow Two", [50, 60, 40, 60, 30, 60], 1.0),
    ];
    host.fellowship = FellowshipState {
        is_in_fellowship: true,
        name: "Test Fellowship".to_owned(),
        leader_id: 1,
        is_open: true,
        is_locked: false,
        member_count: 2,
        ..FellowshipState::default()
    };
    let mut rt = new(&mut host);

    assert!(eval_truthy(&mut rt, &mut host, "getfellowshipstatus[]"));
    assert_eq!(
        eval_string(&mut rt, &mut host, "getfellowshipname[]"),
        "Test Fellowship"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getfellowshipcount[]"), 2.0);
    assert!(eval_truthy(&mut rt, &mut host, "getfellowshipisleader[]"));
    assert!(eval_truthy(&mut rt, &mut host, "getfellowshipcanrecruit[]"));
    assert_eq!(
        eval_string(&mut rt, &mut host, "getfellowname[1]"),
        "Fellow Two"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getfellowid[1]"), 22.0);
    assert_eq!(
        eval_number(&mut rt, &mut host, "listcount[getfellowids[]]"),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: DerethTimeFunctionsUseTheRuntimeWorldClockProjection
#[test]
fn dereth_time_functions_use_the_runtime_world_clock_projection() {
    let mut host = create_automation();
    host.world_time = WorldTime {
        is_available: true,
        game_ticks: 123_456.0,
        year: 142,
        month: 6,
        day: 17,
        hour: 10,
        month_name: "HarvestGain".to_owned(),
        hour_name: "Warmtide".to_owned(),
        is_day: true,
        minutes_until_day: 0.0,
        minutes_until_night: 12.5,
    };
    let mut rt = new(&mut host);

    assert_eq!(eval_number(&mut rt, &mut host, "getgameyear[]"), 142.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getgamemonth[]"), 6.0);
    assert_eq!(
        eval_string(&mut rt, &mut host, "getgamemonthname[6]"),
        "HarvestGain"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getgameday[]"), 17.0);
    assert_eq!(
        eval_string(&mut rt, &mut host, "getgamehourname[10]"),
        "Warmtide"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getgameticks[]"), 123_456.0);
    assert!(eval_truthy(&mut rt, &mut host, "getisday[]"));
    assert_eq!(
        eval_number(&mut rt, &mut host, "getminutesuntilnight[]"),
        12.5
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ExperienceMeterAccumulatesCanonicalXpAndLuminanceDeltas
#[test]
fn experience_meter_accumulates_canonical_xp_and_luminance_deltas() {
    let mut host = create_automation();
    host.properties.insert(
        1,
        properties(&[(5, 50), (96, 100)], &[(1, 1000), (6, 10)], &[]),
    );
    let mut rt = new(&mut host);
    let mut services = TestServices::default();
    rt.on_tick(&mut host, &mut services, 1.0);
    host.properties.insert(
        1,
        properties(&[(5, 50), (96, 100)], &[(1, 1100), (6, 15)], &[]),
    );
    rt.on_tick(&mut host, &mut services, 1.0);

    assert_eq!(eval_number(&mut rt, &mut host, "xptotal[]"), 100.0);
    assert_eq!(eval_number(&mut rt, &mut host, "lumtotal[]"), 5.0);
    assert_eq!(eval_number(&mut rt, &mut host, "xpduration[]"), 2.0);
    assert_eq!(eval_number(&mut rt, &mut host, "xpavg[]"), 180_000.0);
    assert!(eval_string(&mut rt, &mut host, "xpmeter[]").contains("100 XP"));
    assert!(eval_truthy(&mut rt, &mut host, "xpreset[]"));
    assert_eq!(eval_number(&mut rt, &mut host, "xptotal[]"), 0.0);
}

/// Delivers a server line the way the client does: the filters are asked
/// first, and a line one of them drops never reaches the transcript. True
/// when the line was shown.
fn deliver(rt: &mut ExpressionRuntime, host: &mut FakeHost, text: &str) -> bool {
    if !host.filters.is_empty() && rt.filter_chat(text) {
        return false;
    }
    host.add_chat(worded(0, 0, "", text, "", 0, "", 0));
    true
}

fn chat_line(sequence: u64, text: &str) -> ac_plugin_abi::ChatLine {
    let mut line = worded(0, 0, "", text, "", 0, "", 0);
    line.sequence = sequence;
    line
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AnOwnQuestRefreshHidesTheQuestLinesItReads
#[test]
fn an_own_quest_refresh_hides_the_quest_lines_it_reads() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    let mut services = TestServices::default();
    assert_eq!(host.submitted, ["/myquests"]);

    assert!(!deliver(
        &mut rt,
        &mut host,
        "killtaskdrudges - 3 solves (0)\"Drudges killed\" 10 0"
    ));
    assert!(deliver(&mut rt, &mut host, "Horan tells you, \"hi\""));
    rt.on_tick(&mut host, &mut services, 0.0);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "testquestflag[`killtaskdrudges`]"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "getquestktprogress[`killtaskdrudges`]"),
        3.0
    );
    assert!(host.posted.is_empty());

    rt.on_tick(&mut host, &mut services, 1.001);
    assert!(!eval_truthy(&mut rt, &mut host, "isrefreshingquests[]"));
    // The reference's ordinary line, in its System text class.
    assert_eq!(
        host.posted,
        [("[UB] Quest data updated.".to_owned(), UB_GENERIC_CHAT_TYPE)]
    );

    assert!(deliver(
        &mut rt,
        &mut host,
        "onevisit - 1 solves (1700000000)\"Visited once\" 1 0"
    ));
    rt.on_tick(&mut host, &mut services, 0.0);
    assert!(eval_truthy(&mut rt, &mut host, "testquestflag[`onevisit`]"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: TheQuestFilterGoesWithTheRuntime
#[test]
fn the_quest_filter_goes_with_the_runtime() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    assert_eq!(host.filters.len(), 1);

    rt.dispose(&mut host);

    assert!(host.filters.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: AKillTaskFlagAtOneSolveOfOneIsTimedAsUtilityBeltTreatsIt
#[test]
fn a_kill_task_flag_at_one_solve_of_one_is_timed_as_utility_belt_treats_it() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    host.chat.push(chat_line(
        1,
        "slayerquestbanderling - 1 solves (1700000000)\"Banderlings\" 1 60",
    ));
    host.chat.push(chat_line(
        2,
        "onevisit - 1 solves (1700000000)\"Visited once\" 1 60",
    ));
    rt.on_tick(&mut host, &mut TestServices::default(), 0.0);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "getqueststatus[`slayerquestbanderling`]"
    ));
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "getqueststatus[`SlayerQuestBanderling`]"
    ));
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getqueststatus[`onevisit`]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: QuestFunctionsParseTheAuthoritativeMyquestsTranscript
#[test]
fn quest_functions_parse_the_authoritative_myquests_transcript() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    let mut services = TestServices::default();
    assert_eq!(host.submitted, ["/myquests"]);
    assert!(eval_truthy(&mut rt, &mut host, "isrefreshingquests[]"));

    host.chat.push(chat_line(
        1,
        "killtaskdrudges - 3 solves (0)\"Drudges killed\" 10 0",
    ));
    host.chat.push(chat_line(
        2,
        "onevisit - 1 solves (1700000000)\"Visited once\" 1 0",
    ));
    rt.on_tick(&mut host, &mut services, 0.0);

    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "testquestflag[`killtaskdrudges`]"
    ));
    assert_eq!(
        eval_number(&mut rt, &mut host, "getquestktprogress[`killtaskdrudges`]"),
        3.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getquestktrequired[`killtaskdrudges`]"),
        10.0
    );
    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "getqueststatus[`onevisit`]"
    ));
    assert!(eval_truthy(&mut rt, &mut host, "getqueststatus[`unknown`]"));

    rt.on_tick(&mut host, &mut services, 1.001);
    assert!(!eval_truthy(&mut rt, &mut host, "isrefreshingquests[]"));
}

fn corpse(id: u32, wcid: u32, distance: f32, opened: bool) -> LootContainer {
    LootContainer {
        object_id: id,
        weenie_class_id: wcid,
        name: "Corpse".to_owned(),
        distance,
        has_been_opened: opened,
        ..LootContainer::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: CorpseFunctionsUseTheCanonicalExternalContainerHistory
#[test]
fn corpse_functions_use_the_canonical_external_container_history() {
    let mut host = create_automation();
    host.corpses = vec![corpse(30, 300, 2.0, false), corpse(31, 301, 3.0, true)];
    for id in [30, 31] {
        host.objects
            .push(object(id, 21, "Corpse", ObjectClass::Corpse, 0x200, 1, 0));
    }
    let mut rt = new(&mut host);

    assert!(!eval_truthy(
        &mut rt,
        &mut host,
        "hascorpsebeenopenedbyme[30]"
    ));
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "hascorpsebeenopenedbyme[31]"
    ));
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listgetitem[getcorpsesunopenedbyme[],0]"
        ),
        30.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: UnopenedCorpsesAreListedBySignedIdAsUtilityBeltListsThem
#[test]
fn unopened_corpses_are_listed_by_signed_id_as_utility_belt_lists_them() {
    const CORPSE: u32 = 0x8000_0030;
    let mut host = create_automation();
    host.corpses = vec![corpse(CORPSE, 300, 2.0, false)];
    host.objects.push(object(
        CORPSE,
        21,
        "Corpse",
        ObjectClass::Corpse,
        0x200,
        1,
        0,
    ));
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listgetitem[getcorpsesunopenedbyme[],0]"
        ),
        -2_147_483_600.0
    );
    assert!(eval_truthy(
        &mut rt,
        &mut host,
        "listcontains[getcorpsesunopenedbyme[],wobjectgetid[wobjectfindbyid[-2147483600]]]"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: ComponentFunctionsUseTheAuthenticDatCatalogProjection
#[test]
fn component_functions_use_the_authentic_dat_catalog_projection() {
    let mut host = create_automation();
    host.components.insert(
        7,
        ComponentInfo {
            component_id: 7,
            weenie_class_id: 101,
            name: "Malar Herb".to_owned(),
            burn_rate: 0.25,
            gesture_id: 0x1300_0001,
            gesture_speed: 0.75,
            icon_id: 0x0600_0010,
            sort_key: 4,
            component_type: "Herb".to_owned(),
            word: "Malar".to_owned(),
        },
    );
    let mut rt = new(&mut host);

    assert_eq!(
        eval_string(&mut rt, &mut host, "componentname[7]"),
        "Malar Herb"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "dictgetitem[componentdata[7],`BurnRate`]"
        ),
        0.25
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "dictgetitem[componentdata[7],`Word`]"),
        "Malar"
    );
}

fn inventory_item(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.cs :: UstFunctionsStageAndSubmitThroughTheCanonicalSalvageCommand
#[test]
fn ust_functions_stage_and_submit_through_the_canonical_salvage_command() {
    let mut host = create_automation();
    host.owned_items = vec![
        inventory_item(40, "Ust"),
        inventory_item(41, "Salvage One"),
        inventory_item(42, "Salvage Two"),
    ];
    for id in [41, 42] {
        let mut salvage = object(
            id,
            20981,
            "Salvage",
            ObjectClass::Salvage,
            0x4000_0000,
            1,
            0,
        );
        salvage.is_owned = true;
        host.objects.push(salvage);
    }
    let mut rt = new(&mut host);

    assert!(eval_truthy(&mut rt, &mut host, "ustopen[]"));
    assert_eq!(host.used_object, 40);
    assert!(eval_truthy(&mut rt, &mut host, "ustadd[41]"));
    assert!(eval_truthy(&mut rt, &mut host, "ustadd[42]"));
    assert!(eval_truthy(&mut rt, &mut host, "ustsalvage[]"));
    assert_eq!(host.last_salvage, Some((40, vec![41, 42])));
    assert!(!eval_truthy(&mut rt, &mut host, "ustsalvage[]"));
}

#[test]
fn a_failed_code_string_reports_in_the_error_class() {
    let mut host = create_automation();
    let mut rt = new(&mut host);
    eval(&mut rt, &mut host, "exec[`nosuchfunction[]`]").unwrap();
    assert!(
        host.posted
            .iter()
            .all(|(_, class)| *class == UB_ERROR_CHAT_TYPE)
    );
}
