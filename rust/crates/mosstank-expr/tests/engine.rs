//! The expression language's behaviour, ported from the C# engine tests.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::{assert_close, display, evaluate, evaluate_in, number, string, value};
use mosstank_expr::{
    CoreOptions, EvaluationContext, ExpressionError, ExpressionProgram, ExpressionState,
    ExpressionValueKind, ExpressionVariableScope, Fault, create_default,
};

fn get(state: &mut ExpressionState, scope: ExpressionVariableScope, name: &str) -> f64 {
    state.get(scope, name).unwrap().as_number("").unwrap()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ArithmeticUsesUtilityBeltPrecedence
#[test]
fn arithmetic_uses_utility_belt_precedence() {
    for (source, expected) in [
        ("1+2*3", 7.0),
        ("(1+2)*3", 9.0),
        ("0x10+1", 17.0),
        ("~1", -2.0),
        ("16>>2", 4.0),
    ] {
        assert_close(expected, number(source), 10);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: BitwiseXorRoundsToIntegersAndBindsAboveArithmetic
#[test]
fn bitwise_xor_rounds_to_integers_and_binds_above_arithmetic() {
    for (source, expected) in [
        ("0^1", 1.0),
        ("7.9^3.2", 11.0),
        ("7.9^0", 8.0),
        ("1||0^1", 1.0),
        ("1||0&&0", 0.0),
        ("2^3==1", 1.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: StatementsAndAllVariableScopesReturnLastValue
#[test]
fn statements_and_all_variable_scopes_return_last_value() {
    let mut state = ExpressionState::new();
    let result = value(
        "$session=2;@persistent=3;&global=4;setvar[dynamic,5];$session+@persistent+&global+$dynamic",
        &mut state,
    );
    assert_eq!(result.as_number("").unwrap(), 14.0);
    assert_eq!(
        get(&mut state, ExpressionVariableScope::Session, "session"),
        2.0
    );
    assert_eq!(
        get(
            &mut state,
            ExpressionVariableScope::Persistent,
            "persistent"
        ),
        3.0
    );
    assert_eq!(
        get(&mut state, ExpressionVariableScope::Global, "global"),
        4.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: NumericAndComputedVariableNamesAreSupported
#[test]
fn numeric_and_computed_variable_names_are_supported() {
    let mut state = ExpressionState::new();
    let result = value("$1=7;$name=`chosen`;$getvar[name]=9;$1+$chosen", &mut state);
    assert_eq!(result.as_number("").unwrap(), 16.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: BooleanOperatorsShortCircuitAndReturnUtilityBeltValues
#[test]
fn boolean_operators_short_circuit_and_return_utility_belt_values() {
    let mut functions = create_default(&CoreOptions::seeded(1234));
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    functions.register(
        "boom",
        0,
        0,
        move |_, _| {
            counted.set(counted.get() + 1);
            Err(Fault::runtime("must not run"))
        },
        None,
        "",
    );
    let mut state = ExpressionState::new();
    let mut context = EvaluationContext::new(&mut state, &functions);
    let run = |source: &str, context: &mut EvaluationContext<'_>| {
        ExpressionProgram::compile(source)
            .unwrap()
            .evaluate(context)
            .unwrap()
            .as_number("")
            .unwrap()
    };
    assert_eq!(run("0&&boom[]", &mut context), 0.0);
    assert_eq!(run("3||boom[]", &mut context), 3.0);
    assert_eq!(calls.get(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AndWithAFalseRightSideAnswersZeroAsInUtilityBelt
#[test]
fn and_with_a_false_right_side_answers_zero_as_in_utility_belt() {
    for source in ["1&&``", "`a`&&0.0"] {
        let result = evaluate(source).unwrap();
        assert_eq!(result.kind(), ExpressionValueKind::Number, "{source}");
        assert_eq!(result.as_number("").unwrap(), 0.0, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AndWithBothSidesTrueAnswersTheRightSideAsInUtilityBelt
#[test]
fn and_with_both_sides_true_answers_the_right_side_as_in_utility_belt() {
    assert_eq!(string("`a`&&`b`"), "b");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AStringOnTheLeftComparesWithTheRightAsItPrintsAsInUtilityBelt
#[test]
fn a_string_on_the_left_compares_with_the_right_as_it_prints_as_in_utility_belt() {
    for (source, expected) in [
        ("`5`==5", 1.0),
        ("`5`!=5", 0.0),
        ("`2.5`==5/2", 1.0),
        ("`[1,A]`==listcreate[1,`a`]", 1.0),
        ("5==`5`", 0.0),
        ("5!=`5`", 1.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ACaptureGroupEqualsTheNumberItSpellsAsInUtilityBelt
#[test]
fn a_capture_group_equals_the_number_it_spells_as_in_utility_belt() {
    let mut state = ExpressionState::new();
    let result = value(
        "`level 5`#`level (?<n>[0-9]+)`;$capturegroup_n==5",
        &mut state,
    );
    assert_eq!(result.as_number("").unwrap(), 1.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ListFunctionsFindAnItemExactlyAsInUtilityBelt
#[test]
fn list_functions_find_an_item_exactly_as_in_utility_belt() {
    for (source, expected) in [
        ("listcontains[listcreate[Bob],bob]", 0.0),
        ("listcontains[listcreate[Bob],Bob]", 1.0),
        ("listindexof[listcreate[a,A],A]", 1.0),
        ("listlastindexof[listcreate[A,a,b],A]", 0.0),
        ("listcount[listremove[listcreate[Bob],bob]]", 1.0),
        ("listcontains[listcreate[`5`],5]", 0.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: StringsAreCaseInsensitiveAndPreserveDashedBareText
#[test]
fn strings_are_case_insensitive_and_preserve_dashed_bare_text() {
    assert!(evaluate("`Olthoi`==`olthoi`").unwrap().is_truthy());
    assert_eq!(string("Olthoi-Noble"), "Olthoi-Noble");
    assert_eq!(string("`Olthoi `+Noble"), "Olthoi Noble");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AStringMinusAStringIsItsSourceTextAsInUtilityBelt
#[test]
fn a_string_minus_a_string_is_its_source_text_as_in_utility_belt() {
    for (source, expected) in [
        ("$a=x;$b=y;$a-$b", "$a-$b"),
        ("`a`-`b`", "`a`-`b`"),
        ("$a=x;($a)-b-c", "($a)-b-c"),
        ("Olthoi-Noble", "Olthoi-Noble"),
    ] {
        assert_eq!(string(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: EscapedDelimitersRemainPartOfBareStrings
#[test]
fn escaped_delimiters_remain_part_of_bare_strings() {
    assert_eq!(string(r"\/vt nav load +cstr[3]"), "/vt nav load 3");
    assert_eq!(string(r"\/f Pick flowers\: +cstr[3]"), "/f Pick flowers: 3");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: RegexOperatorPublishesCaptureGroups
#[test]
fn regex_operator_publishes_capture_groups() {
    let mut state = ExpressionState::new();
    let result = value(
        "`Olthoi 275`#`(?<level>[0-9]+)`;$capturegroup_level",
        &mut state,
    );
    assert_eq!(result.as_str("").unwrap(), "275");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AFailedRegexMatchLeavesTheLastCapturesAsInUtilityBelt
#[test]
fn a_failed_regex_match_leaves_the_last_captures_as_in_utility_belt() {
    let mut state = ExpressionState::new();
    let result = value(
        "`Olthoi 275`#`(?<level>[0-9]+)`;`Olthoi`#`(?<level>[0-9]+)`;$capturegroup_level",
        &mut state,
    );
    assert_eq!(result.to_display_string(), "275");
    assert_eq!(
        value("`Olthoi`#`(?<level>[0-9]+)`", &mut state)
            .as_number("")
            .unwrap(),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ListsSupportMutationIndexSlicesAndHigherOrderFunctions
#[test]
fn lists_support_mutation_index_slices_and_higher_order_functions() {
    let mut state = ExpressionState::new();
    value("$0=old0;$1=old1;$2=old2", &mut state);
    let mut text = |source: &str| value(source, &mut state).to_display_string();
    assert_eq!(
        text("$items=listcreate[1,2,3];listadd[$items,4];listcount[$items]"),
        "4"
    );
    assert_eq!(text("$items{-1}"), "4");
    assert_eq!(text("$items{1:3}"), "[2,3]");
    assert_eq!(text("listmap[$items,`$1*2`]"), "[2,4,6,8]");
    assert_eq!(text("listfilter[$items,`$1%2==1`]"), "[1,3]");
    assert_eq!(text("listreduce[$items,`$2+$1`]"), "10");
    assert_eq!(text("listsort[$items,`$2-$1`]"), "[4,3,2,1]");
    // listfilter restores only after an item it keeps; listreduce restores
    // what it found; listsort leaves the last pair it compared.
    assert_eq!(get(&mut state, ExpressionVariableScope::Session, "0"), 3.0);
    assert_eq!(get(&mut state, ExpressionVariableScope::Session, "1"), 2.0);
    assert_eq!(get(&mut state, ExpressionVariableScope::Session, "2"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ListRangeCountsUpOnlyAsInUtilityBelt
#[test]
fn list_range_counts_up_only_as_in_utility_belt() {
    assert_eq!(display("listfromrange[1,3]"), "[1,2,3]");
    assert_eq!(display("listfromrange[2,2]"), "[2]");
    assert_eq!(display("listfromrange[3,1]"), "[]");
    assert_eq!(display("listfromrange[1.9,2.9]"), "[1,2]");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ListSortWithoutAnExpressionUsesTheDefaultOrderAsInUtilityBelt
#[test]
fn list_sort_without_an_expression_uses_the_default_order_as_in_utility_belt() {
    for (source, expected) in [
        ("listsort[listcreate[`a`,`_`]]", "[_,a]"),
        ("listsort[listcreate[`B`,`b`,`a`]]", "[a,b,B]"),
        ("listsort[listcreate[3,1,2]]", "[1,2,3]"),
        ("listsort[listcreate[listcreate[1]]]", "[[1]]"),
    ] {
        assert_eq!(display(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ListSortWithoutAnExpressionFailsOnValuesItCannotCompareAsInUtilityBelt
#[test]
fn list_sort_without_an_expression_fails_on_values_it_cannot_compare_as_in_utility_belt() {
    for source in [
        "listsort[listcreate[1,`a`]]",
        "listsort[listcreate[listcreate[1],listcreate[2]]]",
    ] {
        assert!(
            matches!(evaluate(source), Err(ExpressionError::Evaluation(_))),
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: DictionariesSupportMutationAndShallowCopy
#[test]
fn dictionaries_support_mutation_and_shallow_copy() {
    let mut state = ExpressionState::new();
    let mut run = |source: &str| value(source, &mut state);
    assert_eq!(
        run("$dict=dictcreate[a,1,b,2];$dict{b}")
            .as_number("")
            .unwrap(),
        2.0
    );
    assert!(!run("dictadditem[$dict,c,3]").is_truthy());
    assert!(run("dictadditem[$dict,c,4]").is_truthy());
    assert_eq!(run("dictsize[$dict]").as_number("").unwrap(), 3.0);
    assert_eq!(run("dictkeys[$dict]").to_display_string(), "[a,b,c]");
    assert_eq!(
        run("dictgetitem[dictcopy[$dict],c]").as_number("").unwrap(),
        4.0
    );
    assert!(run("dictremovekey[$dict,b]").is_truthy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: CollectionsRejectDirectAndIndirectCycles
#[test]
fn collections_reject_direct_and_indirect_cycles() {
    let mut state = ExpressionState::new();
    value("$first=listcreate[];$second=listcreate[$first]", &mut state);
    for source in ["listadd[$first,$first]", "listadd[$first,$second]"] {
        let error = evaluate_in(source, &mut state).unwrap_err();
        assert!(matches!(error, ExpressionError::Evaluation(_)));
        assert!(error.message().to_lowercase().contains("cyclic"), "{error}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: CoordinatesUseAcCompassScaleAndMeters
#[test]
fn coordinates_use_ac_compass_scale_and_meters() {
    assert_eq!(
        number("coordinategetns[coordinateparse[`12.5S, 3.0E`]]"),
        -12.5
    );
    assert_close(
        24.0,
        number("coordinatedistanceflat[coordinateparse[`0N, 0E`],coordinateparse[`0.1N, 0E`]]"),
        8,
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: CoordinateToStringHasTwoDecimalsAndAHeight
// (The C# also runs the last case under sv-SE; the Rust text is culture-free.)
#[test]
fn coordinate_to_string_has_two_decimals_and_a_height() {
    assert_eq!(
        string("coordinatetostring[coordinateparse[`12.5S, 3.0E`]]"),
        "12.50S, 3.00E, 0.00Z"
    );
    assert_eq!(
        string("coordinatetostring[coordinateparse[`12.55S, 3.14W, 240.0Z`]]"),
        "12.55S, 3.14W, 1.00Z"
    );
    assert_eq!(
        string("coordinatetostring[coordinateparse[`0.1N, 0E, -120.0Z`]]"),
        "0.10N, 0.00E, -0.50Z"
    );
    assert_eq!(
        string("coordinatetostring[coordinateparse[`36.5S, 28.9E, 57.6Z`]]"),
        "36.50S, 28.90E, 0.24Z"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: CoordinateParseReadsTheReferenceFormsIncludingZ
#[test]
fn coordinate_parse_reads_the_reference_forms_including_z() {
    assert_eq!(
        number("coordinategetns[coordinateparse[`41.34S, 44.09E, 0.48Z`]]"),
        -41.34
    );
    assert_eq!(
        number("coordinategetwe[coordinateparse[`41.34S, 44.09E, 0.48Z`]]"),
        44.09
    );
    assert_close(
        0.48 / 240.0,
        number("coordinategetz[coordinateparse[`41.34S, 44.09E, 0.48Z`]]"),
        12,
    );
    assert_close(
        -12.3 / 240.0,
        number("coordinategetz[coordinateparse[`1.2N 3.4W -12.3z`]]"),
        12,
    );
    assert_eq!(
        number("coordinategetwe[coordinateparse[`1.2N 3.4W -12.3z`]]"),
        -3.4
    );
    assert_eq!(
        number("coordinategetwe[coordinateparse[`at 1.2n,3.4 please`]]"),
        3.4
    );
    assert_eq!(number("coordinategetns[coordinateparse[`nowhere`]]"), 0.0);
    assert_eq!(number("coordinategetwe[coordinateparse[`nowhere`]]"), 0.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: InstructionBudgetAndCancellationBoundNestedEvaluation
#[test]
fn instruction_budget_and_cancellation_bound_nested_evaluation() {
    let functions = create_default(&CoreOptions::seeded(1234));
    let mut state = ExpressionState::new();
    let mut budgeted = EvaluationContext::with_budget(&mut state, &functions, 25);
    let error = ExpressionProgram::compile("listmap[listfromrange[1,100],`$1*2`]")
        .unwrap()
        .evaluate(&mut budgeted)
        .unwrap_err();
    assert!(matches!(error, ExpressionError::Evaluation(_)), "{error}");

    let mut state = ExpressionState::new();
    let mut cancelled =
        EvaluationContext::new(&mut state, &functions).with_cancellation(Rc::new(Cell::new(true)));
    let error = ExpressionProgram::compile("1+1")
        .unwrap()
        .evaluate(&mut cancelled)
        .unwrap_err();
    assert_eq!(error, ExpressionError::Cancelled);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: FunctionErrorsNameTheSignatureAndOffset
#[test]
fn function_errors_name_the_signature_and_offset() {
    let error = evaluate("sqrt[1,2]").unwrap_err();
    assert!(matches!(error, ExpressionError::Evaluation(_)));
    assert!(error.message().contains("sqrt[number]"), "{error}");
    assert!(error.message().contains("offset 0"), "{error}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: ARuntimeErrorInsideAnOperatorIsAnEvaluationErrorAsInUtilityBelt
#[test]
fn a_runtime_error_inside_an_operator_is_an_evaluation_error_as_in_utility_belt() {
    for source in [
        "(0/0)&1",
        "10000000000|0",
        "~(0/0)",
        "1<<(0/0)",
        "`abc`#`(`",
    ] {
        let Err(ExpressionError::Evaluation(error)) = evaluate(source) else {
            panic!("{source} should fail with an evaluation error");
        };
        assert_eq!(
            error.inner.as_deref(),
            Some(error.reason.as_str()),
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AnIndexIsCutTowardZeroAsInUtilityBelt
#[test]
fn an_index_is_cut_toward_zero_as_in_utility_belt() {
    for (source, expected) in [
        ("$l=listcreate[a,b,c];$l{listcount[$l]/2}", "b"),
        ("`abc`{1.9}", "b"),
        ("`abc`{-1.5}", "c"),
        ("$l=listcreate[a,b,c];tostring[$l{0:1.5}]", "[a]"),
    ] {
        assert_eq!(display(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionEngineTests.cs :: AnIndexWithNoWholeThirtyTwoBitPartIsOutOfRange
#[test]
fn an_index_with_no_whole_thirty_two_bit_part_is_out_of_range() {
    for source in ["`abc`{0/0}", "`abc`{9999999999}"] {
        assert!(
            matches!(evaluate(source), Err(ExpressionError::Evaluation(_))),
            "{source}"
        );
    }
}
