//! The name tags (`NametagController`, `NametagSettings`): a name hung over
//! the players, pets, portals, non-player characters, vendors and monsters
//! around the character, with a second line for a player's allegiance or a
//! portal's destination.
//!
//! The client draws the labels: it takes the whole set at once and draws
//! each at a constant screen size wherever its object goes. So this keeps
//! one tag per object, rebuilds the set every frame from those tags, and
//! hands it over only when it differs from the set last handed over.
//!
//! A tag's text is refreshed at most once a second, and an appraisal is
//! asked for on a backoff: only when the object is closer than when last
//! asked, at most every five seconds, and not past a try limit. Objects
//! dropped while the tags are walked are removed after the walk.

use std::collections::BTreeMap;

use ac_plugin_abi::ui::WorldLabel;
use ac_plugin_abi::world::{
    ItemProperties, ObjectChange, ObjectChangeKind, ObjectClass, Position, WorldObject,
};

use crate::catalog::UbCatalog;
use crate::host::UbHost;

/// How often one tag's text is refreshed from the object table.
pub const TEXT_UPDATE_INTERVAL_SECONDS: f64 = 1.0;
/// The least time between two appraisal requests for one object.
pub const ASSESS_RETRY_SECONDS: f64 = 5.0;
/// The count past which no further appraisal is asked for (one more than
/// this is sent before an object is given up on).
pub const ASSESS_ATTEMPT_LIMIT: i32 = 10;
/// How often the page's rows are re-read for a change.
pub const SETTINGS_POLL_SECONDS: f64 = 0.25;
/// The most labels the client shows for a plugin at once.
pub const MAXIMUM_LABELS: usize = 256;

const LEVEL_PROPERTY: u32 = 25;
const MONARCH_PROPERTY: u32 = 26;
const PET_OWNER_PROPERTY: u32 = 44;
const PORTAL_DESTINATION_PROPERTY: u32 = 38;
const ALLEGIANCE_NAME_PROPERTY: u32 = 47;

/// The kinds of thing a name is hung over, each with its own display group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NametagGroup {
    /// A player.
    Player,
    /// A monster with an owner.
    Pet,
    /// A player under the character's own monarch.
    AllegiancePlayer,
    /// A portal.
    Portal,
    /// A non-player character.
    Npc,
    /// A vendor.
    Vendor,
    /// A monster.
    Monster,
}

/// One display group's five rows (`NametagGroupSettings`). The sizes are
/// kept for the page; the label surface draws every label at one size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NametagGroupSettings {
    /// It draws.
    pub enabled: bool,
    /// The name's colour, ARGB.
    pub tag_color: u32,
    /// The name's size.
    pub tag_size: f32,
    /// The second line's colour, ARGB.
    pub ticker_color: u32,
    /// The second line's size.
    pub ticker_size: f32,
}

const fn group(enabled: bool, tag: u32, ticker: u32) -> NametagGroupSettings {
    NametagGroupSettings {
        enabled,
        tag_color: tag,
        tag_size: 0.15,
        ticker_color: ticker,
        ticker_size: 0.1,
    }
}

/// Everything the name tags read from the UB page, as one value
/// (`NametagSettings`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NametagSettings {
    /// The master switch.
    pub enabled: bool,
    /// How far off a tag shows, metres.
    pub max_range: f32,
    /// Players.
    pub player: NametagGroupSettings,
    /// Pets.
    pub pet: NametagGroupSettings,
    /// Players under the character's monarch.
    pub allegiance_player: NametagGroupSettings,
    /// Portals.
    pub portal: NametagGroupSettings,
    /// Non-player characters.
    pub npc: NametagGroupSettings,
    /// Vendors.
    pub vendor: NametagGroupSettings,
    /// Monsters.
    pub monster: NametagGroupSettings,
}

impl Default for NametagSettings {
    /// The catalogue's own defaults.
    fn default() -> Self {
        Self {
            enabled: true,
            max_range: 35.0,
            player: group(true, 0xFF00_FFFF, 0xFF00_FFFF),
            pet: group(true, 0xFF00_FFFF, 0xFF00_FFFF),
            allegiance_player: group(true, 0xFF00_FF00, 0xFF00_FF00),
            portal: group(true, 0xFF00_FF00, 0xFF00_FF00),
            npc: group(true, 0xFFFF_FF00, 0xFFFF_FF00),
            vendor: group(true, 0xFFFF_00FF, 0xFFFF_00FF),
            monster: group(true, 0xFFFF_0000, 0xFFFF_0000),
        }
    }
}

impl NametagSettings {
    /// The five rows for one group.
    #[must_use]
    pub fn of(&self, group: NametagGroup) -> NametagGroupSettings {
        match group {
            NametagGroup::Player => self.player,
            NametagGroup::Pet => self.pet,
            NametagGroup::AllegiancePlayer => self.allegiance_player,
            NametagGroup::Portal => self.portal,
            NametagGroup::Npc => self.npc,
            NametagGroup::Vendor => self.vendor,
            NametagGroup::Monster => self.monster,
        }
    }

    fn any_group_on(&self) -> bool {
        [
            self.player,
            self.pet,
            self.allegiance_player,
            self.portal,
            self.npc,
            self.vendor,
            self.monster,
        ]
        .iter()
        .any(|group| group.enabled)
    }
}

/// Where the tags' rows are read from.
pub trait NametagSettingsSource {
    /// The rows as they stand.
    fn read(&self) -> NametagSettings;
}

impl NametagSettingsSource for NametagSettings {
    fn read(&self) -> NametagSettings {
        *self
    }
}

/// The catalogue as the tags' settings.
pub struct CatalogNametagSettings<'a>(pub &'a dyn UbCatalog);

impl std::fmt::Debug for CatalogNametagSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogNametagSettings")
    }
}

impl NametagSettingsSource for CatalogNametagSettings<'_> {
    fn read(&self) -> NametagSettings {
        read_settings(self.0)
    }
}

/// The tags' rows from the UB settings catalogue (`ReadNametagSettings`).
#[must_use]
pub fn read_settings(catalog: &dyn UbCatalog) -> NametagSettings {
    let group = |name: &str| {
        let prefix = format!("Nametags.{name}.");
        NametagGroupSettings {
            enabled: catalog.flag(&format!("{prefix}Enabled")),
            tag_color: catalog.value(&format!("{prefix}TagColor")).as_color(),
            tag_size: catalog.value(&format!("{prefix}TagSize")).as_single(),
            ticker_color: catalog.value(&format!("{prefix}TickerColor")).as_color(),
            ticker_size: catalog.value(&format!("{prefix}TickerSize")).as_single(),
        }
    };
    NametagSettings {
        enabled: catalog.flag("Nametags.Enabled"),
        max_range: catalog.value("Nametags.MaxRange").as_single(),
        player: group("Player"),
        pet: group("Pet"),
        allegiance_player: group("AllegiancePlayer"),
        portal: group("Portal"),
        npc: group("Npc"),
        vendor: group("Vendor"),
        monster: group("Monster"),
    }
}

/// One object's tag: what it says and what it still waits to learn.
#[derive(Debug, Clone)]
struct Nametag {
    object_id: u32,
    group: NametagGroup,
    name: String,
    level: i32,
    ticker: String,
    show_ticker: bool,
    needs_process: bool,
    out_of_range: bool,
    range: f32,
    last_monarch: u32,
    last_text_update: f64,
    last_assess_range: f32,
    next_assess_at: f64,
    assess_count: i32,
}

impl Nametag {
    fn new(object_id: u32, group: NametagGroup, name: &str) -> Self {
        Self {
            object_id,
            group,
            name: name.to_owned(),
            level: 0,
            ticker: String::new(),
            show_ticker: false,
            needs_process: true,
            out_of_range: true,
            range: f32::MAX,
            last_monarch: u32::MAX,
            last_text_update: -1.0,
            last_assess_range: f32::MAX,
            next_assess_at: f64::MIN,
            assess_count: 0,
        }
    }

    fn text(&self) -> String {
        if self.level > 0 {
            format!("{} [{}]", self.name, self.level)
        } else {
            self.name.clone()
        }
    }
}

/// The name tags.
#[derive(Debug, Clone)]
pub struct NametagController {
    tags: BTreeMap<u32, Nametag>,
    destruction_queue: Vec<u32>,
    last_pushed: Vec<WorldLabel>,
    settings: NametagSettings,
    now: f64,
    settings_poll_remaining: f64,
    needs_scan: bool,
    walking: bool,
    push_count: usize,
}

impl NametagController {
    /// Tags reading their rows from `settings` now; the world is scanned on
    /// the first frame.
    #[must_use]
    pub fn new(settings: &dyn NametagSettingsSource) -> Self {
        Self {
            tags: BTreeMap::new(),
            destruction_queue: Vec::new(),
            last_pushed: Vec::new(),
            settings: settings.read(),
            now: 0.0,
            settings_poll_remaining: 0.0,
            needs_scan: true,
            walking: false,
            push_count: 0,
        }
    }

    /// How many label sets have been handed to the client.
    #[must_use]
    pub fn push_count(&self) -> usize {
        self.push_count
    }

    /// The objects carrying a tag, by id.
    #[must_use]
    pub fn tagged_object_ids(&self) -> Vec<u32> {
        self.tags.keys().copied().collect()
    }

    /// One object arrived, was appraised, or left (the client's object
    /// change event): arrival adds a tag when its group is on, an appraisal
    /// refreshes its text at once, leaving drops it.
    pub fn on_object_changed(&mut self, host: &mut dyn UbHost, change: &ObjectChange) {
        match change.kind {
            ObjectChangeKind::Created if self.settings.enabled => {
                self.add_tag(host, change.object_id);
            }
            ObjectChangeKind::IdentReceived if self.tags.contains_key(&change.object_id) => {
                self.update_data(host, change.object_id, true);
            }
            ObjectChangeKind::Released => self.remove_tag(change.object_id),
            _ => {}
        }
    }

    /// The frame: re-read the page now and then, refresh every tag on its
    /// own second, drop what the client dropped, and hand over the set when
    /// it changed.
    pub fn on_tick(
        &mut self,
        host: &mut dyn UbHost,
        settings: &dyn NametagSettingsSource,
        elapsed_seconds: f64,
    ) {
        let elapsed = elapsed_seconds.max(0.0);
        self.now += elapsed;
        self.settings_poll_remaining -= elapsed;
        if self.settings_poll_remaining <= 0.0 {
            self.settings_poll_remaining = SETTINGS_POLL_SECONDS;
            self.apply_settings(settings.read());
        }
        if !self.settings.enabled {
            self.push_if_changed(host);
            return;
        }
        if self.needs_scan {
            self.needs_scan = false;
            self.scan_world(host);
        }
        self.walking = true;
        let ids: Vec<u32> = self.tags.keys().copied().collect();
        for id in ids {
            if self.tags.contains_key(&id) {
                self.update_data(host, id, false);
            }
        }
        self.walking = false;
        self.flush_destruction_queue();
        self.push_if_changed(host);
    }

    /// The session ended: every tag belongs to a world that is gone; the
    /// client's set is cleared and the next world scanned afresh.
    pub fn reset(&mut self, host: &mut dyn UbHost) {
        self.tags.clear();
        self.destruction_queue.clear();
        self.needs_scan = true;
        self.push_if_changed(host);
    }

    /// Takes every label back from the client.
    pub fn dispose(&mut self, host: &mut dyn UbHost) {
        self.tags.clear();
        self.destruction_queue.clear();
        self.push_if_changed(host);
    }

    fn apply_settings(&mut self, next: NametagSettings) {
        if next == self.settings {
            return;
        }
        let was_enabled = self.settings.enabled;
        self.settings = next;
        if !next.enabled {
            self.tags.clear();
            self.destruction_queue.clear();
            self.needs_scan = true;
            return;
        }
        // A group switched off loses its tags; a group switched on gets them
        // on the scan that follows.
        for tag in self.tags.values() {
            if !next.of(tag.group).enabled {
                self.destruction_queue.push(tag.object_id);
            }
        }
        self.flush_destruction_queue();
        self.needs_scan = self.needs_scan || !was_enabled || next.any_group_on();
    }

    fn scan_world(&mut self, host: &mut dyn UbHost) {
        for world in host.objects() {
            if world.is_landscape {
                self.add_tag(host, world.object_id);
            }
        }
    }

    fn add_tag(&mut self, host: &mut dyn UbHost, object_id: u32) {
        let self_id = host.character().map_or(0, |character| character.id);
        if object_id == 0 || object_id == self_id || self.tags.contains_key(&object_id) {
            return;
        }
        let Some(world) = host.object(object_id) else {
            return;
        };
        let Some(group) = classify(&world) else {
            return;
        };
        if !self.settings.of(group).enabled {
            return;
        }
        self.tags
            .insert(object_id, Nametag::new(object_id, group, &world.name));
        self.update_data(host, object_id, false);
    }

    fn remove_tag(&mut self, object_id: u32) {
        if self.walking {
            self.destruction_queue.push(object_id);
        } else {
            self.tags.remove(&object_id);
        }
    }

    fn flush_destruction_queue(&mut self) {
        for object_id in self.destruction_queue.drain(..) {
            self.tags.remove(&object_id);
        }
    }

    /// Refreshes one tag from the object table, at most once a second unless
    /// forced by an appraisal arriving; the text is worked out only while
    /// the object is in range and still has something to learn.
    fn update_data(&mut self, host: &mut dyn UbHost, object_id: u32, force: bool) {
        let now = self.now;
        let max_range = self.settings.max_range;
        let Some(mut tag) = self.tags.get(&object_id).cloned() else {
            return;
        };
        if !force
            && now - tag.last_text_update < TEXT_UPDATE_INTERVAL_SECONDS
            && tag.last_text_update >= 0.0
        {
            return;
        }
        tag.last_text_update = now;
        let Some(world) = host.object(object_id) else {
            self.tags.insert(object_id, tag);
            self.remove_tag(object_id);
            return;
        };
        tag.name.clone_from(&world.name);
        let range = range_to(host, &world);
        tag.range = range;
        let out_of_range = range > max_range;
        if force || (tag.needs_process && !out_of_range) {
            match tag.group {
                NametagGroup::Player | NametagGroup::AllegiancePlayer => {
                    process_player(host, &mut tag, &world, range, now);
                }
                NametagGroup::Portal => process_portal(host, &mut tag, &world, range, now),
                NametagGroup::Monster => process_monster(host, &mut tag, range, now),
                _ => tag.needs_process = false,
            }
        }
        tag.out_of_range = out_of_range;
        self.tags.insert(object_id, tag);
    }

    /// Rebuilds the set the client should show and hands it over only when
    /// it differs from the last; a set past the client's cap is trimmed to
    /// the nearest objects.
    fn push_if_changed(&mut self, host: &mut dyn UbHost) {
        let mut labels = Vec::new();
        if self.settings.enabled {
            let mut shown: Vec<&Nametag> = self
                .tags
                .values()
                .filter(|tag| !tag.out_of_range && self.settings.of(tag.group).enabled)
                .collect();
            if shown.len() * 2 > MAXIMUM_LABELS {
                shown.sort_by(|a, b| {
                    a.range
                        .total_cmp(&b.range)
                        .then(a.object_id.cmp(&b.object_id))
                });
            } else {
                shown.sort_by_key(|tag| tag.object_id);
            }
            for tag in shown {
                let group = self.settings.of(tag.group);
                let needed = if tag.show_ticker { 2 } else { 1 };
                if labels.len() + needed > MAXIMUM_LABELS {
                    break;
                }
                labels.push(label(
                    tag.object_id,
                    &tag.text(),
                    group.tag_color,
                    i32::from(tag.show_ticker),
                    self.settings.max_range,
                ));
                if tag.show_ticker {
                    labels.push(label(
                        tag.object_id,
                        &tag.ticker,
                        group.ticker_color,
                        0,
                        self.settings.max_range,
                    ));
                }
            }
        }
        if labels == self.last_pushed {
            return;
        }
        self.last_pushed = labels;
        self.push_count += 1;
        host.show_labels(&self.last_pushed);
    }
}

fn classify(world: &WorldObject) -> Option<NametagGroup> {
    match world.object_class {
        ObjectClass::Player => Some(NametagGroup::Player),
        ObjectClass::CombatPet => Some(NametagGroup::Pet),
        ObjectClass::Monster => Some(NametagGroup::Monster),
        ObjectClass::Portal => Some(NametagGroup::Portal),
        ObjectClass::Npc => Some(NametagGroup::Npc),
        ObjectClass::Vendor => Some(NametagGroup::Vendor),
        _ => None,
    }
}

fn process_player(
    host: &mut dyn UbHost,
    tag: &mut Nametag,
    world: &WorldObject,
    range: f32,
    now: f64,
) {
    let properties = host.properties(tag.object_id);
    let level = level_of(properties.as_ref());
    if level <= 0 {
        try_assess(host, tag, range, now);
        return;
    }
    tag.level = level;
    let monarch = properties
        .as_ref()
        .and_then(|properties| instance_id(properties, MONARCH_PROPERTY))
        .unwrap_or(0);
    if monarch != tag.last_monarch {
        tag.last_monarch = monarch;
        if monarch == 0 {
            tag.show_ticker = false;
        } else {
            if shares_allegiance(host, tag.object_id, monarch) {
                tag.group = NametagGroup::AllegiancePlayer;
            }
            tag.show_ticker = true;
            let allegiance = properties
                .as_ref()
                .and_then(|properties| string(properties, ALLEGIANCE_NAME_PROPERTY))
                .unwrap_or_default();
            tag.ticker = if monarch == tag.object_id {
                format!("<{}>", world.name)
            } else {
                format!("<{allegiance}>")
            };
        }
    }
    tag.needs_process = false;
}

fn process_portal(
    host: &mut dyn UbHost,
    tag: &mut Nametag,
    world: &WorldObject,
    range: f32,
    now: f64,
) {
    if !world.has_appraisal_data {
        try_assess(host, tag, range, now);
        return;
    }
    let destination = host
        .properties(tag.object_id)
        .and_then(|properties| string(&properties, PORTAL_DESTINATION_PROPERTY))
        .unwrap_or_default();
    tag.needs_process = false;
    tag.show_ticker = true;
    tag.ticker = format!("<{destination}>");
}

fn process_monster(host: &mut dyn UbHost, tag: &mut Nametag, range: f32, now: f64) {
    let properties = host.properties(tag.object_id);
    let owner = properties
        .as_ref()
        .and_then(|properties| instance_id(properties, PET_OWNER_PROPERTY))
        .unwrap_or(0);
    if owner != 0 {
        tag.group = NametagGroup::Pet;
        tag.needs_process = false;
        return;
    }
    let level = level_of(properties.as_ref());
    if level > 0 {
        tag.needs_process = false;
        tag.level = level;
    }
    try_assess(host, tag, range, now);
}

/// The appraisal backoff: closer than last time, at most every five
/// seconds, and not past the try limit; the first request always goes.
fn try_assess(host: &mut dyn UbHost, tag: &mut Nametag, range: f32, now: f64) {
    if tag.assess_count > ASSESS_ATTEMPT_LIMIT {
        tag.needs_process = false;
        return;
    }
    if tag.next_assess_at <= now && tag.last_assess_range > range {
        tag.last_assess_range = range;
        tag.next_assess_at = now + ASSESS_RETRY_SECONDS;
        tag.assess_count += 1;
        host.identify(tag.object_id);
    }
}

fn shares_allegiance(host: &mut dyn UbHost, object_id: u32, monarch: u32) -> bool {
    let mine = host.allegiance().monarch_id;
    mine != 0 && (monarch == mine || object_id == mine)
}

fn level_of(properties: Option<&ItemProperties>) -> i32 {
    properties
        .and_then(|properties| properties.int(LEVEL_PROPERTY))
        .unwrap_or(0)
}

fn instance_id(properties: &ItemProperties, key: u32) -> Option<u32> {
    properties
        .instance_ids
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| *value)
}

fn string(properties: &ItemProperties, key: u32) -> Option<String> {
    properties
        .strings
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, value)| value.clone())
}

#[allow(clippy::cast_possible_truncation)]
fn range_to(host: &mut dyn UbHost, world: &WorldObject) -> f32 {
    let snapshot = host.navigation();
    match (&world.position, snapshot.is_available) {
        (Some(position), true) => snapshot.position.horizontal_distance_metres(position) as f32,
        _ => f32::MAX,
    }
}

fn label(object_id: u32, text: &str, argb: u32, line: i32, max_range: f32) -> WorldLabel {
    let channel = |shift: u32| f32::from(u8::try_from((argb >> shift) & 0xFF).unwrap_or(0)) / 255.0;
    WorldLabel {
        object_id,
        text: text.to_owned(),
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: channel(24),
        height_offset: 0.0,
        line,
        max_range,
        outline: true,
    }
}

/// The character's own position, when the client places it.
#[must_use]
pub fn self_position(host: &mut dyn UbHost) -> Option<Position> {
    let snapshot = host.navigation();
    snapshot.is_available.then_some(snapshot.position)
}
