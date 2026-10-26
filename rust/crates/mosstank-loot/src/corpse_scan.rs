//! The looter's corpse half (`Looting.CorpseScan.cs`): which corpses the
//! client knows of, which of them may be looted at all, which one the next
//! pass picks, and how a corpse is remembered -- refused, unopenable,
//! finished -- until it is forgotten again.

use std::sync::OnceLock;

use ac_plugin_abi::world::Position;
use mosstank_automation::locks::ActionLockKind;
use mosstank_dotnet::text::{casing, ordinal};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use crate::controller::{DENIAL_SKIP_SECONDS, LootController, clamp, whole};
use crate::host::{LootCorpse, LootHost};
use mosstank_automation::heading;
use mosstank_combat::mover::STUCK_PROGRESS_METERS;

/// The radius the open step picks within -- arm's reach, a little over five
/// metres. Outside it the corpse is the approach step's business.
pub const CORPSE_OPEN_RANGE_METERS: f64 = 240.0 / 48.0;

/// The margin the reference adds to the approach range when it asks
/// whether an undescribed corpse is close enough to hold the walks off: ten
/// metres.
pub const CORPSE_ID_WAIT_MARGIN_METERS: f64 = 240.0 / 24.0;

/// How long before the announcement a corpse may have appeared and still
/// be the one it is about.
pub const RARE_ANNOUNCEMENT_LEAD_SECONDS: f64 = 5.0;

/// How long an announcement keeps new corpses worth describing.
pub const RARE_ANNOUNCEMENT_WINDOW_SECONDS: f64 = 30.0;

/// How many times a corpse may fail to answer for itself before the looter
/// stops asking.
const CORPSE_DESCRIPTION_ATTEMPT_LIMIT: i32 = 3;

fn pattern(cell: &'static OnceLock<DotNetRegex>, source: &str) -> &'static DotNetRegex {
    cell.get_or_init(|| DotNetRegex::new(source, RegexOptions::NONE).expect("the pattern compiles"))
}

fn rare_discovered() -> &'static DotNetRegex {
    static CELL: OnceLock<DotNetRegex> = OnceLock::new();
    pattern(
        &CELL,
        r"^(?<finder>[^\r\n]+?) has discovered the (?<item>[^\r\n]+)!$",
    )
}

fn corpse_already_in_use() -> &'static DotNetRegex {
    static CELL: OnceLock<DotNetRegex> = OnceLock::new();
    pattern(
        &CELL,
        r"The (Corpse)|(Treasure) of ([a-zA-Z\ \-\']*) is already in use by someone else!",
    )
}

fn no_right_to_loot() -> &'static DotNetRegex {
    static CELL: OnceLock<DotNetRegex> = OnceLock::new();
    pattern(
        &CELL,
        r"You do not yet have the right to loot the (Corpse)|(Treasure) of .*",
    )
}

#[cfg(test)]
fn killed_by_description() -> &'static DotNetRegex {
    static CELL: OnceLock<DotNetRegex> = OnceLock::new();
    pattern(&CELL, r"(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)")
}

/// A kill description that goes on to say the corpse generated something:
/// "Killed by Name. This corpse generated a rare item!".
#[cfg(test)]
fn rare_kill_description() -> &'static DotNetRegex {
    static CELL: OnceLock<DotNetRegex> = OnceLock::new();
    pattern(
        &CELL,
        r"(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)([gG]enerated)(?:.*)",
    )
}

fn is_match(regex: &DotNetRegex, text: &str) -> bool {
    regex.is_match(text).unwrap_or(false)
}

/// The killer a corpse's long description names, empty when it names none.
#[must_use]
pub fn killer_name(description: &str) -> String {
    if casing::is_white_space_only(description) {
        return String::new();
    }
    killed_by_at_start(description)
        .map(str::to_owned)
        .unwrap_or_default()
}

/// A corpse is rare when the host flags it so, or when its description
/// names its killer and then says it generated something.
#[must_use]
pub fn is_rare(corpse: &LootCorpse) -> bool {
    if corpse.is_generated_rare {
        return true;
    }
    let description = corpse.long_description.as_str();
    killed_by_at_start(description).is_some() && names_a_generated_kill(description)
}

// The two kill patterns, read by hand: run on every corpse about on every
// pass, the regular expressions cost the plugin more than a busy town's
// corpses leave room for. The tests hold them to the patterns.

/// Where a description says who killed it.
const KILLED_BY: &str = "Killed by ";

/// How many bytes at the start of `text` a name may have
/// (`[a-zA-Z\ \-\']*`).
fn name_run(text: &str) -> usize {
    text.bytes()
        .take_while(|byte| byte.is_ascii_alphabetic() || matches!(byte, b' ' | b'-' | b'\''))
        .count()
}

/// The killer, when the kill pattern (`(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)`)
/// matches at the description's start: the name runs as far as name
/// characters go, and a full stop follows it.
fn killed_by_at_start(description: &str) -> Option<&str> {
    let rest = description.strip_prefix(KILLED_BY)?;
    let run = name_run(rest);
    (rest.as_bytes().get(run) == Some(&b'.')).then(|| &rest[..run])
}

/// Whether the rare kill pattern
/// (`(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)([gG]enerated)(?:.*)`) matches
/// anywhere: a kill naming its killer, then on the same line `generated`.
fn names_a_generated_kill(description: &str) -> bool {
    description.match_indices(KILLED_BY).any(|(at, _)| {
        let rest = &description[at + KILLED_BY.len()..];
        let run = name_run(rest);
        if rest.as_bytes().get(run) != Some(&b'.') {
            return false;
        }
        let line = rest[run + 1..].split('\n').next().unwrap_or("");
        line.match_indices("enerated")
            .any(|(word, _)| word > 0 && matches!(line.as_bytes()[word - 1], b'g' | b'G'))
    })
}

/// Has the server answered this corpse's description request, with the
/// description or without one?
#[must_use]
pub fn is_description_answered(corpse: &LootCorpse) -> bool {
    corpse.is_identified || corpse.is_appraisal_answered
}

/// How far round, in degrees, the character would have to turn to face the
/// corpse; a corpse with no place answers a number no real corpse can beat.
fn heading_offset_to(own: &Position, corpse: &LootCorpse) -> f64 {
    let Some(position) = &corpse.position else {
        return 99999.0;
    };
    f64::from(
        heading::signed_heading_delta(own.heading_degrees, heading::desired_heading(own, position))
            .abs(),
    )
}

impl LootController {
    /// A finished corpse is closed with a second use, which waits for the
    /// item channel to be free; a refused one is tried again next pass.
    pub(crate) fn close_finished_corpse(
        &mut self,
        host: &mut dyn LootHost,
        corpse_id: u32,
        can_act: bool,
    ) -> bool {
        if corpse_id == 0 {
            return false;
        }
        if !can_act
            || !host.items_available()
            || !host.loot_available()
            || self.item_slot_held(host)
        {
            "Waiting to close corpse…".clone_into(&mut self.status);
            return true;
        }
        self.status = if host.loot_close(corpse_id).accepted() {
            "Corpse complete.".to_owned()
        } else {
            "Waiting to close corpse…".to_owned()
        };
        true
    }

    /// Drops the three slots an open took, the moment the container it was
    /// waiting for is actually open; runs on the host's frame. True when
    /// the slots were released this frame.
    pub fn observe_corpse_opened(&mut self, host: &mut dyn LootHost) -> bool {
        let Some(locks) = self.action_locks.clone() else {
            return false;
        };
        if !locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
            || self.active_corpse == 0
            || !host.is_available()
        {
            return false;
        }
        if !host.loot_available() || host.current_container_id() != self.active_corpse {
            return false;
        }
        let mut locks = locks.borrow_mut();
        locks.release(ActionLockKind::CorpseOpenAttempt);
        locks.release(ActionLockKind::Navigation);
        locks.release(ActionLockKind::ItemUse);
        true
    }

    pub(crate) fn is_own_death_corpse(&self, host: &mut dyn LootHost, corpse: &LootCorpse) -> bool {
        let character = host.character().name;
        !character.is_empty() && corpse.name == format!("Corpse of {character}")
    }

    /// The corpse the approach step walks at: the nearest one it may loot
    /// inside the approach range. It is the same pick the open step makes,
    /// with the other metric and the other radius.
    pub fn try_select_approach_corpse(
        &mut self,
        host: &mut dyn LootHost,
        range_meters: f64,
    ) -> Option<LootCorpse> {
        if !self.settings.profile_active || !self.settings.enabled || !host.is_available() {
            return None;
        }
        if !host.loot_available() {
            return None;
        }
        if self.settings.rules.is_empty()
            && casing::is_white_space_only(&self.settings.external_classifier_id)
        {
            return None;
        }
        let known = host.loot_corpses(f32::MAX);
        self.select_corpse(host, &known, range_meters, false, true)
    }

    /// Whether the open step still has known work while its paced scan is
    /// waiting.
    pub fn has_eligible_corpse_in_open_range(&mut self, host: &mut dyn LootHost) -> bool {
        if !self.settings.profile_active || !self.settings.enabled || !host.is_available() {
            return false;
        }
        if !host.loot_available() {
            return false;
        }
        let known = host.loot_corpses(f32::MAX);
        self.select_corpse(host, &known, CORPSE_OPEN_RANGE_METERS, true, false)
            .is_some()
    }

    /// Whether a corpse whose description has not arrived lies within
    /// `range_meters`: the reference's "a corpse id request is pending".
    pub fn has_corpse_awaiting_description_within(
        &mut self,
        host: &mut dyn LootHost,
        range_meters: f64,
    ) -> bool {
        if !self.can_process_loot(host) || !self.rare_window_open_or_not_rare_only() {
            return false;
        }
        if !host.loot_available() {
            return false;
        }
        for corpse in host.loot_corpses(f32::MAX) {
            let distance = f64::from(corpse.distance);
            if !is_description_answered(&corpse)
                && self.wants_description(&corpse)
                && (distance <= range_meters
                    || (self.settings.walk_to_own_rare_corpses
                        && distance <= crate::RARE_CORPSE_REACH_METERS
                        && self.appeared_with_own_rare_announcement(&corpse)))
                && !self.completed_corpses.contains_key(&corpse.object_id)
                && !self.is_corpse_denied(corpse.object_id)
                && !self.is_corpse_blacklisted(corpse.object_id)
            {
                return true;
            }
        }
        false
    }

    /// The corpse pick. Any rare corpse beats any non-rare one whatever the
    /// running best. The walk ranks by distance; the open ranks by how far
    /// round the character would have to turn to face it. The walk's pick
    /// alone (`own_rare_reach`) lets this character's own rare through out
    /// to the rare reach.
    pub(crate) fn select_corpse(
        &mut self,
        host: &mut dyn LootHost,
        corpses: &[LootCorpse],
        range_meters: f64,
        by_heading: bool,
        own_rare_reach: bool,
    ) -> Option<LootCorpse> {
        let own = host.position();
        let mut selected: Option<&LootCorpse> = None;
        let mut best = f64::MAX;
        let mut rare_picked = false;
        for corpse in corpses {
            let reach = if own_rare_reach {
                self.approach_reach_for(host, corpse, range_meters)
            } else {
                range_meters
            };
            if f64::from(corpse.distance) > reach
                || self.completed_corpses.contains_key(&corpse.object_id)
                || self.is_corpse_denied(corpse.object_id)
                || self.is_corpse_blacklisted(corpse.object_id)
                || !corpse.is_identified
                || !self.can_loot(host, corpse)
            {
                continue;
            }
            let metric = if by_heading {
                heading_offset_to(&own, corpse)
            } else {
                f64::from(corpse.distance)
            };
            let rare = is_rare(corpse);
            if rare && !rare_picked {
                rare_picked = true;
                best = metric;
                selected = Some(corpse);
            } else if (rare || !rare_picked) && metric < best {
                best = metric;
                selected = Some(corpse);
            }
        }
        selected.cloned()
    }

    /// The newest chat line's sequence the host still holds; zero when it
    /// holds none.
    pub(crate) fn latest_chat_sequence(host: &mut dyn LootHost) -> u64 {
        host.chat_messages(0)
            .iter()
            .map(|message| message.sequence)
            .max()
            .unwrap_or(0)
    }

    /// The chat since the last read: a rare announcement opens the window
    /// in which new corpses are the rare's.
    pub(crate) fn observe_chat(&mut self, host: &mut dyn LootHost) {
        for message in host.chat_messages(self.chat_sequence) {
            if message.sequence > self.chat_sequence {
                self.chat_sequence = message.sequence;
            }
            self.observe_rare_announcement(host, &message.text);
        }
    }

    /// A status-line notice from the server. It refuses a corpse someone
    /// else owns or has open with one (never a chat line); the corpse is
    /// then skipped for ten seconds.
    pub fn observe_status_text(&mut self, text: &str) {
        // Always the corpse the last pick chose, never the one that happens
        // to be open -- that is what the refusal is an answer to.
        let denied = self.selected_corpse;
        if denied == 0
            || (!is_match(corpse_already_in_use(), text) && !is_match(no_right_to_loot(), text))
        {
            return;
        }
        self.corpse_denied_at.insert(denied, self.lifetime);
        self.write_log(&format!(
            "LootCorpse: 0x{denied:08X} refused, skipping it for {} seconds",
            whole(DENIAL_SKIP_SECONDS)
        ));
        if self.active_corpse == denied {
            self.active_corpse = 0;
            self.active_corpse_saw_contents = false;
            self.active_corpse_is_own_death = false;
            self.state_age = 0.0;
        }
    }

    /// "Name has discovered the Item!": when the name is this character's,
    /// the corpses that appeared around then are the rare's. Listened for
    /// only with rare-only looting or the own-rare walk on.
    fn observe_rare_announcement(&mut self, host: &mut dyn LootHost, text: &str) {
        if !self.settings.loot_only_rare_corpses && !self.settings.walk_to_own_rare_corpses {
            return;
        }
        let Ok(Some(found)) = rare_discovered().find(text) else {
            return;
        };
        let finder = found.group("finder").value.to_owned();
        let item = found.group("item").value.to_owned();
        let name = host.character().name;
        let character = name.trim_start_matches('+');
        if character.is_empty()
            || !ordinal::equals_ignore_case(finder.trim_start_matches('+'), character)
        {
            return;
        }
        self.rare_announced_at = self.lifetime;
        let line = if self.settings.loot_only_rare_corpses {
            format!("LootCorpse: rare announced ({item}); describing new corpses")
        } else {
            format!("LootCorpse: rare announced ({item})")
        };
        self.write_log(&line);
    }

    /// Whether this corpse is worth a description request: with rare-only
    /// looting on, only corpses that appeared around this character's own
    /// rare announcement are.
    pub(crate) fn wants_description(&self, corpse: &LootCorpse) -> bool {
        !self.settings.loot_only_rare_corpses || self.appeared_with_own_rare_announcement(corpse)
    }

    /// Whether this corpse may be the one this character's latest rare
    /// announcement is about.
    pub(crate) fn appeared_with_own_rare_announcement(&self, corpse: &LootCorpse) -> bool {
        self.lifetime - self.rare_announced_at <= RARE_ANNOUNCEMENT_WINDOW_SECONDS
            && self
                .corpse_first_seen
                .get(&corpse.object_id)
                .is_none_or(|&first_seen| {
                    first_seen >= self.rare_announced_at - RARE_ANNOUNCEMENT_LEAD_SECONDS
                })
    }

    /// False only while rare-only looting waits for an announcement.
    pub(crate) fn rare_window_open_or_not_rare_only(&self) -> bool {
        !self.settings.loot_only_rare_corpses
            || self.lifetime - self.rare_announced_at <= RARE_ANNOUNCEMENT_WINDOW_SECONDS
    }

    pub(crate) fn is_corpse_denied(&self, corpse_id: u32) -> bool {
        self.corpse_denied_at
            .get(&corpse_id)
            .is_some_and(|&at| self.lifetime - at < DENIAL_SKIP_SECONDS)
    }

    fn can_loot(&self, host: &mut dyn LootHost, corpse: &LootCorpse) -> bool {
        if self.settings.loot_only_rare_corpses && !is_rare(corpse) {
            return false;
        }
        if self.is_own_kill(host, corpse) {
            return true;
        }
        // A corpse whose description names no killer is nobody's.
        let killer = killer_name(&corpse.long_description);
        if killer.is_empty() {
            return false;
        }
        if is_rare(corpse) {
            return false;
        }
        let first_seen = self
            .corpse_first_seen
            .get(&corpse.object_id)
            .copied()
            .unwrap_or(self.lifetime);
        let age = self.lifetime - first_seen;
        if let Some(member) = host
            .fellowship_members()
            .into_iter()
            .find(|member| ordinal::equals_ignore_case(&member.name, &killer))
            .filter(|member| member.object_id != 0)
        {
            if !self.settings.loot_fellow_corpses {
                return false;
            }
            return member.share_loot || age >= 100.0;
        }
        self.settings.loot_all_corpses && age >= 100.0
    }

    /// The three slots an open attempt holds, for the profile's open
    /// timeout, armed after every attempt whether the use went out or not.
    pub(crate) fn arm_corpse_open_slots(&self) {
        let open_window = self.settings.corpse_open_timeout_seconds.max(0.25);
        self.arm(ActionLockKind::ItemUse, open_window);
        self.arm(ActionLockKind::Navigation, open_window);
        self.arm(ActionLockKind::CorpseOpenAttempt, open_window);
    }

    /// Counts a description that never arrived, and after the third skips
    /// the corpse for the profile's blacklist period.
    pub(crate) fn note_corpse_description_failed(&mut self, corpse_id: u32) {
        if corpse_id == 0 || self.is_corpse_blacklisted(corpse_id) {
            return;
        }
        let attempts = self
            .corpse_description_attempts
            .get(&corpse_id)
            .copied()
            .unwrap_or(0)
            + 1;
        if attempts < CORPSE_DESCRIPTION_ATTEMPT_LIMIT {
            self.corpse_description_attempts.insert(corpse_id, attempts);
            return;
        }
        self.corpse_description_attempts.remove(&corpse_id);
        self.corpse_blacklisted_at.insert(corpse_id, self.lifetime);
        let timeout = clamp(
            self.settings.blacklist_corpse_open_timeout_seconds,
            1.0,
            3600.0,
        );
        self.write_log(&format!(
            "Skipping corpse 0x{corpse_id:08X}: no description after {attempts} attempts, for {} seconds",
            whole(timeout)
        ));
    }

    pub(crate) fn blacklist_failed_corpse(&mut self, corpse_id: u32) {
        if corpse_id == 0 {
            return;
        }
        let attempts = self
            .corpse_open_attempts
            .get(&corpse_id)
            .copied()
            .unwrap_or(0)
            + 1;
        let threshold = self
            .settings
            .blacklist_corpse_open_attempt_count
            .clamp(1, 1000);
        if attempts < threshold {
            self.corpse_open_attempts.insert(corpse_id, attempts);
            self.status = format!("Retrying corpse ({attempts}/{threshold})…");
            return;
        }
        self.corpse_open_attempts.remove(&corpse_id);
        self.corpse_blacklisted_at.insert(corpse_id, self.lifetime);
        self.status = format!(
            "Blacklisted unopenable corpse for {} seconds.",
            whole(clamp(
                self.settings.blacklist_corpse_open_timeout_seconds,
                1.0,
                3600.0
            ))
        );
    }

    pub(crate) fn is_corpse_blacklisted(&mut self, corpse_id: u32) -> bool {
        let Some(&since) = self.corpse_blacklisted_at.get(&corpse_id) else {
            return false;
        };
        let timeout = clamp(
            self.settings.blacklist_corpse_open_timeout_seconds,
            1.0,
            3600.0,
        );
        if self.lifetime - since < timeout {
            return true;
        }
        self.corpse_blacklisted_at.remove(&corpse_id);
        false
    }

    /// Sets aside, for the profile's blacklist period, a corpse the
    /// character cannot get to: a walk to it that stopped covering ground,
    /// or an open whose walk there covered none. It is not recorded as
    /// looted; after the period it is tried again (MossTank register
    /// LOOT-8).
    pub(crate) fn set_aside_unreachable_corpse(&mut self, corpse_id: u32, reason: &str) {
        if corpse_id == 0 {
            return;
        }
        self.corpse_open_attempts.remove(&corpse_id);
        self.corpse_blacklisted_at.insert(corpse_id, self.lifetime);
        let timeout = clamp(
            self.settings.blacklist_corpse_open_timeout_seconds,
            1.0,
            3600.0,
        );
        self.status = format!(
            "Set aside a corpse out of reach for {} seconds.",
            whole(timeout)
        );
        self.write_log(&format!(
            "LootCorpse: setting aside 0x{corpse_id:08X} for {} seconds: {reason}",
            whole(timeout)
        ));
    }

    /// Notes when the open of the active corpse has gone out: the client
    /// asks for the container, or answers a use of the corpse.
    pub(crate) fn observe_open_request(&mut self, host: &mut dyn LootHost) {
        if self.active_corpse == 0 || self.active_corpse_requested {
            return;
        }
        let answered = host.last_item_completion();
        if host.requested_container_id() == self.active_corpse
            || (answered.revision != self.open_completion_revision
                && answered.source_id == self.active_corpse)
        {
            self.active_corpse_requested = true;
        }
    }

    /// The corpse walk steered the character: a walk the client was making
    /// for the open, if any, is called off by it, not given up.
    pub fn note_corpse_walk_steered(&mut self) {
        if self.active_corpse != 0 {
            self.open_walk_called_off = true;
        }
    }

    /// An open of a corpse out of arm's reach is the client's walk first;
    /// the open goes out when the walk arrives. When the client is idle
    /// again with the open never sent and the character standing where it
    /// opened, the walk covered no ground (the client gives up a walk that
    /// gets no closer): something stands between the character and the
    /// corpse. Opening it again sends the same walk into the same obstacle,
    /// so the corpse is set aside. A walk the corpse walk called off, or one
    /// that covered ground, is left to open again.
    pub(crate) fn set_aside_open_that_never_left(&mut self, host: &mut dyn LootHost, current: u32) {
        if self.active_corpse == 0
            || current == self.active_corpse
            || self.active_corpse_requested
            || self.state_age <= self.settings.corpse_open_timeout_seconds.max(0.25)
            || host.items_busy()
        {
            return;
        }
        let unreached = self.active_corpse;
        let covered = heading::horizontal_distance_meters(&host.position(), &self.open_position);
        if self.open_walk_called_off || covered >= STUCK_PROGRESS_METERS {
            return;
        }
        self.active_corpse = 0;
        self.active_corpse_saw_contents = false;
        self.active_corpse_is_own_death = false;
        self.state_age = 0.0;
        self.set_aside_unreachable_corpse(unreached, "the walk to it covered no ground");
    }

    /// A corpse this pass is done with, however it ended: emptied, or given
    /// up on.
    pub(crate) fn is_corpse_finished(&self, corpse_id: u32) -> bool {
        corpse_id != 0
            && (self.completed_corpses.contains_key(&corpse_id)
                || corpse_id == self.abandoned_corpse)
    }

    /// Lets go of the corpse the pass is working when the client will not
    /// send a pull for what is in it: skipped for the blacklist period, not
    /// recorded as looted.
    pub(crate) fn give_up_on_corpse(&mut self, corpse_id: u32, reason: &str) {
        if corpse_id == 0 {
            return;
        }
        self.abandoned_corpse = corpse_id;
        self.corpse_blacklisted_at.insert(corpse_id, self.lifetime);
        self.write_log(&format!(
            "LootCorpse: giving up on 0x{corpse_id:08X}: {reason}"
        ));
    }

    pub(crate) fn mark_corpse_complete(&mut self, corpse_id: u32) {
        if corpse_id == 0 {
            return;
        }
        self.completed_corpses.insert(corpse_id, self.lifetime);
        if corpse_id == self.opened_own_rare_corpse {
            self.completed_own_rare_corpses.insert(corpse_id);
        }
        self.corpse_open_attempts.remove(&corpse_id);
        self.corpse_description_attempts.remove(&corpse_id);
        self.corpse_blacklisted_at.remove(&corpse_id);
    }

    /// Stamps the corpses the client is reporting and drops the records of
    /// the ones it has stopped reporting, once the cache timeout has run
    /// since each last came into the known set.
    pub(crate) fn prune_corpse_cache(&mut self, known: &[LootCorpse]) {
        let expiry = clamp(self.settings.corpse_cache_timeout_minutes, 1.0, 1440.0) * 60.0;

        self.present_corpses.clear();
        for seen in known {
            self.present_corpses.insert(seen.object_id);
            // Newly known, or known again after having gone: either way this
            // is when its eviction clock restarts. The first-sighting stamp
            // the ownership timers measure against is never restarted.
            let newly_known = !self.corpse_first_seen.contains_key(&seen.object_id);
            if newly_known {
                self.corpse_first_seen.insert(seen.object_id, self.lifetime);
            }
            if newly_known || self.released_corpses.remove(&seen.object_id) {
                self.corpse_last_seen.insert(seen.object_id, self.lifetime);
            }
        }
        for id in self.corpse_first_seen.keys() {
            if !self.present_corpses.contains(id) {
                self.released_corpses.insert(*id);
            }
        }

        self.evicted_corpses.clear();
        for id in &self.released_corpses {
            if let Some(&last) = self.corpse_last_seen.get(id)
                && self.lifetime - last >= expiry
            {
                self.evicted_corpses.push(*id);
            }
        }
        for id in std::mem::take(&mut self.evicted_corpses) {
            self.corpse_first_seen.remove(&id);
            self.corpse_last_seen.remove(&id);
            self.released_corpses.remove(&id);
            self.completed_corpses.remove(&id);
            self.completed_own_rare_corpses.remove(&id);
            self.corpse_denied_at.remove(&id);
            self.corpse_blacklisted_at.remove(&id);
            self.corpse_open_attempts.remove(&id);
            self.corpse_description_attempts.remove(&id);
        }
    }

    /// Advances from the last accepted request so one corpse that never
    /// answers cannot monopolize every later request. True when a request
    /// went out or the client is busy with one.
    pub(crate) fn try_request_next_corpse_description(
        &mut self,
        host: &mut dyn LootHost,
        known: &[LootCorpse],
    ) -> bool {
        let last_index = known
            .iter()
            .position(|corpse| corpse.object_id == self.last_corpse_description_request)
            .map_or(-1, |index| i64::try_from(index).unwrap_or(i64::MAX));
        let count = i64::try_from(known.len()).unwrap_or(i64::MAX);
        for offset in 1..=count {
            let index = usize::try_from((last_index + offset) % count).unwrap_or(0);
            let candidate = &known[index];
            if is_description_answered(candidate)
                || !self.wants_description(candidate)
                || self.completed_corpses.contains_key(&candidate.object_id)
                || self.is_corpse_denied(candidate.object_id)
                || self.is_corpse_blacklisted(candidate.object_id)
            {
                continue;
            }
            let identify = host.loot_identify(candidate.object_id);
            if identify.accepted() {
                self.awaiting_corpse_appraisal = candidate.object_id;
                self.last_corpse_description_request = candidate.object_id;
                self.state_age = 0.0;
                self.identify_age = 0.0;
                self.status = format!("Identifying {}…", candidate.name);
                let distance = openac_plugin_compat::number::format_f32(candidate.distance, "0.0")
                    .expect("a supported format");
                self.write_log(&format!(
                    "Identifying corpse {} (0x{:08X}) at {distance}m",
                    candidate.name, candidate.object_id
                ));
                return true;
            }
            if identify.status == ac_plugin_abi::items::ItemStatus::Busy {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kill patterns read by hand find what the regular expressions
    /// find.
    #[test]
    fn the_kill_patterns_read_by_hand_find_what_the_patterns_find() {
        let descriptions = [
            "",
            "   ",
            "Killed by Mossy Hunter.",
            "Killed by Mossy Hunter",
            "Killed by +Acdream.",
            "Killed by O'Brien-Smith.",
            "Killed by Brightblade 1.",
            "Killed by .",
            "Killed by Ayla. This corpse generated a rare item!",
            "Killed by Ayla. This corpse Generated a rare item!",
            "Killed by Ayla.\nThis corpse generated a rare item!",
            "Killed by Ayla.generated",
            "Killed by Ayla. enerated",
            "killed by Ayla. generated",
            "A corpse. Killed by Ayla. generated",
            "Killed by Ayla! Killed by Bo. generated",
            "Killed by \u{e9}mile.",
            "Killed by Ayla.\r generated",
            "xKilled by Ayla.",
        ];
        for description in descriptions {
            let regex_killer = if casing::is_white_space_only(description) {
                String::new()
            } else {
                match killed_by_description().find(description) {
                    Ok(Some(found)) if found.index() == 0 => found.group_number(1).value.to_owned(),
                    _ => String::new(),
                }
            };
            assert_eq!(killer_name(description), regex_killer, "{description:?}");
            let regex_rare = matches!(killed_by_description().find(description), Ok(Some(found)) if found.index() == 0)
                && is_match(rare_kill_description(), description);
            let corpse = LootCorpse {
                long_description: description.to_owned(),
                ..LootCorpse::default()
            };
            assert_eq!(is_rare(&corpse), regex_rare, "{description:?}");
        }
    }
}
