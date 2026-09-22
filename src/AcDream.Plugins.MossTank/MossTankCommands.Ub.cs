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
    private readonly List<(double RemainingSeconds, string Command)> _delayedCommands = [];

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
            HandleAllegianceCommand(arguments, swearFlags.Contains('p'), swear: true);
            return true;
        }
        if (TryFlags(verb, "breakallegiance", "p", out string breakFlags))
        {
            HandleAllegianceCommand(arguments, breakFlags.Contains('p'), swear: false);
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

    // ── /vt ub, /vt help ────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt ub</c>: which build of the compatibility surface is loaded. A
    /// macro that behaves differently between two clients needs one line it
    /// can be asked for.
    /// </summary>
    private void WriteUbVersion()
    {
        WriteVtank($"MossTank UB compatibility {PluginVersion}");
        WriteVtank("Type /vt help or /vt help <command> for help.");
    }

    /// <summary>
    /// <c>/vt help [command]</c>. Named, it prints that command's usage and
    /// summary; bare, the macro's own verb lists plus the published usage
    /// lines' names.
    /// </summary>
    private void HandleHelpCommand(string arguments)
    {
        string requested = arguments.Trim();
        if (requested.Length != 0)
        {
            if (!UbCommandHelp.TryGet(requested, out UbCommandUsage usage))
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
        WriteVtank("/vt commands (documented): "
            + string.Join(", ", UbCommandHelp.Entries.Select(entry => entry.Name)));
        WriteVtank("For help with a specific command, use /vt help [command].");
    }

    // ── /vt setmotion ───────────────────────────────────────────────────

    [GeneratedRegex(
        @"^(?<motion>\w.+) (?<state>[01])$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant)]
    private static partial Regex SetMotionPattern();

    /// <summary>
    /// <c>/vt setmotion &lt;motion&gt; &lt;0|1&gt;</c>: presses or releases
    /// one movement key and leaves it that way, through the same held keys
    /// the <c>setmotion[]</c> expression uses.
    /// </summary>
    private void HandleSetMotionCommand(string arguments)
    {
        Match match = SetMotionPattern().Match(arguments.Trim());
        if (!match.Success)
        {
            WriteVtank("Bad command syntax");
            WriteVtank("Usage: " + HeldMotions.Usage);
            return;
        }
        string name = match.Groups["motion"].Value;
        if (!HeldMotions.TryParse(name, out HeldMotion motion))
        {
            WriteVtank(
                $"Invalid option ({name}). Valid values are: {HeldMotions.ValidNames}");
            return;
        }
        _expressions.HeldMotions.Set(motion, match.Groups["state"].Value == "1");
    }

    // ── /vt ig ──────────────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt ig[p] &lt;lootProfile&gt; to &lt;target&gt;</c>, and
    /// <c>/vt ig stop</c> to call a run off: the chat form of the profile
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
            _profileGive.StopRequested();
            WriteVtank(_profileGive.Status);
            return;
        }

        // Split at the FIRST " to " for the same reason the name-give does:
        // the profile name is the short half of the line.
        int separator = text.IndexOf(" to ", StringComparison.OrdinalIgnoreCase);
        if (separator <= 0)
        {
            WriteVtank("Syntax: /vt ig[p] <lootProfile> to <target>, or /vt ig stop");
            return;
        }
        string profile = StripExtension(text[..separator].Trim(), ".utl", ".json");
        string target = text[(separator + 4)..].Trim();
        _profileGive.TryStart(profile, target, partialTarget);
        WriteVtank(_profileGive.Status);
    }

    // ── /vt jump, /vt simplejump ────────────────────────────────────────

    /// <summary>
    /// <c>/vt jump[swzxc] [heading] [holdtime]</c>: the reference's grammar.
    /// <c>s</c> holds shift so the jump is a running one, <c>w</c> presses
    /// forward, <c>x</c> backward, <c>z</c> and <c>c</c> strafe left and
    /// right; with no letter at all the character jumps straight up.
    /// </summary>
    /// <remarks>
    /// The older <c>/vt jump &lt;heading&gt; &lt;true|false&gt; &lt;ms&gt;
    /// [direction]</c> form still works: a second word of true or false is
    /// not a hold time in this grammar, so it can only be the older one, and
    /// routes saved with it keep working.
    /// </remarks>
    private void HandleUbJumpCommand(string flags, string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (flags.Length == 0
            && parts.Length >= 2
            && bool.TryParse(parts[1], out _))
        {
            HandleJumpCommand(arguments, addToRoute: false);
            return;
        }
        if (parts.Length > 2)
        {
            WriteVtank("Syntax: /vt jump[swzxc] [heading] [holdtime]");
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
            if (!double.TryParse(
                    headingText,
                    NumberStyles.Float,
                    CultureInfo.InvariantCulture,
                    out double requested))
            {
                WriteVtank("Syntax: /vt jump[swzxc] [heading] [holdtime]");
                return;
            }
            if (requested is < 0d or > 359d)
            {
                WriteVtank("direction should be a number between 0 and 359");
                return;
            }
            heading = (float)requested;
        }

        if (!RefuseWhenAlreadyJumping())
            return;
        StartCommandJump(
            heading,
            shift: flags.Contains('s'),
            milliseconds,
            JumpDirectionFromFlags(flags, out bool omitDirection),
            omitDirection: omitDirection);
    }

    /// <summary>
    /// <c>/vt simplejump [holdtime]</c>: a jump where the character already
    /// stands, with no turn first.
    /// </summary>
    private void HandleSimpleJumpCommand(string arguments)
    {
        string[] parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length > 1)
        {
            WriteVtank("Syntax: /vt simplejump [holdtime]");
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
            omitDirection: true);
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
            WriteVtank("holdtime should be a number between 0 and 1000");
            return false;
        }
        if (milliseconds is < 0 or > 1000)
        {
            WriteVtank("holdtime should be a number between 0 and 1000");
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
        WriteVtank("You are already jumping. try again later.");
        return false;
    }

    private static RouteJumpDirection JumpDirectionFromFlags(
        string flags,
        out bool omitDirection)
    {
        omitDirection = false;
        if (flags.Contains('w'))
            return RouteJumpDirection.Forward;
        if (flags.Contains('x'))
            return RouteJumpDirection.Backward;
        if (flags.Contains('z'))
            return RouteJumpDirection.StrafeLeft;
        if (flags.Contains('c'))
            return RouteJumpDirection.StrafeRight;
        omitDirection = true;
        return RouteJumpDirection.Forward;
    }

    // ── /vt calcdamage ──────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt calcdamage</c>: what the selected missile weapon would hit for
    /// once every tinker it can still take has gone in. Only the cantrips on
    /// the weapon itself count; nothing the character is wearing does.
    /// </summary>
    private void CalculateSelectedDamage()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u
            || !_host.Automation.Objects.TryGet(selected, out PluginWorldObject item))
        {
            WriteVtank("Nothing selected");
            return;
        }
        if (!_host.Automation.Objects.TryCaptureProperties(
                selected,
                out PluginItemProperties properties)
            || properties.WeaponProfile is not { } profile)
        {
            WriteVtank($"{item.Name} does not have id data, please examine it first.");
            return;
        }
        if (item.ObjectClass != PluginObjectClass.MissileWeapon)
        {
            WriteVtank($"Calc Damage: {item.ObjectClass} is not currently supported");
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
            WriteVtank(Invariant(
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
            WriteVtank(Invariant(
                $"{tinkersAvailable} mahogany salvage adds {perTinker} to DamageModifier"));
        }
        WriteVtank("Formula: (DamageBonus + ElementalBonus) * DamageModifier");
        WriteVtank(Invariant(
            $"Calculated Formula: ({maxDamage}(+{cantripBonus} from cantrips) + {elementalBonus}) * {damageBonus - 1d}(+{perTinker} from {tinkersAvailable} tinkers)"));
        WriteVtank(Invariant($"Calculated (after tinks): {damage}"));
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

    // ── /vt pos, /vt id, /vt vitae, /vt combatstate, /vt date ───────────

    /// <summary>
    /// <c>/vt pos</c>: where the selected object stands, in every form the
    /// client can say it -- the id, the compass coordinates, the landcell and
    /// the raw position inside it.
    /// </summary>
    private void PrintSelectedPosition()
    {
        if (!TryGetSelectedObject("pos", out PluginWorldObject item))
            return;
        if (!item.HasPosition)
        {
            WriteVtank($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
            WriteVtank("pos: the client knows no position for that object.");
            return;
        }
        PluginNavigationPosition position = item.Position;
        var coordinates = new ExpressionCoordinates(
            position.EastWest,
            position.NorthSouth,
            position.Elevation);
        WriteVtank($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
        WriteVtank($"Coords: {coordinates}");
        WriteVtank($"Landcell: 0x{position.CellId:X8}");
        WriteVtank(Invariant(
            $"Position: ew:{position.EastWest} ns:{position.NorthSouth} z:{position.Elevation}"));
        WriteVtank(Invariant(
            $"Distance: {UbObjectSearch.Distance(_host.Automation.Navigation.Snapshot, item):0.###} m"));
    }

    /// <summary><c>/vt id</c>: the selected object's id, both ways round.</summary>
    private void PrintSelectedId()
    {
        if (!TryGetSelectedObject("Id", out PluginWorldObject item))
            return;
        WriteVtank($"Id: {item.ObjectId} ( 0x{item.ObjectId:X8} )");
    }

    private bool TryGetSelectedObject(string verb, out PluginWorldObject item)
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u)
        {
            WriteVtank($"{verb}: No object selected");
            item = default;
            return false;
        }
        if (!_host.Automation.Objects.TryGet(selected, out item))
        {
            WriteVtank($"{verb}: null object selected");
            return false;
        }
        return true;
    }

    /// <summary>
    /// <c>/vt vitae</c>: says the penalty out loud, as a think, so a macro
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
    private void Think(string text)
    {
        string name = _host.Automation.Character.Name;
        if (name.Length == 0 || !_host.Automation.Chat.Submit($"/t {name}, {text}"))
            WriteVtank($"You think, \"{text}\"");
    }

    /// <summary>
    /// <c>/vt combatstate (peace|melee|missile|magic)</c>: asks the client to
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
                WriteVtank($"{requested} is not a valid option");
                return;
        }
        PluginCombatCommandResult result = _host.Automation.Combat.EnterMode(mode);
        WriteVtank(result.Accepted
            ? $"Combat state set to {mode}."
            : $"Could not set combat state to {mode}: {result.Status}");
    }

    /// <summary>
    /// <c>/vt date[utc] [format]</c>. The format is a .NET custom date
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
            WriteVtank("Current Date: "
                + now.ToString(pattern, CultureInfo.InvariantCulture));
        }
        catch (FormatException error)
        {
            WriteVtank(error.Message);
        }
    }

    // ── /vt delay ───────────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt delay &lt;milliseconds&gt; &lt;command&gt;</c>: hands the line
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
            WriteVtank("Syntax: /vt delay <milliseconds> <command>");
            return;
        }
        _delayedCommands.Add((milliseconds / 1000d, rest));
        _delayedCommands.Sort(static (left, right) =>
            left.RemainingSeconds.CompareTo(right.RemainingSeconds));
        WriteVtank(Invariant(
            $"Scheduling command `{rest}` with delay of {milliseconds}ms"));
    }

    /// <summary>Runs the delayed commands whose wait is over.</summary>
    private void TickDelayedCommands(double elapsedSeconds)
    {
        if (_delayedCommands.Count == 0)
            return;
        double step = Math.Max(0d, elapsedSeconds);
        var due = new List<string>();
        for (int index = _delayedCommands.Count - 1; index >= 0; index--)
        {
            (double remaining, string command) = _delayedCommands[index];
            remaining -= step;
            if (remaining > 0d)
            {
                _delayedCommands[index] = (remaining, command);
                continue;
            }
            due.Add(command);
            _delayedCommands.RemoveAt(index);
        }
        // The loop above walks backwards, so the oldest entry comes out last;
        // a pair scheduled together must run in the order they were typed.
        for (int index = due.Count - 1; index >= 0; index--)
            _host.Automation.Chat.Submit(due[index]);
    }

    // ── /vt opt, over both settings groups ──────────────────────────────

    /// <summary>
    /// The second half of the option list: the UB settings, which are named
    /// with a dot ("AutoVendor.Enabled") and so can never collide with the
    /// macro's own option names.
    /// </summary>
    private void ListUbSettings()
    {
        WriteVtank($"UB settings: ({_ubCatalog.Settings.Count})");
        foreach (UbSetting setting in _ubCatalog.Settings)
            WriteVtank($"   {setting.Name} ({setting.Kind}) = {setting.Display()}");
    }

    /// <summary>Prints one UB setting. False when there is no such row.</summary>
    private bool TryWriteUbSetting(string name)
    {
        if (!_ubCatalog.TryGet(name.Trim(), out UbSetting setting))
            return false;
        WriteVtank($"{setting.Name} ({setting.Kind}) = {setting.Display()}");
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
        if (rawValue.Trim().Length == 0)
        {
            WriteVtank($"{setting.Name} ({setting.Kind}) = {setting.Display()}");
            return true;
        }
        if (!UbSettingValue.TryParse(
            setting.Kind,
            rawValue.Trim(),
            out UbSettingValue value))
        {
            WriteVtank(
                $"Option set: Invalid value specified. {setting.Name} is a "
                + $"{setting.Kind}.");
            return true;
        }
        setting.Set(value);
        WriteVtank($"{setting.Name} ({setting.Kind}) = {setting.Display()}");
        return true;
    }

    /// <summary>
    /// <c>/vt opt toggle &lt;option&gt;</c>: flips a switch, in whichever of
    /// the two groups owns the name. Anything that is not a switch is left
    /// alone, because there is no second value to flip to.
    /// </summary>
    private void ToggleOption(string name)
    {
        string requested = name.Trim();
        if (requested.Length == 0)
        {
            WriteVtank("Syntax: /vt opt toggle <option>");
            return;
        }
        if (VtankOptionCatalog.IsKnown(requested))
        {
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
            return;
        }
        if (!_ubCatalog.TryGet(requested, out UbSetting setting))
        {
            WriteVtank("Option toggle: Invalid option specified.");
            return;
        }
        if (setting.Kind != UbSettingKind.Bool)
        {
            WriteVtank($"Unable to toggle setting {setting.Name}: it is not a switch.");
            return;
        }
        setting.Set(UbSettingValue.FromBool(!setting.Get().Boolean));
        WriteVtank($"{setting.Name} ({setting.Kind}) = {setting.Display()}");
    }

    // ── /vt closestportal, /vt portal ───────────────────────────────────

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
            WriteVtank("Could not find a portal");
            return;
        }
        WriteVtank($"Attempting to use portal: {portal.Name}");
        PluginItemCommandResult result = _host.Automation.Items.Use(portal.ObjectId);
        if (!result.Accepted)
            WriteVtank($"Unable to use portal {portal.Name}: {result.Status}");
    }

    // ── /vt follow ──────────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt follow[p] &lt;name&gt;</c>: turns the loaded route into a follow
    /// of that player. A follow route needs no waypoints of its own, so this
    /// is the mode switch and the target, nothing more.
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
            WriteVtank(requested.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {requested}");
            return;
        }
        _navigationSettings.FollowTargetObjectId = player.ObjectId;
        _navigationSettings.FollowTargetName = player.Name;
        _navigationSettings.Mode = RouteMode.Target;
        _navigation.Reset();
        RefreshRouteEditor();
        SaveRouteProfile();
        WriteVtank($"Following {player.Name}[0x{player.ObjectId:X8}]");
        if (!_navigationSettings.Enabled)
            WriteVtank("Turn Enable Navigation on to start.");
    }

    // ── /vt use, /vt select, /vt close ──────────────────────────────────

    /// <summary>
    /// <c>/vt use[li][p] [itemOne] on [itemTwo]</c>. One name uses that
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
            WriteVtank("Syntax: /vt use[li][p] [itemOne] on [itemTwo]");
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
            WriteVtank($"Could not find object: {first}");
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
            WriteVtank($"{second} is null");
            return;
        }
        WriteVtank($"using {one.Name} on {two.Name}");
        _host.Automation.Items.Apply(one.ObjectId, two.ObjectId);
    }

    private void UseSingleObject(in PluginWorldObject one, bool partial)
    {
        WriteVtank("using " + one.Name);
        switch (one.ObjectClass)
        {
            case PluginObjectClass.Portal:
                UsePortalByName(one.Name, partial);
                return;
            case PluginObjectClass.Vendor:
                foreach (string line in _vendorTrade.VendorCommand("open " + one.Name))
                    WriteVtank(line);
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

    /// <summary><c>/vt select[li][p] [item]</c>.</summary>
    private void HandleSelectCommand(string flags, string arguments)
    {
        if (!TryReadScope(flags, out UbSearchScope scope, out bool partial))
            return;
        string text = arguments.Trim();
        if (text.Length == 0)
        {
            WriteVtank("Syntax: /vt select[li][p] [item]");
            return;
        }
        if (!UbObjectSearch.TryFind(
            _host, text, scope, partial, 0u, out PluginWorldObject item))
        {
            WriteVtank($"Could not find object: {text}");
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
            WriteVtank("l and i cannot be used in the same command");
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

    /// <summary><c>/vt close corpse</c> (or chest).</summary>
    private void HandleCloseCommand(string arguments)
    {
        string requested = arguments.Trim().ToLowerInvariant();
        if (requested is not ("corpse" or "chest"))
        {
            WriteVtank("Syntax: /vt close corpse");
            return;
        }
        uint open = _host.Automation.Objects.OpenContainerObjectId;
        if (open == 0u)
        {
            WriteVtank("No container is currently open.");
            return;
        }
        if (!_host.Automation.Objects.TryGet(open, out PluginWorldObject container))
        {
            WriteVtank("No container is currently open.");
            return;
        }
        bool matches = requested == "corpse"
            ? container.ObjectClass == PluginObjectClass.Corpse
            : container.ObjectClass == PluginObjectClass.Container;
        if (!matches)
        {
            WriteVtank($"The open container is a {container.ObjectClass}, not a {requested}.");
            return;
        }
        _host.Automation.Loot.Close(open);
    }

    // ── /vt swearallegiance, /vt breakallegiance ────────────────────────

    /// <summary>
    /// Resolves who the allegiance command means and asks the client to send
    /// it. Whether the server allows it is the server's own decision and
    /// arrives later as a restated allegiance, so a sent command reports
    /// only that it went out.
    /// </summary>
    private void HandleAllegianceCommand(string name, bool partial, bool swear)
    {
        string requested = name.Trim();
        if (!UbObjectSearch.TryFindNearest(
            _host,
            requested,
            partial,
            PlayerClasses,
            out PluginWorldObject player))
        {
            WriteVtank(requested.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {requested}");
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
                WriteVtank(swear
                    ? $"Swearing allegiance to {who}."
                    : $"Breaking allegiance from {who}.");
                return;
            case PluginAllegianceCommandStatus.InvalidTarget:
                WriteVtank(swear
                    ? $"Cannot swear allegiance to {who}."
                    : $"{who} is not in your allegiance.");
                return;
            case PluginAllegianceCommandStatus.Refused:
                // A refusal is about the session, not about the target, so the
                // client's own reason is what is worth printing.
                WriteVtank(string.IsNullOrWhiteSpace(result.Notice)
                    ? (swear
                        ? $"Refused to swear allegiance to {who}."
                        : $"Refused to break allegiance from {who}.")
                    : result.Notice);
                return;
            default:
                WriteVtank(swear
                    ? "Cannot swear allegiance right now."
                    : "Cannot break allegiance right now.");
                return;
        }
    }

    // ── /vt printcolors ─────────────────────────────────────────────────

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

    // ── /vt listvars, listpvars, listgvars ──────────────────────────────

    /// <summary>
    /// Prints one variable store. The three stores differ only in how long
    /// they live: the session's, the character's, and the server's.
    /// </summary>
    private void ListVariables(ExpressionVariableScope scope)
    {
        WriteVtank(scope switch
        {
            ExpressionVariableScope.Persistent => "Defined persistent variables:",
            ExpressionVariableScope.Global => "Defined global variables:",
            _ => "Defined variables:",
        });
        foreach ((string name, ExpressionValue value) in _expressions.State
            .Capture(scope)
            .OrderBy(static pair => pair.Key, StringComparer.OrdinalIgnoreCase))
        {
            WriteVtank($"{name} ({value.Kind}) = {value.ToDisplayString()}");
        }
    }

    // ── /vt translateroute ──────────────────────────────────────────────

    /// <summary>
    /// <c>/vt translateroute &lt;startLandblock&gt; &lt;route&gt;
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
            WriteVtank(
                "Syntax: /vt translateroute <startLandblock> <routeToLoad> "
                + "<endLandblock> <routeToSaveAs> [force]");
            return;
        }
        if (!TryParseLandblock(parts[0], out uint start))
        {
            WriteVtank($"Could not parse hex value from StartLandblock: {parts[0]}");
            return;
        }
        if (!TryParseLandblock(parts[2], out uint end))
        {
            WriteVtank($"Could not parse hex value from EndLandblock: {parts[2]}");
            return;
        }

        double eastWest = LandblockDifference(start >> 24, end >> 24) / 240d;
        double northSouth =
            LandblockDifference((start << 8) >> 24, (end << 8) >> 24) / 240d;
        bool force = parts.Length == 5;
        bool translated = _routeProfiles.TryTranslate(
            StripExtension(parts[1], ".nav", ".af"),
            StripExtension(parts[3], ".nav", ".af"),
            eastWest,
            northSouth,
            force,
            _host.Automation.Spells,
            out string notice);
        WriteVtank(notice);
        if (translated)
        {
            WriteVtank(Invariant(
                $"Translated from {start:X8} to {end:X8} by adding offsets NS:{northSouth} EW:{eastWest}"));
        }
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
    /// A line whose numbers read the same on every machine. Retail text is
    /// US-formatted for everyone, and a macro parsing its own chat must not
    /// see a comma where it expects a decimal point.
    /// </summary>
    private static string Invariant(FormattableString text) =>
        text.ToString(CultureInfo.InvariantCulture);
}
