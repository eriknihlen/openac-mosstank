//! VTank's fellowship manager (`FellowshipManager`): it answers players'
//! tells ("xp", "line", "remove", "leader", "help", "startvote", "vote",
//! "location"), keeps a waiting list and recruits from it while the
//! character leads, runs the members' votes (kick, ban, give the lead,
//! open or close), and says everything in VTank's own words.

use std::collections::VecDeque;

use ac_plugin_abi::social::{FellowMember, FellowshipStatus};
use ac_plugin_abi::types::ChatLine;
use openac_plugin_compat::number::{NumberStyles, parse_i32};
use openac_plugin_compat::text::{casing, ordinal};

use crate::client::FellowshipAutomation;

const MAXIMUM_OTHER_MEMBERS: usize = 8;
const REQUEST_LIFETIME_SECONDS: f64 = 300.0;
const VOTE_LIFETIME_SECONDS: f64 = 120.0;
const VOTE_CALLER_COOLDOWN_SECONDS: f64 = 240.0;
const RECRUIT_RANGE_METERS: f64 = 10.0;

/// The kind of tell the manager answers (a tell's chat kind).
const TELL_KIND: u32 = 3;

#[derive(Debug, Clone)]
struct WaitingPlayer {
    name: String,
    object_id: u32,
    expires_at: f64,
    attempts: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FellowVoteKind {
    Kick,
    Ban,
    GiveLeader,
    SetOpen,
}

#[derive(Debug, Clone)]
struct FellowVote {
    id: i32,
    kind: FellowVoteKind,
    parameter: String,
    expires_at: f64,
    /// Each voter's ballot, names compared ignoring case.
    ballots: Vec<(String, bool)>,
}

impl FellowVote {
    fn description(&self) -> String {
        format!("'{:?} {}' (ID {})", self.kind, self.parameter, self.id)
    }

    fn cast(&mut self, voter: &str, yes: bool) {
        match self
            .ballots
            .iter_mut()
            .find(|(name, _)| ordinal::equals_ignore_case(name, voter))
        {
            Some(ballot) => ballot.1 = yes,
            None => self.ballots.push((voter.to_owned(), yes)),
        }
    }

    fn tally(&self) -> (usize, usize) {
        let yes = self.ballots.iter().filter(|(_, yes)| *yes).count();
        (yes, self.ballots.len() - yes)
    }
}

fn same_name(left: &str, right: &str) -> bool {
    ordinal::equals_ignore_case(left, right)
}

fn trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

/// `bool.TryParse`: "true" or "false" in any case, spaces around allowed.
fn parse_bool(text: &str) -> Option<bool> {
    let text = trim(text);
    if same_name(text, "true") {
        Some(true)
    } else if same_name(text, "false") {
        Some(false)
    } else {
        None
    }
}

/// `Split(' ', count, RemoveEmptyEntries)`: the words between spaces, the
/// last of at most `count` holding the rest of the line.
fn split_words(text: &str, count: usize) -> Vec<&str> {
    let mut words = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() {
            break;
        }
        if words.len() + 1 == count {
            words.push(rest.trim_end_matches(' '));
            break;
        }
        let end = rest.find(' ').unwrap_or(rest.len());
        words.push(&rest[..end]);
        rest = &rest[end..];
    }
    words
}

fn is_incoming_tell(message: &ChatLine) -> bool {
    message.kind == TELL_KIND && message.sender_id != 0
}

/// The fellowship manager.
#[derive(Debug)]
pub struct FellowshipManager {
    waiting: Vec<WaitingPlayer>,
    banned: Vec<String>,
    vote_cooldowns: Vec<(String, f64)>,
    votes: Vec<FellowVote>,
    tell_rate: Vec<(String, VecDeque<f64>)>,
    chat_sequence: u64,
    now: f64,
    next_recruit_at: f64,
    next_vote_id: i32,
    was_leader: bool,
    desired_open: bool,
    status: String,
}

impl Default for FellowshipManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FellowshipManager {
    /// An idle manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            waiting: Vec::new(),
            banned: Vec::new(),
            vote_cooldowns: Vec::new(),
            votes: Vec::new(),
            tell_rate: Vec::new(),
            chat_sequence: 0,
            now: 0.0,
            next_recruit_at: 0.0,
            next_vote_id: 1,
            was_leader: false,
            desired_open: true,
            status: "Fellow manager idle".to_owned(),
        }
    }

    /// What the manager is doing, for the panel.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The players waiting to be recruited, in order.
    #[must_use]
    pub fn waiting_names(&self) -> Vec<String> {
        self.waiting
            .iter()
            .map(|player| player.name.clone())
            .collect()
    }

    /// One turn: reads the new tells, then, in a fellowship, keeps the
    /// waiting list and the votes and recruits while the character leads.
    pub fn tick(
        &mut self,
        automation: &mut dyn FellowshipAutomation,
        elapsed_seconds: f64,
        enabled: bool,
    ) {
        self.now += elapsed_seconds.max(0.0);
        for message in automation.chat_messages(self.chat_sequence) {
            self.chat_sequence = self.chat_sequence.max(message.sequence);
            if enabled && is_incoming_tell(&message) {
                self.handle_tell(automation, &message);
            }
        }

        let state = automation.fellowship_state();
        if !enabled || !state.is_in_fellowship {
            self.status = if enabled {
                "Not in a fellowship"
            } else {
                "Fellow manager disabled"
            }
            .to_owned();
            if !state.is_in_fellowship {
                self.reset_social_state();
            }
            return;
        }

        let is_leader = state.leader_id == automation.character().id;
        if self.was_leader && !is_leader {
            if !self.votes.is_empty() {
                Self::fellow(
                    automation,
                    "[VT Fellow Manager] I am no longer the fellowship leader. All votes have been canceled.  -v-",
                );
            }
            self.votes.clear();
            self.waiting.clear();
            self.banned.clear();
        }
        self.was_leader = is_leader;

        let roster = automation.fellowship_roster();
        self.remove_joined_players(&roster);
        self.expire_votes(automation, is_leader);
        self.expire_waiting_players(automation);
        if is_leader {
            self.recruit_next(automation);
        }
        self.status = if is_leader {
            format!(
                "Fellow leader \u{2014} {} waiting, {} vote(s)",
                self.waiting.len(),
                self.votes.len()
            )
        } else {
            format!(
                "Fellow member \u{2014} leader {}",
                Self::leader_name(automation)
            )
        };
    }

    /// Back to idle: everything forgotten.
    pub fn reset(&mut self) {
        self.chat_sequence = 0;
        self.now = 0.0;
        self.next_recruit_at = 0.0;
        self.next_vote_id = 1;
        self.was_leader = false;
        self.reset_social_state();
        self.tell_rate.clear();
        "Fellow manager idle".clone_into(&mut self.status);
    }

    fn handle_tell(&mut self, automation: &mut dyn FellowshipAutomation, message: &ChatLine) {
        let sender = trim(&message.sender).to_owned();
        let command = trim(&message.text).to_owned();
        if sender.is_empty() || command.is_empty() || self.is_spam(&sender) {
            return;
        }

        let roster = automation.fellowship_roster();
        let is_member = roster.iter().any(|member| same_name(&member.name, &sender));
        let state = automation.fellowship_state();
        let is_leader = state.leader_id == automation.character().id;

        if same_name(&command, "xp") {
            self.request_recruit(automation, &sender, message.sender_id, &roster, is_leader);
            return;
        }
        if same_name(&command, "line")
            || same_name(&command, "list")
            || same_name(&command, "status")
        {
            self.send_line_status(automation, &sender, is_leader);
            return;
        }
        if same_name(&command, "remove") {
            self.remove_waiting(&sender);
            Self::tell(
                automation,
                &sender,
                "[VT Fellow Manager] You have been removed from the list.  -v-",
            );
            return;
        }
        if same_name(&command, "leader") {
            let openness = if state.is_open { "open" } else { "closed" };
            let text = if is_leader {
                format!(
                    "[VT Fellow Manager] I am the fellowship leader. The fellowship is {openness}.  -v-"
                )
            } else {
                format!(
                    "[VT Fellow Manager] The leader is currently: {}. The fellowship is {openness}.  -v-",
                    Self::leader_name(automation)
                )
            };
            Self::tell(automation, &sender, &text);
            return;
        }
        if same_name(&command, "help") {
            Self::tell(
                automation,
                &sender,
                "[VT Fellow Manager] Available commands: xp, line, remove, leader, startvote, vote, location, help  -v-",
            );
            return;
        }
        if same_name(&command, "help startvote") {
            Self::tell(
                automation,
                &sender,
                "[VT Fellow Manager] Usage: startvote [votetype] [parameter]. Possible vote types: kick, ban, giveleader, setopen.  -v-",
            );
            return;
        }
        if same_name(&command, "help vote") {
            Self::tell(
                automation,
                &sender,
                "[VT Fellow Manager] Usage: vote [vote id] [yes/no]  -v-",
            );
            return;
        }
        if ordinal::starts_with_ignore_case(&command, "startvote ") {
            self.start_vote(automation, &sender, &command, &roster, is_member, is_leader);
            return;
        }
        if ordinal::starts_with_ignore_case(&command, "vote ") {
            self.cast_vote(automation, &sender, &command, is_member);
            return;
        }
        if same_name(&command, "location") {
            let text = if is_member {
                format!(
                    "[VT Fellow Manager] I am currently located in landcell: {:08X}  -v-",
                    automation.navigation_snapshot().position.cell_id
                )
            } else {
                "[VT Fellow Manager] Sorry, I can only send my location to members of the fellowship.  -v-"
                    .to_owned()
            };
            Self::tell(automation, &sender, &text);
        }
    }

    fn request_recruit(
        &mut self,
        automation: &mut dyn FellowshipAutomation,
        sender: &str,
        sender_object_id: u32,
        roster: &[FellowMember],
        is_leader: bool,
    ) {
        if roster.iter().any(|member| same_name(&member.name, sender)) {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] You are already in the fellowship.  -v-",
            );
            return;
        }
        if self.banned.iter().any(|name| same_name(name, sender)) {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] Sorry, but you have been banned from this fellowship.  -v-",
            );
            return;
        }
        let state = automation.fellowship_state();
        if !is_leader && !state.is_open {
            let text = format!(
                "[VT Fellow Manager] I'm sorry, but the fellowship is closed and I am not the leader. The leader is currently: {}  -v-",
                Self::leader_name(automation)
            );
            Self::tell(automation, sender, &text);
            return;
        }
        if let Some(position) = self
            .waiting
            .iter()
            .position(|player| same_name(&player.name, sender))
        {
            let existing = &mut self.waiting[position];
            if sender_object_id != 0 {
                existing.object_id = sender_object_id;
            }
            existing.expires_at = self.now + REQUEST_LIFETIME_SECONDS;
            let text = format!(
                "[VT Fellow Manager] You are already number {} of {} on the waiting list.  -v-",
                position + 1,
                self.waiting.len()
            );
            Self::tell(automation, sender, &text);
            return;
        }

        self.waiting.push(WaitingPlayer {
            name: sender.to_owned(),
            object_id: sender_object_id,
            expires_at: self.now + REQUEST_LIFETIME_SECONDS,
            attempts: 0,
        });
        if is_leader && roster.len() > MAXIMUM_OTHER_MEMBERS {
            self.desired_open = state.is_open;
            let _ = automation.fellowship_set_open(false);
            let text = format!(
                "[VT Fellow Manager] The fellow is full, and I am the leader. I am adding you to the waiting list at position {}  -v-",
                self.waiting.len()
            );
            Self::tell(automation, sender, &text);
        } else {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] I will recruit you in a moment. Please stand close to me.  -v-",
            );
        }
    }

    fn recruit_next(&mut self, automation: &mut dyn FellowshipAutomation) {
        if self.waiting.is_empty() {
            if automation.fellowship_state().is_open != self.desired_open {
                let _ = automation.fellowship_set_open(self.desired_open);
            }
            return;
        }
        if automation.fellowship_roster().len() > MAXIMUM_OTHER_MEMBERS {
            if automation.fellowship_state().is_open {
                let _ = automation.fellowship_set_open(false);
            }
            return;
        }
        if self.now < self.next_recruit_at {
            return;
        }
        let object_id = self.waiting[0].object_id;
        if object_id == 0 || !Self::is_near(automation, object_id) {
            let player = &mut self.waiting[0];
            player.attempts += 1;
            self.next_recruit_at = self.now + 1.0;
            let name = player.name.clone();
            let attempts = player.attempts;
            if attempts == 16 {
                Self::tell(
                    automation,
                    &name,
                    "[VT Fellow Manager] You are too far away. I will wait 20 seconds and give you one more chance.  -v-",
                );
            }
            if attempts > 30 {
                Self::tell(
                    automation,
                    &name,
                    "[VT Fellow Manager] I'm sorry, but I couldn't recruit you. Please try again.  -v-",
                );
                self.waiting.remove(0);
            }
            return;
        }
        let result = automation.fellowship_recruit(object_id);
        self.next_recruit_at = self.now + 1.0;
        if result != FellowshipStatus::Accepted {
            self.waiting[0].attempts += 1;
        }
    }

    fn start_vote(
        &mut self,
        automation: &mut dyn FellowshipAutomation,
        sender: &str,
        command: &str,
        roster: &[FellowMember],
        is_member: bool,
        is_leader: bool,
    ) {
        if !is_member || self.banned.iter().any(|name| same_name(name, sender)) {
            return;
        }
        if !is_leader {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] I am not the fellowship leader and cannot manage votes.  -v-",
            );
            return;
        }
        if self
            .vote_cooldowns
            .iter()
            .any(|(name, ready_at)| same_name(name, sender) && *ready_at > self.now)
        {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] You have initiated a vote too recently.  -v-",
            );
            return;
        }
        let parts = split_words(command, 3);
        if parts.len() != 3 {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] Not enough parameters to startvote command. Tell me 'help startvote' for more information.  -v-",
            );
            return;
        }
        let kind_text = casing::to_lower_invariant(parts[1]);
        let mut parameter = trim(parts[2]).to_owned();
        let kind = match kind_text.as_str() {
            "kick" | "ban" | "giveleader" => {
                if !roster
                    .iter()
                    .any(|member| same_name(&member.name, &parameter))
                {
                    let text = format!(
                        "[VT Fellow Manager] Cannot vote to {kind_text} {parameter}, that player is not in the fellow.  -v-"
                    );
                    Self::tell(automation, sender, &text);
                    return;
                }
                match kind_text.as_str() {
                    "kick" => FellowVoteKind::Kick,
                    "ban" => FellowVoteKind::Ban,
                    _ => FellowVoteKind::GiveLeader,
                }
            }
            "setopen" if parse_bool(&parameter).is_some() => {
                parameter = casing::to_lower_invariant(&parameter);
                FellowVoteKind::SetOpen
            }
            _ => {
                Self::tell(
                    automation,
                    sender,
                    "[VT Fellow Manager] Unknown vote type. Tell me 'help startvote' for more information.  -v-",
                );
                return;
            }
        };
        if self
            .votes
            .iter()
            .any(|vote| vote.kind == kind && same_name(&vote.parameter, &parameter))
        {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] An identical vote is already in progress!  -v-",
            );
            return;
        }
        let mut vote = FellowVote {
            id: self.next_vote_id,
            kind,
            parameter: parameter.clone(),
            expires_at: self.now + VOTE_LIFETIME_SECONDS,
            ballots: Vec::new(),
        };
        self.next_vote_id += 1;
        vote.cast(sender, true);
        let id = vote.id;
        self.votes.push(vote.clone());
        let ready_at = self.now + VOTE_CALLER_COOLDOWN_SECONDS;
        match self
            .vote_cooldowns
            .iter_mut()
            .find(|(name, _)| same_name(name, sender))
        {
            Some(cooldown) => cooldown.1 = ready_at,
            None => self.vote_cooldowns.push((sender.to_owned(), ready_at)),
        }
        let text = format!(
            "[VT Fellow Manager] {sender} has called a new vote: {kind_text} {parameter}! To vote, tell me 'vote {id} yes' or 'vote {id} no'. You have 2 minutes.  -v-"
        );
        Self::fellow(automation, &text);
        Self::announce_vote(automation, &vote);
    }

    fn cast_vote(
        &mut self,
        automation: &mut dyn FellowshipAutomation,
        sender: &str,
        command: &str,
        is_member: bool,
    ) {
        if !is_member || self.banned.iter().any(|name| same_name(name, sender)) {
            return;
        }
        let parts = split_words(command, usize::MAX);
        let id = if parts.len() == 3 {
            parse_i32(parts[1], NumberStyles::INTEGER)
        } else {
            None
        };
        let Some(id) = id.filter(|_| same_name(parts[2], "yes") || same_name(parts[2], "no"))
        else {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] Invalid vote command. Votes should look like: vote idnumber yes, or: vote idnumber no  -v-",
            );
            return;
        };
        let Some(vote) = self.votes.iter_mut().find(|vote| vote.id == id) else {
            Self::tell(
                automation,
                sender,
                "[VT Fellow Manager] Invalid vote ID number. Votes should look like: vote idnumber yes, or: vote idnumber no  -v-",
            );
            return;
        };
        vote.cast(sender, same_name(parts[2], "yes"));
        let vote = vote.clone();
        Self::announce_vote(automation, &vote);
    }

    fn expire_votes(&mut self, automation: &mut dyn FellowshipAutomation, is_leader: bool) {
        let (expired, kept): (Vec<FellowVote>, Vec<FellowVote>) = self
            .votes
            .drain(..)
            .partition(|vote| vote.expires_at <= self.now);
        self.votes = kept;
        for vote in expired {
            let (yes, no) = vote.tally();
            let passed = yes > (yes + no) / 2;
            let text = format!(
                "[VT Fellow Manager] Vote {} {} ({yes}/{no}).  -v-",
                vote.description(),
                if passed { "passed" } else { "failed" }
            );
            Self::fellow(automation, &text);
            if passed && is_leader {
                self.execute_vote(automation, &vote);
            }
        }
    }

    fn execute_vote(&mut self, automation: &mut dyn FellowshipAutomation, vote: &FellowVote) {
        let target = automation
            .fellowship_roster()
            .into_iter()
            .find(|member| same_name(&member.name, &vote.parameter))
            .unwrap_or_default();
        match vote.kind {
            FellowVoteKind::Kick if target.object_id != 0 => {
                let _ = automation.fellowship_dismiss(target.object_id);
            }
            FellowVoteKind::Ban if target.object_id != 0 => {
                if !self.banned.iter().any(|name| same_name(name, &target.name)) {
                    self.banned.push(target.name.clone());
                }
                let _ = automation.fellowship_dismiss(target.object_id);
            }
            FellowVoteKind::GiveLeader if target.object_id != 0 => {
                let _ = automation.fellowship_assign_leader(target.object_id);
            }
            FellowVoteKind::SetOpen => {
                self.desired_open = parse_bool(&vote.parameter).unwrap_or(self.desired_open);
                let _ = automation.fellowship_set_open(self.desired_open);
            }
            _ => {}
        }
    }

    fn send_line_status(
        &mut self,
        automation: &mut dyn FellowshipAutomation,
        sender: &str,
        is_leader: bool,
    ) {
        if !is_leader {
            let state = automation.fellowship_state();
            let text = format!(
                "[VT Fellow Manager] The leader is currently: {}. The fellowship is {}.  -v-",
                Self::leader_name(automation),
                if state.is_open { "open" } else { "closed" }
            );
            Self::tell(automation, sender, &text);
            return;
        }
        let Some(position) = self
            .waiting
            .iter()
            .position(|player| same_name(&player.name, sender))
        else {
            let text = if self.waiting.is_empty() {
                format!(
                    "[VT Fellow Manager] There is no waiting list. The fellowship has {} members.  -v-",
                    automation.fellowship_roster().len()
                )
            } else {
                format!(
                    "[VT Fellow Manager] The waiting list contains {} players. You are not on it.  -v-",
                    self.waiting.len()
                )
            };
            Self::tell(automation, sender, &text);
            return;
        };
        let text = format!(
            "[VT Fellow Manager] You are number {} of {} on the waiting list.  -v-",
            position + 1,
            self.waiting.len()
        );
        Self::tell(automation, sender, &text);
    }

    fn remove_joined_players(&mut self, roster: &[FellowMember]) {
        let in_roster = |name: &str| roster.iter().any(|member| same_name(&member.name, name));
        self.waiting.retain(|waiting| !in_roster(&waiting.name));
        for vote in &mut self.votes {
            vote.ballots.retain(|(voter, _)| in_roster(voter));
        }
    }

    fn expire_waiting_players(&mut self, automation: &mut dyn FellowshipAutomation) {
        let now = self.now;
        let (expired, kept): (Vec<WaitingPlayer>, Vec<WaitingPlayer>) = self
            .waiting
            .drain(..)
            .partition(|player| player.expires_at <= now);
        self.waiting = kept;
        for player in expired {
            Self::tell(
                automation,
                &player.name,
                "[VT Fellow Manager] Your spot in the fellowship has expired. You have been removed from the list.  -v-",
            );
        }
    }

    fn is_near(automation: &mut dyn FellowshipAutomation, object_id: u32) -> bool {
        let own = automation.navigation_snapshot();
        own.is_available
            && automation
                .navigation_object(object_id)
                .is_some_and(|target| {
                    own.position.horizontal_distance_metres(&target.position)
                        <= RECRUIT_RANGE_METERS
                })
    }

    fn is_spam(&mut self, sender: &str) -> bool {
        let now = self.now;
        let index = match self
            .tell_rate
            .iter()
            .position(|(name, _)| same_name(name, sender))
        {
            Some(index) => index,
            None => {
                self.tell_rate.push((sender.to_owned(), VecDeque::new()));
                self.tell_rate.len() - 1
            }
        };
        let times = &mut self.tell_rate[index].1;
        while times.front().is_some_and(|time| *time <= now - 180.0) {
            times.pop_front();
        }
        times.push_back(now);
        times.len() > 8
    }

    fn leader_name(automation: &mut dyn FellowshipAutomation) -> String {
        let leader = automation.fellowship_state().leader_id;
        automation
            .fellowship_roster()
            .into_iter()
            .find(|member| member.object_id == leader)
            .map(|member| member.name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "????".to_owned())
    }

    fn remove_waiting(&mut self, name: &str) {
        self.waiting.retain(|player| !same_name(&player.name, name));
    }

    fn announce_vote(automation: &mut dyn FellowshipAutomation, vote: &FellowVote) {
        let (yes, no) = vote.tally();
        let text = format!(
            "[VT Fellow Manager] Vote total for {}: {yes}/{no}  -v-",
            vote.description()
        );
        Self::fellow(automation, &text);
    }

    fn tell(automation: &mut dyn FellowshipAutomation, player: &str, text: &str) {
        let _ = automation.submit_chat(&format!("/t {player}, {text}"));
    }

    fn fellow(automation: &mut dyn FellowshipAutomation, text: &str) {
        let _ = automation.submit_chat(&format!("/f {text}"));
    }

    fn reset_social_state(&mut self) {
        self.waiting.clear();
        self.banned.clear();
        self.vote_cooldowns.clear();
        self.votes.clear();
        self.was_leader = false;
    }
}
