//! The reference parser's verdicts and VTank's built-in function behaviour,
//! ported from the C# parity tests.

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::{number, string};
use mosstank_expr::value::ExpressionStopwatch;
use mosstank_expr::{
    CoreOptions, EvaluationContext, ExpressionError, ExpressionProgram, ExpressionState,
    ExpressionValue, create_default,
};

const REFERENCE_VERDICTS: &[(&str, bool)] = &[
    // A '!' not followed by '=' is dropped with the character after it.
    ("x[24,1]!0&&y[]==0", true),
    ("1!2", true),
    ("echo[a! b,13]", true),
    ("1!=2", true),
    ("!", false),
    ("a! =b", false),
    // A character no token starts with is dropped on its own; a '.' is
    // dropped with the character after it unless a number follows.
    ("a?b", true),
    ("a.b", true),
    (r"a\", true),
    ("1.", true),
    ("1.2.3", true),
    (".x", false),
    ("echo[a.]", false),
    ("`abc", false),
    // Numbers and hexadecimal: '0x' needs a digit and a lower-case x.
    ("0x", true),
    ("0X1", true),
    ("5abc", true),
    ("a:b", false),
    ("x[true ,1]", true),
    // After the second or later argument, any run of stray tokens is
    // skipped up to what may follow, and missing ']' are supplied.
    ("f[1,2)x)]", true),
    ("f[1,2 ) ) ]", true),
    ("f[1,2", true),
    ("f[1,g[2,3", true),
    ("f[g[2,3", true),
    ("f[1,2 3+4]", true),
    ("f[1,2 3;4", true),
    ("f[1,2 3=4]", true),
    ("f[1,2 3 x;y", true),
    ("f[1,g[2,3;5", true),
    ("(g[2,3", true),
    ("f[(g[2,3", true),
    ("(f[1,2 3)", true),
    ("max[1,3", true),
    ("max[1,3;4]", true),
    // After a first argument only one stray token is dropped.
    ("f[1)x)]", false),
    ("f[1 2 3]", false),
    ("f[1", false),
    ("f[1,g[2 3", false),
    ("g[f[1,2 3),4]", false),
    // Statements: after the second or later one, stray tokens are skipped
    // to the next ';' or the end; after the first, only one.
    ("1;2 3 4", true),
    ("1;2 3+4", true),
    ("(1;2 3)", true),
    ("1 2", true),
    ("1 2 3", false),
    ("(1))+2", false),
    // A missing ')' is supplied before anything that may follow any group,
    // call or statement it sits in.
    ("((1;2))", true),
    ("max[(1;2),3]", true),
    ("1+(2;3)*4", true),
    ("f[(1+2;3]", true),
    ("f[(1;2)]", true),
    ("f[1,(1+2", true),
    ("(1 2", false),
    ("(1 abc", false),
    ("f[1,(2 3 4)]", false),
    ("f[1{0 2]", false),
    ("getvar[a]<", false),
    ("istrue[getvar[a]==0&&&&getvar[b]==0]", false),
    // A string value must be followed by what may end the construct it sits
    // in when the next token ends constructs at all.
    ("(a;b)", false),
    ("(1;b)", false),
    ("f[(b,1)]", false),
    ("$b)", false),
    ("b]", false),
    ("f[1,b", false),
    ("f[1,b,c", false),
    ("f[b)]", false),
    ("$b=1", true),
    ("x{b:1}", true),
    ("f[1+b]", true),
    ("`s`;1", true),
    ("true)", true),
    ("5)", true),
    // Corpus shapes.
    (
        r"@RynCMDDebugMode==1&&echo[\[RynCMD\] Restocking Mana Scarabs, Platinum Scarabs, Prismatic Tapers, and Mana Charges. Threshold is 15 Mana Scarabs, 15 Platinum Scarabs, 600 Prismatic Tapers, and 1 Titan Mana Charge.]",
        false,
    ),
    (
        "listcount[listfilter[$list,`wobjectfindnearestbynameandobjectclass[24,$1]!0&&listcontains[getfellownames[],$1]==0`]]!=0",
        true,
    ),
    (
        "wobjectfindnearestbynameandobjectclass[24,$1]!0&&listcontains[getfellownames[],$1]==0",
        true,
    ),
    (
        r#"iif[a==1,0.09,iif[a==2,0.07,iif[a==3,0.05,iif[a==4,`2600`],0.03,0)")")")")]]"#,
        true,
    ),
    (
        r"@RynCMDDebugMode==1&&echo[\[RynCMD\] Restocking. Threshold is 5 Mana Scarabs.]",
        false,
    ),
    (r"chatbox[\/mt opt set Looting.AutoLootChests false]", true),
    (
        r"echo[\[RynCMD\] Exiting to RynCMD. Will continue routing,13]",
        true,
    ),
];

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionReferenceParserParityTests.cs :: TheParserAcceptsWhatTheReferenceParserAccepts
#[test]
fn the_parser_accepts_what_the_reference_parser_accepts() {
    for (source, accepted) in REFERENCE_VERDICTS {
        let result = ExpressionProgram::compile(source);
        if *accepted {
            assert!(result.is_ok(), "{source}: {:?}", result.err());
        } else {
            assert!(
                matches!(result, Err(ExpressionError::Parse { .. })),
                "{source} should be refused"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionReferenceParserParityTests.cs :: ACutLineRunsTheTextTheReferenceRuns
#[test]
fn a_cut_line_runs_the_text_the_reference_runs() {
    for (source, expected) in [
        (
            r"capture[\/mt opt set Looting.AutoLootChests false]",
            "/mt opt set Looting",
        ),
        (
            r"capture[\[RynCMD\] Exiting to RynCMD. Will continue routing,13]",
            "[RynCMD] Exiting to RynCMD",
        ),
        (
            r"capture[\[RynCMD\] Attempting to enable Vtank patch expression engine with \/ub bc \/ub opt set VTank.PatchExpressionEngine True,13]",
            "[RynCMD] Attempting to enable Vtank patch expression engine with /ub bc /ub opt set VTank",
        ),
        ("capture[a? b]", "a"),
    ] {
        let captured = Rc::new(RefCell::new(Vec::new()));
        let sink = captured.clone();
        let mut functions = create_default(&CoreOptions::seeded(1));
        functions.register(
            "capture",
            1,
            2,
            move |_, args| {
                sink.borrow_mut().push(args[0].to_display_string());
                Ok(ExpressionValue::one())
            },
            Some("capture[text,color?]"),
            "",
        );
        let mut state = ExpressionState::new();
        ExpressionProgram::compile(source)
            .unwrap()
            .evaluate(&mut EvaluationContext::new(&mut state, &functions))
            .unwrap();
        assert_eq!(*captured.borrow(), vec![expected.to_owned()], "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionReferenceParserParityTests.cs :: ABangWithoutEqualsDropsItselfAndTheNextCharacter
#[test]
fn a_bang_without_equals_drops_itself_and_the_next_character() {
    let functions = create_default(&CoreOptions::seeded(1));
    let mut state = ExpressionState::new();
    let value = ExpressionProgram::compile("5!0&&7")
        .unwrap()
        .evaluate(&mut EvaluationContext::new(&mut state, &functions))
        .unwrap();
    assert_eq!(value.as_number("").unwrap(), 7.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionVtankParityTests.cs :: ObjectInternalTypeReportsTheTokenType
#[test]
fn object_internal_type_reports_the_token_type() {
    for (source, expected) in [
        ("getobjectinternaltype[5]", 1.0),
        ("getobjectinternaltype[1==1]", 1.0),
        ("getobjectinternaltype[`text`]", 3.0),
        ("getobjectinternaltype[stopwatchcreate[]]", 7.0),
        (
            "getobjectinternaltype[coordinateparse[`10.0N, 20.0E`]]",
            7.0,
        ),
        ("getobjectinternaltype[listcreate[1,2]]", 7.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionVtankParityTests.cs :: NumberConversionAcceptsGroupSeparatorsAndFailsToZero
#[test]
fn number_conversion_accepts_group_separators_and_fails_to_zero() {
    for (source, expected) in [
        ("cnumber[`1,234`]", 1234.0),
        ("cnumber[`1,234.5`]", 1234.5),
        ("cnumber[`-12`]", -12.0),
        ("cnumber[`nonsense`]", 0.0),
    ] {
        assert_eq!(number(source), expected, "{source}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionVtankParityTests.cs :: StopwatchElapsedSecondsAreQuantisedToWholeMilliseconds
// (The Rust stopwatch reads a millisecond clock the host supplies; the C#
// spins on the real clock.)
#[test]
fn stopwatch_elapsed_seconds_are_quantised_to_whole_milliseconds() {
    let now = Rc::new(Cell::new(1_000_u64));
    let clock = now.clone();
    let stopwatch = ExpressionStopwatch::new(Rc::new(move || clock.get()));
    stopwatch.start();
    now.set(1_007);
    stopwatch.stop();
    let milliseconds = stopwatch.elapsed_seconds() * 1000.0;
    assert!((milliseconds.round() - milliseconds).abs() < 1e-9);
    assert_eq!(milliseconds.round(), 7.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExpressionVtankParityTests.cs :: FormattedStringsOnlyGoThroughHexForAWholeHexSpecifier
#[test]
fn formatted_strings_only_go_through_hex_for_a_whole_hex_specifier() {
    for (source, expected) in [
        ("cstrf[255,`X`]", "FF"),
        ("cstrf[255,`x`]", "ff"),
        ("cstrf[255,`X4`]", "00FF"),
        ("cstrf[3.5,`0.0 x`]", "3.5 x"),
        ("cstrf[3.5,`0.0 X`]", "3.5 X"),
        ("cstrf[3.5,`0.00`]", "3.50"),
    ] {
        assert_eq!(string(source), expected, "{source}");
    }
}
