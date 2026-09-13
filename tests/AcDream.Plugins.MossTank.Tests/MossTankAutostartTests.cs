using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MossTankAutostartTests
{
    [Fact]
    public void NoSessionSettingsDeclaredIsANoOp()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.TickAutostart();

        Assert.False(panel.CombatMacroRunning);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void NotInWorldIsANoOpEvenWithSettingsDeclared()
    {
        var automation = new FakeAutomation { IsAvailable = false };
        var settings = new Dictionary<string, string> { ["startMacro"] = "true" };
        var panel = new MossTankPanel(
            new FakeHost(automation, sessionSettings: settings));

        panel.TickAutostart();

        Assert.False(panel.CombatMacroRunning);
    }

    [Fact]
    public void StartMacroTrueStartsTheMacroTheSamePathAsVtStart()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var settings = new Dictionary<string, string> { ["startMacro"] = "true" };
        var panel = new MossTankPanel(
            new FakeHost(automation, sessionSettings: settings));

        panel.TickAutostart();

        Assert.True(panel.CombatMacroRunning);
    }

    [Fact]
    public void MissingNamedProfileLogsOneLineButKeepsGoingToStartTheMacro()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var settings = new Dictionary<string, string>
        {
            ["settingsProfile"] = "DoesNotExist",
            ["metaProfile"] = "DoesNotExistEither",
            ["navProfile"] = "NorThisOne",
            ["lootProfile"] = "NorThisOneEither",
            ["startMacro"] = "true",
        };
        var panel = new MossTankPanel(
            new FakeHost(automation, sessionSettings: settings));

        panel.TickAutostart();

        // "Errors -> one log line each, keep going": four failed selects,
        // and the macro still starts despite all four.
        Assert.Equal(4, automation.Logger.Errors.Count);
        Assert.Contains(
            automation.Logger.Errors,
            message => message.Contains("DoesNotExist", StringComparison.Ordinal));
        Assert.True(panel.CombatMacroRunning);
    }

    [Fact]
    public void RealNamedProfilesAreSelectedThroughTheSameStoreMethodsAsTheProfilesTab()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "settings save myprofile");
        Command(panel, "meta save myMeta");
        Command(panel, "nav save myNav");
        Command(panel, "loot new myLoot");

        var settings = new Dictionary<string, string>
        {
            ["settingsProfile"] = "myprofile",
            ["metaProfile"] = "myMeta",
            ["navProfile"] = "myNav",
            ["lootProfile"] = "myLoot",
        };
        host.SessionSettingsValue = settings;

        panel.TickAutostart();

        Assert.Empty(automation.Logger.Errors);
    }

    /// <summary>
    /// A session with no window in front of it must still leave a record of
    /// which profiles it came up with. The tabs' own notices are invisible
    /// there, so each selection says so in the log.
    /// </summary>
    [Fact]
    public void SelectingAProfileSaysSoInTheLogNotOnlyInTheTabsNotice()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "settings save myprofile");
        Command(panel, "meta save myMeta");
        Command(panel, "nav save myNav");
        Command(panel, "loot new myLoot");

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "myprofile",
            ["metaProfile"] = "myMeta",
            ["navProfile"] = "myNav",
            ["lootProfile"] = "myLoot",
        };
        automation.Logger.Infos.Clear();

        panel.TickAutostart();

        Assert.Empty(automation.Logger.Errors);
        foreach (string profile in new[] { "myprofile", "myMeta", "myNav", "myLoot" })
        {
            Assert.Contains(
                automation.Logger.Infos,
                message => message.Contains("oaded", StringComparison.Ordinal)
                    && message.Contains(profile, StringComparison.OrdinalIgnoreCase));
        }
    }

    [Fact]
    public void EnableMetaTrueEnablesTheEngineAndSelectsItsDeclaredProfile()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        Command(panel, "meta save myMeta");

        Assert.False(panel.MetaEnabled);

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["metaProfile"] = "myMeta",
            ["enableMeta"] = "true",
        };

        panel.TickAutostart();

        Assert.True(panel.MetaEnabled);
        Assert.Equal("myMeta", panel.SelectedMetaProfile);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void AbsentEnableMetaKeyLeavesEnableMetaAsWhateverTheSettingsProfileSays()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "settings save myprofile");
        Command(panel, "opt set enablemeta true");
        Assert.True(panel.MetaEnabled);
        panel.ToggleMeta();
        Assert.False(panel.MetaEnabled);

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "myprofile",
        };

        panel.TickAutostart();

        Assert.True(panel.MetaEnabled);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void AutostartAppliesExactlyOncePerGenerationAndReappliesAfterReconnect()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var settings = new Dictionary<string, string>
        {
            ["settingsProfile"] = "DoesNotExist",
        };
        var panel = new MossTankPanel(
            new FakeHost(automation, sessionSettings: settings));

        panel.TickAutostart();
        Assert.Single(automation.Logger.Errors);

        // Still in world, ticked again: no second application.
        panel.TickAutostart();
        Assert.Single(automation.Logger.Errors);

        // Reconnect: drop out of world, then re-enter — a fresh generation
        // re-applies exactly once more.
        automation.IsAvailable = false;
        panel.TickAutostart();
        Assert.Single(automation.Logger.Errors);

        automation.IsAvailable = true;
        panel.TickAutostart();
        Assert.Equal(2, automation.Logger.Errors.Count);
    }


    [Fact]
    public void AutostartSelectingASettingsProfileDoesNotDestroyItsSavedContent()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        panel.SetMetaOption("spelldiffexcessthreshold-hunt", ExpressionValue.Number(77));
        Command(panel, "settings save myprofile");
        string macroKey = panel.SelectedMacroProfile;
        string savedMyProfileText = host.Storage.ReadText(macroKey)!;
        Assert.False(string.IsNullOrEmpty(savedMyProfileText));

        Command(panel, "settings save otherprofile");
        Assert.NotEqual(macroKey, panel.SelectedMacroProfile);
        panel.SetMetaOption("spelldiffexcessthreshold-hunt", ExpressionValue.Number(13));

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "myprofile",
        };
        panel.TickAutostart();

        Assert.Equal(savedMyProfileText, host.Storage.ReadText(macroKey));
        Assert.Equal(macroKey, panel.SelectedMacroProfile);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void AutostartSelectingAMetaProfileDoesNotDestroyItsSavedContentOrRuleCount()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        panel.CreateNamedMetaProfile("myMeta");
        panel.SelectMetaAction("ExpressionAction");
        panel.SetMetaActionTextDraft("setvar['x',1]");
        panel.AddMetaRule();
        Assert.Single(panel.MetaRows);
        string metaKey = $"{VtankProfileDirectory.MetaFolder}/myMeta.af";
        string savedMyMetaText = host.Storage.ReadText(metaKey)!;
        Assert.False(string.IsNullOrEmpty(savedMyMetaText));

        panel.CreateNamedMetaProfile("otherMeta");
        Assert.Empty(panel.MetaRows);
        Assert.Equal("otherMeta", panel.SelectedMetaProfile);

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["metaProfile"] = "myMeta",
        };
        panel.TickAutostart();

        Assert.Equal(savedMyMetaText, host.Storage.ReadText(metaKey));
        Assert.Equal("myMeta", panel.SelectedMetaProfile);
        Assert.Single(panel.MetaRows);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void AutostartSelectingANavProfileDoesNotDestroyItsSavedContent()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "nav save otherNav");

        // "myNav" is the autostart TARGET: give it one real waypoint.
        Command(panel, "nav save myNav");
        panel.SetRoutePauseSecondsText("7");
        panel.AddRoutePause();
        string navKey = $"{VtankProfileDirectory.NavFolder}/myNav.af";
        string savedMyNavText = host.Storage.ReadText(navKey)!;
        Assert.False(string.IsNullOrEmpty(savedMyNavText));

        Command(panel, "nav load otherNav");
        Assert.Equal("otherNav", panel.SelectedRouteProfile);

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["navProfile"] = "myNav",
        };
        panel.TickAutostart();

        Assert.Equal(savedMyNavText, host.Storage.ReadText(navKey));
        Assert.Equal("myNav", panel.SelectedRouteProfile);
        Assert.Empty(automation.Logger.Errors);
    }

    [Fact]
    public void AutostartSelectingALootProfileDoesNotDestroyItsSavedContent()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "loot new myLoot");
        panel.AddLootRule();
        Assert.Single(panel.LootRuleRows);
        string lootKey = "myLoot.utl";
        string savedMyLootText = host.Storage.ReadText(lootKey)!;
        Assert.False(string.IsNullOrEmpty(savedMyLootText));

        Command(panel, "loot new otherLoot");
        Assert.Empty(panel.LootRuleRows);
        Assert.Equal("otherLoot", panel.LootProfileName);

        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["lootProfile"] = "myLoot",
        };
        panel.TickAutostart();

        Assert.Equal(savedMyLootText, host.Storage.ReadText(lootKey));
        Assert.Equal("myLoot", panel.LootProfileName);
        Assert.Single(panel.LootRuleRows);
        Assert.Empty(automation.Logger.Errors);
    }


    [Fact]
    public void SettingsProfileStoreExistsProbeDoesNotMutateSelectionOrStorage()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var store = new MossTankProfileStore(host);
        store.BindCharacter("TestChar");
        var allSettings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };
        Assert.True(store.Create(
            "myprofile", copyCurrent: true, allSettings, new HashSet<string>(),
            new HashSet<string>(), out _));
        Assert.True(store.Create(
            "otherprofile", copyCurrent: true, allSettings, new HashSet<string>(),
            new HashSet<string>(), out _));
        string selectedBefore = store.Selected;
        var storageBefore = new Dictionary<string, string>(
            ((MemoryStorage)host.Storage).Text, StringComparer.Ordinal);

        Assert.True(store.Exists("myprofile"));
        Assert.False(store.Exists("doesNotExist"));

        Assert.Equal(selectedBefore, store.Selected);
        Assert.Equal(storageBefore, ((MemoryStorage)host.Storage).Text);
    }

    /// <summary>
    /// "Mine only" is a picker filter: it decides which files the profiles
    /// tab lists, not which files exist. A settings profile dropped into the
    /// profile directory by hand — which is how a bot run, a shared build or
    /// a fresh install gets one — must still be selectable by name, exactly
    /// as the loot and route profiles already are.
    /// </summary>
    [Fact]
    public void ASettingsProfileFileIsSelectableByNameEvenWhileTheMineOnlyPickerHidesIt()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        host.VtankProfiles.WriteText(
            "vt-proof-settings.usd",
            VtankDefaultSettingsDatabase.Parse().Render());
        var store = new MossTankProfileStore(host);
        store.BindCharacter("TestChar");

        Assert.True(store.MineOnly);
        Assert.DoesNotContain("vt-proof-settings.usd", store.AvailableNames);

        Assert.True(store.Exists("vt-proof-settings"));
        Assert.True(store.Select("vt-proof-settings"));
        Assert.Equal("vt-proof-settings.usd", store.Selected);
    }

    /// <summary>
    /// The reserved "--" family still belongs to whichever character owns it.
    /// Resolving a bare name against the directory must not become a way
    /// around that.
    /// </summary>
    [Fact]
    public void ANamedLookupStillRefusesAnotherCharactersReservedProfileFile()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        host.VtankProfiles.WriteText(
            "--SomeoneElse_Coldeve.usd",
            VtankDefaultSettingsDatabase.Parse().Render());
        var store = new MossTankProfileStore(host);
        store.BindCharacter("TestChar");

        Assert.False(store.Exists("--SomeoneElse_Coldeve"));
        Assert.False(store.Select("--SomeoneElse_Coldeve"));
    }

    /// <summary>
    /// The whole autostart path, end to end: a named settings profile that
    /// only exists as a file is applied without an error line.
    /// </summary>
    [Fact]
    public void AutostartAppliesASettingsProfileThatOnlyExistsAsAFileInTheProfileDirectory()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        host.VtankProfiles.WriteText(
            "vt-proof-settings.usd",
            VtankDefaultSettingsDatabase.Parse().Render());
        var panel = new MossTankPanel(host);
        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "vt-proof-settings",
        };

        panel.TickAutostart();

        Assert.Empty(automation.Logger.Errors);
        Assert.Equal("vt-proof-settings.usd", panel.SelectedMacroProfile);
    }

    [Fact]
    public void MetaProfileStoreExistsProbeDoesNotMutateSelectionOrStorage()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var store = new MossTankMetaProfileStore(host);
        store.BindCharacter("TestChar");
        Assert.True(store.Create("myMeta", copyCurrent: false, new MetaProfile(), out _));
        Assert.True(store.Create("otherMeta", copyCurrent: false, new MetaProfile(), out _));
        string selectedBefore = store.Selected;
        var storageBefore = new Dictionary<string, string>(
            ((MemoryStorage)host.Storage).Text, StringComparer.Ordinal);

        Assert.True(store.Exists("myMeta"));
        Assert.False(store.Exists("doesNotExist"));

        Assert.Equal(selectedBefore, store.Selected);
        Assert.Equal(storageBefore, ((MemoryStorage)host.Storage).Text);
    }

    [Fact]
    public void RouteProfileStoreExistsProbeDoesNotMutateSelectionOrStorage()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var store = new MossTankRouteProfileStore(host);
        store.BindCharacter("TestChar");
        var navigationSettings = new NavigationSettings();
        Assert.True(store.Create("myNav", copyCurrent: true, navigationSettings, out _));
        Assert.True(store.Create("otherNav", copyCurrent: true, navigationSettings, out _));
        string selectedBefore = store.Selected;
        var storageBefore = new Dictionary<string, string>(
            ((MemoryStorage)host.Storage).Text, StringComparer.Ordinal);

        Assert.True(store.Exists("myNav"));
        Assert.False(store.Exists("doesNotExist"));

        Assert.Equal(selectedBefore, store.Selected);
        Assert.Equal(storageBefore, ((MemoryStorage)host.Storage).Text);
    }

    [Fact]
    public void LootProfileStoreExistsProbeDoesNotMutateSelectionOrStorage()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var store = new MossTankLootProfileStore(host);
        store.BindCharacter("TestChar");
        Assert.True(store.Create("myLoot", copyCurrent: false, Array.Empty<LootRule>(), out _));
        Assert.True(store.Create("otherLoot", copyCurrent: false, Array.Empty<LootRule>(), out _));
        string selectedBefore = store.Selected;
        var storageBefore = new Dictionary<string, string>(
            ((MemoryStorage)host.Storage).Text, StringComparer.Ordinal);

        Assert.True(store.Exists("myLoot"));
        Assert.False(store.Exists("doesNotExist"));

        Assert.Equal(selectedBefore, store.Selected);
        Assert.Equal(storageBefore, ((MemoryStorage)host.Storage).Text);
    }

    /// <summary>
    /// The whole autostart path against profiles nobody in this process
    /// wrote: the very files the automation-session proof feeds a live bot,
    /// read straight off disk. Every offline autostart case before this one
    /// SAVED a profile from the running panel first, so "an externally
    /// authored .usd / .utl / .af applies" was never pinned — and when the
    /// live proof reported that no rule was ever valid, nothing here could
    /// say whether the fixture had arrived at all.
    /// </summary>
    [Fact]
    public void AutostartAppliesTheSessionProofsOwnAuthoredProfilesWithNoSidecar()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        WriteSessionProofFixture(host.VtankProfiles);
        var panel = new MossTankPanel(host);
        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "vt-proof-settings",
            ["lootProfile"] = "vt-proof-loot",
            ["navProfile"] = "vt-proof-route",
            ["enableMeta"] = "false",
            ["startMacro"] = "true",
        };

        panel.TickAutostart();

        Assert.Empty(automation.Logger.Errors);
        Assert.Equal("vt-proof-settings.usd", panel.SelectedMacroProfile);
        Assert.Equal("vt-proof-loot", panel.LootProfileName);
        Assert.Equal("vt-proof-route", panel.SelectedRouteProfile);

        // The four switches the proof's red rows all depend on.
        Assert.True(panel.GetMetaOptionForTest("enablebuffing"), "EnableBuffing");
        Assert.True(panel.GetMetaOptionForTest("enablecombat"), "EnableCombat");
        Assert.True(panel.GetMetaOptionForTest("enablenav"), "EnableNav");
        Assert.True(panel.GetMetaOptionForTest("enablelooting"), "EnableLooting");

        // The route really reached the live navigation settings.
        Assert.Equal(4, panel.RouteRows.Count);
        Assert.Equal("Circular", panel.SelectedRouteMode);
        Assert.True(panel.CombatMacroRunning);
    }

    /// <summary>
    /// A route profile that is on disk but will not parse must keep its file
    /// and must not be reported as loaded. The loader used to answer the
    /// failure by writing whatever route happened to be in memory back over
    /// the author's file and then announcing "Loaded route ..." — so a
    /// destroyed route and a good one looked identical from outside.
    /// </summary>
    [Fact]
    public void ARouteProfileThatWillNotParseKeepsItsFileAndIsNotReportedLoaded()
    {
        const string unreadable = "~~ {\nNAV: broken 4\n";
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        host.VtankProfiles.WriteText("navs/vt-broken-route.af", unreadable);
        var panel = new MossTankPanel(host);

        // A route the panel already holds, so a silent overwrite would have
        // something of its own to write.
        WriteSessionProofFixture(host.VtankProfiles);
        panel.SelectRouteProfile("vt-proof-route");
        Assert.Equal(4, panel.RouteRows.Count);
        automation.Logger.Infos.Clear();

        panel.SelectRouteProfile("vt-broken-route");

        Assert.Equal(unreadable, host.VtankProfiles.ReadText("navs/vt-broken-route.af"));
        Assert.DoesNotContain(
            automation.Logger.Infos,
            message => message.Contains("Loaded route profile", StringComparison.Ordinal)
                || (message.Contains("vt-broken-route", StringComparison.Ordinal)
                    && message.Contains("oaded", StringComparison.Ordinal)));
        Assert.Contains(
            automation.Logger.Errors,
            message => message.Contains("vt-broken-route", StringComparison.Ordinal));
        Assert.Contains("could not be read", panel.RouteNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// A session with no window in front of it cannot type /vt log in time.
    /// Several of the plugin's most useful lines are emitted once per run —
    /// the buff planner's refusal among them — so a channel switched on
    /// after the macro has started has already missed them, and the run's
    /// record is silent about the very thing it was meant to explain.
    /// Autostart therefore opens the declared channels before it starts the
    /// macro, and any casing names the same channel the emitter uses.
    /// </summary>
    [Fact]
    public void AutostartOpensTheDeclaredLogChannelsBeforeItStartsTheMacro()
    {
        var automation = new FakeAutomation { IsAvailable = true };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["logChannels"] = "misc, RuleInfo activerule",
            ["startMacro"] = "true",
        };

        panel.TickAutostart();
        panel.OnTick(1d);

        Assert.Empty(automation.Logger.Errors);
        Assert.True(panel.CombatMacroRunning);
        // The very first scheduler pass is already on the record. Turning the
        // channel on afterwards, which is all a session could do from
        // outside, would have lost it.
        Assert.Contains(
            automation.Logger.Infos,
            message => message.Contains(
                "[vt log ActiveRule] ----------- Primary logic loop started",
                StringComparison.Ordinal));

        Command(panel, "log");
        Assert.Contains(
            ((FakeAutomation)host.Automation).Messages,
            message => message.Contains("ActiveRule", StringComparison.Ordinal)
                && message.Contains("Misc", StringComparison.Ordinal)
                && message.Contains("RuleInfo", StringComparison.Ordinal));
    }

    /// <summary>
    /// The automation surface can report itself available before the
    /// character's name arrives — a live session does exactly that. Every
    /// profile autostart applies belongs to a character, and binding a
    /// character re-reads which profile is selected from that character's
    /// own binding file, so a profile set applied before the name landed was
    /// thrown away on the tick it did land: the declared settings, the loot
    /// rules, the route and the open log channels all reverted to the
    /// character's own defaults, and nothing said so. A bot then ran a whole
    /// session on the wrong profile.
    /// </summary>
    [Fact]
    public void AutostartWaitsForTheCharactersNameAndItsChoiceThenSurvivesTheBind()
    {
        var automation = new FakeAutomation { IsAvailable = true, Name = string.Empty };
        var host = new FakeHost(automation);
        WriteSessionProofFixture(host.VtankProfiles);
        var panel = new MossTankPanel(host);
        host.SessionSettingsValue = new Dictionary<string, string>
        {
            ["settingsProfile"] = "vt-proof-settings",
            ["navProfile"] = "vt-proof-route",
            ["logChannels"] = "RuleInfo",
        };

        // In world, but the character has not been named yet.
        panel.TickAutostart();
        Assert.NotEqual("vt-proof-settings.usd", panel.SelectedMacroProfile);

        automation.Name = "TestChar";
        panel.TickAutostart();
        Assert.Equal("vt-proof-settings.usd", panel.SelectedMacroProfile);
        Assert.Equal(4, panel.RouteRows.Count);

        // The tick that binds the character must not undo any of it.
        panel.OnTick(1d);
        panel.OnTick(1d);

        Assert.Equal("vt-proof-settings.usd", panel.SelectedMacroProfile);
        Assert.Equal("vt-proof-route", panel.SelectedRouteProfile);
        Assert.Equal(4, panel.RouteRows.Count);
        Assert.True(panel.GetMetaOptionForTest("enablenav"), "EnableNav");
        Command(panel, "log");
        Assert.Contains(
            ((FakeAutomation)host.Automation).Messages,
            message => message.Contains("Log state", StringComparison.Ordinal)
                && message.Contains("RuleInfo", StringComparison.Ordinal));
        Assert.Empty(automation.Logger.Errors);
    }

    /// <summary>
    /// The proof's fixture folder, copied into the fake profile storage. One
    /// copy of these files exists in the tree — the live proof reads the same
    /// three — so this pin and that run cannot drift apart.
    /// </summary>
    private static void WriteSessionProofFixture(IPluginStorage storage)
    {
        string root = Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vt-proof");
        storage.WriteText(
            "vt-proof-settings.usd",
            File.ReadAllText(Path.Combine(root, "vt-proof-settings.usd")));
        storage.WriteText(
            "vt-proof-loot.utl",
            File.ReadAllText(Path.Combine(root, "vt-proof-loot.utl")));
        storage.WriteText(
            "navs/vt-proof-route.af",
            File.ReadAllText(Path.Combine(root, "navs", "vt-proof-route.af")));
    }

    private static void Command(MossTankPanel panel, string arguments) =>
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", arguments, "/vt " + arguments));

    private sealed class FakeHost(
        FakeAutomation automation,
        IPluginStorage? storage = null,
        IReadOnlyDictionary<string, string>? sessionSettings = null)
        : IPluginHost
    {
        private readonly IPluginStorage _storage = storage ?? new MemoryStorage();

        internal IReadOnlyDictionary<string, string>? SessionSettingsValue { get; set; } =
            sessionSettings;

        public bool HasUi => false;
        public IPluginLogger Log => automation.Logger;
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation { get; } = automation;
        public IPluginStorage Storage => _storage;
        public IPluginStorage VtankProfiles => _storage;
        public IReadOnlyDictionary<string, string> SessionSettings =>
            SessionSettingsValue ?? new Dictionary<string, string>();
    }

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, ISpellCatalog, IMagicCommands, IPluginChat
    {
        internal FakeLogger Logger { get; } = new();

        public bool IsAvailable { get; set; }
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;

        public bool IsInWorld => IsAvailable;
        public string Name { get; set; } = "TestChar";
        public uint ObjectId => 1;
        public uint CurrentHealth => 0;
        public uint MaxHealth => 0;
        public uint CurrentStamina => 0;
        public uint MaxStamina => 0;
        public uint CurrentMana => 0;
        public uint MaxMana => 0;
        public IReadOnlyList<PluginSkillInfo> Skills { get; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments { get; } = [];
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs { get; } = [];
        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            info = default;
            return false;
        }

        public bool IsCasting => false;
        public PluginCastGate EvaluateGate(uint spellId) => PluginCastGate.Unavailable;
        public bool Cast(uint spellId) => false;

        public List<string> Messages { get; } = [];
        public void PostSystemMessage(string text) => Messages.Add(text);
    }

    private sealed class FakeLogger : IPluginLogger
    {
        public List<string> Errors { get; } = [];
        public List<string> Infos { get; } = [];
        public void Info(string message) => Infos.Add(message);
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) =>
            Errors.Add(message);
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class FakeState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class FakeEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }

        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }

    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId { get; private set; }
        public uint? PreviousObjectId { get; private set; }

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId)
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = objectId;
            return true;
        }

        public bool Clear()
        {
            if (SelectedObjectId is null)
                return false;
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = null;
            return true;
        }
    }
}
