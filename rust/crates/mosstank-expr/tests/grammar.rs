//! The expression grammar a profile is written against: the precedence
//! ladder, the atom forms and the reference parser's recovery, ported from
//! the C# conformance suite.
//!
//! The ladder, loosest to tightest: `|| &&` (one level), comparisons, `=`
//! (right to left), `#`, `+ -`, `* / %`, `& ^ |` (one level), `<< >>`, `~`
//! and a negative literal, `{index}` and `{slice}`.

mod common;

use common::{assert_close, evaluate, number, string, value};
use mosstank_expr::{
    ExpressionError, ExpressionState, ExpressionValueKind, ExpressionVariableScope,
};

fn get(state: &mut ExpressionState, name: &str) -> mosstank_expr::ExpressionValue {
    state.get(ExpressionVariableScope::Session, name).unwrap()
}

fn num(state: &mut ExpressionState, source: &str) -> f64 {
    value(source, state).as_number("").unwrap()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: BitwiseAndBindsTighterThanMultiplication
#[test]
fn bitwise_and_binds_tighter_than_multiplication() {
    assert_eq!(number("2 & 3 * 2"), 4.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: ShiftBindsTighterThanAddition
#[test]
fn shift_binds_tighter_than_addition() {
    assert_eq!(number("1 << 2 + 3"), 7.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: CaretIsAnExclusiveOrInsideTheBitwiseGroup
#[test]
fn caret_is_an_exclusive_or_inside_the_bitwise_group() {
    assert_eq!(number("2 ^ 3 * 2"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AnUnquotedStringKeepsATrailingSpaceButANameDoesNot
#[test]
fn an_unquoted_string_keeps_a_trailing_space_but_a_name_does_not() {
    let mut state = ExpressionState::new();
    assert_eq!(number("strlen[Blackfire ]"), 10.0);
    assert_eq!(string("listcreate[Blackfire , Sword]{0}"), "Blackfire ");
    assert_eq!(number("strlen[ Blackfire]"), 9.0);
    assert_eq!(string("listcreate[ Blackfire, Sword]{0}"), "Blackfire");
    assert_eq!(num(&mut state, "$x = 1;$x + 1"), 2.0);
    assert_eq!(get(&mut state, "x").as_number("").unwrap(), 1.0);
    assert_eq!(number("strlen [Blackfire]"), 9.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: EveryBitwiseOperatorRoundsItsOperandsRatherThanTruncating
#[test]
fn every_bitwise_operator_rounds_its_operands_rather_than_truncating() {
    for (source, expected) in [
        ("7.9 & 15", 8.0),
        ("7.9 | 0", 8.0),
        ("7.9 ^ 0", 8.0),
        ("7.9 << 0", 8.0),
        ("7.9 >> 0", 8.0),
        ("~7.9", -9.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AssignmentBindsTighterThanComparison
#[test]
fn assignment_binds_tighter_than_comparison() {
    let mut state = ExpressionState::new();
    assert_eq!(num(&mut state, "$x = 2 == 2"), 1.0);
    let stored = get(&mut state, "x");
    assert_eq!(stored.kind(), ExpressionValueKind::Number);
    assert_eq!(stored.as_number("").unwrap(), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: EveryOperatorPairBindsTheWayTheLadderSays
#[test]
fn every_operator_pair_binds_the_way_the_ladder_says() {
    for (source, expected) in [
        // '~' and a negative literal bind tighter than every binary operator.
        ("~1 << 2", -8.0),
        ("~1 & 3", 2.0),
        ("~1 * 2", -4.0),
        ("~1 + 2", 0.0),
        ("~(1 + 2)", -4.0),
        ("-2 * 3", -6.0),
        ("-2 + 3", 1.0),
        ("3 - -2", 5.0),
        ("-2 & 3", 2.0),
        ("- 2", -2.0),
        // Shift binds tighter than the bitwise group.
        ("3 & 1 << 2", 0.0),
        ("1 << 2 & 3", 0.0),
        ("12 >> 1 | 1", 7.0),
        // The bitwise group is one level, left to right.
        ("6 ^ 3 & 1", 1.0),
        ("4 | 3 & 1", 1.0),
        ("4 ^ 3 & 2", 2.0),
        ("1 & 2 | 3 ^ 4", 7.0),
        // The bitwise group binds tighter than multiplication.
        ("2 & 3 * 2", 4.0),
        ("2 ^ 3 * 2", 2.0),
        ("12 | 3 * 2", 30.0),
        ("2 * 3 & 4", 0.0),
        ("6 / 3 & 2", 3.0),
        ("10 % 3 | 4", 3.0),
        // Multiplication binds tighter than addition.
        ("1 + 2 * 3", 7.0),
        ("2 * 3 + 1", 7.0),
        ("10 - 6 / 2", 7.0),
        // Shift binds tighter than addition, from both sides.
        ("1 + 2 << 3", 17.0),
        ("1 << 2 + 3", 7.0),
        // Addition binds tighter than the regex operator.
        ("1 + 1 # 3", 0.0),
        ("1 # 1 + 1", 0.0),
        // Comparison binds tighter than the boolean operators.
        ("0 == 1 || 2", 2.0),
        ("3 || 1 == 2", 3.0),
        // The boolean operators are one level, left to right.
        ("1 || 0 && 0", 0.0),
        // Left to right within a level.
        ("1 - 2 - 3", -4.0),
        ("64 >> 2 >> 2", 4.0),
        ("1 << 2 << 3", 32.0),
        ("1 < 2 == 1", 1.0),
    ] {
        assert_close(expected, number(source), 10);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AssignmentTakesEverythingTighterThanAComparison
#[test]
fn assignment_takes_everything_tighter_than_a_comparison() {
    let mut state = ExpressionState::new();
    assert_eq!(num(&mut state, "$sum = 1 + 2"), 3.0);
    assert_eq!(get(&mut state, "sum").as_number("").unwrap(), 3.0);
    assert!(value("$hit = 5 # 5", &mut state).is_truthy());
    assert!(get(&mut state, "hit").is_truthy());

    assert_eq!(num(&mut state, "$flag = 1 && 0"), 0.0);
    assert_eq!(get(&mut state, "flag").as_number("").unwrap(), 1.0);

    assert_eq!(num(&mut state, "$a = $b = 3"), 3.0);
    assert_eq!(get(&mut state, "a").as_number("").unwrap(), 3.0);
    assert_eq!(get(&mut state, "b").as_number("").unwrap(), 3.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: IndexAndSliceFormsBindTighterThanEveryOperator
#[test]
fn index_and_slice_forms_bind_tighter_than_every_operator() {
    let mut state = ExpressionState::new();
    value("$l = listcreate[10,20,30,40]", &mut state);
    let mut text = |source: &str| value(source, &mut state).to_display_string();

    assert_eq!(text("$l{0}"), "10");
    assert_eq!(text("$l{-1}"), "40");
    assert_eq!(text("$l{1:}"), "[20,30,40]");
    assert_eq!(text("$l{:2}"), "[10,20]");
    assert_eq!(text("$l{1:3}"), "[20,30]");
    assert_eq!(text("$l{:}"), "[10,20,30,40]");
    assert_eq!(text("$l{1:3}{0}"), "20");

    assert_eq!(text("$l{0} + 1"), "11");
    assert_eq!(text("$l{0} & 3"), "2");
    assert_eq!(text("$l{1} + $l{0}"), "30");

    assert_eq!(text("$l{1 == 1}"), "20");
    assert_eq!(text("$l{0 + 1}"), "20");

    assert_eq!(string("`hello`{1}"), "e");
    assert_eq!(string("`hello`{1:3}"), "el");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: ParenthesesArgumentsAndBracesRestartAtTheLoosestLevel
#[test]
fn parentheses_arguments_and_braces_restart_at_the_loosest_level() {
    let mut state = ExpressionState::new();
    assert_eq!(number("(1 + 2) * 3"), 9.0);
    assert_eq!(number("listcreate[1 == 1]{0}"), 1.0);
    assert_eq!(number("listcount[listcreate[1 == 1, 2 || 3]]"), 2.0);
    assert_eq!(num(&mut state, "$x = (1 == 1)"), 1.0);
    assert_eq!(get(&mut state, "x").as_number("").unwrap(), 1.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AtomFormsParseAsAtoms
#[test]
fn atom_forms_parse_as_atoms() {
    assert_eq!(number("0xFF"), 255.0);
    assert_eq!(number("0x10 + 1"), 17.0);
    assert_eq!(number("0xff & 0x0f"), 15.0);
    assert_eq!(number("-2"), -2.0);
    assert_close(0.5, number(".5"), 10);
    assert_close(1.5, number("1.5"), 10);
    assert_eq!(string("`hello world`"), "hello world");
    assert_eq!(string(r"`a\`b`"), "a`b");
    assert_eq!(string(r"`\\`"), "\\");
    assert_eq!(string("Olthoi Noble"), "Olthoi Noble");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: OnlyTheBacktickDelimitsAString
#[test]
fn only_the_backtick_delimits_a_string() {
    assert_eq!(string("Olthoi's Claw"), "Olthoi's Claw");
    assert_eq!(
        string("listcreate[Olthoi's Claw, Gharu'ndim Robe]{0}"),
        "Olthoi's Claw"
    );
    assert_eq!(
        string("listcreate[Olthoi's Claw, Gharu'ndim Robe]{1}"),
        "Gharu'ndim Robe"
    );
    assert_eq!(string("'Neath Boots"), "'Neath Boots");
    assert_eq!(number("strlen['Neath Boots]"), 12.0);
    assert_eq!(string("\"hello\""), "\"hello\"");
    assert_eq!(string("Say \"Ho\" Staff"), "Say \"Ho\" Staff");
    assert_eq!(string("`Olthoi's Claw`"), "Olthoi's Claw");
    assert_eq!(string(r"`a\`b`"), "a`b");
    assert!(
        evaluate("Olthoi's Claw==`Olthoi's Claw`")
            .unwrap()
            .is_truthy()
    );
    assert!(
        evaluate("Olthoi's Claw == `Olthoi's Claw `")
            .unwrap()
            .is_truthy()
    );
    assert!(
        !evaluate("Olthoi's Claw == `Olthoi's Claw`")
            .unwrap()
            .is_truthy()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: HexadecimalLiteralsAreSigned32BitPatterns
#[test]
fn hexadecimal_literals_are_signed_32_bit_patterns() {
    assert_eq!(number("0xFFFFFFFF"), -1.0);
    assert_eq!(number("0x80000000"), -2_147_483_648.0);
    assert_eq!(number("0x7FFFFFFF"), 2_147_483_647.0);
    assert_eq!(number("0x00000000FF"), 255.0);
    assert!(evaluate("0xFFFFFFFF == 0 - 1").unwrap().is_truthy());
    assert_eq!(number("0xFFFFFFFF & 0xFF"), 255.0);
    assert_eq!(number("0x80000000 & 0x7FFFFFFF"), 0.0);
    let error = evaluate("0x100000000").unwrap_err();
    assert!(matches!(error, ExpressionError::Parse { .. }), "{error}");
    assert!(error.message().contains("32 bits"), "{error}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: ASemicolonInsideParenthesesRunsAsASequence
#[test]
fn a_semicolon_inside_parentheses_runs_as_a_sequence() {
    let mut state = ExpressionState::new();
    assert_eq!(num(&mut state, "(setvar[a,1];setvar[b,2])"), 2.0);
    assert_eq!(num(&mut state, "getvar[a]"), 1.0);
    assert_eq!(num(&mut state, "getvar[b]"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AStrayClosingTokenAtTheEndIsDropped
#[test]
fn a_stray_closing_token_at_the_end_is_dropped() {
    assert_eq!(number("1+(4-2))"), 3.0);
    assert_eq!(number("strlen[abc]]"), 3.0);
    assert_eq!(number("1;2)"), 2.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: AMissingParenthesisIsSuppliedBeforeWhatMayFollowIt
#[test]
fn a_missing_parenthesis_is_supplied_before_what_may_follow_it() {
    assert_eq!(number("round[2*(1+2]"), 6.0);
    assert_eq!(number("3*(1+2"), 9.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionGrammarConformanceTests.cs :: ABrokenExpressionStaysBroken
#[test]
fn a_broken_expression_stays_broken() {
    for source in [
        "istrue[getvar[a]==0&&&&getvar[b]==0]",
        "getvar[a]<",
        "1 2 3",
        "(1))+2",
        "(1 2",
        "(1 abc",
    ] {
        assert!(
            matches!(evaluate(source), Err(ExpressionError::Parse { .. })),
            "{source}: {:?}",
            evaluate(source).map(|value| value.to_display_string())
        );
    }
}
