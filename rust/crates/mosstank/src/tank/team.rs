//! `/vt team`: who is in the team (the fellows and the owner's other
//! characters the peer network reports), what the team has debuffed, and
//! why the helper and the target choice last did what they did. `/vt team
//! overlay [on|off]` switches the team overlay.

use mosstank_automation::team::{self, TeamMember};
use mosstank_dotnet::text::casing;
use mosstank_expr::ExpressionValue;
use mosstank_settings::options;

use super::{Core, TankCx};
use crate::client::TankClient;

/// The syntax line.
const SYNTAX: &str = "Syntax: /vt team [overlay [on|off]]";

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// `/vt team [overlay [on|off]]`.
pub(crate) fn team_command(cx: &mut TankCx<'_>, arguments: &str) {
    let words: Vec<String> = arguments
        .split_whitespace()
        .map(casing::to_lower_invariant)
        .collect();
    match words
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => report(cx),
        ["overlay"] => {
            let on = !cx.core.settings.vitals.team_overlay;
            set_overlay(cx, on);
        }
        ["overlay", "on"] => set_overlay(cx, true),
        ["overlay", "off"] => set_overlay(cx, false),
        _ => Core::write_vtank(cx.client, SYNTAX),
    }
}

/// Sets `TeamOverlay` and says what it is now.
fn set_overlay(cx: &mut TankCx<'_>, on: bool) {
    let _ = cx.set_meta_option(options::TEAM_OVERLAY, &ExpressionValue::Boolean(on));
    let now = cx.core.settings.vitals.team_overlay;
    Core::write_vtank(cx.client, &format!("Team overlay {}.", on_off(now)));
}

/// The team as the client reports it now.
pub(crate) fn members(client: &mut dyn TankClient) -> Vec<TeamMember> {
    let automation = client.rules();
    let own_id = automation.character().id;
    let fellows = if automation.in_fellowship() {
        automation.fellowship_members()
    } else {
        Vec::new()
    };
    let peers = if automation.network_available() {
        automation.network_clients()
    } else {
        Vec::new()
    };
    team::roster(own_id, &fellows, &peers, &mut |id| {
        automation.object(id).and_then(|object| object.position)
    })
}

/// One member in a line: name, fellow or peer, where it plays, health,
/// distance and how old its vitals are.
fn member_line(member: &TeamMember, here: Option<&ac_plugin_abi::world::Position>) -> String {
    let kind = match (member.fellow, member.peer) {
        (true, true) => "fellow and network peer",
        (true, false) => "fellow, not on the network",
        _ => "network peer",
    };
    let place = if !member.peer {
        ""
    } else if member.imported {
        ", imported"
    } else {
        ", this computer"
    };
    let health = team::percent(member.health)
        .map_or_else(|| "health unknown".to_owned(), |p| format!("health {p}%"));
    let distance = member.distance_from(here).map_or_else(
        || "not in range".to_owned(),
        |metres| format!("{metres:.0} m"),
    );
    let age = match member.vitals_age_seconds {
        Some(age) => format!(", vitals {age:.0} s old"),
        None if member.peer => ", vitals from the network".to_owned(),
        None => ", no vitals yet".to_owned(),
    };
    format!(
        "  {} ({kind}{place}): {health}, {distance}{age}",
        member.name
    )
}

/// The whole report.
fn report(cx: &mut TankCx<'_>) {
    let vitals = &cx.core.settings.vitals;
    let header = format!(
        "Team: HelpFellows {}, HelpNetworkPeers {}, TeamFocusDebuffed {}, TeamOverlay {}.",
        on_off(vitals.help_others),
        on_off(vitals.help_network_peers),
        on_off(cx.core.settings.combat.team_focus_debuffed),
        on_off(vitals.team_overlay),
    );
    let mut lines = vec![header];

    let members = members(cx.client);
    let snapshot = cx.client.rules().navigation_snapshot();
    let here = snapshot.is_available.then_some(snapshot.position);
    if members.is_empty() {
        lines.push("  Nobody else in the team (no fellows, no network peers).".to_owned());
    }
    for member in &members {
        lines.push(member_line(member, here.as_ref()));
    }

    let debuffs = cx.core.combat.team_debuffs();
    if debuffs.is_empty() {
        lines.push("Team debuffs: none in force.".to_owned());
    }
    for debuff in debuffs {
        let spell = spell_name(cx.client, debuff.spell_id);
        let monster = monster_name(&debuff.target_name, debuff.target_id);
        lines.push(format!(
            "Debuff: {} ({spell}) on {monster}, {:.0} s left",
            debuff.debuff, debuff.seconds_left
        ));
    }

    let helper = cx
        .core
        .vital_helper_recharge
        .last_helper_decision()
        .map_or_else(|| "none yet".to_owned(), ToString::to_string);
    lines.push(format!("Last helper decision: {helper}."));

    for line in lines {
        Core::write_vtank(cx.client, &line);
    }
}

fn spell_name(client: &mut dyn TankClient, spell_id: u32) -> String {
    client
        .rules()
        .spells()
        .try_get(spell_id)
        .map_or_else(|| format!("spell {spell_id}"), |spell| spell.name)
}

fn monster_name(name: &str, id: u32) -> String {
    if name.trim().is_empty() {
        format!("0x{id:08X}")
    } else {
        format!("{name} (0x{id:08X})")
    }
}
