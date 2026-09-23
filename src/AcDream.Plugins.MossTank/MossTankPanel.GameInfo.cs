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
    ];

    internal IReadOnlyDictionary<string, VtankHealKit> HealKitsForTest =>
        _combatSettings.HealKits;

    internal Task<VtankGameInfoUpdateResult>? GameInfoUpdatePendingForTest =>
        _gameInfoUpdater.PendingForTest;

    /// <summary>
    /// Once a session, as soon as it is in the world, the game database asks
    /// its service for what changed -- the reference client's own check at
    /// login. The request runs off the game thread; what it says and the
    /// database it brings back arrive here, on the tick.
    /// </summary>
    private void TickGameInfoUpdate()
    {
        if (!_gameInfoCheckedThisSession)
        {
            _gameInfoCheckedThisSession = true;
            // A session with nothing to check with has nothing to say; the
            // command says why when it is asked for.
            if (_gameInfoUpdater.CanUpdate)
                _gameInfoUpdater.Start();
        }
        _gameInfoUpdater.Drain(WriteVtank, ApplyGameInfo);
    }

    /// <summary>
    /// The one place a newer game database is handed over. Every reader is
    /// given the new one here; none of them keeps a table of its own.
    /// </summary>
    private void ApplyGameInfo(VtankGameInfoDatabase gameInfo)
    {
        _gameInfo = gameInfo;
        _combatSettings.MonsterFacts.Replace(gameInfo);
        _combatSettings.HealKits = HealKitTable(gameInfo);
        _combat.ReplaceGameInfo(gameInfo);
    }

    private static Dictionary<string, VtankHealKit> HealKitTable(VtankGameInfoDatabase gameInfo)
    {
        var healKits = new Dictionary<string, VtankHealKit>(StringComparer.OrdinalIgnoreCase);
        foreach (VtankHealKit kit in gameInfo.HealKits)
            healKits[kit.Name] = kit;
        return healKits;
    }

    /// <summary><c>/vt gamedb [update]</c>, and the reference's own <c>/vt getdb</c>.</summary>
    private void HandleGameDbCommand(string arguments)
    {
        string value = arguments.Trim();
        if (value.Length == 0)
        {
            WriteVtank(DescribeGameInfo());
            return;
        }
        if (!value.Equals("update", StringComparison.OrdinalIgnoreCase))
        {
            WriteVtank("Syntax: /vt gamedb [update]");
            return;
        }
        StartGameInfoUpdate();
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
            ? VtankGameInfoUpdater.FromUnixSeconds(seconds)
                .ToString("yyyy-MM-dd HH:mm 'UTC'", CultureInfo.InvariantCulture)
            : "never";
        string running = _gameInfoUpdater.IsRunning ? " An update is running." : string.Empty;
        return "Game database: " + source + ". Loaded: version " + version
            + ", last updated " + updated
            + ". Ammunition " + Count(_gameInfo.AmmunitionOptions.Count)
            + ", monster damage " + Count(_gameInfo.MonsterDamageOverrides.Count)
            + ", species damage " + Count(_gameInfo.SpeciesDamages.Count)
            + ", species members " + Count(_gameInfo.SpeciesMembers.Count)
            + ", immunities " + Count(_gameInfo.MonsterImmunities.Count)
            + ", heal kits " + Count(_gameInfo.HealKits.Count)
            + ", drain spells " + Count(_gameInfo.DrainSpellOptions.Count)
            + ", martyr spells " + Count(_gameInfo.MartyrSpellOptions.Count)
            + "." + running;

        static string Count(int value) => value.ToString(CultureInfo.InvariantCulture);
    }
}
