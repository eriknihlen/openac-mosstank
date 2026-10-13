//! The usage lines `/ub help` prints (`UbCommandHelp`), one per UtilityBelt
//! command MossTank answers; the list is also every word `/ub` knows, which
//! the unknown-command answer guesses from. They repeat the reference's own
//! wording, because a macro written against those commands was written
//! against those words: the square brackets are the optional single-letter
//! flags, and the braces a choice of verbs. `/vt help <command>` has its own
//! short table ([`VTANK_ENTRIES`]) of MossTank's additions to the macro's
//! commands.

use openac_plugin_compat::text::ordinal;

/// One worked example of a command: what to type, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UbCommandExample {
    /// What to type.
    pub command: &'static str,
    /// What it does.
    pub description: &'static str,
}

/// One command's usage line, one-sentence summary and examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UbCommandUsage {
    /// The command word.
    pub name: &'static str,
    /// The usage line.
    pub usage: &'static str,
    /// What it does.
    pub summary: &'static str,
    /// The reference's worked examples, which its full help prints under
    /// "Examples:"; none for a command it gives none.
    pub examples: &'static [UbCommandExample],
}

/// The entry for a command name, ignoring letter case.
#[must_use]
pub fn find(entries: &'static [UbCommandUsage], name: &str) -> Option<&'static UbCommandUsage> {
    let text = name.trim();
    entries
        .iter()
        .find(|candidate| ordinal::equals_ignore_case(candidate.name, text))
}

/// The `/ub` entry for a command name, ignoring letter case.
#[must_use]
pub fn try_get(name: &str) -> Option<&'static UbCommandUsage> {
    find(ENTRIES, name)
}

/// The `/ub` entry for a command this table must carry.
///
/// # Panics
/// The table has no such entry: the verbs are this crate's constants.
#[must_use]
pub fn require(name: &str) -> &'static UbCommandUsage {
    try_get(name).unwrap_or_else(|| panic!("No usage line for {name}."))
}

/// The `/vt` entry for a command name, ignoring letter case.
#[must_use]
pub fn try_get_vtank(name: &str) -> Option<&'static UbCommandUsage> {
    find(VTANK_ENTRIES, name)
}

/// Edits between two words (insertions, deletions, substitutions, and a
/// swap of two neighbouring letters), the measure the reference uses to
/// guess at a mistyped command. The swap is priced as the reference prices
/// it, at the cost of the substitution in the same cell. Counted in UTF-16
/// code units, as .NET indexes a string.
#[must_use]
pub fn edit_distance(one: &str, two: &str) -> usize {
    let one: Vec<u16> = one.encode_utf16().collect();
    let two: Vec<u16> = two.encode_utf16().collect();
    let columns = two.len() + 1;
    let mut matrix = vec![0usize; (one.len() + 1) * columns];
    for row in 0..=one.len() {
        matrix[row * columns] = row;
    }
    for (column, cell) in matrix.iter_mut().enumerate().take(columns) {
        *cell = column;
    }
    for row in 1..=one.len() {
        for column in 1..=two.len() {
            let cost = usize::from(one[row - 1] != two[column - 1]);
            let mut distance = (matrix[row * columns + column - 1] + 1)
                .min(matrix[(row - 1) * columns + column] + 1)
                .min(matrix[(row - 1) * columns + column - 1] + cost);
            if row > 1
                && column > 1
                && one[row - 1] == two[column - 2]
                && one[row - 2] == two[column - 1]
            {
                distance = distance.min(matrix[(row - 2) * columns + column - 2] + cost);
            }
            matrix[row * columns + column] = distance;
        }
    }
    matrix[one.len() * columns + two.len()]
}

/// Every UtilityBelt command MossTank answers, in name order.
pub static ENTRIES: &[UbCommandUsage] = &[
    UbCommandUsage {
        name: "autocram",
        usage: "/ub autocram",
        summary: "Auto Cram into side packs.",
        examples: &[],
    },
    UbCommandUsage {
        name: "autostack",
        usage: "/ub autostack",
        summary: "Auto Stack your inventory.",
        examples: &[],
    },
    UbCommandUsage {
        name: "autotinker",
        usage: "/ub autotinker",
        summary: "Starts autotinker.",
        examples: &[],
    },
    UbCommandUsage {
        name: "autovendor",
        usage: "/ub autovendor <cancel|lootProfile>",
        summary: "Auto buy/sell from vendors.",
        examples: &[
            UbCommandExample {
                command: "/ub autovendor",
                description: "Loads VendorName.utl and starts the AutoVendor process.",
            },
            UbCommandExample {
                command: "/ub autovendor cancel",
                description: "Cancels the current autovendor session.",
            },
            UbCommandExample {
                command: "/ub autovendor recomp.utl",
                description: "Loads recomp.utl and starts the AutoVendor process.",
            },
        ],
    },
    UbCommandUsage {
        name: "bc",
        usage: "/ub bc [millisecondDelay] <command>",
        summary: "Broadcasts a command to all open clients, with optional millisecondDelay inbetween each.",
        examples: &[
            UbCommandExample {
                command: "/ub bc 5000 /say hello",
                description: "Runs \"/say hello\" on every client, with a 5000ms delay between each",
            },
            UbCommandExample {
                command: "/ub bc /say hello",
                description: "Runs \"/say hello\" on every client, with no delay",
            },
        ],
    },
    UbCommandUsage {
        name: "bct",
        usage: "/ub bct <tags> [millisecondDelay] <command>",
        summary: "Broadcasts a command to all clients with the specified comma-separated tags, with optional millisecondDelay inbetween each. Tags are managed with the Networking.Tags setting.",
        examples: &[
            UbCommandExample {
                command: "/ub bct one,two 5000 /say hello",
                description: "Runs \"/say hello\" on every client tagged `one` or `two`, with a 5000ms delay between each",
            },
            UbCommandExample {
                command: "/ub bct three /say hello",
                description: "Runs \"/say hello\" on every client tagged `three`, with no delay",
            },
            UbCommandExample {
                command: "/ub bct \"some tag\",\"another tag\" /say hello",
                description: "Runs \"/say hello\" on every client tagged `some tag` or `another tag`, with no delay",
            },
        ],
    },
    UbCommandUsage {
        name: "breakallegiance",
        usage: "/ub breakallegiance[p][ <name|id|selected>]",
        summary: "Break your allegiance to a character.",
        examples: &[
            UbCommandExample {
                command: "/ub breakallegiance Yonneh",
                description: "Break your Allegiance to `Yonneh` (noooooooooooo)",
            },
            UbCommandExample {
                command: "/ub breakallegiancep Yo",
                description: "Break Allegiance from a character with a name partially matching `Yo`.",
            },
            UbCommandExample {
                command: "/ub breakallegiance",
                description: "Break Allegiance from the closest character",
            },
            UbCommandExample {
                command: "/ub breakallegiance selected",
                description: "Break Allegiance from the selected character",
            },
        ],
    },
    UbCommandUsage {
        name: "calcdamage",
        usage: "/ub calcdamage",
        summary: "Calculates the buffed damage of the currently selected item. Only cantrip buffs are included in the calculation.",
        examples: &[UbCommandExample {
            command: "/ub calcdamage",
            description: "calcdamage",
        }],
    },
    UbCommandUsage {
        name: "clearbugged",
        usage: "/ub clearbugged",
        summary: "ID everything in your inventory, and remove bugged items.",
        examples: &[],
    },
    UbCommandUsage {
        name: "clearmotion",
        usage: "/ub clearmotion",
        summary: "Clears all wanted motions, in the client.",
        examples: &[UbCommandExample {
            command: "/ub clearmotion",
            description: "Clears all wanted motions, in the client.",
        }],
    },
    UbCommandUsage {
        name: "close",
        usage: "/ub close corpse",
        summary: "Close the open corpse.",
        examples: &[UbCommandExample {
            command: "/ub close corpse",
            description: "Close the corpse if one is open",
        }],
    },
    UbCommandUsage {
        name: "closestportal",
        usage: "/ub closestportal",
        summary: "Uses the closest portal.",
        examples: &[UbCommandExample {
            command: "/ub closestportal",
            description: "Uses the closest portal",
        }],
    },
    UbCommandUsage {
        name: "combatstate",
        usage: "/ub combatstate (peace|melee|missile|magic)",
        summary: "Sets combat state.",
        examples: &[],
    },
    UbCommandUsage {
        name: "count",
        usage: "/ub count {item <name> | profile <p> | player <range>}",
        summary: "Counts matching items, or the players around you.",
        examples: &[
            UbCommandExample {
                command: "/ub count Prismatic Taper",
                description: "Counts the total number of Prismatic Tapers in your inventory.",
            },
            UbCommandExample {
                command: "/ub count recomp.utl",
                description: "Counts the number of items matching recomp.utl in your inventory, thinking to yourself when finished",
            },
        ],
    },
    UbCommandUsage {
        name: "date",
        usage: "/ub date[utc] [format]",
        summary: "Prints the current date with an optional .NET format string.",
        examples: &[
            UbCommandExample {
                command: "/ub date hh:mm:ss tt",
                description: "Prints current local time '06:09:01 PM'",
            },
            UbCommandExample {
                command: "/ub dateutc dddd dd MMMM",
                description: "Prints current utc date 'Friday 29 August'",
            },
        ],
    },
    UbCommandUsage {
        name: "delay",
        usage: "/ub delay <millisecondDelay> <command>",
        summary: "Runs a command after a delay.",
        examples: &[UbCommandExample {
            command: "/ub delay 5000 /say hello",
            description: "Runs \"/say hello\" after a 3000ms delay (3 seconds)",
        }],
    },
    UbCommandUsage {
        name: "dumpskills",
        usage: "/ub dumpskills",
        summary: "Prints all skills and training levels to chat.",
        examples: &[UbCommandExample {
            command: "/ub dumpskills",
            description: "Prints all skills and training levels to chat",
        }],
    },
    UbCommandUsage {
        name: "equip",
        usage: "/ub equip {list | load [p] | test [p] | create [p]}",
        summary: "Manages equipment profiles.",
        examples: &[
            UbCommandExample {
                command: "/ub equip load profile.utl",
                description: "Equips all items matching profile.utl.",
            },
            UbCommandExample {
                command: "/ub equip list",
                description: "Lists available equipment profiles.",
            },
            UbCommandExample {
                command: "/ub equip test profile.utl",
                description: "Test equipping profile.utl",
            },
        ],
    },
    UbCommandUsage {
        name: "face",
        usage: "/ub face <heading>",
        summary: "Face heading commands with built in navigation pausing and retries.",
        examples: &[UbCommandExample {
            command: "/ub face 180",
            description: "Faces your character towards 180 degrees (south).",
        }],
    },
    UbCommandUsage {
        name: "fellow",
        usage: "/ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]",
        summary: "UB Fellowship Commands.",
        examples: &[],
    },
    UbCommandUsage {
        name: "follow",
        usage: "/ub follow[p] <name>",
        summary: "Sets a navigation route to follow a player.",
        examples: &[
            UbCommandExample {
                command: "/ub follow Zero Cool",
                description: "Sets a VTank nav route to follow \"Zero Cool\"",
            },
            UbCommandExample {
                command: "/ub followp Zero",
                description: "Sets a VTank nav route to follow a character with a name partially matching \"Zero\"",
            },
        ],
    },
    UbCommandUsage {
        name: "getjob",
        usage: "/ub getjob",
        summary: "Run this command to see tinker jobs currently in queue.",
        examples: &[],
    },
    UbCommandUsage {
        name: "give",
        usage: "/ub give[p{P|r}] [itemCount] <itemName> to <target>",
        summary: "Gives items matching the provided name to a player. Small p matches part of the item name, capital P part of the target's, and r reads the item name as a regular expression.",
        examples: &[
            UbCommandExample {
                command: "/ub givep 10 Prismatic to Zero Cool",
                description: "Gives 10 items partially matching the name \"Prismatic\" to Zero Cool",
            },
            UbCommandExample {
                command: "/ub giveP 10 Prismatic Tapers to Zero",
                description: "Gives 10 Prismatic Tapers to a character with a name partially matching \"Zero\"",
            },
            UbCommandExample {
                command: "/ub give Hero Token to Zero Cool",
                description: "Gives all Hero Tokens to Zero Cool",
            },
            UbCommandExample {
                command: "/ub giver Hero.* to Zero Cool",
                description: "Gives all items matching the regex \"Hero.*\" to Zero Cool",
            },
        ],
    },
    UbCommandUsage {
        name: "help",
        usage: "/ub help [command]",
        summary: "Prints help for UB command line usage.",
        examples: &[
            UbCommandExample {
                command: "/ub help",
                description: "Prints out all available UB commands",
            },
            UbCommandExample {
                command: "/ub help printcolors",
                description: "Prints out help and usage information for the printcolors command",
            },
        ],
    },
    UbCommandUsage {
        name: "id",
        usage: "/ub id",
        summary: "Prints the object id for the currently selected object.",
        examples: &[],
    },
    UbCommandUsage {
        name: "ig",
        usage: "/ub ig[p] <lootProfile> to <target>",
        summary: "Gives items matching the provided loot profile to a player.",
        examples: &[
            UbCommandExample {
                command: "/ub ig muledItems.utl to Zero Cool",
                description: "Gives all items matching Keep rules in muledItems.utl to Zero Cool",
            },
            UbCommandExample {
                command: "/ub igp muledItems.utl to Zero",
                description: "Gives all items matching Keep rules in muledItems.utl to a character partially matching the name Zero",
            },
        ],
    },
    UbCommandUsage {
        name: "jump",
        usage: "/ub jump[swzxc] [heading] [holdtime]",
        summary: "Jump commands with built in navigation pausing and retries.",
        examples: &[
            UbCommandExample {
                command: "/ub jumpsw 180 500",
                description: "Face 180 degrees (south) and jump forward with 500/1000 power.",
            },
            UbCommandExample {
                command: "/ub jumpsx 300",
                description: "Jump backward with 300/1000 power.",
            },
            UbCommandExample {
                command: "/ub jump",
                description: "Taps jump.",
            },
        ],
    },
    UbCommandUsage {
        name: "listgvars",
        usage: "/ub listgvars",
        summary: "Prints out all defined global variables for this server.",
        examples: &[UbCommandExample {
            command: "/ub listgvars",
            description: "Prints out all defined global variables for this server",
        }],
    },
    UbCommandUsage {
        name: "listpvars",
        usage: "/ub listpvars",
        summary: "Prints out all defined persistent variables for this character.",
        examples: &[UbCommandExample {
            command: "/ub listpvars",
            description: "Prints out all defined persistent variables for this character",
        }],
    },
    UbCommandUsage {
        name: "listvars",
        usage: "/ub listvars",
        summary: "Prints out all defined variables.",
        examples: &[UbCommandExample {
            command: "/ub listvars",
            description: "Prints out all defined variables",
        }],
    },
    UbCommandUsage {
        name: "login",
        usage: "/ub login [next[r][l] <name|index> | clear | list]",
        summary: "Chooses which character logs in next.",
        examples: &[
            UbCommandExample {
                command: "/ub login next 2",
                description: "Logs in the character at index 2 (the third one)",
            },
            UbCommandExample {
                command: "/ub login nextr -1",
                description: "Logs in the character with a name before the current one alphabetically",
            },
            UbCommandExample {
                command: "/ub login nextrl 1",
                description: "Logs in the character that comes next alphabetically, looping to the beginning",
            },
            UbCommandExample {
                command: "/ub login next salv",
                description: "Logs in the first character with 'salv' as part of their name",
            },
            UbCommandExample {
                command: "/ub login clear",
                description: "Clears the next login.",
            },
            UbCommandExample {
                command: "/ub login list",
                description: "Lists the characters and indices for this account.",
            },
        ],
    },
    UbCommandUsage {
        name: "mexec",
        usage: "/ub mexec <expression>",
        summary: "Evaluates a meta expression.",
        examples: &[UbCommandExample {
            command: "/ub mexec <expression>",
            description: "Evaluates expression",
        }],
    },
    UbCommandUsage {
        name: "myquests",
        usage: "/ub myquests",
        summary: "Refreshes your quest flags with /myquests.",
        examples: &[UbCommandExample {
            command: "/ub myquests",
            description: "Refreshes your quest flags with /myquests, and hides the output",
        }],
    },
    UbCommandUsage {
        name: "netclients",
        usage: "/ub netclients <tag>",
        summary: "Lists all available clients on the network, optionally limited to the specified tag.",
        examples: &[
            UbCommandExample {
                command: "/ub netclients",
                description: "Show all clients on the ub network",
            },
            UbCommandExample {
                command: "/ub netclients one",
                description: "Show all clients on the ub network with tag `one`",
            },
        ],
    },
    UbCommandUsage {
        name: "opt",
        usage: "/ub opt {list | get <option> | set <option> <newValue> | toggle <option>}",
        summary: "Manage settings from the command line.",
        examples: &[
            UbCommandExample {
                command: "/ub opt list",
                description: "Lists all available settings.",
            },
            UbCommandExample {
                command: "/ub opt get Plugin.Debug",
                description: "Gets the current value for the \"Plugin.Debug\" setting",
            },
            UbCommandExample {
                command: "/ub opt toggle Plugin.Debug",
                description: "Toggles the current value for the \"Plugin.Debug\" setting",
            },
            UbCommandExample {
                command: "/ub opt set Plugin.Debug true",
                description: "Sets the \"Plugin.Debug\" setting to True",
            },
        ],
    },
    UbCommandUsage {
        name: "playeroption",
        usage: "/ub playeroption (list|<option> {on | true | off | false})",
        summary: "Turns one of the character's own options on or off.",
        examples: &[
            UbCommandExample {
                command: "/ub playeroption AutoRepeatAttack on",
                description: "Enables the AutoRepeatAttack player option.",
            },
            UbCommandExample {
                command: "/ub",
                description: "Prints current build version to chat",
            },
        ],
    },
    UbCommandUsage {
        name: "portal",
        usage: "/ub portal[p] <portalName>",
        summary: "Uses a portal by name, with built in navigation pausing.",
        examples: &[
            UbCommandExample {
                command: "/ub portal Gateway",
                description: "Uses portal with exact name \"Gateway\"",
            },
            UbCommandExample {
                command: "/ub portalp Portal",
                description: "Uses portal with name partially matching \"Portal\"",
            },
        ],
    },
    UbCommandUsage {
        name: "pos",
        usage: "/ub pos",
        summary: "Prints position information for the currently selected object.",
        examples: &[],
    },
    UbCommandUsage {
        name: "prepclick",
        usage: "/ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}",
        summary: "Used to prepare for the first message box selection to appear after running the command.",
        examples: &[
            UbCommandExample {
                command: "Click yes within 10s",
                description: "/ub prepclick yes 10",
            },
            UbCommandExample {
                command: "Stop watching for message box",
                description: "/ub prepclick stop",
            },
        ],
    },
    UbCommandUsage {
        name: "printcolors",
        usage: "/ub printcolors",
        summary: "Prints out all available chat colors.",
        examples: &[UbCommandExample {
            command: "/ub printcolors",
            description: "Prints out all available chat colors",
        }],
    },
    UbCommandUsage {
        name: "propertydump",
        usage: "/ub propertydump",
        summary: "Prints information for the currently selected object.",
        examples: &[],
    },
    UbCommandUsage {
        name: "quit",
        usage: "/ub quit",
        summary: "Closes the client.",
        examples: &[],
    },
    UbCommandUsage {
        name: "select",
        usage: "/ub select[li][p] [item]",
        summary: "Selects an item by name.",
        examples: &[
            UbCommandExample {
                command: "/ub select Cake",
                description: "Select an item with exact name of Cake",
            },
            UbCommandExample {
                command: "/ub selectpi gold",
                description: "Select a partial match to the word gold ",
            },
            UbCommandExample {
                command: "/ub selectlp plant",
                description: "Select a partial match of a plant on the landscape.",
            },
        ],
    },
    UbCommandUsage {
        name: "setmotion",
        usage: "/ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>",
        summary: "Sets a wanted motion, in the client.",
        examples: &[
            UbCommandExample {
                command: "/ub setmotion Forward 1",
                description: "Makes your character run forward forever.",
            },
            UbCommandExample {
                command: "/ub setmotion Forward 0",
                description: "Might make your character stop running forward.",
            },
        ],
    },
    UbCommandUsage {
        name: "simplejump",
        usage: "/ub simplejump [holdtime]",
        summary: "Jumps where the character already faces.",
        examples: &[UbCommandExample {
            command: "/ub face 180",
            description: "Faces your character towards 180 degrees (south).",
        }],
    },
    UbCommandUsage {
        name: "swearallegiance",
        usage: "/ub swearallegiance[p][ <name|id|selected>]",
        summary: "Swear allegiance to a character.",
        examples: &[
            UbCommandExample {
                command: "/ub swearallegiance Yonneh",
                description: "Swear Allegiance to `Yonneh`",
            },
            UbCommandExample {
                command: "/ub swearallegiancep Yo",
                description: "Swear Allegiance to a character with a name partially matching `Yo`.",
            },
            UbCommandExample {
                command: "/ub swearallegiance",
                description: "Swear Allegiance to the closest character",
            },
            UbCommandExample {
                command: "/ub swearallegiance selected",
                description: "Swear Allegiance to the selected character",
            },
        ],
    },
    UbCommandUsage {
        name: "tinkcalc",
        usage: "/ub tinkcalc",
        summary: "Select an item and run this command to see best iron/granite combination.",
        examples: &[],
    },
    UbCommandUsage {
        name: "translateroute",
        usage: "/ub translateroute <startLandblock> <routeToLoad> <endLandblock> <routeToSaveAs> [force]",
        summary: "Translates a navigation route from one landblock to another. Add the force flag to overwrite the output route.",
        examples: &[
            UbCommandExample {
                command: "/ub translateroute 0x00640371 eo-east.nav 0x002B0371 eo-main.nav",
                description: "Translates eo-east.nav to landblock 0x002B0371(eo main) and saves it as eo-main.nav if the file doesn't exist",
            },
            UbCommandExample {
                command: "/ub translateroute 0x00640371 eo-east.nav 0x002B0371 eo-main.nav force",
                description: "Translates eo-east.nav to landblock 0x002B0371(eo main) and saves it as eo-main.nav, overwriting if the file exists",
            },
        ],
    },
    UbCommandUsage {
        name: "use",
        usage: "/ub use[li][p] [itemOne] on [itemTwo]",
        summary: "Uses an item, or uses one item on another.",
        examples: &[
            UbCommandExample {
                command: "/ub use Cake",
                description: "Use an item with exact name of Cake",
            },
            UbCommandExample {
                command: "/ub usepi splitting on gold pea",
                description: "Use a partial match (splitting) splitting tool on a gold pea",
            },
            UbCommandExample {
                command: "/ub uselp plant",
                description: "Use a plant on the landscape.",
            },
            UbCommandExample {
                command: "/ub usei Stamina Elixer",
                description: "Use a stamina elixer in your inventory.",
            },
        ],
    },
    UbCommandUsage {
        name: "vendor",
        usage: "/ub vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> | addsell[p] <item>}",
        summary: "Vendor commands, with built in navigation pausing.",
        examples: &[
            UbCommandExample {
                command: "/ub vendor open Tunlok Weapons Master",
                description: "Opens vendor with name \"Tunlok Weapons Master\"",
            },
            UbCommandExample {
                command: "/ub vendor opencancel",
                description: "Quietly cancels the last /ub vendor open* command",
            },
            UbCommandExample {
                command: "/ub vendor addbuy 10 Mana Scarab",
                description: "Adds 10 Mana Scarabs to the buy list. use /ub vendor buyall to actually buy",
            },
        ],
    },
    UbCommandUsage {
        name: "vitae",
        usage: "/ub vitae",
        summary: "Thinks to yourself with your current vitae percentage.",
        examples: &[],
    },
    UbCommandUsage {
        name: "xp",
        usage: "/ub xp [level|test|slow|export|import]",
        summary: "Spends experience by the configured plan.",
        examples: &[
            UbCommandExample {
                command: "/ub xp",
                description: "Displays the weights of the current xp policy.",
            },
            UbCommandExample {
                command: "/ub xp test",
                description: "Displays the way current experience would be spent with the current policy",
            },
            UbCommandExample {
                command: "/ub xp level",
                description: "Begins (or halts) quickly spending experience with up to MaxXpChunk.",
            },
            UbCommandExample {
                command: "/ub xp slow",
                description: "Begins (or halts) spending experience one level at a time.",
            },
            UbCommandExample {
                command: "/ub xp export",
                description: "Writes the current policy out to chat in a format that can be imported.",
            },
            UbCommandExample {
                command: "/ub xp import Alchemy=1;Cooking=1;...",
                description: "Imports any valid key-weight pairs to your policy, overriding existing and adding missing values.",
            },
        ],
    },
];

/// The usage lines `/vt help <command>` prints: MossTank's own additions to
/// the macro's commands, in name order. The macro's own commands have no
/// usage lines of their own; `/vt help` lists them by name.
pub static VTANK_ENTRIES: &[UbCommandUsage] = &[
    UbCommandUsage {
        name: "gamedb",
        usage: "/vt gamedb [update | interval [hours]]",
        summary: "Shows the game information database in use, checks openac-gamedata for a newer one now, or shows or sets how many hours a check stays fresh (MossTank checks at login when it is older; 0 checks at every login).",
        examples: &[],
    },
    UbCommandUsage {
        name: "getdb",
        usage: "/vt getdb",
        summary: "Checks openac-gamedata for a newer game information database now.",
        examples: &[],
    },
    UbCommandUsage {
        name: "help",
        usage: "/vt help [command]",
        summary: "Prints the command lists, or the usage of one of the commands below.",
        examples: &[],
    },
    UbCommandUsage {
        name: "metaaf",
        usage: "/vt metaaf [save/load] [filename]",
        summary: "Saves the meta as, or loads, filename.af in MossTank's own metaf form. /vt meta save writes a .met.",
        examples: &[],
    },
    UbCommandUsage {
        name: "metainterval",
        usage: "/vt metainterval [milliseconds]",
        summary: "Shows or sets how often the meta is checked (50-2000 ms; VTank checks every 293 ms).",
        examples: &[],
    },
    UbCommandUsage {
        name: "navaf",
        usage: "/vt navaf [save/load] [filename]",
        summary: "Saves the route as, or loads, filename.af in MossTank's own metaf form. /vt nav save writes a .nav.",
        examples: &[],
    },
    UbCommandUsage {
        name: "nextwp",
        usage: "/vt nextwp [count]",
        summary: "Skips the waypoint the route is heading for, or the next count waypoints.",
        examples: &[],
    },
    UbCommandUsage {
        name: "prevwp",
        usage: "/vt prevwp [count]",
        summary: "Steps the route back to the waypoint before the one it is heading for, or count waypoints back.",
        examples: &[],
    },
    UbCommandUsage {
        name: "ub",
        usage: "/vt ub",
        summary: "Prints the compatibility version line.",
        examples: &[],
    },
];
