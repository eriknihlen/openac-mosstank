//! The Inventory window's search ([`InventorySearch`]): a port of Mag-Tools'
//! Tools, Inventory page (`InventoryToolsView`), widened to every character
//! the inventory logger saved on this server.
//!
//! - The pattern is a .NET regular expression, case ignored, matched
//!   against each item's info text ([`crate::item_info::item_info`], with
//!   the `ShowBuffedValues` and `ShowValueAndBurden` settings): empty
//!   matches everything, a pattern that does not compile lists nothing and
//!   says [`BAD_PATTERN`]; one that would compile too large to fit a call
//!   (`w{1000}`, `(w+) (w+) (w+)`) is refused and says
//!   [`PATTERN_TOO_LARGE`]. It is compiled once each time its text changes.
//! - The rows: the logged-in character's items, live (its own saved file is
//!   not read), then every other `Mag-Tools/<world>/*.inventory.json` in the
//!   plugin's data, read again each time the window is opened.
//! - Clicking a row of the logged-in character opens the side pack holding
//!   it, selects it and shows its text; another character's row shows the
//!   text its file gave. The text goes with the click once the list no
//!   longer shows the item (the pattern or a filter left it out).
//!
//! Nothing is done while the window is hidden. The work is staged so no
//! call does more than a tick's worth: a file is read in one tick, its items
//! found in the next, then [`ITEMS_PER_TICK`] items a tick are read and
//! their texts made (the live items the same); each text is made once and
//! kept. A new pattern is compiled in the call that typed it (a pattern
//! that compiles near the size limit is a call's worth of work), and
//! matched from the next tick: [`NEW_PATTERN_MATCHES`] texts first (the
//! engine builds its tables then), then [`MATCHES_PER_CALL`] a tick; a pattern that [`backtracks`] against
//! [`BACKTRACKING_MATCHES_PER_CALL`] a call, each text stopped after
//! [`STEP_LIMIT`] steps. The status says how far it is. The rows shown are
//! worked out again at once for what the player does (a pattern, a filter),
//! and otherwise only once the reading and matching are done, so thousands
//! of rows are not pushed to the window at every tick of the work.
//!
//! The filter window's filter ([`InventoryFilter`]) is ANDed with the
//! pattern: each row's facts are worked out once when it is read (its
//! spells' names from the spell table, each spell asked for once), and a
//! filter set is applied [`FILTER_DELAY_MS`] after its last change, the
//! rows tested [`FILTER_TESTS_PER_CALL`] a tick. The characters searched
//! are the filter's; the status counts the rows left out only because they
//! were never appraised (`N not identified`).
//!
//! A weapon's OD (the OD column, the Weapon OD filter) is the highest step
//! of the loaded loot profile's `(OD +N)` rules it matches
//! ([`mosstank_loot::od`]); every character's items are judged by the same
//! profile. It is worked out with the row's facts, once, for an appraised
//! item; the shell gives the profile before each tick
//! ([`InventorySearch::set_loot_rules`]), which looks at it once a second,
//! and only a change of its OD rules has every row read again.
//!
//! Where it differs from Mag-Tools: the rows of other characters, the
//! filters, and the Count, Where, Slot, Wield, OD and Value columns are
//! new; an item that leaves the inventory leaves the rows (Mag-Tools' list
//! held what it found until the next keystroke); a text the pattern cannot
//! be matched against within [`STEP_LIMIT`] steps is not listed, and the
//! status counts it (.NET, with no timeout, ran until it was done).
//!
//! Where it differs from the plan: a file is read and its items found whole
//! (one storage read in one tick, finding its items in the next), not 64 KB
//! a tick: the storage reads whole files, and the fuel case's 1,500-item
//! file fits (the reading ticks' worst is under 30% of the budget).

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ItemProperties, ObjectChange, ObjectClass};
use mosstank_dotnet::text::ordinal;
use mosstank_formats::utl::LootRule;
use mosstank_loot::od::OdRules;
use openac_plugin_compat::regex::{
    DotNetRegex, RegexError, RegexLimits, RegexOptions, UNTRUSTED_PATTERN_SIZE_LIMIT,
};

use crate::host::MagHost;
use crate::inventory_filter::{InventoryFilter, RowFacts, Verdict, set_name};
use crate::inventory_log::{LogReader, storage_key};
use crate::item_info::{ItemInfoInput, display_name, item_info, names};

/// The filters' choice that keeps every row.
pub const ALL: &str = "All";

/// The status shown for a pattern that does not compile.
pub const BAD_PATTERN: &str = "bad pattern";

/// How often, milliseconds, the loaded loot profile is looked at for a
/// change of its OD rules while the window is shown: reading a large
/// profile's rules costs about a million instructions, too much for every
/// tick.
pub const OD_CHECK_MS: f64 = 1000.0;

/// The status line's note while the Weapon OD filter is set and the loaded
/// loot profile has no `(OD +N)` rule (so no item has an OD).
pub const OD_NEEDS_RULES: &str = "Weapon OD needs a loot profile with (OD +N) rules";

/// The status shown for a pattern refused because it would compile past
/// the size a typed pattern may take ([`UNTRUSTED_PATTERN_SIZE_LIMIT`]).
pub const PATTERN_TOO_LARGE: &str = "pattern too large";

/// Items read (from a file, or live) and given their text a tick.
pub const ITEMS_PER_TICK: usize = 50;

/// Item texts matched against the pattern in one call (a tenth of it on a
/// tick that also read items).
pub const MATCHES_PER_CALL: usize = 1500;

/// Item texts matched on the first tick after a new pattern is typed: its
/// first matches build the engine's tables, several times the cost of
/// later ones.
pub const NEW_PATTERN_MATCHES: usize = 500;

/// Item texts matched in one call against a pattern the engine must
/// backtrack over ([`backtracks`]): at most [`STEP_LIMIT`] steps each. A
/// text stopped at the limit cost about a million instructions on the fuel
/// case's heavy character (some 700 a step), so a call's matching stays
/// under 30 million instructions whatever the pattern.
pub const BACKTRACKING_MATCHES_PER_CALL: usize = 25;

/// The backtracking steps one text may take; past them the text counts as
/// too costly to search. The look-arounds the window is used with take
/// fewer (the fuel case holds `(?<!x)Pack` and `(?=.*Sword)(?=.*Fire)` to
/// none too costly over its 6,000 texts).
pub const STEP_LIMIT: usize = 1500;

/// Rows tested against a new filter in one call (a tenth of it on a tick
/// that also read items): each test reads the row's facts, worked out when
/// it was read.
pub const FILTER_TESTS_PER_CALL: usize = 1500;

/// How long, milliseconds, a filter change waits for the next one before
/// the rows are tested against it (the website's 0.4 s).
pub const FILTER_DELAY_MS: f64 = 400.0;

/// The file name every inventory log ends with.
const LOG_SUFFIX: &str = ".inventory.json";

/// The appraisal's mark on a spell active on the item.
const ACTIVE_ON_ITEM: u32 = 0x8000_0000;

/// One equipment set and how many items of the characters searched are
/// of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCount {
    /// The set's id.
    pub id: i32,
    /// Its name.
    pub name: String,
    /// The items of it.
    pub count: usize,
}

/// One spell as the rows read it.
#[derive(Debug)]
struct CachedSpell {
    info: SpellInfo,
    /// Its name, lowercase.
    lower: Rc<str>,
}

/// The spell table's answers, each asked for once: thousands of rows share
/// a few hundred spells.
#[derive(Debug, Default)]
struct SpellCache {
    known: HashMap<u32, Option<CachedSpell>>,
}

impl SpellCache {
    fn get(&mut self, host: &mut dyn MagHost, id: u32) -> Option<&CachedSpell> {
        self.known
            .entry(id)
            .or_insert_with(|| {
                host.spell(id).map(|info| CachedSpell {
                    lower: Rc::from(info.name.to_lowercase()),
                    info,
                })
            })
            .as_ref()
    }

    /// The names of the item's spells, lowercase: the appraisal's list
    /// with the active ones, each once; a spell the table does not know
    /// has none.
    fn names(&mut self, host: &mut dyn MagHost, item: &InventoryItem) -> Vec<Rc<str>> {
        let mut ids: Vec<u32> = Vec::with_capacity(item.appraised_spell_ids.len());
        for id in &item.appraised_spell_ids {
            let id = id & !ACTIVE_ON_ITEM;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids.into_iter()
            .filter_map(|id| self.get(host, id).map(|spell| Rc::clone(&spell.lower)))
            .collect()
    }
}

/// The settings the item texts are made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InfoSettings {
    /// `Mag.ItemInfoOnIdent.ShowBuffedValues`.
    pub show_buffed_values: bool,
    /// `Mag.ItemInfoOnIdent.ShowValueAndBurden`.
    pub show_value_and_burden: bool,
}

/// The pattern uses what only a backtracking engine matches: a look-ahead
/// or look-behind, an atomic group, a conditional, or a back-reference.
/// Without them the engine runs in time linear in the text.
#[must_use]
pub fn backtracks(pattern: &str) -> bool {
    let grouped = ["(?=", "(?!", "(?<=", "(?<!", "(?>", "(?("]
        .iter()
        .any(|opening| pattern.contains(opening));
    let referenced = pattern
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'\\' && (matches!(pair[1], b'1'..=b'9') || pair[1] == b'k'));
    grouped || referenced
}

/// The pattern as compiled.
#[derive(Debug, Default)]
enum Pattern {
    /// Empty: every item.
    #[default]
    All,
    /// A .NET regular expression, and whether it [`backtracks`].
    Regex(Box<DotNetRegex>, bool),
    /// It does not compile: no item.
    Bad,
    /// It would compile too large: no item.
    TooLarge,
}

impl Pattern {
    fn compile(text: &str) -> Self {
        if text.is_empty() {
            return Self::All;
        }
        let limits = RegexLimits {
            step_limit: STEP_LIMIT,
            compiled_size_limit: Some(UNTRUSTED_PATTERN_SIZE_LIMIT),
        };
        match DotNetRegex::with_limits(text, RegexOptions::IGNORE_CASE, limits) {
            Ok(regex) => Self::Regex(Box::new(regex), backtracks(text)),
            Err(RegexError::TooLarge(_)) => Self::TooLarge,
            Err(_) => Self::Bad,
        }
    }

    /// Whether `text` matches; `None` when it ran past the step limit.
    fn is_match(&self, text: &str) -> Option<bool> {
        match self {
            Self::All => Some(true),
            Self::Regex(regex, _) => regex.is_match(text).ok(),
            Self::Bad | Self::TooLarge => Some(false),
        }
    }

    fn is_regex(&self) -> bool {
        matches!(self, Self::Regex(..))
    }

    /// The texts matched in one call: `first`, the first call after it was
    /// typed.
    fn matches_per_call(&self, first: bool) -> usize {
        match self {
            Self::Regex(_, true) => BACKTRACKING_MATCHES_PER_CALL,
            _ if first => NEW_PATTERN_MATCHES,
            _ => MATCHES_PER_CALL,
        }
    }
}

/// One item, as the rows show it.
#[derive(Debug, Clone)]
struct Row {
    id: u32,
    name: String,
    class: ObjectClass,
    icon: u32,
    count: i32,
    container_id: u32,
    container_slot: i32,
    worn: bool,
    /// Its info text.
    info: String,
    /// The pattern generation it was last matched against.
    tested: u64,
    matched: bool,
    /// Matching it ran past the step limit.
    too_costly: bool,
    /// What the filters read of it.
    facts: RowFacts,
    /// The filter generation it was last tested against, and the verdict.
    filter_tested: u64,
    verdict: Verdict,
}

impl Row {
    fn new(
        item: &InventoryItem,
        properties: &ItemProperties,
        info: String,
        facts: RowFacts,
    ) -> Self {
        Self {
            facts,
            filter_tested: 0,
            verdict: Verdict::Pass,
            id: item.object_id,
            name: display_name(item, properties),
            class: item.object_class,
            icon: item.icon_id,
            count: item.stack_size,
            container_id: item.container_id,
            container_slot: item.container_slot,
            worn: is_worn(item),
            info,
            tested: 0,
            matched: false,
            too_costly: false,
        }
    }
}

/// The item is worn or wielded.
fn is_worn(item: &InventoryItem) -> bool {
    item.wielder_id != 0 || item.equipped_location != 0
}

/// One character's items.
#[derive(Debug, Clone, Default)]
struct Owner {
    name: String,
    /// Its log's storage key.
    key: String,
    rows: Vec<Row>,
}

impl Owner {
    /// Its side packs: each one's place among the packs.
    fn packs(&self) -> HashMap<u32, i32> {
        self.rows
            .iter()
            .filter(|row| row.class == ObjectClass::Container)
            .map(|row| (row.id, row.container_slot))
            .collect()
    }
}

/// Where an item is: `Worn`, `Side pack N` (N its pack's place among the
/// packs, from 1), or `Pack`.
fn place(row: &Row, packs: &HashMap<u32, i32>) -> String {
    if row.worn {
        return "Worn".to_owned();
    }
    match packs.get(&row.container_id) {
        Some(&slot) if slot >= 0 => format!("Side pack {}", slot + 1),
        Some(_) => "Side pack".to_owned(),
        None => "Pack".to_owned(),
    }
}

/// The logged-in character.
#[derive(Debug, Default)]
struct Live {
    owner: Owner,
    player_id: u32,
    world: String,
    /// Items still to read when its rows are first made.
    pending: VecDeque<InventoryItem>,
    /// Items changed since they were read, in the order heard.
    dirty: VecDeque<u32>,
    dirty_set: HashSet<u32>,
    /// The ids of its rows and of the items still to read.
    known: HashSet<u32>,
}

/// The file being read.
#[derive(Debug)]
enum Reading {
    /// Read, its items not yet found.
    Text { key: String, text: String },
    /// Its items being read.
    Items {
        reader: Box<LogReader>,
        owner: Owner,
    },
}

/// Which item each row shown is: what a click resolves against.
#[derive(Debug, Default)]
struct Targets {
    /// Each row's owner (in `owner_keys`) and the item's id.
    entries: Vec<(usize, u32)>,
    /// The owners' log keys.
    owner_keys: Vec<String>,
}

/// What the window shows, worked out once each time it changes.
#[derive(Debug, Default)]
struct View {
    targets: Rc<Targets>,
    characters: Vec<String>,
    names: Vec<String>,
    counts: Vec<String>,
    places: Vec<String>,
    icons: Vec<u32>,
    slots: Vec<String>,
    wields: Vec<String>,
    ods: Vec<String>,
    values: Vec<String>,
    wield_keys: Vec<String>,
    od_keys: Vec<String>,
    value_keys: Vec<String>,
    character_choices: Vec<String>,
    class_choices: Vec<String>,
    set_counts: Vec<SetCount>,
    /// Rows left out only because they were never appraised.
    not_identified: usize,
    selected: i32,
}

/// The Inventory window's state.
#[derive(Debug, Default)]
pub struct InventorySearch {
    pattern_text: String,
    pattern: Pattern,
    /// Bumped by each new pattern; a row matched against an older one is
    /// matched again.
    generation: u64,
    /// The pattern is new: nothing has been matched against it yet.
    fresh: bool,
    settings: Option<InfoSettings>,
    live: Option<Live>,
    /// No character to search live (none in the world).
    no_character: bool,
    others: Vec<Owner>,
    /// The other characters' files are to be listed again.
    relist: bool,
    /// Listed at least once since the window was first shown.
    listed: bool,
    files: VecDeque<String>,
    /// The keys listed this time: the others not among them are dropped
    /// once every file is read.
    found: Vec<String>,
    reading: Option<Reading>,
    was_shown: bool,
    /// The filter the rows are tested against.
    filter: InventoryFilter,
    /// A filter set and not yet applied, and since when (the first tick
    /// after it was set, milliseconds).
    pending_filter: Option<(InventoryFilter, Option<f64>)>,
    /// Bumped by each filter applied that changes what the rows are tested
    /// against.
    filter_generation: u64,
    /// Bumped by each filter set or applied.
    filter_revision: u64,
    /// The loaded loot profile's OD rules, which each row's OD is worked
    /// out by when it is read.
    od_rules: Rc<OdRules>,
    /// [`OdRules::fingerprint`] of the rules they were built from; none
    /// before the first profile was given.
    od_fingerprint: Option<u64>,
    /// When the profile's OD rules were last looked at, milliseconds.
    od_checked_ms: Option<f64>,
    spells: SpellCache,
    class_filter: String,
    /// The clicked item: its owner's key and its id.
    clicked: Option<(String, u32)>,
    info: String,
    view: View,
    /// The rows as the window last had them (their names pushed): a click
    /// is on one of them.
    pushed: Option<Rc<Targets>>,
    /// The view is to be worked out again at once (a pattern, a filter).
    view_stale: bool,
    /// The rows changed since the view was worked out: it is worked out
    /// again once the reading and matching are done.
    changed: bool,
    revision: u64,
}

impl InventorySearch {
    /// The pattern as typed.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern_text
    }

    /// A new pattern typed: compiled (when its text changed); the item
    /// texts are matched against it on the next ticks, the rows shown once
    /// they all are.
    pub fn set_pattern(&mut self, text: &str) {
        if self.generation != 0 && text == self.pattern_text {
            return;
        }
        text.clone_into(&mut self.pattern_text);
        self.pattern = Pattern::compile(text);
        self.generation += 1;
        // Compiling is this call's work: the matching starts on the next
        // tick (an empty or refused pattern settles every row at no cost).
        self.match_some(0);
        self.fresh = true;
        self.view_stale = true;
        self.refresh_view();
    }

    /// The filter as last set (applied or still waiting).
    #[must_use]
    pub fn filter(&self) -> &InventoryFilter {
        self.pending_filter
            .as_ref()
            .map_or(&self.filter, |(filter, _)| filter)
    }

    /// A new filter: the rows are tested against it from
    /// [`FILTER_DELAY_MS`] after the last change, within each tick's
    /// budget; until then the rows shown stay.
    pub fn set_filter(&mut self, filter: InventoryFilter) {
        if filter == *self.filter() {
            return;
        }
        self.pending_filter = Some((filter, None));
        self.filter_revision += 1;
    }

    /// The filter as last set, changed by `edit` (a control of the
    /// filters window), and set ([`Self::set_filter`]).
    pub fn edit_filter(&mut self, edit: impl FnOnce(&mut InventoryFilter)) {
        let mut filter = self.filter().clone();
        edit(&mut filter);
        self.set_filter(filter);
    }

    /// Every filter cleared (the Clear button).
    pub fn clear_filter(&mut self) {
        self.set_filter(InventoryFilter::default());
    }

    /// The filter's summary line ([`InventoryFilter::summary`]).
    #[must_use]
    pub fn filter_summary(&self) -> String {
        self.filter().summary()
    }

    /// Every character searched or not: the logged-in one, then the saved
    /// ones (the Characters tab).
    #[must_use]
    pub fn character_names(&self) -> Vec<String> {
        self.view.character_choices.clone()
    }

    /// Every equipment set of the set table (and any other set held), with
    /// how many items of the characters searched are of it: the most first,
    /// then by name, the ones nobody holds last at 0 (the Set tab). The
    /// other filters do not change it.
    #[must_use]
    pub fn set_counts(&self) -> &[SetCount] {
        &self.view.set_counts
    }

    /// Applies a filter: the rows are tested against it on the next ticks
    /// (a change of the characters alone is shown at once).
    fn apply_filter(&mut self, filter: InventoryFilter) {
        let rows_differ = filter.for_rows() != self.filter.for_rows();
        self.filter = filter;
        self.filter_revision += 1;
        if rows_differ {
            self.filter_generation += 1;
            self.changed = true;
        } else {
            self.view_stale = true;
            self.refresh_view();
        }
    }

    /// Applies the filter set once it has waited [`FILTER_DELAY_MS`].
    fn apply_pending_filter(&mut self, host: &mut dyn MagHost) {
        let Some((_, since)) = &mut self.pending_filter else {
            return;
        };
        let now = host.now_ms();
        let since = *since.get_or_insert(now);
        if now - since >= FILTER_DELAY_MS
            && let Some((filter, _)) = self.pending_filter.take()
        {
            self.apply_filter(filter);
        }
    }

    /// The Object class filter's choice.
    #[must_use]
    pub fn class_filter(&self) -> &str {
        if self.class_filter.is_empty() {
            ALL
        } else {
            &self.class_filter
        }
    }

    /// Only this kind of object ([`ALL`]: every one).
    pub fn set_class_filter(&mut self, choice: &str) {
        if choice != self.class_filter() {
            choice.clone_into(&mut self.class_filter);
            self.view_stale = true;
            self.refresh_view();
        }
    }

    /// The Refresh button: the other characters' files are listed and read
    /// again from the next tick, as opening the window does (a file another
    /// client rewrote shows what it holds now). The rows shown stay until
    /// the reading is done.
    pub fn refresh(&mut self) {
        self.relist = true;
    }

    /// The loot profile loaded now (empty for none), whose `(OD +N)` rules
    /// give each weapon its OD: given before each tick at `now_ms`, and
    /// looked at on the window's first tick shown and then every
    /// [`OD_CHECK_MS`]. Its OD rules are built again only when they
    /// changed ([`OdRules::fingerprint`]), and then, when rows were read
    /// with the old ones, every row is read again from the next tick (as a
    /// settings change does); the rows shown stay until then. The
    /// profile's other rules change nothing.
    pub fn set_loot_rules(&mut self, rules: &[LootRule], now_ms: f64) {
        let recently = self
            .od_checked_ms
            .is_some_and(|at| (at..at + OD_CHECK_MS).contains(&now_ms));
        if self.was_shown && recently {
            return;
        }
        self.od_checked_ms = Some(now_ms);
        let fingerprint = OdRules::fingerprint(rules);
        if self.od_fingerprint == Some(fingerprint) {
            return;
        }
        self.od_fingerprint = Some(fingerprint);
        let od_rules = OdRules::new(rules);
        if od_rules == *self.od_rules {
            return;
        }
        self.od_rules = Rc::new(od_rules);
        if self.settings.is_some() {
            self.read_again();
        }
    }

    /// Every row is to be read again: the live character's from its
    /// inventory, the others' from their files.
    fn read_again(&mut self) {
        self.live = None;
        self.others.clear();
        self.reading = None;
        self.files.clear();
        self.relist = true;
        self.changed = true;
    }

    /// The character left the world: every row is forgotten; the pattern
    /// and the filters stay, and the loot profile's OD rules.
    pub fn reset(&mut self) {
        let pattern_text = std::mem::take(&mut self.pattern_text);
        let pattern = std::mem::take(&mut self.pattern);
        let generation = self.generation;
        let filter = std::mem::take(&mut self.filter);
        let pending_filter = self.pending_filter.take();
        let filter_generation = self.filter_generation;
        let spells = std::mem::take(&mut self.spells);
        let filter_revision = self.filter_revision;
        let od_rules = std::mem::take(&mut self.od_rules);
        let od_fingerprint = self.od_fingerprint;
        let class_filter = std::mem::take(&mut self.class_filter);
        let revision = self.revision;
        *self = Self {
            pattern_text,
            pattern,
            generation,
            filter,
            pending_filter,
            filter_generation,
            filter_revision,
            od_rules,
            od_fingerprint,
            spells,
            class_filter,
            revision: revision + 1,
            ..Self::default()
        };
    }

    /// An object changed: a live item is read again on a later tick, a new
    /// one is added, one gone is dropped.
    pub fn on_object_changed(&mut self, change: &ObjectChange) {
        let Some(live) = &mut self.live else {
            return;
        };
        let id = change.object_id;
        let owned = change
            .current
            .as_ref()
            .is_some_and(|object| object.is_owned);
        let listed = live.known.contains(&id);
        if (owned || listed) && live.dirty_set.insert(id) {
            live.dirty.push_back(id);
        }
    }

    /// One tick: a tick's worth of reading, then of matching. Nothing while
    /// the window is hidden; opening it lists the other characters' files
    /// again. True when it did a tick's worth of work.
    pub fn tick(&mut self, host: &mut dyn MagHost, shown: bool, settings: InfoSettings) -> bool {
        if !shown {
            self.was_shown = false;
            return false;
        }
        if !self.was_shown {
            self.was_shown = true;
            self.relist = true;
        }
        if self.settings != Some(settings) {
            if self.settings.is_some() {
                // Every text is made again with the new settings.
                self.read_again();
            }
            self.settings = Some(settings);
        }
        self.apply_pending_filter(host);
        let worked = self.read_some(host, settings);
        let budget = self.pattern.matches_per_call(self.fresh);
        let matched = self.match_some(if worked { budget / 10 } else { budget });
        if matched > 0 {
            self.fresh = false;
        }
        let filter_budget = if worked {
            FILTER_TESTS_PER_CALL / 10
        } else {
            FILTER_TESTS_PER_CALL
        };
        let filtered = self.filter_some(filter_budget);
        // The rows shown are worked out on a tick of their own (thousands of
        // them are a tick's worth, and so is pushing them): the window's
        // push waits for the next light tick.
        let refreshed =
            self.changed && !worked && matched == 0 && filtered == 0 && self.work_done();
        if refreshed {
            self.view_stale = true;
            self.refresh_view();
        }
        worked || refreshed || matched >= budget / 10 || filtered >= filter_budget / 10
    }

    /// Nothing is left to read or match, and the rows shown are the rows.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.work_done() && !self.changed
    }

    /// Nothing is left to read or match.
    fn work_done(&self) -> bool {
        self.listed
            && !self.relist
            && self.pending_filter.is_none()
            && self.reading.is_none()
            && self.files.is_empty()
            && (self.no_character
                || self
                    .live
                    .as_ref()
                    .is_some_and(|live| live.pending.is_empty() && live.dirty.is_empty()))
            && self.untested().0 == 0
    }

    /// A row clicked: a live item is selected, its side pack opened first
    /// when it is in one, and its text shown as it is now; another
    /// character's item shows the text its file gave.
    pub fn click(&mut self, host: &mut dyn MagHost, row: i32, settings: InfoSettings) {
        // The row as the window last had it: the rows worked out since may
        // differ (a file read again, an item gone).
        let targets = Rc::clone(self.pushed.as_ref().unwrap_or(&self.view.targets));
        let Some(&(owner, id)) = usize::try_from(row)
            .ok()
            .and_then(|row| targets.entries.get(row))
        else {
            return;
        };
        let key = targets.owner_keys[owner].clone();
        let Some(item) = self
            .owners()
            .find(|owner| owner.key == key)
            .and_then(|owner| owner.rows.iter().find(|row| row.id == id))
        else {
            return;
        };
        let mut info = item.info.clone();
        if let Some(live) = self.live.as_ref().filter(|live| live.owner.key == key) {
            let player_id = live.player_id;
            let owned = host.owned_items_shared();
            if let Some(item) = owned.iter().find(|item| item.object_id == id) {
                if !is_worn(item) && item.container_id != 0 && item.container_id != player_id {
                    host.use_item(item.container_id);
                }
                host.select(id);
                let properties = host.item_properties(id).unwrap_or_default();
                info = info_text(host, &mut self.spells, item, &properties, settings);
            }
        }
        self.info = info;
        self.clicked = Some((key, id));
        self.view.selected = row;
        self.revision += 1;
    }

    /// The clicked item's text.
    #[must_use]
    pub fn info(&self) -> &str {
        &self.info
    }

    /// The clicked item's row, -1 when it is not shown.
    #[must_use]
    pub fn selected_row(&self) -> i32 {
        self.view.selected
    }

    /// The status line: [`BAD_PATTERN`], [`PATTERN_TOO_LARGE`], the work in
    /// hand, or how many items are shown, with notes (too costly, not
    /// identified, [`OD_NEEDS_RULES`]).
    #[must_use]
    pub fn status(&self) -> String {
        match self.pattern {
            Pattern::Bad => return BAD_PATTERN.to_owned(),
            Pattern::TooLarge => return PATTERN_TOO_LARGE.to_owned(),
            Pattern::All | Pattern::Regex(..) => {}
        }
        let (untested, too_costly) = self.untested();
        let total = || self.owners().map(|owner| owner.rows.len()).sum::<usize>();
        if untested > 0 || self.pending_filter.is_some() {
            let total = total();
            // A filter waiting: none tested against it yet.
            let untested = if self.pending_filter.is_some() {
                total
            } else {
                untested
            };
            return format!("Searching... {} of {total}", total - untested);
        }
        if !self.work_done() {
            return format!("Reading... {} items", total());
        }
        if self.changed {
            // Matched, the rows not yet worked out.
            let total = total();
            return format!("Searching... {total} of {total}");
        }
        let shown = match self.view.targets.entries.len() {
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        };
        let mut notes = Vec::new();
        if too_costly > 0 {
            notes.push(format!("{too_costly} too costly to search"));
        }
        if self.view.not_identified > 0 {
            notes.push(format!("{} not identified", self.view.not_identified));
        }
        if self.filter.min_od.is_some() && self.od_rules.is_empty() {
            notes.push(OD_NEEDS_RULES.to_owned());
        }
        if notes.is_empty() {
            shown
        } else {
            format!("{shown} ({})", notes.join(", "))
        }
    }

    /// A number that changes whenever the rows shown, the filters' choices,
    /// the selection or the clicked item's text do.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A number that changes whenever the filter (as last set) does: the
    /// filters window and the summary line; the rows change with
    /// [`Self::revision`], once the filter is applied.
    #[must_use]
    pub fn filter_revision(&self) -> u64 {
        self.filter_revision
    }

    /// The Object class filter's choices: [`ALL`], then each class present,
    /// by name.
    #[must_use]
    pub fn classes(&self) -> Vec<String> {
        choices(&self.view.class_choices)
    }

    /// The rows' Character column.
    #[must_use]
    pub fn row_characters(&self) -> Vec<String> {
        self.view.characters.clone()
    }

    /// The rows' Name column, as the window is given it: a click is on
    /// one of these rows.
    #[must_use]
    pub fn row_names(&mut self) -> Vec<String> {
        self.pushed = Some(Rc::clone(&self.view.targets));
        self.view.names.clone()
    }

    /// The rows' Count column.
    #[must_use]
    pub fn row_counts(&self) -> Vec<String> {
        self.view.counts.clone()
    }

    /// The rows' Where column.
    #[must_use]
    pub fn row_places(&self) -> Vec<String> {
        self.view.places.clone()
    }

    /// The rows' icons (portal.dat icon ids).
    #[must_use]
    pub fn row_icons(&self) -> Vec<u32> {
        self.view.icons.clone()
    }

    /// The rows' Slot column: where each can be worn or wielded, `-` for
    /// nowhere.
    #[must_use]
    pub fn row_slots(&self) -> Vec<String> {
        self.view.slots.clone()
    }

    /// The rows' Wield column: the character level each needs, blank for
    /// none.
    #[must_use]
    pub fn row_wields(&self) -> Vec<String> {
        self.view.wields.clone()
    }

    /// The rows' OD column: each weapon's OD by the loaded loot profile's
    /// `(OD +N)` rules, blank for none.
    #[must_use]
    pub fn row_ods(&self) -> Vec<String> {
        self.view.ods.clone()
    }

    /// The rows' Value column (grouped digits), blank for none.
    #[must_use]
    pub fn row_values(&self) -> Vec<String> {
        self.view.values.clone()
    }

    /// The OD column's sort keys: the OD as plain digits, blank for none.
    #[must_use]
    pub fn row_od_keys(&self) -> Vec<String> {
        self.view.od_keys.clone()
    }

    /// The Wield column's sort keys: the level as plain digits, blank for
    /// none (the list parses them for a number sort, and puts a blank key
    /// last both ways).
    #[must_use]
    pub fn row_wield_keys(&self) -> Vec<String> {
        self.view.wield_keys.clone()
    }

    /// The Value column's sort keys: the value as plain digits, blank for
    /// none.
    #[must_use]
    pub fn row_value_keys(&self) -> Vec<String> {
        self.view.value_keys.clone()
    }

    /// The live character's items, then the other characters'.
    fn owners(&self) -> impl Iterator<Item = &Owner> {
        self.live
            .as_ref()
            .map(|live| &live.owner)
            .into_iter()
            .chain(&self.others)
    }

    /// The rows not yet matched against the pattern, and those it could
    /// not be matched against within the step limit.
    fn untested(&self) -> (usize, usize) {
        let regex = self.pattern.is_regex();
        let generation = self.generation;
        let filter_generation = self.filter_generation;
        self.owners()
            .flat_map(|owner| &owner.rows)
            .fold((0, 0), |(untested, costly), row| {
                if (regex && row.tested != generation) || row.filter_tested != filter_generation {
                    (untested + 1, costly)
                } else {
                    (untested, costly + usize::from(regex && row.too_costly))
                }
            })
    }

    /// Tests up to `budget` rows against the filter; how many it tested.
    fn filter_some(&mut self, budget: usize) -> usize {
        let filter_generation = self.filter_generation;
        let filter = &self.filter;
        let mut done = 0;
        let owners = self
            .live
            .as_mut()
            .map(|live| &mut live.owner)
            .into_iter()
            .chain(&mut self.others);
        'owners: for owner in owners {
            for row in &mut owner.rows {
                if row.filter_tested == filter_generation {
                    continue;
                }
                if done >= budget {
                    break 'owners;
                }
                filter_row(row, filter, filter_generation);
                done += 1;
            }
        }
        if done > 0 {
            self.changed = true;
        }
        done
    }

    /// Matches up to `budget` rows against the pattern (an empty or bad
    /// pattern costs nothing and settles every row); how many it matched.
    fn match_some(&mut self, budget: usize) -> usize {
        let generation = self.generation;
        let pattern = &self.pattern;
        let costly = pattern.is_regex();
        let mut done = 0;
        let owners = self
            .live
            .as_mut()
            .map(|live| &mut live.owner)
            .into_iter()
            .chain(&mut self.others);
        'owners: for owner in owners {
            for row in &mut owner.rows {
                if row.tested == generation {
                    continue;
                }
                if costly && done >= budget {
                    break 'owners;
                }
                test(row, pattern, generation);
                done += 1;
            }
        }
        if done > 0 {
            self.changed = true;
        }
        if costly { done } else { 0 }
    }

    /// A tick's worth of reading: the live character's items first, then
    /// the other characters' files. True when it did any.
    fn read_some(&mut self, host: &mut dyn MagHost, settings: InfoSettings) -> bool {
        if self.live.is_none() && !self.no_character {
            return self.start_live(host);
        }
        if let Some(live) = &mut self.live {
            if !live.pending.is_empty() {
                let tests = Tests {
                    pattern: &self.pattern,
                    generation: self.generation,
                    filter: &self.filter,
                    filter_generation: self.filter_generation,
                    od: &self.od_rules,
                };
                for _ in 0..ITEMS_PER_TICK {
                    let Some(item) = live.pending.pop_front() else {
                        break;
                    };
                    let row = live_row(host, &mut self.spells, &item, settings, &tests);
                    live.owner.rows.push(row);
                }
                self.changed = true;
                return true;
            }
            if !live.dirty.is_empty() {
                self.read_dirty(host, settings);
                return true;
            }
        }
        if self.relist {
            self.list(host);
            return true;
        }
        self.read_file(host, settings)
    }

    /// The logged-in character: who it is, and its items to read.
    fn start_live(&mut self, host: &mut dyn MagHost) -> bool {
        let Some(character) = host.character() else {
            self.no_character = true;
            return false;
        };
        let pending: VecDeque<InventoryItem> = host.owned_items_shared().iter().cloned().collect();
        let known = pending.iter().map(|item| item.object_id).collect();
        self.live = Some(Live {
            owner: Owner {
                key: storage_key(&character.world, &character.name),
                name: character.name,
                rows: Vec::new(),
            },
            player_id: character.id,
            world: character.world,
            pending,
            known,
            ..Live::default()
        });
        self.changed = true;
        true
    }

    /// Reads again up to a tick's worth of the live items changed.
    fn read_dirty(&mut self, host: &mut dyn MagHost, settings: InfoSettings) {
        let Some(live) = &mut self.live else {
            return;
        };
        let owned = host.owned_items_shared();
        let tests = Tests {
            pattern: &self.pattern,
            generation: self.generation,
            filter: &self.filter,
            filter_generation: self.filter_generation,
            od: &self.od_rules,
        };
        let mut complete = None;
        let mut later = Vec::new();
        for _ in 0..ITEMS_PER_TICK {
            let Some(id) = live.dirty.pop_front() else {
                break;
            };
            live.dirty_set.remove(&id);
            let at = live.owner.rows.iter().position(|row| row.id == id);
            if let Some(item) = owned.iter().find(|item| item.object_id == id) {
                let row = live_row(host, &mut self.spells, item, settings, &tests);
                match at {
                    Some(at) => live.owner.rows[at] = row,
                    None => {
                        live.known.insert(id);
                        live.owner.rows.push(row);
                    }
                }
            } else if let Some(at) = at {
                // Gone, once the client has listed the whole inventory.
                if *complete.get_or_insert_with(|| host.owned_inventory_complete()) {
                    live.owner.rows.remove(at);
                    live.known.remove(&id);
                } else {
                    later.push(id);
                }
            }
        }
        for id in later {
            if live.dirty_set.insert(id) {
                live.dirty.push_back(id);
            }
        }
        self.changed = true;
    }

    /// Lists the other characters' files on this world.
    fn list(&mut self, host: &mut dyn MagHost) {
        self.relist = false;
        self.listed = true;
        self.reading = None;
        self.files.clear();
        self.found.clear();
        let (world, own_key) = match &self.live {
            Some(live) => (live.world.clone(), live.owner.key.clone()),
            None => match host.character() {
                Some(character) => (character.world, String::new()),
                None => return,
            },
        };
        let any = storage_key(&world, "_");
        let folder = &any[..=any.rfind('/').unwrap_or(0)];
        let keys = match host.storage().list(folder) {
            Ok(keys) => keys,
            Err(error) => {
                host.log(
                    LogLevel::Warn,
                    &format!("The inventory logs in {folder} could not be listed: {error}"),
                );
                return;
            }
        };
        for key in keys {
            let name = key.get(folder.len()..).unwrap_or_default();
            let is_log = ordinal::starts_with_ignore_case(&key, folder)
                && !name.contains('/')
                && ordinal::ends_with_ignore_case(name, LOG_SUFFIX);
            if is_log && !ordinal::equals_ignore_case(&key, &own_key) {
                self.found.push(key.clone());
                self.files.push_back(key);
            }
        }
        if self.files.is_empty() {
            self.finish_listing();
        }
    }

    /// A tick's worth of the other characters' files: one file's text read,
    /// or its items found, or a tick's worth of its items read.
    fn read_file(&mut self, host: &mut dyn MagHost, settings: InfoSettings) -> bool {
        match self.reading.take() {
            None => {
                let Some(key) = self.files.pop_front() else {
                    return false;
                };
                match host.storage().read_text(&key) {
                    Ok(Some(text)) => self.reading = Some(Reading::Text { key, text }),
                    Ok(None) => self.file_done(None),
                    Err(error) => {
                        host.log(
                            LogLevel::Warn,
                            &format!("The inventory log {key} could not be read: {error}"),
                        );
                        self.file_done(None);
                    }
                }
            }
            Some(Reading::Text { key, text }) => match LogReader::new(text) {
                Ok(reader) => {
                    let owner = Owner {
                        name: reader.header().character.clone(),
                        key,
                        rows: Vec::new(),
                    };
                    self.reading = Some(Reading::Items {
                        reader: Box::new(reader),
                        owner,
                    });
                }
                Err(error) => {
                    host.log(
                        LogLevel::Warn,
                        &format!("The inventory log {key} is not searched: {error:?}"),
                    );
                    self.file_done(None);
                }
            },
            Some(Reading::Items {
                mut reader,
                mut owner,
            }) => match reader.read_items(ITEMS_PER_TICK) {
                Ok(records) => {
                    let tests = Tests {
                        pattern: &self.pattern,
                        generation: self.generation,
                        filter: &self.filter,
                        filter_generation: self.filter_generation,
                        od: &self.od_rules,
                    };
                    for record in records {
                        let item = record.item();
                        let row = new_row(
                            host,
                            &mut self.spells,
                            &item,
                            record.properties(),
                            record.has_id_data,
                            settings,
                            &tests,
                        );
                        owner.rows.push(row);
                    }
                    if reader.remaining() == 0 {
                        self.file_done(Some(owner));
                    } else {
                        self.reading = Some(Reading::Items { reader, owner });
                    }
                }
                Err(error) => {
                    host.log(
                        LogLevel::Warn,
                        &format!("The inventory log {} is not searched: {error:?}", owner.key),
                    );
                    self.file_done(None);
                }
            },
        }
        true
    }

    /// A file read (or not): its rows take the place of the ones it gave
    /// before; the last one read, the characters whose files are gone are
    /// dropped.
    fn file_done(&mut self, owner: Option<Owner>) {
        if let Some(owner) = owner {
            match self.others.iter().position(|o| o.key == owner.key) {
                Some(at) => self.others[at] = owner,
                None => {
                    self.others.push(owner);
                    self.others
                        .sort_by(|a, b| ordinal::compare_ignore_case(&a.key, &b.key));
                }
            }
            self.changed = true;
        }
        if self.files.is_empty() && self.reading.is_none() {
            self.finish_listing();
        }
    }

    fn finish_listing(&mut self) {
        let found = std::mem::take(&mut self.found);
        let before = self.others.len();
        self.others.retain(|owner| found.contains(&owner.key));
        if self.others.len() != before {
            self.changed = true;
        }
    }

    /// Works the shown rows out again when asked to.
    fn refresh_view(&mut self) {
        if !self.view_stale {
            return;
        }
        self.view_stale = false;
        self.changed = false;
        self.revision += 1;
        // The class chosen: none for every class (a name no class has
        // keeps nothing).
        let class = self.class_filter();
        let class = (class != ALL).then(|| {
            ObjectClass::ALL
                .iter()
                .copied()
                .find(|kind| class_name(*kind) == class)
        });
        let characters = self.filter.characters.as_ref();
        let mut view = View {
            selected: -1,
            ..View::default()
        };
        let mut targets = Targets::default();
        let mut classes: Vec<ObjectClass> = Vec::new();
        let mut sets: HashMap<i32, usize> = HashMap::new();
        for (index, owner) in self.owners().enumerate() {
            view.character_choices.push(owner.name.clone());
            targets.owner_keys.push(owner.key.clone());
            let owner_shown = characters.is_none_or(|names| names.contains(&owner.name));
            let packs = owner.packs();
            for row in &owner.rows {
                if !classes.contains(&row.class) {
                    classes.push(row.class);
                }
                if !owner_shown {
                    continue;
                }
                if let Some(set) = row.facts.set_id {
                    *sets.entry(set).or_default() += 1;
                }
                let found = row.tested == self.generation
                    && row.matched
                    && class.is_none_or(|class| class == Some(row.class))
                    && row.filter_tested == self.filter_generation;
                if found && row.verdict == Verdict::NotIdentified {
                    view.not_identified += 1;
                }
                if !found || row.verdict != Verdict::Pass {
                    continue;
                }
                if self
                    .clicked
                    .as_ref()
                    .is_some_and(|(key, id)| *id == row.id && *key == owner.key)
                {
                    view.selected = i32::try_from(targets.entries.len()).unwrap_or(-1);
                }
                targets.entries.push((index, row.id));
                view.characters.push(owner.name.clone());
                view.names.push(row.name.clone());
                view.counts.push(row.count.max(1).to_string());
                view.places.push(place(row, &packs));
                view.icons.push(row.icon);
                let facts = &row.facts;
                view.slots.push(facts.slot.clone());
                view.wields.push(facts.wield_text());
                view.ods.push(facts.od_text());
                view.values.push(facts.value_text());
                view.wield_keys.push(sort_key(facts.wield_level));
                view.od_keys.push(sort_key(facts.od));
                view.value_keys.push(sort_key(facts.value));
            }
        }
        // Every set of the set table, at 0 when nobody searched holds it,
        // and any other set held.
        for (id, _) in names::ATTRIBUTE_SET_INFO {
            sets.entry(*id).or_default();
        }
        let mut set_counts: Vec<SetCount> = sets
            .into_iter()
            .map(|(id, count)| SetCount {
                id,
                name: set_name(id),
                count,
            })
            .collect();
        set_counts.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| ordinal::compare_ignore_case(&a.name, &b.name))
        });
        view.set_counts = set_counts;
        let mut class_names: Vec<String> = classes.into_iter().map(class_name).collect();
        class_names.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        view.class_choices.push(ALL.to_owned());
        view.class_choices.extend(class_names);
        // A clicked item the list no longer shows (the pattern or a filter
        // left it out, the list left empty) shows no text: the click is
        // forgotten. Not while the rows are still being read or matched,
        // when it is missing only for now.
        if view.selected < 0 && self.work_done() {
            self.clicked = None;
            self.info.clear();
        }
        view.targets = Rc::new(targets);
        self.view = view;
    }
}

/// A filter's choices: [`ALL`] alone before anything is read.
fn choices(choices: &[String]) -> Vec<String> {
    if choices.is_empty() {
        vec![ALL.to_owned()]
    } else {
        choices.to_vec()
    }
}

/// A number column's sort key: the number in plain digits, blank for none.
fn sort_key(value: Option<i32>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

/// An object class's name, as Decal spelled it.
fn class_name(class: ObjectClass) -> String {
    format!("{class:?}")
}

/// Matches a row against the pattern of `generation`.
fn test(row: &mut Row, pattern: &Pattern, generation: u64) {
    let matched = pattern.is_match(&row.info);
    row.matched = matched == Some(true);
    row.too_costly = matched.is_none();
    row.tested = generation;
}

/// Tests a row against the filter of `generation`.
fn filter_row(row: &mut Row, filter: &InventoryFilter, generation: u64) {
    row.verdict = filter.verdict(&row.facts);
    row.filter_tested = generation;
}

/// What a row read now is tested against.
struct Tests<'a> {
    pattern: &'a Pattern,
    generation: u64,
    filter: &'a InventoryFilter,
    filter_generation: u64,
    /// The loaded loot profile's OD rules.
    od: &'a OdRules,
}

impl Tests<'_> {
    fn run(&self, row: &mut Row) {
        test(row, self.pattern, self.generation);
        filter_row(row, self.filter, self.filter_generation);
    }
}

/// A row made now: its text, its facts (`identified`: it was appraised;
/// only then has it an OD), tested.
fn new_row(
    host: &mut dyn MagHost,
    spells: &mut SpellCache,
    item: &InventoryItem,
    properties: &ItemProperties,
    identified: bool,
    settings: InfoSettings,
    tests: &Tests<'_>,
) -> Row {
    let info = info_text(host, spells, item, properties, settings);
    let names = spells.names(host, item);
    let mut facts = RowFacts::new(item, properties, identified, names);
    if identified {
        // A cantrip-gated step reads the spells' names from the spell table.
        let mut spell = |id: u32| spells.get(host, id).map(|spell| spell.info.clone());
        facts.od = tests.od.od(item, properties, &mut spell);
    }
    let mut row = Row::new(item, properties, info, facts);
    tests.run(&mut row);
    row
}

/// A live item's row, its properties read now.
fn live_row(
    host: &mut dyn MagHost,
    spells: &mut SpellCache,
    item: &InventoryItem,
    settings: InfoSettings,
    tests: &Tests<'_>,
) -> Row {
    let properties = host.item_properties(item.object_id).unwrap_or_default();
    let identified = host
        .object(item.object_id)
        .is_some_and(|object| object.has_appraisal_data);
    new_row(host, spells, item, &properties, identified, settings, tests)
}

/// The item's info text, as the item info line prints it.
fn info_text(
    host: &mut dyn MagHost,
    spells: &mut SpellCache,
    item: &InventoryItem,
    properties: &ItemProperties,
    settings: InfoSettings,
) -> String {
    let input = ItemInfoInput {
        item,
        properties,
        show_buffed_values: settings.show_buffed_values,
        show_value_and_burden: settings.show_value_and_burden,
    };
    item_info(&input, &mut |id| {
        spells.get(host, id).map(|spell| spell.info.clone())
    })
}
