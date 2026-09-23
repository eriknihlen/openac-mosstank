namespace AcDream.Plugins.MossTank;

/// <summary>One command's usage line and one-sentence summary.</summary>
internal readonly record struct UbCommandUsage(
    string Name,
    string Usage,
    string Summary);

/// <summary>
/// The usage lines <c>/vt help</c> prints. They repeat the reference's own
/// wording, because a macro written against those commands was written
/// against those words: the square brackets are the optional single-letter
/// flags, and the braces a choice of verbs.
/// </summary>
internal static class UbCommandHelp
{
    /// <summary>Every command with a published usage line, in name order.</summary>
    internal static IReadOnlyList<UbCommandUsage> Entries { get; } =
    [
        new("autotinker",
            "/vt autotinker",
            "Starts autotinker."),
        new("autovendor",
            "/vt autovendor <cancel|lootProfile>",
            "Auto buy/sell from vendors."),
        new("bc",
            "/vt bc [millisecondDelay] <command>",
            "Broadcasts a command to all open clients, with optional "
            + "millisecondDelay inbetween each."),
        new("bct",
            "/vt bct <tags> [millisecondDelay] <command>",
            "Broadcasts a command to all clients with the specified "
            + "comma-separated tags, with optional millisecondDelay "
            + "inbetween each. Tags are managed with the Networking.Tags "
            + "setting."),
        new("breakallegiance",
            "/vt breakallegiance[p][ <name|id|selected>]",
            "Break your allegiance to a character."),
        new("calcdamage",
            "/vt calcdamage",
            "Calculates the buffed damage of the currently selected item. Only cantrip buffs are included in the calculation."),
        new("clearmotion",
            "/vt clearmotion",
            "Clears all wanted motions, in the client."),
        new("close",
            "/vt close corpse",
            "Close the open corpse."),
        new("closestportal",
            "/vt closestportal",
            "Uses the closest portal."),
        new("combatstate",
            "/vt combatstate (peace|melee|missile|magic)",
            "Sets combat state."),
        new("count",
            "/vt count {item <name> | profile <p> | player <range>}",
            "Counts matching items, or the players around you."),
        new("date",
            "/vt date[utc] [format]",
            "Prints the current date with an optional .NET format string."),
        new("delay",
            "/vt delay <millisecondDelay> <command>",
            "Runs a command after a delay."),
        new("equip",
            "/vt equip {list | load [p] | test [p] | create [p]}",
            "Manages equipment profiles."),
        new("face",
            "/vt face <heading>",
            "Face heading commands with built in navigation pausing and retries."),
        new("fellow",
            MossTankPanel.FellowUsage,
            "UB Fellowship Commands."),
        new("follow",
            "/vt follow[p] <name>",
            "Sets a navigation route to follow a player."),
        new("gamedb",
            "/vt gamedb [update | interval [hours]]",
            "Shows the game information database in use, checks openac-gamedata for a newer one now, "
            + "or shows or sets how many hours a check stays fresh (MossTank checks at login when it is older; 0 checks at every login)."),
        new("getdb",
            "/vt getdb",
            "Checks openac-gamedata for a newer game information database now."),
        new("getjob",
            "/vt getjob",
            "Run this command to see tinker jobs currently in queue."),
        new("give",
            "/vt give[p{P|r}] [itemCount] <itemName> to <target>",
            "Gives items matching the provided name to a player. Small p "
            + "matches part of the item name, capital P part of the target's, "
            + "and r reads the item name as a regular expression."),
        new("help",
            "/vt help [command]",
            "Prints help for command line usage."),
        new("id",
            "/vt id",
            "Prints the object id for the currently selected object."),
        new("ig",
            "/vt ig[p] <lootProfile> to <target>",
            "Gives items matching the provided loot profile to a player."),
        new("jump",
            "/vt jump[swzxc] [heading] [holdtime]",
            "Jump commands with built in navigation pausing and retries."),
        new("listgvars",
            "/vt listgvars",
            "Prints out all defined global variables for this server."),
        new("listpvars",
            "/vt listpvars",
            "Prints out all defined persistent variables for this character."),
        new("listvars",
            "/vt listvars",
            "Prints out all defined variables."),
        new("login",
            "/vt login [next[r][l] <name|index> | clear | list]",
            "Chooses which character logs in next."),
        new("mexec",
            "/vt mexec <expression>",
            "Evaluates a meta expression."),
        new("metainterval",
            "/vt metainterval [milliseconds]",
            "Shows or sets how often the meta is checked (50-2000 ms; VTank checks every 293 ms)."),
        new("netclients",
            "/vt netclients <tag>",
            "Lists all available clients on the network, optionally limited "
            + "to the specified tag."),
        new("nextwp",
            "/vt nextwp [count]",
            "Skips the waypoint the route is heading for, or the next count waypoints."),
        new("opt",
            "/vt opt {list | get <option> | set <option> <newValue> | toggle <option>}",
            "Manage settings from the command line."),
        new("portal",
            "/vt portal[p] <portalName>",
            "Uses a portal by name, with built in navigation pausing."),
        new("pos",
            "/vt pos",
            "Prints position information for the currently selected object."),
        new("prepclick",
            PrepClickController.Usage,
            "Used to prepare for the first message box selection to appear after running the command."),
        new("printcolors",
            "/vt printcolors",
            "Prints out all available chat colors."),
        new("propertydump",
            "/vt propertydump",
            "Prints information for the currently selected object."),
        new("select",
            "/vt select[li][p] [item]",
            "Selects an item by name."),
        new("setmotion",
            HeldMotions.Usage,
            "Sets a wanted motion, in the client."),
        new("simplejump",
            "/vt simplejump [holdtime]",
            "Jumps where the character already faces."),
        new("swearallegiance",
            "/vt swearallegiance[p][ <name|id|selected>]",
            "Swear allegiance to a character."),
        new("tinkcalc",
            "/vt tinkcalc",
            "Select an item and run this command to see best iron/granite combination."),
        new("translateroute",
            "/vt translateroute <startLandblock> <routeToLoad> <endLandblock> <routeToSaveAs> [force]",
            "Translates a navigation route from one landblock to another. Add the force flag to overwrite the output route."),
        new("ub",
            "/vt ub",
            "Prints the compatibility version line."),
        new("use",
            "/vt use[li][p] [itemOne] on [itemTwo]",
            "Uses an item, or uses one item on another."),
        new("vendor",
            "/vt vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | "
            + "sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> "
            + "| addsell[p] <item>}",
            "Vendor commands, with built in navigation pausing."),
        new("vitae",
            "/vt vitae",
            "Thinks to yourself with your current vitae percentage."),
        new("xp",
            "/vt xp [level|test|slow|export|import]",
            "Spends experience by the configured plan."),
    ];

    /// <summary>The entry for a command name, ignoring letter case.</summary>
    internal static bool TryGet(string? name, out UbCommandUsage usage)
    {
        string text = name?.Trim() ?? string.Empty;
        foreach (UbCommandUsage candidate in Entries)
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
