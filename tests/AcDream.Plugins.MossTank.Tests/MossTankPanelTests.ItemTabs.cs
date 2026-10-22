using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The Items and Consumables pages as VTank shows them: one row per buffed
/// item, its hands choice kept in the profile, and the consumables list with
/// the gems and food first.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const string ImportedProfileKey = "mosstank/profiles/Imported.usd";
    private const uint NoBuffsRowColor = 0xFF0000u;

    private static VtankRow ObjectRow(int objectId, int value) =>
        new() { Cells = { VtankCell.Int(objectId), VtankCell.Int(value) } };

    private static VtankRow NamedRow(string name, int value) =>
        new() { Cells = { VtankCell.String(name), VtankCell.Int(value) } };

    private static (MossTankPanel Panel, FakeHost Host, FakeAutomation Automation)
        LoadImportedProfile(
            MemoryStorage storage,
            VtankDatabase database,
            params PluginInventoryItem[] owned)
    {
        storage.Text[ImportedProfileKey] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ItemEntries = owned;
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        Command(panel, "settings load Imported");
        return (panel, host, automation);
    }

    private static PluginInventoryItem OwnedWand() =>
        Item(10, "War Wand", 0x8000u, validLocations: 0x01000000u);

    private static PluginInventoryItem OwnedSword() =>
        Item(11, "Fire Sword", 1u, validLocations: 0x00100000u);

    private static VtankTable SavedTable(MemoryStorage storage, string table) =>
        VtankDatabase.Parse(storage.Text[ImportedProfileKey]).Find(table)!;

    /// <summary>
    /// One row per item, whatever number of spells it carries; the item the
    /// profile lists for "any equipped weapon" and the character itself are
    /// not rows. The first spell listed for an item decides whether it is
    /// shown as carrying no buffs. An item the client does not know shows
    /// its id as a signed number.
    /// </summary>
    [Fact]
    public void ItemsPageListsOneRowPerBuffedItemAndSkipsTheAnyWeaponAndOwnRows()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable buffed = database.Find("BuffedItems")!;
        buffed.Rows.Add(ObjectRow(10, 101));
        buffed.Rows.Add(ObjectRow(10, 102));
        buffed.Rows.Add(ObjectRow(-1, 101));
        buffed.Rows.Add(ObjectRow(0x50000001, 101));
        buffed.Rows.Add(ObjectRow(11, -1));
        buffed.Rows.Add(ObjectRow(11, 101));
        buffed.Rows.Add(ObjectRow(12, 101));
        buffed.Rows.Add(ObjectRow(12, -1));
        buffed.Rows.Add(ObjectRow(13, -1));
        buffed.Rows.Add(ObjectRow(13, 101));
        buffed.Rows.Add(ObjectRow(unchecked((int)0x80001234u), 101));

        (MossTankPanel panel, _, _) = LoadImportedProfile(
            storage, database, OwnedWand(), OwnedSword());

        Assert.Equal(
            [
                "War Wand",
                "Fire Sword (NO BUFFS)",
                "<INVALID 12>",
                "<INVALID 13> (NO BUFFS)",
                "<INVALID -2147478988>",
            ],
            panel.ItemNameColumn);
        IReadOnlyList<uint> colors = panel.ItemNameColors;
        Assert.Equal(5, colors.Count);
        Assert.Equal(NoBuffsRowColor, colors[1]);
        Assert.Equal(NoBuffsRowColor, colors[3]);
        Assert.NotEqual(NoBuffsRowColor, colors[0]);
        Assert.NotEqual(NoBuffsRowColor, colors[2]);
        Assert.NotEqual(NoBuffsRowColor, colors[4]);
    }

    /// <summary>
    /// An item whose properties name its material reads with the material in
    /// front, as VTank lists it ("Silver Acid Mazule"); one without reads its
    /// name alone. Mutation: listing the plain name drops "Silver".
    /// </summary>
    [Fact]
    public void ItemsPageNamesAnItemWithItsMaterialInFront()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable buffed = database.Find("BuffedItems")!;
        buffed.Rows.Add(ObjectRow(10, 101));
        buffed.Rows.Add(ObjectRow(11, 101));
        storage.Text[ImportedProfileKey] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(10, "Acid Mazule", 1u, validLocations: 0x00100000u),
            OwnedSword(),
        ];
        automation.Properties[10u] = new PluginItemProperties(
            new Dictionary<uint, int> { [131u] = 63 },
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        Command(panel, "settings load Imported");

        Assert.Equal(["Silver Acid Mazule", "Fire Sword"], panel.ItemNameColumn);
    }

    /// <summary>
    /// Clicking an item's name takes the whole item off the page and out of
    /// the saved profile: every spell row it had and its hands choice.
    /// </summary>
    [Fact]
    public void ClickingAnItemNameRemovesTheWholeItemAndItsHandsChoice()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable buffed = database.Find("BuffedItems")!;
        buffed.Rows.Add(ObjectRow(10, 101));
        buffed.Rows.Add(ObjectRow(10, 102));
        buffed.Rows.Add(ObjectRow(11, -1));
        VtankTable uses = database.Find("ItemUseSpecifiers")!;
        uses.Rows.Add(ObjectRow(10, 1));
        uses.Rows.Add(ObjectRow(11, 2));

        (MossTankPanel panel, _, _) = LoadImportedProfile(
            storage, database, OwnedWand(), OwnedSword());
        Assert.Equal(["War Wand", "Fire Sword (NO BUFFS)"], panel.ItemNameColumn);

        panel.DeleteItemRowAt(0);

        Assert.Equal(["Fire Sword (NO BUFFS)"], panel.ItemNameColumn);
        VtankTable savedBuffed = SavedTable(storage, "BuffedItems");
        Assert.DoesNotContain(savedBuffed.Rows, row => row.Cells[0].AsInt() == 10);
        Assert.Contains(savedBuffed.Rows, row =>
            row.Cells[0].AsInt() == 11 && row.Cells[1].AsInt() == -1);
        VtankTable savedUses = SavedTable(storage, "ItemUseSpecifiers");
        Assert.DoesNotContain(savedUses.Rows, row => row.Cells[0].AsInt() == 10);
        Assert.Contains(savedUses.Rows, row =>
            row.Cells[0].AsInt() == 11 && row.Cells[1].AsInt() == 2);
    }

    /// <summary>
    /// The hands column shows for a melee weapon and for an item the client
    /// does not know, reading the profile's hands choice (both hands when the
    /// profile has none). A click steps it on, keeps it in the profile, and a
    /// reload reads it back.
    /// </summary>
    [Fact]
    public void HandsColumnShowsTheProfileChoiceAndACycleIsSavedAndReadBack()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable buffed = database.Find("BuffedItems")!;
        buffed.Rows.Add(ObjectRow(10, 101));
        buffed.Rows.Add(ObjectRow(11, 101));
        buffed.Rows.Add(ObjectRow(12, 101));
        buffed.Rows.Add(ObjectRow(14, 101));
        VtankTable uses = database.Find("ItemUseSpecifiers")!;
        uses.Rows.Add(ObjectRow(11, 2));
        uses.Rows.Add(ObjectRow(12, 7));

        (MossTankPanel panel, _, _) = LoadImportedProfile(
            storage, database, OwnedWand(), OwnedSword());

        Assert.Equal(["", "LeftHand", "", "BothHands"], panel.ItemHandsColumn);

        panel.CycleItemHandsAt(0);
        panel.CycleItemHandsAt(2);
        Assert.Equal(["", "LeftHand", "", "BothHands"], panel.ItemHandsColumn);

        panel.CycleItemHandsAt(1);
        Assert.Equal("BothHands", panel.ItemHandsColumn[1]);
        panel.CycleItemHandsAt(1);
        Assert.Equal("DONOTUSE", panel.ItemHandsColumn[1]);
        panel.CycleItemHandsAt(1);
        Assert.Equal("RightHand", panel.ItemHandsColumn[1]);
        panel.CycleItemHandsAt(3);
        Assert.Equal("DONOTUSE", panel.ItemHandsColumn[3]);

        VtankTable saved = SavedTable(storage, "ItemUseSpecifiers");
        Assert.Equal(
            [(11, 1), (12, 7), (14, 0)],
            saved.Rows.Select(row => (row.Cells[0].AsInt(), row.Cells[1].AsInt())));

        FakeAutomation again = ItemEnchantAutomation();
        again.ItemEntries = [OwnedWand(), OwnedSword()];
        var reloaded = new MossTankPanel(new FakeHost(again, storage));
        Command(reloaded, "settings load Imported");
        Assert.Equal(["", "RightHand", "", "DONOTUSE"], reloaded.ItemHandsColumn);
    }

    /// <summary>
    /// An item the profile names but the client has not seen yet is shown by
    /// id; once it arrives the page names it, without an edit or a refresh.
    /// </summary>
    [Fact]
    public void ItemsPageNamesAnItemWhenItArrivesAfterTheProfileLoaded()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(ObjectRow(10, 101));

        (MossTankPanel panel, FakeHost host, FakeAutomation automation) =
            LoadImportedProfile(storage, database);
        Assert.Equal(["<INVALID 10>"], panel.ItemNameColumn);
        Assert.Equal(["BothHands"], panel.ItemHandsColumn);

        automation.ItemEntries = [OwnedWand()];
        ((FakeEvents)host.Events).RaiseObjectChanged(10u, PluginObjectChangeKind.Created);
        panel.OnTick(0.3d);

        Assert.Equal(["War Wand"], panel.ItemNameColumn);
        Assert.Equal([""], panel.ItemHandsColumn);
    }

    /// <summary>
    /// Entering the world names the profile's items too.
    /// </summary>
    [Fact]
    public void ItemsPageNamesItsItemsWhenLoginCompletes()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(ObjectRow(10, 101));

        (MossTankPanel panel, FakeHost host, FakeAutomation automation) =
            LoadImportedProfile(storage, database);
        Assert.Equal(["<INVALID 10>"], panel.ItemNameColumn);

        automation.ItemEntries = [OwnedWand()];
        ((FakeEvents)host.Events).RaiseLoginComplete();
        panel.OnTick(0.3d);

        Assert.Equal(["War Wand"], panel.ItemNameColumn);
    }

    /// <summary>
    /// The consumables list is the profile's gems and food, then its other
    /// consumables, each in the profile's own order. A click removes the
    /// name from the gems and food when it is there, else from the others,
    /// and saves.
    /// </summary>
    [Fact]
    public void ConsumablesListGemsAndFoodFirstAndAClickRemovesFromTheRightTable()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable gems = database.Find("GemFoodItems")!;
        gems.Rows.Clear();
        gems.Rows.Add(NamedRow("Mana Gem", 2001));
        gems.Rows.Add(NamedRow("Bread Loaf", 2002));
        VtankTable assist = database.Find("AssistItems")!;
        assist.Rows.Add(NamedRow("Zeta Kit", 0));
        assist.Rows.Add(NamedRow("Mana Gem", 8));

        (MossTankPanel panel, _, _) = LoadImportedProfile(storage, database);

        Assert.Equal(["Mana Gem", "Bread Loaf", "Zeta Kit", "Mana Gem"], panel.ConsumableRows);

        panel.SelectConsumableRow(3);

        Assert.Equal(["Bread Loaf", "Zeta Kit", "Mana Gem"], panel.ConsumableRows);
        Assert.Equal(["Bread Loaf"],
            SavedTable(storage, "GemFoodItems").Rows.Select(row => row.Cells[0].AsString()));
        Assert.Equal(["Zeta Kit", "Mana Gem"],
            SavedTable(storage, "AssistItems").Rows.Select(row => row.Cells[0].AsString()));

        panel.SelectConsumableRow(1);

        Assert.Equal(["Bread Loaf", "Mana Gem"], panel.ConsumableRows);
        Assert.Equal(["Mana Gem"],
            SavedTable(storage, "AssistItems").Rows.Select(row => row.Cells[0].AsString()));
    }

    /// <summary>
    /// A listed item only moving -- carried about, nothing about it changed
    /// -- does not rebuild the page.
    /// </summary>
    [Fact]
    public void ItemsPageIsNotRebuiltWhenAListedItemOnlyMoves()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(ObjectRow(10, 101));

        (MossTankPanel panel, FakeHost host, FakeAutomation automation) =
            LoadImportedProfile(storage, database);
        Assert.Equal(["<INVALID 10>"], panel.ItemNameColumn);

        automation.ItemEntries = [OwnedWand()];
        ((FakeEvents)host.Events).RaiseObjectChanged(10u, PluginObjectChangeKind.Moved);
        panel.OnTick(0.3d);

        Assert.Equal(["<INVALID 10>"], panel.ItemNameColumn);
    }

    /// <summary>
    /// An item added with no buff the profile can list by spell id reads as
    /// carrying no buffs at once, as the saved profile will say it does.
    /// </summary>
    [Fact]
    public void AnItemAddedWithoutASpellIdReadsNoBuffsAtOnce()
    {
        var storage = new MemoryStorage();
        // A shield's default banes are not in this client's spell table.
        var automation = new FakeAutomation
        {
            ItemEntries = [Item(20, "Kite Shield", 2u, validLocations: 0x00200000u)],
        };
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        host.Selection.Select(20u);

        panel.AddSelectedItem();

        Assert.Equal(["Kite Shield (NO BUFFS)"], panel.ItemNameColumn);
        Assert.Equal(NoBuffsRowColor, panel.ItemNameColors[0]);
        var reloaded = new MossTankPanel(new FakeHost(new FakeAutomation
        {
            ItemEntries = [Item(20, "Kite Shield", 2u, validLocations: 0x00200000u)],
        }, storage));
        Assert.Equal(["Kite Shield (NO BUFFS)"], reloaded.ItemNameColumn);
    }

    /// <summary>
    /// A profile made from an old MossTank file starts with no hand choices:
    /// the ones of the profile loaded before it are not carried into it.
    /// </summary>
    [Fact]
    public void AMigratedOldProfileDoesNotInheritThePreviousProfilesHandChoices()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(ObjectRow(11, 101));
        database.Find("ItemUseSpecifiers")!.Rows.Add(ObjectRow(11, 1));
        (MossTankPanel panel, _, _) = LoadImportedProfile(
            storage, database, OwnedWand(), OwnedSword());
        Assert.Equal(["RightHand"], panel.ItemHandsColumn);
        storage.Text[LegacyNamedProfileKey("Old")] =
            """{ "Combat": { "MaximumRange": 42.0 } }""";

        Command(panel, "settings load Old");

        string migrated = Assert.Single(storage.Text.Keys,
            key => key.EndsWith("Old.usd", StringComparison.Ordinal));
        Assert.Empty(VtankDatabase.Parse(storage.Text[migrated])
            .Find("ItemUseSpecifiers")!.Rows);
    }
}
