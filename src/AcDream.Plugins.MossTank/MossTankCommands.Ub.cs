using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The commands the reference plugin published beside the macro's own: the
/// ones that name an object, print what the client knows, or run something
/// later. They share the controllers the panel already owns; nothing here
/// keeps a second copy of any state.
/// </summary>
internal sealed partial class MossTankPanel
{
    /// <summary>
    /// The classes a portal command will accept. The reference takes a
    /// non-player character too, because several world portals are one.
    /// </summary>
    private static readonly PluginObjectClass[] PortalClasses =
        [PluginObjectClass.Portal, PluginObjectClass.Npc];

    private static readonly PluginObjectClass[] PlayerClasses =
        [PluginObjectClass.Player];

    /// <summary>
    /// Cantrips that raise a weapon's maximum damage, and by how much. The
    /// reference carries the same five; nothing else on a weapon changes the
    /// number the damage estimate starts from.
    /// </summary>
    private static readonly (uint SpellId, int Bonus)[] MaxDamageCantrips =
    [
        (2598u, 2),    // Minor Blood Thirst
        (2586u, 4),    // Major Blood Thirst
        (4661u, 7),    // Epic Blood Thirst
        (6089u, 10),   // Legendary Blood Thirst
        (3688u, 300),  // Prodigal Blood Drinker
    ];

    /// <summary>The appraised elemental damage added to every swing.</summary>
    private const uint ElementalDamageBonusProperty = 204u;

    /// <summary>How many times a tinker has already been put into an item.</summary>
    private const uint NumberTimesTinkeredProperty = 171u;

    /// <summary>Non-zero once an imbue has taken one of the ten attempts.</summary>
    private const uint ImbuedProperty = 179u;

    /// <summary>The default format a bare date command prints in.</summary>
    private const string DefaultDateFormat = "dddd dd MMMM HH:mm:ss";

    /// <summary>
    /// Commands waiting on their delay, soonest first. They are held here and
    /// not on a timer of their own so that a session that ends takes them with
    /// it: a line scheduled before a logout must not arrive after it.
    /// </summary>
    private readonly List<(double RemainingSeconds, string Command, double DelayMilliseconds)> _delayedCommands = [];

    /// <summary>
    /// Runs a command whose verb carries single-letter flags, which a switch
    /// on the whole verb cannot reach. False when the verb is none of them,
    /// which is what makes it an unknown command.
    /// </summary>
    private bool TryExecuteFlaggedCommand(string verb, string arguments)
    {
        if (TryFlags(verb, "jump", "swzxc", out string jumpFlags))
        {
            HandleUbJumpCommand(jumpFlags, arguments);
            return true;
        }
        if (TryFlags(verb, "ig", "p", out string giverFlags))
        {
            HandleItemGiverCommand(giverFlags.Contains('p'), arguments);
            return true;
        }
        if (TryFlags(verb, "portal", "p", out string portalFlags))
        {
            UsePortalByName(arguments, portalFlags.Contains('p'));
            return true;
        }
        if (TryFlags(verb, "follow", "p", out string followFlags))
        {
            HandleFollowCommand(arguments, followFlags.Contains('p'));
            return true;
        }
        if (TryFlags(verb, "use", "lip", out string useFlags))
        {
            HandleUseCommand(useFlags, arguments);
            return true;
        }
        if (TryFlags(verb, "select", "lip", out string selectFlags))
        {
            HandleSelectCommand(selectFlags, arguments);
            return true;
        }
        if (TryFlags(verb, "swearallegiance", "p", out string swearFlags))
        {
            HandleAllegianceCommand(verb, arguments, swearFlags.Contains('p'), swear: true);
            return true;
        }
        if (TryFlags(verb, "breakallegiance", "p", out string breakFlags))
        {
            HandleAllegianceCommand(verb, arguments, breakFlags.Contains('p'), swear: false);
            return true;
        }
        return false;
    }

    /// <summary>
    /// Splits a verb into its command name and its flag letters. False when
    /// the verb is not that command, or carries a letter the command has no
    /// flag for -- "jumpq" is not a jump with an unknown flag, it is a typo,
    /// and answering it with the jump usage says so.
    /// </summary>
    private static bool TryFlags(
        string verb,
        string name,
        string allowed,
        out string flags)
    {
        flags = string.Empty;
        if (!verb.StartsWith(name, StringComparison.OrdinalIgnoreCase))
            return false;
        string rest = verb[name.Length..];
        foreach (char letter in rest)
        {
            if (!allowed.Contains(letter, StringComparison.Ordinal))
                return false;
        }
        flags = rest;
        return true;
    }

    // ── /ub: the UtilityBelt command word ───────────────────────────────

    /// <summary>
    /// Runs one line typed under <c>/ub</c>: the UtilityBelt commands, and
    /// only those. The macro's own commands are not among them -- they answer
    /// on <c>/vt</c> -- so a word this does not know gets the reference's
    /// "command not found" answer, with its guesses at what was meant.
    /// </summary>
    internal void ExecuteUbCommand(PluginCommand command)
    {
        try
        {
            ExecuteUbCommandCore(command.Arguments);
        }
        catch (Exception error)
        {
            WriteUbError($"Command failed: {error.GetBaseException().Message}");
            _host.Log.Error("MossTank /ub command failed.", error);
        }
    }

    private void ExecuteUbCommandCore(string input)
    {
        (string typed, string arguments) = SplitHead(input);

        // The give verb carries its flags in the letters after it, and their
        // case is the whole meaning: little p matches part of the ITEM's
        // name, big P part of the TARGET's. Read before the verb is folded to
        // one case, because folding loses the difference.
        if (TryReadGiveFlags(
            typed,
            out GiveNameMatch giveMatch,
            out bool givePartialTarget))
        {
            HandleGiveCommand(arguments, giveMatch, givePartialTarget);
            return;
        }

        string verb = typed.ToLowerInvariant();
        switch (verb)
        {
            case "":
                WriteUbVersion();
                return;
            case "help":
                HandleUbHelpCommand(arguments);
                return;
            case "opt":
                HandleUbOptionCommand(arguments);
                return;
            case "mexec":
                ExecuteUbExpression(arguments);
                return;
            // The quiet form: the expression runs and only an error is said.
            case "mexecm":
                ExecuteUbExpression(arguments, silent: true);
                return;
            case "propertydump":
                DumpSelectedPropertiesForUb();
                return;
            case "dumpskills":
                DumpSkills(ub: true);
                return;
            case "count":
                HandleCountCommand(arguments);
                return;
            case "login":
                HandleLoginCommand(arguments);
                return;
            case "autovendor":
                HandleAutoVendorCommand(arguments);
                return;
            case "vendor":
                PostUbReply("vendor", _vendorTrade.VendorCommand(arguments));
                return;
            case "xp":
                PostUbReply("xp", _experienceSpend.Command(arguments));
                return;
            case "equip":
                PostUbReply("equip", _equipProfile.Command(arguments));
                return;
            case "simplejump":
                HandleSimpleJumpCommand(arguments);
                return;
            case "calcdamage":
                CalculateSelectedDamage();
                return;
            case "pos":
                PrintSelectedPosition();
                return;
            case "id":
                PrintSelectedId();
                return;
            case "vitae":
                ThinkVitae();
                return;
            case "combatstate":
                HandleCombatStateCommand(arguments);
                return;
            case "date":
                PrintDate(utc: false, arguments);
                return;
            case "dateutc":
                PrintDate(utc: true, arguments);
                return;
            case "delay":
                HandleDelayCommand(arguments);
                return;
            case "bc":
                HandleBroadcastCommand(arguments);
                return;
            case "bct":
                HandleTaggedBroadcastCommand(arguments);
                return;
            case "netclients":
                HandleNetClientsCommand(arguments);
                return;
            case "closestportal":
                UsePortalByName(string.Empty, partial: true);
                return;
            case "close":
                HandleCloseCommand(arguments);
                return;
            case "printcolors":
                PrintChatColors();
                return;
            case "autotinker":
                StartAutoTinker();
                return;
            case "getjob":
                PrintTinkerJobs();
                return;
            case "tinkcalc":
                PrintTinkerCalculation();
                return;
            case "listvars":
                ListVariables(ExpressionVariableScope.Session);
                return;
            case "listpvars":
                ListVariables(ExpressionVariableScope.Persistent);
                return;
            case "listgvars":
                ListVariables(ExpressionVariableScope.Global);
                return;
            case "myquests":
                PostUb(UbChat.Tool(UbChat.Tools.QuestTracker, "Refreshing quests"));
                _expressions.RefreshQuests();
                return;
            case "quit":
                QuitClient();
                return;
            case "autostack":
                _ubStackCram.Start(stack: true);
                return;
            case "autocram":
                _ubStackCram.Start(stack: false);
                return;
            case "clearbugged":
                _ubClearBugged.Start();
                return;
            case "playeroption":
                HandlePlayerOptionCommand(arguments);
                return;
            case "translateroute":
                HandleTranslateRouteCommand(arguments);
                return;
            case "face":
                HandleFaceCommand(arguments);
                return;
            case "setmotion":
                HandleSetMotionCommand(arguments);
                return;
            case "clearmotion":
                _expressions.HeldMotions.Clear();
                return;
            case "prepclick":
                _prepClick.Command(arguments);
                return;
            case "fellow":
                HandleFellowCommand(arguments);
                return;
            default:
                // The flagged verbs (jumpsw, igp, usepi, ...) cannot be
                // switch cases: their letters combine, so they are matched by
                // name and flag set rather than spelled out.
                if (!TryExecuteFlaggedCommand(verb, arguments))
                    WriteUnknownUbCommand(verb);
                return;
        }
    }

    /// <summary>
    /// The reference's answer to a word it has no command for: where to look,
    /// then every command within two edits of what was typed (a swap of two
    /// neighbouring letters counting as one), so a typo names its fix. Both
    /// are errors, in the error display's class and behind its switch.
    /// </summary>
    private void WriteUnknownUbCommand(string verb)
    {
        PostUb(UnknownUbCommand);
        string[] near = UbCommandHelp.Entries
            .Select(static entry => entry.Name)
            .Where(name => EditDistance(verb, name) <= 2)
            .ToArray();
        if (near.Length > 0)
            WriteUbError("Did you mean one of these? " + string.Join(", ", near));
    }

    /// <summary>What <c>/ub</c> answers to a word it has no command for.</summary>
    internal const string UnknownUbCommand =
        UbChat.Tag + "Error: Command not found! Type \"ub help\" for a list of commands.";

    /// <summary>
    /// Edits between two words -- insertions, deletions, substitutions, and
    /// a swap of two neighbouring letters -- the measure the reference uses
    /// to guess at a mistyped command. The swap is priced as the reference
    /// prices it, at the cost of the substitution in the same cell.
    /// </summary>
    internal static int EditDistance(string one, string two)
    {
        int[,] matrix = new int[one.Length + 1, two.Length + 1];
        for (int row = 0; row <= one.Length; row++)
            matrix[row, 0] = row;
        for (int column = 0; column <= two.Length; column++)
            matrix[0, column] = column;
        for (int row = 1; row <= one.Length; row++)
        {
            for (int column = 1; column <= two.Length; column++)
            {
                int cost = one[row - 1] == two[column - 1] ? 0 : 1;
                int distance = Math.Min(
                    matrix[row, column - 1] + 1,
                    Math.Min(matrix[row - 1, column] + 1, matrix[row - 1, column - 1] + cost));
                if (row > 1 && column > 1
                    && one[row - 1] == two[column - 2]
                    && one[row - 2] == two[column - 1])
                {
                    distance = Math.Min(distance, matrix[row - 2, column - 2] + cost);
                }
                matrix[row, column] = distance;
            }
        }
        return matrix[one.Length, two.Length];
    }

    /// <summary>
    /// Bare <c>/ub</c> (and <c>/vt ub</c>): which build of the compatibility
    /// surface is loaded. A macro that behaves differently between two
    /// clients needs one line it can be asked for.
    /// </summary>
    private void WriteUbVersion()
    {
        // The reference prints the two as one message, so the second line
        // carries no tag of its own.
        WriteUbMessage(
            $"MossTank UB compatibility {PluginVersion}",
            [" Type `/ub help` or `/ub help <command>` for help."]);
    }

    /// <summary>
    /// <c>/vt help [command]</c>. Bare, the macro's own verb lists; named, the
    /// usage of one of MossTank's own additions to them.
    /// </summary>
    private void HandleVtankHelpCommand(string arguments)
    {
        string requested = arguments.Trim();
        if (requested.Length != 0)
        {
            if (!VtankCommandHelp.TryGet(requested, out UbCommandUsage usage))
            {
                WriteVtank($"No help found for command: {requested}");
                return;
            }
            WriteVtank("Syntax: " + usage.Usage);
            WriteVtank(usage.Summary);
            return;
        }
        foreach (string line in VtankHelp)
            WriteVtank(line);
        WriteVtank("MossTank /vt — documented: "
            + string.Join(", ", VtankCommandHelp.Entries.Select(entry => entry.Name)));
        WriteVtank("For help with a specific command, use /vt help [command].");
        WriteVtank("UtilityBelt commands answer on /ub; type /ub help for them.");
    }

    /// <summary>
    /// <c>/ub help [command]</c>, in the reference's words: a known command
    /// prints its usage and description, anything else (or nothing) the whole
    /// list of commands.
    /// </summary>
    private void HandleUbHelpCommand(string arguments)
    {
        string requested = arguments.Trim();
        if (requested.Length != 0
            && UbCommandHelp.TryGet(requested, out UbCommandUsage usage))
        {
            WriteUbCommandHelp(usage);
            return;
        }
        WriteUbMessage(
            "All available UB commands: /ub {"
                + string.Join(", ", UbCommandHelp.Entries.Select(static entry => entry.Name))
                + "}",
            ["For help with a specific command, use `/ub help [command]`"]);
    }

    /// <summary>
    /// <c>/ub mexec &lt;expression&gt;</c>, in the reference's words: the
    /// expression, then its result with the result's type and how long it
    /// took. A true or false is a number to it, as it is to the expression
    /// language it mirrors. <c>/ub mexecm</c> is the silent form: the
    /// expression runs and only an error is printed.
    /// </summary>
    private void ExecuteUbExpression(string source, bool silent = false)
    {
        if (!silent)
            UbChat.PostExpression(_host.Automation.Chat, $"Evaluating expression: \"{source}\"");
        var watch = System.Diagnostics.Stopwatch.StartNew();
        ExpressionValue result;
        try
        {
            result = _expressions.Evaluate(source);
        }
        catch (Exception error)
        {
            // One message in the reference: the error under the tag, then
            // the reason indented on the line below it.
            WriteUbErrorMessage($"Error in expression: {source}", ["  " + error.Message]);
            return;
        }
        watch.Stop();
        if (silent)
            return;
        double milliseconds = Math.Round(watch.Elapsed.TotalMilliseconds, 3);
        string type = ReferenceArguments.FriendlyTypeName(result);
        string text = UbValueText(result);
        UbChat.PostExpression(
            _host.Automation.Chat,
            Invariant($"Result: [{type}] {text} ({milliseconds}ms)"));
    }

    /// <summary>
    /// A value as the reference prints it beside its type name: a true or
    /// false is the number 1 or 0 to it, everything else its display text.
    /// </summary>
    private static string UbValueText(in ExpressionValue value) =>
        value.Kind == ExpressionValueKind.Boolean
            ? (value.IsTruthy ? "1" : "0")
            : value.ToDisplayString();

    /// <summary>
    /// <c>/ub opt {list | get &lt;option&gt; | set &lt;option&gt;
    /// &lt;newValue&gt; | toggle &lt;option&gt;}</c>: the UB settings, and
    /// only those -- the macro's own options are <c>/vt opt</c>'s.
    /// </summary>
    private void HandleUbOptionCommand(string arguments)
    {
        (string operation, string tail) = SplitHead(arguments.Trim());
        (string name, string rawValue) = SplitHead(tail);
        switch (operation.ToLowerInvariant())
        {
            case "list" when tail.Length == 0:
                // The reference lists its profile settings before the
                // character's own state.
                WriteUbMessage(
                    "All Settings:",
                    _ubCatalog.Settings
                        .OrderBy(static setting => setting.Scope == UbSettingScope.Character)
                        .Select(static setting => setting.FullDisplayValue()));
                return;
            case "get" when name.Length != 0 && rawValue.Length == 0:
                if (!TryWriteUbSetting(name))
                    WriteUbInvalidOption(name);
                return;
            // A set with nothing after the name is not a set: the reference
            // answers it with its usage.
            case "set" when name.Length != 0 && rawValue.Length != 0:
                if (!TrySetUbSetting(name, rawValue))
                    WriteUbInvalidOption(name);
                return;
            case "toggle" when name.Length != 0 && rawValue.Length == 0:
                ToggleUbSetting(name);
                return;
            default:
                WriteUbBadSyntax("opt");
                return;
        }
    }

    // ── /ub setmotion ───────────────────────────────────────────────────

    [GeneratedRegex(
        @"^(?<motion>\w.+) (?<state>[01])$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant)]
    private static partial Regex SetMotionPattern();

    /// <summary>
    /// <c>/ub setmotion &lt;motion&gt; &lt;0|1&gt;</c>: presses or releases
    /// one movement key and leaves it that way, through the same held keys
    /// the <c>setmotion[]</c> expression uses.
    /// </summary>
    private void HandleSetMotionCommand(string arguments)
    {
        Match match = SetMotionPattern().Match(arguments.Trim());
        if (!match.Success)
        {
            WriteUbBadSyntax("setmotion");
            return;
        }
        string name = match.Groups["motion"].Value;
        if (!HeldMotions.TryParse(name, out HeldMotion motion))
        {
            WriteUbError(
                $"Invalid option ({name}). Valid values are: {HeldMotions.ValidNames}");
            return;
        }
        _expressions.HeldMotions.Set(motion, match.Groups["state"].Value == "1");
    }

    // ── /ub ig ──────────────────────────────────────────────────────────

    /// <summary>
    /// Says how a hand-over ended -- finished, stopped, given up or its target
    /// gone -- as a think when ItemGiver.Think is on and as a plain line
    /// otherwise, which is how the reference reports every end of a run.
    /// </summary>
    private void ReportGiveEnd()
    {
        // A run that bailed says so first, as the tool's error.
        if (_profileGive.EndError.Length != 0)
            PostUb(UbChat.ToolError(UbChat.Tools.InventoryManager, _profileGive.EndError));
        if (_ubCatalog.Require("ItemGiver.Think").Get().Boolean)
            Think(_profileGive.FinishedLine);
        else
            WriteUb(_profileGive.FinishedLine);
    }

    /// <summary>
    /// Starts a hand-over, or says why not as the reference's inventory tool
    /// does: a second run while one is going, then whatever the start
    /// refused. A run that starts says nothing until it ends.
    /// </summary>
    private void StartGive(Func<bool> start, string stopVerb)
    {
        if (_profileGive.IsRunning)
        {
            PostUb(UbChat.ToolError(
                UbChat.Tools.InventoryManager,
                "Already running.  Please wait until it completes or use "
                + $"/ub {stopVerb} stop to quit previous session"));
            return;
        }
        if (start())
            return;
        PostUb(UbChat.ToolError(UbChat.Tools.InventoryManager, _profileGive.Refusal));
    }

    /// <summary>
    /// A stop: the run's end line when one was going, the reference's error
    /// when none was.
    /// </summary>
    private void StopGive()
    {
        if (_profileGive.StopRequested())
            ReportGiveEnd();
        else
            WriteUbError("ItemGiver is not running.");
    }

    /// <summary>
    /// <c>/ub ig[p] &lt;lootProfile&gt; to &lt;target&gt;</c>, and
    /// <c>/ub ig stop</c> to call a run off: the chat form of the profile
    /// hand-over the expression engine already exposes.
    /// </summary>
    private void HandleItemGiverCommand(bool partialTarget, string arguments)
    {
        string text = arguments.Trim();
        if (text.Equals("stop", StringComparison.OrdinalIgnoreCase)
            || text.Equals("cancel", StringComparison.OrdinalIgnoreCase)
            || text.Equals("quit", StringComparison.OrdinalIgnoreCase)
            || text.Equals("abort", StringComparison.OrdinalIgnoreCase))
        {
            StopGive();
            return;
        }

        // Split at the FIRST " to " for the same reason the name-give does:
        // the profile name is the short half of the line.
        int separator = text.IndexOf(" to ", StringComparison.OrdinalIgnoreCase);
        if (separator <= 0)
        {
            WriteUbBadSyntax("ig");
            return;
        }
        string profile = StripExtension(text[..separator].Trim(), ".utl", ".json");
        string target = text[(separator + 4)..].Trim();
        StartGive(() => _profileGive.TryStart(profile, target, partialTarget), "ig");
    }

    // ── /ub jump, /ub simplejump ────────────────────────────────────────

    /// <summary>
    /// <c>/ub jump[swzxc] [heading] [holdtime]</c>: the reference's grammar.
    /// <c>s</c> holds shift, the game's walk key, so the jump is a walking
    /// one rather than the default run, <c>w</c> presses
    /// forward, <c>x</c> backward, <c>z</c> and <c>c</c> strafe left and
    /// right; with no letter at all the character jumps straight up.
    /// </summary>
    /// <remarks>
    /// The macro's own <c>/vt jump &lt;heading&gt; &lt;true|false&gt;
    /// &lt;ms&gt; [direction]</c> is a different command on the other word;
    /// this grammar does not read it.
    /// </remarks>
    private void HandleUbJumpCommand(string flags, string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length > 2)
        {
            WriteUbBadSyntax("jump");
            return;
        }

        // Two numbers are heading then hold; one number on its own is the
        // hold, because a jump in place is the common case.
        string? headingText = parts.Length == 2 ? parts[0] : null;
        string? holdText = parts.Length switch
        {
            2 => parts[1],
            1 => parts[0],
            _ => null,
        };

        if (!TryParseHoldTime(holdText, out int milliseconds))
            return;
        float heading = _host.Automation.Navigation.Snapshot.Position.HeadingDegrees;
        if (headingText is not null)
        {
            // The reference's grammar takes a heading of digits and points
            // only, so "NaN", "Infinity", "1e3" and "-5" never read as one.
            if (!IsPlainDecimal(headingText)
                || !double.TryParse(
                    headingText,
                    NumberStyles.AllowDecimalPoint,
                    CultureInfo.InvariantCulture,
                    out double requested))
            {
                WriteUbBadSyntax("jump");
                return;
            }
            if (requested is < 0d or > 359d)
            {
                PostUb(UbChat.ToolError(
                    UbChat.Tools.Jumper,
                    "direction should be a number between 0 and 359"));
                return;
            }
            heading = (float)requested;
        }

        if (!RefuseWhenAlreadyJumping())
            return;
        // With no heading the reference does not turn at all; with one it
        // has the client turn there first.
        StartCommandJump(
            heading,
            shift: flags.Contains('s'),
            milliseconds,
            direction: null,
            turn: headingText is null ? CommandJumpTurn.None : CommandJumpTurn.Client,
            keys: JumpKeysFromFlags(flags));
    }

    /// <summary>True when the text is nothing but digits and decimal points.</summary>
    private static bool IsPlainDecimal(string text) =>
        text.Length != 0 && text.All(static letter => letter is '.' or (>= '0' and <= '9'));

    /// <summary>
    /// <c>/ub simplejump [holdtime]</c>: a jump where the character already
    /// stands, with no turn first.
    /// </summary>
    private void HandleSimpleJumpCommand(string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length > 1)
        {
            WriteUbBadSyntax("simplejump");
            return;
        }
        if (!TryParseHoldTime(parts.Length == 1 ? parts[0] : null, out int milliseconds))
            return;
        if (!RefuseWhenAlreadyJumping())
            return;
        StartCommandJump(
            _host.Automation.Navigation.Snapshot.Position.HeadingDegrees,
            shift: false,
            milliseconds,
            RouteJumpDirection.Forward,
            omitDirection: true,
            turn: CommandJumpTurn.None);
    }

    /// <summary>
    /// The hold in milliseconds, or 0 when none was typed. False when it was
    /// typed but is not a number the client can hold a jump key for.
    /// </summary>
    private bool TryParseHoldTime(string? text, out int milliseconds)
    {
        milliseconds = 0;
        if (text is null)
            return true;
        if (!int.TryParse(
            text,
            NumberStyles.Integer,
            CultureInfo.InvariantCulture,
            out milliseconds))
        {
            PostUb(UbChat.ToolError(
                UbChat.Tools.Jumper,
                "holdtime should be a number between 0 and 1000"));
            return false;
        }
        if (milliseconds is < 0 or > 1000)
        {
            PostUb(UbChat.ToolError(
                UbChat.Tools.Jumper,
                "holdtime should be a number between 0 and 1000"));
            milliseconds = 0;
            return false;
        }
        return true;
    }

    /// <summary>
    /// False, having said so, when a jump is already in the air. Two charges
    /// at once would fight over the movement keys.
    /// </summary>
    private bool RefuseWhenAlreadyJumping()
    {
        if (!_commandJumpActive)
            return true;
        PostUb(UbChat.ToolError(
            UbChat.Tools.Jumper,
            "You are already jumping. try again later."));
        return false;
    }

    /// <summary>
    /// The movement keys a jump's flag letters hold, each letter its own key
    /// as the reference sets them: w forward, x backward, z left, c right.
    /// Two letters hold two keys, so <c>jumpwz</c> leaps forward and to the
    /// left at once; no letter holds none, a jump straight up.
    /// </summary>
    private static PluginMovementIntent JumpKeysFromFlags(string flags) => new(
        Forward: flags.Contains('w'),
        Backward: flags.Contains('x'),
        StrafeLeft: flags.Contains('z'),
        StrafeRight: flags.Contains('c'),
        Run: !flags.Contains('s'),
        Jump: true);

    // ── /ub calcdamage ──────────────────────────────────────────────────

    /// <summary>
    /// <c>/ub calcdamage</c>: what the selected missile weapon would hit for
    /// once every tinker it can still take has gone in. Only the cantrips on
    /// the weapon itself count; nothing the character is wearing does.
    /// </summary>
    private void CalculateSelectedDamage()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u
            || !_host.Automation.Objects.TryGet(selected, out PluginWorldObject item))
        {
            WriteUbError("Nothing selected");
            return;
        }
        if (!_host.Automation.Objects.TryCaptureProperties(
                selected,
                out PluginItemProperties properties)
            || properties.WeaponProfile is not { } profile)
        {
            WriteUbError($"{item.Name} does not have id data, please examine it first.");
            return;
        }
        if (item.ObjectClass != PluginObjectClass.MissileWeapon)
        {
            WriteUbError($"Calc Damage: {item.ObjectClass} is not currently supported");
            return;
        }

        double maxDamage = profile.Damage;
        double damageBonus = Math.Round(profile.DamageMod, 2);
        int elementalBonus = ReadInt(properties, ElementalDamageBonusProperty);
        // Walked in the order the item carries its spells, and each one that
        // lifts the maximum damage is named: the "+N from cantrips" total
        // below says nothing about which spells made it.
        int cantripBonus = 0;
        foreach (uint spellId in item.SpellIds)
        {
            int bonus = MaxDamageCantripBonus(spellId);
            if (bonus == 0)
                continue;
            cantripBonus += bonus;
            WriteUb(Invariant(
                $"Spell {SpellName(spellId)} buffs MaxDamage by {bonus}"));
        }
        int timesTinkered = ReadInt(properties, NumberTimesTinkeredProperty);
        bool imbued = ReadInt(properties, ImbuedProperty) > 0;
        int tinkersAvailable = 10 - Math.Min(10, timesTinkered + (imbued ? 0 : 1));
        double perTinker = tinkersAvailable * 0.04d;
        double damage = (maxDamage + cantripBonus + elementalBonus)
            * (damageBonus + perTinker - 1d);

        if (tinkersAvailable > 0)
        {
            WriteUb(Invariant(
                $"{tinkersAvailable} mahogany salvage adds {perTinker} to DamageModifier"));
        }
        WriteUb("Formula: (DamageBonus + ElementalBonus) * DamageModifier");
        WriteUb(Invariant(
            $"Calculated Formula: ({maxDamage}(+{cantripBonus} from cantrips) + {elementalBonus}) * {damageBonus - 1d}(+{perTinker} from {tinkersAvailable} tinkers)"));
        WriteUb(Invariant($"Calculated (after tinks): {damage}"));
    }

    /// <summary>
    /// What one spell adds to an item's maximum damage, or zero when it is
    /// not one of the cantrips that does.
    /// </summary>
    private static int MaxDamageCantripBonus(uint spellId)
    {
        foreach ((uint candidate, int bonus) in MaxDamageCantrips)
        {
            if (candidate == spellId)
                return bonus;
        }
        return 0;
    }

    /// <summary>
    /// The spell's name, falling back to its number when this client's
    /// catalog has no entry for it.
    /// </summary>
    private string SpellName(uint spellId) =>
        _host.Automation.Spells.TryGet(spellId, out PluginSpellInfo spell)
            && spell.Name.Length > 0
                ? spell.Name
                : spellId.ToString(CultureInfo.InvariantCulture);

    private static int ReadInt(in PluginItemProperties properties, uint key) =>
        properties.Ints.TryGetValue(key, out int value) ? value : 0;

    // ── /ub pos, /ub id, /ub vitae, /ub combatstate, /ub date ───────────

    /// <summary>
    /// <c>/ub pos</c>: where the selected object stands, in every form the
    /// client can say it -- the id, the compass coordinates, the landcell and
    /// the raw position inside it.
    /// </summary>
    private void PrintSelectedPosition()
    {
        if (!TryGetSelectedObject("pos", out PluginWorldObject item))
            return;
        if (!item.HasPosition)
        {
            WriteUb($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
            WriteUbError("pos: the client knows no position for that object.");
            return;
        }
        PluginNavigationPosition position = item.Position;
        var coordinates = new ExpressionCoordinates(
            position.EastWest,
            position.NorthSouth,
            position.Elevation);
        WriteUb($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
        WriteUb($"Coords: {coordinates.ToCompassText()}");
        WriteUb($"Landcell: 0x{position.CellId:X8}");
        WriteUb(Invariant(
            $"Position: ew:{position.EastWest} ns:{position.NorthSouth} z:{position.Elevation}"));
        WriteUb(Invariant(
            $"Distance: {UbObjectSearch.Distance(_host.Automation.Navigation.Snapshot, item):0.###} m"));
    }

    /// <summary><c>/ub id</c>: the selected object's id, both ways round.</summary>
    private void PrintSelectedId()
    {
        if (!TryGetSelectedObject("Id", out PluginWorldObject item))
            return;
        WriteUb($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
    }

    private bool TryGetSelectedObject(string verb, out PluginWorldObject item)
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u)
        {
            WriteUbError($"{verb}: No object selected");
            item = default;
            return false;
        }
        if (!_host.Automation.Objects.TryGet(selected, out item))
        {
            WriteUbError($"{verb}: null object selected");
            return false;
        }
        return true;
    }

    /// <summary>
    /// <c>/ub vitae</c>: says the penalty out loud, as a think, so a macro
    /// watching its own chat can read it back.
    /// </summary>
    private void ThinkVitae()
    {
        // Vitae is the float multiplier the server keeps on the character,
        // 1.0 with no penalty; nothing else carries it.
        double multiplier = 1d;
        if (_host.Automation.Objects.TryCaptureProperties(
            _host.Automation.Character.ObjectId,
            out PluginItemProperties properties)
            && properties.Floats.TryGetValue(129u, out double value))
        {
            multiplier = value;
        }
        Think(Invariant($"My vitae is {Math.Round(multiplier * 100d)}%"));
    }

    /// <summary>
    /// Says something to yourself the way the client does, so it reaches the
    /// chat window through the same road a typed think takes.
    /// </summary>
    private void Think(string text) => UbChat.Think(_host.Automation, text);

    /// <summary>
    /// <c>/ub combatstate (peace|melee|missile|magic)</c>: asks the client to
    /// change stance, without the macro having to be running.
    /// </summary>
    private void HandleCombatStateCommand(string arguments)
    {
        string requested = arguments.Trim().ToLowerInvariant();
        PluginCombatMode mode;
        switch (requested)
        {
            case "peace":
                mode = PluginCombatMode.Peace;
                break;
            case "melee":
                mode = PluginCombatMode.Melee;
                break;
            case "missile":
                mode = PluginCombatMode.Missile;
                break;
            case "magic":
                mode = PluginCombatMode.Magic;
                break;
            default:
                WriteUbError($"{requested} is not a valid option");
                return;
        }
        PluginCombatCommandResult result = _host.Automation.Combat.EnterMode(mode);
        if (result.Accepted)
            WriteUb($"Combat state set to {mode}.");
        else
            WriteUbError($"Could not set combat state to {mode}: {result.Status}");
    }

    /// <summary>
    /// <c>/ub date[utc] [format]</c>. The format is a .NET custom date
    /// format, read and printed in the invariant culture so that a macro
    /// written on one machine reads the same on every other.
    /// </summary>
    private void PrintDate(bool utc, string format)
    {
        string pattern = format.Trim();
        if (pattern.Length == 0)
            pattern = DefaultDateFormat;
        DateTime now = utc ? DateTime.UtcNow : DateTime.Now;
        try
        {
            WriteUb("Current Date: "
                + now.ToString(pattern, CultureInfo.InvariantCulture));
        }
        catch (FormatException error)
        {
            WriteUb(error.Message);
        }
    }

    // ── /ub delay ───────────────────────────────────────────────────────

    /// <summary>
    /// <c>/ub delay &lt;milliseconds&gt; &lt;command&gt;</c>: hands the line
    /// back to the client's own command routing once the delay is up, so it
    /// reaches whichever plugin owns the verb.
    /// </summary>
    private void HandleDelayCommand(string arguments)
    {
        (string head, string rest) = SplitHead(arguments);
        if (rest.Length == 0
            || !double.TryParse(
                head,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out double milliseconds)
            || !double.IsFinite(milliseconds)
            || milliseconds <= 0d)
        {
            WriteUbBadSyntax("delay");
            return;
        }
        ScheduleUbCommand(rest, milliseconds);
    }

    /// <summary>
    /// Queues a line to run once its delay is up. The reference says so only
    /// in its debug output, and again when the line runs.
    /// </summary>
    private void ScheduleUbCommand(string command, double milliseconds)
    {
        WriteUbDebug(Invariant(
            $"Scheduling command `{command}` with delay of {milliseconds}ms"));
        _delayedCommands.Add((milliseconds / 1000d, command, milliseconds));
        _delayedCommands.Sort(static (left, right) =>
            left.RemainingSeconds.CompareTo(right.RemainingSeconds));
    }

    /// <summary>Runs the delayed commands whose wait is over.</summary>
    private void TickDelayedCommands(double elapsedSeconds)
    {
        if (_delayedCommands.Count == 0)
            return;
        double step = Math.Max(0d, elapsedSeconds);
        var due = new List<(string Command, double DelayMilliseconds)>();
        for (int index = _delayedCommands.Count - 1; index >= 0; index--)
        {
            (double remaining, string command, double delay) = _delayedCommands[index];
            remaining -= step;
            if (remaining > 0d)
            {
                _delayedCommands[index] = (remaining, command, delay);
                continue;
            }
            due.Add((command, delay));
            _delayedCommands.RemoveAt(index);
        }
        // The loop above walks backwards, so the oldest entry comes out last;
        // a pair scheduled together must run in the order they were typed.
        for (int index = due.Count - 1; index >= 0; index--)
        {
            (string command, double delay) = due[index];
            WriteUbToolDebug(
                UbChat.Tools.Plugin,
                Invariant($"Executing command `{command}` (delay was {delay}ms)"));
            _host.Automation.Chat.Submit(command);
        }
    }

    // ── /ub opt ─────────────────────────────────────────────────────────

    /// <summary>
    /// A typed get, set or toggle of a name no setting has, as the reference
    /// answers it: the name in lower case as its error, then one error line
    /// per character setting -- its per-character state, not every setting
    /// -- each "  - " and the setting's name.
    /// </summary>
    private void WriteUbInvalidOption(string name)
    {
        WriteUbError("Invalid option: " + name.Trim().ToLowerInvariant());
        foreach (UbSetting setting in _ubCatalog.Settings)
        {
            if (setting.Scope == UbSettingScope.Character)
                WriteUbError("  - " + setting.Name);
        }
    }

    /// <summary>Prints one UB setting. False when there is no such row.</summary>
    private bool TryWriteUbSetting(string name)
    {
        if (!_ubCatalog.TryGet(name.Trim(), out UbSetting setting))
            return false;
        WriteUb(setting.FullDisplayValue());
        return true;
    }

    /// <summary>
    /// Writes one UB setting from typed text. False when there is no such
    /// row; a row that exists but was given a value of the wrong shape says
    /// so and still counts as handled.
    /// </summary>
    private bool TrySetUbSetting(string name, string rawValue)
    {
        if (!_ubCatalog.TryGet(name.Trim(), out UbSetting setting))
            return false;
        if (setting.Kind == UbSettingKind.Collection)
        {
            EditUbList(setting, rawValue.Trim());
            return true;
        }
        if (!UbSettingValue.TryParse(
            setting.Kind,
            rawValue.Trim(),
            out UbSettingValue value))
        {
            WriteUbError(
                $"Option set: Invalid value specified. {setting.Name} is a "
                + $"{setting.Kind}.");
            return true;
        }
        setting.Set(value);
        EchoUbSettingChange(setting);
        return true;
    }

    /// <summary>
    /// The line a set or a toggle leaves. The reference writes the new value
    /// as an ordinary line only while debug is off; with it on, the same
    /// line comes from its change log instead, as a debug line.
    /// </summary>
    private void EchoUbSettingChange(UbSetting setting)
    {
        string text = setting.FullDisplayValue();
        if (UbDebug)
            WriteUbDebug(text);
        else
            WriteUb(text);
    }

    /// <summary>
    /// <c>/ub quit</c>: closes the client by the route its own close button
    /// takes -- a graceful logout, then shutting down. A client with no
    /// window ends this session the same way.
    /// </summary>
    private void QuitClient()
    {
        WriteUb("Quitting Client");
        HostWindowResult result = _host.Window.RequestClose();
        if (!result.Succeeded)
            WriteUbError(result.Notice ?? "The client could not be closed from here.");
    }

    /// <summary>
    /// <c>/ub playeroption (list|&lt;option&gt; &lt;on|true|off|false&gt;)</c>:
    /// turns one of the character's own options on or off, by the name the
    /// character options page gives it. Anything but on or true turns it
    /// off, as the original reads it.
    /// </summary>
    private void HandlePlayerOptionCommand(string arguments)
    {
        ICharacterOptionsAutomation options = _host.Automation.CharacterOptions;
        string text = arguments.Trim();
        if (text.Equals("list", StringComparison.OrdinalIgnoreCase))
        {
            WriteUb($"Valid values are: {string.Join(", ", options.Names)}");
            return;
        }
        string[] parts = text.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length != 2)
        {
            WriteUbError("Usage: /ub playeroption <option> <on/true|off/false>");
            return;
        }
        string value = parts[1].ToLowerInvariant();
        bool on = value is "on" or "true";
        PluginCharacterOptionResult result = options.Set(parts[0], on);
        switch (result.Status)
        {
            case PluginCharacterOptionStatus.Accepted:
                WriteUb($"Setting {CanonicalOptionName(options, parts[0])} = {on}");
                return;
            case PluginCharacterOptionStatus.UnknownOption:
                WriteUbError($"Invalid option. Valid values are: {string.Join(", ", options.Names)}");
                return;
            default:
                WriteUbError(
                    $"Unable to set {parts[0]}: "
                    + (result.Notice ?? result.Status.ToString()));
                return;
        }
    }

    private static string CanonicalOptionName(ICharacterOptionsAutomation options, string name) =>
        options.Names.FirstOrDefault(candidate =>
            candidate.Equals(name, StringComparison.OrdinalIgnoreCase)) ?? name;

    /// <summary>
    /// <c>uboptget[name]</c>: a UB setting's value, a list setting as a list
    /// of its lines. A name that is not a UB setting reads the macro's own
    /// option of that name, which the original would refuse; nothing a UB
    /// name could be is one of those, since every UB name is dotted. A name
    /// that is neither is the reference's invalid option: an error and 0.
    /// </summary>
    private ExpressionValue GetUbOption(string name)
    {
        if (!_ubCatalog.TryGet(name.Trim(), out UbSetting setting))
        {
            if (VtankOptionCatalog.IsKnown(name))
                return GetMetaOption(name);
            WriteUbError("Invalid option: " + name.ToLowerInvariant());
            return ExpressionValue.Zero;
        }
        UbSettingValue value = setting.Get();
        return value.Kind switch
        {
            UbSettingKind.Bool => ExpressionValue.Boolean(value.Boolean),
            UbSettingKind.String => ExpressionValue.String(value.Text),
            UbSettingKind.Collection => ExpressionValue.List(new ExpressionList(
                value.Items.Select(static item => ExpressionValue.String(item)))),
            _ => ExpressionValue.Number(value.Number),
        };
    }

    /// <summary>
    /// <c>uboptset[name,value]</c>: writes a UB setting, and says the new
    /// value in chat as a typed <c>opt set</c> does. A list setting takes a
    /// list and is replaced by it. A name that is not a UB setting writes
    /// the macro's own option of that name; a name that is neither is the
    /// reference's invalid option: an error and 0.
    /// </summary>
    private bool SetUbOption(string name, ExpressionValue value)
    {
        if (!_ubCatalog.TryGet(name.Trim(), out UbSetting setting))
        {
            if (VtankOptionCatalog.IsKnown(name))
                return SetMetaOption(name, value);
            WriteUbError("Invalid option: " + name.ToLowerInvariant());
            return false;
        }
        if (setting.Kind == UbSettingKind.Collection)
        {
            if (value.Kind != ExpressionValueKind.List)
            {
                WriteUbError($"{setting.Name} expects a value of type list");
                return false;
            }
            setting.Set(UbSettingValue.FromCollection(
                value.AsList().Items.Select(static item => item.ToDisplayString())));
            return true;
        }
        if (!TryConvertUbOption(setting.Kind, value, out UbSettingValue converted))
        {
            WriteUbError($"{setting.Name} is a {setting.Kind}; {value.ToDisplayString()} is not.");
            return false;
        }
        setting.Set(converted);
        EchoUbSettingChange(setting);
        return true;
    }

    private static bool TryConvertUbOption(
        UbSettingKind kind,
        ExpressionValue value,
        out UbSettingValue converted)
    {
        if (value.Kind == ExpressionValueKind.String)
            return UbSettingValue.TryParse(kind, value.AsString(), out converted);
        if (kind == UbSettingKind.String)
        {
            converted = UbSettingValue.FromText(value.ToDisplayString());
            return true;
        }
        if (value.Kind is not (ExpressionValueKind.Number or ExpressionValueKind.Boolean))
        {
            converted = default;
            return false;
        }
        double number = value.AsNumber();
        converted = kind switch
        {
            UbSettingKind.Bool => UbSettingValue.FromBool(value.IsTruthy),
            UbSettingKind.Int => UbSettingValue.FromInt(
                (int)Math.Round(number, MidpointRounding.AwayFromZero)),
            UbSettingKind.Enum => UbSettingValue.FromChoice(
                (int)Math.Round(number, MidpointRounding.AwayFromZero)),
            UbSettingKind.Single => UbSettingValue.FromSingle((float)number),
            UbSettingKind.Double => UbSettingValue.FromDouble(number),
            UbSettingKind.Color => UbSettingValue.FromColor(unchecked((uint)(long)number)),
            _ => default,
        };
        return kind is not UbSettingKind.Collection;
    }

    /// <summary>
    /// A list setting is edited, never replaced, from the command line:
    /// <c>add a[,b]</c> appends each name it does not already hold,
    /// <c>remove a[,b]</c> takes each out, and <c>clear</c> empties it. This
    /// is how a macro puts its own broadcast tag on the list without
    /// wiping the tags someone else put there. Names are compared exactly
    /// and taken as written between the commas.
    /// </summary>
    private void EditUbList(UbSetting setting, string edit)
    {
        string[] parts = edit.Split(' ', 2, StringSplitOptions.RemoveEmptyEntries);
        string verb = parts[0].ToLowerInvariant();
        string names = parts.Length > 1 ? parts[1] : string.Empty;
        var items = setting.Get().Items.ToList();
        switch (verb)
        {
            case "add":
                if (names.Trim().Length == 0)
                {
                    WriteUbDebug("Missing items to add");
                    return;
                }
                foreach (string name in names.Split(','))
                {
                    if (!items.Contains(name, StringComparer.Ordinal))
                        items.Add(name);
                }
                break;
            case "remove":
                if (names.Trim().Length == 0)
                {
                    WriteUbDebug("Missing items to remove");
                    return;
                }
                foreach (string name in names.Split(','))
                {
                    int index = items.FindIndex(item =>
                        item.Equals(name, StringComparison.Ordinal));
                    if (index >= 0)
                        items.RemoveAt(index);
                }
                break;
            case "clear":
                items.Clear();
                break;
            default:
                // The reference names the word after the verb, not the verb:
                // "replace a,b" answers "Unknown verb: a,b". A verb on its own
                // has no such word and the reference prints nothing at all;
                // naming the verb itself is the one departure, so the line is
                // never silently ignored.
                WriteUbError("Unknown verb: " + (parts.Length > 1 ? parts[1] : parts[0]));
                return;
        }
        setting.Set(UbSettingValue.FromCollection(items));
        WriteUb(setting.FullDisplayValue());
    }

    /// <summary>
    /// <c>/vt opt toggle &lt;option&gt;</c>: flips one of the macro's own
    /// switches. Anything that is not a switch is left alone, because there
    /// is no second value to flip to.
    /// </summary>
    private void ToggleVtankOption(string name)
    {
        string requested = name.Trim();
        if (requested.Length == 0)
        {
            WriteVtank("Syntax: /vt opt toggle <option>");
            return;
        }
        if (!VtankOptionCatalog.IsKnown(requested))
        {
            WriteVtank("Option toggle: Invalid option specified.");
            return;
        }
        string canonical = VtankOptionCatalog.Canonical(requested);
        if (VtankOptionCatalog.DeclaredType(canonical) != VtankSettingValueType.Bool)
        {
            WriteVtank($"Unable to toggle setting {canonical}: it is not a switch.");
            return;
        }
        SetMetaOption(
            canonical,
            ExpressionValue.Boolean(!GetMetaOption(canonical).IsTruthy));
        WriteVtank($"Set option {canonical} = {GetMetaOption(canonical).ToDisplayString()}");
    }

    /// <summary>
    /// <c>/ub opt toggle &lt;option&gt;</c>: flips one of the UB switches,
    /// refusing anything that is not one in the reference's words.
    /// </summary>
    private void ToggleUbSetting(string name)
    {
        string requested = name.Trim();
        if (!_ubCatalog.TryGet(requested, out UbSetting setting))
        {
            WriteUbInvalidOption(requested);
            return;
        }
        if (setting.Kind != UbSettingKind.Bool)
        {
            // The reference's toggle reads the value as a true/false and
            // prints the runtime's own message for the failed read.
            WriteUbError($"Unable to toggle setting {setting.Name}: Specified cast is not valid.");
            return;
        }
        setting.Set(UbSettingValue.FromBool(!setting.Get().Boolean));
        EchoUbSettingChange(setting);
    }

    // ── /ub closestportal, /ub portal ───────────────────────────────────

    /// <summary>
    /// Uses a portal by name, or -- with a blank name -- the nearest one.
    /// </summary>
    private void UsePortalByName(string name, bool partial)
    {
        if (!UbObjectSearch.TryFindNearest(
            _host,
            name,
            partial,
            PortalClasses,
            out PluginWorldObject portal))
        {
            if (_ubCatalog.Require("Plugin.PortalThink").Get().Boolean)
                Think("Could not find a portal");
            else
                WriteUb("Could not find a portal");
            return;
        }
        WriteUb($"Attempting to use portal: {portal.Name}");
        WriteUbToolDebug(UbChat.Tools.Plugin, "Attempting to use portal " + portal.Name);
        PluginItemCommandResult result = _host.Automation.Items.Use(portal.ObjectId);
        if (!result.Accepted)
        {
            PostUb(UbChat.Tool(
                UbChat.Tools.Plugin,
                $"Unable to use portal {portal.Name}: {result.Status}"));
        }
    }

    // ── /ub follow ──────────────────────────────────────────────────────

    /// <summary>
    /// <c>/ub follow[p] &lt;name&gt;</c>: switches to the follow route and
    /// aims it at that player. The route that was loaded is left as it is,
    /// file and all.
    /// </summary>
    private void HandleFollowCommand(string name, bool partial)
    {
        string requested = name.Trim();
        if (!UbObjectSearch.TryFindNearest(
            _host,
            requested,
            partial,
            PlayerClasses,
            out PluginWorldObject player))
        {
            WriteUbError(requested.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {requested}");
            return;
        }
        if (!FollowOnFollowRoute(player.ObjectId, player.Name))
        {
            WriteUbError($"Failed to follow {player.Name}[0x{player.ObjectId:X8}]");
            return;
        }
        WriteUb($"Following {player.Name}[0x{player.ObjectId:X8}]");
        if (!_navigationSettings.Enabled)
            WriteUb("Turn Enable Navigation on to start.");
    }

    // ── /ub use, /ub select, /ub close ──────────────────────────────────

    /// <summary>
    /// <c>/ub use[li][p] [itemOne] on [itemTwo]</c>. One name uses that
    /// object -- a portal by the portal road, a vendor by opening it, a
    /// container or corpse by opening it, anything else by a plain use. Two
    /// names apply the first to the second.
    /// </summary>
    private void HandleUseCommand(string flags, string arguments)
    {
        if (!TryReadScope(flags, out UbSearchScope scope, out bool partial))
            return;
        string text = arguments.Trim();
        if (text.Length == 0)
        {
            WriteUbBadSyntax("use");
            return;
        }

        string first = text;
        string? second = null;
        int separator = text.IndexOf(" on ", StringComparison.OrdinalIgnoreCase);
        if (separator > 0)
        {
            first = text[..separator].Trim();
            second = text[(separator + 4)..].Trim();
        }

        if (!UbObjectSearch.TryFind(
            _host, first, scope, partial, 0u, out PluginWorldObject one))
        {
            WriteUbError($"Could not find object: {first}");
            return;
        }
        if (string.IsNullOrEmpty(second))
        {
            UseSingleObject(one, partial);
            return;
        }
        if (!UbObjectSearch.TryFind(
            _host,
            second,
            UbSearchScope.All,
            partial,
            one.ObjectId,
            out PluginWorldObject two))
        {
            WriteUb($"{second} is null");
            return;
        }
        WriteUb($"using {one.Name} on {two.Name}");
        _host.Automation.Items.Apply(one.ObjectId, two.ObjectId);
    }

    private void UseSingleObject(in PluginWorldObject one, bool partial)
    {
        WriteUb("using " + one.Name);
        switch (one.ObjectClass)
        {
            case PluginObjectClass.Portal:
                UsePortalByName(one.Name, partial);
                return;
            case PluginObjectClass.Vendor:
                PostUbReply("vendor", _vendorTrade.VendorCommand("open " + one.Name));
                return;
            case PluginObjectClass.Container:
            case PluginObjectClass.Corpse:
                _host.Automation.Loot.Open(one.ObjectId);
                return;
            default:
                _host.Automation.Items.Use(one.ObjectId);
                return;
        }
    }

    /// <summary><c>/ub select[li][p] [item]</c>.</summary>
    private void HandleSelectCommand(string flags, string arguments)
    {
        if (!TryReadScope(flags, out UbSearchScope scope, out bool partial))
            return;
        string text = arguments.Trim();
        if (text.Length == 0)
        {
            WriteUbBadSyntax("select");
            return;
        }
        if (!UbObjectSearch.TryFind(
            _host, text, scope, partial, 0u, out PluginWorldObject item))
        {
            WriteUbError($"Could not find object: {text}");
            return;
        }
        _host.Selection.Select(item.ObjectId);
    }

    /// <summary>
    /// The search scope the flag letters ask for. The two place flags are a
    /// choice, not a pair: asking for both is a line that cannot be obeyed.
    /// </summary>
    private bool TryReadScope(string flags, out UbSearchScope scope, out bool partial)
    {
        partial = flags.Contains('p');
        if (flags.Contains('l') && flags.Contains('i'))
        {
            WriteUb("l and i cannot be used in the same command");
            scope = UbSearchScope.All;
            return false;
        }
        scope = flags.Contains('i')
            ? UbSearchScope.Inventory
            : flags.Contains('l')
                ? UbSearchScope.Landscape
                : UbSearchScope.All;
        return true;
    }

    /// <summary><c>/ub close corpse</c> (or chest).</summary>
    private void HandleCloseCommand(string arguments)
    {
        string requested = arguments.Trim().ToLowerInvariant();
        if (requested is not ("corpse" or "chest"))
        {
            WriteUbBadSyntax("close");
            return;
        }
        uint open = _host.Automation.Objects.OpenContainerObjectId;
        if (open == 0u)
        {
            WriteUb("No container is currently open.");
            return;
        }
        if (!_host.Automation.Objects.TryGet(open, out PluginWorldObject container))
        {
            WriteUb("No container is currently open.");
            return;
        }
        bool matches = requested == "corpse"
            ? container.ObjectClass == PluginObjectClass.Corpse
            : container.ObjectClass == PluginObjectClass.Container;
        if (!matches)
        {
            WriteUbError($"The open container is a {container.ObjectClass}, not a {requested}.");
            return;
        }
        _host.Automation.Loot.Close(open);
    }

    // ── /ub swearallegiance, /ub breakallegiance ────────────────────────

    /// <summary>
    /// Resolves who the allegiance command means and asks the client to send
    /// it. Whether the server allows it is the server's own decision and
    /// arrives later as a restated allegiance, so a sent command reports
    /// only that it went out. A miss names the verb as typed, flags and all,
    /// as the reference's error does.
    /// </summary>
    private void HandleAllegianceCommand(string verb, string name, bool partial, bool swear)
    {
        string requested = name.Trim();
        if (!UbObjectSearch.TryFindNearest(
            _host,
            requested,
            partial,
            PlayerClasses,
            out PluginWorldObject player))
        {
            WriteUbError((requested.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {requested}") + $" Command:{verb}");
            return;
        }
        string who = $"{player.Name}[0x{player.ObjectId:X8}]";
        IAllegianceAutomation allegiance = _host.Automation.Allegiance;
        PluginAllegianceCommandResult result = swear
            ? allegiance.Swear(player.ObjectId)
            : allegiance.Break(player.ObjectId);
        switch (result.Status)
        {
            case PluginAllegianceCommandStatus.Sent:
                WriteUb(swear
                    ? $"Swearing Allegiance to {who}"
                    : $"Breaking Allegiance from {who}");
                return;
            case PluginAllegianceCommandStatus.InvalidTarget:
                WriteUbError(swear
                    ? $"Cannot swear allegiance to {who}."
                    : $"{who} is not in your allegiance.");
                return;
            case PluginAllegianceCommandStatus.Refused:
                // A refusal is about the session, not about the target, so the
                // client's own reason is what is worth printing.
                WriteUbError(string.IsNullOrWhiteSpace(result.Notice)
                    ? (swear
                        ? $"Refused to swear allegiance to {who}."
                        : $"Refused to break allegiance from {who}.")
                    : result.Notice);
                return;
            default:
                WriteUbError(swear
                    ? "Cannot swear allegiance right now."
                    : "Cannot break allegiance right now.");
                return;
        }
    }

    // ── /ub printcolors ─────────────────────────────────────────────────

    /// <summary>
    /// Prints one line in each of the client's text classes, so the player can
    /// see which number gives which colour before choosing one in a profile.
    /// </summary>
    private void PrintChatColors()
    {
        foreach (UbChatMessageType type in UbChatMessageTypes.All)
        {
            _host.Automation.Chat.PostMessage(
                $"[PrintColors]{type.Name} ({type.Value})",
                type.Value);
        }
    }

    // ── /ub listvars, listpvars, listgvars ──────────────────────────────

    /// <summary>
    /// Prints one variable store. The three stores differ only in how long
    /// they live: the session's, the character's, and the server's.
    /// </summary>
    private void ListVariables(ExpressionVariableScope scope)
    {
        // Read the whole store first: a variable that cannot be read back
        // fails the listing as one command, with nothing half printed, as
        // the reference builds its listing before it prints any of it.
        KeyValuePair<string, ExpressionValue>[] variables = _expressions.State
            .Capture(scope)
            .OrderBy(static pair => pair.Key, StringComparer.OrdinalIgnoreCase)
            .ToArray();
        WriteUbMessage(
            scope switch
            {
                ExpressionVariableScope.Persistent => "Defined persistent variables:",
                ExpressionVariableScope.Global => "Defined global variables:",
                _ => "Defined variables:",
            },
            variables.Select(static pair =>
                $"{pair.Key} ({ReferenceArguments.FriendlyTypeName(pair.Value)}) = "
                + UbValueText(pair.Value)));
    }

    // ── /ub translateroute ──────────────────────────────────────────────

    /// <summary>
    /// <c>/ub translateroute &lt;startLandblock&gt; &lt;route&gt;
    /// &lt;endLandblock&gt; &lt;saveAs&gt; [force]</c>: copies a route onto
    /// another landblock by shifting every point by the distance between the
    /// two blocks. One landblock step is 192 meters, and a route's points are
    /// written in coordinates, which are 240 meters each.
    /// </summary>
    private void HandleTranslateRouteCommand(string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length is < 4 or > 5
            || (parts.Length == 5
                && !parts[4].Equals("force", StringComparison.OrdinalIgnoreCase)))
        {
            WriteUbBadSyntax("translateroute");
            return;
        }
        WriteUbToolDebug(UbChat.Tools.VTank, Invariant(
            $"Translating route: RouteToLoad:{parts[1]} StartLandblock:{parts[0]} EndLandblock:{parts[2]} RouteToSaveAs:{parts[3]} Force:{parts.Length == 5}"));
        if (!TryParseLandblock(parts[0], out uint start))
        {
            PostUb(UbChat.ToolError(
                UbChat.Tools.VTank,
                $"Could not parse hex value from StartLandblock: {parts[0]}"));
            return;
        }
        if (!TryParseLandblock(parts[2], out uint end))
        {
            PostUb(UbChat.ToolError(
                UbChat.Tools.VTank,
                $"Could not parse hex value from EndLandblock: {parts[2]}"));
            return;
        }

        double eastWest = LandblockDifference(start >> 24, end >> 24) / 240d;
        double northSouth =
            LandblockDifference((start << 8) >> 24, (end << 8) >> 24) / 240d;
        bool force = parts.Length == 5;
        // Both names keep their extensions: the route read is that file when
        // both forms sit side by side, and the route written is that form. A
        // bare name to write is a .nav, as a route saved by name is.
        bool translated = _routeProfiles.TryTranslate(
            parts[1],
            parts[3],
            eastWest,
            northSouth,
            force,
            _host.Automation.Spells,
            out string notice,
            out int records);
        if (!translated)
        {
            PostUb(UbChat.ToolError(UbChat.Tools.VTank, notice));
            return;
        }
        // A route that translated is reported only in the debug output.
        WriteUbToolDebug(UbChat.Tools.VTank, Invariant(
            $"Translated {records} records from {start:X8} to {end:X8} by adding offsets NS:{northSouth} EW:{eastWest}\nSaved to file: {notice}"));
    }

    /// <summary>
    /// The distance between two landblock coordinates, in meters. A landblock
    /// is eight cells of 24 meters, so one step along it is 192 meters.
    /// </summary>
    private static double LandblockDifference(uint from, uint to) =>
        ((double)to - from) * 192d;

    private static bool TryParseLandblock(string text, out uint value)
    {
        string hex = text.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
            ? text[2..]
            : text;
        return uint.TryParse(
            hex,
            NumberStyles.HexNumber,
            CultureInfo.InvariantCulture,
            out value);
    }

    /// <summary>
    /// A line whose numbers read the same on every machine. The game's text is
    /// US-formatted for everyone, and a macro parsing its own chat must not
    /// see a comma where it expects a decimal point.
    /// </summary>
    private static string Invariant(FormattableString text) =>
        text.ToString(CultureInfo.InvariantCulture);
}
