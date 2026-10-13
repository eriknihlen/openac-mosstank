//! The host's functions answering as the reference does: bool properties,
//! case-sensitive names, code strings, the argument check, lost objects,
//! world objects as text, spelldata, split and move. Ported from the C#'s
//! `HostExpressionFunctionsTests.ReferenceParity`.

mod common;

use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use common::*;
use mosstank_expr::{ExpressionValue, ExpressionValueKind};
use mosstank_meta::chat::UB_ERROR_CHAT_TYPE;
use mosstank_meta::host::SpellExtras;
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::testing::{FakeHost, SharedStorage};
use mosstank_stores::global_variables::hash;

fn new(host: &mut FakeHost) -> ExpressionRuntime {
    runtime(host)
}

fn on_storage(storage: &SharedStorage) -> FakeHost {
    let mut host = create_automation();
    host.storage = storage.clone();
    host
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ObjectBoolPropertyIsANumber
#[test]
fn object_bool_property_is_a_number() {
    let mut host = create_automation();
    host.objects.push(object(
        40,
        7010,
        "Ivory Robe",
        ObjectClass::Clothing,
        0x4,
        1,
        0,
    ));
    host.objects.push(object(
        41,
        7011,
        "Plain Robe",
        ObjectClass::Clothing,
        0x4,
        1,
        0,
    ));
    host.properties.insert(
        40,
        ItemProperties {
            bools: vec![(99, true), (100, false)],
            ..ItemProperties::default()
        },
    );
    let mut rt = new(&mut host);

    for (source, text) in [
        ("wobjectgetboolprop[wobjectfindbyid[40],99]", "1"),
        ("wobjectgetboolprop[wobjectfindbyid[40],100]", "0"),
        ("wobjectgetboolprop[wobjectfindbyid[40],24]", "0"),
        ("wobjectgetboolprop[wobjectfindbyid[41],99]", "0"),
    ] {
        let value = eval(&mut rt, &mut host, source).unwrap();
        assert_eq!(value.kind(), ExpressionValueKind::Number, "{source}");
        assert_eq!(value.to_display_string(), text, "{source}");
    }
    assert_eq!(
        eval_string(
            &mut rt,
            &mut host,
            "`ivory `+wobjectgetboolprop[wobjectfindbyid[40],99]"
        ),
        "ivory 1"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: GlobalVariableNamesAreCaseSensitive
#[test]
fn global_variable_names_are_case_sensitive() {
    let storage = SharedStorage::new();
    let mut host = on_storage(&storage);
    let mut rt = new(&mut host);

    eval(&mut rt, &mut host, "setgvar[`Rare`,1]").unwrap();
    assert!(!eval_truthy(&mut rt, &mut host, "testgvar[`rare`]"));
    assert_eq!(eval_number(&mut rt, &mut host, "&rare"), 0.0);
    assert!(!eval_truthy(&mut rt, &mut host, "cleargvar[`RARE`]"));
    eval(&mut rt, &mut host, "&rare=2").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "getgvar[`Rare`]"), 1.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getgvar[`rare`]"), 2.0);

    eval(
        &mut rt,
        &mut host,
        "setgvar[`L`,listcreate[]];setgvar[`l`,listcreate[]]",
    )
    .unwrap();
    eval(&mut rt, &mut host, "listadd[&L,1];listadd[&l,2]").unwrap();
    assert_eq!(eval_display(&mut rt, &mut host, "&L"), "[1]");
    assert_eq!(eval_display(&mut rt, &mut host, "&l"), "[2]");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: GlobalVariableNamesAreCaseSensitiveWithoutAStore
#[test]
fn global_variable_names_are_case_sensitive_without_a_store() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    eval(&mut rt, &mut host, "setgvar[`a`,1]").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "&A"), 0.0);
    assert!(!eval_truthy(&mut rt, &mut host, "testgvar[`A`]"));
    assert_eq!(eval_number(&mut rt, &mut host, "&a"), 1.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: AGlobalVariableSavedBeforeCaseSensitivityIsFoundByItsExactName
#[test]
fn a_global_variable_saved_before_case_sensitivity_is_found_by_its_exact_name() {
    let storage = SharedStorage::new();
    {
        let mut host = on_storage(&storage);
        let mut first = new(&mut host);
        first.dispose(&mut host);
    }
    let old_key = format!(
        "expressions/global/{}/{}.json",
        hash("Coldeve"),
        hash("CARRIED")
    );
    storage.insert(
        &old_key,
        "{\"Name\":\"Carried\",\"Value\":{\"Kind\":\"number\",\"Number\":7}}",
    );
    let mut host = on_storage(&storage);
    let mut rt = new(&mut host);

    assert_eq!(eval_number(&mut rt, &mut host, "getgvar[`Carried`]"), 7.0);
    assert!(eval_truthy(&mut rt, &mut host, "testgvar[`Carried`]"));
    assert!(!eval_truthy(&mut rt, &mut host, "testgvar[`carried`]"));
    assert_eq!(eval_number(&mut rt, &mut host, "&CARRIED"), 0.0);

    eval(&mut rt, &mut host, "setgvar[`Carried`,8]").unwrap();
    assert!(storage.get(&old_key).is_none());
    assert_eq!(eval_number(&mut rt, &mut host, "&Carried"), 8.0);
    assert!(eval_truthy(&mut rt, &mut host, "cleargvar[`Carried`]"));
    assert!(!eval_truthy(&mut rt, &mut host, "testgvar[`Carried`]"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ACodeStringRunsWithItsOwnGlobalVariableRecord
#[test]
fn a_code_string_runs_with_its_own_global_variable_record() {
    let storage = SharedStorage::new();
    let mut host = on_storage(&storage);
    let mut rt = new(&mut host);
    eval(&mut rt, &mut host, "setgvar[`L`,listcreate[]]").unwrap();

    assert_eq!(
        eval_number(&mut rt, &mut host, "listadd[&L,1];exec[`listcount[&L]`]"),
        0.0
    );
    assert_eq!(eval_display(&mut rt, &mut host, "&L"), "[1]");

    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listadd[&L,2];ifthen[1,`listadd[&L,3]`];listcount[&L]"
        ),
        2.0
    );
    assert_eq!(eval_display(&mut rt, &mut host, "&L"), "[1,2]");

    eval(
        &mut rt,
        &mut host,
        "listmap[listcreate[4,5],`listadd[&L,$1]`]",
    )
    .unwrap();
    assert_eq!(eval_display(&mut rt, &mut host, "&L"), "[1,2,4,5]");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: AnErrorInACodeStringEndsOnlyThatRun
#[test]
fn an_error_in_a_code_string_ends_only_that_run() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "exec[`listcount[5]`]+1"),
        1.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "ifthen[1,`listcount[5]`]"),
        0.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "setvar[x,1];exec[`setvar[x,2];listcount[5];setvar[x,3]`];getvar[x]"
        ),
        2.0
    );

    assert!(eval(&mut rt, &mut host, "exec[`1+`]").is_err());
    assert!(eval(&mut rt, &mut host, "listmap[listcreate[1],`1+`]").is_err());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ListFunctionsTakeAFailedRunAsTheReferenceDoes
#[test]
fn list_functions_take_a_failed_run_as_the_reference_does() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_display(
            &mut rt,
            &mut host,
            "listfilter[listcreate[listcreate[],2,listcreate[]],`listcount[$1]`]"
        ),
        "[2]"
    );
    assert_eq!(
        eval_display(
            &mut rt,
            &mut host,
            "listmap[listcreate[listcreate[7],2],`listcount[$1]`]"
        ),
        "[1,0]"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "listreduce[listcreate[listcreate[7],2,listcreate[7,8]],`$2+listcount[$1]`]"
        ),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ListFunctionsLeaveTheIterationVariablesAsTheReferenceDoes
#[test]
fn list_functions_leave_the_iteration_variables_as_the_reference_does() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    eval(&mut rt, &mut host, "listmap[listcreate[5],`$1`]").unwrap();
    assert!(eval_truthy(&mut rt, &mut host, "testvar[`0`]"));
    assert!(eval_truthy(&mut rt, &mut host, "testvar[`2`]"));
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 0.0);

    eval(&mut rt, &mut host, "setvar[`1`,9]").unwrap();
    eval(&mut rt, &mut host, "listfilter[listcreate[1,2],`$1==1`]").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 2.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`0`]"), 1.0);
    eval(&mut rt, &mut host, "setvar[`1`,9]").unwrap();
    eval(&mut rt, &mut host, "listfilter[listcreate[1,2],`$1==2`]").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 9.0);

    eval(&mut rt, &mut host, "setvar[`1`,9];setvar[`2`,9]").unwrap();
    eval(&mut rt, &mut host, "listreduce[listcreate[1,2],`$2+$1`]").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 9.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`2`]"), 9.0);

    eval(&mut rt, &mut host, "listsort[listcreate[3,1,2],`$1-$2`]").unwrap();
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 2.0);
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`2`]"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ListSortComparesAndOrdersAsTheReference
#[test]
fn list_sort_compares_and_orders_as_the_reference() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_display(
            &mut rt,
            &mut host,
            "listsort[listcreate[3,1,2],`($1-$2)/10`]"
        ),
        "[2,1,3]"
    );
    assert_eq!(
        eval_display(
            &mut rt,
            &mut host,
            "listsort[listcreate[21,12,22,11],`floor[$1/10]-floor[$2/10]`]"
        ),
        "[11,12,21,22]"
    );
    assert_eq!(
        eval_display(
            &mut rt,
            &mut host,
            "setvar[`1`,7];listsort[listcreate[5],`$1-$2`]"
        ),
        "[5]"
    );
    assert_eq!(eval_number(&mut rt, &mut host, "getvar[`1`]"), 7.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: AnUnknownObjectIdIsAnErrorBeforeTheFunctionRuns
#[test]
fn an_unknown_object_id_is_an_error_before_the_function_runs() {
    for (source, message) in [
        (
            "wobjectgetname[12345]",
            "wobjectgetname[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 12345",
        ),
        (
            "wobjectgetintprop[-7,5]",
            "wobjectgetintprop[WorldObject, number] expects argument #1/2 to be a WorldObject but an invalid (number)id was passed instead. Passed value: -7",
        ),
        (
            "actiontrymove[10,999]",
            "actiontrymove[WorldObject, WorldObject, number, number] expects argument #2/4 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
        (
            "actiontrycastbyidontarget[1,999]",
            "actiontrycastbyidontarget[number, WorldObject] expects argument #2/2 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
        (
            "wobjectisvalid[999]",
            "wobjectisvalid[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
        (
            "getfreecontainerslots[999]",
            "getfreecontainerslots[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
        (
            "hascorpsebeenopenedbyme[999]",
            "hascorpsebeenopenedbyme[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
        (
            "wobjectgethealth[0]",
            "wobjectgethealth[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 0",
        ),
        (
            "ustadd[999]",
            "ustadd[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999",
        ),
    ] {
        let mut host = create_automation();
        let mut rt = new(&mut host);

        let error = eval_error(&mut rt, &mut host, source);
        assert!(
            error.message().contains(message),
            "{source}: {}",
            error.message()
        );
        assert_eq!(
            eval_string(&mut rt, &mut host, "wobjectgetname[10]"),
            "Health Elixir"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ALostObjectFailsOnlyWhereTheFunctionReadsIt
#[test]
fn a_lost_object_fails_only_where_the_function_reads_it() {
    let mut host = create_automation();
    host.objects.push(object(
        50,
        7050,
        "Old Pack",
        ObjectClass::Container,
        0x200,
        1,
        0,
    ));
    let mut rt = new(&mut host);
    eval(&mut rt, &mut host, "$o=wobjectfindbyid[50]").unwrap();
    host.objects.retain(|object| object.object_id != 50);

    for source in [
        "wobjectgetname[$o]",
        "wobjectgetid[$o]",
        "wobjectgetobjectclass[$o]",
        "wobjectgettemplatetype[$o]",
        "wobjectgetintprop[$o,5]",
        "wobjectgetdoubleprop[$o,5]",
        "wobjectgetboolprop[$o,5]",
        "wobjectgetstringprop[$o,1]",
        "wobjecthasdata[$o]",
        "wobjectlastidtime[$o]",
        "wobjectgetisdooropen[$o]",
        "wobjectgetphysicscoordinates[$o]",
        "getheading[$o]",
        "getheadingto[$o]",
        "wobjectgethealth[$o]",
        "wobjectgetmanavalue[$o]",
        "wobjectgetspellids[$o]",
        "wobjectgetactivespellids[$o]",
        "getfreeitemslots[$o]",
        "getcontaineritemcount[$o]",
        "actiontryselect[$o]",
        "actiontryuseitem[$o]",
    ] {
        let error = eval_error(&mut rt, &mut host, source);
        assert!(
            error
                .message()
                .contains("Object reference not set to an instance of an object"),
            "{source}: {}",
            error.message()
        );
    }

    assert_eq!(eval_number(&mut rt, &mut host, "wobjectisvalid[$o]"), 0.0);
    assert_eq!(
        eval_number(&mut rt, &mut host, "getobjectinternaltype[$o]"),
        0.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "hascorpsebeenopenedbyme[$o]"),
        0.0
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getfreecontainerslots[$o]"),
        -1.0
    );
    eval(&mut rt, &mut host, "wobjectrequestdata[$o]").unwrap();
    eval(&mut rt, &mut host, "actiontrydrop[$o]").unwrap();
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "getobjectinternaltype[wobjectgetplayer[]]"
        ),
        7.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: TrueAndFalseAreTheNumbersOneAndZero
#[test]
fn true_and_false_are_the_numbers_one_and_zero() {
    for (source, expected) in [
        ("true", "1"),
        ("false", "0"),
        ("1==1", "1"),
        ("1<0", "0"),
        ("`a`==`A`", "1"),
        ("istrue[5]", "1"),
        ("isfalse[5]", "0"),
        ("testvar[`nothing here`]", "0"),
        ("tostring[istrue[1]]", "1"),
        ("`x`+(2>1)", "x1"),
        ("`x`+testvar[`nothing here`]", "x0"),
        ("listcreate[1==1,false]", "[1,0]"),
        ("$b=istrue[1];$b", "1"),
        ("wobjecthasdata[wobjectgetplayer[]]", "0"),
    ] {
        let mut host = create_automation();
        let mut rt = new(&mut host);

        let result = eval(&mut rt, &mut host, source).unwrap();

        assert_eq!(result.to_display_string(), expected, "{source}");
        if !matches!(
            result.kind(),
            ExpressionValueKind::String | ExpressionValueKind::List
        ) {
            assert_eq!(result.kind(), ExpressionValueKind::Number, "{source}");
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: SessionAndPersistentVariableNamesAreCaseSensitive
#[test]
fn session_and_persistent_variable_names_are_case_sensitive() {
    let storage = SharedStorage::new();
    {
        let mut host = on_storage(&storage);
        let mut rt = new(&mut host);
        eval(&mut rt, &mut host, "$a=1;$A=2").unwrap();
        assert_eq!(eval_number(&mut rt, &mut host, "$a"), 1.0);
        assert_eq!(eval_number(&mut rt, &mut host, "getvar[`A`]"), 2.0);
        assert!(!eval_truthy(&mut rt, &mut host, "testvar[`B`]"));
        eval(&mut rt, &mut host, "setvar[`b`,1]").unwrap();
        assert!(!eval_truthy(&mut rt, &mut host, "testvar[`B`]"));
        assert!(!eval_truthy(&mut rt, &mut host, "clearvar[`B`]"));

        eval(&mut rt, &mut host, "@p=1;@P=2").unwrap();
        assert_eq!(eval_number(&mut rt, &mut host, "@p"), 1.0);
        assert_eq!(eval_number(&mut rt, &mut host, "getpvar[`P`]"), 2.0);
        assert!(!eval_truthy(&mut rt, &mut host, "testpvar[`q`]"));
        rt.dispose(&mut host);
    }

    let mut host = on_storage(&storage);
    let mut reloaded = new(&mut host);
    assert_eq!(eval_number(&mut reloaded, &mut host, "@p"), 1.0);
    assert_eq!(eval_number(&mut reloaded, &mut host, "@P"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: APersistentVariableSavedBeforeCaseSensitivityIsFoundByItsExactName
#[test]
fn a_persistent_variable_saved_before_case_sensitivity_is_found_by_its_exact_name() {
    let storage = SharedStorage::new();
    {
        let mut host = on_storage(&storage);
        let mut first = new(&mut host);
        eval(&mut first, &mut host, "@Route=`north`").unwrap();
        first.dispose(&mut host);
    }
    let mut host = on_storage(&storage);
    let mut rt = new(&mut host);

    assert_eq!(eval_string(&mut rt, &mut host, "@Route"), "north");
    assert!(!eval_truthy(&mut rt, &mut host, "testpvar[`route`]"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: AFailedCodeStringRunIsReportedAsTheReferenceReportsIt
#[test]
fn a_failed_code_string_run_is_reported_as_the_reference_reports_it() {
    let mut host = create_automation();
    host.objects
        .push(object(60, 7060, "Gone", ObjectClass::Misc, 0x80, 1, 0));
    let mut rt = new(&mut host);
    eval(&mut rt, &mut host, "$o=wobjectfindbyid[60]").unwrap();
    host.objects.retain(|object| object.object_id != 60);

    eval(&mut rt, &mut host, "exec[`wobjectgetname[ $o ]`]").unwrap();
    eval(&mut rt, &mut host, "exec[`nosuchfunction[1, 2]`]").unwrap();

    assert_eq!(
        host.posted,
        [
            (
                "[UB] Error: Error running string expression: wobjectgetname[$o ]<EOF>".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
            (
                "[UB] Error: Exception has been thrown by the target of an invocation.".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
            (
                "[UB] Error: Error running string expression: nosuchfunction[1,2]<EOF>".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
            (
                "[UB] Error: Unknown expression method: nosuchfunction".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: NoBoolPropertyIsAnsweredWithoutAnAppraisal
#[test]
fn no_bool_property_is_answered_without_an_appraisal() {
    for key in [0x0C00_0000_u32, 0x0C00_0001, 2, 91] {
        let mut host = create_automation();
        let mut book = object(70, 7070, "Blank Book", ObjectClass::Book, 0x2000, 1, 0);
        book.is_owned = true;
        host.objects.push(book);
        let mut rt = new(&mut host);

        assert_eq!(
            eval_number(
                &mut rt,
                &mut host,
                &format!("wobjectgetboolprop[wobjectfindbyid[70],{key}]")
            ),
            0.0,
            "{key}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: AWorldObjectReadsAsItsHexIdAndName
#[test]
fn a_world_object_reads_as_its_hex_id_and_name() {
    let storage = SharedStorage::new();
    let mut host = on_storage(&storage);
    host.objects.push(object(
        0x8000_08B5,
        105,
        "Tome",
        ObjectClass::Book,
        0x2,
        1,
        0,
    ));
    let mut rt = new(&mut host);

    assert_eq!(
        eval_display(&mut rt, &mut host, "wobjectgetplayer[]"),
        "0x00000001: Expression Tester"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "tostring[wobjectfindbyid[-2147481419]]"),
        "0x800008B5: Tome"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "`is `+wobjectfindbyid[10]"),
        "is 0x0000000A: Health Elixir"
    );
    assert_eq!(
        eval_display(&mut rt, &mut host, "listcreate[wobjectfindbyid[10]]"),
        "[0x0000000A: Health Elixir]"
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectgetplayer[]==wobjectfindbyid[10]"
        ),
        0.0
    );
    assert_eq!(
        eval_number(
            &mut rt,
            &mut host,
            "wobjectfindbyid[10]==wobjectfindbyid[10]"
        ),
        1.0
    );

    eval(
        &mut rt,
        &mut host,
        "$tome=wobjectfindbyid[-2147481419];setgvar[`tome`,$tome]",
    )
    .unwrap();
    host.objects
        .retain(|object| object.object_id != 0x8000_08B5);
    assert_eq!(
        eval_string(&mut rt, &mut host, "tostring[$tome]"),
        "0x800008B5 (Invalid)"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "tostring[&tome]"),
        "0x800008B5 (Invalid)"
    );
    host.objects.push(object(
        0x8000_08B5,
        105,
        "Tome",
        ObjectClass::Book,
        0x2,
        1,
        0,
    ));
    assert_eq!(
        eval_string(&mut rt, &mut host, "tostring[&tome]"),
        "0x800008B5: Tome"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: FunctionNamesAreCaseSensitive
#[test]
fn function_names_are_case_sensitive() {
    for (source, name) in [
        ("ISTRUE[1]", "ISTRUE"),
        ("Wobjectgetplayer[]", "Wobjectgetplayer"),
        ("getVar[`x`]", "getVar"),
    ] {
        let mut host = create_automation();
        let mut rt = new(&mut host);

        let error = eval_error(&mut rt, &mut host, source);
        assert_eq!(error.reason(), format!("Unknown expression method: {name}"));
        assert!(eval(&mut rt, &mut host, "Setvar[`y`,1]").is_err());
        assert_eq!(eval_number(&mut rt, &mut host, "testvar[`y`]"), 0.0);
        assert!(eval(&mut rt, &mut host, "Istrue[setvar[`z`,1]]").is_err());
        assert_eq!(eval_number(&mut rt, &mut host, "testvar[`z`]"), 1.0);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ArgumentErrorsAreWordedAsTheReferenceWordsThem
#[test]
fn argument_errors_are_worded_as_the_reference_words_them() {
    for (source, message) in [
        (
            "listcount[5]",
            "listcount[List] expects argument #1/1 to be a List but a number was passed instead. Passed value: 5",
        ),
        (
            "abs[`a b`]",
            "abs[number] expects argument #1/1 to be a number but a string was passed instead. Passed value: a b",
        ),
        (
            "setvar[5,1]",
            "setvar[string, Object] expects argument #1/2 to be a string but a number was passed instead. Passed value: 5",
        ),
        (
            "dictgetitem[listcreate[1],`k`]",
            "dictgetitem[Dictionary, string] expects argument #1/2 to be a Dictionary but a List was passed instead. Passed value: [1]",
        ),
        (
            "coordinatetostring[stopwatchcreate[]]",
            "coordinatetostring[Coordinates] expects argument #1/1 to be a Coordinates but a Stopwatch was passed instead. Passed value: 0",
        ),
        (
            "wobjectgetintprop[wobjectgetplayer[],`x`]",
            "wobjectgetintprop[WorldObject, number] expects argument #2/2 to be a number but a string was passed instead. Passed value: x",
        ),
        (
            "wobjectgetname[`x`]",
            "wobjectgetname[WorldObject] expects argument #1/1 to be a WorldObject but a string was passed instead. Passed value: x",
        ),
        (
            "listcount[]",
            "listcount[List] expects 1 arguments. 0 arguments were passed.",
        ),
        (
            "abs[1,2]",
            "abs[number] expects 1 arguments. 2 arguments were passed.",
        ),
        (
            "echo[`x`]",
            "echo[string, number] expects 2 arguments. 1 arguments were passed.",
        ),
        (
            "getgvar[`a`,`b`]",
            "getgvar[string] expects 1 arguments. 3 arguments were passed.",
        ),
        (
            "wobjectgetname[]",
            "Index was out of range. Must be non-negative and less than the size of the collection.\r\nParameter name: index",
        ),
    ] {
        let mut host = create_automation();
        let mut rt = new(&mut host);

        let error = eval_error(&mut rt, &mut host, source);
        assert_eq!(error.reason(), message, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: TheReferenceArgumentCheckLetsTheReferenceFormsThrough
#[test]
fn the_reference_argument_check_lets_the_reference_forms_through() {
    let mut host = create_automation();
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "listpop[listcreate[1]]"),
        1.0
    );
    assert_eq!(eval_display(&mut rt, &mut host, "listcreate[]"), "[]");
    assert_eq!(
        eval_display(&mut rt, &mut host, "listcreate[1,`a`]"),
        "[1,a]"
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "wobjectgetname[10]"),
        "Health Elixir"
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "getcharacterindex[``]"),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: SpellDataIsADictionaryOfTheSpell
#[test]
fn spell_data_is_a_dictionary_of_the_spell() {
    let mut host = create_automation();
    let mut fire = spell(2001, 34, 275);
    "Burns.".clone_into(&mut fire.description);
    fire.icon_id = 0x0600_1234;
    fire.raw_flags = 0x4000 | 0x4 | 0x1;
    fire.spell_type = 1;
    fire.target_mask = 0x10;
    fire.formula_component_ids = vec![1, 2, 63];
    host.spells.insert(2001, fire);
    host.spell_extras.insert(
        2001,
        SpellExtras {
            caster_effect: 0x39,
            target_effect: 0x1F,
            formula_version: 2,
            display_order: 1417,
            component_loss: 0.25,
        },
    );
    let mut life = spell(2002, 33, 10);
    life.spell_type = 2;
    host.spells.insert(2002, life);
    let mut rt = new(&mut host);

    eval(&mut rt, &mut host, "$d=spelldata[2001]").unwrap();
    for (key, expected) in [
        ("Name", ExpressionValue::string("Spell 2001")),
        ("Description", ExpressionValue::string("Burns.")),
        ("School", ExpressionValue::string("War Magic")),
        ("Id", ExpressionValue::Number(2001.0)),
        ("Difficulty", ExpressionValue::Number(275.0)),
        ("Mana", ExpressionValue::Number(10.0)),
        ("Family", ExpressionValue::Number(1.0)),
        ("Duration", ExpressionValue::Number(30.0)),
        (
            "IconId",
            ExpressionValue::Number(f64::from(0x0600_1234_u32)),
        ),
        ("Flags", ExpressionValue::Number(f64::from(0x4005_u32))),
        ("Type", ExpressionValue::Number(1.0)),
        ("TargetMask", ExpressionValue::Number(16.0)),
        ("IsOffensive", ExpressionValue::Number(1.0)),
        ("IsIrresistible", ExpressionValue::Number(1.0)),
        ("IsFastWindup", ExpressionValue::Number(1.0)),
        ("IsUntargetted", ExpressionValue::Number(0.0)),
        ("IsDebuff", ExpressionValue::Number(0.0)),
        ("IsFellowship", ExpressionValue::Number(0.0)),
        ("CasterEffect", ExpressionValue::Number(57.0)),
        ("TargetEffect", ExpressionValue::Number(31.0)),
        ("Generation", ExpressionValue::Number(2.0)),
        ("SortKey", ExpressionValue::Number(1417.0)),
        ("Speed", ExpressionValue::Number(0.25)),
    ] {
        let actual = eval(&mut rt, &mut host, &format!("$d{{`{key}`}}")).unwrap();
        assert!(actual.same_value(&expected), "{key}: {actual:?}");
    }
    assert_eq!(
        eval_display(&mut rt, &mut host, "$d{`ComponentIds`}"),
        "[1,2,63]"
    );
    // Every key the reference answers, and no others.
    assert_eq!(eval_number(&mut rt, &mut host, "dictsize[$d]"), 24.0);

    assert_eq!(
        eval_number(&mut rt, &mut host, "spelldata[2002]{`Duration`}"),
        -1.0
    );
    assert_eq!(
        eval_string(&mut rt, &mut host, "spelldata[2002]{`School`}"),
        "Life Magic"
    );
    assert_eq!(
        eval_number(&mut rt, &mut host, "dictsize[spelldata[9999]]"),
        0.0
    );
    assert!(eval(&mut rt, &mut host, "spelldata[2001,`name`]").is_err());
}

fn money_and_pack(host: &mut FakeHost) {
    let mut pyreals = object(80, 300, "Pyreals", ObjectClass::Money, 0x40, 1, 0);
    pyreals.is_owned = true;
    host.objects.push(pyreals);
    let mut pack = object(81, 301, "Pack", ObjectClass::Container, 0x200, 1, 0);
    pack.is_owned = true;
    host.objects.push(pack);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ActionTrySplitTakesTheReferencesSlotAndMerge
#[test]
fn action_try_split_takes_the_references_slot_and_merge() {
    let mut host = create_automation();
    money_and_pack(&mut host);
    let mut rt = new(&mut host);

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrysplit[80,5,81,3]"),
        1.0
    );
    let last = |host: &FakeHost| *host.moves.last().unwrap();
    assert_eq!(last(&host), (80, 81, 5, 3, false));
    assert_eq!(host.selection.selected, Some(80));
    assert_eq!(eval_number(&mut rt, &mut host, "actiontrysplit[80,7]"), 1.0);
    assert_eq!(last(&host), (80, 1, 7, 0, false));
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrysplit[80,2,81,0,0]"),
        1.0
    );
    assert!(!last(&host).4);

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrysplit[80,2,81,0,1]"),
        1.0
    );
    assert_eq!(last(&host), (80, 81, 2, 0, true));

    set_busy(&mut host, true);
    let moves = host.moves.len();
    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrysplit[80,5,81,3]"),
        0.0
    );
    assert_eq!(host.moves.len(), moves);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HostExpressionFunctionsTests.ReferenceParity.cs :: ActionTryMoveJoinsAStackUnlessToldNotTo
#[test]
fn action_try_move_joins_a_stack_unless_told_not_to() {
    let mut host = create_automation();
    money_and_pack(&mut host);
    let mut rt = new(&mut host);
    let last = |host: &FakeHost| *host.moves.last().unwrap();

    assert_eq!(eval_number(&mut rt, &mut host, "actiontrymove[80,81]"), 1.0);
    assert_eq!(last(&host), (80, 81, 0, 0, true));

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrymove[80,81,2]"),
        1.0
    );
    assert_eq!(last(&host), (80, 81, 0, 2, true));

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrymove[80,81,3,0]"),
        1.0
    );
    assert_eq!(last(&host), (80, 81, 0, 3, false));

    assert_eq!(
        eval_number(&mut rt, &mut host, "actiontrymove[80,81,3,1]"),
        1.0
    );
    assert!(last(&host).4);
}
