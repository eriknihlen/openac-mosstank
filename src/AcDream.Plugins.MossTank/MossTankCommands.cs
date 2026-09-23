using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private static readonly string[] VtankHelp =
    [
        "MossTank /vt — profiles: settings nav loot meta opt testitem propertydump addnavpt refresh getdb gamedb addnavjump addnavcheckpoint",
        "MossTank /vt — actions: start stop forcebuff cancelforcebuff setmetastate fakedeath deathrestore deletemonster reverseroute reverseroutequery equipitemsfor equip mexec metainterval nextwp echo tapjump jump face setattackbar setmotion clearmotion prepclick fellow count login give autovendor vendor xp",
        "MossTank /vt — game info: dumpspells dumpspecies dumpmats dumpskills",
        "MossTank /vt — debug: log testmonster lockdump dumptracker clearlocks clearbusy listmonstervariables dumpmetavars listmetafunctions metafunchelp fakeimp pscount testspell testpet",
    ];

    /// <summary>The macro's own command word.</summary>
    internal const string VtankVerb = "vt";

    /// <summary>The second command word, the one UtilityBelt metas type.</summary>
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

    private bool _commandJumpActive;
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
    private bool _commandPortalState;
    private int _commandPortalCount;

    /// <summary>
    /// Runs one line typed under either verb, <c>/vt</c> or <c>/ub</c>: the
    /// two answer to the same commands, so a meta written for either runs
    /// unchanged. Only a bare verb differs: <c>/vt</c> alone prints the help,
    /// <c>/ub</c> alone the compatibility line, as <c>/vt ub</c> does.
    /// </summary>
    internal void ExecuteVtankCommand(PluginCommand command)
    {
        try
        {
            if (command.Verb.Equals(UbVerb, StringComparison.OrdinalIgnoreCase)
                && string.IsNullOrWhiteSpace(command.Arguments))
            {
                WriteUbVersion();
                return;
            }
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
            case "help":
                HandleHelpCommand(arguments);
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
            case "nav":
                HandleRouteProfileCommand(arguments);
                return;
            case "loot":
                HandleLootProfileCommand(arguments);
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
                foreach (string line in _vendorTrade.VendorCommand(arguments))
                    WriteVtank(line);
                return;
            case "xp":
                foreach (string line in _experienceSpend.Command(arguments))
                    WriteVtank(line);
                return;
            case "equip":
                foreach (string line in _equipProfile.Command(arguments))
                    WriteVtank(line);
                return;
            case "tapjump":
                StartCommandJump(
                    _host.Automation.Navigation.Snapshot.Position.HeadingDegrees,
                    shift: false,
                    milliseconds: 100,
                    null);
                return;
            case "jump":
                HandleUbJumpCommand(string.Empty, arguments);
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
                // The flagged verbs (jumpsw, igp, usepi, ...) cannot be
                // switch cases: their letters combine, so they are matched by
                // name and flag set rather than spelled out.
                if (!TryExecuteFlaggedCommand(verb, arguments))
                    WriteVtank("Unknown /vt command. Use /vt help.");
                return;
        }
    }

    /// <summary>
    /// <c>/vt count item &lt;namepattern&gt;</c>,
    /// <c>/vt count profile &lt;lootprofile&gt;</c>,
    /// <c>/vt count player &lt;range&gt;</c> and <c>/vt count stop</c>. The
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
                if (!_inventoryCount.TryStartProfile(
                    StripExtension(subject, ".utl", ".json"),
                    foreground: true))
                {
                    WriteVtank(_inventoryCount.Status);
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
                    WriteVtank($"bad player count range: {subject}");
                    return;
                }
                _inventoryCount.ReportPlayerCount(range);
                return;
            case "stop":
                // A profile count holds the character while it waits, so it
                // needs a way back that is not a relog.
                WriteVtank(_inventoryCount.Cancel());
                return;
            default:
                WriteVtank(
                    "Syntax: /vt count [item <namepattern> | "
                    + "profile <lootprofile> | player <range> | stop]");
                return;
        }
    }

    /// <summary>
    /// <c>/vt login next[r][l] &lt;name-or-index&gt;</c>, <c>/vt login clear</c>
    /// and <c>/vt login list</c>: which character the client logs in as after
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
            WriteVtank("Syntax: /vt login [next[r][l] <name|index> | clear | list]");
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
            WriteVtank("Syntax: /vt login [next[r][l] <name|index> | clear | list]");
            return;
        }
        if (selector.Length == 0)
        {
            WriteVtank("Specify part of a name or an index: /vt login next <name|index>");
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
            WriteVtank(failure switch
            {
                LoginRosterFailure.Empty =>
                    "The account's character list has not arrived yet.",
                LoginRosterFailure.NoCurrentCharacter =>
                    "Cannot log in relative to a character that is not on the list.",
                LoginRosterFailure.OutOfRange =>
                    "Login index is out of bounds. Add the [l]oop flag to wrap "
                    + "around: /vt login nextl -100",
                LoginRosterFailure.PendingDelete =>
                    "That character is scheduled for deletion and cannot be played.",
                _ => $"No character found with name {selector}.",
            });
            if (failure != LoginRosterFailure.Empty)
                ClearNextLogin();
            return;
        }

        PluginLoginCharacter chosen = roster[index];
        WriteVtank(login.SetNextLogin(chosen.ObjectId)
            ? $"Logging in as {chosen.Name} next at index {index}."
            : $"Could not set the next login to {chosen.Name}.");
    }

    /// <summary>
    /// <c>/vt autovendor</c> runs the open vendor by its own profile,
    /// <c>/vt autovendor &lt;profile&gt;</c> by a named one, and
    /// <c>/vt autovendor stop</c> (or cancel, quit) calls a run off.
    /// </summary>
    private void HandleAutoVendorCommand(string arguments)
    {
        string text = arguments.Trim();
        if (text.Equals("stop", StringComparison.OrdinalIgnoreCase)
            || text.Equals("cancel", StringComparison.OrdinalIgnoreCase)
            || text.Equals("quit", StringComparison.OrdinalIgnoreCase))
        {
            _vendorTrade.StopRequested();
            WriteVtank(_vendorTrade.Status);
            return;
        }
        _vendorTrade.TryStart(text.Length == 0 ? null : text);
        WriteVtank(_vendorTrade.Status);
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
    /// <c>/vt give[p{P|r}] [count] &lt;item&gt; to &lt;target&gt;</c>, and
    /// <c>/vt give stop</c> (or cancel, quit, abort) to call a run off.
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
            _profileGive.StopRequested();
            WriteVtank(_profileGive.Status);
            return;
        }

        int separator = text.IndexOf(" to ", StringComparison.OrdinalIgnoreCase);
        if (separator <= 0)
        {
            WriteVtank(
                "Syntax: /vt give[p{P|r}] [itemCount] <itemName> to <target>, "
                + "or /vt give stop");
            return;
        }
        string item = text[..separator].Trim();
        string target = text[(separator + 4)..].Trim();

        // A leading whole number is a count, but only when something is left
        // to be the item name: "/vt give 10 to Bob" gives an item called 10.
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

        if (!_profileGive.TryStartByName(item, match, count, target, partialTarget))
            WriteVtank(_profileGive.Status);
    }

    private void ClearNextLogin() => WriteVtank(
        _host.Automation.Login.ClearNextLogin()
            ? "Next login cleared."
            : "There was no next login to clear.");

    /// <summary>
    /// The account's characters with both numbers that describe them: the
    /// alphabetical index every selector speaks in, and the slot the account's
    /// own list keeps each character in. They are rarely the same number, so
    /// printing only one of them would mislead.
    /// </summary>
    private void PrintLoginRoster()
    {
        IReadOnlyList<PluginLoginCharacter> roster =
            LoginRoster.Capture(_host.Automation.Login);
        WriteVtank($"Listing {roster.Count} logins.");
        if (roster.Count == 0)
            return;
        WriteVtank($"{"Index",-8}{"Name",-32}{"Id",-12}Slot");
        string current = _host.Automation.Character.Name;
        for (int index = 0; index < roster.Count; index++)
        {
            PluginLoginCharacter character = roster[index];
            string name = character.Name.Equals(
                current,
                StringComparison.OrdinalIgnoreCase)
                    ? $"**{character.Name}**"
                    : character.Name;
            if (character.IsPendingDelete)
                name += " (deleting)";
            WriteVtank(
                $"{index,-8}{name,-32}0x{character.ObjectId:X8}  "
                + character.ActiveIndex.ToString(CultureInfo.InvariantCulture));
        }
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

    private void HandleRouteProfileCommand(string arguments)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0 || operation is not ("save" or "load"))
        {
            WriteVtank("Syntax: /vt nav [save/load] [filename]");
            return;
        }
        name = StripExtension(name, ".nav", ".af");
        if (operation == "save")
        {
            _routeProfiles.Create(
                name,
                copyCurrent: true,
                _navigationSettings,
                out string notice);
            RefreshRouteEditor();
            WriteVtank(notice);
            return;
        }
        if (!_routeProfiles.Select(name))
        {
            if (!_routeProfiles.TryImportLegacy(
                    name,
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

    private void HandleMetaProfileCommand(string arguments)
    {
        (string operation, string name) = SplitHead(arguments);
        operation = operation.ToLowerInvariant();
        if (name.Length == 0 || operation is not ("save" or "load"))
        {
            WriteVtank("Syntax: /vt meta [save/load] [filename]");
            return;
        }
        name = StripExtension(name, ".met", ".json", ".af");
        if (operation == "save")
        {
            _metaProfiles.Create(
                name,
                copyCurrent: true,
                _metaProfile,
                out string notice);
            LoadMetaProfile();
            WriteVtank(notice);
            return;
        }
        if (!_metaProfiles.Select(name))
        {
            if (!_metaProfiles.TryImportLegacy(
                name,
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
                ListUbSettings();
                return;
            case "toggle":
                ToggleOption(tail);
                return;
            case "get":
                if (!VtankOptionCatalog.IsKnown(tail))
                {
                    // A name the macro's own catalogue does not carry may
                    // still be one of the UB settings, which are dotted.
                    if (TryWriteUbSetting(tail))
                        return;
                    WriteVtank("Option get: unknown option name.");
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
                    if (TrySetUbSetting(name, rawValue))
                        return;
                    WriteVtank("Option set: unknown option name.");
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
                JumpRun = shift,
                JumpChargeMilliseconds = milliseconds,
                JumpDirection = direction,
            });
            SaveRouteProfile();
            RefreshRouteEditor();
            WriteVtank("Added jump to the current route.");
            if (direction == RouteJumpDirection.Backward)
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
    /// <c>/vt face &lt;heading&gt;</c>: the jump command's turn on its own.
    /// It runs the same alignment loop and stops the moment the character is
    /// pointing the right way.
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
            WriteVtank("Syntax: /vt face [heading]");
            return;
        }
        StartCommandJump(
            heading,
            shift: false,
            milliseconds: 0,
            RouteJumpDirection.Forward,
            faceOnly: true);
    }

    /// <summary>
    /// Begins a command jump: turn to the heading, then hold the jump key for
    /// the charge time while pressing whichever movement key the jump leans
    /// on.
    /// </summary>
    /// <param name="omitDirection">
    /// True for a jump that presses no movement key at all -- straight up on
    /// the spot, which is what a bare jump verb asks for.
    /// </param>
    private void StartCommandJump(
        float heading,
        bool shift,
        int milliseconds,
        RouteJumpDirection? direction,
        bool faceOnly = false,
        bool omitDirection = false)
    {
        PluginNavigationSnapshot snapshot = _host.Automation.Navigation.Snapshot;
        if (!snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            WriteVtank(faceOnly
                ? "Turning unavailable outside the world."
                : "Jump unavailable outside the world.");
            return;
        }
        RouteJumpDirection resolved = direction ?? RouteJumpDirection.Forward;
        _commandJumpHeading = NormalizeHeading(heading);
        _commandJumpIntent = new PluginMovementIntent(
            Forward: !omitDirection && resolved == RouteJumpDirection.Forward,
            Backward: !omitDirection && resolved == RouteJumpDirection.Backward,
            StrafeLeft: !omitDirection && resolved == RouteJumpDirection.StrafeLeft,
            StrafeRight: !omitDirection && resolved == RouteJumpDirection.StrafeRight,
            Run: shift,
            Jump: true);
        _commandJumpChargeSeconds = Math.Clamp(milliseconds / 1000d, 0.05d, 5d);
        _commandJumpElapsed = 0d;
        _commandJumpTurnElapsed = 0d;
        _commandJumpReleased = false;
        _commandJumpCharging = false;
        _commandJumpFaceOnly = faceOnly;
        _commandJumpSawAirborne = false;
        _commandJumpAttempt = 0;
        _commandJumpSequence = 0L;
        _commandJumpActive = true;
        HoldNavigationForJump(faceOnly
            ? CommandTurnNavigationHoldSeconds
            : CommandJumpNavigationHoldSeconds);
        WriteVtank(faceOnly
            ? $"Turning to heading {_commandJumpHeading:0.#}."
            : $"Turning to heading {_commandJumpHeading:0.#} for jump.");
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
        else
            WriteVtank(reason);
    }

    private bool TickCommandJump(double elapsedSeconds)
    {
        if (!_commandJumpActive)
            return false;

        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot snapshot = navigation.Snapshot;
        if (!snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            navigation.ClearMovementIntent();
            FailCommandJump(_commandJumpFaceOnly
                ? "Turn canceled because the character left the world."
                : "Jump canceled because the character left the world.");
            return false;
        }

        if (!_commandJumpCharging)
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

            _commandJumpCharging = navigation.SetMovementIntent(_commandJumpIntent)
                == PluginNavigationCommandStatus.Accepted;
            if (!_commandJumpCharging)
            {
                navigation.ClearMovementIntent();
                FailCommandJump("Jump command was refused by the host.");
                return false;
            }
            _commandJumpElapsed = 0d;
            _commandJumpReleased = false;
            _commandJumpSawAirborne = false;
            // The client counts the jumps it has begun. Whatever that count
            // stands at now is what this attempt has to move past.
            _commandJumpSequence = navigation.MoveReport.JumpSequence;
            _commandJumpAttempt++;
            WriteVtank(_commandJumpAttempt == 1
                ? $"Jump charging at heading {_commandJumpHeading:0.#}."
                : $"Jump charging at heading {_commandJumpHeading:0.#} "
                    + $"(attempt {_commandJumpAttempt}).");
        }

        _commandJumpElapsed += Math.Max(0d, elapsedSeconds);
        if (!_commandJumpReleased && _commandJumpElapsed >= _commandJumpChargeSeconds)
        {
            _commandJumpReleased = true;
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
            navigation.ClearMovementIntent();
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
            navigation.ClearMovementIntent();
            _commandJumpCharging = false;
            FailCommandJump(
                $"Jump gave up after {_commandJumpAttempt} attempt(s) with no "
                + "jump reported.");
            return false;
        }
        _commandJumpCharging = false;
        _commandJumpTurnElapsed = 0d;
        // The hold was taken against one charge; the next one takes its own.
        HoldNavigationForJump(CommandJumpNavigationHoldSeconds);
        return true;
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

    private void TestPet()
    {
        IReadOnlyList<PluginCombatTarget> targets = _host.Automation.Combat
            .CaptureHostileTargets((float)(_combatSettings.PetRangeMode == PetRangeMode.Custom
                ? _combatSettings.PetCustomRange
                : _combatSettings.MaximumRange));
        PetAutomationChoice choice = PetAutomation.Select(
            _host.Automation.Items.CaptureOwnedItems(),
            targets,
            _host.Automation.Character,
            _combatSettings,
            _host.Automation.Items.ActiveOwnedPetCount,
            allowRefill: true,
            allowSummon: true);
        WriteVtank(choice.Kind == PetAutomationActionKind.None
            ? "Pet can spawn: False"
            : $"Pet can spawn: True, action: {choice.Kind}, device: {choice.Device.Name}");
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

    private void DumpSkills()
    {
        WriteVtank($"Character skills ({_host.Automation.Character.Skills.Count}):");
        foreach (PluginSkillInfo skill in _host.Automation.Character.Skills.OrderBy(static value => value.SkillId))
            WriteVtank($"{skill.SkillId}\t{skill.Name}\t{skill.Base}\t{skill.Current}\t{skill.Training}");
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
            _host.Automation.Navigation.ClearMovementIntent();
        _commandJumpActive = false;
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

    private bool TryParseCoordinates(string source, out PluginNavigationPosition position)
    {
        position = default;
        string[] parts = source.Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2
            || !TryParseCompass(parts[0], northSouth: true, out double northSouth)
            || !TryParseCompass(parts[1], northSouth: false, out double eastWest))
        {
            return false;
        }
        double elevation = 0d;
        if (parts.Length >= 3
            && !double.TryParse(parts[2], NumberStyles.Float, CultureInfo.InvariantCulture, out elevation))
        {
            return false;
        }
        PluginNavigationPosition current = _host.Automation.Navigation.Snapshot.Position;
        position = new PluginNavigationPosition(
            current.CellId,
            eastWest,
            northSouth,
            elevation,
            current.HeadingDegrees,
            IsOutdoor: true);
        return true;
    }

    private static bool TryParseCompass(string source, bool northSouth, out double value)
    {
        value = 0d;
        string trimmed = source.Trim();
        if (trimmed.Length < 2)
            return false;
        char direction = char.ToUpperInvariant(trimmed[^1]);
        if (northSouth ? direction is not ('N' or 'S') : direction is not ('E' or 'W'))
            return false;
        if (!double.TryParse(trimmed[..^1], NumberStyles.Float, CultureInfo.InvariantCulture, out double magnitude))
            return false;
        value = direction is 'S' or 'W' ? -Math.Abs(magnitude) : Math.Abs(magnitude);
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
