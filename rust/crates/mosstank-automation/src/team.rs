//! The team as one list: the fellows and the owner's other characters the
//! peer network reports (on this computer, or imported from another), one
//! entry per character. A fellow who is also a network peer is listed
//! once, as a fellow, with the network's place for it when the world does
//! not show it. `/vt team` and the team overlay read the same list.

use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::{METRES_PER_MAP_UNIT, Position};

/// One character of the team.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamMember {
    /// Its character's object id.
    pub player_id: u32,
    /// The network's number for its client, 0 when the network does not
    /// report it.
    pub client_id: u32,
    /// Its name.
    pub name: String,
    /// In this character's fellowship.
    pub fellow: bool,
    /// Reported by the peer network.
    pub peer: bool,
    /// The network's record was imported from another computer.
    pub imported: bool,
    /// Health now and at most.
    pub health: (u32, u32),
    /// Stamina now and at most.
    pub stamina: (u32, u32),
    /// Mana now and at most.
    pub mana: (u32, u32),
    /// Seconds since a fellow's vitals were last sampled live, if ever;
    /// `None` for a peer, whose record the network keeps fresh.
    pub vitals_age_seconds: Option<f64>,
    /// Where it stands: where the world shows it, else where the network
    /// last said; `None` when neither knows.
    pub position: Option<Position>,
    /// How far away, metres, as the fellowship roster says, when it says
    /// (a fellow the client sees).
    pub roster_distance: Option<f32>,
}

impl TeamMember {
    /// How far from `here`, metres: measured to where it stands, else the
    /// fellowship roster's distance; `None` when neither is known.
    #[must_use]
    pub fn distance_from(&self, here: Option<&Position>) -> Option<f64> {
        match (here, &self.position) {
            (Some(here), Some(there)) => Some(distance_metres(here, there)),
            _ => self.roster_distance.map(f64::from),
        }
    }
}

/// The team: the fellows in the roster's order, then the network's other
/// characters in the order it reports them; never this character (`own_id`)
/// and never one character twice. `seen` is where the world shows a
/// character, if it does.
pub fn roster(
    own_id: u32,
    fellows: &[FellowMember],
    peers: &[NetworkClient],
    seen: &mut dyn FnMut(u32) -> Option<Position>,
) -> Vec<TeamMember> {
    let mut members: Vec<TeamMember> = Vec::new();
    for fellow in fellows {
        if fellow.object_id == 0
            || fellow.object_id == own_id
            || members.iter().any(|m| m.player_id == fellow.object_id)
        {
            continue;
        }
        members.push(TeamMember {
            player_id: fellow.object_id,
            client_id: 0,
            name: fellow.name.clone(),
            fellow: true,
            peer: false,
            imported: false,
            health: (fellow.current_health, fellow.max_health),
            stamina: (fellow.current_stamina, fellow.max_stamina),
            mana: (fellow.current_mana, fellow.max_mana),
            vitals_age_seconds: fellow.vitals_age_seconds,
            position: seen(fellow.object_id),
            roster_distance: fellow.distance.is_finite().then_some(fellow.distance),
        });
    }
    for peer in peers {
        if peer.player_id == 0 || peer.player_id == own_id {
            continue;
        }
        if let Some(member) = members.iter_mut().find(|m| m.player_id == peer.player_id) {
            if !member.peer {
                member.peer = true;
                member.client_id = peer.client_id;
                member.imported = peer.is_remote;
                if member.position.is_none() {
                    member.position = Some(peer.position.clone());
                }
                // A fellow whose vitals the fellowship has not sampled
                // reads the network's.
                if member.vitals_age_seconds.is_none() {
                    member.health = (peer.current_health, peer.max_health);
                    member.stamina = (peer.current_stamina, peer.max_stamina);
                    member.mana = (peer.current_mana, peer.max_mana);
                }
            }
            continue;
        }
        members.push(TeamMember {
            player_id: peer.player_id,
            client_id: peer.client_id,
            name: peer.name.clone(),
            fellow: false,
            peer: true,
            imported: peer.is_remote,
            health: (peer.current_health, peer.max_health),
            stamina: (peer.current_stamina, peer.max_stamina),
            mana: (peer.current_mana, peer.max_mana),
            vitals_age_seconds: None,
            position: Some(seen(peer.player_id).unwrap_or_else(|| peer.position.clone())),
            roster_distance: None,
        });
    }
    members
}

/// How far apart two places are, metres: across the ground and up
/// together.
#[must_use]
pub fn distance_metres(from: &Position, to: &Position) -> f64 {
    let east = to.east_west - from.east_west;
    let north = to.north_south - from.north_south;
    let up = to.elevation - from.elevation;
    (east * east + north * north + up * up).sqrt() * METRES_PER_MAP_UNIT
}

/// A vital as a whole percentage, `None` when its maximum is unknown.
#[must_use]
pub fn percent((current, maximum): (u32, u32)) -> Option<u32> {
    if maximum == 0 {
        return None;
    }
    let fraction = f64::from(current.min(maximum)) / f64::from(maximum);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((fraction * 100.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fellow(id: u32, name: &str, age: Option<f64>) -> FellowMember {
        FellowMember {
            object_id: id,
            name: name.to_owned(),
            current_health: 50,
            max_health: 100,
            distance: f32::INFINITY,
            vitals_age_seconds: age,
            ..FellowMember::default()
        }
    }

    fn peer(id: u32, name: &str, remote: bool) -> NetworkClient {
        NetworkClient {
            player_id: id,
            name: name.to_owned(),
            current_health: 20,
            max_health: 100,
            is_remote: remote,
            position: Position {
                east_west: 1.0,
                ..Position::default()
            },
            ..NetworkClient::default()
        }
    }

    #[test]
    fn a_fellow_who_is_also_a_peer_is_listed_once_with_both_marks() {
        let members = roster(
            1,
            &[fellow(2, "Both", Some(1.0)), fellow(1, "Me", None)],
            &[
                peer(2, "Both", true),
                peer(3, "Net", false),
                peer(1, "Me", false),
            ],
            &mut |_| None,
        );
        assert_eq!(members.len(), 2);
        assert!(members[0].fellow && members[0].peer && members[0].imported);
        // The fellowship's sampled vitals are kept.
        assert_eq!(members[0].health, (50, 100));
        // The network's place stands in for one the world does not show.
        assert!(members[0].position.is_some());
        assert!(!members[1].fellow && members[1].peer && !members[1].imported);
    }

    #[test]
    fn a_fellow_never_sampled_reads_the_networks_vitals() {
        let members = roster(
            1,
            &[fellow(2, "Both", None)],
            &[peer(2, "Both", false)],
            &mut |_| None,
        );
        assert_eq!(members[0].health, (20, 100));
    }

    #[test]
    fn a_fellow_out_of_sight_and_off_the_network_has_no_distance() {
        let members = roster(1, &[fellow(2, "Far", Some(30.0))], &[], &mut |_| None);
        assert_eq!(members[0].distance_from(Some(&Position::default())), None);
    }
}
