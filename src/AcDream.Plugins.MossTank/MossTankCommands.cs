using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private static readonly string[] VtankHelp =
    [
        "MossTank /vt — profiles: settings nav navaf loot meta metaaf opt testitem propertydump addnavpt refresh getdb gamedb addnavjump addnavcheckpoint",
        "MossTank /vt — actions: start stop forcebuff cancelforcebuff setmetastate fakedeath deathrestore deletemonster reverseroute reverseroutequery equipitemsfor mexec metainterval nextwp prevwp echo tapjump jump setattackbar",
        "MossTank /vt — game info: dumpspells dumpspecies dumpmats dumpskills",
        "MossTank /vt — debug: log testmonster lockdump dumptracker clearlocks clearbusy listmonstervariables dumpmetavars listmetafunctions metafunchelp fakeimp pscount testspell testpet",
    ];

    /// <summary>The macro's own command word.</summary>
    internal const string VtankVerb = "vt";

    /// <summary>
    /// The UtilityBelt command word. Its commands are its own: none of them
    /// answers on <see cref="VtankVerb"/>, and none of the macro's answers
    /// here.
    /// </summary>
    internal const string UbVerb = "ub";

    private readonly HashSet<string> _commandLogTypes =
        new(StringComparer.OrdinalIgnoreCase);

    // Channels the session asked for from outside (autostart), kept apart
    // from the profile's own: a profile load replaces those, and a request
    // made for the whole session must outlive the loads within it.
    private readonly HashSet<string> _sessionLogTypes =
        new(StringComparer.OrdinalIgnoreCase);

    private bool IsLogging(MacroLogChannel channel) =>
        _commandLogTypes.Contains(channel.ToString())
        || _sessionLogTypes.Contains(channel.ToString());
    /// <summary>
    /// How long after the charge is let go the character is given to settle
    /// before the client is asked whether a jump happened at all.
    /// </summary>
    private const double CommandJumpSettleSeconds = 0.25d;

    /// <summary>
    /// How long past the hold a charge is given to become a jump before that
    /// attempt counts as never having taken.
    /// </summary>
    private const double CommandJumpGraceSeconds = 5d;

    /// <summary>
    /// How long a jump holds the route still when Jumper.PauseNav is on. It
    /// is a ceiling, not a duration: the hold is dropped the moment the jump
    /// ends, and this is only what keeps the route from walking off if the
    /// client never answers at all.
    /// </summary>
    private const double CommandJumpNavigationHoldSeconds = 20d;

    /// <summary>The same ceiling for a turn, which is the shorter job.</summary>
    private const double CommandTurnNavigationHoldSeconds = 15d;

    /// <summary>
    /// How often a client turn is looked at: done when close enough,
    /// otherwise the client is asked again.
    /// </summary>
    private const double ClientTurnCheckSeconds = 0.1d;

    /// <summary>How close a client turn has to get to count as done.</summary>
    private const float ClientTurnToleranceDegrees = 1f;

    /// <summary>How long a client turn is given before it counts as failed.</summary>
    private const double ClientTurnGiveUpSeconds = 5d;

    /// <summary>
    /// How a command jump or turn gets the character pointing the right way.
    /// </summary>
    private enum CommandJumpTurn
    {
        /// <summary>
        /// Hold the turn keys until within a few degrees: the macro's own
        /// jump.
        /// </summary>
        HeldKeys,

        /// <summary>
        /// Ask the client to turn to the exact heading and look again every
        /// tenth of a second: the reference's face and jump. The client's
        /// turn lands on the heading exactly.
        /// </summary>
        Client,

        /// <summary>No turn at all: the reference's jump with no heading.</summary>
        None,
    }

    private bool _commandJumpActive;
    private CommandJumpTurn _commandJumpTurn;
    private bool _commandJumpTurning;
    private double _commandJumpTurnSinceCheck;
    private bool _commandJumpHeldNavigation;
    private bool _commandJumpReleased;
    private bool _commandJumpCharging;
    private bool _commandJumpFaceOnly;
    private bool _commandJumpSawAirborne;
    private int _commandJumpAttempt;
    private long _commandJumpSequence;
    private double _commandJumpElapsed;
    private double _commandJumpTurnElapsed;
    private double _commandJumpChargeSeconds;
    private float _commandJumpHeading;
    private PluginMovementIntent _commandJumpIntent;

    /// <summary>
    /// True when the jump is asked of the client by power, with its keys held
    /// as client-driven moves; false for the one set of keys that cannot be
    /// held that way, which charges on the held keys instead.
    /// </summary>
    private bool _commandJumpExact;
    private float _commandJumpPower;

    /// <summary>True while the client holds the keys an exact jump asked for.</summary>
    private bool _commandJumpMoving;
    private bool _commandPortalState;
    private int _commandPortalCount;

    /// <summary>
    /// Runs one line typed under <c>/vt</c>: the macro's own commands and
    /// MossTank's additions to them. The UtilityBelt commands are not among
    /// them -- they answer on <c>/ub</c> only, and here they are unknown, as
    /// they are to the macro itself.
    /// </summary>
    internal void ExecuteVtankCommand(PluginCommand command)
    {
        try
        {
            ExecuteVtankCommandCore(command.Arguments);
        }
        catch (Exception error)
        {
            WriteVtank($"Command failed: {error.GetBaseException().Message}");
            _host.Log.Error("MossTank /vt command failed.", error);
        }
    }

    private void ExecuteVtankCommandCore(string input)
    {
        (string typed, string arguments) = SplitHead(input);
        string verb = typed.ToLowerInvariant();
        switch (verb)
        {
            case "":
            case "help":
                HandleVtankHelpCommand(arguments);
                return;
            case "ub":
                WriteUbVersion();
                return;

            case "start":
                if (!_combat.Enabled)
                    SetMacroRunning(true);
                else
                    WriteVtank("Macro is already running.");
                return;
            case "stop":
                if (_combat.Enabled)
                    SetMacroRunning(false);
                WriteVtank("Macro stopped.");
                return;
            case "forcebuff":
                _buffRule.StartForce();
                return;
            case "cancelforcebuff":
                _buffRule.CancelForce();
                return;
            case "settings":
                HandleSettingsCommand(arguments);
                return;
            case "navaf":
                HandleRouteProfileCommand(arguments, af: true);
                return;
            case "nav":
                HandleRouteProfileCommand(arguments);
                return;
            case "loot":
                HandleLootProfileCommand(arguments);
                return;
            case "metaaf":
                HandleMetaProfileCommand(arguments, af: true);
                return;
            case "meta":
                HandleMetaProfileCommand(arguments);
                return;
            case "opt":
                HandleOptionCommand(arguments);
                return;
            case "nextwp":
                HandleNextWaypointCommand(arguments);
                return;
            case "prevwp":
                HandlePreviousWaypointCommand(arguments);
                return;
            case "metainterval":
                HandleMetaIntervalCommand(arguments);
                return;
            case "setmetastate":
                SetMetaStateFromCommand(arguments);
                return;
            case "mexec":
                ExecuteExpression(arguments);
                return;
            case "echo":
                WriteVtank(arguments);
                return;
            case "setattackbar":
                SetAttackBar(arguments);
                return;
            case "tapjump":
                StartCommandJump(
                    _host.Automation.Navigation.Snapshot.Position.HeadingDegrees,
                    shift: false,
                    milliseconds: 100,
                    null);
                return;
            case "jump":
                HandleJumpCommand(arguments, addToRoute: false);
                return;
            case "addnavjump":
                HandleJumpCommand(arguments, addToRoute: true);
                return;
            case "addnavpt":
                AddCommandRoutePoint(arguments, checkpoint: false);
                return;
            case "addnavcheckpoint":
                AddCommandRoutePoint(arguments, checkpoint: true);
                return;
            case "reverseroute":
                _navigation.ToggleReverse();
                WriteVtank($"Setting nav backwards to: {_navigation.Reversing}");
                return;
            case "reverseroutequery":
                WriteVtank($"Nav backwards is: {_navigation.Reversing}");
                return;
            case "deletemonster":
                DeleteSelectedMonster();
                return;
            case "equipitemsfor":
                EquipItemsFor(arguments);
                return;
            case "testitem":
                TestSelectedItem();
                return;
            case "propertydump":
                DumpSelectedProperties();
                return;
            case "testmonster":
                TestSelectedMonster();
                return;
            case "testspell":
                TestSpell(arguments);
                return;
            case "testpet":
                TestPet();
                return;
            case "listmonstervariables":
                WriteVtank("Monster expression variables you can use:");
                WriteVtank("true, false, name, typeid, species, maxhp, range, hasshield, metastate, setting_<OptionName>");
                return;
            case "dumpmetavars":
                DumpMetaVariables();
                return;
            case "listmetafunctions":
                ListMetaFunctions();
                return;
            case "metafunchelp":
                MetaFunctionHelp(arguments);
                return;
            case "deathrestore":
                RestoreAfterDeath();
                return;
            case "fakedeath":
                // The reference's verb calls the death handler itself rather
                // than only poking the meta engine, so the whole death
                // happens — here, that is the meta edge and the macro stop.
                _meta.TriggerFakeDeath();
                HandleDeath(_combat.Enabled);
                WriteVtank("Fake character death trigger fired.");
                return;
            case "pscount":
                WriteVtank($"Portal space toggle count: {_commandPortalCount}");
                return;
            case "refresh":
                EnsureDefaultMonsterRule();
                RefreshMonsterEditor();
                RefreshItemEditors();
                RefreshLootEditor();
                RefreshRouteEditor();
                RefreshMetaEditor();
                WriteVtank("Settings pages reloaded.");
                return;
            case "getdb":
                StartGameInfoUpdate();
                return;
            case "gamedb":
                HandleGameDbCommand(arguments);
                return;
            case "log":
                HandleLogCommand(arguments);
                return;
            case "lockdump":
                WriteVtank($"Action busy: magic={_host.Automation.Magic.IsCasting}, items={_host.Automation.Items.IsBusy}, equipment={_host.Automation.Equipment.IsBusy}");
                return;
            case "dumptracker":
                DumpObjectTracker();
                return;
            case "clearlocks":
                ClearMossTankActionLocks();
                WriteVtank("Action locks cleared.");
                return;
            case "clearbusy":
                PluginRecoveryResult recovery = _host.Automation.Recovery
                    .ClearOneBusyReference();
                WriteVtank(recovery.Accepted
                    ? $"Action busy: {recovery.PreviousCount} -> {recovery.CurrentCount}."
                    : recovery.Message);
                return;
            case "fakeimp":
                FakeImperil();
                return;
            case "dumpspells":
                DumpSpells();
                return;
            case "dumpspecies":
                DumpSpecies();
                return;
            case "dumpmats":
                DumpMaterials();
                return;
            case "dumpskills":
                DumpSkills();
                return;
            default:
                WriteVtank(UnknownVtankCommand);
                return;
        }
    }

    /// <summary>
    /// What <c>/vt</c> answers to a word it does not know -- including every
    /// UtilityBelt command, which answers on <c>/ub</c> only.
    /// </summary>
    internal const string UnknownVtankCommand = "Unknown /vt command. Use /vt help.";

    /// <summary>
    /// <c>/ub count item &lt;namepattern&gt;</c>,
    /// <c>/ub count profile &lt;lootprofile&gt;</c>,
    /// <c>/ub count player &lt;range&gt;</c> and <c>/ub count stop</c>. The
    /// first and the third answer here and now; the profile count may have to
    /// wait for appraisals and answers as they land, and <c>stop</c> cancels
    /// one that is still waiting.
    /// </summary>
    private void HandleCountCommand(string arguments)
    {
        (string mode, string subject) = SplitHead(arguments);
        switch (mode.ToLowerInvariant())
        {
            case "item":
                _inventoryCount.ReportNameCount(subject);
                return;
            case "profile":
                // The reference says the profile's name first, then only its
                // own refusal when the file cannot be had.
                if (_inventoryCount.IsRunning)
                {
                    PostUb(UbChat.ToolError(UbChat.Tools.Counter, "Counter already running."));
                    return;
                }
                WriteUb(subject.Trim());
                if (!_inventoryCount.TryStartProfile(
                    StripExtension(subject, ".utl", ".json"),
                    foreground: true))
                {
                    PostUb(UbChat.Tool(
                        UbChat.Tools.Counter,
                        "Profile does not exist: " + subject.Trim()));
                }
                return;
            case "player":
                if (!double.TryParse(
                        subject,
                        NumberStyles.Float,
                        CultureInfo.InvariantCulture,
                        out double range)
                    || !double.IsFinite(range)
                    || range <= 0d)
                {
                    PostUb(UbChat.ToolError(
                        UbChat.Tools.Counter,
                        $"bad player count range: {subject}"));
                    return;
                }
                _inventoryCount.ReportPlayerCount(range);
                return;
            case "stop":
                // A profile count holds the character while it waits, so it
                // needs a way back that is not a relog.
                PostUb(UbChat.Tool(UbChat.Tools.Counter, _inventoryCount.Cancel()));
                return;
            default:
                WriteUbBadSyntax("count");
                return;
        }
    }

    /// <summary>
    /// <c>/ub login next[r][l] &lt;name-or-index&gt;</c>, <c>/ub login clear</c>
    /// and <c>/ub login list</c>: which character the client logs in as after
    /// this one logs out. <c>r</c> counts the index on from the character the
    /// client is on rather than from the start of the list, and <c>l</c> wraps
    /// an index that runs off either end back around.
    /// </summary>
    /// <remarks>
    /// A selector that names nobody clears the pick rather than leaving an
    /// earlier one standing: a mule rotation that mistypes a name should stop
    /// where it is, not log in as whoever was chosen last.
    /// </remarks>
    private void HandleLoginCommand(string arguments)
    {
        (string head, string selector) = SplitHead(arguments);
        string verb = head.ToLowerInvariant();
        if (verb is "clear")
        {
            ClearNextLogin();
            return;
        }
        if (verb is "list")
        {
            PrintLoginRoster();
            return;
        }
        if (!verb.StartsWith("next", StringComparison.Ordinal))
        {
            WriteUbBadSyntax("login");
            return;
        }

        string flags = verb["next".Length..];
        bool relative = flags.StartsWith('r');
        if (relative)
            flags = flags[1..];
        bool looping = flags.StartsWith('l');
        if (looping)
            flags = flags[1..];
        if (flags.Length != 0)
        {
            WriteUbBadSyntax("login");
            return;
        }
        if (selector.Length == 0)
        {
            WriteUb("Specify part of name or index: /ub login next <name|index>");
            return;
        }

        ILoginAutomation login = _host.Automation.Login;
        IReadOnlyList<PluginLoginCharacter> roster = LoginRoster.Capture(login);
        if (!LoginRoster.TryResolve(
            roster,
            selector,
            _host.Automation.Character.Name,
            relative,
            looping,
            out int index,
            out LoginRosterFailure failure))
        {
            WriteUb(failure switch
            {
                LoginRosterFailure.Empty =>
                    "The account's character list has not arrived yet.",
                LoginRosterFailure.NoCurrentCharacter =>
                    "Cannot log in relative to a character that is not on the list.",
                LoginRosterFailure.OutOfRange =>
                    "Login index is out of bounds.  Use the [l]oop option to wrap "
                    + "around: /ub login nextl -100",
                LoginRosterFailure.PendingDelete =>
                    "That character is scheduled for deletion and cannot be played.",
                _ => $"No character found with name {selector}.  Clearing next login.",
            });
            if (failure != LoginRosterFailure.Empty)
                ClearNextLogin();
            return;
        }

        PluginLoginCharacter chosen = roster[index];
        if (login.SetNextLogin(chosen.ObjectId))
            WriteUb($"Logging in as {chosen.Name} next at index {index}");
        else
            WriteUbError($"Could not set the next login to {chosen.Name}.");
    }

    /// <summary>
    /// <c>/ub autovendor</c> runs the open vendor by its own profile,
    /// <c>/ub autovendor &lt;profile&gt;</c> by a named one, and
    /// <c>/ub autovendor stop</c> (or cancel, quit) calls a run off.
    /// </summary>
    private void HandleAutoVendorCommand(string arguments)
    {
        string text = arguments.Trim();
        if (text.Equals("stop", StringComparison.OrdinalIgnoreCase)
            || text.Equals("cancel", StringComparison.OrdinalIgnoreCase)
            || text.Equals("quit", StringComparison.OrdinalIgnoreCase))
        {
            _vendorTrade.StopRequested();
            return;
        }
        // The run says what it has to say itself: nothing when it starts, as
        // the reference says nothing then, and its errors when it cannot.
        _vendorTrade.TryStart(text.Length == 0 ? null : text);
    }

    /// <summary>
    /// Reads a typed verb as the give verb and its flags. <c>p</c> matches
    /// part of the item's name, <c>P</c> part of the target's, and <c>r</c>
    /// reads the item name as a regular expression -- which wins over
    /// <c>p</c> when both are typed, the way the reference's own matching
    /// order does. Any other letter means this was not a give verb at all.
    /// </summary>
    private static bool TryReadGiveFlags(
        string typed,
        out GiveNameMatch match,
        out bool partialTarget)
    {
        match = GiveNameMatch.Exact;
        partialTarget = false;
        if (!typed.StartsWith("give", StringComparison.OrdinalIgnoreCase))
            return false;

        bool partialItem = false;
        bool pattern = false;
        foreach (char flag in typed.AsSpan(4))
        {
            switch (flag)
            {
                case 'p':
                    partialItem = true;
                    break;
                case 'P':
                    partialTarget = true;
                    break;
                case 'r':
                case 'R':
                    pattern = true;
                    break;
                default:
                    partialTarget = false;
                    return false;
            }
        }
        match = pattern
            ? GiveNameMatch.Pattern
            : partialItem ? GiveNameMatch.Partial : GiveNameMatch.Exact;
        return true;
    }

    /// <summary>
    /// <c>/ub give[p{P|r}] [count] &lt;item&gt; to &lt;target&gt;</c>, and
    /// <c>/ub give stop</c> (or cancel, quit, abort) to call a run off.
    /// The flags decide how the names are matched: <c>give</c> wants the
    /// whole item name, <c>givep</c> any part of it, <c>giver</c> a regular
    /// expression, and <c>giveP</c> takes any part of the target's name.
    /// </summary>
    /// <remarks>
    /// The line is split at the FIRST " to ", because the item name is the
    /// short half: a target called "Taper Mule" would otherwise be swallowed
    /// by an item name ending in "to".
    /// </remarks>
    private void HandleGiveCommand(
        string arguments,
        GiveNameMatch match,
        bool partialTarget)
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

        int separator = text.IndexOf(" to ", StringComparison.OrdinalIgnoreCase);
        if (separator <= 0)
        {
            WriteUbBadSyntax("give");
            return;
        }
        string item = text[..separator].Trim();
        string target = text[(separator + 4)..].Trim();

        // A leading whole number is a count, but only when something is left
        // to be the item name: "/ub give 10 to Bob" gives an item called 10.
        int count = 0;
        (string head, string rest) = SplitHead(item);
        if (rest.Length > 0
            && int.TryParse(
                head,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out int parsed)
            && parsed > 0)
        {
            count = parsed;
            item = rest;
        }

        StartGive(
            () => _profileGive.TryStartByName(item, match, count, target, partialTarget),
            "give");
    }

    /// <summary>
    /// Clears the next login and says so whether or not one was set, as the
    /// reference does.
    /// </summary>
    private void ClearNextLogin()
    {
        _host.Automation.Login.ClearNextLogin();
        WriteUb("Next login cleared.");
    }

    /// <summary>
    /// The account's characters with both numbers that describe them: the
    /// alphabetical index every selector speaks in, and the slot the account's
    /// own list keeps each character in. They are rarely the same number, so
    /// printing only one of them would mislead. The columns are the
    /// reference's: its widths, its headings, the id in decimal, and the
    /// character the client is on marked by an exact name match.
    /// </summary>
    private void PrintLoginRoster()
    {
        IReadOnlyList<PluginLoginCharacter> roster =
            LoginRoster.Capture(_host.Automation.Login);
        // One message in the reference: the heading under the tag, the rows
        // below it without, all of it in one class. The column heading is
        // printed even when there is nobody to list.
        var rows = new List<string>(roster.Count + 1)
        {
            $"{"Index",-10}{"Name",-30}{"ID",-20}Filter Index",
        };
        string current = _host.Automation.Character.Name;
        for (int index = 0; index < roster.Count; index++)
        {
            PluginLoginCharacter character = roster[index];
            string name = character.Name.Equals(current, StringComparison.Ordinal)
                ? $"**{character.Name}**"
                : character.Name;
            rows.Add(string.Create(
                CultureInfo.InvariantCulture,
                $"{index,-10}{name,-30}{character.ObjectId,-20}{character.ActiveIndex}"));
        }
        WriteUbMessage($"Listing {roster.Count} logins.", rows);
    }

    private void HandleSettingsCommand(string arguments)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0
            || operation is not ("save" or "load" or "savechar" or "loadchar"))
        {
            WriteVtank("Syntax: /vt settings [save/load/savechar/loadchar] [filename]");
            return;
        }
        name = StripExtension(name, ".usd", ".settings");
        if (operation is "save" or "savechar")
        {
            if (operation == "savechar")
                _profiles.SetMineOnly(true);
            if (_profiles.Create(
                name,
                copyCurrent: true,
                _allSettings,
                _noBuffItemNames,
                _commandLogTypes,
                out string notice))
            {
                ResetProfileConsumers();
            }
            WriteVtank(notice);
            return;
        }
        if (!_profiles.Select(name))
        {
            WriteVtank($"Settings profile '{name}' was not found.");
            return;
        }
        if (LoadSelectedProfile() == MossTankProfileLoad.Failed)
        {
            WriteVtank(_profileLifecycleNotice);
            return;
        }
        WriteVtank($"Loaded settings profile {_profiles.Selected}.");
    }

    /// <summary>
    /// <c>/vt nav [save/load] name</c>, and <c>/vt navaf</c> for the plugin's
    /// own ".af" form. A name that says ".nav" or ".af" means exactly that
    /// file, even when the other one sits beside it. A bare name saves as a
    /// ".nav", and loads the ".nav" first and the ".af" second; under
    /// <c>navaf</c> it saves and loads the ".af" and nothing else.
    /// </summary>
    private void HandleRouteProfileCommand(string arguments, bool af = false)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0 || operation is not ("save" or "load"))
        {
            WriteVtank($"Syntax: /vt {(af ? "navaf" : "nav")} [save/load] [filename]");
            return;
        }
        string bareName = StripExtension(name, ".nav", ".af");
        string fileName = af ? bareName + ".af" : name;
        if (operation == "save")
        {
            _routeProfiles.Create(
                fileName,
                copyCurrent: true,
                _navigationSettings,
                out string notice);
            RefreshRouteEditor();
            WriteVtank(notice);
            return;
        }
        if (!_routeProfiles.Select(fileName, exactOnly: af))
        {
            if (af)
            {
                WriteVtank($"Navigation profile {fileName} was not found.");
                return;
            }
            if (!_routeProfiles.TryImportLegacy(
                    bareName,
                    _navigationSettings,
                    _host.Automation.Spells,
                    out string importNotice))
            {
                WriteVtank(importNotice);
                return;
            }
            _navigation.Reset();
            RefreshRouteEditor();
            WriteVtank(importNotice);
            return;
        }
        LoadRouteProfile();
        WriteVtank($"Loaded navigation profile {_routeProfiles.Selected}.");
    }

    private void HandleLootProfileCommand(string arguments)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0 || operation is not ("new" or "save" or "load"))
        {
            WriteVtank("Syntax: /vt loot [load/new] [filename]");
            return;
        }
        name = StripExtension(name, ".utl", ".json");
        if (operation is "new" or "save")
        {
            if (_lootProfiles.Create(
                name,
                copyCurrent: operation == "save",
                _inventorySettings.Loot.Rules,
                out string notice,
                _inventorySettings.Loot))
            {
                LoadLootProfile();
            }
            WriteVtank(notice);
            return;
        }
        if (_lootProfiles.Exists(name))
        {
            SelectLootProfileCore(name);
            WriteVtank(_lootEditorNotice);
            return;
        }
        if (!_lootProfiles.TryImportLegacy(
                name,
                _inventorySettings.Loot.Rules,
                _inventorySettings.Loot,
                out string importNotice))
        {
            WriteVtank(importNotice);
            return;
        }
        _loot.Reset();
        RefreshLootEditor();
        WriteVtank(importNotice);
    }

    private void HandleNextWaypointCommand(string arguments)
    {
        string value = arguments.Trim();
        int count = 1;
        if (value.Length != 0
            && (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out count)
                || count < 1))
        {
            WriteVtank("Syntax: /vt nextwp [number of waypoints, default 1]");
            return;
        }
        WriteVtank(SkipRouteWaypoints(count));
    }

    private void HandlePreviousWaypointCommand(string arguments)
    {
        string value = arguments.Trim();
        int count = 1;
        if (value.Length != 0
            && (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out count)
                || count < 1))
        {
            WriteVtank("Syntax: /vt prevwp [number of waypoints, default 1]");
            return;
        }
        WriteVtank(StepBackRouteWaypoints(count));
    }

    private void HandleMetaIntervalCommand(string arguments)
    {
        string value = arguments.Trim();
        if (value.Length != 0)
        {
            if (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out int milliseconds))
            {
                WriteVtank(
                    $"Syntax: /vt metainterval [{MetaEngine.MinimumIntervalMilliseconds}-{MetaEngine.MaximumIntervalMilliseconds} ms]");
                return;
            }
            _profiles.SetMetaIntervalMilliseconds(milliseconds);
            ApplyMetaInterval();
        }
        WriteVtank(
            $"Meta is checked every {_meta.IntervalMilliseconds} ms "
            + $"(VTank: {(int)Math.Round(MetaEngine.DecisionIntervalSeconds * 1000d)} ms).");
    }

    /// <summary>
    /// <c>/vt meta [save/load] name</c>, and <c>/vt metaaf</c> for the
    /// plugin's own ".af" form. A name that says ".met" or ".af" means
    /// exactly that file, even when the other one sits beside it. A bare name
    /// saves as a ".met", and loads the ".met" first and the ".af" second;
    /// under <c>metaaf</c> it saves and loads the ".af" and nothing else.
    /// </summary>
    private void HandleMetaProfileCommand(string arguments, bool af = false)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0 || operation is not ("save" or "load"))
        {
            WriteVtank($"Syntax: /vt {(af ? "metaaf" : "meta")} [save/load] [filename]");
            return;
        }
        name = StripExtension(name, ".json");
        string bareName = StripExtension(name, ".met", ".af");
        string fileName = af ? bareName + ".af" : name;
        if (operation == "save")
        {
            _metaProfiles.Create(
                fileName,
                copyCurrent: true,
                _metaProfile,
                out string notice);
            LoadMetaProfile();
            WriteVtank(notice);
            return;
        }
        if (!_metaProfiles.Select(fileName, exactOnly: af))
        {
            if (af)
            {
                WriteVtank($"Meta profile {fileName} was not found.");
                return;
            }
            if (!_metaProfiles.LegacyImportExists(bareName))
            {
                // The reference loads a meta that does not exist by creating
                // it, empty, and switching to it: the old meta stops, the new
                // one starts in Default, and the pass that asked for it ends.
                if (!_metaProfiles.Create(fileName, copyCurrent: false, _metaProfile, out string created))
                {
                    WriteVtank(created);
                    return;
                }
                LoadMetaProfile();
                WriteVtank($"Meta profile {_metaProfiles.Selected} did not exist; started it empty.");
                return;
            }
            if (!_metaProfiles.TryImportLegacy(
                bareName,
                out MetaProfile imported,
                out string importNotice))
            {
                WriteVtank(importNotice);
                return;
            }
            _metaProfile = imported;
            _meta.ReplaceProfile(_metaProfile);
            if (_initialized)
                ApplyPersistedOptionOverrides();
            _selectedMetaRule = 0;
            RefreshMetaEditor();
            WriteVtank(importNotice);
            return;
        }
        LoadMetaProfile();
        // A file that could not be read has already been announced with why.
        if (_metaProfiles.LastLoadError is null)
            WriteVtank($"Loaded Meta profile {_metaProfiles.Selected}.");
    }

    private void HandleOptionCommand(string arguments)
    {
        (string operation, string tail) = SplitHead(arguments);
        switch (operation.ToLowerInvariant())
        {
            case "list":
                if (tail.Length != 0)
                {
                    WriteVtank("Syntax: /vt opt list");
                    return;
                }
                WriteVtank($"Available options: ({VtankOptionCatalog.Names.Length})");
                for (int index = 0; index < VtankOptionCatalog.Names.Length; index += 4)
                    WriteVtank("   " + string.Join("   ", VtankOptionCatalog.Names.Skip(index).Take(4)));
                return;
            case "toggle":
                ToggleVtankOption(tail);
                return;
            case "get":
                if (!VtankOptionCatalog.IsKnown(tail))
                {
                    WriteVtank("Option get: Invalid option specified.");
                    return;
                }
                string canonical = VtankOptionCatalog.Canonical(tail);
                WriteVtank($"Option {canonical} = {GetMetaOption(canonical).ToDisplayString()}");
                return;
            case "set":
            case "setinall":
                (string name, string rawValue) = SplitHead(tail);
                if (!VtankOptionCatalog.IsKnown(name))
                {
                    WriteVtank("Option set: Invalid option specified.");
                    return;
                }
                canonical = VtankOptionCatalog.Canonical(name);
                VtankSettingValueType declaredType = VtankOptionCatalog.DeclaredType(canonical);
                if (rawValue.Length == 0 || !TryParseOptionValue(rawValue, declaredType, out ExpressionValue value))
                {
                    WriteVtank(
                        "Option set: bad value. " + canonical
                        + " takes a " + DeclaredClrTypeName(declaredType) + ".");
                    return;
                }
                SetMetaOption(canonical, value);
                if (operation.Equals("setinall", StringComparison.OrdinalIgnoreCase))
                {
                    int count = _profiles.SetOptionInAll(canonical, _allSettings);
                    WriteVtank($"Done saving setting {canonical} to all profiles. (Changed {count} profiles)");
                }
                else
                {
                    WriteVtank($"Set option {canonical} = {GetMetaOption(canonical).ToDisplayString()}");
                }
                return;
            default:
                WriteVtank("Syntax: /vt opt [list/get/set/setinall/toggle]");
                return;
        }
    }

    private void SetMetaStateFromCommand(string state)
    {
        if (state.Length == 0)
        {
            WriteVtank("Syntax: /vt setmetastate [somestate]");
            WriteVtank("State names are case sensitive.");
            return;
        }
        string target = _meta.States.FirstOrDefault(value =>
            value.Equals(state, StringComparison.Ordinal)) ?? MetaEngine.DefaultState;
        if (!target.Equals(state, StringComparison.Ordinal))
            WriteVtank("That state is not used by this meta; using the default instead.");
        _meta.Transition(target);
        _combatSettings.MetaState = _meta.CurrentState;
        WriteVtank($"Meta state is now {_meta.CurrentState}.");
    }

    private void ExecuteExpression(string source)
    {
        WriteVtank($"MExec evaluating expression: \"{source.Trim()}\"");
        try
        {
            WriteVtank("Result: " + _expressions.Evaluate(source).ToDisplayString());
        }
        catch (Exception error)
        {
            WriteVtank("Expression error: " + error.Message);
        }
    }

    private void SetAttackBar(string value)
    {
        if (!float.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out float power)
            || power is < 0f or > 1f)
        {
            WriteVtank("Syntax: /vt setattackbar [0 to 1]");
            return;
        }
        _combatSettings.AttackPower = power;
        SaveProfile();
        WriteVtank($"Attack bar set to {power.ToString("0.###", CultureInfo.InvariantCulture)}.");
    }

    private void HandleJumpCommand(string arguments, bool addToRoute)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length is < 3 or > 4
            || !float.TryParse(parts[0], NumberStyles.Float, CultureInfo.InvariantCulture, out float heading)
            || !bool.TryParse(parts[1], out bool shift)
            || !int.TryParse(parts[2], NumberStyles.Integer, CultureInfo.InvariantCulture, out int milliseconds)
            || milliseconds is < 0 or > 5000
            || !TryParseJumpDirection(parts.Length == 4 ? parts[3] : null, out RouteJumpDirection direction))
        {
            WriteVtank(addToRoute
                ? "Syntax: /vt addnavjump [heading] [shift: true or false] [milliseconds] [forward|backward|strafeleft|straferight]"
                : "Syntax: /vt jump [heading] [shift: true or false] [milliseconds] [forward|backward|strafeleft|straferight]");
            return;
        }
        if (addToRoute)
        {
            PluginNavigationPosition position = _host.Automation.Navigation.Snapshot.Position;
            _navigationSettings.Waypoints.Add(new RouteWaypoint
            {
                Type = RouteWaypointType.Jump,
                Position = position,
                JumpHeadingDegrees = NormalizeHeading(heading),
                JumpHoldShift = shift,
                JumpChargeMilliseconds = milliseconds,
                JumpDirection = direction,
            });
            bool saved = SaveRouteProfile();
            RefreshRouteEditor();
            WriteVtank("Added jump to the current route.");
            if (!saved)
            {
                // The route's file cannot hold what was just added; say so
                // now rather than let the next save fail unexplained.
                WriteVtank(_routeNotice);
            }
            else if (direction == RouteJumpDirection.Backward)
            {
                // The interchange route format has codes for forward and the
                // two strafes and none for backward, so a saved route reads
                // this jump back as forward. Said here because this is the one
                // moment the player can do something about it.
                WriteVtank(
                    "Note: a saved route file has no backward jump; this one "
                    + "will read back as forward.");
            }
            return;
        }
        StartCommandJump(heading, shift, milliseconds, direction);
    }

    /// <summary>
    /// <c>/ub face &lt;heading&gt;</c>: the jump command's turn on its own.
    /// The client turns to the exact heading; the command is done once the
    /// character is within a degree of it, and failed after five seconds.
    /// </summary>
    private void HandleFaceCommand(string arguments)
    {
        if (!float.TryParse(
                arguments.Trim(),
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out float heading)
            || !float.IsFinite(heading))
        {
            WriteUbBadSyntax("face");
            return;
        }
        StartCommandJump(
            heading,
            shift: false,
            milliseconds: 0,
            RouteJumpDirection.Forward,
            faceOnly: true,
            turn: CommandJumpTurn.Client);
    }

    /// <summary>
    /// Begins a command jump: turn to the heading, then ask the client for a
    /// jump of exactly the power the charge time comes to, a full charge per
    /// second, while it holds whichever movement keys the jump leans on.
    /// </summary>
    /// <param name="omitDirection">
    /// True for a jump that presses no movement key at all -- straight up on
    /// the spot, which is what a bare jump verb asks for.
    /// </param>
    /// <param name="turn">
    /// How the character is pointed at the heading first. The reference's
    /// commands also say nothing in chat while they work, and give up in the
    /// reference's words.
    /// </param>
    /// <param name="keys">
    /// The movement keys to hold, when the caller names them outright rather
    /// than one direction; the shift and the jump are added to them.
    /// </param>
    private void StartCommandJump(
        float heading,
        bool shift,
        int milliseconds,
        RouteJumpDirection? direction,
        bool faceOnly = false,
        bool omitDirection = false,
        CommandJumpTurn turn = CommandJumpTurn.HeldKeys,
        PluginMovementIntent? keys = null)
    {
        PluginNavigationSnapshot snapshot = _host.Automation.Navigation.Snapshot;
        if (!snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            string refusal = faceOnly
                ? "Turning unavailable outside the world."
                : "Jump unavailable outside the world.";
            // The macro's own jump answers in its own words; the UtilityBelt
            // commands answer as the jumper tool's error.
            if (turn == CommandJumpTurn.HeldKeys)
                WriteVtank(refusal);
            else
                PostUb(UbChat.ToolError(UbChat.Tools.Jumper, refusal));
            return;
        }
        RouteJumpDirection resolved = direction ?? RouteJumpDirection.Forward;
        _commandJumpHeading = NormalizeHeading(heading);
        // A caller that names its keys outright (the flag letters, which
        // combine) holds exactly those; otherwise the one direction. Shift
        // is the game's walk key: held, the keys walk; otherwise they run.
        _commandJumpIntent = keys is { } held
            ? held with { Run = !shift, Jump = true }
            : new PluginMovementIntent(
                Forward: !omitDirection && resolved == RouteJumpDirection.Forward,
                Backward: !omitDirection && resolved == RouteJumpDirection.Backward,
                StrafeLeft: !omitDirection && resolved == RouteJumpDirection.StrafeLeft,
                StrafeRight: !omitDirection && resolved == RouteJumpDirection.StrafeRight,
                Run: !shift,
                Jump: true);
        // Forward with backward, or left with right, is two keys on one
        // channel, which a client-driven move cannot hold; that jump charges
        // on the held keys instead and leaves with what the frames add up to.
        _commandJumpExact =
            !(_commandJumpIntent.Forward && _commandJumpIntent.Backward)
            && !(_commandJumpIntent.StrafeLeft && _commandJumpIntent.StrafeRight);
        _commandJumpPower = JumpPower.FromMilliseconds(milliseconds);
        _commandJumpMoving = false;
        _commandJumpChargeSeconds = _commandJumpExact
            ? JumpPower.ChargeSeconds(milliseconds)
            : Math.Clamp(milliseconds / 1000d, 0.05d, 5d);
        _commandJumpElapsed = 0d;
        _commandJumpTurnElapsed = 0d;
        _commandJumpReleased = false;
        _commandJumpCharging = false;
        _commandJumpFaceOnly = faceOnly;
        _commandJumpSawAirborne = false;
        _commandJumpAttempt = 0;
        _commandJumpSequence = 0L;
        _commandJumpTurn = turn;
        _commandJumpTurning = turn == CommandJumpTurn.Client;
        // The first look comes on the very next tick.
        _commandJumpTurnSinceCheck = ClientTurnCheckSeconds;
        _commandJumpActive = true;
        HoldNavigationForJump(faceOnly
            ? CommandTurnNavigationHoldSeconds
            : CommandJumpNavigationHoldSeconds);
        if (turn == CommandJumpTurn.Client)
        {
            // Whatever it answers, the checks that follow ask again, and the
            // time limit ends a turn that never comes.
            _host.Automation.Navigation.FaceHeading(_commandJumpHeading);
            return;
        }
        if (turn == CommandJumpTurn.HeldKeys)
        {
            WriteVtank(faceOnly
                ? $"Turning to heading {_commandJumpHeading:0.#}."
                : $"Turning to heading {_commandJumpHeading:0.#} for jump.");
        }
    }

    /// <summary>
    /// One tick of a client turn. Every tenth of a second it looks at the
    /// heading: within a degree ends the turn, anything else asks the client
    /// to turn again. Five seconds without getting there fails the command.
    /// </summary>
    private void TickClientTurn(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot,
        double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        _commandJumpTurnElapsed += elapsed;
        _commandJumpTurnSinceCheck += elapsed;
        if (_commandJumpTurnSinceCheck >= ClientTurnCheckSeconds)
        {
            _commandJumpTurnSinceCheck = 0d;
            float delta = NavigationController.SignedHeadingDelta(
                snapshot.Position.HeadingDegrees,
                _commandJumpHeading);
            if (Math.Abs(delta) < ClientTurnToleranceDegrees)
                _commandJumpTurning = false;
            else
                navigation.FaceHeading(_commandJumpHeading);
        }
        if (_commandJumpTurning && _commandJumpTurnElapsed >= ClientTurnGiveUpSeconds)
        {
            _commandJumpTurning = false;
            FailCommandJump("Turning failed");
        }
    }

    /// <summary>
    /// Holds the route still for the length of a jump or turn, when
    /// Jumper.PauseNav asks for it. The macro's walking and a charge fight
    /// over the same movement keys, so one of them has to stand down.
    /// </summary>
    private void HoldNavigationForJump(double seconds)
    {
        if (!JumperPauseNav)
            return;
        _commandJumpHeldNavigation = true;
        _actionLocks.Arm(ActionLockKind.Navigation, seconds);
        _actionLocks.Arm(ActionLockKind.CorpseOpenAttempt, seconds);
    }

    /// <summary>
    /// Lets the route go again at the end of a jump or turn, whichever way it
    /// ended. Only a hold this command took is dropped.
    /// </summary>
    private void ReleaseNavigationAfterJump()
    {
        if (!_commandJumpHeldNavigation)
            return;
        _commandJumpHeldNavigation = false;
        _actionLocks.Release(ActionLockKind.Navigation);
        _actionLocks.Release(ActionLockKind.CorpseOpenAttempt);
    }

    /// <summary>Hold the route still while a jump or turn is in the air.</summary>
    private bool JumperPauseNav =>
        _ubCatalog.Require("Jumper.PauseNav").Get().Boolean;

    /// <summary>Think to yourself when a jump or turn lands.</summary>
    private bool JumperThinkComplete =>
        _ubCatalog.Require("Jumper.ThinkComplete").Get().Boolean;

    /// <summary>Think to yourself when a jump or turn does not happen.</summary>
    private bool JumperThinkFail =>
        _ubCatalog.Require("Jumper.ThinkFail").Get().Boolean;

    /// <summary>
    /// Ends a command jump or turn that did not come off: the route is let
    /// go, and the reason goes out as a think when Jumper.ThinkFail asks for
    /// it and as a written line otherwise.
    /// </summary>
    private void FailCommandJump(string reason)
    {
        _commandJumpActive = false;
        ReleaseNavigationAfterJump();
        if (JumperThinkFail)
            Think(reason);
        else if (_commandJumpTurn == CommandJumpTurn.HeldKeys)
            WriteVtank(reason);
        else
            WriteUb(reason);
    }

    private bool TickCommandJump(double elapsedSeconds)
    {
        if (!_commandJumpActive)
            return false;

        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot snapshot = navigation.Snapshot;
        if (!snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            ReleaseCommandJumpKeys(navigation);
            FailCommandJump(_commandJumpFaceOnly
                ? "Turn canceled because the character left the world."
                : "Jump canceled because the character left the world.");
            return false;
        }

        if (!_commandJumpCharging)
        {
            if (_commandJumpTurn == CommandJumpTurn.HeldKeys)
            {
                _commandJumpTurnElapsed += Math.Max(0d, elapsedSeconds);
                float delta = NavigationController.SignedHeadingDelta(
                    snapshot.Position.HeadingDegrees,
                    _commandJumpHeading);
                if (Math.Abs(delta) > 4f)
                {
                    if (_commandJumpTurnElapsed > 10d
                        || navigation.SetMovementIntent(new PluginMovementIntent(
                            TurnLeft: delta < 0f,
                            TurnRight: delta > 0f,
                            Run: _commandJumpIntent.Run))
                            != PluginNavigationCommandStatus.Accepted)
                    {
                        navigation.ClearMovementIntent();
                        FailCommandJump(_commandJumpFaceOnly
                            ? "Could not turn to the requested heading."
                            : "Jump command could not align to the requested heading.");
                        return false;
                    }
                    return true;
                }

                // Facing a heading is this loop and nothing after it.
                if (_commandJumpFaceOnly)
                {
                    navigation.ClearMovementIntent();
                    _commandJumpActive = false;
                    ReleaseNavigationAfterJump();
                    WriteVtank($"Facing heading {_commandJumpHeading:0.#}.");
                    if (JumperThinkComplete)
                        Think("Turning Success");
                    return false;
                }
            }
            else
            {
                if (_commandJumpTurning)
                {
                    TickClientTurn(navigation, snapshot, elapsedSeconds);
                    if (_commandJumpTurning || !_commandJumpActive)
                        return _commandJumpActive;
                }

                // Facing a heading is the turn and nothing after it. The
                // client may still be finishing the last fraction of a
                // degree, so its turn is left to land rather than stopped.
                if (_commandJumpFaceOnly)
                {
                    _commandJumpActive = false;
                    ReleaseNavigationAfterJump();
                    if (JumperThinkComplete)
                        Think("Turning Success");
                    return false;
                }
            }

            // The client counts the jumps it has begun. Whatever that count
            // stands at before this charge is what the attempt has to move
            // past.
            _commandJumpSequence = navigation.MoveReport.JumpSequence;
            _commandJumpCharging = _commandJumpExact
                ? BeginExactCommandJump(navigation)
                : navigation.SetMovementIntent(_commandJumpIntent)
                    == PluginNavigationCommandStatus.Accepted;
            if (!_commandJumpCharging)
            {
                ReleaseCommandJumpKeys(navigation);
                FailCommandJump("Jump command was refused by the host.");
                return false;
            }
            _commandJumpElapsed = 0d;
            _commandJumpReleased = false;
            _commandJumpSawAirborne = false;
            _commandJumpAttempt++;
            if (_commandJumpTurn == CommandJumpTurn.HeldKeys)
            {
                WriteVtank(_commandJumpAttempt == 1
                    ? $"Jump charging at heading {_commandJumpHeading:0.#}."
                    : $"Jump charging at heading {_commandJumpHeading:0.#} "
                        + $"(attempt {_commandJumpAttempt}).");
            }
        }

        _commandJumpElapsed += Math.Max(0d, elapsedSeconds);
        if (!_commandJumpReleased && _commandJumpElapsed >= _commandJumpChargeSeconds)
        {
            _commandJumpReleased = true;
            // An exact jump is let go by the client itself, at its power.
            if (!_commandJumpExact)
                navigation.SetMovementIntent(_commandJumpIntent with { Jump = false });
        }
        if (_commandJumpElapsed
            < _commandJumpChargeSeconds + CommandJumpSettleSeconds)
        {
            return true;
        }

        // Two ways the client says a jump happened, and it publishes whichever
        // suits the road the jump took: the character left the ground, or the
        // count of jumps begun moved on. Either ends the attempt.
        _commandJumpSawAirborne |= snapshot.IsAirborne;
        if (_commandJumpSawAirborne
            || navigation.MoveReport.JumpSequence != _commandJumpSequence)
        {
            ReleaseCommandJumpKeys(navigation);
            _commandJumpActive = false;
            _commandJumpCharging = false;
            ReleaseNavigationAfterJump();
            if (JumperThinkComplete)
                Think("Jumper Success");
            return false;
        }
        if (_commandJumpElapsed
            < _commandJumpChargeSeconds + CommandJumpGraceSeconds)
        {
            return true;
        }

        // The hold and the grace both went by with no jump reported, so the
        // charge never took. Charge again from the alignment, up to the
        // attempt ceiling, and then let the character go.
        if (_commandJumpAttempt >= Math.Max(1, _navigationSettings.JumpAttempts))
        {
            ReleaseCommandJumpKeys(navigation);
            _commandJumpCharging = false;
            // The reference's commands give up in the reference's words,
            // which is the line a macro waiting on the jump listens for.
            FailCommandJump(_commandJumpTurn == CommandJumpTurn.HeldKeys
                ? $"Jump gave up after {_commandJumpAttempt} attempt(s) with no "
                    + "jump reported."
                : "You have failed to jump too many times.");
            return false;
        }
        // The keys an exact charge held are let go before the next alignment,
        // which a held turn could not make under them.
        if (_commandJumpMoving)
            ReleaseCommandJumpKeys(navigation);
        _commandJumpCharging = false;
        _commandJumpTurnElapsed = 0d;
        // The hold was taken against one charge; the next one takes its own.
        HoldNavigationForJump(CommandJumpNavigationHoldSeconds);
        return true;
    }

    /// <summary>
    /// Asks the client for the jump at its exact power, holding the jump's
    /// movement keys as client-driven moves at the jump's pace for the charge.
    /// Any held keys the turn left down are let go first. False, with nothing
    /// left held, when the client refuses any part of it.
    /// </summary>
    private bool BeginExactCommandJump(INavigationAutomation navigation)
    {
        if (_commandJumpTurn == CommandJumpTurn.HeldKeys)
            navigation.ClearMovementIntent();
        PluginMovePace pace = _commandJumpIntent.Run ? PluginMovePace.Run : PluginMovePace.Walk;
        _commandJumpMoving = true;
        bool accepted = true;
        if (_commandJumpIntent.Forward)
            accepted &= navigation.Move(PluginMoveDirection.Forward, pace, 0f) == PluginNavigationCommandStatus.Accepted;
        if (_commandJumpIntent.Backward)
            accepted &= navigation.Move(PluginMoveDirection.Backward, pace, 0f) == PluginNavigationCommandStatus.Accepted;
        if (_commandJumpIntent.StrafeLeft)
            accepted &= navigation.Move(PluginMoveDirection.StrafeLeft, pace, 0f) == PluginNavigationCommandStatus.Accepted;
        if (_commandJumpIntent.StrafeRight)
            accepted &= navigation.Move(PluginMoveDirection.StrafeRight, pace, 0f) == PluginNavigationCommandStatus.Accepted;
        return accepted
            && navigation.Jump(_commandJumpPower) == PluginNavigationCommandStatus.Accepted;
    }

    /// <summary>
    /// Lets go of the keys a command jump holds: the channels an exact
    /// jump's moves are on, and nothing else the client is moving; or the
    /// held keys, for a jump or turn that charges on them.
    /// </summary>
    private void ReleaseCommandJumpKeys(INavigationAutomation navigation)
    {
        if (!_commandJumpMoving)
        {
            navigation.ClearMovementIntent();
            return;
        }
        _commandJumpMoving = false;
        if (_commandJumpIntent.Forward || _commandJumpIntent.Backward)
            navigation.StopMoving(PluginMoveChannel.Travel);
        if (_commandJumpIntent.StrafeLeft || _commandJumpIntent.StrafeRight)
            navigation.StopMoving(PluginMoveChannel.Strafe);
    }

    private void AddCommandRoutePoint(string coordinates, bool checkpoint)
    {
        PluginNavigationPosition position;
        if (coordinates.Length == 0)
        {
            position = _host.Automation.Navigation.Snapshot.Position;
        }
        else if (!TryParseCoordinates(coordinates, out position))
        {
            WriteVtank(checkpoint
                ? "Syntax: /vt addnavcheckpoint [coords], or /vt addnavcheckpoint alone"
                : "Syntax: /vt addnavpt [coords], or /vt addnavpt alone");
            return;
        }
        _navigationSettings.Waypoints.Add(new RouteWaypoint
        {
            Type = checkpoint ? RouteWaypointType.Checkpoint : RouteWaypointType.Point,
            Position = position,
        });
        SaveRouteProfile();
        RefreshRouteEditor();
        WriteVtank(checkpoint ? "Added navigation checkpoint." : "Added navigation point.");
    }

    private void DeleteSelectedMonster()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        PluginCombatTarget target = _host.Automation.Combat
            .CaptureHostileTargets(float.MaxValue)
            .FirstOrDefault(value => value.ObjectId == selected);
        if (target.ObjectId == 0u)
        {
            WriteVtank("Pick a monster first, then /vt deletemonster");
            return;
        }
        PluginCombatCommandResult result =
            _host.Automation.Combat.DismissGhostTarget(selected);
        WriteVtank(result.Accepted
            ? $"Forcing the client to delete {target.Name} ({target.ObjectId})!!"
            : $"Unable to delete {target.Name}: {result.Status}");
    }

    private void EquipItemsFor(string monsterName)
    {
        if (monsterName.Length == 0)
        {
            WriteVtank("Syntax: /vt equipitemsfor [monster name]");
            WriteVtank("NOTE: each use of this command invokes one equipment step; multiple calls may be required.");
            return;
        }
        bool ready = _combat.EquipOneStepForMonster(monsterName);
        WriteVtank($"Changing items for monster \"{monsterName}\", ready: {ready}");
    }

    private void TestSelectedItem()
    {
        if (!TryGetSelectedInventoryItem(out PluginInventoryItem item))
        {
            WriteVtank("TestItem: pick an item first.");
            return;
        }
        if (!_host.Automation.Items.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties))
        {
            _host.Automation.Objects.Identify(item.ObjectId);
            WriteVtank("TestItem: Waiting for appraisal data.");
            return;
        }
        LootDecision? decision = LootRuleEngine.Decide(
            item,
            properties,
            _inventorySettings.Loot.Rules,
            _host.Automation.Items.CaptureOwnedItems(),
            host: _host);
        WriteVtank(decision is { } match
            ? $"TestItem: {item.Name} => {match.Action} ({match.RuleName}, priority {match.Priority})."
            : $"TestItem: {item.Name} => NoLoot.");
    }

    /// <summary>
    /// <c>/ub propertydump</c>: the reference's errors for a missing
    /// selection, then the dump as one message under the tag -- its heading
    /// tagged, the rows below it not.
    /// </summary>
    private void DumpSelectedPropertiesForUb()
    {
        if (!TryGetSelectedObject("propertydump", out PluginWorldObject item))
            return;
        WriteUb($"Property Dump for {item.Name}");
        WriteVtank($"Object 0x{item.ObjectId:X8}: {item.Name}, class={(int)item.ObjectClass}, WCID={item.WeenieClassId}");
        if (!_host.Automation.Objects.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties))
            return;
        DumpPropertyTable("Int", properties.Ints);
        DumpPropertyTable("Int64", properties.Int64s);
        DumpPropertyTable("Bool", properties.Bools);
        DumpPropertyTable("Float", properties.Floats);
        DumpPropertyTable("String", properties.Strings);
        DumpPropertyTable("DataId", properties.DataIds);
        DumpPropertyTable("InstanceId", properties.InstanceIds);
    }

    private void DumpSelectedProperties()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u
            || !_host.Automation.Objects.TryGet(selected, out PluginWorldObject item)
            || !_host.Automation.Objects.TryCaptureProperties(selected, out PluginItemProperties properties))
        {
            WriteVtank("Propertydump: Either no object selected or current selection object is invalid or not appraised.");
            return;
        }
        WriteVtank($"Object 0x{item.ObjectId:X8}: {item.Name}, class={(int)item.ObjectClass}, WCID={item.WeenieClassId}");
        DumpPropertyTable("Int", properties.Ints);
        DumpPropertyTable("Int64", properties.Int64s);
        DumpPropertyTable("Bool", properties.Bools);
        DumpPropertyTable("Float", properties.Floats);
        DumpPropertyTable("String", properties.Strings);
        DumpPropertyTable("DataId", properties.DataIds);
        DumpPropertyTable("InstanceId", properties.InstanceIds);
    }

    private void TestSelectedMonster()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        PluginCombatTarget target = _host.Automation.Combat
            .CaptureHostileTargets(float.MaxValue)
            .FirstOrDefault(value => value.ObjectId == selected);
        if (target.ObjectId == 0u)
        {
            WriteVtank("TestMonster: pick a monster first.");
            return;
        }
        ResolvedMonsterRule resolved = _combatSettings.ResolveRule(target);
        WriteVtank($"TestMonster: evaluating monster rules for monster {target.Name}, type {target.WeenieClassId}");
        WriteVtank($"Matched '{resolved.Rule.Expression}', priority {resolved.Priority}, damage {resolved.Actions.DamageType}, attack={resolved.Actions.UsesPrimaryAttack}.");
        if (resolved.EvaluationError is { Length: > 0 } error)
            WriteVtank("Expression warning: " + error);
    }

    private void TestSpell(string argument)
    {
        if (!uint.TryParse(argument, NumberStyles.Integer, CultureInfo.InvariantCulture, out uint spellId))
        {
            WriteVtank("Syntax: /vt testspell [spellid]");
            return;
        }
        if (!_host.Automation.Spells.TryGet(spellId, out PluginSpellInfo spell))
        {
            WriteVtank("Invalid spellid.");
            return;
        }
        WriteVtank("---------------------------------");
        WriteVtank($"Testing ability to cast spell {spell.Name}, family {spell.Family}, quality {spell.Quality}, diff {spell.Difficulty}");
        WriteVtank($"Known: {_host.Automation.Spells.IsKnown(spellId)}, cast gate: {_host.Automation.Magic.EvaluateGate(spellId)}");
        WriteVtank("---------------------------------");
    }

    /// <summary>
    /// The reference's pet test: whether there is room ahead for a pet to
    /// appear, the summon rule's last question, and how long asking took. A
    /// client that cannot tell answers True, as the rule counts it. The
    /// essence and monster the rule would choose follow on their own line.
    /// </summary>
    private void TestPet()
    {
        IAutomationSurface automation = _host.Automation;
        long started = System.Diagnostics.Stopwatch.GetTimestamp();
        bool room = automation.Navigation.CheckRoomAhead(SummonPetRule.PetRoomAheadMeters).Status
            != PluginRoomAheadStatus.Blocked;
        double milliseconds =
            System.Diagnostics.Stopwatch.GetElapsedTime(started).TotalMilliseconds;
        WriteVtank(string.Create(
            CultureInfo.InvariantCulture,
            $"Pet can spawn: {room}, test time: {milliseconds}ms"));
        PetSummonChoice choice = PetAutomation.SelectPet(
            automation.Items,
            automation.Items.CaptureOwnedItems(),
            automation.Combat.CaptureHostileTargets(PetAutomation.PetRange(_combatSettings)),
            automation.Character,
            _combatSettings,
            _combat.AttackElementFor,
            name => _combat.GameInfo.DamagePreferences(name));
        WriteVtank(choice.IsNone
            ? "Pet choice: none"
            : $"Pet choice: {choice.Device.Name}, for: {choice.Target.Name}");
    }

    private void DumpMetaVariables()
    {
        WriteVtank("Assigned meta variables:");
        foreach (ExpressionVariableScope scope in Enum.GetValues<ExpressionVariableScope>())
        {
            foreach ((string name, ExpressionValue value) in _expressions.State.Capture(scope))
                WriteVtank($"{scope}.{name} = {value.ToDisplayString()}");
        }
    }

    private void ListMetaFunctions()
    {
        WriteVtank("Meta functions built in to MossTank:");
        WriteChunks(_expressions.Functions
            .OrderBy(static function => function.Name, StringComparer.OrdinalIgnoreCase)
            .Select(static function => function.Name));
    }

    private void MetaFunctionHelp(string name)
    {
        ExpressionFunction? function = _expressions.Functions.FirstOrDefault(value =>
            value.Name.Equals(name, StringComparison.OrdinalIgnoreCase));
        if (function is null)
        {
            WriteVtank($"Function not defined \"{name}\"");
            return;
        }
        WriteVtank("-------------------------------");
        WriteVtank("Function: " + function.Name);
        WriteVtank("Signature: " + function.Signature);
        WriteVtank("Description: " + function.Description);
        string count = function.MinimumArguments == function.MaximumArguments
            ? function.MinimumArguments.ToString(CultureInfo.InvariantCulture)
            : $"{function.MinimumArguments}..{function.MaximumArguments}";
        WriteVtank("Parameter count: " + count);
        WriteVtank("-------------------------------");
    }

    private void HandleLogCommand(string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0)
        {
            var logging = new HashSet<string>(_commandLogTypes, StringComparer.OrdinalIgnoreCase);
            logging.UnionWith(_sessionLogTypes);
            WriteVtank(logging.Count == 0
                ? "Not currently logging."
                : "Log state:  " + string.Join(' ', logging.Order(StringComparer.OrdinalIgnoreCase)));
            WriteVtank("Log types: ActiveRule SalvageList SpellCast RuleInfo Timers CastInfo DebuffChoice Loot CharProps Misc BusyState");
            return;
        }
        if (parts.Length == 2)
            parts[1] = parts[1].ToLowerInvariant();
        if (parts.Length != 2 || parts[1] is not ("on" or "off"))
        {
            WriteVtank("Syntax: /vt log [type] [on/off]");
            return;
        }
        string type = CanonicalLogChannelName(parts[0]);
        bool changed = parts[1] == "on"
            ? _commandLogTypes.Add(type)
            : _commandLogTypes.Remove(type) | _sessionLogTypes.Remove(type);
        WriteVtank((parts[1] == "on" ? "Set " : "Reset ") + type);
        if (changed)
            SaveProfile();
    }

    /// <summary>
    /// The channel name as the emitter spells it, so a channel asked for in
    /// any casing is the channel that then logs. A name that is not one of
    /// ours is kept as typed — the oracle's list has two channels this port
    /// has no emitter for, and remembering them costs nothing.
    /// </summary>
    private static string CanonicalLogChannelName(string requested)
    {
        foreach (MacroLogChannel channel in Enum.GetValues<MacroLogChannel>())
        {
            string name = channel.ToString();
            if (name.Equals(requested, StringComparison.OrdinalIgnoreCase))
                return name;
        }
        return requested;
    }

    /// <summary>
    /// Turn on the named log channels before the first scheduler pass runs.
    /// A session with no window in front of it cannot type <c>/vt log</c>
    /// in time: several of the plugin's most useful lines — the buff
    /// planner's refusal among them — are emitted once per run, so a channel
    /// switched on after the macro starts has already missed them.
    /// </summary>
    internal void ApplyLogChannels(string requested)
    {
        foreach (string name in requested.Split(
            [',', ' ', ';'],
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            if (name.Equals("All", StringComparison.OrdinalIgnoreCase))
            {
                foreach (MacroLogChannel channel in Enum.GetValues<MacroLogChannel>())
                    _sessionLogTypes.Add(channel.ToString());
                continue;
            }
            _sessionLogTypes.Add(CanonicalLogChannelName(name));
        }
    }

    private void EmitMacroLog(MacroLogChannel channel, string message) =>
        EmitMacroLog(channel, message, chat: true);

    private void EmitMacroLog(
        MacroLogChannel channel, string message, bool chat)
    {
        if (!IsLogging(channel))
            return;
        if (chat)
            _host.Automation.Chat.PostSystemMessage("[MossTank] " + message);
        _host.Log.Info("[vt log " + channel + "] " + message);
    }

    private void DumpObjectTracker()
    {
        IReadOnlyList<PluginWorldObject> objects = _host.Automation.Objects.CaptureObjects();
        WriteVtank($"Object tracker count: {objects.Count}");
        foreach (PluginWorldObject item in objects.Take(100))
            WriteVtank($"0x{item.ObjectId:X8} {item.ObjectClass} {item.Name}");
        if (objects.Count > 100)
            WriteVtank($"... {objects.Count - 100} more objects omitted from chat.");
    }

    private void DumpSpells()
    {
        PluginSpellInfo[] spells = _host.Automation.Spells.KnownCombatSpells
            .Concat(_host.Automation.Spells.KnownSelfBuffs)
            .GroupBy(static spell => spell.SpellId)
            .Select(static group => group.First())
            .OrderBy(static spell => spell.SpellId)
            .ToArray();
        WriteVtank($"Known spell table ({spells.Length}):");
        foreach (PluginSpellInfo spell in spells)
            WriteVtank($"{spell.SpellId}\t{spell.Name}\t{spell.Family}\t{spell.Difficulty}");
    }

    private void DumpSpecies()
    {
        var species = _host.Automation.Combat.CaptureHostileTargets(float.MaxValue)
            .Where(static target => target.SpeciesId != 0)
            .GroupBy(static target => target.SpeciesId)
            .Select(static group => (Id: group.Key, Name: group.First().SpeciesName))
            .OrderBy(static value => value.Id)
            .ToArray();
        WriteVtank($"Currently observed species ({species.Length}):");
        foreach (var entry in species)
            WriteVtank($"{entry.Id}\t{entry.Name}");
    }

    private void DumpMaterials()
    {
        var materials = _host.Automation.Items.CaptureOwnedItems()
            .Where(static item => item.MaterialType != 0u)
            .GroupBy(static item => item.MaterialType)
            .OrderBy(static group => group.Key);
        WriteVtank("Materials present in owned inventory:");
        foreach (IGrouping<uint, PluginInventoryItem> group in materials)
            WriteVtank($"{group.Key}\t{group.First().Name}");
    }

    /// <summary>
    /// The character's skills. On <c>/ub</c> every line is a message of its
    /// own under the tag, as the reference prints them.
    /// </summary>
    private void DumpSkills(bool ub = false)
    {
        Action<string> write = ub ? WriteUb : WriteVtank;
        write($"Character skills ({_host.Automation.Character.Skills.Count}):");
        foreach (PluginSkillInfo skill in _host.Automation.Character.Skills.OrderBy(static value => value.SkillId))
            write($"{skill.SkillId}\t{skill.Name}\t{skill.Base}\t{skill.Current}\t{skill.Training}");
    }

    private void ObserveCommandPortalState()
    {
        bool current = _host.Automation.Navigation.Snapshot.IsPortalSpace;
        if (current != _commandPortalState)
        {
            _commandPortalState = current;
            _commandPortalCount++;
        }
    }

    private void ResetCommandSession()
    {
        if (_commandJumpActive || _commandJumpCharging)
            ReleaseCommandJumpKeys(_host.Automation.Navigation);
        _commandJumpActive = false;
        _commandJumpExact = false;
        _commandJumpPower = 0f;
        _commandJumpMoving = false;
        _commandJumpReleased = false;
        _commandJumpCharging = false;
        _commandJumpElapsed = 0d;
        _commandJumpTurnElapsed = 0d;
        _commandJumpChargeSeconds = 0d;
        _commandJumpHeading = 0f;
        _commandJumpIntent = default;
        _commandJumpFaceOnly = false;
        _commandJumpSawAirborne = false;
        _commandJumpAttempt = 0;
        _commandJumpSequence = 0L;
        _commandJumpTurn = CommandJumpTurn.HeldKeys;
        _commandJumpTurning = false;
        _commandJumpTurnSinceCheck = 0d;
        ReleaseNavigationAfterJump();
        _commandPortalState = _host.Automation.Navigation.Snapshot.IsPortalSpace;
        _commandPortalCount = 0;
        // A line scheduled before the session ended must not arrive after it:
        // the character it was typed for is no longer the one standing there.
        _delayedCommands.Clear();
        _prepClick.Reset();
        _expressions.HeldMotions.Reset();
    }

    private void WriteVtank(string text) =>
        _host.Automation.Chat.PostSystemMessage(text);

    /// <summary>A finished UtilityBelt line, in its kind's text class.</summary>
    private void PostUb(string line) => UbChat.Post(_host.Automation.Chat, line);

    /// <summary>
    /// A line from a tool that writes both kinds: one under the UtilityBelt
    /// tag goes out in its kind's text class, anything else as the macro's
    /// own plain line.
    /// </summary>
    private void WriteTranscriptLine(string line)
    {
        if (line.StartsWith(UbChat.Tag, StringComparison.Ordinal))
            PostUb(line);
        else
            WriteVtank(line);
    }

    /// <summary>Whether the UtilityBelt side prints its debug lines.</summary>
    private bool UbDebug => _ubCatalog.Require(UbChat.DebugSetting).Get().Boolean;

    /// <summary>
    /// A kind of UB line's display, as its two settings say; the shipped one
    /// until the settings are in place.
    /// </summary>
    private UbChat.Display UbMessageDisplay(UbChat.Kind kind)
    {
        if (_ubCatalog is null)
            return UbChat.DefaultDisplay(kind);
        string display = UbChat.DisplaySetting(kind);
        return new UbChat.Display(
            _ubCatalog.Require(display + ".Enabled").Get().Boolean,
            _ubCatalog.Require(display + ".Color").Get().AsInt32());
    }

    /// <summary>A UtilityBelt debug line, printed only while debug is on.</summary>
    private void WriteUbDebug(string text) =>
        UbChat.PostDebug(_host.Automation.Chat, UbDebug, text);

    /// <summary>A tool's UtilityBelt debug line, printed only while debug is on.</summary>
    private void WriteUbToolDebug(string tool, string text) =>
        UbChat.PostToolDebug(_host.Automation.Chat, UbDebug, tool, text);

    /// <summary>A plain UtilityBelt line: "[UB] " and the text.</summary>
    private void WriteUb(string text) => PostUb(UbChat.Line(text));

    /// <summary>A UtilityBelt error: "[UB] Error: " and the text.</summary>
    private void WriteUbError(string text) => PostUb(UbChat.Error(text));

    /// <summary>
    /// A plain UtilityBelt message of several lines: the tag on the heading
    /// only, every line in the generic class and behind its switch.
    /// </summary>
    private void WriteUbMessage(string heading, IEnumerable<string> continuation) =>
        UbChat.PostMessage(_host.Automation.Chat, UbChat.Kind.Generic, heading, continuation);

    /// <summary>
    /// A UtilityBelt error of several lines: the error lead on the heading
    /// only, every line in the error class and behind its switch.
    /// </summary>
    private void WriteUbErrorMessage(string heading, IEnumerable<string> continuation) =>
        UbChat.PostMessage(_host.Automation.Chat, UbChat.Kind.Error, heading, continuation);

    /// <summary>
    /// What <c>/ub</c> answers when a command's arguments do not parse: the
    /// error, then the command's full help.
    /// </summary>
    private void WriteUbBadSyntax(string verb)
    {
        WriteUbError("Bad command syntax");
        WriteUbCommandHelp(UbCommandHelp.Require(verb));
    }

    /// <summary>
    /// Prints what a tool's command answered, line by line; a null answer is
    /// a line the tool could not parse, which gets the bad-syntax answer.
    /// </summary>
    private void PostUbReply(string verb, IReadOnlyList<string>? reply)
    {
        if (reply is null)
        {
            WriteUbBadSyntax(verb);
            return;
        }
        foreach (string line in reply)
            PostUb(line);
    }

    /// <summary>
    /// The reference's full help for one command, as one message: the usage,
    /// the description, then "Examples:" and each example's command and what
    /// it does, indented by one and two spaces.
    /// </summary>
    private void WriteUbCommandHelp(in UbCommandUsage usage)
    {
        var lines = new List<string> { "Description: " + usage.Summary, "Examples:" };
        foreach (UbCommandExample example in usage.Examples ?? [])
        {
            lines.Add(" " + example.Command);
            lines.Add("  " + example.Description);
        }
        WriteUbMessage("Usage: " + usage.Usage, lines);
    }

    private void WriteChunks(IEnumerable<string> values)
    {
        var line = new StringBuilder();
        foreach (string value in values)
        {
            int extra = line.Length == 0 ? value.Length : value.Length + 2;
            if (line.Length != 0 && line.Length + extra > 240)
            {
                WriteVtank(line.ToString());
                line.Clear();
            }
            if (line.Length != 0)
                line.Append(", ");
            line.Append(value);
        }
        if (line.Length != 0)
            WriteVtank(line.ToString());
    }

    private void DumpPropertyTable<T>(string kind, IReadOnlyDictionary<uint, T> values)
    {
        foreach ((uint key, T value) in values.OrderBy(static pair => pair.Key))
            WriteVtank($"{kind}[{key}] = {value}");
    }

    private static (string Head, string Tail) SplitHead(string value)
    {
        string trimmed = value.Trim();
        int separator = trimmed.IndexOfAny([' ', '\t']);
        return separator < 0
            ? (trimmed, string.Empty)
            : (trimmed[..separator], trimmed[(separator + 1)..].Trim());
    }

    private static string StripExtension(string name, params string[] extensions)
    {
        string result = name.Trim();
        foreach (string extension in extensions)
        {
            if (result.EndsWith(extension, StringComparison.OrdinalIgnoreCase))
                return result[..^extension.Length];
        }
        return result;
    }

    private static bool TryParseOptionValue(string source, out ExpressionValue value) =>
        TryParseOptionValue(source, declaredType: null, out value);

    private static bool TryParseOptionValue(
        string source,
        VtankSettingValueType? declaredType,
        out ExpressionValue value)
    {
        if (source.Length == 0)
        {
            value = default;
            return false;
        }
        switch (declaredType)
        {
            case VtankSettingValueType.Bool:
                if (bool.TryParse(source, out bool boolean))
                {
                    value = ExpressionValue.Boolean(boolean);
                    return true;
                }
                value = default;
                return false;
            case VtankSettingValueType.Int:
            case VtankSettingValueType.Enum:
                if (int.TryParse(
                    source, NumberStyles.Integer, CultureInfo.InvariantCulture, out int integer))
                {
                    value = ExpressionValue.Number(integer);
                    return true;
                }
                value = default;
                return false;
            case VtankSettingValueType.Double:
            case VtankSettingValueType.Single:
                if (double.TryParse(
                    source, NumberStyles.Float, CultureInfo.InvariantCulture, out double number))
                {
                    value = ExpressionValue.Number(number);
                    return true;
                }
                value = default;
                return false;
            case VtankSettingValueType.String:
                value = ExpressionValue.String(source);
                return true;
            default:
                if (bool.TryParse(source, out bool freeBoolean))
                {
                    value = ExpressionValue.Boolean(freeBoolean);
                    return true;
                }
                if (double.TryParse(
                    source, NumberStyles.Float, CultureInfo.InvariantCulture, out double freeNumber))
                {
                    value = ExpressionValue.Number(freeNumber);
                    return true;
                }
                value = ExpressionValue.String(source);
                return true;
        }
    }

    private static string DeclaredClrTypeName(VtankSettingValueType type) => type switch
    {
        VtankSettingValueType.Bool => "System.Boolean",
        VtankSettingValueType.Double => "System.Double",
        VtankSettingValueType.Int => "System.Int32",
        VtankSettingValueType.Single => "System.Single",
        VtankSettingValueType.String => "System.String",
        VtankSettingValueType.Enum => "System.Int32",
        _ => "System.Object",
    };

    /// <summary>
    /// The coordinate pair a route-point command takes: a north/south number
    /// and letter, then an east/west number and letter, with spaces and at
    /// most one comma between them. It is searched for, not matched whole, so
    /// text around the pair -- the height a coordinate string ends with, a
    /// trailing comma -- is ignored, as the reference ignores it.
    /// </summary>
    [GeneratedRegex(
        @"(?<ns>[0-9]{1,3}(\.[0-9]*)?)(?<nschr>[nNsS])[ ]*,?[ ]*(?<ew>[0-9]{1,3}(\.[0-9]*)?)(?<ewchr>[eEwW])",
        RegexOptions.CultureInvariant)]
    private static partial Regex RoutePointCoordinates();

    /// <summary>
    /// Reads the point a route-point command names. The point is placed at
    /// ground height whatever the text says about height; only the pair
    /// counts.
    /// </summary>
    private bool TryParseCoordinates(string source, out PluginNavigationPosition position)
    {
        position = default;
        Match match = RoutePointCoordinates().Match(source);
        if (!match.Success)
            return false;
        double northSouth = double.Parse(
            match.Groups["ns"].Value, NumberStyles.Float, CultureInfo.InvariantCulture);
        double eastWest = double.Parse(
            match.Groups["ew"].Value, NumberStyles.Float, CultureInfo.InvariantCulture);
        if (match.Groups["nschr"].Value is "s" or "S")
            northSouth = -northSouth;
        if (match.Groups["ewchr"].Value is "w" or "W")
            eastWest = -eastWest;
        PluginNavigationPosition current = _host.Automation.Navigation.Snapshot.Position;
        position = new PluginNavigationPosition(
            current.CellId,
            eastWest,
            northSouth,
            0d,
            current.HeadingDegrees,
            IsOutdoor: true);
        return true;
    }

    private static bool TryParseJumpDirection(string? value, out RouteJumpDirection direction)
    {
        direction = RouteJumpDirection.Forward;
        if (string.IsNullOrWhiteSpace(value) || value.Equals("forward", StringComparison.OrdinalIgnoreCase))
            return true;
        if (value.Equals("strafeleft", StringComparison.OrdinalIgnoreCase))
        {
            direction = RouteJumpDirection.StrafeLeft;
            return true;
        }
        if (value.Equals("straferight", StringComparison.OrdinalIgnoreCase))
        {
            direction = RouteJumpDirection.StrafeRight;
            return true;
        }
        if (value.Equals("backward", StringComparison.OrdinalIgnoreCase))
        {
            direction = RouteJumpDirection.Backward;
            return true;
        }
        return false;
    }

    private static float NormalizeHeading(float heading)
    {
        float result = heading % 360f;
        return result < 0f ? result + 360f : result;
    }
}
