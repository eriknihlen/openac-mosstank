using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Which file a route or meta name reaches, and which files a follow
/// writes.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const string OnePointRoute =
        "NAV: nav0 circular ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n~~ }\r\n";

    private const string OneRuleMeta =
        "STATE: {Default} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n";

    private static string DroppedRoute() => File.ReadAllText(Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "nav_ab.nav"));

    private static string DroppedMeta() => File.ReadAllText(Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank", "met", "bella.met"));

    /// <summary>
    /// A route name typed with its extension loads that file even when a
    /// route of the other form shares its name, and a bare name loads the
    /// dropped .nav first, so the .nav goes by "foo". Where both sit side by
    /// side the .af goes by its full name, which loads it again. Mutation:
    /// the plugin's own format first loads the .af for "foo"; leaving the
    /// .af's extension off names it "foo", which loads the .nav.
    /// </summary>
    [Fact]
    public void ANavLoadByBareNameLoadsTheNavFirstAndAnExtensionLoadsThatFile()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/foo.nav"] = DroppedRoute();
        storage.Text["mosstank/navs/foo.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));

        Command(panel, "nav load foo");
        Assert.Equal("foo", panel.SelectedRouteProfile);
        Assert.True(panel.RouteRows.Count > 1);

        Command(panel, "nav load foo.af");
        Assert.Equal("foo.af", panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);
        Assert.Contains("foo.af", panel.RouteProfileNames);
        Assert.Contains("foo", panel.RouteProfileNames);
        Assert.DoesNotContain("foo.nav", panel.RouteProfileNames);

        Command(panel, "nav load foo.nav");
        Assert.True(panel.RouteRows.Count > 1);
        Command(panel, "nav load " + Assert.Single(
            panel.RouteProfileNames,
            static name => name.EndsWith(".af", StringComparison.Ordinal)));
        Assert.Single(panel.RouteRows);
    }

    /// <summary>
    /// A bare name with only the plugin's own file there loads that file, a
    /// named file that is missing falls back to the other form, and a lone
    /// .af still goes by its bare name. Mutation: dropping the fallback
    /// leaves "foo.nav" unavailable.
    /// </summary>
    [Fact]
    public void ANavLoadFallsBackToTheFormThatIsThere()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/foo.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));

        Command(panel, "nav load foo");
        Assert.Equal("foo", panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);

        Command(panel, "nav load foo.nav");
        Assert.Equal("foo", panel.SelectedRouteProfile);
        Assert.Contains("foo", panel.RouteProfileNames);
    }

    /// <summary>
    /// Editing a route loaded from a .nav saves it back into that .nav, in
    /// the .nav form, and writes nothing beside it; loading the name again
    /// loads the edit. Mutation: saving beside the .nav writes foo.af and
    /// leaves the edit out of the .nav.
    /// </summary>
    [Fact]
    public void EditingARouteLoadedFromANavSavesItBackIntoTheNav()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/foo.nav"] = DroppedRoute();
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "nav load foo");
        int points = panel.RouteRows.Count;

        panel.RemoveRouteWaypoint();

        Assert.False(storage.Text.ContainsKey("mosstank/navs/foo.af"));
        Assert.StartsWith("uTank2 NAV 1.2\r\n", storage.Text["mosstank/navs/foo.nav"], StringComparison.Ordinal);
        Assert.Equal("foo", panel.SelectedRouteProfile);

        Command(panel, "nav load foo");
        Assert.Equal("foo", panel.SelectedRouteProfile);
        Assert.Equal(points - 1, panel.RouteRows.Count);
    }

    /// <summary>
    /// Switching away from a route loaded from a .nav saves it back where it
    /// came from, byte for byte when nothing changed, and never over the .af
    /// of the same name beside it. Mutation: saving beside the .nav writes
    /// the .nav's route over foo.af.
    /// </summary>
    [Fact]
    public void SwitchingFromANavRouteLeavesTheAfBesideItAlone()
    {
        var storage = new MemoryStorage();
        string original = DroppedRoute();
        storage.Text["mosstank/navs/foo.nav"] = original;
        storage.Text["mosstank/navs/foo.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "nav load foo");

        panel.SelectRouteProfile("foo.af");

        Assert.Single(panel.RouteRows);
        Assert.Equal(original, storage.Text["mosstank/navs/foo.nav"]);
        Assert.Equal(OnePointRoute, storage.Text["mosstank/navs/foo.af"]);
    }

    /// <summary>
    /// A route file written with bare line feeds keeps them when it is saved
    /// back, so saving it unchanged leaves it byte for byte. Mutation:
    /// writing the form's usual line breaks over it rewrites every line.
    /// </summary>
    [Fact]
    public void ANavWithBareLineFeedsSavesBackWithThem()
    {
        string original = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "emp_lower_north.nav"));
        Assert.DoesNotContain("\r", original, StringComparison.Ordinal);
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/lf.nav"] = original;
        var store = new MossTankRouteProfileStore(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        store.BindCharacter("Barris");
        Assert.True(store.Select("lf"));
        var route = new NavigationSettings();
        Assert.Equal(
            MossTankProfileLoad.Loaded,
            store.LoadCurrent(route, MetafSerializer.NoOpSpells.Instance));

        Assert.True(store.SaveCurrent(route));

        Assert.Equal(original, storage.Text["mosstank/navs/lf.nav"]);
    }

    /// <summary>
    /// A jump backward cannot be written into a .nav, so adding one to a
    /// route loaded from a .nav says so at once and leaves the file as it
    /// was, rather than write the jump as forward. Mutation: writing it
    /// forward saves a route that jumps the wrong way.
    /// </summary>
    [Fact]
    public void ABackwardJumpAddedToANavRouteIsRefusedAndSaidSo()
    {
        var storage = new MemoryStorage();
        string original = DroppedRoute();
        storage.Text["mosstank/navs/foo.nav"] = original;
        var automation = new FakeAutomation
        {
            Name = "Barris",
            WorldName = "Coldeve",
            NavigationSnapshot = NavigationAt(90f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "nav load foo");

        Command(panel, "addnavjump 90 false 100 backward");

        Assert.Equal(original, storage.Text["mosstank/navs/foo.nav"]);
        Assert.Contains(
            automation.Messages,
            static message => message.Contains("NOT saved", StringComparison.Ordinal)
                && message.Contains("backward", StringComparison.Ordinal));
        Assert.Contains("NOT saved", panel.RouteNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// Editing a meta loaded from a .met saves it back into that .met, in
    /// the .met form, and writes nothing beside it. Mutation: saving beside
    /// the .met writes foo.af and leaves the edit out of the .met.
    /// </summary>
    [Fact]
    public void EditingAMetaLoadedFromAMetSavesItBackIntoTheMet()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/foo.met"] = DroppedMeta();
        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        Assert.True(store.Select("foo"));
        MetaProfile loaded = store.LoadCurrent();
        int rules = loaded.Rules.Count;

        loaded.Rules.Add(new MetaRule
        {
            State = "Added",
            Condition = MetaCondition.Always(),
            Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say added" },
        });
        Assert.True(store.SaveCurrent(loaded));

        Assert.False(storage.Text.ContainsKey("mosstank/metas/foo.af"));
        Assert.StartsWith("1\r\nCondAct\r\n", storage.Text["mosstank/metas/foo.met"], StringComparison.Ordinal);
        MetaProfile reloaded = store.LoadCurrent();
        Assert.Equal(rules + 1, reloaded.Rules.Count);
        Assert.Equal("/say added", reloaded.Rules[^1].Action.Text);
    }

    /// <summary>
    /// A meta the .met form cannot hold is not saved over its .met: the
    /// store says why and the file stays as it was. Mutation: dropping the
    /// switched-off rule saves a meta that silently lost it.
    /// </summary>
    [Fact]
    public void AMetaAMetCannotHoldIsNotSavedOverTheMet()
    {
        var storage = new MemoryStorage();
        string original = DroppedMeta();
        storage.Text["mosstank/metas/foo.met"] = original;
        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        Assert.True(store.Select("foo"));
        MetaProfile loaded = store.LoadCurrent();

        loaded.Rules[0].Enabled = false;

        Assert.False(store.SaveCurrent(loaded));
        Assert.Contains("switched off", store.SaveNotice, StringComparison.Ordinal);
        Assert.Equal(original, storage.Text["mosstank/metas/foo.met"]);
    }

    /// <summary>
    /// The same rule for metas: a bare name loads the .met first, ".met"
    /// loads the .met and ".af" the .af, which goes by its full name while
    /// the .met sits beside it. Mutation: the plugin's own format first
    /// loads the .af for "foo".
    /// </summary>
    [Fact]
    public void AMetaLoadByBareNameLoadsTheMetFirstAndAnExtensionLoadsThatFile()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/foo.met"] = DroppedMeta();
        storage.Text["mosstank/metas/foo.af"] = OneRuleMeta;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));

        Command(panel, "meta load foo");
        Assert.Equal("foo", panel.SelectedMetaProfile);
        Assert.True(panel.MetaRows.Count > 1);

        Command(panel, "meta load foo.af");
        Assert.Equal("foo.af", panel.SelectedMetaProfile);
        Assert.Single(panel.MetaRows);
        Assert.Contains("foo.af", panel.MetaProfileNames);
        Assert.Contains("foo", panel.MetaProfileNames);
        Assert.DoesNotContain("foo.met", panel.MetaProfileNames);

        Command(panel, "meta load foo.met");
        Assert.True(panel.MetaRows.Count > 1);
        panel.SelectMetaProfile("foo.af");
        Assert.Single(panel.MetaRows);
    }

    /// <summary>
    /// A route translated by its full name reads that file, not the other
    /// form beside it. Mutation: dropping the extension translates the .af.
    /// </summary>
    [Fact]
    public void TranslateRouteReadsTheFileItsExtensionNames()
    {
        var vtank = new MemoryStorage();
        vtank.Text["mosstank/navs/eo-east.nav"] = DroppedRoute();
        vtank.Text["mosstank/navs/eo-east.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation(), new MemoryStorage(), vtankProfiles: vtank));

        UbCommand(panel, "translateroute 0x00640371 eo-east.af 0x01640371 eo-main");

        string saved = vtank.Text["mosstank/navs/eo-main.nav"];
        Assert.DoesNotContain("/ah", saved, StringComparison.Ordinal);
        Assert.Contains("26.1", saved, StringComparison.Ordinal);
    }

    /// <summary>
    /// A route translated by its bare name reads the .nav first. Mutation:
    /// the plugin's own format first translates the .af.
    /// </summary>
    [Fact]
    public void TranslateRouteByBareNameReadsTheNavFirst()
    {
        var vtank = new MemoryStorage();
        vtank.Text["mosstank/navs/eo-east.nav"] = DroppedRoute();
        vtank.Text["mosstank/navs/eo-east.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation(), new MemoryStorage(), vtankProfiles: vtank));

        UbCommand(panel, "translateroute 0x00640371 eo-east 0x01640371 eo-main");

        string saved = vtank.Text["mosstank/navs/eo-main.nav"];
        Assert.Contains("/ah", saved, StringComparison.Ordinal);
        Assert.DoesNotContain("26.1", saved, StringComparison.Ordinal);
    }

    // ── what a save by name writes ───────────────────────────────────────

    /// <summary>
    /// A route or meta saved by a bare name is written in the older form, as
    /// the reference writes it, and goes by that bare name. Mutation: the
    /// plugin's own form for a bare name writes Foo.af.
    /// </summary>
    [Fact]
    public void SavingByABareNameWritesTheNavAndTheMetForm()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/start.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "nav load start");

        Command(panel, "nav save Foo");
        Command(panel, "meta save Foo");

        Assert.StartsWith("uTank2 NAV 1.2\r\n", storage.Text["mosstank/navs/Foo.nav"], StringComparison.Ordinal);
        Assert.StartsWith("1\r\nCondAct\r\n", storage.Text["mosstank/metas/Foo.met"], StringComparison.Ordinal);
        Assert.False(storage.Text.ContainsKey("mosstank/navs/Foo.af"));
        Assert.False(storage.Text.ContainsKey("mosstank/metas/Foo.af"));
        Assert.Equal("Foo", panel.SelectedRouteProfile);
        Assert.Equal("Foo", panel.SelectedMetaProfile);
    }

    /// <summary>
    /// The window's New buttons follow the same default: a bare name is a
    /// .nav or a .met, and a name the player gives ".af" is an .af.
    /// Mutation: the plugin's own form for a bare name writes Bar.af.
    /// </summary>
    [Fact]
    public void TheWindowsNewButtonsWriteTheNavAndMetFormUnlessTheNameSaysAf()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));

        panel.CreateNamedRouteProfile("Bar");
        panel.CreateNamedMetaProfile("Bar");
        panel.CreateNamedRouteProfile("Baz.af");
        panel.CreateNamedMetaProfile("Baz.af");

        Assert.True(storage.Text.ContainsKey("mosstank/navs/Bar.nav"));
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Bar.met"));
        Assert.True(storage.Text.ContainsKey("mosstank/navs/Baz.af"));
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Baz.af"));
    }

    /// <summary>
    /// /vt navaf and /vt metaaf save the plugin's own .af, whatever the name
    /// carries, and load exactly the .af, never the .nav or .met of the same
    /// name. Mutation: routing them through the plain commands saves a .nav
    /// and a .met, and loads the .nav.
    /// </summary>
    [Fact]
    public void TheAfCommandsSaveAndLoadTheAfOnly()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/foo.nav"] = DroppedRoute();
        storage.Text["mosstank/metas/foo.met"] = DroppedMeta();
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "nav load foo");
        Command(panel, "meta load foo");
        int points = panel.RouteRows.Count;

        Command(panel, "navaf save foo.nav");
        Command(panel, "metaaf save foo");

        Assert.Contains("NAV: ", storage.Text["mosstank/navs/foo.af"], StringComparison.Ordinal);
        Assert.Contains("STATE: ", storage.Text["mosstank/metas/foo.af"], StringComparison.Ordinal);
        Assert.Equal("foo.af", panel.SelectedRouteProfile);
        Assert.Equal("foo.af", panel.SelectedMetaProfile);

        storage.Text["mosstank/navs/foo.af"] = OnePointRoute;
        Command(panel, "nav load foo");
        Assert.Equal(points, panel.RouteRows.Count);
        Command(panel, "navaf load foo");
        Assert.Equal("foo.af", panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);

        automation.Messages.Clear();
        storage.Text["mosstank/navs/elsewhere.nav"] = DroppedRoute();
        storage.Text["mosstank/metas/elsewhere.met"] = DroppedMeta();
        Command(panel, "navaf load elsewhere");
        Command(panel, "metaaf load elsewhere");
        Assert.Contains("Navigation profile elsewhere.af was not found.", automation.Messages);
        Assert.Contains("Meta profile elsewhere.af was not found.", automation.Messages);
        Assert.Equal("foo.af", panel.SelectedRouteProfile);
        Assert.False(storage.Text.ContainsKey("mosstank/metas/elsewhere.af"));
    }

    /// <summary>
    /// The Nav and Meta tabs' "Copy to .af" buttons put the .af save command
    /// in the chat entry for the player to name, and send nothing. Mutation:
    /// composing the plain save command writes a .nav or a .met instead.
    /// </summary>
    [Fact]
    public void TheCopyToAfButtonsStartTheAfSaveInChat()
    {
        var automation = new FakeAutomation { AcceptsCompose = true };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        panel.CopyRouteProfileToAf();
        panel.CopyMetaProfileToAf();

        Assert.Equal(["/vt navaf save ", "/vt metaaf save "], automation.Composed);
        Assert.DoesNotContain(automation.Submitted, static line => line.Contains("save", StringComparison.Ordinal));
    }

    /// <summary>
    /// The help lists the .af commands and prints their usage. Mutation:
    /// leaving them out of the help leaves "/vt help navaf" unanswered.
    /// </summary>
    [Fact]
    public void TheHelpDocumentsTheAfCommands()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "help navaf");
        Command(panel, "help metaaf");

        Assert.Contains("Syntax: /vt navaf [save/load] [filename]", automation.Messages);
        Assert.Contains("Syntax: /vt metaaf [save/load] [filename]", automation.Messages);
    }

    // ── follow never writes the loaded route ────────────────────────────

    private const string FollowRouteKey = "mosstank/navs/UBFollow.af";

    private static (MossTankPanel Panel, FakeHost Host, MemoryStorage Storage) PanelWithRoute(
        string key,
        string text)
    {
        var storage = new MemoryStorage();
        storage.Text[key] = text;
        var automation = new FakeAutomation
        {
            Name = "Barris",
            WorldName = "Coldeve",
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Player(0x50000ABCu, "Zero Cool")],
        };
        automation.NavigationObjects.Add(new PluginNavigationObject(
            0x50000ABCu, "Zero Cool", default));
        var host = new FakeHost(automation, storage);
        return (new MossTankPanel(host), host, storage);
    }

    private static Dictionary<string, string> RouteFiles(MemoryStorage storage) =>
        storage.Text
            .Where(static pair => pair.Key.StartsWith("mosstank/navs/", StringComparison.Ordinal))
            .ToDictionary(static pair => pair.Key, static pair => pair.Value);

    /// <summary>
    /// "/ub follow" moves to the follow route and aims that; the route that
    /// was loaded keeps its file byte for byte, and no other route file is
    /// written. Mutation: aiming the loaded route writes the follow over
    /// jaw_1.af and wipes its waypoint.
    /// </summary>
    [Fact]
    public void UbFollowAimsTheFollowRouteAndLeavesTheLoadedRouteFileAlone()
    {
        (MossTankPanel panel, _, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw_1.af", OnePointRoute);
        Command(panel, "nav load jaw_1");
        Dictionary<string, string> before = RouteFiles(storage);

        UbCommand(panel, "follow Zero Cool");

        Assert.Equal(OnePointRoute, storage.Text["mosstank/navs/jaw_1.af"]);
        Assert.Equal("UBFollow", panel.SelectedRouteProfile);
        Assert.Equal("Follow", panel.SelectedRouteMode);
        Assert.Contains("flw 50000ABC", storage.Text[FollowRouteKey], StringComparison.Ordinal);
        Dictionary<string, string> after = RouteFiles(storage);
        after.Remove(FollowRouteKey);
        Assert.Equal(before, after);

        // Back on the route, it is the route it was.
        Command(panel, "nav load jaw_1");
        Assert.Single(panel.RouteRows);
        Assert.Equal("Circular", panel.SelectedRouteMode);
    }

    /// <summary>
    /// A route loaded from a dropped .nav gets no .af written beside it by a
    /// follow. Mutation: saving the loaded route on the way to the follow
    /// route writes jaw.af.
    /// </summary>
    [Fact]
    public void UbFollowWritesNothingBesideARouteLoadedFromANav()
    {
        string original = DroppedRoute();
        (MossTankPanel panel, _, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw.nav", original);
        Command(panel, "nav load jaw.nav");
        Dictionary<string, string> before = RouteFiles(storage);

        UbCommand(panel, "follow Zero Cool");

        Dictionary<string, string> after = RouteFiles(storage);
        Assert.True(after.Remove(FollowRouteKey));
        Assert.Equal(before, after);
        Assert.False(storage.Text.ContainsKey("mosstank/navs/jaw.af"));
        Assert.Equal(original, storage.Text["mosstank/navs/jaw.nav"]);
    }

    /// <summary>
    /// The window's Follow button and the mode menu's Follow do the same as
    /// the command. Mutation: aiming the loaded route rewrites jaw_1.af.
    /// </summary>
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void TheWindowsFollowLeavesTheLoadedRouteFileAlone(bool fromModeMenu)
    {
        (MossTankPanel panel, FakeHost host, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw_1.af", OnePointRoute);
        Command(panel, "nav load jaw_1");
        host.Selection.Select(0x50000ABCu);

        if (fromModeMenu)
            panel.SelectRouteMode("Follow");
        else
            panel.SetFollowTarget();

        Assert.Equal(OnePointRoute, storage.Text["mosstank/navs/jaw_1.af"]);
        Assert.Equal("UBFollow", panel.SelectedRouteProfile);
        Assert.Equal("Follow target: Zero Cool", panel.RouteFollowTargetText);
        Assert.Contains("Zero Cool", storage.Text[FollowRouteKey], StringComparison.Ordinal);
    }

    /// <summary>
    /// Picking Follow in the mode menu with nothing selected changes nothing,
    /// on screen or on disk. Mutation: setting the mode first writes a
    /// target-less follow over jaw_1.af.
    /// </summary>
    [Fact]
    public void TheModeMenusFollowWithNothingSelectedChangesNothing()
    {
        (MossTankPanel panel, _, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw_1.af", OnePointRoute);
        Command(panel, "nav load jaw_1");

        panel.SelectRouteMode("Follow");

        Assert.Equal("Select a live object to follow first.", panel.RouteNotice);
        Assert.Equal("Circular", panel.SelectedRouteMode);
        Assert.Equal("jaw_1", panel.SelectedRouteProfile);
        Assert.Equal(OnePointRoute, storage.Text["mosstank/navs/jaw_1.af"]);
        Assert.False(storage.Text.ContainsKey(FollowRouteKey));
    }
}
