//! Every setting the UB tab offers, as authored. The display tools nest
//! their settings three deep; the names are flattened to dotted paths, and
//! the repeated shapes (a nametag, a map colour, a map marker) are expanded
//! from one helper each so a shape cannot drift between its uses. Window
//! positions are deliberately absent: the host keeps where a window sits.

use std::collections::HashMap;
use std::sync::LazyLock;

use mosstank_dotnet::{eq_ignore_case, to_upper_invariant};

use super::{UbSettingDefinition, UbSettingKind, UbSettingScope, UbSettingValue};
use crate::defaults::VtankEnumValue;

/// The name the plugin's debug switch is kept under.
pub const DEBUG_SETTING: &str = "Plugin.Debug";

/// The settings profile a character opens until told otherwise.
pub const DEFAULT_PROFILE_NAME: &str = "default";

/// The kinds of line the plugin prints in chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbChatKind {
    /// An ordinary line.
    Generic,
    /// A debug line.
    Debug,
    /// An expression's output.
    Expression,
    /// An error.
    Error,
}

impl UbChatKind {
    fn name(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Debug => "Debug",
            Self::Expression => "Expression",
            Self::Error => "Error",
        }
    }
}

/// The text class of an ordinary line (System).
pub const GENERIC_CHAT_TYPE: i32 = 0x05;
/// The text class of a debug line (Abuse).
pub const DEBUG_CHAT_TYPE: i32 = 0x0E;
/// The text class of an error line (Help).
pub const ERROR_CHAT_TYPE: i32 = 0x0F;

/// Whether a kind of line is shown in chat, and the class it prints in, as
/// shipped.
#[must_use]
pub fn default_display(kind: UbChatKind) -> (bool, i32) {
    let chat_type = match kind {
        UbChatKind::Debug => DEBUG_CHAT_TYPE,
        UbChatKind::Error => ERROR_CHAT_TYPE,
        _ => GENERIC_CHAT_TYPE,
    };
    (true, chat_type)
}

/// The name a kind's display settings are kept under
/// (`Plugin.ErrorMessageDisplay`).
#[must_use]
pub fn display_setting(kind: UbChatKind) -> String {
    format!("Plugin.{}MessageDisplay", kind.name())
}

/// The client's text classes by the names macro authors know them by,
/// ascending (0x01 and 0x1A are unused).
pub const CHAT_MESSAGE_TYPES: [(&str, i32); 31] = [
    ("Broadcast", 0x00),
    ("Speech", 0x02),
    ("Tell", 0x03),
    ("OutgoingTell", 0x04),
    ("System", 0x05),
    ("Combat", 0x06),
    ("Magic", 0x07),
    ("Channel", 0x08),
    ("ChannelSend", 0x09),
    ("Social", 0x0A),
    ("SocialSend", 0x0B),
    ("Emote", 0x0C),
    ("Advancement", 0x0D),
    ("Abuse", 0x0E),
    ("Help", 0x0F),
    ("Appraisal", 0x10),
    ("Spellcasting", 0x11),
    ("Allegiance", 0x12),
    ("Fellowship", 0x13),
    ("WorldBroadcast", 0x14),
    ("CombatEnemy", 0x15),
    ("CombatSelf", 0x16),
    ("Recall", 0x17),
    ("Craft", 0x18),
    ("Salvaging", 0x19),
    ("General", 0x1B),
    ("Trade", 0x1C),
    ("LFG", 0x1D),
    ("Roleplay", 0x1E),
    ("AdminTell", 0x1F),
    ("Olthoi", 0x20),
];

/// How a spend of experience is shared out until somebody says otherwise:
/// a weight (a ratio, not an amount) for all six attributes, three vitals
/// and thirty-eight skills. What ships is a fighter's.
pub const DEFAULT_XP_POLICY: [&str; 47] = [
    "Strength = 1",
    "Endurance = 1",
    "Coordination = 1",
    "Quickness = 1",
    "Focus = 1",
    "Self = 1",
    "Health = 1.4",
    "Stamina = 0.1",
    "Mana = 0.1",
    "Alchemy = 0",
    "ArcaneLore = 0.1",
    "ArmorTinkering = 0",
    "AssessCreature = 0",
    "AssessPerson = 0",
    "Cooking = 0",
    "CreatureEnchantment = 0.2",
    "Deception = 0.1",
    "DirtyFighting = 0.1",
    "DualWield = 0.1",
    "FinesseWeapons = 10",
    "Fletching = 0.1",
    "Healing = 0.1",
    "HeavyWeapons = 10",
    "ItemEnchantment = 0.2",
    "ItemTinkering = 0",
    "Jump = 0.02",
    "Leadership = 0.1",
    "LifeMagic = 1",
    "LightWeapons = 10",
    "Lockpick = 0",
    "Loyalty = 0.1",
    "MagicDefense = 0.1",
    "MagicItemTinkering = 0",
    "ManaConversion = 0.1",
    "MeleeDefense = 5",
    "MissileDefense = 5",
    "MissileWeapons = 10",
    "Recklessness = 0.1",
    "Run = 0.1",
    "Salvaging = 0.1",
    "Shield = 0",
    "SneakAttack = 0.1",
    "Summoning = 1",
    "TwoHandedCombat = 10",
    "VoidMagic = 10",
    "WarMagic = 10",
    "WeaponTinkering = 0",
];

/// The names the original gives settings that are called something else
/// here, so a macro using the original's spelling reaches the same row.
pub const ORIGINAL_SPELLINGS: [(&str, &str); 6] = [
    ("VTank.VitalSharing", "Sharing.Vitals"),
    ("InventoryManager.IGThink", "ItemGiver.Think"),
    ("InventoryManager.IGFailure", "ItemGiver.FailureLimit"),
    ("InventoryManager.IGBusyCount", "ItemGiver.BusyRetryLimit"),
    ("InventoryManager.IGRange", "ItemGiver.Range"),
    ("GameEvents.GameEventHandlers", "GameEvents.Handlers"),
];

/// The name the original gives a row.
#[must_use]
pub fn original_name_of(name: &str) -> &str {
    ORIGINAL_SPELLINGS
        .iter()
        .find(|(_, row)| eq_ignore_case(row, name))
        .map_or(name, |(spelling, _)| spelling)
}

static ALL: LazyLock<Vec<UbSettingDefinition>> = LazyLock::new(build);
static ACCEPTED_ONLY: LazyLock<Vec<UbSettingDefinition>> = LazyLock::new(build_accepted_only);
static KINDS_BY_NAME: LazyLock<HashMap<String, UbSettingKind>> = LazyLock::new(|| {
    all()
        .iter()
        .chain(accepted_only())
        .map(|definition| (to_upper_invariant(&definition.name), definition.kind))
        .collect()
});

/// Every setting, grouped by tool and in page order.
#[must_use]
pub fn all() -> &'static [UbSettingDefinition] {
    &ALL
}

/// Names macros set or read that are not rows on the tab: settings of
/// tools this plugin does not have, or switches whose choice is already
/// made here. They are accepted so a macro carries on, and each reads back
/// what is actually in force.
#[must_use]
pub fn accepted_only() -> &'static [UbSettingDefinition] {
    &ACCEPTED_ONLY
}

/// The shape one name's value has; a name never heard of reads as text.
#[must_use]
pub fn kind_of(name: &str) -> UbSettingKind {
    KINDS_BY_NAME
        .get(&to_upper_invariant(name))
        .copied()
        .unwrap_or(UbSettingKind::String)
}

fn build_accepted_only() -> Vec<UbSettingDefinition> {
    vec![
        switch(
            "VTank.PatchExpressionEngine",
            "The extended expression functions; always on here.",
            true,
        ),
        switch(
            "InventoryManager.TreatStackAsSingleItem",
            "Count a stack as one item when handing over; always off here.",
            false,
        ),
        line_in(
            "Plugin.SettingsProfile",
            "The named settings profile this character has open.",
            DEFAULT_PROFILE_NAME,
            UbSettingScope::Character,
        ),
        switch(
            "Plugin.PortalThink",
            "Think to yourself when a portal is not found.",
            false,
        ),
        switch(
            "AutoSalvage.Think",
            "Kept for macros; there is no auto-salvage here.",
            false,
        ),
        switch(
            "AutoTrade.Think",
            "Kept for macros; there is no auto-trade here.",
            false,
        ),
        switch(
            "Looter.EnableChests",
            "Kept for macros; there is no chest looter here.",
            false,
        ),
        switch(
            "Plugin.VideoPatch",
            "Kept for macros; the world is always drawn.",
            false,
        ),
        switch(
            "Plugin.VideoPatchFocus",
            "Kept for macros; the world is always drawn.",
            false,
        ),
    ]
}

fn build() -> Vec<UbSettingDefinition> {
    let mut rows = Vec::new();
    add_nametags(&mut rows);
    add_dungeon_maps(&mut rows);
    add_landscape_maps(&mut rows);
    add_network_ui(&mut rows);
    add_behaviour_tools(&mut rows);
    rows
}

fn add_nametags(rows: &mut Vec<UbSettingDefinition>) {
    rows.push(switch_in(
        "Nametags.Enabled",
        "Draw a name above the creatures and objects around you.",
        true,
        UbSettingScope::Global,
    ));
    rows.push(decimal32(
        "Nametags.MaxRange",
        "How far away, in meters, a name is still drawn.",
        35.0,
    ));
    // Every kind of thing that can carry a name gets the same five controls.
    add_nametag(rows, "Player", "another player", 0xFF00_FFFF);
    add_nametag(rows, "Pet", "a summoned pet", 0xFF00_FFFF);
    add_nametag(
        rows,
        "AllegiancePlayer",
        "a player in your allegiance",
        0xFF00_FF00,
    );
    add_nametag(rows, "Portal", "a portal", 0xFF00_FF00);
    add_nametag(rows, "Npc", "a non-player character", 0xFFFF_FF00);
    add_nametag(rows, "Vendor", "a vendor", 0xFFFF_00FF);
    add_nametag(rows, "Monster", "a monster", 0xFFFF_0000);
}

fn add_nametag(rows: &mut Vec<UbSettingDefinition>, group: &str, subject: &str, color: u32) {
    let prefix = format!("Nametags.{group}.");
    rows.push(switch(
        &format!("{prefix}Enabled"),
        &format!("Draw a name over {subject}."),
        true,
    ));
    rows.push(colour(
        &format!("{prefix}TagColor"),
        &format!("The colour of the name over {subject}."),
        color,
    ));
    rows.push(decimal32(
        &format!("{prefix}TagSize"),
        &format!("The height of the name over {subject}, in meters."),
        0.15,
    ));
    rows.push(colour(
        &format!("{prefix}TickerColor"),
        &format!("The colour of the second line under {subject}'s name."),
        color,
    ));
    rows.push(decimal32(
        &format!("{prefix}TickerSize"),
        &format!("The height of the second line under {subject}'s name, in meters."),
        0.1,
    ));
}

fn add_dungeon_maps(rows: &mut Vec<UbSettingDefinition>) {
    rows.push(switch_in(
        "DungeonMaps.Enabled",
        "Draw a floor plan of the dungeon you are standing in.",
        true,
        UbSettingScope::Global,
    ));
    rows.push(switch_in(
        "DungeonMaps.Debug",
        "Report what the floor plan is drawing while it draws it.",
        false,
        UbSettingScope::Global,
    ));
    rows.push(switch(
        "DungeonMaps.DrawWhenClosed",
        "Keep drawing the floor plan while its own window is shut.",
        true,
    ));
    rows.push(switch(
        "DungeonMaps.ShowVisitedTiles",
        "Shade the parts of the floor you have already walked.",
        true,
    ));
    rows.push(colour(
        "DungeonMaps.VisitedTilesColor",
        "The tint laid over a part of the floor you have walked.",
        0xFFFF_96FF,
    ));
    rows.push(switch(
        "DungeonMaps.ShowCompass",
        "Draw a compass on the floor plan.",
        true,
    ));
    rows.push(whole(
        "DungeonMaps.Opacity",
        "How solid the floor plan is drawn, from 0 to 255.",
        16,
    ));
    rows.push(decimal32(
        "DungeonMaps.MapZoom",
        "How far the floor plan is zoomed in.",
        4.2,
    ));
    rows.push(whole(
        "DungeonMaps.LabelFontSize",
        "The height of a floor-plan label.",
        10,
    ));

    // Eight pieces of dungeon geometry, each with a switch and a colour.
    add_map_colour(
        rows,
        "DungeonName",
        "the dungeon's name across the top",
        0xFFFF_FFFF,
    );
    add_map_colour(rows, "Walls", "an outer wall", 0xFF00_007F);
    add_map_colour(rows, "InnerWalls", "a wall inside a room", 0xFF7F_BFFF);
    add_map_colour(rows, "RampedWalls", "a sloped wall", 0xFF4E_A6FF);
    add_map_colour(rows, "Stairs", "a staircase", 0xFF00_3F7F);
    add_map_colour(rows, "Floors", "the floor", 0xFF00_7FBF);
    add_map_colour(
        rows,
        "VisualNavStickyPoint",
        "a route's anchor point",
        0xFFAD_FF2F,
    );
    add_map_colour(
        rows,
        "VisualNavLines",
        "the line between two route points",
        0xFFFF_00FF,
    );

    // Eleven kinds of thing that can appear as a marker, each with the same
    // five controls.
    add_map_marker(rows, "You", "you", true, false, false, 0xFFFF_0000);
    add_map_marker(
        rows,
        "Others",
        "another player",
        true,
        true,
        true,
        0xFFFF_FFFF,
    );
    add_map_marker(
        rows,
        "Items",
        "an item on the ground",
        true,
        true,
        true,
        0xFFFF_FFFF,
    );
    add_map_marker(
        rows,
        "Monsters",
        "a monster",
        true,
        true,
        false,
        0xFFFF_A500,
    );
    add_map_marker(
        rows,
        "NPCs",
        "a non-player character",
        true,
        true,
        false,
        0xFFFF_FF00,
    );
    add_map_marker(
        rows,
        "MyCorpse",
        "your own corpse",
        true,
        true,
        true,
        0xFFFF_0000,
    );
    add_map_marker(
        rows,
        "OtherCorpses",
        "somebody else's corpse",
        false,
        true,
        false,
        0xFFF5_F5F5,
    );
    add_map_marker(rows, "Portals", "a portal", true, true, true, 0xFFFF_F0FF);
    add_map_marker(
        rows,
        "Containers",
        "a chest or other container",
        true,
        true,
        false,
        0xFFF4_A460,
    );
    add_map_marker(rows, "Doors", "a door", true, false, false, 0xFFA5_2A2A);
    add_map_marker(
        rows,
        "EverythingElse",
        "anything with no marker of its own",
        false,
        true,
        false,
        0xFFF5_F5F5,
    );
}

fn add_map_colour(rows: &mut Vec<UbSettingDefinition>, group: &str, subject: &str, color: u32) {
    let prefix = format!("DungeonMaps.Display.{group}.");
    rows.push(switch(
        &format!("{prefix}Enabled"),
        &format!("Draw {subject} on the floor plan."),
        true,
    ));
    rows.push(colour(
        &format!("{prefix}Color"),
        &format!("The colour {subject} is drawn in."),
        color,
    ));
}

#[allow(clippy::fn_params_excessive_bools)]
fn add_map_marker(
    rows: &mut Vec<UbSettingDefinition>,
    group: &str,
    subject: &str,
    enabled: bool,
    use_icon: bool,
    show_label: bool,
    color: u32,
) {
    let prefix = format!("DungeonMaps.Display.Markers.{group}.");
    rows.push(switch(
        &format!("{prefix}Enabled"),
        &format!("Mark {subject} on the floor plan."),
        enabled,
    ));
    rows.push(switch(
        &format!("{prefix}ShowLabel"),
        &format!("Write the name beside the mark for {subject}."),
        show_label,
    ));
    rows.push(switch(
        &format!("{prefix}UseIcon"),
        &format!("Use the icon rather than a dot for {subject}."),
        use_icon,
    ));
    rows.push(colour(
        &format!("{prefix}Color"),
        &format!("The colour the mark for {subject} is drawn in."),
        color,
    ));
    rows.push(whole(
        &format!("{prefix}Size"),
        &format!("How many pixels across the mark for {subject} is."),
        3,
    ));
}

fn add_landscape_maps(rows: &mut Vec<UbSettingDefinition>) {
    rows.push(switch_in(
        "LandscapeMaps.Enabled",
        "Draw a map of the countryside you are standing in.",
        true,
        UbSettingScope::Global,
    ));
    rows.push(whole(
        "LandscapeMaps.Opacity",
        "How solid the countryside map is drawn, from 0 to 255.",
        16,
    ));
}

fn add_network_ui(rows: &mut Vec<UbSettingDefinition>) {
    rows.push(switch_in(
        "NetworkUI.Enabled",
        "Show the panel listing your other characters on this machine.",
        false,
        UbSettingScope::Global,
    ));
    rows.push(switch_in(
        "NetworkUI.ShowHudWhenClosed",
        "Keep the readout on screen while the main window is shut.",
        true,
        UbSettingScope::Global,
    ));
    rows.push(lines(
        "NetworkUI.TrackedItems",
        "Items whose count is shown for every character, one name a line.",
        &[],
        UbSettingScope::Profile,
    ));
    rows.push(line_in(
        "NetworkUI.SelectedTag",
        "Which tag's characters the panel lists; All lists every one.",
        "All",
        UbSettingScope::Character,
    ));
    rows.push(lines(
        "Networking.Tags",
        "Names for this character that a broadcast can be aimed at, one a line.",
        &[],
        UbSettingScope::Character,
    ));
}

#[allow(clippy::too_many_lines)]
fn add_behaviour_tools(rows: &mut Vec<UbSettingDefinition>) {
    // The reference prints its debug lines only while this is on.
    rows.push(switch(DEBUG_SETTING, "Show debug messages.", false));
    // Each kind of UB line: whether it is shown in chat and its text class.
    for (kind, what) in [
        (UbChatKind::Generic, "generic"),
        (UbChatKind::Debug, "debug"),
        (UbChatKind::Expression, "expression"),
        (UbChatKind::Error, "error"),
    ] {
        let (show, chat_type) = default_display(kind);
        let display = display_setting(kind);
        rows.push(switch(
            &format!("{display}.Enabled"),
            &format!("Show plugin {what} messages in chat."),
            show,
        ));
        rows.push(UbSettingDefinition {
            choices: CHAT_MESSAGE_TYPES
                .iter()
                .map(|&(name, value)| VtankEnumValue {
                    value,
                    label: name.to_owned(),
                })
                .collect(),
            ..UbSettingDefinition::new(
                &format!("{display}.Color"),
                &format!("The text class plugin {what} messages are printed in."),
                UbSettingKind::Enum,
                UbSettingValue::from_choice(chat_type),
                UbSettingScope::Profile,
            )
        });
    }
    rows.push(switch(
        "Aliases.Enabled",
        "Rewrite what you type using your alias list.",
        true,
    ));
    rows.push(line_in(
        "Aliases.Profile",
        "Which alias set this character uses; [character] keeps a private one.",
        "[character]",
        UbSettingScope::Character,
    ));
    rows.push(lines(
        "Aliases.DefinedAliases",
        "The aliases themselves, as 'what you type = what is sent', one a line.",
        &[],
        UbSettingScope::Profile,
    ));

    rows.push(switch(
        "AutoVendor.Enabled",
        "Buy and sell at a vendor without being asked.",
        true,
    ));
    rows.push(switch(
        "AutoVendor.EnableBuying",
        "Let a vendor run buy what the profile wants.",
        true,
    ));
    rows.push(switch(
        "AutoVendor.EnableSelling",
        "Let a vendor run sell what the profile allows.",
        true,
    ));
    rows.push(switch(
        "AutoVendor.TestMode",
        "Say what would be bought and sold instead of doing it.",
        false,
    ));
    rows.push(switch(
        "AutoVendor.Think",
        "Think to yourself when a vendor run finishes.",
        false,
    ));
    rows.push(switch(
        "AutoVendor.ShowMerchantInfo",
        "Report what a vendor deals in when you walk up to one.",
        true,
    ));
    rows.push(switch(
        "AutoVendor.OnlyFromMainPack",
        "Sell only what is in the main pack, leaving side packs alone.",
        false,
    ));
    rows.push(whole(
        "AutoVendor.Tries",
        "How many times opening a vendor is attempted.",
        4,
    ));
    rows.push(whole(
        "AutoVendor.TriesTime",
        "How long to wait between attempts to open a vendor, in milliseconds.",
        5000,
    ));

    rows.push(whole(
        "AutoXp.StopBeforeMax",
        "How many levels short of the maximum to stop raising something.",
        10,
    ));
    rows.push(whole(
        "AutoXp.TriesTime",
        "How long to wait between one spend and the next, in milliseconds.",
        300,
    ));
    rows.push(whole(
        "AutoXp.MaxXpChunk",
        "The most experience to spend in a single request.",
        1_000_000_000,
    ));
    rows.push(lines(
        "AutoXp.Policy",
        "How experience is shared out, as 'target = weight', one a line.",
        &DEFAULT_XP_POLICY,
        UbSettingScope::Profile,
    ));

    rows.push(switch(
        "EquipmentManager.Think",
        "Think to yourself when an equipment set has finished going on.",
        false,
    ));

    rows.push(switch(
        "GameEvents.Enabled",
        "Run your own handlers when something happens.",
        true,
    ));
    rows.push(line_in(
        "GameEvents.Profile",
        "Which handler set this character uses; [character] keeps a private one.",
        "[character]",
        UbSettingScope::Character,
    ));
    rows.push(lines(
        "GameEvents.Handlers",
        "The handlers themselves, as 'event = what to run', one a line. They run only while the macro runs with meta enabled.",
        &[],
        UbSettingScope::Profile,
    ));

    rows.push(decimal64(
        "ItemGiver.Range",
        "How far off someone may stand and still be handed items, in meters.",
        15.0,
    ));
    rows.push(decimal64(
        "ItemGiver.Delay",
        "How long to wait between one hand-over and the next, in seconds.",
        0.0,
    ));
    rows.push(whole(
        "ItemGiver.BusyRetryLimit",
        "How many times one item is offered again before it is written off.",
        10,
    ));
    rows.push(whole(
        "ItemGiver.FailureLimit",
        "How many written-off items stop the whole hand-over.",
        3,
    ));
    rows.push(switch(
        "ItemGiver.Think",
        "Think to yourself when a hand-over has finished.",
        false,
    ));

    rows.push(decimal32(
        "AutoTinker.MinPercentage",
        "Minimum percentage required to perform tinker.",
        99.5,
    ));
    rows.push(whole(
        "AutoTinker.MaxTinks",
        "Max number of tinker attempts.",
        10,
    ));
    // The client cannot read which augmentations a character carries, and
    // Charmed Smith lifts an imbue's odds by five points.
    rows.push(switch_in(
        "AutoTinker.CharmedSmith",
        "You carry the Charmed Smith augmentation, which adds five points to every imbue's chance.",
        false,
        UbSettingScope::Character,
    ));

    rows.push(switch(
        "Jumper.PauseNav",
        "Hold the route still while a jump is in the air.",
        true,
    ));
    rows.push(switch(
        "Jumper.ThinkComplete",
        "Think to yourself when a jump lands.",
        false,
    ));
    rows.push(switch(
        "Jumper.ThinkFail",
        "Think to yourself when a jump does not happen.",
        false,
    ));
    rows.push(whole(
        "Jumper.Attempts",
        "How many times a jump is tried before giving up.",
        3,
    ));

    // One switch for both directions of sharing, as the original has it.
    rows.push(switch_in(
        "Sharing.Vitals",
        "Tell your other characters on this machine how you are doing and what you have cast, and take in what they cast.",
        false,
        UbSettingScope::Character,
    ));
    rows.push(switch_in(
        "Networking.ReceiveCommands",
        "Receive broadcast commands from trusted local clients in the same world.",
        true,
        UbSettingScope::Character,
    ));
    rows.push(line_in(
        "Sharing.CastTag",
        "Only take in casts from characters whose client carries this tag; blank takes them all.",
        "",
        UbSettingScope::Character,
    ));
}

fn switch(name: &str, summary: &str, value: bool) -> UbSettingDefinition {
    switch_in(name, summary, value, UbSettingScope::Profile)
}

fn switch_in(name: &str, summary: &str, value: bool, scope: UbSettingScope) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Bool,
        UbSettingValue::from_bool(value),
        scope,
    )
}

fn whole(name: &str, summary: &str, value: i32) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Int,
        UbSettingValue::from_int(value),
        UbSettingScope::Profile,
    )
}

fn decimal32(name: &str, summary: &str, value: f32) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Single,
        UbSettingValue::from_single(value),
        UbSettingScope::Profile,
    )
}

fn decimal64(name: &str, summary: &str, value: f64) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Double,
        UbSettingValue::from_double(value),
        UbSettingScope::Profile,
    )
}

fn colour(name: &str, summary: &str, argb: u32) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Color,
        UbSettingValue::from_color(argb),
        UbSettingScope::Profile,
    )
}

fn line_in(name: &str, summary: &str, value: &str, scope: UbSettingScope) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::String,
        UbSettingValue::from_text(value),
        scope,
    )
}

fn lines(name: &str, summary: &str, value: &[&str], scope: UbSettingScope) -> UbSettingDefinition {
    UbSettingDefinition::new(
        name,
        summary,
        UbSettingKind::Collection,
        UbSettingValue::from_collection(value),
        scope,
    )
}
