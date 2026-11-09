//! The mana list: every item the character wears, its mana, how long that
//! lasts and whether its spells are up, from Mag-Tools'
//! `Trackers/Equipment/EquipmentTracker.cs`, `EquipmentTrackedItem.cs` and
//! `Views/ManaTrackerGUI.cs` (43ee2ec).
//!
//! - **Tracked:** every owned item worn or wielded by the character
//!   (`equipped_location != 0`, `wielder_id` the character): all of them at
//!   the login, one put on (a placement, or created already worn) as it
//!   comes; one taken off or released leaves. Aetheria, cloaks and
//!   ammunition have no row but count in the totals.
//! - **State:** an item with no appraisal is Unknown; one with no spells or
//!   no mana at full is not activatable; one with no mana is not active.
//!   Otherwise every spell it carries (not the spell it casts when used,
//!   not a cast-on-strike one) must be up: a spell of the same family at
//!   least as strong, either on the item itself (its appraisal's spells with
//!   the high bit) or cast by an item on the character (an enchantment that
//!   does not run out, or has run out: every enchantment, surpassed ones
//!   too). The state is worked out when the item's appraisal arrives and
//!   kept until the next one (before the first, at every read).
//! - **Mana:** the appraisal's mana, less one for every burn period since
//!   it arrived (the period is the item's mana rate on a 5 s grid); the
//!   time left is that times the period.
//! - **Appraisals** are asked for when an item is tracked and when a system
//!   line names it: `Your … is low on Mana.`, `… is out of Mana.`, `The
//!   Mana Stone gives …`.
//!
//! Changes: only system lines are read (F12; Mag-Tools skipped lines that
//! began `You say, ` or held `says, "`, which an OpenAC line's words never
//! carry, so another player's words could trigger it). Mag-Tools' "Recharge"
//! is not ported (MossTank's own mana recharger keeps the job). Mag-Tools
//! also restarted the burn clock on Decal's mana-change notice; the client
//! sends plugins no such notice, so only an appraisal does. The rows are
//! worked out when the page reads them, not on a burn timer per item: the
//! numbers only move on the burn grid, so they read the same.

use std::time::Duration;

use ac_plugin_abi::ChatLine;
use ac_plugin_abi::items::{EquipmentObservation, InventoryItem};
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind};

use crate::host::MagHost;
use crate::identify::IdentifyQueue;

/// The state icon of an item whose spells are up (a green circle).
pub const ICON_ACTIVE: u32 = 0x0600_11F9;
/// The state icon of an item whose spells are not all up (a red circle).
pub const ICON_NOT_ACTIVE: u32 = 0x0600_11F8;
/// The state icon of an item not appraised yet (a circle).
pub const ICON_UNKNOWN: u32 = 0x0600_20B5;
/// The state icon of an item that casts nothing (a small grey dot).
pub const ICON_NONE: u32 = 0x0600_287A;
/// A cloak's valid locations: no row.
pub const CLOAK_SLOTS: u32 = 0x0800_0000;
/// The ammunition slot: no row, and never counted as unretained.
pub const AMMO_SLOT: u32 = 0x0080_0000;
/// The time left of an active item that does not burn: `new TimeSpan(99,
/// 99, 0)`.
pub const NO_BURN: Duration = Duration::from_secs(99 * 3600 + 99 * 60);

/// A spellbook entry with this bit is a spell active on the item.
const ACTIVE_ON_ITEM: u32 = 0x8000_0000;
/// A system line's kind.
const SYSTEM_LINE: u32 = 4;
/// The mana rate's float property (`ManaRate`, mana a second, negative).
const PROPERTY_MANA_RATE: u32 = 5;
/// The sort key of a row with no mana shown: `int.MaxValue`.
const NO_MANA_SORT_KEY: f64 = 2_147_483_647.0;

/// Whether an item's spells are up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManaState {
    /// Not appraised (or not known to the client).
    #[default]
    Unknown,
    /// No spells, or no mana at full.
    NotActivatable,
    /// Every spell it carries is up.
    Active,
    /// A spell is not up, or it has no mana.
    NotActive,
}

impl ManaState {
    /// The icon its row shows.
    #[must_use]
    pub fn icon(self) -> u32 {
        match self {
            Self::Active => ICON_ACTIVE,
            Self::NotActive => ICON_NOT_ACTIVE,
            Self::Unknown => ICON_UNKNOWN,
            Self::NotActivatable => ICON_NONE,
        }
    }

    /// Its row shows mana and time.
    fn shows_mana(self) -> bool {
        matches!(self, Self::Active | Self::NotActive)
    }
}

/// Seconds an item takes to burn one mana at `rate` (mana a second,
/// negative): `(int)ceil(-0.2 / rate) * 5`, the server burning on a 5 s
/// grid; 0 for an item that does not burn. A positive rate is not guarded
/// (Mag-Tools did not).
#[must_use]
pub fn seconds_per_burn(rate: f64) -> i32 {
    if rate == 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let period = (-0.2 / rate).ceil() as i32;
    period.wrapping_mul(5)
}

/// The time-left text: `3h07m` (`"{0:d}h{1:d2}m"` of the whole hours and
/// the minutes).
#[must_use]
pub fn time_text(time: Duration) -> String {
    time_text_seconds(i64::try_from(time.as_secs()).unwrap_or(i64::MAX))
}

/// [`time_text`] of a signed number of seconds (a positive mana rate gives
/// a negative time).
fn time_text_seconds(total: i64) -> String {
    let hours = total / 3600;
    let minutes = (total / 60) % 60;
    if minutes < 0 {
        format!("{hours}h-{:02}m", -minutes)
    } else {
        format!("{hours}h{minutes:02}m")
    }
}

/// `Mana needed: N`.
#[must_use]
pub fn mana_needed_text(mana: i32) -> String {
    format!("Mana needed: {mana}")
}

/// `Unretained Items: N`.
#[must_use]
pub fn unretained_text(count: usize) -> String {
    format!("Unretained Items: {count}")
}

/// What the client says about a tracked item, read when it changes.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    /// The client knows the object.
    exists: bool,
    /// It is in the owned list (the placement fields below are read).
    listed: bool,
    name: String,
    wielder_id: u32,
    valid_locations: u32,
    equipped_location: u32,
    /// An appraisal arrived.
    has_id_data: bool,
    /// The spells it carries (the appraisal's entries without the high
    /// bit).
    spells: Vec<u32>,
    /// The spells active on it (the entries with the high bit, without it).
    active_on_item: Vec<u32>,
    /// The spell it casts when used, 0 for none.
    associated_spell: u32,
    current_mana: i32,
    maximum_mana: i32,
    /// Mana a second, negative; 0 when not known.
    mana_rate: f64,
    retained: bool,
}

impl Snapshot {
    fn read(host: &mut dyn MagHost, object_id: u32) -> Self {
        let Some(object) = host.object(object_id) else {
            return Self::default();
        };
        let owned = host.owned_items_shared();
        let item = owned.iter().find(|item| item.object_id == object_id);
        let mut snapshot = Self {
            exists: true,
            listed: item.is_some(),
            name: object.name,
            wielder_id: object.wielder_id,
            has_id_data: object.has_appraisal_data,
            ..Self::default()
        };
        for &spell in &object.spell_ids {
            if spell & ACTIVE_ON_ITEM == 0 {
                snapshot.spells.push(spell);
            } else {
                snapshot.active_on_item.push(spell & !ACTIVE_ON_ITEM);
            }
        }
        if let Some(item) = item {
            snapshot.wielder_id = item.wielder_id;
            snapshot.valid_locations = item.valid_locations;
            snapshot.equipped_location = item.equipped_location;
            snapshot.associated_spell = item.spell_id;
            snapshot.current_mana = item.item_current_mana;
            snapshot.maximum_mana = item.item_maximum_mana;
            snapshot.retained = item.retained;
        }
        drop(owned);
        snapshot.mana_rate = host
            .item_properties(object_id)
            .and_then(|properties| {
                properties
                    .floats
                    .iter()
                    .find(|(id, _)| *id == PROPERTY_MANA_RATE)
                    .map(|(_, rate)| *rate)
            })
            .unwrap_or(0.0);
        snapshot
    }

    /// Aetheria, a cloak or ammunition: no row.
    fn hidden(&self) -> bool {
        self.name.contains("Aetheria")
            || self.valid_locations == CLOAK_SLOTS
            || self.equipped_location == AMMO_SLOT
    }
}

/// The tracking rule: worn or wielded by the character.
fn worn_by(item: &InventoryItem, me: u32) -> bool {
    item.equipped_location != 0 && item.wielder_id == me
}

/// The spells items cast on the character (an enchantment with no end, or
/// one that ended), as family and difficulty: read and looked up once for
/// every state worked out in one reading of the list.
#[derive(Debug, Default)]
struct ItemCasts(Option<Vec<(u32, i32)>>);

impl ItemCasts {
    fn get(&mut self, host: &mut dyn MagHost) -> &[(u32, i32)] {
        self.0.get_or_insert_with(|| {
            host.all_enchantments()
                .into_iter()
                .filter(|e| e.seconds_remaining <= 0.0 || e.seconds_remaining.is_infinite())
                .filter_map(|e| host.spell(e.spell_id))
                .map(|spell| (spell.family, spell.difficulty))
                .collect()
        })
    }
}

/// The state worked out afresh; `None` when a spell is not in the spell
/// table (Mag-Tools threw there, and the row kept its state).
fn work_out_state(
    snapshot: &Snapshot,
    host: &mut dyn MagHost,
    casts: &mut ItemCasts,
) -> Option<ManaState> {
    if !snapshot.has_id_data {
        return Some(ManaState::Unknown);
    }
    if snapshot.spells.is_empty() || snapshot.maximum_mana == 0 {
        return Some(ManaState::NotActivatable);
    }
    if snapshot.current_mana == 0 {
        return Some(ManaState::NotActive);
    }
    casts.get(host);
    for &spell_id in &snapshot.spells {
        if snapshot.associated_spell != 0 && snapshot.associated_spell == spell_id {
            continue;
        }
        let spell = host.spell(spell_id)?;
        if spell.is_debuff || spell.is_offensive {
            continue;
        }
        let mut active = false;
        for &on_item in &snapshot.active_on_item {
            let other = host.spell(on_item)?;
            if other.family == spell.family && other.difficulty >= spell.difficulty {
                active = true;
                break;
            }
        }
        if !active {
            active = casts.get(host).iter().any(|&(family, difficulty)| {
                family == spell.family && difficulty >= spell.difficulty
            });
        }
        if !active {
            return Some(ManaState::NotActive);
        }
    }
    Some(ManaState::Active)
}

/// One tracked item.
#[derive(Debug)]
struct Tracked {
    object_id: u32,
    snapshot: Snapshot,
    /// No row (decided when it was tracked).
    hidden: bool,
    /// When its last appraisal arrived (the plugin's clock, ms); `None`
    /// before the first since it was tracked.
    last_ident_ms: Option<f64>,
    /// How many appraisals arrived since it was tracked.
    idents: u64,
    /// The appraisal count the kept state was worked out for.
    worked_out_for: u64,
    state: ManaState,
}

impl Tracked {
    fn new(object_id: u32, snapshot: Snapshot) -> Self {
        Self {
            object_id,
            hidden: snapshot.hidden(),
            snapshot,
            last_ident_ms: None,
            idents: 0,
            worked_out_for: 0,
            state: ManaState::Unknown,
        }
    }

    /// The state: the kept one, worked out again when an appraisal arrived
    /// since, or while none has.
    fn state(&mut self, host: &mut dyn MagHost, casts: &mut ItemCasts) -> ManaState {
        if !self.snapshot.exists {
            return ManaState::Unknown;
        }
        if self.idents == 0 || self.worked_out_for != self.idents {
            self.worked_out_for = self.idents;
            if let Some(state) = work_out_state(&self.snapshot, host, casts) {
                self.state = state;
            }
        }
        self.state
    }

    fn seconds_per_burn(&self) -> i32 {
        if !self.snapshot.exists || !self.snapshot.has_id_data {
            return 0;
        }
        seconds_per_burn(self.snapshot.mana_rate)
    }

    /// The mana now: the appraisal's, less what burned since.
    fn calculated_mana(&self, state: ManaState, now_ms: f64) -> i32 {
        if !state.shows_mana() {
            return 0;
        }
        let current = self.snapshot.current_mana;
        let period = self.seconds_per_burn();
        if state == ManaState::NotActive || period == 0 {
            return current;
        }
        // Before the first appraisal Mag-Tools counted from the start of
        // time: everything burned.
        #[allow(clippy::cast_possible_truncation)]
        let burned = self.last_ident_ms.map_or(i32::MAX, |ident| {
            ((now_ms - ident) / 1000.0 / f64::from(period)) as i32
        });
        current.wrapping_sub(burned.min(current)).max(0)
    }

    /// Seconds its mana keeps it up; 0 unless it is active.
    fn time_left(&self, state: ManaState, now_ms: f64) -> i64 {
        if state != ManaState::Active {
            return 0;
        }
        let period = self.seconds_per_burn();
        if period == 0 {
            return i64::try_from(NO_BURN.as_secs()).unwrap_or(i64::MAX);
        }
        i64::from(self.calculated_mana(state, now_ms)) * i64::from(period)
    }

    fn mana_needed(&mut self, host: &mut dyn MagHost, casts: &mut ItemCasts, now_ms: f64) -> i32 {
        let state = self.state(host, casts);
        let maximum = if self.snapshot.exists {
            self.snapshot.maximum_mana
        } else {
            0
        };
        maximum
            .wrapping_sub(self.calculated_mana(state, now_ms))
            .max(0)
    }
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct ManaRow {
    /// The item (its icon is drawn from it).
    pub object_id: u32,
    /// Its name.
    pub name: String,
    /// Its state (the icon: [`ManaState::icon`]).
    pub state: ManaState,
    /// `mana / at full`, or `-`.
    pub mana_text: String,
    /// The time left, or `-`.
    pub time_text: String,
    /// The order: seconds left (0 for not active), `int.MaxValue` for no
    /// mana shown.
    pub sort_key: f64,
}

/// The page's values, worked out once a moment.
#[derive(Debug, Clone, Default)]
pub struct ManaView {
    /// The rows, in order.
    pub rows: Vec<ManaRow>,
    /// The mana every tracked item lacks.
    pub mana_needed: i32,
    /// The tracked items not retained.
    pub unretained: usize,
}

impl ManaView {
    /// The items, one a row (the icon column).
    #[must_use]
    pub fn object_ids(&self) -> Vec<u32> {
        self.rows.iter().map(|row| row.object_id).collect()
    }

    /// The names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.name.clone()).collect()
    }

    /// The state icons.
    #[must_use]
    pub fn state_icons(&self) -> Vec<u32> {
        self.rows.iter().map(|row| row.state.icon()).collect()
    }

    /// The mana texts.
    #[must_use]
    pub fn mana_texts(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.mana_text.clone()).collect()
    }

    /// The time texts.
    #[must_use]
    pub fn time_texts(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.time_text.clone()).collect()
    }

    /// `Mana needed: N`.
    #[must_use]
    pub fn mana_needed_text(&self) -> String {
        mana_needed_text(self.mana_needed)
    }

    /// `Unretained Items: N`.
    #[must_use]
    pub fn unretained_text(&self) -> String {
        unretained_text(self.unretained)
    }
}

/// The tracked items.
#[derive(Debug, Default)]
pub struct ManaTracker {
    /// In the order they were tracked.
    items: Vec<Tracked>,
    /// Items put on, looked at on the next tick.
    placed: Vec<u32>,
    view: ManaView,
    /// The clock the view was built at.
    built_ms: Option<f64>,
    /// Something changed since it was built.
    dirty: bool,
}

impl ManaTracker {
    /// Whether an item is tracked.
    #[must_use]
    pub fn is_tracked(&self, object_id: u32) -> bool {
        self.items.iter().any(|item| item.object_id == object_id)
    }

    /// How many items are tracked (hidden ones too).
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Nothing is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Forgets everything (logging off, the session ended).
    pub fn reset(&mut self) {
        *self = Self::default();
        self.dirty = true;
    }

    /// The login completed: every worn item, afresh.
    pub fn on_login(&mut self, host: &mut dyn MagHost, queue: &mut IdentifyQueue) {
        self.reset();
        let Some(me) = host.character().map(|character| character.id) else {
            return;
        };
        let worn: Vec<u32> = host
            .owned_items_shared()
            .iter()
            .filter(|item| worn_by(item, me))
            .map(|item| item.object_id)
            .collect();
        let mut casts = ItemCasts::default();
        for object_id in worn {
            self.add(host, object_id, queue, &mut casts);
        }
    }

    /// An item went on (looked at on the next [`Self::tick`]) or came off.
    pub fn on_equipment(&mut self, observation: &EquipmentObservation) {
        if observation.is_removal {
            self.remove(observation.object_id);
        } else if !self.placed.contains(&observation.object_id) {
            self.placed.push(observation.object_id);
        }
    }

    /// Items put on are waiting for a [`Self::tick`].
    #[must_use]
    pub fn has_placements(&self) -> bool {
        !self.placed.is_empty()
    }

    /// One turn: the items put on are tracked when the character wears
    /// them.
    pub fn tick(&mut self, host: &mut dyn MagHost, queue: &mut IdentifyQueue) {
        for object_id in std::mem::take(&mut self.placed) {
            self.add_if_worn(host, object_id, queue);
        }
    }

    /// An object changed: a tracked one is read again (its appraisal noted
    /// and its state worked out), and leaves when the character no longer
    /// wears it or it is released; one the character wears now is tracked.
    pub fn on_object_changed(
        &mut self,
        host: &mut dyn MagHost,
        change: &ObjectChange,
        queue: &mut IdentifyQueue,
    ) {
        let object_id = change.object_id;
        if change.kind == ObjectChangeKind::Released {
            self.remove(object_id);
            return;
        }
        let Some(index) = self
            .items
            .iter()
            .position(|item| item.object_id == object_id)
        else {
            // Only an object the character wields is worth a look.
            let me = host.character().map(|character| character.id);
            if me.is_some_and(|me| change.current.as_ref().is_some_and(|o| o.wielder_id == me)) {
                self.add_if_worn(host, object_id, queue);
            }
            return;
        };
        let snapshot = Snapshot::read(host, object_id);
        let me = host.character().map_or(0, |character| character.id);
        if snapshot.listed && (snapshot.equipped_location == 0 || snapshot.wielder_id != me) {
            self.remove(object_id);
            return;
        }
        let now = host.now_ms();
        let item = &mut self.items[index];
        item.snapshot = snapshot;
        if change.kind == ObjectChangeKind::IdentReceived {
            item.last_ident_ms = Some(now);
            item.idents += 1;
            // Mag-Tools redrew the row at once, reading the state.
            item.state(host, &mut ItemCasts::default());
        }
        self.dirty = true;
    }

    /// A chat line: a system line naming a tracked item's mana asks for its
    /// appraisal (F12: only system lines).
    pub fn on_chat(&self, line: &ChatLine, queue: &mut IdentifyQueue) {
        if line.kind != SYSTEM_LINE {
            return;
        }
        let text = line.text.as_str();
        let stone = text.contains("The Mana Stone gives ");
        let low = text.contains("Your ")
            && (text.contains(" is low on Mana.") || text.contains(" is out of Mana."));
        if !stone && !low {
            return;
        }
        for item in &self.items {
            if item.snapshot.exists && text.contains(item.snapshot.name.as_str()) {
                queue.push(item.object_id);
            }
        }
    }

    /// The rows at `now_ms`, in order.
    pub fn rows(&mut self, host: &mut dyn MagHost, now_ms: f64) -> Vec<ManaRow> {
        self.rows_with(host, &mut ItemCasts::default(), now_ms)
    }

    fn rows_with(
        &mut self,
        host: &mut dyn MagHost,
        casts: &mut ItemCasts,
        now_ms: f64,
    ) -> Vec<ManaRow> {
        let mut rows = Vec::with_capacity(self.items.len());
        for item in &mut self.items {
            if item.hidden {
                continue;
            }
            let state = item.state(host, casts);
            let row = if state.shows_mana() {
                let left = item.time_left(state, now_ms);
                ManaRow {
                    object_id: item.object_id,
                    name: item.snapshot.name.clone(),
                    state,
                    mana_text: format!(
                        "{} / {}",
                        item.calculated_mana(state, now_ms),
                        item.snapshot.maximum_mana
                    ),
                    time_text: time_text_seconds(left),
                    #[allow(clippy::cast_precision_loss)]
                    sort_key: left as f64,
                }
            } else {
                ManaRow {
                    object_id: item.object_id,
                    name: item.snapshot.name.clone(),
                    state,
                    mana_text: "-".to_owned(),
                    time_text: "-".to_owned(),
                    sort_key: NO_MANA_SORT_KEY,
                }
            };
            rows.push(row);
        }
        rows.sort_by(|a, b| a.sort_key.total_cmp(&b.sort_key));
        rows
    }

    /// The mana every tracked item lacks (hidden ones too).
    pub fn mana_needed(&mut self, host: &mut dyn MagHost, now_ms: f64) -> i32 {
        self.mana_needed_with(host, &mut ItemCasts::default(), now_ms)
    }

    fn mana_needed_with(
        &mut self,
        host: &mut dyn MagHost,
        casts: &mut ItemCasts,
        now_ms: f64,
    ) -> i32 {
        self.items.iter_mut().fold(0, |sum, item| {
            sum.wrapping_add(item.mana_needed(host, casts, now_ms))
        })
    }

    /// The appraised tracked items not retained (ammunition not counted).
    #[must_use]
    pub fn unretained(&self) -> usize {
        self.items
            .iter()
            .filter(|item| {
                let snapshot = &item.snapshot;
                snapshot.exists
                    && snapshot.equipped_location != AMMO_SLOT
                    && snapshot.has_id_data
                    && !snapshot.retained
            })
            .count()
    }

    /// Builds the view at the client's clock, unless it was built at this
    /// moment and nothing changed since.
    pub fn refresh(&mut self, host: &mut dyn MagHost) {
        let now = host.now_ms();
        if !self.dirty && self.built_ms == Some(now) {
            return;
        }
        let mut casts = ItemCasts::default();
        let rows = self.rows_with(host, &mut casts, now);
        let mana_needed = self.mana_needed_with(host, &mut casts, now);
        self.view = ManaView {
            rows,
            mana_needed,
            unretained: self.unretained(),
        };
        self.built_ms = Some(now);
        self.dirty = false;
    }

    /// The view last built ([`Self::refresh`]).
    #[must_use]
    pub fn view(&self) -> &ManaView {
        &self.view
    }

    fn add_if_worn(&mut self, host: &mut dyn MagHost, object_id: u32, queue: &mut IdentifyQueue) {
        if self.is_tracked(object_id) {
            return;
        }
        let Some(me) = host.character().map(|character| character.id) else {
            return;
        };
        let worn = host
            .owned_items_shared()
            .iter()
            .any(|item| item.object_id == object_id && worn_by(item, me));
        if worn {
            self.add(host, object_id, queue, &mut ItemCasts::default());
        }
    }

    /// Tracks an item (once) and asks for its appraisal.
    fn add(
        &mut self,
        host: &mut dyn MagHost,
        object_id: u32,
        queue: &mut IdentifyQueue,
        casts: &mut ItemCasts,
    ) {
        if self.is_tracked(object_id) {
            return;
        }
        let snapshot = Snapshot::read(host, object_id);
        if !snapshot.exists {
            return;
        }
        queue.push(object_id);
        let mut item = Tracked::new(object_id, snapshot);
        item.state(host, casts);
        self.items.push(item);
        self.dirty = true;
    }

    fn remove(&mut self, object_id: u32) {
        let before = self.items.len();
        self.items.retain(|item| item.object_id != object_id);
        self.placed.retain(|&id| id != object_id);
        if self.items.len() != before {
            self.dirty = true;
        }
    }
}
