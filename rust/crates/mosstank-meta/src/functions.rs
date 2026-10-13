//! The host's expression functions (`HostExpressionFunctions` and the
//! runtime's own: experience, quests, salvage, the status window, delayed
//! runs). Each is registered under its name and runs against the client
//! the evaluation carries ([`crate::runtime::Call`]).

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::magic::SpellList;
use ac_plugin_abi::navigation::NavigationStatus;
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use mosstank_dotnet::text::ordinal;
use mosstank_expr::value::signed_object_id;
use mosstank_expr::value::{ExpressionDictionary, ExpressionList};
use mosstank_expr::{
    EvaluationContext, EvaluationError, ExpressionCoordinates, ExpressionError, ExpressionValue,
    Fault, FunctionRegistry,
};

use crate::held_motions::{self, HeldMotion};
use crate::host::{Character, SpellExtras};
use crate::login_roster;
use crate::materials;
use crate::runtime::Call;
use crate::services::PeerNeed;
use crate::view_manager::MetaViewLabelResult;

type Result<T> = std::result::Result<T, Fault>;

const GAME_MONTH_NAMES: [&str; 12] = [
    "Morningthaw",
    "Solclaim",
    "Seedsow",
    "Leafdawning",
    "Verdantine",
    "Thistledown",
    "HarvestGain",
    "Leafcull",
    "Frostfell",
    "Snowreap",
    "Coldeve",
    "Wintersebb",
];

const GAME_HOUR_NAMES: [&str; 16] = [
    "Darktide",
    "Darktide-and-Half",
    "Foredawn",
    "Foredawn-and-Half",
    "Dawnsong",
    "Dawnsong-and-Half",
    "Morntide",
    "Morntide-and-Half",
    "Midsong",
    "Midsong-and-Half",
    "Warmtide",
    "Warmtide-and-Half",
    "Evensong",
    "Evensong-and-Half",
    "Gloaming",
    "Gloaming-and-Half",
];

/// Every host function: name, fewest and most arguments, signature, in
/// the C#'s registration order.
const HOST_FUNCTIONS: &[(&str, usize, usize, &str)] = &[
    // The character.
    ("getworldname", 0, 0, "getworldname[]"),
    ("getaccounthash", 0, 0, "getaccounthash[]"),
    ("getcharacterindex", 1, 1, "getcharacterindex[name]"),
    ("getplayercoordinates", 0, 0, "getplayercoordinates[]"),
    ("getplayerlandblock", 0, 0, "getplayerlandblock[]"),
    ("getplayerlandcell", 0, 0, "getplayerlandcell[]"),
    ("isportaling", 0, 0, "isportaling[]"),
    ("getcharburden", 0, 0, "getcharburden[]"),
    ("getcharintprop", 1, 1, "getcharintprop[property]"),
    ("getchardoubleprop", 1, 1, "getchardoubleprop[property]"),
    ("getcharquadprop", 1, 1, "getcharquadprop[property]"),
    ("getcharboolprop", 1, 1, "getcharboolprop[property]"),
    ("getcharstringprop", 1, 1, "getcharstringprop[property]"),
    (
        "getcharattribute_base",
        1,
        1,
        "getcharattribute_base[attributeId]",
    ),
    (
        "getcharattribute_buffed",
        1,
        1,
        "getcharattribute_buffed[attributeId]",
    ),
    ("getcharskill_base", 1, 1, "getcharskill_base[skillId]"),
    ("getcharskill_buffed", 1, 1, "getcharskill_buffed[skillId]"),
    (
        "getcharskill_traininglevel",
        1,
        1,
        "getcharskill_traininglevel[skillId]",
    ),
    ("getcharvital_base", 1, 1, "getcharvital_base[vitalId]"),
    (
        "getcharvital_buffedmax",
        1,
        1,
        "getcharvital_buffedmax[vitalId]",
    ),
    (
        "getcharvital_current",
        1,
        1,
        "getcharvital_current[vitalId]",
    ),
    ("vitae", 0, 0, "vitae[]"),
    // Spells.
    ("componentname", 1, 1, "componentname[componentid]"),
    ("componentdata", 1, 1, "componentdata[componentid]"),
    ("getisspellknown", 1, 1, "getisspellknown[spellId]"),
    ("getknownspells", 0, 0, "getknownspells[]"),
    ("spellname", 1, 1, "spellname[spellId]"),
    ("spelldata", 1, 1, "spelldata[spellId]"),
    ("getspellexpiration", 1, 1, "getspellexpiration[spellId]"),
    (
        "getspellexpirationbyname",
        1,
        1,
        "getspellexpirationbyname[name]",
    ),
    (
        "getcooldownexpiration",
        1,
        1,
        "getcooldownexpiration[cooldownId]",
    ),
    (
        "getcancastspell_buff",
        1,
        1,
        "getcancastspell_buff[spellId]",
    ),
    (
        "getcancastspell_hunt",
        1,
        1,
        "getcancastspell_hunt[spellId]",
    ),
    // Objects.
    ("wobjectfindbyid", 1, 1, "wobjectfindbyid[id]"),
    ("wobjectgetplayer", 0, 0, "wobjectgetplayer[]"),
    ("wobjectgetselection", 0, 0, "wobjectgetselection[]"),
    ("wobjectgetopencontainer", 0, 0, "wobjectgetopencontainer[]"),
    ("wobjectgetid", 1, 1, "wobjectgetid[object]"),
    ("wobjectgetname", 1, 1, "wobjectgetname[object]"),
    (
        "wobjectgetobjectclass",
        1,
        1,
        "wobjectgetobjectclass[object]",
    ),
    (
        "wobjectgettemplatetype",
        1,
        1,
        "wobjectgettemplatetype[object]",
    ),
    (
        "wobjectgetinternaltype",
        1,
        1,
        "wobjectgetinternaltype[object]",
    ),
    ("wobjecthasdata", 1, 1, "wobjecthasdata[object]"),
    ("wobjectlastidtime", 1, 1, "wobjectlastidtime[object]"),
    ("wobjectisvalid", 1, 1, "wobjectisvalid[object]"),
    ("wobjectrequestdata", 1, 1, "wobjectrequestdata[object]"),
    ("wobjectgetisdooropen", 1, 1, "wobjectgetisdooropen[object]"),
    (
        "wobjectgetphysicscoordinates",
        1,
        1,
        "wobjectgetphysicscoordinates[object]",
    ),
    ("getheading", 1, 1, "getheading[object]"),
    ("getheadingto", 1, 1, "getheadingto[object]"),
    (
        "wobjectgetintprop",
        2,
        2,
        "wobjectgetintprop[object,property]",
    ),
    (
        "wobjectgetdoubleprop",
        2,
        2,
        "wobjectgetdoubleprop[object,property]",
    ),
    (
        "wobjectgetboolprop",
        2,
        2,
        "wobjectgetboolprop[object,property]",
    ),
    (
        "wobjectgetstringprop",
        2,
        2,
        "wobjectgetstringprop[object,property]",
    ),
    ("wobjectgetspellids", 1, 1, "wobjectgetspellids[object]"),
    (
        "wobjectgetactivespellids",
        1,
        1,
        "wobjectgetactivespellids[object]",
    ),
    ("wobjectfindall", 0, 0, "wobjectfindall[]"),
    (
        "wobjectfindallbyobjectclass",
        1,
        1,
        "wobjectfindallbyobjectclass[value]",
    ),
    (
        "wobjectfindallbytemplatetype",
        1,
        1,
        "wobjectfindallbytemplatetype[value]",
    ),
    (
        "wobjectfindallbynamerx",
        1,
        1,
        "wobjectfindallbynamerx[pattern]",
    ),
    (
        "wobjectfindallinventorybyobjectclass",
        1,
        1,
        "wobjectfindallinventorybyobjectclass[value]",
    ),
    (
        "wobjectfindallinventorybytemplatetype",
        1,
        1,
        "wobjectfindallinventorybytemplatetype[value]",
    ),
    (
        "wobjectfindallinventorybynamerx",
        1,
        1,
        "wobjectfindallinventorybynamerx[pattern]",
    ),
    (
        "wobjectfindalllandscapebyobjectclass",
        1,
        1,
        "wobjectfindalllandscapebyobjectclass[value]",
    ),
    (
        "wobjectfindalllandscapebytemplatetype",
        1,
        1,
        "wobjectfindalllandscapebytemplatetype[value]",
    ),
    (
        "wobjectfindalllandscapebynamerx",
        1,
        1,
        "wobjectfindalllandscapebynamerx[pattern]",
    ),
    ("wobjectfindallinventory", 0, 0, "wobjectfindallinventory[]"),
    ("wobjectfindalllandscape", 0, 0, "wobjectfindalllandscape[]"),
    (
        "wobjectfindallbycontainer",
        1,
        1,
        "wobjectfindallbycontainer[container]",
    ),
    (
        "wobjectfindininventorybyname",
        1,
        1,
        "wobjectfindininventorybyname[name]",
    ),
    (
        "wobjectfindininventorybynamerx",
        1,
        1,
        "wobjectfindininventorybynamerx[pattern]",
    ),
    (
        "wobjectfindininventorybytemplatetype",
        1,
        1,
        "wobjectfindininventorybytemplatetype[templateType]",
    ),
    (
        "wobjectfindnearestbyobjectclass",
        1,
        1,
        "wobjectfindnearestbyobjectclass[...]",
    ),
    (
        "wobjectfindnearestbytemplatetype",
        1,
        1,
        "wobjectfindnearestbytemplatetype[...]",
    ),
    (
        "wobjectfindnearestbynameandobjectclass",
        2,
        2,
        "wobjectfindnearestbynameandobjectclass[objectClass,namePattern]",
    ),
    (
        "wobjectfindnearestdoor",
        0,
        0,
        "wobjectfindnearestdoor[...]",
    ),
    (
        "wobjectfindnearestmonster",
        0,
        0,
        "wobjectfindnearestmonster[...]",
    ),
    (
        "getitemcountininventorybyname",
        1,
        1,
        "getitemcountininventorybyname[name]",
    ),
    (
        "getitemcountininventorybynamerx",
        1,
        1,
        "getitemcountininventorybynamerx[pattern]",
    ),
    (
        "getinventorycountbytemplatetype",
        1,
        1,
        "getinventorycountbytemplatetype[templateType]",
    ),
    (
        "getcontaineritemcount",
        0,
        1,
        "getcontaineritemcount[container?]",
    ),
    ("getfreeitemslots", 0, 1, "getfreeitemslots[container?]"),
    (
        "getfreecontainerslots",
        0,
        1,
        "getfreecontainerslots[container?]",
    ),
    ("wobjectgethealth", 1, 1, "wobjectgethealth[object]"),
    (
        "wobjectgethealthvalue",
        1,
        1,
        "wobjectgethealthvalue[object]",
    ),
    (
        "wobjectgetstaminavalue",
        1,
        1,
        "wobjectgetstaminavalue[object]",
    ),
    ("wobjectgetmanavalue", 1, 1, "wobjectgetmanavalue[object]"),
    // Loot.
    (
        "hascorpsebeenopenedbyme",
        1,
        1,
        "hascorpsebeenopenedbyme[corpse]",
    ),
    ("getcorpsesunopenedbyme", 0, 0, "getcorpsesunopenedbyme[]"),
    // Fellowship.
    ("getfellowshipstatus", 0, 0, "getfellowshipstatus[]"),
    ("getfellowshipname", 0, 0, "getfellowshipname[]"),
    ("getfellowshipcount", 0, 0, "getfellowshipcount[]"),
    ("getfellowshipleaderid", 0, 0, "getfellowshipleaderid[]"),
    ("getfellowid", 1, 1, "getfellowid[index]"),
    ("getfellowname", 1, 1, "getfellowname[index]"),
    ("getfellowshiplocked", 0, 0, "getfellowshiplocked[]"),
    ("getfellowshipisleader", 0, 0, "getfellowshipisleader[]"),
    ("getfellowshipisopen", 0, 0, "getfellowshipisopen[]"),
    ("getfellowshipisfull", 0, 0, "getfellowshipisfull[]"),
    ("getfellowshipcanrecruit", 0, 0, "getfellowshipcanrecruit[]"),
    ("getfellownames", 0, 0, "getfellownames[]"),
    ("getfellowids", 0, 0, "getfellowids[]"),
    // World time.
    ("getgameyear", 0, 0, "getgameyear[]"),
    ("getgamemonth", 0, 0, "getgamemonth[]"),
    ("getgamemonthname", 1, 1, "getgamemonthname[monthIndex]"),
    ("getgameday", 0, 0, "getgameday[]"),
    ("getgamehour", 0, 0, "getgamehour[]"),
    ("getgamehourname", 1, 1, "getgamehourname[hourIndex]"),
    ("getminutesuntilday", 0, 0, "getminutesuntilday[]"),
    ("getminutesuntilnight", 0, 0, "getminutesuntilnight[]"),
    ("getgameticks", 0, 0, "getgameticks[]"),
    ("getisday", 0, 0, "getisday[]"),
    ("getisnight", 0, 0, "getisnight[]"),
    // Windows.
    ("uigetcontrol", 2, 2, "uigetcontrol[windowName,controlName]"),
    ("uisetlabel", 2, 2, "uisetlabel[control,label]"),
    ("uisetvisible", 2, 2, "uisetvisible[control,visible]"),
    ("uiviewexists", 1, 1, "uiviewexists[windowName]"),
    ("uiviewvisible", 1, 1, "uiviewvisible[windowName]"),
    // Actions.
    ("echo", 2, 2, "echo[text,color]"),
    ("chatbox", 1, 1, "chatbox[text]"),
    ("chatboxpaste", 1, 1, "chatboxpaste[text]"),
    ("actiontryselect", 1, 1, "actiontryselect[object]"),
    ("actiontryuseitem", 1, 1, "actiontryuseitem[object]"),
    (
        "actiontryapplyitem",
        2,
        2,
        "actiontryapplyitem[source,target]",
    ),
    ("actiontrygiveitem", 2, 2, "actiontrygiveitem[item,target]"),
    ("actiontrydrop", 1, 1, "actiontrydrop[item]"),
    (
        "actiontrymove",
        2,
        4,
        "actiontrymove[item,destination,slot?,addToStack?]",
    ),
    (
        "actiontrysplit",
        2,
        5,
        "actiontrysplit[item,newStackSize,destination?,slot?,merge?]",
    ),
    ("actiontrycastbyid", 1, 1, "actiontrycastbyid[spellId]"),
    (
        "actiontrycastbyidontarget",
        2,
        2,
        "actiontrycastbyidontarget[spellId,target]",
    ),
    ("actiontryequipanywand", 0, 0, "actiontryequipanywand[]"),
    // Combat and movement.
    ("getcombatstate", 0, 0, "getcombatstate[]"),
    ("setcombatstate", 1, 1, "setcombatstate[state]"),
    ("getbusystate", 0, 0, "getbusystate[]"),
    ("getequippedweapontype", 0, 0, "getequippedweapontype[]"),
    ("setmotion", 2, 2, "setmotion[motion,state]"),
    ("getmotion", 1, 1, "getmotion[motion]"),
    ("clearmotion", 0, 0, "clearmotion[]"),
    // Login.
    ("setnextlogin", 1, 1, "setnextlogin[nameOrRelativeIndex]"),
    ("clearnextlogin", 0, 0, "clearnextlogin[]"),
    // The other clients.
    ("netclients", 0, 1, "netclients[tag?]"),
    ("netcasts", 0, 1, "netcasts[tag?]"),
    // The runtime's own: experience.
    ("xpreset", 0, 0, "xpreset[]"),
    ("xpmeter", 0, 0, "xpmeter[]"),
    ("xpduration", 0, 0, "xpduration[]"),
    ("xptotal", 0, 0, "xptotal[]"),
    ("lumtotal", 0, 0, "lumtotal[]"),
    ("xpavg", 0, 0, "xpavg[]"),
    ("lumavg", 0, 0, "lumavg[]"),
    // Quests.
    ("testquestflag", 1, 1, "testquestflag[questflag]"),
    ("getqueststatus", 1, 1, "getqueststatus[questflag]"),
    ("getquestktprogress", 1, 1, "getquestktprogress[questflag]"),
    ("getquestktrequired", 1, 1, "getquestktrequired[questflag]"),
    ("isrefreshingquests", 0, 0, "isrefreshingquests[]"),
    // Salvage.
    ("ustadd", 1, 1, "ustadd[object]"),
    ("ustopen", 0, 0, "ustopen[]"),
    ("ustsalvage", 0, 0, "ustsalvage[]"),
    // The status window.
    ("statushud", 2, 2, "statushud[key,value]"),
    ("statushudcolored", 3, 3, "statushudcolored[key,value,rgb]"),
];

/// Registers every host function, and the code-string runner `exec`.
pub(crate) fn register(registry: &mut FunctionRegistry) {
    for &(name, minimum, maximum, signature) in HOST_FUNCTIONS {
        let function = name.to_owned();
        registry.register(
            name,
            minimum,
            maximum,
            move |context, args| context.call_host(&function, args),
            Some(signature),
            "",
        );
    }
    registry.register(
        "exec",
        1,
        1,
        |context, args| {
            let source = args[0].as_str("exec")?.to_owned();
            let program = context.compile(&source)?;
            Ok(context.run_separately(&program)?.0)
        },
        Some("exec[expression]"),
        "",
    );
    for (name, minimum, maximum, signature) in [
        ("delayexec", 2, 2, "delayexec[milliseconds,expression]"),
        ("clearexec", 1, 1, "clearexec[id]"),
    ] {
        let function = name.to_owned();
        registry.register(
            name,
            minimum,
            maximum,
            move |context, args| context.call_host(&function, args),
            Some(signature),
            "",
        );
    }
}

fn number(value: f64) -> ExpressionValue {
    ExpressionValue::Number(value)
}

fn boolean(value: bool) -> ExpressionValue {
    ExpressionValue::Boolean(value)
}

fn evaluation(reason: impl Into<String>) -> Fault {
    ExpressionError::evaluation(reason).into()
}

/// `checked((uint)value.AsNumber(operation))`.
fn to_uint(value: &ExpressionValue, operation: &str) -> Result<u32> {
    let number = value.as_number(operation)?;
    let whole = number.trunc();
    if number.is_nan() || whole < 0.0 || whole > f64::from(u32::MAX) {
        return Err(Fault::overflow());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(whole as u32)
}

/// `unchecked((int)value)`: the integer part, and int.MinValue for what
/// does not fit (the x64 conversion).
fn unchecked_int(value: f64) -> i32 {
    let whole = value.trunc();
    if value.is_nan() || whole < f64::from(i32::MIN) || whole > f64::from(i32::MAX) {
        i32::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let result = whole as i32;
        result
    }
}

fn get_int(values: &[(u32, i32)], key: u32) -> Option<i32> {
    values
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| *value)
}

fn get_int64(values: &[(u32, i64)], key: u32) -> Option<i64> {
    values
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| *value)
}

fn get_float(values: &[(u32, f64)], key: u32) -> Option<f64> {
    values
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| *value)
}

fn get_bool(values: &[(u32, bool)], key: u32) -> bool {
    values.iter().any(|(id, value)| *id == key && *value)
}

fn get_string(values: &[(u32, String)], key: u32) -> Option<String> {
    values
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| value.clone())
}

#[allow(clippy::cast_precision_loss)]
fn int64_number(value: i64) -> f64 {
    value as f64
}

fn new_list(context: &EvaluationContext<'_>, items: Vec<ExpressionValue>) -> ExpressionValue {
    ExpressionValue::List(ExpressionList::new(items, context.state.changes()))
}

fn new_dictionary(
    context: &EvaluationContext<'_>,
    entries: Vec<(&str, ExpressionValue)>,
) -> ExpressionValue {
    let dictionary = ExpressionDictionary::new(context.state.changes());
    dictionary.with_items(|items| {
        for (key, value) in entries {
            items.insert(key, value);
        }
    });
    ExpressionValue::Dictionary(dictionary)
}

fn number_list(context: &EvaluationContext<'_>, values: &[u32]) -> ExpressionValue {
    new_list(
        context,
        values
            .iter()
            .map(|&value| number(f64::from(value)))
            .collect(),
    )
}

fn coordinates(position: &Position) -> ExpressionValue {
    ExpressionValue::coordinates(ExpressionCoordinates::new(
        position.east_west,
        position.north_south,
        position.elevation,
    ))
}

fn normalize_heading(heading: f64) -> f64 {
    let result = heading % 360.0;
    if result < 0.0 { result + 360.0 } else { result }
}

fn heading_to(from: &Position, to: &Position) -> f64 {
    let east = to.east_west - from.east_west;
    let north = to.north_south - from.north_south;
    normalize_heading(east.atan2(north) * 180.0 / std::f64::consts::PI)
}

/// Straight-line distance in metres, elevation included (heights in
/// 240-metre units, like the map coordinates).
fn distance_meters(from: &Position, to: &Position) -> f64 {
    let flat = from.horizontal_distance_metres(to);
    let elevation = (from.elevation - to.elevation) * 240.0;
    (flat * flat + elevation * elevation).sqrt()
}

fn regex_fault(error: &mosstank_dotnet::compat::regex::RegexError) -> Fault {
    match error {
        mosstank_dotnet::compat::regex::RegexError::StepLimitExceeded => {
            Fault::runtime(error.to_string())
        }
        _ => Fault::argument(error.to_string()),
    }
}

fn create_regex(pattern: &str) -> Result<DotNetRegex> {
    DotNetRegex::new(
        pattern,
        RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
    )
    .map_err(|error| regex_fault(&error))
}

/// The regex flavour the name-matching finders use: case counts.
fn create_case_sensitive_regex(pattern: &str) -> Result<DotNetRegex> {
    DotNetRegex::new(pattern, RegexOptions::CULTURE_INVARIANT).map_err(|error| regex_fault(&error))
}

fn is_match(regex: &DotNetRegex, text: &str) -> Result<bool> {
    regex.is_match(text).map_err(|error| regex_fault(&error))
}

/// Which objects a finder searches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectSet {
    All,
    Inventory,
    Landscape,
}

fn in_set(object: &WorldObject, set: ObjectSet) -> bool {
    match set {
        ObjectSet::All => true,
        ObjectSet::Inventory => object.is_owned,
        ObjectSet::Landscape => object.is_landscape,
    }
}

fn class_number(class: ObjectClass) -> i32 {
    i32::from(class.tag())
}

const ITEM_SLOTS_KEY: u32 = 0x0D00_0004;
const NAME_STRING_KEY: u32 = 1;
const CLASS_ID_KEY: u32 = 0x0D00_0000;
const CONTAINER_KEY: u32 = 0x0D00_0002;
const LANDCELL_KEY: u32 = 0x0D00_0003;
const PACK_SLOTS_KEY: u32 = 0x0D00_0005;
const WIELDER_KEY: u32 = 0x0D00_000A;

/// One integer an object answers from its description: a signed 32-bit
/// number, as every integer property is.
fn object_description_int(object: &WorldObject, key: u32) -> Option<i32> {
    let value = match key {
        CLASS_ID_KEY => object.weenie_class_id,
        CONTAINER_KEY => {
            if object.container_id != 0 {
                object.container_id
            } else {
                object.wielder_id
            }
        }
        LANDCELL_KEY => object.position.as_ref()?.cell_id,
        ITEM_SLOTS_KEY => return Some(object.items_capacity),
        PACK_SLOTS_KEY => return Some(object.containers_capacity),
        WIELDER_KEY => object.wielder_id,
        _ => return None,
    };
    Some(value.cast_signed())
}

fn school_name(skill: u32) -> Result<&'static str> {
    Ok(match skill {
        34 => "War Magic",
        33 => "Life Magic",
        32 => "Item Enchantment",
        31 => "Creature Enchantment",
        43 => "Void Magic",
        _ => {
            return Err(Fault::runtime(
                "Object reference not set to an instance of an object.",
            ));
        }
    })
}

fn legacy_string_hash(value: &str) -> i32 {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut hash1: i32 = 5381;
    let mut hash2: i32 = hash1;
    let mut index = 0;
    while index < units.len() {
        hash1 = (hash1.wrapping_shl(5).wrapping_add(hash1)) ^ i32::from(units[index]);
        if index == units.len() - 1 {
            break;
        }
        hash2 = (hash2.wrapping_shl(5).wrapping_add(hash2)) ^ i32::from(units[index + 1]);
        index += 2;
    }
    hash1.wrapping_add(hash2.wrapping_mul(1_566_083_941))
}

fn strip_control_characters(text: &str) -> String {
    // char.IsControl: categories Cc (U+0000-U+001F, U+007F-U+009F).
    text.chars()
        .filter(|c| !matches!(u32::from(*c), 0x00..=0x1F | 0x7F..=0x9F))
        .collect()
}

impl Call<'_> {
    fn character(&mut self) -> Character {
        Character::of(self.host)
    }

    /// An object argument, read the way the reference reads it before the
    /// function runs: a number is an object id, and an id the client does
    /// not know is the reference's argument error.
    fn object_argument(
        &mut self,
        args: &[ExpressionValue],
        index: usize,
        parameters: &str,
    ) -> Result<u32> {
        let value = &args[index];
        let ExpressionValue::Number(raw) = value else {
            return value.as_object_id(parameters);
        };
        let id = signed_object_id(*raw)?;
        if self.object(id).is_some() {
            return Ok(id);
        }
        let count = parameters.chars().filter(|c| *c == ',').count() + 1;
        Err(EvaluationError {
            reason: format!(
                "{parameters} expects argument #{}/{count} to be a WorldObject but an invalid (number)id was passed instead. Passed value: {}",
                index + 1,
                value.to_display_string()
            ),
            is_argument_error: true,
            ..EvaluationError::default()
        }
        .into())
    }

    /// The object behind an id, for a function that reads the object
    /// itself: one the client has lost fails there.
    fn known_object(&mut self, id: u32, parameters: &str) -> Result<WorldObject> {
        self.object(id).ok_or_else(|| {
            evaluation(format!(
                "{parameters} failed: Object reference not set to an instance of an object."
            ))
        })
    }

    fn object_read(
        &mut self,
        args: &[ExpressionValue],
        index: usize,
        parameters: &str,
    ) -> Result<WorldObject> {
        let id = self.object_argument(args, index, parameters)?;
        self.known_object(id, parameters)
    }

    /// The name a profile means: the object's material in front of its
    /// bare name, unless the name already carries it. Both parts trimmed.
    fn display_name(&mut self, object: &WorldObject) -> String {
        const MATERIAL_TYPE_PROPERTY: u32 = 131;
        match self
            .host
            .int_property(object.object_id, MATERIAL_TYPE_PROPERTY)
            .and_then(materials::material_name)
        {
            Some(prefix) if ordinal::index_of(&object.name, prefix).is_none() => {
                format!("{prefix} {}", object.name.trim())
            }
            _ => object.name.trim().to_owned(),
        }
    }

    fn is_busy(&mut self) -> bool {
        self.host.item_state().is_busy || self.host.equipment_busy() || self.host.is_casting()
    }

    fn object_list(
        &mut self,
        context: &EvaluationContext<'_>,
        mut objects: Vec<WorldObject>,
    ) -> ExpressionValue {
        objects.sort_by_key(|object| object.object_id);
        let items = objects
            .iter()
            .map(|object| self.object_value(object.object_id))
            .collect();
        new_list(context, items)
    }

    fn set_objects(&mut self, set: ObjectSet) -> Vec<WorldObject> {
        self.objects()
            .iter()
            .filter(|object| in_set(object, set))
            .cloned()
            .collect()
    }

    /// The nearest matching object in `set` to the player, in three
    /// dimensions, never the player; an object without a position sorts
    /// last; ties break on the id.
    fn nearest(
        &mut self,
        set: ObjectSet,
        predicate: &mut dyn FnMut(&mut Self, &WorldObject) -> Result<bool>,
    ) -> Result<ExpressionValue> {
        let player = self.host.navigation();
        if !player.is_available {
            return Ok(ExpressionValue::zero());
        }
        let own = self.character().id;
        let mut best: Option<(f64, u32)> = None;
        for object in self.set_objects(set) {
            if object.object_id == own || !predicate(self, &object)? {
                continue;
            }
            let distance = object.position.as_ref().map_or(f64::MAX, |position| {
                distance_meters(&player.position, position)
            });
            let candidate = (distance, object.object_id);
            if best.is_none_or(|current| {
                candidate.0 < current.0 || (candidate.0 == current.0 && candidate.1 < current.1)
            }) {
                best = Some(candidate);
            }
        }
        Ok(best.map_or_else(ExpressionValue::zero, |(_, id)| self.object_value(id)))
    }

    fn first_object(
        &mut self,
        set: ObjectSet,
        predicate: &mut dyn FnMut(&mut Self, &WorldObject) -> Result<bool>,
    ) -> Result<ExpressionValue> {
        let mut found: Option<u32> = None;
        for object in self.set_objects(set) {
            if predicate(self, &object)? && found.is_none_or(|id| object.object_id < id) {
                found = Some(object.object_id);
            }
        }
        Ok(found.map_or_else(ExpressionValue::zero, |id| self.object_value(id)))
    }

    fn player_property_numbers(
        &mut self,
        key: &ExpressionValue,
        read: PropertyRead,
    ) -> Result<f64> {
        let character = self.character();
        let Some(properties) = self.host.properties(character.id) else {
            return Ok(0.0);
        };
        let key = to_uint(key, "character property")?;
        Ok(match read {
            PropertyRead::Int => get_int(&properties.ints, key).map_or(0.0, f64::from),
            PropertyRead::Int64 => get_int64(&properties.int64s, key).map_or(0.0, int64_number),
            PropertyRead::Double => get_float(&properties.floats, key).unwrap_or(0.0),
            PropertyRead::Bool => {
                if get_bool(&properties.bools, key) {
                    1.0
                } else {
                    0.0
                }
            }
        })
    }

    fn vital(&mut self, id: &ExpressionValue, read: VitalRead) -> Result<f64> {
        let character = self.character();
        let (current, maximum, base) = match id.as_int32("character vital")? {
            1 => character.health,
            2 => character.stamina,
            3 => character.mana,
            _ => (0, 0, 0),
        };
        let value = f64::from(match read {
            VitalRead::Current => current,
            VitalRead::Base => base,
            VitalRead::Maximum => maximum,
        });
        Ok(if value < 1.0 { 1.0 } else { value })
    }

    fn object_vital(
        &mut self,
        args: &[ExpressionValue],
        parameters: &str,
        read: VitalObjectRead,
    ) -> Result<f64> {
        let id = self.object_read(args, 0, parameters)?.object_id;
        let character = self.character();
        if id == character.id {
            return Ok(match read {
                VitalObjectRead::Fraction => {
                    if character.health.1 == 0 {
                        -1.0
                    } else {
                        f64::from(character.health.0) / f64::from(character.health.1)
                    }
                }
                VitalObjectRead::Health => f64::from(character.health.0),
                VitalObjectRead::Stamina => f64::from(character.stamina.0),
                VitalObjectRead::Mana => f64::from(character.mana.0),
            });
        }
        if matches!(read, VitalObjectRead::Fraction | VitalObjectRead::Health) {
            for target in self.host.hostile_targets(f32::MAX) {
                if target.object_id != id || !target.is_health_known {
                    continue;
                }
                return Ok(if read == VitalObjectRead::Fraction {
                    f64::from(target.health_fraction)
                } else if target.maximum_health > 0 {
                    f64::from(target.health_fraction) * f64::from(target.maximum_health)
                } else {
                    -1.0
                });
            }
        }
        Ok(-1.0)
    }

    fn free_slots(&mut self, args: &[ExpressionValue], containers: bool) -> Result<f64> {
        let parameters = if containers {
            "getfreecontainerslots[WorldObject]"
        } else {
            "getfreeitemslots[WorldObject]"
        };
        let own = self.character().id;
        let container_id = if args.is_empty() {
            own
        } else {
            self.object_argument(args, 0, parameters)?
        };
        // The item-slot count reads any container but the character's own,
        // so a lost one fails; the container-slot count reads the id only.
        if !containers && container_id != own {
            self.known_object(container_id, parameters)?;
        }
        let Some(container) = self.object(container_id).filter(|object| {
            matches!(
                object.object_class,
                ObjectClass::Container | ObjectClass::Player
            )
        }) else {
            return Ok(-1.0);
        };
        let used = self
            .objects()
            .iter()
            .filter(|item| {
                item.container_id == container_id
                    && (item.object_class == ObjectClass::Container) == containers
            })
            .count();
        let capacity = if containers {
            container.containers_capacity
        } else {
            container.items_capacity
        };
        let used = i32::try_from(used).unwrap_or(i32::MAX);
        Ok(f64::from(capacity.saturating_sub(used).max(0)))
    }

    /// Whether the character's skill in the spell's school reaches its
    /// difficulty plus the margin; a spell the catalogue does not describe,
    /// or one with no school, fails.
    fn has_skill_for(&mut self, spell_id: u32, hunting: bool) -> bool {
        let Some(spell) = self.host.spell(spell_id) else {
            return false;
        };
        if spell.school == 0 {
            return false;
        }
        let skill = self
            .host
            .skill(spell.school)
            .map_or(0, |info| i64::from(info.current));
        skill >= i64::from(spell.difficulty) + i64::from(self.services.skill_margin(hunting))
    }

    fn can_cast_now(&mut self, spell_id: u32, hunting: bool) -> bool {
        self.host.is_spell_known(spell_id)
            && self.host.has_components(spell_id)
            && self.has_skill_for(spell_id, hunting)
    }

    /// One step towards being able to cast: wield a wand, then take the
    /// magic stance. True only when both are done; a busy character takes
    /// no step at all.
    fn magic_mode_step(&mut self) -> bool {
        if self.is_busy() {
            return false;
        }
        let Some(wand) = self.host.owned_equipment().into_iter().find(|item| {
            item.valid_locations & 0x0100_0000 != 0 || item.equipped_location & 0x0100_0000 != 0
        }) else {
            return false;
        };
        if wand.equipped_location == 0 {
            self.host.equip(wand.object_id, 0);
            return false;
        }
        if self.host.combat().mode != CombatMode::Magic {
            self.host.enter_combat_mode(CombatMode::Magic);
            return false;
        }
        true
    }

    fn cast_result(&mut self, spell_id: u32, target: Option<u32>) -> Result<ExpressionValue> {
        const IMPOSSIBLE: f64 = 2.0;
        const NOT_ATTEMPTED: f64 = 0.0;
        const BEGUN: f64 = 1.0;
        if target.is_none() && !self.host.is_spell_known(spell_id) {
            return Ok(number(IMPOSSIBLE));
        }
        if !self.has_skill_for(spell_id, true) {
            return Ok(number(IMPOSSIBLE));
        }
        if !self.magic_mode_step() {
            return Ok(number(NOT_ATTEMPTED));
        }
        // The target is read only now, so a lost target fails here.
        if let Some(target_id) = target {
            self.known_object(target_id, "actiontrycastbyidontarget[number, WorldObject]")?;
        }
        if !self.services.begin_cast(self.host, spell_id, target) {
            self.host.cast(spell_id, target.unwrap_or(0));
        }
        Ok(number(BEGUN))
    }

    fn object_pair(&mut self, args: &[ExpressionValue], parameters: &str) -> Result<(u32, u32)> {
        let first = self.object_argument(args, 0, parameters)?;
        let second = self.object_argument(args, 1, parameters)?;
        if !self.is_busy() {
            self.known_object(first, parameters)?;
            self.known_object(second, parameters)?;
        }
        Ok((first, second))
    }

    fn object_property(
        &mut self,
        name: &str,
        args: &[ExpressionValue],
        read: PropertyRead2,
    ) -> Result<ExpressionValue> {
        let parameters = format!("{name}[WorldObject, number]");
        let object_id = self.object_argument(args, 0, &parameters)?;
        let key = to_uint(&args[1], name)?;
        let object = self.known_object(object_id, &parameters)?;
        let properties = self.host.properties(object_id);
        // An appraised value wins; otherwise what the client learned when
        // the object appeared; otherwise the kind's default.
        Ok(match read {
            PropertyRead2::Int => number(
                properties
                    .as_ref()
                    .and_then(|properties| get_int(&properties.ints, key))
                    .or_else(|| object_description_int(&object, key))
                    .map_or(0.0, f64::from),
            ),
            PropertyRead2::Double => number(
                properties
                    .as_ref()
                    .and_then(|properties| get_float(&properties.floats, key))
                    .unwrap_or(0.0),
            ),
            // The reference answers a bool property as the number 1 or 0.
            PropertyRead2::Bool => number(
                if properties
                    .as_ref()
                    .is_some_and(|properties| get_bool(&properties.bools, key))
                {
                    1.0
                } else {
                    0.0
                },
            ),
            PropertyRead2::String => ExpressionValue::string(&match properties
                .as_ref()
                .and_then(|properties| get_string(&properties.strings, key))
            {
                Some(text) => text,
                None if key == NAME_STRING_KEY => object.name.clone(),
                None => String::new(),
            }),
        })
    }

    fn spell_expiration(&mut self, spell_id: u32) -> f64 {
        self.host
            .enchantments()
            .iter()
            .find(|enchantment| enchantment.spell_id == spell_id)
            .map_or(0.0, |enchantment| enchantment.seconds_remaining)
    }

    fn view_holds(&self, view: &str) -> bool {
        self.views
            .as_deref()
            .is_some_and(|views| views.holds_controls(view))
    }
}

#[derive(Clone, Copy)]
enum PropertyRead {
    Int,
    Int64,
    Double,
    Bool,
}

#[derive(Clone, Copy)]
enum PropertyRead2 {
    Int,
    Double,
    Bool,
    String,
}

#[derive(Clone, Copy)]
enum VitalRead {
    Current,
    Base,
    Maximum,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VitalObjectRead {
    Fraction,
    Health,
    Stamina,
    Mana,
}

const ACTION_TRY_MOVE_PARAMETERS: &str = "actiontrymove[WorldObject, WorldObject, number, number]";

/// Runs the host function `name`; `None` for a name that is not one.
#[allow(clippy::too_many_lines)]
pub(crate) fn call(
    call: &mut Call<'_>,
    name: &str,
    context: &mut EvaluationContext<'_>,
    args: &[ExpressionValue],
) -> Option<Result<ExpressionValue>> {
    Some(match name {
        // ---- the character ----
        "getworldname" => Ok(ExpressionValue::string(&call.character().world)),
        "getaccounthash" => Ok(ExpressionValue::string(&format!(
            "{:X}",
            legacy_string_hash(&call.character().account)
        ))),
        "getcharacterindex" => (|| {
            let character = call.character();
            let given = args[0].as_str("getcharacterindex")?;
            // An empty name is the character's own, as in the reference.
            let name = if given.is_empty() {
                character.name.clone()
            } else {
                given.to_owned()
            };
            let roster = login_roster::sorted(call.host.login().roster);
            // Before the account's list has arrived there is no order to
            // place anyone in, so the client's own answer stands.
            Ok(number(f64::from(if roster.is_empty() {
                character.index
            } else {
                login_roster::index_of_name(&roster, &name)
            })))
        })(),
        "getplayercoordinates" => {
            let snapshot = call.host.navigation();
            Ok(if snapshot.is_available {
                coordinates(&snapshot.position)
            } else {
                ExpressionValue::zero()
            })
        }
        // The landblock keeps its high word in place (0xAABB0000).
        "getplayerlandblock" => Ok(number(f64::from(
            call.host.navigation().position.cell_id & 0xFFFF_0000,
        ))),
        "getplayerlandcell" => Ok(number(f64::from(call.host.navigation().position.cell_id))),
        "isportaling" => {
            let in_world = call.character().in_world;
            Ok(boolean(!in_world || call.host.navigation().is_portal_space))
        }
        "getcharburden" => {
            let id = call.character().id;
            Ok(match call.host.properties(id) {
                None => ExpressionValue::zero(),
                Some(properties) => {
                    let burden = get_int(&properties.ints, 5).map_or(0.0, f64::from);
                    let capacity = get_int(&properties.ints, 96).map_or(0.0, f64::from);
                    number(if capacity > 0.0 {
                        (burden * 100.0 / capacity).round_ties_even()
                    } else {
                        0.0
                    })
                }
            })
        }
        "getcharintprop" => call
            .player_property_numbers(&args[0], PropertyRead::Int)
            .map(number),
        "getchardoubleprop" => call
            .player_property_numbers(&args[0], PropertyRead::Double)
            .map(number),
        "getcharquadprop" => call
            .player_property_numbers(&args[0], PropertyRead::Int64)
            .map(number),
        "getcharboolprop" => call
            .player_property_numbers(&args[0], PropertyRead::Bool)
            .map(|value| boolean(value != 0.0)),
        "getcharstringprop" => (|| {
            let id = call.character().id;
            let Some(properties) = call.host.properties(id) else {
                return Ok(ExpressionValue::zero());
            };
            let key = to_uint(&args[0], "getcharstringprop")?;
            Ok(get_string(&properties.strings, key)
                .map_or_else(ExpressionValue::zero, |text| ExpressionValue::string(&text)))
        })(),
        "getcharattribute_base" | "getcharattribute_buffed" => (|| {
            let kind = args[0].as_int32("character attribute")?.wrapping_sub(1);
            let buffed = name == "getcharattribute_buffed";
            Ok(number(
                call.host
                    .attributes()
                    .iter()
                    .find(|attribute| attribute.kind == kind)
                    .map_or(0.0, |attribute| {
                        f64::from(if buffed {
                            attribute.current
                        } else {
                            attribute.base
                        })
                    }),
            ))
        })(),
        "getcharskill_base" | "getcharskill_buffed" | "getcharskill_traininglevel" => (|| {
            let id = to_uint(&args[0], "character skill")?;
            let Some(skill) = call.host.skill(id) else {
                return Ok(ExpressionValue::zero());
            };
            Ok(number(match name {
                "getcharskill_base" => f64::from(skill.base),
                "getcharskill_buffed" => f64::from(skill.current),
                _ => match skill.training {
                    ac_plugin_abi::character::Training::Untrained => 1.0,
                    ac_plugin_abi::character::Training::Trained => 2.0,
                    ac_plugin_abi::character::Training::Specialized => 3.0,
                    ac_plugin_abi::character::Training::Unknown => 0.0,
                },
            }))
        })(),
        "getcharvital_base" => call.vital(&args[0], VitalRead::Base).map(number),
        "getcharvital_buffedmax" => call.vital(&args[0], VitalRead::Maximum).map(number),
        "getcharvital_current" => call.vital(&args[0], VitalRead::Current).map(number),
        "vitae" => {
            let id = call.character().id;
            Ok(number(match call.host.properties(id) {
                None => 100.0,
                // PropertyFloat.Vitae (129) is the multiplier 0..1.
                Some(properties) => get_float(&properties.floats, 129).unwrap_or(1.0) * 100.0,
            }))
        }

        // ---- spells ----
        "componentname" => (|| {
            let id = to_uint(&args[0], "componentname")?;
            Ok(call
                .host
                .spell_component(id)
                .map_or_else(ExpressionValue::zero, |component| {
                    ExpressionValue::string(&component.name)
                }))
        })(),
        "componentdata" => (|| {
            let id = to_uint(&args[0], "componentdata")?;
            let Some(component) = call.host.spell_component(id) else {
                return Ok(new_dictionary(context, Vec::new()));
            };
            Ok(new_dictionary(
                context,
                vec![
                    ("BurnRate", number(component.burn_rate)),
                    ("GestureId", number(f64::from(component.gesture_id))),
                    ("GestureSpeed", number(component.gesture_speed)),
                    ("IconId", number(f64::from(component.icon_id))),
                    ("Id", number(f64::from(component.component_id))),
                    ("Name", ExpressionValue::string(&component.name)),
                    ("SortKey", number(f64::from(component.sort_key))),
                    ("Type", ExpressionValue::string(&component.component_type)),
                    ("Word", ExpressionValue::string(&component.word)),
                ],
            ))
        })(),
        "getisspellknown" => {
            to_uint(&args[0], "getisspellknown").map(|id| boolean(call.host.is_spell_known(id)))
        }
        "getknownspells" => {
            let mut ids: Vec<u32> = call
                .host
                .known_spells(SpellList::SelfBuffs)
                .into_iter()
                .chain(call.host.known_spells(SpellList::Combat))
                .map(|spell| spell.spell_id)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            Ok(number_list(context, &ids))
        }
        "spellname" => to_uint(&args[0], "spellname").map(|id| {
            call.host
                .spell(id)
                .map_or_else(ExpressionValue::zero, |spell| {
                    ExpressionValue::string(&spell.name)
                })
        }),
        // The reference reads the id as a whole number and answers an empty
        // dictionary for a spell it does not have.
        "spelldata" => (|| {
            let id = unchecked_int(args[0].as_number("spelldata")?).cast_unsigned();
            let Some(spell) = call.host.spell(id) else {
                return Ok(new_dictionary(context, Vec::new()));
            };
            let extras = call.host.spell_extras(id);
            spell_data(context, &spell, &extras)
        })(),
        "getspellexpiration" => (|| {
            let id = to_uint(&args[0], "getspellexpiration")?;
            Ok(number(call.spell_expiration(id)))
        })(),
        "getspellexpirationbyname" => (|| {
            let wanted = args[0].as_str("getspellexpirationbyname")?.to_owned();
            let found = call
                .host
                .known_spells(SpellList::SelfBuffs)
                .into_iter()
                .chain(call.host.known_spells(SpellList::Combat))
                .find(|spell| ordinal::equals_ignore_case(&spell.name, &wanted));
            Ok(number(match found {
                Some(spell) if spell.spell_id > 0 => call.spell_expiration(spell.spell_id),
                _ => 0.0,
            }))
        })(),
        "getcooldownexpiration" => to_uint(&args[0], "getcooldownexpiration")
            .map(|id| number(call.host.cooldown_remaining(id))),
        // Two questions, two answers: the buffing and hunting margins over
        // a spell's difficulty are separate profile settings.
        "getcancastspell_buff" => to_uint(&args[0], "getcancastspell_buff")
            .map(|id| boolean(call.can_cast_now(id, false))),
        "getcancastspell_hunt" => {
            to_uint(&args[0], "getcancastspell_hunt").map(|id| boolean(call.can_cast_now(id, true)))
        }

        // ---- objects ----
        "wobjectfindbyid" => (|| {
            let id = signed_object_id(args[0].as_number("wobjectfindbyid")?)?;
            Ok(if call.object(id).is_some() {
                call.object_value(id)
            } else {
                ExpressionValue::zero()
            })
        })(),
        "wobjectgetplayer" => {
            let id = call.character().id;
            Ok(call.object_value(id))
        }
        "wobjectgetselection" => Ok(match call.host.selection().selected {
            Some(id) => call.object_value(id),
            None => ExpressionValue::zero(),
        }),
        "wobjectgetopencontainer" => {
            let id = call.host.open_container();
            Ok(if id != 0 {
                call.object_value(id)
            } else {
                ExpressionValue::zero()
            })
        }
        // Ids come out signed, the form profiles store and hand back.
        "wobjectgetid" => call
            .object_read(args, 0, "wobjectgetid[WorldObject]")
            .map(|object| ExpressionValue::object_id_number(object.object_id)),
        // The name a profile reads off an object is its DISPLAY name.
        "wobjectgetname" => call
            .object_read(args, 0, "wobjectgetname[WorldObject]")
            .map(|object| ExpressionValue::string(&call.display_name(&object))),
        "wobjectgetobjectclass" => call
            .object_read(args, 0, "wobjectgetobjectclass[WorldObject]")
            .map(|object| number(f64::from(class_number(object.object_class)))),
        "wobjectgettemplatetype" => call
            .object_read(args, 0, "wobjectgettemplatetype[WorldObject]")
            .map(|object| number(f64::from(object.weenie_class_id))),
        "wobjectgetinternaltype" => (|| {
            let id = args[0].as_object_id("wobjectgetinternaltype")?;
            Ok(call
                .object(id)
                .map_or_else(ExpressionValue::zero, |object| {
                    number(f64::from(object.item_type))
                }))
        })(),
        "wobjecthasdata" => call
            .object_read(args, 0, "wobjecthasdata[WorldObject]")
            .map(|object| boolean(object.has_appraisal_data)),
        "wobjectlastidtime" => call
            .object_read(args, 0, "wobjectlastidtime[WorldObject]")
            .map(|object| number(f64::from(object.last_id_time))),
        // A lost object is simply not valid; an unknown id is still an error.
        "wobjectisvalid" => (|| {
            let id = call.object_argument(args, 0, "wobjectisvalid[WorldObject]")?;
            Ok(boolean(
                call.object(id)
                    .is_some_and(|object| object.position.is_some()),
            ))
        })(),
        "wobjectrequestdata" => (|| {
            let id = call.object_argument(args, 0, "wobjectrequestdata[WorldObject]")?;
            Ok(boolean(call.host.identify(id).accepted()))
        })(),
        "wobjectgetisdooropen" => call
            .object_read(args, 0, "wobjectgetisdooropen[WorldObject]")
            .map(|object| boolean(object.is_door_open)),
        "wobjectgetphysicscoordinates" => call
            .object_read(args, 0, "wobjectgetphysicscoordinates[WorldObject]")
            .map(|object| {
                object
                    .position
                    .as_ref()
                    .map_or_else(ExpressionValue::zero, coordinates)
            }),
        "getheading" => call
            .object_read(args, 0, "getheading[WorldObject]")
            .map(|object| {
                object
                    .position
                    .as_ref()
                    .map_or_else(ExpressionValue::zero, |position| {
                        number(normalize_heading(f64::from(position.heading_degrees)))
                    })
            }),
        "getheadingto" => (|| {
            let object = call.object_read(args, 0, "getheadingto[WorldObject]")?;
            let player = call.host.navigation();
            Ok(match (&object.position, player.is_available) {
                (Some(position), true) => number(heading_to(&player.position, position)),
                _ => ExpressionValue::zero(),
            })
        })(),
        "wobjectgetintprop" => call.object_property(name, args, PropertyRead2::Int),
        "wobjectgetdoubleprop" => call.object_property(name, args, PropertyRead2::Double),
        "wobjectgetboolprop" => call.object_property(name, args, PropertyRead2::Bool),
        "wobjectgetstringprop" => call.object_property(name, args, PropertyRead2::String),
        "wobjectgetspellids" => call
            .object_read(args, 0, "wobjectgetspellids[WorldObject]")
            .map(|object| number_list(context, &object.spell_ids)),
        "wobjectgetactivespellids" => call
            .object_read(args, 0, "wobjectgetactivespellids[WorldObject]")
            .map(|object| number_list(context, &object.active_spell_ids)),
        "wobjectfindall" => {
            let objects = call.set_objects(ObjectSet::All);
            Ok(call.object_list(context, objects))
        }
        "wobjectfindallbyobjectclass"
        | "wobjectfindallinventorybyobjectclass"
        | "wobjectfindalllandscapebyobjectclass" => (|| {
            let class = args[0].as_int32("")?;
            let objects: Vec<WorldObject> = call
                .set_objects(finder_set(name))
                .into_iter()
                .filter(|object| class_number(object.object_class) == class)
                .collect();
            Ok(call.object_list(context, objects))
        })(),
        "wobjectfindallbytemplatetype"
        | "wobjectfindallinventorybytemplatetype"
        | "wobjectfindalllandscapebytemplatetype" => (|| {
            let mut objects = Vec::new();
            for object in call.set_objects(finder_set(name)) {
                if object.weenie_class_id == to_uint(&args[0], "template type")? {
                    objects.push(object);
                }
            }
            Ok(call.object_list(context, objects))
        })(),
        // A case-sensitive regex over the DISPLAY name.
        "wobjectfindallbynamerx"
        | "wobjectfindallinventorybynamerx"
        | "wobjectfindalllandscapebynamerx" => (|| {
            let regex = create_case_sensitive_regex(args[0].as_str(name)?)?;
            let mut objects = Vec::new();
            for object in call.set_objects(finder_set(name)) {
                let display = call.display_name(&object);
                if is_match(&regex, &display)? {
                    objects.push(object);
                }
            }
            Ok(call.object_list(context, objects))
        })(),
        "wobjectfindallinventory" => {
            let objects = call.set_objects(ObjectSet::Inventory);
            Ok(call.object_list(context, objects))
        }
        "wobjectfindalllandscape" => {
            let objects = call.set_objects(ObjectSet::Landscape);
            Ok(call.object_list(context, objects))
        }
        "wobjectfindallbycontainer" => (|| {
            let container = args[0].as_object_id("wobjectfindallbycontainer")?;
            let objects: Vec<WorldObject> = call
                .set_objects(ObjectSet::All)
                .into_iter()
                .filter(|object| object.container_id == container)
                .collect();
            Ok(call.object_list(context, objects))
        })(),
        // Exact match on the DISPLAY name, case included.
        "wobjectfindininventorybyname" => (|| {
            let wanted = args[0].as_str("wobjectfindininventorybyname")?.to_owned();
            call.first_object(ObjectSet::Inventory, &mut |call, object| {
                Ok(call.display_name(object) == wanted)
            })
        })(),
        "wobjectfindininventorybynamerx" => (|| {
            let regex =
                create_case_sensitive_regex(args[0].as_str("wobjectfindininventorybynamerx")?)?;
            call.first_object(ObjectSet::Inventory, &mut |call, object| {
                let display = call.display_name(object);
                is_match(&regex, &display)
            })
        })(),
        "wobjectfindininventorybytemplatetype" => call
            .first_object(ObjectSet::Inventory, &mut |_, object| {
                Ok(object.weenie_class_id == to_uint(&args[0], "template type")?)
            }),
        // The class finders search every object the client knows; the
        // template-type finder searches only the ground.
        "wobjectfindnearestbyobjectclass" => call.nearest(ObjectSet::All, &mut |_, object| {
            Ok(class_number(object.object_class) == args[0].as_int32("")?)
        }),
        "wobjectfindnearestbytemplatetype" => call
            .nearest(ObjectSet::Landscape, &mut |_, object| {
                Ok(object.weenie_class_id == to_uint(&args[0], "template type")?)
            }),
        // Object class first, then a case-sensitive regex over the BARE name.
        "wobjectfindnearestbynameandobjectclass" => (|| {
            let class = args[0].as_int32("wobjectfindnearestbynameandobjectclass")?;
            let regex = create_case_sensitive_regex(
                args[1].as_str("wobjectfindnearestbynameandobjectclass")?,
            )?;
            call.nearest(ObjectSet::All, &mut |_, object| {
                Ok(class_number(object.object_class) == class && is_match(&regex, &object.name)?)
            })
        })(),
        "wobjectfindnearestdoor" => call.nearest(ObjectSet::All, &mut |_, object| {
            Ok(object.object_class == ObjectClass::Door)
        }),
        // Every monster, blacklisted or not.
        "wobjectfindnearestmonster" => call.nearest(ObjectSet::All, &mut |_, object| {
            Ok(object.object_class == ObjectClass::Monster)
        }),
        // Both counts read the DISPLAY name and ignore case.
        "getitemcountininventorybyname" => (|| {
            let wanted = args[0].as_str("getitemcountininventorybyname")?.to_owned();
            let mut total = 0_i64;
            for object in call.objects().iter() {
                if object.is_owned
                    && ordinal::equals_ignore_case(&call.display_name(object), &wanted)
                {
                    total += i64::from(object.stack_size.max(1));
                }
            }
            Ok(number(int64_number(total)))
        })(),
        "getitemcountininventorybynamerx" => (|| {
            let regex = create_regex(args[0].as_str("getitemcountininventorybynamerx")?)?;
            let mut total = 0_i64;
            for object in call.objects().iter() {
                if object.is_owned {
                    let display = call.display_name(object);
                    if is_match(&regex, &display)? {
                        total += i64::from(object.stack_size.max(1));
                    }
                }
            }
            Ok(number(int64_number(total)))
        })(),
        "getinventorycountbytemplatetype" => (|| {
            let template = to_uint(&args[0], "getinventorycountbytemplatetype")?;
            let total: i64 = call
                .objects()
                .iter()
                .filter(|object| object.is_owned && object.weenie_class_id == template)
                .map(|object| i64::from(object.stack_size.max(1)))
                .sum();
            Ok(number(int64_number(total)))
        })(),
        "getcontaineritemcount" => (|| {
            const PARAMETERS: &str = "getcontaineritemcount[WorldObject]";
            let own = call.character().id;
            let container = if args.is_empty() {
                own
            } else {
                call.object_argument(args, 0, PARAMETERS)?
            };
            // Any container but the character's own is read, so a lost one
            // fails.
            if container != own {
                call.known_object(container, PARAMETERS)?;
            }
            if !call.object(container).is_some_and(|object| {
                matches!(
                    object.object_class,
                    ObjectClass::Container | ObjectClass::Player
                )
            }) {
                return Ok(number(-1.0));
            }
            let count = call
                .objects()
                .iter()
                .filter(|item| item.container_id == container)
                .count();
            #[allow(clippy::cast_precision_loss)]
            Ok(number(count as f64))
        })(),
        "getfreeitemslots" => call.free_slots(args, false).map(number),
        "getfreecontainerslots" => call.free_slots(args, true).map(number),
        "wobjectgethealth" => call
            .object_vital(
                args,
                "wobjectgethealth[WorldObject]",
                VitalObjectRead::Fraction,
            )
            .map(number),
        "wobjectgethealthvalue" => call
            .object_vital(
                args,
                "wobjectgethealthvalue[WorldObject]",
                VitalObjectRead::Health,
            )
            .map(number),
        "wobjectgetstaminavalue" => call
            .object_vital(
                args,
                "wobjectgetstaminavalue[WorldObject]",
                VitalObjectRead::Stamina,
            )
            .map(number),
        "wobjectgetmanavalue" => call
            .object_vital(
                args,
                "wobjectgetmanavalue[WorldObject]",
                VitalObjectRead::Mana,
            )
            .map(number),

        // ---- loot ----
        "hascorpsebeenopenedbyme" => (|| {
            let id = call.object_argument(args, 0, "hascorpsebeenopenedbyme[WorldObject]")?;
            Ok(boolean(call.host.corpses(f32::MAX).iter().any(|corpse| {
                corpse.object_id == id && corpse.has_been_opened
            })))
        })(),
        // Corpse ids are listed signed, as wobjectgetid hands them out.
        "getcorpsesunopenedbyme" => {
            let items = call
                .host
                .corpses(f32::MAX)
                .iter()
                .filter(|corpse| !corpse.has_been_opened)
                .map(|corpse| ExpressionValue::object_id_number(corpse.object_id))
                .collect();
            Ok(new_list(context, items))
        }

        // ---- fellowship ----
        "getfellowshipstatus" => Ok(boolean(call.host.fellowship().is_in_fellowship)),
        "getfellowshipname" => Ok(ExpressionValue::string(&call.host.fellowship().name)),
        "getfellowshipcount" => Ok(number(f64::from(call.host.fellowship().member_count))),
        "getfellowshipleaderid" => Ok(ExpressionValue::object_id_number(
            call.host.fellowship().leader_id,
        )),
        "getfellowid" | "getfellowname" => (|| {
            let roster = call.host.fellowship_roster();
            let index = args[0].as_int32(name)?;
            let member = usize::try_from(index)
                .ok()
                .and_then(|index| roster.get(index));
            Ok(match (member, name) {
                (Some(member), "getfellowid") => {
                    ExpressionValue::object_id_number(member.object_id)
                }
                (Some(member), _) => ExpressionValue::string(&member.name),
                (None, "getfellowid") => ExpressionValue::zero(),
                (None, _) => ExpressionValue::string(""),
            })
        })(),
        "getfellowshiplocked" => Ok(boolean(call.host.fellowship().is_locked)),
        "getfellowshipisleader" => {
            let fellowship = call.host.fellowship();
            let own = call.character().id;
            Ok(boolean(
                fellowship.is_in_fellowship && fellowship.leader_id == own,
            ))
        }
        "getfellowshipisopen" => Ok(boolean(call.host.fellowship().is_open)),
        "getfellowshipisfull" => {
            let fellowship = call.host.fellowship();
            Ok(boolean(
                fellowship.is_in_fellowship && fellowship.member_count == 9,
            ))
        }
        "getfellowshipcanrecruit" => {
            let fellowship = call.host.fellowship();
            let own = call.character().id;
            Ok(boolean(
                fellowship.is_in_fellowship
                    && (fellowship.is_open || fellowship.leader_id == own)
                    && fellowship.member_count < 9,
            ))
        }
        "getfellownames" => {
            let items = call
                .host
                .fellowship_roster()
                .iter()
                .map(|member| ExpressionValue::string(&member.name))
                .collect();
            Ok(new_list(context, items))
        }
        "getfellowids" => {
            let items = call
                .host
                .fellowship_roster()
                .iter()
                .map(|member| ExpressionValue::object_id_number(member.object_id))
                .collect();
            Ok(new_list(context, items))
        }

        // ---- world time ----
        "getgameyear" => Ok(number(f64::from(call.host.world_time().year))),
        "getgamemonth" => Ok(number(f64::from(call.host.world_time().month))),
        "getgamemonthname" => args[0].as_int32("getgamemonthname").map(|index| {
            ExpressionValue::string(
                usize::try_from(index)
                    .ok()
                    .and_then(|index| GAME_MONTH_NAMES.get(index))
                    .copied()
                    .unwrap_or_default(),
            )
        }),
        "getgameday" => Ok(number(f64::from(call.host.world_time().day))),
        "getgamehour" => Ok(number(f64::from(call.host.world_time().hour))),
        "getgamehourname" => args[0].as_int32("getgamehourname").map(|index| {
            ExpressionValue::string(
                usize::try_from(index)
                    .ok()
                    .and_then(|index| GAME_HOUR_NAMES.get(index))
                    .copied()
                    .unwrap_or_default(),
            )
        }),
        "getminutesuntilday" => Ok(number(call.host.world_time().minutes_until_day)),
        "getminutesuntilnight" => Ok(number(call.host.world_time().minutes_until_night)),
        "getgameticks" => Ok(number(call.host.world_time().game_ticks)),
        "getisday" => Ok(boolean(call.host.world_time().is_day)),
        "getisnight" => Ok(boolean(!call.host.world_time().is_day)),

        // ---- windows: a view a meta built answers for itself, drawn or
        // not; any other name is one of the client's own windows ----
        "uigetcontrol" => (|| {
            let view = args[0].as_str("uigetcontrol")?.to_owned();
            let control = args[1].as_str("uigetcontrol")?.to_owned();
            let exists = if call.view_holds(&view) {
                call.views
                    .as_deref()
                    .is_some_and(|views| views.control_exists(&view, &control))
            } else {
                call.host.ui_control_exists(&view, &control)
            };
            Ok(if exists {
                ExpressionValue::ui_control(&view, &control)
            } else {
                ExpressionValue::zero()
            })
        })(),
        // A control that cannot take a label is an error; a meta view's
        // control that has gone with its view answers 0.
        "uisetlabel" => (|| {
            let control = args[0].as_ui_control("uisetlabel")?.clone();
            let label = args[1].as_str("uisetlabel")?.to_owned();
            if call.view_holds(&control.view) {
                let result = call
                    .views
                    .as_deref_mut()
                    .map_or(MetaViewLabelResult::NotFound, |views| {
                        views.set_control_label(&control.view, &control.control, &label)
                    });
                return match result {
                    MetaViewLabelResult::Set => Ok(ExpressionValue::one()),
                    MetaViewLabelResult::NotFound => Ok(ExpressionValue::zero()),
                    MetaViewLabelResult::TakesNoLabel => {
                        Err(evaluation("uisetlabel: that control type takes no label"))
                    }
                };
            }
            if !call
                .host
                .ui_set_control_label(&control.view, &control.control, &label)
            {
                return Err(evaluation("uisetlabel: that control type takes no label"));
            }
            Ok(ExpressionValue::one())
        })(),
        // Any non-zero number means visible; the second argument is handed
        // back unchanged.
        "uisetvisible" => (|| {
            let control = args[0].as_ui_control("uisetvisible")?.clone();
            let visible = args[1].as_number("uisetvisible")? != 0.0;
            if call.view_holds(&control.view) {
                if let Some(views) = call.views.as_deref_mut() {
                    views.set_control_visible(&control.view, &control.control, visible);
                }
            } else {
                call.host
                    .ui_set_control_visible(&control.view, &control.control, visible);
            }
            Ok(args[1].clone())
        })(),
        "uiviewexists" => (|| {
            let view = args[0].as_str("uiviewexists")?.to_owned();
            let held = call
                .views
                .as_deref()
                .is_some_and(|views| views.view_exists(&view));
            Ok(boolean(held || call.host.ui_view_exists(&view)))
        })(),
        "uiviewvisible" => (|| {
            let view = args[0].as_str("uiviewvisible")?.to_owned();
            Ok(boolean(if call.view_holds(&view) {
                match call.views.as_deref_mut() {
                    Some(views) => views.is_view_visible(call.host, &view),
                    None => false,
                }
            } else {
                call.host.ui_view_visible(&view)
            }))
        })(),

        // ---- actions ----
        // The reference's echo takes the chat colour (a text type) as its
        // second argument, prints in it and answers true.
        "echo" => (|| {
            let text = context.display(&args[0]);
            let color = args[1].as_int32("echo")?;
            call.host.chat_post(&text, color.cast_unsigned());
            Ok(boolean(true))
        })(),
        // `chatbox` sends and hands the argument straight back.
        "chatbox" => (|| {
            let text = args[0].as_str("chatbox")?;
            if !text.is_empty()
                && let Some(profile) = call.services.submit_chat(call.host, text)
                && let Some(extension) = call.extension.as_deref_mut()
            {
                extension.profile_loaded(call.host, call.services, profile);
            }
            Ok(args[0].clone())
        })(),
        // `chatboxpaste` only stages the text, control characters removed.
        "chatboxpaste" => (|| {
            let text = strip_control_characters(args[0].as_str("chatboxpaste")?);
            Ok(boolean(!text.is_empty() && call.host.chat_compose(&text)))
        })(),
        // The selection is attempted and the answer is always false.
        "actiontryselect" => (|| {
            let object = call.object_read(args, 0, "actiontryselect[WorldObject]")?;
            call.host.select(object.object_id);
            Ok(ExpressionValue::zero())
        })(),
        "actiontryuseitem" => (|| {
            let object = call.object_read(args, 0, "actiontryuseitem[WorldObject]")?;
            Ok(boolean(call.host.use_item(object.object_id).accepted()))
        })(),
        "actiontryapplyitem" => (|| {
            let (source, target) =
                call.object_pair(args, "actiontryapplyitem[WorldObject, WorldObject]")?;
            Ok(boolean(call.host.apply_item(source, target).accepted()))
        })(),
        "actiontrygiveitem" => (|| {
            let (item, target) =
                call.object_pair(args, "actiontrygiveitem[WorldObject, WorldObject]")?;
            Ok(boolean(call.host.give_item(item, target, 0).accepted()))
        })(),
        "actiontrydrop" => (|| {
            let id = call.object_argument(args, 0, "actiontrydrop[WorldObject]")?;
            Ok(boolean(call.host.drop_item(id, 0).accepted()))
        })(),
        // The reference joins a stack of the same thing already in the
        // destination unless addToStack is given as 0.
        "actiontrymove" => (|| {
            let item = call.object_argument(args, 0, ACTION_TRY_MOVE_PARAMETERS)?;
            let destination = call.object_argument(args, 1, ACTION_TRY_MOVE_PARAMETERS)?;
            let slot = if args.len() >= 3 {
                args[2].as_int32("actiontrymove")?
            } else {
                0
            };
            let join = args.len() < 4 || args[3].as_number("actiontrymove")? != 0.0;
            Ok(boolean(
                call.host
                    .move_item(item, destination, 0, slot, join)
                    .accepted(),
            ))
        })(),
        // Nothing while the character is busy; otherwise select the item,
        // move the new stack size of it into the destination at the slot,
        // and answer 1 whatever the server then does.
        "actiontrysplit" => (|| {
            const PARAMETERS: &str =
                "actiontrysplit[WorldObject, number, WorldObject, number, number]";
            let item = call.object_argument(args, 0, PARAMETERS)?;
            let destination = if args.len() >= 3 {
                call.object_argument(args, 2, PARAMETERS)?
            } else {
                call.character().id
            };
            let slot = if args.len() >= 4 {
                unchecked_int(args[3].as_number("actiontrysplit")?)
            } else {
                0
            };
            let merge = args.len() >= 5 && args[4].as_number("actiontrysplit")? != 0.0;
            if call.is_busy() {
                return Ok(ExpressionValue::zero());
            }
            call.host.select(item);
            let amount = unchecked_int(args[1].as_number("actiontrysplit")?).cast_unsigned();
            call.host.move_item(item, destination, amount, slot, merge);
            Ok(ExpressionValue::one())
        })(),
        // 2 impossible, 0 not attempted yet, 1 begun.
        "actiontrycastbyid" => (|| {
            let spell = to_uint(&args[0], "actiontrycastbyid")?;
            call.cast_result(spell, None)
        })(),
        "actiontrycastbyidontarget" => (|| {
            let spell = to_uint(&args[0], "actiontrycastbyidontarget")?;
            let target =
                call.object_argument(args, 1, "actiontrycastbyidontarget[number, WorldObject]")?;
            call.cast_result(spell, Some(target))
        })(),
        "actiontryequipanywand" => Ok(boolean(call.magic_mode_step())),

        // ---- combat and movement ----
        "getcombatstate" => Ok(ExpressionValue::string(combat_mode_name(
            call.host.combat().mode,
        ))),
        "setcombatstate" => (|| {
            let text = args[0].as_str("setcombatstate")?;
            let Some(mode) = parse_combat_mode(text) else {
                return Ok(ExpressionValue::zero());
            };
            Ok(boolean(call.host.enter_combat_mode(mode).accepted()))
        })(),
        "getbusystate" => Ok(number(if call.is_busy() { 1.0 } else { 0.0 })),
        "getequippedweapontype" => {
            let mut answer = "None";
            for item in call
                .host
                .owned_equipment()
                .iter()
                .filter(|item| item.equipped_location != 0)
            {
                if item.equipped_location & 0x0040_0000 != 0 {
                    answer = "Missile";
                    break;
                }
                if item.equipped_location & 0x0100_0000 != 0 {
                    answer = "Wand";
                    break;
                }
                if item.equipped_location & 0x0010_0000 != 0 {
                    answer = "Melee";
                    break;
                }
            }
            Ok(ExpressionValue::string(answer))
        }
        "setmotion" => (|| {
            let motion = args[0].as_str("setmotion")?.to_owned();
            let enabled = args[1].as_number("setmotion")? != 0.0;
            let Some(held) = HeldMotion::parse(&motion) else {
                return Err(evaluation(format!(
                    "Invalid motion '{motion}'. Valid values are: {}",
                    held_motions::valid_names()
                )));
            };
            Ok(boolean(
                call.parts.held_motions.set(call.host, held, enabled) == NavigationStatus::Accepted,
            ))
        })(),
        "getmotion" => (|| {
            args[0].as_str("getmotion")?;
            Ok(number(if call.host.navigation().is_moving {
                2.0
            } else {
                0.0
            }))
        })(),
        "clearmotion" => Ok(boolean(
            call.parts.held_motions.clear(call.host) == NavigationStatus::Accepted,
        )),

        // ---- login ----
        "setnextlogin" => {
            let login = call.host.login();
            let roster = login_roster::sorted(login.roster);
            if !login.is_available || roster.is_empty() {
                Ok(ExpressionValue::zero())
            } else {
                // An index written in an expression always counts on from
                // the current character and always wraps.
                let selector = context.display(&args[0]);
                let current = call.character().name;
                Ok(
                    match login_roster::try_resolve(&roster, &selector, &current, true, true) {
                        Ok(index) => boolean(call.host.login_set_next(roster[index].object_id)),
                        Err(_) => ExpressionValue::zero(),
                    },
                )
            }
        }
        "clearnextlogin" => Ok(boolean(call.host.login_clear_next())),

        // ---- the other clients ----
        "netclients" => (|| {
            call.services
                .require_peers(call.host, PeerNeed::ClientState);
            let tag = if args.is_empty() {
                None
            } else {
                Some(args[0].as_str("netclients")?.to_owned())
            };
            let mut clients = Vec::new();
            for client in call.host.peer_clients() {
                if let Some(tag) = tag.as_deref().filter(|tag| !tag.is_empty())
                    && !client
                        .tags
                        .iter()
                        .any(|candidate| ordinal::equals_ignore_case(candidate, tag))
                {
                    continue;
                }
                let tags = new_list(
                    context,
                    client
                        .tags
                        .iter()
                        .map(|candidate| ExpressionValue::string(candidate))
                        .collect(),
                );
                clients.push(new_dictionary(
                    context,
                    vec![
                        ("ClientId", number(f64::from(client.client_id))),
                        (
                            "PlayerId",
                            ExpressionValue::object_id_number(client.player_id),
                        ),
                        ("Position", coordinates(&client.position)),
                        ("Name", ExpressionValue::string(&client.name)),
                        ("Tags", tags),
                        ("WorldName", ExpressionValue::string(&client.world_name)),
                        ("CurrentHealth", number(f64::from(client.current_health))),
                        ("CurrentMana", number(f64::from(client.current_mana))),
                        ("CurrentStamina", number(f64::from(client.current_stamina))),
                        ("MaxHealth", number(f64::from(client.max_health))),
                        ("MaxMana", number(f64::from(client.max_mana))),
                        ("MaxStamina", number(f64::from(client.max_stamina))),
                        ("Heading", number(f64::from(client.heading))),
                    ],
                ));
            }
            Ok(new_list(context, clients))
        })(),
        // What the other clients say they cast, read straight from the
        // client and never through the macro's own cursor.
        "netcasts" => (|| {
            call.services.require_peers(call.host, PeerNeed::Casts);
            let tag = if args.is_empty() {
                None
            } else {
                Some(args[0].as_str("netcasts")?.to_owned())
            };
            let tagged: Option<Vec<u32>> = match tag.as_deref().filter(|tag| !tag.is_empty()) {
                None => None,
                Some(tag) => Some(
                    call.host
                        .peer_clients()
                        .iter()
                        .filter(|client| {
                            client
                                .tags
                                .iter()
                                .any(|candidate| ordinal::equals_ignore_case(candidate, tag))
                        })
                        .map(|client| client.client_id)
                        .collect(),
                ),
            };
            let mut casts = Vec::new();
            for cast in call.host.peer_casts(0) {
                if tagged
                    .as_ref()
                    .is_some_and(|tagged| !tagged.contains(&cast.client_id))
                {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                casts.push(new_dictionary(
                    context,
                    vec![
                        ("Sequence", number(cast.sequence as f64)),
                        ("ClientId", number(f64::from(cast.client_id))),
                        (
                            "CasterId",
                            ExpressionValue::object_id_number(cast.caster_id),
                        ),
                        (
                            "TargetId",
                            ExpressionValue::object_id_number(cast.target_id),
                        ),
                        ("SpellId", number(f64::from(cast.spell_id))),
                        ("EffectiveSkill", number(f64::from(cast.effective_skill))),
                        ("SecondsRemaining", number(cast.seconds_remaining)),
                        ("Landed", boolean(cast.landed)),
                    ],
                ));
            }
            Ok(new_list(context, casts))
        })(),

        // ---- the runtime's own ----
        "xpreset" => {
            call.parts.experience.reset();
            Ok(ExpressionValue::one())
        }
        "xpmeter" => Ok(ExpressionValue::string(&call.parts.experience.format())),
        "xpduration" => Ok(number(call.parts.experience.duration_seconds)),
        "xptotal" => Ok(number(int64_number(call.parts.experience.experience))),
        "lumtotal" => Ok(number(int64_number(call.parts.experience.luminance))),
        "xpavg" => Ok(number(call.parts.experience.experience_per_hour())),
        "lumavg" => Ok(number(call.parts.experience.luminance_per_hour())),
        "testquestflag" => args[0]
            .as_str("testquestflag")
            .map(|key| boolean(call.parts.quests.has_completed(key))),
        "getqueststatus" => args[0].as_str("getqueststatus").map(|key| {
            let now = call.parts.clock.utc_now_ms();
            boolean(call.parts.quests.is_ready(key, now))
        }),
        "getquestktprogress" => args[0]
            .as_str("getquestktprogress")
            .map(|key| number(f64::from(call.parts.quests.progress(key)))),
        "getquestktrequired" => args[0]
            .as_str("getquestktrequired")
            .map(|key| number(f64::from(call.parts.quests.required(key)))),
        "isrefreshingquests" => Ok(boolean(call.parts.quests.is_refreshing())),
        "ustadd" => (|| {
            let id = call.object_argument(args, 0, "ustadd[WorldObject]")?;
            Ok(boolean(call.parts.salvage.add(call.host, id)))
        })(),
        "ustopen" => Ok(boolean(call.parts.salvage.open(call.host))),
        "ustsalvage" => Ok(boolean(call.parts.salvage.salvage(call.host))),
        "statushud" => (|| {
            let key = args[0].as_str("statushud")?.to_owned();
            let value = context.display(&args[1]);
            Ok(boolean(
                call.parts.status_hud.update(call.host, &key, &value, None),
            ))
        })(),
        // The colour is a 32-bit pattern, so a negative one is the colour
        // it meant rather than an error.
        "statushudcolored" => (|| {
            let key = args[0].as_str("statushudcolored")?.to_owned();
            let value = context.display(&args[1]);
            let raw = args[2].as_number("statushudcolored")?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let color = (raw as i64) as u32;
            Ok(boolean(call.parts.status_hud.update(
                call.host,
                &key,
                &value,
                Some(color),
            )))
        })(),
        "delayexec" => (|| {
            let delay = args[0].as_number("delayexec")?.max(0.0);
            let source = args[1].as_str("delayexec")?.to_owned();
            let id = call.parts.delay(delay, &source)?;
            Ok(number(f64::from(id)))
        })(),
        "clearexec" => args[0]
            .as_int32("clearexec")
            .map(|id| boolean(call.parts.delayed_remove(id))),
        _ => return None,
    })
}

fn finder_set(name: &str) -> ObjectSet {
    if name.contains("inventory") {
        ObjectSet::Inventory
    } else if name.contains("landscape") {
        ObjectSet::Landscape
    } else {
        ObjectSet::All
    }
}

fn combat_mode_name(mode: CombatMode) -> &'static str {
    match mode {
        CombatMode::Unknown => "Unknown",
        CombatMode::Peace => "Peace",
        CombatMode::Melee => "Melee",
        CombatMode::Missile => "Missile",
        CombatMode::Magic => "Magic",
    }
}

/// `Enum.TryParse(text, ignoreCase: true)` over the combat modes, Unknown
/// refused: a name in any case, or the mode's number.
fn parse_combat_mode(text: &str) -> Option<CombatMode> {
    let text = text.trim();
    let mode = if let Some(value) = mosstank_dotnet::try_parse_int(text) {
        u8::try_from(value).ok().and_then(CombatMode::from_tag)?
    } else {
        *CombatMode::ALL
            .iter()
            .find(|mode| ordinal::equals_ignore_case(combat_mode_name(**mode), text))?
    };
    (mode != CombatMode::Unknown).then_some(mode)
}

/// A spell as the reference's spelldata describes it, key for key.
fn spell_data(
    context: &EvaluationContext<'_>,
    spell: &ac_plugin_abi::magic::SpellInfo,
    info: &SpellExtras,
) -> Result<ExpressionValue> {
    let flags = spell.raw_flags;
    let flag = |bit: u32| number(if flags & bit != 0 { 1.0 } else { 0.0 });
    let components = new_list(
        context,
        spell
            .formula_component_ids
            .iter()
            .map(|&id| number(f64::from(id.cast_signed())))
            .collect(),
    );
    let duration = if matches!(spell.spell_type, 1 | 7 | 12) {
        f64::from(spell.duration_seconds)
    } else {
        -1.0
    };
    let school = school_name(spell.school)?;
    Ok(new_dictionary(
        context,
        vec![
            ("CasterEffect", number(f64::from(info.caster_effect))),
            ("ComponentIds", components),
            ("Description", ExpressionValue::string(&spell.description)),
            ("Difficulty", number(f64::from(spell.difficulty))),
            ("Duration", number(duration)),
            ("Family", number(f64::from(spell.family.cast_signed()))),
            ("Flags", number(f64::from(flags.cast_signed()))),
            ("Generation", number(f64::from(info.formula_version))),
            ("IconId", number(f64::from(spell.icon_id.cast_signed()))),
            ("Id", number(f64::from(spell.spell_id.cast_signed()))),
            ("IsDebuff", flag(0x10)),
            ("IsFastWindup", flag(0x4000)),
            ("IsFellowship", flag(0x2000)),
            ("IsIrresistible", flag(0x4)),
            ("IsOffensive", flag(0x1)),
            ("IsUntargetted", flag(0x8)),
            ("Mana", number(f64::from(spell.mana_cost))),
            ("Name", ExpressionValue::string(&spell.name)),
            ("School", ExpressionValue::string(school)),
            ("SortKey", number(f64::from(info.display_order))),
            ("Speed", number(f64::from(info.component_loss))),
            ("TargetEffect", number(f64::from(info.target_effect))),
            (
                "TargetMask",
                number(f64::from(spell.target_mask.cast_signed())),
            ),
            ("Type", number(f64::from(spell.spell_type))),
        ],
    ))
}

/// The host functions' names, in registration order.
#[must_use]
pub fn host_function_names() -> Vec<&'static str> {
    HOST_FUNCTIONS.iter().map(|(name, ..)| *name).collect()
}
