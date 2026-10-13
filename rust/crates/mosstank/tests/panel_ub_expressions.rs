//! The UtilityBelt expression functions through the whole shell: the
//! audited set is registered, and the VTank ones reach the same running
//! settings the UB option functions do (ported from the C#'s
//! `MossTankPanelTests`).

mod common;

use common::World;
use common::automation::FakeAutomation;
use mosstank::MossTank;

/// The UtilityBelt expression functions the reference audit lists.
const AUDITED_FUNCTIONS: &[&str] = &[
    "abs",
    "acos",
    "actiontryapplyitem",
    "actiontrycastbyid",
    "actiontrycastbyidontarget",
    "actiontrydrop",
    "actiontryequipanywand",
    "actiontrygiveitem",
    "actiontrygiveprofile",
    "actiontrymove",
    "actiontryselect",
    "actiontrysplit",
    "actiontryuseitem",
    "asin",
    "atan",
    "atan2",
    "ceiling",
    "chatbox",
    "chatboxpaste",
    "chr",
    "clearallgvars",
    "clearallpvars",
    "clearallvars",
    "clearexec",
    "cleargvar",
    "clearmotion",
    "clearnextlogin",
    "clearpvar",
    "clearvar",
    "cnumber",
    "componentdata",
    "componentname",
    "coordinatedistanceflat",
    "coordinatedistancewithz",
    "coordinategetns",
    "coordinategetwe",
    "coordinategetz",
    "coordinateparse",
    "coordinatetostring",
    "cos",
    "cosh",
    "cstr",
    "cstrf",
    "delayexec",
    "dictadditem",
    "dictclear",
    "dictcopy",
    "dictcreate",
    "dictgetitem",
    "dicthaskey",
    "dictkeys",
    "dictremovekey",
    "dictsize",
    "dictvalues",
    "echo",
    "exec",
    "floor",
    "getaccounthash",
    "getbusystate",
    "getcancastspell_buff",
    "getcancastspell_hunt",
    "getcharacterindex",
    "getcharattribute_base",
    "getcharattribute_buffed",
    "getcharboolprop",
    "getcharburden",
    "getchardoubleprop",
    "getcharintprop",
    "getcharquadprop",
    "getcharskill_base",
    "getcharskill_buffed",
    "getcharskill_traininglevel",
    "getcharstringprop",
    "getcharvital_base",
    "getcharvital_buffedmax",
    "getcharvital_current",
    "getcombatstate",
    "getcontaineritemcount",
    "getcooldownexpiration",
    "getcorpsesunopenedbyme",
    "getdatetimelocal",
    "getdatetimeutc",
    "getequippedweapontype",
    "getfellowid",
    "getfellowids",
    "getfellowname",
    "getfellownames",
    "getfellowshipcanrecruit",
    "getfellowshipcount",
    "getfellowshipisfull",
    "getfellowshipisleader",
    "getfellowshipisopen",
    "getfellowshipleaderid",
    "getfellowshiplocked",
    "getfellowshipname",
    "getfellowshipstatus",
    "getfreecontainerslots",
    "getfreeitemslots",
    "getgameday",
    "getgamehour",
    "getgamehourname",
    "getgamemonth",
    "getgamemonthname",
    "getgameticks",
    "getgameyear",
    "getgvar",
    "getheading",
    "getheadingto",
    "getinventorycountbytemplatetype",
    "getisday",
    "getisnight",
    "getisspellknown",
    "getitemcountininventorybyname",
    "getitemcountininventorybynamerx",
    "getknownspells",
    "getminutesuntilday",
    "getminutesuntilnight",
    "getmotion",
    "getobjectinternaltype",
    "getplayercoordinates",
    "getplayerlandblock",
    "getplayerlandcell",
    "getpvar",
    "getquestktprogress",
    "getquestktrequired",
    "getqueststatus",
    "getregexmatch",
    "getspellexpiration",
    "getspellexpirationbyname",
    "getunixtime",
    "getvar",
    "getworldname",
    "hascorpsebeenopenedbyme",
    "hexstr",
    "ifthen",
    "iif",
    "isfalse",
    "isportaling",
    "isrefreshingquests",
    "istrue",
    "listadd",
    "listclear",
    "listcontains",
    "listcopy",
    "listcount",
    "listcreate",
    "listfilter",
    "listfromrange",
    "listgetitem",
    "listindexof",
    "listinsert",
    "listlastindexof",
    "listmap",
    "listpop",
    "listreduce",
    "listremove",
    "listremoveat",
    "listreverse",
    "listsort",
    "lumavg",
    "lumtotal",
    "netcasts",
    "netclients",
    "ord",
    "randint",
    "round",
    "setcombatstate",
    "setgvar",
    "setmotion",
    "setnextlogin",
    "setpvar",
    "setvar",
    "sin",
    "sinh",
    "spelldata",
    "spellname",
    "sqrt",
    "statushud",
    "statushudcolored",
    "stopwatchcreate",
    "stopwatchelapsedseconds",
    "stopwatchstart",
    "stopwatchstop",
    "strlen",
    "tan",
    "tanh",
    "testgvar",
    "testpvar",
    "testquestflag",
    "testvar",
    "tostring",
    "touchgvar",
    "touchpvar",
    "touchvar",
    "uboptget",
    "uboptset",
    "uigetcontrol",
    "uisetlabel",
    "uisetvisible",
    "uiviewexists",
    "uiviewvisible",
    "ustadd",
    "ustopen",
    "ustsalvage",
    "vitae",
    "vtgetmeta",
    "vtgetmetastate",
    "vtgetsetting",
    "vtmacroenabled",
    "vtsetmetastate",
    "vtsetsetting",
    "wobjectfindall",
    "wobjectfindallbycontainer",
    "wobjectfindallbynamerx",
    "wobjectfindallbyobjectclass",
    "wobjectfindallbytemplatetype",
    "wobjectfindallinventory",
    "wobjectfindallinventorybynamerx",
    "wobjectfindallinventorybyobjectclass",
    "wobjectfindallinventorybytemplatetype",
    "wobjectfindalllandscape",
    "wobjectfindalllandscapebynamerx",
    "wobjectfindalllandscapebyobjectclass",
    "wobjectfindalllandscapebytemplatetype",
    "wobjectfindbyid",
    "wobjectfindininventorybyname",
    "wobjectfindininventorybynamerx",
    "wobjectfindininventorybytemplatetype",
    "wobjectfindnearestbynameandobjectclass",
    "wobjectfindnearestbyobjectclass",
    "wobjectfindnearestbytemplatetype",
    "wobjectfindnearestdoor",
    "wobjectfindnearestmonster",
    "wobjectgetactivespellids",
    "wobjectgetboolprop",
    "wobjectgetdoubleprop",
    "wobjectgethealth",
    "wobjectgethealthvalue",
    "wobjectgetid",
    "wobjectgetintprop",
    "wobjectgetisdooropen",
    "wobjectgetmanavalue",
    "wobjectgetname",
    "wobjectgetobjectclass",
    "wobjectgetopencontainer",
    "wobjectgetphysicscoordinates",
    "wobjectgetplayer",
    "wobjectgetselection",
    "wobjectgetspellids",
    "wobjectgetstaminavalue",
    "wobjectgetstringprop",
    "wobjectgettemplatetype",
    "wobjecthasdata",
    "wobjectisvalid",
    "wobjectlastidtime",
    "wobjectrequestdata",
    "xpavg",
    "xpduration",
    "xpmeter",
    "xpreset",
    "xptotal",
];

fn evaluate(
    world: &mut World,
    plugin: &mut MossTank,
    source: &str,
) -> mosstank_expr::ExpressionValue {
    world
        .with_tank(plugin, |tank, client| {
            tank.evaluate_expression(client, source)
        })
        .unwrap()
}

/// `AttackRangeText`.
fn attack_range_text(plugin: &MossTank) -> String {
    format!(
        "Maximum target range: {:.0}m",
        plugin.tank().unwrap().settings().combat.maximum_range
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RegistersEveryAuditedUtilityBeltExpressionFunction
#[test]
fn registers_every_audited_utility_belt_expression_function() {
    let mut world = World::with_automation(FakeAutomation::default());
    let mut plugin = world.enable();
    let names = world.with_tank(&mut plugin, |tank, _| tank.expression_function_names());

    let mut missing: Vec<&str> = AUDITED_FUNCTIONS
        .iter()
        .copied()
        .filter(|expected| !names.iter().any(|name| name.eq_ignore_ascii_case(expected)))
        .collect();
    missing.sort_by_key(|name| name.to_lowercase());
    assert!(
        missing.is_empty(),
        "Missing UtilityBelt expression functions: {}",
        missing.join(", ")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UtilityBeltVtankExpressionsControlTheSameLiveOwners
#[test]
fn utility_belt_vtank_expressions_control_the_same_live_owners() {
    let mut world = World::with_automation(FakeAutomation::default());
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.act_text(p, "SelectMetaCondition", "Expression");
    world.act_text(p, "SetMetaConditionTextDraft", "vtmacroenabled[] == 1");
    world.act_text(p, "SelectMetaAction", "ExpressionAction");
    world.act_text(
        p,
        "SetMetaActionTextDraft",
        "vtsetsetting[`AttackDistance`,`0.175`] + vtsetmetastate[`Expression State`]",
    );
    world.act(p, "AddMetaRule");
    world.act(p, "ToggleMeta");
    world.act(p, "ToggleCombat");

    world.with_tank(p, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(p.tank().unwrap().meta_state(), "Expression State");
    assert_eq!(attack_range_text(p), "Maximum target range: 42m");
    assert!(evaluate(&mut world, p, "uboptset[`AttackDistance`,0.1375]").is_truthy());
    assert_eq!(
        evaluate(&mut world, p, "uboptget[`AttackDistance`]")
            .as_number("test")
            .unwrap(),
        0.1375
    );
    assert_eq!(attack_range_text(p), "Maximum target range: 33m");
}
