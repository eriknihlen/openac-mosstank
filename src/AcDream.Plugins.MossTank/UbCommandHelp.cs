namespace AcDream.Plugins.MossTank;

/// <summary>One worked example of a command: what to type, and what it does.</summary>
internal readonly record struct UbCommandExample(string Command, string Description);

/// <summary>One command's usage line, one-sentence summary and examples.</summary>
internal readonly record struct UbCommandUsage(
    string Name,
    string Usage,
    string Summary)
{
    /// <summary>
    /// The reference's worked examples for the command, in its words, which
    /// its full help prints under "Examples:"; none for a command it gives
    /// none.
    /// </summary>
    public IReadOnlyList<UbCommandExample> Examples { get; init; } = [];
}

/// <summary>
/// The usage lines <c>/ub help</c> prints, one per UtilityBelt command
/// MossTank answers; the list is also every word <c>/ub</c> knows, which the
/// unknown-command answer guesses from. They repeat the reference's own
/// wording, because a macro written against those commands was written
/// against those words: the square brackets are the optional single-letter
/// flags, and the braces a choice of verbs.
/// </summary>
internal static class UbCommandHelp
{
    /// <summary>Every UtilityBelt command MossTank answers, in name order.</summary>
    internal static IReadOnlyList<UbCommandUsage> Entries { get; } =
    [
        new("autocram",
            "/ub autocram",
            "Auto Cram into side packs."),
        new("autostack",
            "/ub autostack",
            "Auto Stack your inventory."),
        new("autotinker",
            "/ub autotinker",
            "Starts autotinker."),
        new("autovendor",
            "/ub autovendor <cancel|lootProfile>",
            "Auto buy/sell from vendors.")
        {
            Examples =
            [
                new("/ub autovendor", "Loads VendorName.utl and starts the AutoVendor process."),
                new("/ub autovendor cancel", "Cancels the current autovendor session."),
                new("/ub autovendor recomp.utl", "Loads recomp.utl and starts the AutoVendor process."),
            ],
        },
        new("bc",
            "/ub bc [millisecondDelay] <command>",
            "Broadcasts a command to all open clients, with optional "
            + "millisecondDelay inbetween each.")
        {
            Examples =
            [
                new("/ub bc 5000 /say hello", "Runs \"/say hello\" on every client, with a 5000ms delay between each"),
                new("/ub bc /say hello", "Runs \"/say hello\" on every client, with no delay"),
            ],
        },
        new("bct",
            "/ub bct <tags> [millisecondDelay] <command>",
            "Broadcasts a command to all clients with the specified "
            + "comma-separated tags, with optional millisecondDelay "
            + "inbetween each. Tags are managed with the Networking.Tags "
            + "setting.")
        {
            Examples =
            [
                new("/ub bct one,two 5000 /say hello", "Runs \"/say hello\" on every client tagged `one` or `two`, with a 5000ms delay between each"),
                new("/ub bct three /say hello", "Runs \"/say hello\" on every client tagged `three`, with no delay"),
                new("/ub bct \"some tag\",\"another tag\" /say hello", "Runs \"/say hello\" on every client tagged `some tag` or `another tag`, with no delay"),
            ],
        },
        new("breakallegiance",
            "/ub breakallegiance[p][ <name|id|selected>]",
            "Break your allegiance to a character.")
        {
            Examples =
            [
                new("/ub breakallegiance Yonneh", "Break your Allegiance to `Yonneh` (noooooooooooo)"),
                new("/ub breakallegiancep Yo", "Break Allegiance from a character with a name partially matching `Yo`."),
                new("/ub breakallegiance", "Break Allegiance from the closest character"),
                new("/ub breakallegiance selected", "Break Allegiance from the selected character"),
            ],
        },
        new("calcdamage",
            "/ub calcdamage",
            "Calculates the buffed damage of the currently selected item. Only cantrip buffs are included in the calculation.")
        {
            Examples =
            [
                new("/ub calcdamage", "calcdamage"),
            ],
        },
        new("clearbugged",
            "/ub clearbugged",
            "ID everything in your inventory, and remove bugged items."),
        new("clearmotion",
            "/ub clearmotion",
            "Clears all wanted motions, in the client.")
        {
            Examples =
            [
                new("/ub clearmotion", "Clears all wanted motions, in the client."),
            ],
        },
        new("close",
            "/ub close corpse",
            "Close the open corpse.")
        {
            Examples =
            [
                new("/ub close corpse", "Close the corpse if one is open"),
            ],
        },
        new("closestportal",
            "/ub closestportal",
            "Uses the closest portal.")
        {
            Examples =
            [
                new("/ub closestportal", "Uses the closest portal"),
            ],
        },
        new("combatstate",
            "/ub combatstate (peace|melee|missile|magic)",
            "Sets combat state."),
        new("count",
            "/ub count {item <name> | profile <p> | player <range>}",
            "Counts matching items, or the players around you.")
        {
            Examples =
            [
                new("/ub count Prismatic Taper", "Counts the total number of Prismatic Tapers in your inventory."),
                new("/ub count recomp.utl", "Counts the number of items matching recomp.utl in your inventory, thinking to yourself when finished"),
            ],
        },
        new("date",
            "/ub date[utc] [format]",
            "Prints the current date with an optional .NET format string.")
        {
            Examples =
            [
                new("/ub date hh:mm:ss tt", "Prints current local time '06:09:01 PM'"),
                new("/ub dateutc dddd dd MMMM", "Prints current utc date 'Friday 29 August'"),
            ],
        },
        new("delay",
            "/ub delay <millisecondDelay> <command>",
            "Runs a command after a delay.")
        {
            Examples =
            [
                new("/ub delay 5000 /say hello", "Runs \"/say hello\" after a 3000ms delay (3 seconds)"),
            ],
        },
        new("dumpskills",
            "/ub dumpskills",
            "Prints all skills and training levels to chat.")
        {
            Examples =
            [
                new("/ub dumpskills", "Prints all skills and training levels to chat"),
            ],
        },
        new("equip",
            "/ub equip {list | load [p] | test [p] | create [p]}",
            "Manages equipment profiles.")
        {
            Examples =
            [
                new("/ub equip load profile.utl", "Equips all items matching profile.utl."),
                new("/ub equip list", "Lists available equipment profiles."),
                new("/ub equip test profile.utl", "Test equipping profile.utl"),
            ],
        },
        new("face",
            "/ub face <heading>",
            "Face heading commands with built in navigation pausing and retries.")
        {
            Examples =
            [
                new("/ub face 180", "Faces your character towards 180 degrees (south)."),
            ],
        },
        new("fellow",
            MossTankPanel.FellowUsage,
            "UB Fellowship Commands."),
        new("follow",
            "/ub follow[p] <name>",
            "Sets a navigation route to follow a player.")
        {
            Examples =
            [
                new("/ub follow Zero Cool", "Sets a VTank nav route to follow \"Zero Cool\""),
                new("/ub followp Zero", "Sets a VTank nav route to follow a character with a name partially matching \"Zero\""),
            ],
        },
        new("getjob",
            "/ub getjob",
            "Run this command to see tinker jobs currently in queue."),
        new("give",
            "/ub give[p{P|r}] [itemCount] <itemName> to <target>",
            "Gives items matching the provided name to a player. Small p "
            + "matches part of the item name, capital P part of the target's, "
            + "and r reads the item name as a regular expression.")
        {
            Examples =
            [
                new("/ub givep 10 Prismatic to Zero Cool", "Gives 10 items partially matching the name \"Prismatic\" to Zero Cool"),
                new("/ub giveP 10 Prismatic Tapers to Zero", "Gives 10 Prismatic Tapers to a character with a name partially matching \"Zero\""),
                new("/ub give Hero Token to Zero Cool", "Gives all Hero Tokens to Zero Cool"),
                new("/ub giver Hero.* to Zero Cool", "Gives all items matching the regex \"Hero.*\" to Zero Cool"),
            ],
        },
        new("help",
            "/ub help [command]",
            "Prints help for UB command line usage.")
        {
            Examples =
            [
                new("/ub help", "Prints out all available UB commands"),
                new("/ub help printcolors", "Prints out help and usage information for the printcolors command"),
            ],
        },
        new("id",
            "/ub id",
            "Prints the object id for the currently selected object."),
        new("ig",
            "/ub ig[p] <lootProfile> to <target>",
            "Gives items matching the provided loot profile to a player.")
        {
            Examples =
            [
                new("/ub ig muledItems.utl to Zero Cool", "Gives all items matching Keep rules in muledItems.utl to Zero Cool"),
                new("/ub igp muledItems.utl to Zero", "Gives all items matching Keep rules in muledItems.utl to a character partially matching the name Zero"),
            ],
        },
        new("jump",
            "/ub jump[swzxc] [heading] [holdtime]",
            "Jump commands with built in navigation pausing and retries.")
        {
            Examples =
            [
                new("/ub jumpsw 180 500", "Face 180 degrees (south) and jump forward with 500/1000 power."),
                new("/ub jumpsx 300", "Jump backward with 300/1000 power."),
                new("/ub jump", "Taps jump."),
            ],
        },
        new("listgvars",
            "/ub listgvars",
            "Prints out all defined global variables for this server.")
        {
            Examples =
            [
                new("/ub listgvars", "Prints out all defined global variables for this server"),
            ],
        },
        new("listpvars",
            "/ub listpvars",
            "Prints out all defined persistent variables for this character.")
        {
            Examples =
            [
                new("/ub listpvars", "Prints out all defined persistent variables for this character"),
            ],
        },
        new("listvars",
            "/ub listvars",
            "Prints out all defined variables.")
        {
            Examples =
            [
                new("/ub listvars", "Prints out all defined variables"),
            ],
        },
        new("login",
            "/ub login [next[r][l] <name|index> | clear | list]",
            "Chooses which character logs in next.")
        {
            Examples =
            [
                new("/ub login next 2", "Logs in the character at index 2 (the third one)"),
                new("/ub login nextr -1", "Logs in the character with a name before the current one alphabetically"),
                new("/ub login nextrl 1", "Logs in the character that comes next alphabetically, looping to the beginning"),
                new("/ub login next salv", "Logs in the first character with 'salv' as part of their name"),
                new("/ub login clear", "Clears the next login."),
                new("/ub login list", "Lists the characters and indices for this account."),
            ],
        },
        new("mexec",
            "/ub mexec <expression>",
            "Evaluates a meta expression.")
        {
            Examples =
            [
                new("/ub mexec <expression>", "Evaluates expression"),
            ],
        },
        new("myquests",
            "/ub myquests",
            "Refreshes your quest flags with /myquests.")
        {
            Examples =
            [
                new("/ub myquests", "Refreshes your quest flags with /myquests, and hides the output"),
            ],
        },
        new("netclients",
            "/ub netclients <tag>",
            "Lists all available clients on the network, optionally limited "
            + "to the specified tag.")
        {
            Examples =
            [
                new("/ub netclients", "Show all clients on the ub network"),
                new("/ub netclients one", "Show all clients on the ub network with tag `one`"),
            ],
        },
        new("opt",
            "/ub opt {list | get <option> | set <option> <newValue> | toggle <option>}",
            "Manage settings from the command line.")
        {
            Examples =
            [
                new("/ub opt list", "Lists all available settings."),
                new("/ub opt get Plugin.Debug", "Gets the current value for the \"Plugin.Debug\" setting"),
                new("/ub opt toggle Plugin.Debug", "Toggles the current value for the \"Plugin.Debug\" setting"),
                new("/ub opt set Plugin.Debug true", "Sets the \"Plugin.Debug\" setting to True"),
            ],
        },
        new("playeroption",
            "/ub playeroption (list|<option> {on | true | off | false})",
            "Turns one of the character's own options on or off.")
        {
            Examples =
            [
                new("/ub playeroption AutoRepeatAttack on", "Enables the AutoRepeatAttack player option."),
                new("/ub", "Prints current build version to chat"),
            ],
        },
        new("portal",
            "/ub portal[p] <portalName>",
            "Uses a portal by name, with built in navigation pausing.")
        {
            Examples =
            [
                new("/ub portal Gateway", "Uses portal with exact name \"Gateway\""),
                new("/ub portalp Portal", "Uses portal with name partially matching \"Portal\""),
            ],
        },
        new("pos",
            "/ub pos",
            "Prints position information for the currently selected object."),
        new("prepclick",
            PrepClickController.Usage,
            "Used to prepare for the first message box selection to appear after running the command.")
        {
            Examples =
            [
                new("Click yes within 10s", "/ub prepclick yes 10"),
                new("Stop watching for message box", "/ub prepclick stop"),
            ],
        },
        new("printcolors",
            "/ub printcolors",
            "Prints out all available chat colors.")
        {
            Examples =
            [
                new("/ub printcolors", "Prints out all available chat colors"),
            ],
        },
        new("propertydump",
            "/ub propertydump",
            "Prints information for the currently selected object."),
        new("quit",
            "/ub quit",
            "Closes the client."),
        new("select",
            "/ub select[li][p] [item]",
            "Selects an item by name.")
        {
            Examples =
            [
                new("/ub select Cake", "Select an item with exact name of Cake"),
                new("/ub selectpi gold", "Select a partial match to the word gold "),
                new("/ub selectlp plant", "Select a partial match of a plant on the landscape."),
            ],
        },
        new("setmotion",
            HeldMotions.Usage,
            "Sets a wanted motion, in the client.")
        {
            Examples =
            [
                new("/ub setmotion Forward 1", "Makes your character run forward forever."),
                new("/ub setmotion Forward 0", "Might make your character stop running forward."),
            ],
        },
        new("simplejump",
            "/ub simplejump [holdtime]",
            "Jumps where the character already faces.")
        {
            Examples =
            [
                new("/ub face 180", "Faces your character towards 180 degrees (south)."),
            ],
        },
        new("swearallegiance",
            "/ub swearallegiance[p][ <name|id|selected>]",
            "Swear allegiance to a character.")
        {
            Examples =
            [
                new("/ub swearallegiance Yonneh", "Swear Allegiance to `Yonneh`"),
                new("/ub swearallegiancep Yo", "Swear Allegiance to a character with a name partially matching `Yo`."),
                new("/ub swearallegiance", "Swear Allegiance to the closest character"),
                new("/ub swearallegiance selected", "Swear Allegiance to the selected character"),
            ],
        },
        new("tinkcalc",
            "/ub tinkcalc",
            "Select an item and run this command to see best iron/granite combination."),
        new("translateroute",
            "/ub translateroute <startLandblock> <routeToLoad> <endLandblock> <routeToSaveAs> [force]",
            "Translates a navigation route from one landblock to another. Add the force flag to overwrite the output route.")
        {
            Examples =
            [
                new("/ub translateroute 0x00640371 eo-east.nav 0x002B0371 eo-main.nav", "Translates eo-east.nav to landblock 0x002B0371(eo main) and saves it as eo-main.nav if the file doesn't exist"),
                new("/ub translateroute 0x00640371 eo-east.nav 0x002B0371 eo-main.nav force", "Translates eo-east.nav to landblock 0x002B0371(eo main) and saves it as eo-main.nav, overwriting if the file exists"),
            ],
        },
        new("use",
            "/ub use[li][p] [itemOne] on [itemTwo]",
            "Uses an item, or uses one item on another.")
        {
            Examples =
            [
                new("/ub use Cake", "Use an item with exact name of Cake"),
                new("/ub usepi splitting on gold pea", "Use a partial match (splitting) splitting tool on a gold pea"),
                new("/ub uselp plant", "Use a plant on the landscape."),
                new("/ub usei Stamina Elixer", "Use a stamina elixer in your inventory."),
            ],
        },
        new("vendor",
            "/ub vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | "
            + "sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> "
            + "| addsell[p] <item>}",
            "Vendor commands, with built in navigation pausing.")
        {
            Examples =
            [
                new("/ub vendor open Tunlok Weapons Master", "Opens vendor with name \"Tunlok Weapons Master\""),
                new("/ub vendor opencancel", "Quietly cancels the last /ub vendor open* command"),
                new("/ub vendor addbuy 10 Mana Scarab", "Adds 10 Mana Scarabs to the buy list. use /ub vendor buyall to actually buy"),
            ],
        },
        new("vitae",
            "/ub vitae",
            "Thinks to yourself with your current vitae percentage."),
        new("xp",
            "/ub xp [level|test|slow|export|import]",
            "Spends experience by the configured plan.")
        {
            Examples =
            [
                new("/ub xp", "Displays the weights of the current xp policy."),
                new("/ub xp test", "Displays the way current experience would be spent with the current policy"),
                new("/ub xp level", "Begins (or halts) quickly spending experience with up to MaxXpChunk."),
                new("/ub xp slow", "Begins (or halts) spending experience one level at a time."),
                new("/ub xp export", "Writes the current policy out to chat in a format that can be imported."),
                new("/ub xp import Alchemy=1;Cooking=1;...", "Imports any valid key-weight pairs to your policy, overriding existing and adding missing values."),
            ],
        },
    ];

    /// <summary>The entry for a command name, ignoring letter case.</summary>
    internal static bool TryGet(string? name, out UbCommandUsage usage) =>
        Find(Entries, name, out usage);

    /// <summary>The entry for a command this table must carry.</summary>
    internal static UbCommandUsage Require(string name) =>
        TryGet(name, out UbCommandUsage usage)
            ? usage
            : throw new KeyNotFoundException($"No usage line for {name}.");

    internal static bool Find(
        IReadOnlyList<UbCommandUsage> entries,
        string? name,
        out UbCommandUsage usage)
    {
        string text = name?.Trim() ?? string.Empty;
        foreach (UbCommandUsage candidate in entries)
        {
            if (candidate.Name.Equals(text, StringComparison.OrdinalIgnoreCase))
            {
                usage = candidate;
                return true;
            }
        }
        usage = default;
        return false;
    }
}

/// <summary>
/// The usage lines <c>/vt help &lt;command&gt;</c> prints: MossTank's own
/// additions to the macro's commands. The macro's own commands have no usage
/// lines of their own; <c>/vt help</c> lists them by name.
/// </summary>
internal static class VtankCommandHelp
{
    /// <summary>Every documented <c>/vt</c> command, in name order.</summary>
    internal static IReadOnlyList<UbCommandUsage> Entries { get; } =
    [
        new("gamedb",
            "/vt gamedb [update | interval [hours]]",
            "Shows the game information database in use, checks openac-gamedata for a newer one now, "
            + "or shows or sets how many hours a check stays fresh (MossTank checks at login when it is older; 0 checks at every login)."),
        new("getdb",
            "/vt getdb",
            "Checks openac-gamedata for a newer game information database now."),
        new("help",
            "/vt help [command]",
            "Prints the command lists, or the usage of one of the commands below."),
        new("metaaf",
            "/vt metaaf [save/load] [filename]",
            "Saves the meta as, or loads, filename.af in MossTank's own metaf form. /vt meta save writes a .met."),
        new("metainterval",
            "/vt metainterval [milliseconds]",
            "Shows or sets how often the meta is checked (50-2000 ms; VTank checks every 293 ms)."),
        new("navaf",
            "/vt navaf [save/load] [filename]",
            "Saves the route as, or loads, filename.af in MossTank's own metaf form. /vt nav save writes a .nav."),
        new("nextwp",
            "/vt nextwp [count]",
            "Skips the waypoint the route is heading for, or the next count waypoints."),
        new("prevwp",
            "/vt prevwp [count]",
            "Steps the route back to the waypoint before the one it is heading for, or count waypoints back."),
        new("ub",
            "/vt ub",
            "Prints the compatibility version line."),
    ];

    /// <summary>The entry for a command name, ignoring letter case.</summary>
    internal static bool TryGet(string? name, out UbCommandUsage usage) =>
        UbCommandHelp.Find(Entries, name, out usage);
}
