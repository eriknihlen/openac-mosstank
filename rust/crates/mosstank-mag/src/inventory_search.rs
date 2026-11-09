//! The Inventory window's search ([`InventorySearch`]): a port of Mag-Tools'
//! Tools, Inventory page (`InventoryToolsView`), widened to every character
//! the inventory logger saved on this server.
//!
//! - The pattern is a .NET regular expression, case ignored, matched
//!   against each item's info text ([`crate::item_info::item_info`], with
//!   the `ShowBuffedValues` and `ShowValueAndBurden` settings): empty
//!   matches everything, a pattern that does not compile lists nothing and
//!   says [`BAD_PATTERN`]. It is compiled once each time its text changes.
//! - The rows: the logged-in character's items, live (its own saved file is
//!   not read), then every other `Mag-Tools/<world>/*.inventory.json` in the
//!   plugin's data, read again each time the window is opened.
//! - Clicking a row of the logged-in character opens the side pack holding
//!   it, selects it and shows its text; another character's row shows the
//!   text its file gave.
//!
//! Nothing is done while the window is hidden. The work is staged so no
//! call does more than a tick's worth: a file is read in one tick, its items
//! found in the next, then [`ITEMS_PER_TICK`] items a tick are read and
//! their texts made (the live items the same); each text is made once and
//! kept. A new pattern is matched against [`NEW_PATTERN_MATCHES`] texts in
//! the call that typed it (the engine builds its tables then), then
//! [`MATCHES_PER_CALL`] a tick; a pattern that [`backtracks`] against
//! [`BACKTRACKING_MATCHES_PER_CALL`] a call, each text stopped after
//! [`STEP_LIMIT`] steps. The status says how far it is. The rows shown are
//! worked out again at once for what the player does (a pattern, a filter),
//! and otherwise only once the reading and matching are done, so thousands
//! of rows are not pushed to the window at every tick of the work.
//!
//! Where it differs from Mag-Tools: the rows of other characters, the
//! Character and Object class filters, and the Count and Where columns are
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
use ac_plugin_abi::world::{ItemProperties, ObjectChange, ObjectClass};
use mosstank_dotnet::text::ordinal;
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use crate::host::MagHost;
use crate::inventory_log::{LogReader, storage_key};
use crate::item_info::{ItemInfoInput, item_info};

/// The filters' choice that keeps every row.
pub const ALL: &str = "All";

/// The status shown for a pattern that does not compile.
pub const BAD_PATTERN: &str = "bad pattern";

/// Items read (from a file, or live) and given their text a tick.
pub const ITEMS_PER_TICK: usize = 50;

/// Item texts matched against the pattern in one call (a tenth of it on a
/// tick that also read items).
pub const MATCHES_PER_CALL: usize = 1500;

/// Item texts matched in the call that typed a new pattern: its first
/// matches build the engine's tables, several times the cost of later ones.
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

/// The file name every inventory log ends with.
const LOG_SUFFIX: &str = ".inventory.json";

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
}

impl Pattern {
    fn compile(text: &str) -> Self {
        if text.is_empty() {
            return Self::All;
        }
        match DotNetRegex::with_step_limit(text, RegexOptions::IGNORE_CASE, STEP_LIMIT) {
            Ok(regex) => Self::Regex(Box::new(regex), backtracks(text)),
            Err(_) => Self::Bad,
        }
    }

    /// Whether `text` matches; `None` when it ran past the step limit.
    fn is_match(&self, text: &str) -> Option<bool> {
        match self {
            Self::All => Some(true),
            Self::Regex(regex, _) => regex.is_match(text).ok(),
            Self::Bad => Some(false),
        }
    }

    fn is_regex(&self) -> bool {
        matches!(self, Self::Regex(..))
    }

    /// The texts matched in one call: `first`, the call that typed it.
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
}

impl Row {
    fn new(item: &InventoryItem, info: String) -> Self {
        Self {
            id: item.object_id,
            name: item.name.clone(),
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
    character_choices: Vec<String>,
    class_choices: Vec<String>,
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
    character_filter: String,
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

    /// A new pattern typed: compiled (when its text changed), and matched
    /// against the first [`MATCHES_PER_CALL`] item texts (or
    /// [`BACKTRACKING_MATCHES_PER_CALL`]); the rest are matched on the next
    /// ticks.
    pub fn set_pattern(&mut self, text: &str) {
        if self.generation != 0 && text == self.pattern_text {
            return;
        }
        text.clone_into(&mut self.pattern_text);
        self.pattern = Pattern::compile(text);
        self.generation += 1;
        self.match_some(self.pattern.matches_per_call(true));
        self.view_stale = true;
        self.refresh_view();
    }

    /// The Character filter's choice.
    #[must_use]
    pub fn character_filter(&self) -> &str {
        if self.character_filter.is_empty() {
            ALL
        } else {
            &self.character_filter
        }
    }

    /// Only this character's rows ([`ALL`]: every one).
    pub fn set_character_filter(&mut self, choice: &str) {
        if choice != self.character_filter() {
            choice.clone_into(&mut self.character_filter);
            self.view_stale = true;
            self.refresh_view();
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

    /// The character left the world: every row is forgotten; the pattern
    /// and the filters stay.
    pub fn reset(&mut self) {
        let pattern_text = std::mem::take(&mut self.pattern_text);
        let pattern = std::mem::take(&mut self.pattern);
        let generation = self.generation;
        let character_filter = std::mem::take(&mut self.character_filter);
        let class_filter = std::mem::take(&mut self.class_filter);
        let revision = self.revision;
        *self = Self {
            pattern_text,
            pattern,
            generation,
            character_filter,
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
                self.live = None;
                self.others.clear();
                self.reading = None;
                self.files.clear();
                self.relist = true;
                self.changed = true;
            }
            self.settings = Some(settings);
        }
        let worked = self.read_some(host, settings);
        let budget = self.pattern.matches_per_call(false);
        let matched = self.match_some(if worked { budget / 10 } else { budget });
        // The rows shown are worked out on a tick of their own (thousands of
        // them are a tick's worth, and so is pushing them): the window's
        // push waits for the next light tick.
        let refreshed = self.changed && !worked && matched == 0 && self.work_done();
        if refreshed {
            self.view_stale = true;
            self.refresh_view();
        }
        worked || refreshed || matched >= budget / 10
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
                info = info_text(host, item, &properties, settings);
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

    /// The status line: [`BAD_PATTERN`], the work in hand, or how many
    /// items are shown.
    #[must_use]
    pub fn status(&self) -> String {
        if matches!(self.pattern, Pattern::Bad) {
            return BAD_PATTERN.to_owned();
        }
        let (untested, too_costly) = self.untested();
        let total = || self.owners().map(|owner| owner.rows.len()).sum::<usize>();
        if untested > 0 {
            let total = total();
            return format!("Searching... {} of {total}", total - untested);
        }
        if !self.is_settled() {
            return format!("Reading... {} items", total());
        }
        let shown = match self.view.targets.entries.len() {
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        };
        if too_costly == 0 {
            shown
        } else {
            format!("{shown} ({too_costly} too costly to search)")
        }
    }

    /// A number that changes whenever the rows shown, the filters' choices,
    /// the selection or the clicked item's text do.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The Character filter's choices: [`ALL`], the logged-in character,
    /// then the saved ones.
    #[must_use]
    pub fn characters(&self) -> Vec<String> {
        choices(&self.view.character_choices)
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
        if !self.pattern.is_regex() {
            return (0, 0);
        }
        let generation = self.generation;
        self.owners()
            .flat_map(|owner| &owner.rows)
            .fold((0, 0), |(untested, costly), row| {
                if row.tested != generation {
                    (untested + 1, costly)
                } else {
                    (untested, costly + usize::from(row.too_costly))
                }
            })
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
                let generation = self.generation;
                for _ in 0..ITEMS_PER_TICK {
                    let Some(item) = live.pending.pop_front() else {
                        break;
                    };
                    let row = live_row(host, &item, settings, &self.pattern, generation);
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
        let generation = self.generation;
        let mut complete = None;
        let mut later = Vec::new();
        for _ in 0..ITEMS_PER_TICK {
            let Some(id) = live.dirty.pop_front() else {
                break;
            };
            live.dirty_set.remove(&id);
            let at = live.owner.rows.iter().position(|row| row.id == id);
            if let Some(item) = owned.iter().find(|item| item.object_id == id) {
                let row = live_row(host, item, settings, &self.pattern, generation);
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
                    for record in records {
                        let item = record.item();
                        let info = info_text(host, &item, record.properties(), settings);
                        let mut row = Row::new(&item, info);
                        test(&mut row, &self.pattern, self.generation);
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
        let character = self.character_filter();
        let mut view = View {
            selected: -1,
            ..View::default()
        };
        let mut targets = Targets::default();
        view.character_choices.push(ALL.to_owned());
        let mut classes: Vec<ObjectClass> = Vec::new();
        for (index, owner) in self.owners().enumerate() {
            view.character_choices.push(owner.name.clone());
            targets.owner_keys.push(owner.key.clone());
            let owner_shown = character == ALL || owner.name == character;
            let packs = owner.packs();
            for row in &owner.rows {
                if !classes.contains(&row.class) {
                    classes.push(row.class);
                }
                let shown = owner_shown
                    && row.tested == self.generation
                    && row.matched
                    && class.is_none_or(|class| class == Some(row.class));
                if !shown {
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
            }
        }
        let mut class_names: Vec<String> = classes.into_iter().map(class_name).collect();
        class_names.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        view.class_choices.push(ALL.to_owned());
        view.class_choices.extend(class_names);
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

/// A live item's row, its properties read now.
fn live_row(
    host: &mut dyn MagHost,
    item: &InventoryItem,
    settings: InfoSettings,
    pattern: &Pattern,
    generation: u64,
) -> Row {
    let properties = host.item_properties(item.object_id).unwrap_or_default();
    let info = info_text(host, item, &properties, settings);
    let mut row = Row::new(item, info);
    test(&mut row, pattern, generation);
    row
}

/// The item's info text, as the item info line prints it.
fn info_text(
    host: &mut dyn MagHost,
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
    item_info(&input, &mut |spell| host.spell(spell))
}
