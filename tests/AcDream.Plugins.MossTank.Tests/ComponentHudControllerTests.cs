using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The Comps HUD, as Virindi HUDs draws it: one row per listed item with
/// its icon and the count the character carries, a plus button that adds
/// the selection, a minus button that starts a flashing removal the next
/// row click finishes, a list kept per character and an on switch kept for
/// every character.
/// </summary>
public sealed class ComponentHudControllerTests
{
    private const uint SelfId = 0x50000001u;
    private const uint ScarabId = 0x80000010u;
    private const uint TaperId = 0x80000011u;
    private const uint ScarabIcon = 0x06001F5Bu;

    private static PluginWorldObject Item(uint id, string name, int stack = 1, bool owned = true, uint icon = 0u) =>
        new(id, 1u, name, PluginObjectClass.SpellComponent, 0u, owned ? SelfId : 0u, 0u)
        {
            IsOwned = owned,
            StackSize = stack,
            IconId = icon,
        };

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = [];
        public bool IsAvailable => true;
        public string? ReadText(string key) => Text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class FakeObjects : IWorldObjectAutomation
    {
        public List<PluginWorldObject> Objects { get; } = [];
        public int Captures { get; private set; }
        public bool IsAvailable => true;

        public IReadOnlyList<PluginWorldObject> CaptureObjects()
        {
            Captures++;
            return [.. Objects];
        }

        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            int index = Objects.FindIndex(o => o.ObjectId == objectId);
            value = index < 0 ? default : Objects[index];
            return index >= 0;
        }
    }

    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId { get; set; }
        public uint? PreviousObjectId => null;

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId)
        {
            SelectedObjectId = objectId;
            return true;
        }

        public bool Clear()
        {
            SelectedObjectId = null;
            return true;
        }
    }

    private sealed class Rig
    {
        public LandscapeMapControllerTests.FakeUi Ui { get; } = new();
        public FakeObjects Objects { get; } = new();
        public FakeSelection Selection { get; } = new();
        public MemoryStorage Storage { get; }
        public (string World, string Character) Identity { get; set; } = ("Coldeve", "Acdream");
        public List<string> Notices { get; } = [];
        public ComponentHudController Controller { get; }

        public Rig(MemoryStorage? storage = null, bool enabled = true)
        {
            Storage = storage ?? new MemoryStorage();
            Controller = new ComponentHudController(
                Ui, Objects, Selection, Storage, () => Identity, Notices.Add);
            if (enabled)
                Controller.Enabled = true;
        }

        public LandscapeMapControllerTests.FakeCanvas Canvas => Ui.Canvases[^1];

        public void Ticks(int count, double each = 0.015d)
        {
            for (int i = 0; i < count; i++)
                Controller.OnTick(each);
        }

        /// <summary>Selects an item and presses plus, as the player would.</summary>
        public void Add(uint objectId)
        {
            Selection.SelectedObjectId = objectId;
            Ticks(1);
            Press(ComponentHudController.PlusButton);
            Ticks(1);
        }

        public void Press(PluginRect button)
        {
            double x = button.X + (button.Width / 2d);
            double y = button.Y + (button.Height / 2d);
            Canvas.Down(x, y);
            Canvas.Up(x, y);
        }

        public void ClickRow(int row)
        {
            double y = ComponentHudController.HeaderHeight + (row * ComponentHudController.RowHeight) + 5d;
            Canvas.Down(10d, y);
            Canvas.Up(10d, y);
        }

        public LandscapeMapControllerTests.RecordingPainter Paint()
        {
            var painter = new LandscapeMapControllerTests.RecordingPainter(
                ComponentHudController.HudWidth, Canvas.Height);
            Canvas.PaintCallback(painter);
            return painter;
        }
    }

    /// <summary>
    /// A count adds every owned stack of exactly that name, and an object
    /// with no stack counts one; an item on the ground or of another name
    /// is not the character's. Mutation: count objects rather than stack
    /// sizes and two stacks read 2; drop the owned test and the ground
    /// pile is counted.
    /// </summary>
    [Fact]
    public void ACountAddsTheOwnedStacksOfExactlyThatName()
    {
        PluginWorldObject[] objects =
        [
            Item(1u, "Prismatic Taper", stack: 100),
            Item(2u, "Prismatic Taper", stack: 37),
            Item(3u, "Prismatic Taper", stack: 500, owned: false),
            Item(4u, "Prismatic Tapers", stack: 9),
            Item(5u, "prismatic taper", stack: 9),
        ];
        Assert.Equal(137, ComponentHudController.CountOwned(objects, "Prismatic Taper"));
        Assert.Equal(1, ComponentHudController.CountOwned([Item(6u, "Lead Scarab", stack: 0)], "Lead Scarab"));
    }

    /// <summary>
    /// Plus adds the selected object by its name and icon; the same name
    /// twice is one row; the counts are taken at once and every 600 ms
    /// after. Mutation: drop the duplicate test and the list holds two
    /// scarab rows; drop the refresh and a new stack never shows.
    /// </summary>
    [Fact]
    public void PlusAddsTheSelectionOnceAndTheCountsFollowTheInventory()
    {
        var rig = new Rig();
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab", stack: 12, icon: ScarabIcon));
        rig.Add(ScarabId);
        rig.Add(ScarabId);

        Assert.Equal([new ComponentHudEntry("Lead Scarab", ScarabIcon)], rig.Controller.Entries);
        Assert.Equal([12], rig.Controller.Counts);

        rig.Objects.Objects.Add(Item(0x80000099u, "Lead Scarab", stack: 3));
        rig.Ticks(1, 0.3d);
        Assert.Equal([12], rig.Controller.Counts);
        rig.Ticks(1, 0.31d);
        Assert.Equal([15], rig.Controller.Counts);
    }

    /// <summary>
    /// Each row paints its icon from the client art the item was added
    /// with, and its count. Mutation: skip the icon and no image is drawn.
    /// </summary>
    [Fact]
    public void ARowPaintsTheAddedIconAndTheCount()
    {
        var rig = new Rig();
        rig.Ui.ImageStore.ClientArt.Add(ScarabIcon);
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab", stack: 12, icon: ScarabIcon));
        rig.Add(ScarabId);

        LandscapeMapControllerTests.RecordingPainter painter = rig.Paint();
        Assert.Contains(ScarabIcon, rig.Ui.ImageStore.Requested);
        Assert.Single(painter.Images);
        Assert.Equal(["12"], painter.Texts);
    }

    /// <summary>
    /// The canvas is as tall as the list: the title strip and a row per
    /// item, or 16 pixels of empty list; a list that grew takes a new
    /// canvas at the same place. Mutation: keep the first canvas and the
    /// second row is painted off its bottom.
    /// </summary>
    [Fact]
    public void TheCanvasIsResizedToTheList()
    {
        var rig = new Rig();
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab"));
        rig.Objects.Objects.Add(Item(TaperId, "Prismatic Taper"));
        rig.Ticks(1);
        Assert.Equal(ComponentHudController.HeaderHeight + 16, rig.Canvas.Height);
        Assert.True(rig.Canvas.IsVisible);

        rig.Add(ScarabId);
        rig.Add(TaperId);
        Assert.Equal(ComponentHudController.HeaderHeight + 40, rig.Canvas.Height);
        Assert.All(rig.Ui.Canvases.SkipLast(1), static canvas => Assert.True(canvas.Disposed));
        Assert.False(rig.Canvas.Disposed);
    }

    /// <summary>
    /// Minus starts a removal and says how to finish it; the counts flash
    /// between bright and dark red; the next row clicked comes off and the
    /// removal ends. Mutation: leave the removal running and a second
    /// click takes the next row too.
    /// </summary>
    [Fact]
    public void MinusThenARowClickRemovesThatRow()
    {
        var rig = new Rig();
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab"));
        rig.Objects.Objects.Add(Item(TaperId, "Prismatic Taper"));
        rig.Add(ScarabId);
        rig.Add(TaperId);

        rig.Press(ComponentHudController.MinusButton);
        Assert.True(rig.Controller.Removing);
        Assert.Equal([ComponentHudController.RemovalPrompt], rig.Notices);
        rig.Ticks(1, 0.6d);
        PluginColor first = rig.Controller.CountColor;
        rig.Ticks(1, 0.6d);
        Assert.NotEqual(first, rig.Controller.CountColor);
        Assert.Contains(rig.Controller.CountColor, new[] { ComponentHudController.RemovalBright, ComponentHudController.RemovalDark });

        rig.ClickRow(0);
        Assert.False(rig.Controller.Removing);
        Assert.Equal(["Prismatic Taper"], rig.Controller.Entries.Select(static e => e.Name));
        Assert.Equal(ComponentHudController.Text, rig.Controller.CountColor);

        rig.Ticks(1);
        rig.ClickRow(0);
        Assert.Single(rig.Controller.Entries);
    }

    /// <summary>
    /// Minus twice cancels and says so; a removal left alone lapses after
    /// 30 seconds. Mutation: drop the timeout and the counts flash on.
    /// </summary>
    [Fact]
    public void ARemovalIsCancelledByMinusOrLapsesAfterThirtySeconds()
    {
        var rig = new Rig();
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab"));
        rig.Add(ScarabId);

        rig.Press(ComponentHudController.MinusButton);
        rig.Press(ComponentHudController.MinusButton);
        Assert.False(rig.Controller.Removing);
        Assert.Equal(ComponentHudController.RemovalCancelled, rig.Notices[^1]);

        rig.Press(ComponentHudController.MinusButton);
        rig.Ticks(49, 0.6d);
        Assert.True(rig.Controller.Removing);
        rig.Ticks(3, 0.6d);
        Assert.False(rig.Controller.Removing);
        Assert.Single(rig.Controller.Entries);
    }

    /// <summary>
    /// A row clicked with no removal under way stays; the press is a drag
    /// that moves the HUD by how far the pointer went, kept with the list.
    /// Mutation: remove on any click and a plain drag eats a row.
    /// </summary>
    [Fact]
    public void ADragMovesTheHudAndKeepsWhereItStands()
    {
        var storage = new MemoryStorage();
        var rig = new Rig(storage);
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab"));
        rig.Add(ScarabId);

        rig.Canvas.Drag(10d, 25d, 40d, 5d);
        Assert.Single(rig.Controller.Entries);
        var moved = new PluginPoint(ComponentHudController.DefaultX + 30, ComponentHudController.DefaultY - 20);
        Assert.Equal(moved, rig.Canvas.Offset);

        var again = new Rig(storage, enabled: false);
        again.Ticks(1);
        Assert.Equal(moved, again.Controller.Position);
    }

    /// <summary>
    /// The list is the character's: another character on the same storage
    /// starts empty, and the first one's list comes back at its next
    /// login. The on switch is shared, as the reference keeps it.
    /// Mutation: key the list by nothing and the second character
    /// inherits the scarabs.
    /// </summary>
    [Fact]
    public void TheListIsKeptPerCharacterAndTheSwitchForAll()
    {
        var storage = new MemoryStorage();
        var rig = new Rig(storage);
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab", icon: ScarabIcon));
        rig.Add(ScarabId);

        rig.Controller.Reset();
        rig.Identity = ("Coldeve", "Sigrid");
        rig.Ticks(1);
        Assert.Empty(rig.Controller.Entries);

        var returning = new Rig(storage, enabled: false);
        Assert.True(returning.Controller.Enabled);
        returning.Ticks(1);
        Assert.Equal([new ComponentHudEntry("Lead Scarab", ScarabIcon)], returning.Controller.Entries);
    }

    /// <summary>
    /// Off, or before a character is in the world, there is nothing to
    /// show. Mutation: register on enable alone and an empty HUD shows at
    /// the login screen.
    /// </summary>
    [Fact]
    public void NothingShowsWhileOffOrOutOfTheWorld()
    {
        var rig = new Rig(enabled: false) { Identity = (string.Empty, string.Empty) };
        rig.Ticks(1);
        Assert.Empty(rig.Ui.Canvases);

        rig.Controller.Enabled = true;
        rig.Ticks(1);
        Assert.Empty(rig.Ui.Canvases);

        rig.Identity = ("Coldeve", "Acdream");
        rig.Ticks(1);
        Assert.True(rig.Canvas.IsVisible);
        rig.Ticks(1);
        rig.Controller.Enabled = false;
        rig.Ticks(1);
        Assert.False(rig.Canvas.IsVisible);

        rig.Controller.Enabled = true;
        rig.Ticks(1);
        rig.Controller.Reset();
        rig.Identity = (string.Empty, string.Empty);
        rig.Ticks(1);
        Assert.False(rig.Canvas.IsVisible);
    }

    /// <summary>
    /// The plus button with nothing selected, or a selection the client
    /// does not know, adds nothing. Mutation: add a blank row.
    /// </summary>
    [Fact]
    public void PlusWithoutAKnownSelectionAddsNothing()
    {
        var rig = new Rig();
        rig.Ticks(1);
        rig.Press(ComponentHudController.PlusButton);
        rig.Selection.SelectedObjectId = 0x8000FFFFu;
        rig.Press(ComponentHudController.PlusButton);
        Assert.Empty(rig.Controller.Entries);
    }

    /// <summary>
    /// Removing by name ignores case and reports the listed spelling.
    /// Mutation: an ordinal match removes nothing.
    /// </summary>
    [Fact]
    public void RemovingByNameIgnoresCase()
    {
        var rig = new Rig();
        rig.Objects.Objects.Add(Item(ScarabId, "Lead Scarab"));
        rig.Add(ScarabId);
        Assert.Equal("Lead Scarab", rig.Controller.Remove(" lead scarab "));
        Assert.Empty(rig.Controller.Entries);
        Assert.Null(rig.Controller.Remove("Lead Scarab"));
    }
}
