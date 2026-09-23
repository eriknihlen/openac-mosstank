using System.Globalization;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    /// <summary>The database each of its readers is using right now.</summary>
    internal IReadOnlyList<VtankGameInfoDatabase> GameInfoReadersForTest =>
    [
        _gameInfo,
        _combat.GameInfo,
        _combatSettings.MonsterFacts.Database,
        _crafting.GameInfo,
    ];

    internal IReadOnlyDictionary<string, VtankHealKit> HealKitsForTest =>
        _combatSettings.HealKits;

    internal Task<VtankGameInfoUpdateResult>? GameInfoUpdatePendingForTest =>
        _gameInfoUpdater.PendingForTest;

    /// <summary>
    /// Once a session, as soon as it is in the world, the game database asks
    /// its service for what changed -- the reference client's own check at
    /// login -- unless the database was checked within the interval the
    /// preferences set, so many sessions logging in do not all ask. The request runs off the game thread; what it says and the
    /// database it brings back arrive here, on the tick.
    /// </summary>
    private void TickGameInfoUpdate()
    {
        _gameInfoUpdater.Drain(WriteVtank, ApplyGameInfo);
        // The session counts as checked only once its own check has started.
        // One still running from the session before (a quick relog) is let
        // finish and handed over first, and this session asks after it. A
        // session with nothing to check with has nothing to say; the command
        // says why when it is asked for.
        if (!_gameInfoCheckedThisSession
            && _gameInfoUpdater.CanUpdate
            && !_gameInfoUpdater.IsRunning)
        {
            _gameInfoCheckedThisSession = _gameInfoUpdater.Start(GameInfoCheckInterval) is null;
        }
    }

    /// <summary>
    /// The one place a newer game database is handed over. Every reader --
    /// combat, crafting, the monster facts, the heal kit table -- is given
    /// the new one here; none of them keeps a table of its own.
    /// </summary>
    private void ApplyGameInfo(VtankGameInfoDatabase gameInfo)
    {
        _gameInfo = gameInfo;
        _combatSettings.MonsterFacts.Replace(gameInfo);
        _combatSettings.HealKits = HealKitTable(gameInfo);
        _combat.ReplaceGameInfo(gameInfo);
        _crafting.ReplaceGameInfo(gameInfo);
    }

    private static Dictionary<string, VtankHealKit> HealKitTable(VtankGameInfoDatabase gameInfo)
    {
        var healKits = new Dictionary<string, VtankHealKit>(StringComparer.OrdinalIgnoreCase);
        foreach (VtankHealKit kit in gameInfo.HealKits)
            healKits[kit.Name] = kit;
        return healKits;
    }

    internal const string GameDbSyntax = "Syntax: /vt gamedb [update | interval [hours]]";

    /// <summary>
    /// <c>/vt gamedb [update | interval [hours]]</c>, and the reference's own
    /// <c>/vt getdb</c>.
    /// </summary>
    private void HandleGameDbCommand(string arguments)
    {
        (string verb, string rest) = SplitHead(arguments.Trim());
        switch (verb.ToLowerInvariant())
        {
            case "":
                WriteVtank(DescribeGameInfo());
                WriteVtank(DescribeNextGameInfoCheck());
                return;
            case "update":
                StartGameInfoUpdate();
                return;
            case "interval":
                HandleGameDbIntervalCommand(rest.Trim());
                return;
            default:
                WriteVtank(GameDbSyntax);
                return;
        }
    }

    private void HandleGameDbIntervalCommand(string value)
    {
        if (value.Length != 0)
        {
            if (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out int hours)
                || hours < 0)
            {
                WriteVtank(
                    "Syntax: /vt gamedb interval [0-"
                    + MossTankProfileStore.MaximumGameDbCheckIntervalHours.ToString(CultureInfo.InvariantCulture)
                    + " hours]");
                return;
            }
            _profiles.SetGameDbCheckIntervalHours(hours);
        }
        WriteVtank(DescribeNextGameInfoCheck());
    }

    private TimeSpan GameInfoCheckInterval =>
        TimeSpan.FromHours(_profiles.GameDbCheckIntervalHours);

    /// <summary>When the next login will ask the service, and how that is decided.</summary>
    private string DescribeNextGameInfoCheck()
    {
        int hours = _profiles.GameDbCheckIntervalHours;
        if (hours == 0)
            return "Game database: checked at every login (/vt gamedb interval sets how often).";
        string every = "every " + hours.ToString(CultureInfo.InvariantCulture)
            + (hours == 1 ? " hour" : " hours");
        if (_gameInfo.LastUpdateTime is not { } seconds || seconds <= 0)
            return "Game database: checked " + every + "; the next login checks.";
        return "Game database: checked " + every + "; the next check is after "
            + VtankGameInfoUpdater.FormatUtc(
                VtankGameInfoUpdater.FromUnixSeconds(seconds) + GameInfoCheckInterval)
            + ".";
    }

    private void StartGameInfoUpdate()
    {
        WriteVtank(_gameInfoUpdater.Start() ?? "Checking the game database for updates.");
    }

    private string DescribeGameInfo()
    {
        bool fileExists = _host.VtankProfiles.IsAvailable
            && !string.IsNullOrWhiteSpace(
                _host.VtankProfiles.ReadText(VtankGameInfoDatabase.FileName));
        string source = fileExists
            ? VtankGameInfoDatabase.FileName + " in the VTank profile folder"
            : "no " + VtankGameInfoDatabase.FileName
              + " in the VTank profile folder; /vt gamedb update downloads it";
        string version = _gameInfo.Version is { } number
            ? number.ToString(CultureInfo.InvariantCulture)
            : "none";
        string updated = _gameInfo.LastUpdateTime is { } seconds && seconds > 0
            ? VtankGameInfoUpdater.FormatUtc(VtankGameInfoUpdater.FromUnixSeconds(seconds))
            : "never";
        string running = _gameInfoUpdater.IsRunning ? " An update is running." : string.Empty;
        return "Game database: " + source + ". Loaded: version " + version
            + ", last checked " + updated
            + ". Ammunition " + Count(_gameInfo.AmmunitionOptions.Count)
            + ", monster damage " + Count(_gameInfo.MonsterDamageOverrides.Count)
            + ", species damage " + Count(_gameInfo.SpeciesDamages.Count)
            + ", species members " + Count(_gameInfo.SpeciesMembers.Count)
            + ", immunities " + Count(_gameInfo.MonsterImmunities.Count)
            + ", heal kits " + Count(_gameInfo.HealKits.Count)
            + ", grenades " + Count(_gameInfo.GrenadeOptions.Count)
            + ", drain spells " + Count(_gameInfo.DrainSpellOptions.Count)
            + ", martyr spells " + Count(_gameInfo.MartyrSpellOptions.Count)
            + ", craft recipes " + Count(_gameInfo.Crafts.Recipes.Count)
            + "." + running;

        static string Count(int value) => value.ToString(CultureInfo.InvariantCulture);
    }
}
