using System.Xml.Linq;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MetaViewManagerTests
{
    private const string Markup =
        "<panel w=\"120\" h=\"60\" title=\"Meta\" visible=\"{WindowAvailable}\" />";

    [Fact]
    public void CreateReplaceDestroyAndDestroyAllOwnTheRegistrationTokens()
    {
        var ui = new RecordingUiRegistry();
        var manager = new MetaViewManager(new StubHost(ui));

        Assert.True(manager.Create("Status", Markup));
        Registration first = Assert.Single(ui.Registrations);
        Assert.Equal("Status", first.Descriptor.Title);
        Assert.True(first.Descriptor.ShowInSidePanel);
        Assert.True(manager.Create("Status", Markup));
        Assert.True(first.Disposed);
        Assert.Equal(1, manager.Count);

        Assert.True(manager.Destroy("Status"));
        Assert.True(ui.Registrations[^1].Disposed);
        Assert.False(manager.Destroy("Status"));

        Assert.True(manager.Create("One", Markup));
        Assert.True(manager.Create("Two", Markup));
        manager.DestroyAll();
        Assert.Equal(0, manager.Count);
        Assert.All(ui.Registrations[^2..], static registration =>
            Assert.True(registration.Disposed));
    }

    [Fact]
    public void InvalidMarkupAndTheOfficialViewBoundaryArePreserved()
    {
        var manager = new MetaViewManager(new StubHost(new RecordingUiRegistry()));

        Assert.False(manager.Create("Bad", "not xml"));
        Assert.False(manager.Create("Bad", "<label />"));
        for (int index = 0; index < 6; index++)
            Assert.True(manager.Create($"View {index}", Markup));

        Assert.Equal(6, manager.Count);
        Assert.False(manager.Create("Seventh", Markup));
        Assert.False(manager.Create("View 0", Markup));
    }

    [Fact]
    public void StatusHudReusesOneShelfWindowAndOwnsItsLifetime()
    {
        var ui = new RecordingUiRegistry();
        var manager = new StatusHudManager(new StubHost(ui));

        Assert.True(manager.Update("State", "Hunting"));
        Registration registration = Assert.Single(ui.Registrations);
        Assert.Equal("VTank Meta Status", registration.Descriptor.Title);
        Assert.True(registration.Descriptor.ShowInSidePanel);
        Assert.Equal(["State: Hunting"], manager.Rows);
        Assert.Equal([0xE8DEC3u], manager.RowColors);

        Assert.True(manager.Update("State", "Resting", 0x00FF00u));
        Assert.True(manager.Update("Target", "Drudge", 0xFF9900u));
        Assert.Single(ui.Registrations);
        Assert.Equal(["State: Resting", "Target: Drudge"], manager.Rows);
        Assert.Equal([0x00FF00u, 0xFF9900u], manager.RowColors);

        manager.Destroy();
        Assert.True(registration.Disposed);
        Assert.Equal(0, manager.Count);
        Assert.Empty(manager.Rows);
        Assert.Empty(manager.RowColors);
    }

    [Fact]
    public void UtilityBeltUiExpressionsUseThePluginScopedViewRegistry()
    {
        var ui = new RecordingUiRegistry();
        using var expressions = new MossTankExpressionRuntime(new StubHost(ui));

        Assert.True(expressions.Evaluate("uiviewexists[`Status`]").IsTruthy);
        Assert.True(expressions.Evaluate("uiviewvisible[`Status`]").IsTruthy);
        Assert.True(expressions.Evaluate(
            "uisetlabel[uigetcontrol[`Status`,`Action`],`Run`]").IsTruthy);
        Assert.Equal("Run", ui.Label);
        // The visibility setter hands its second argument back, so hiding a
        // control answers 0.
        Assert.Equal(0d, expressions.Evaluate(
            "uisetvisible[uigetcontrol[`Status`,`Action`],0]").AsNumber());
        Assert.False(ui.ControlVisible);
    }

    /// <summary>
    /// A view action never fails its rule, whether or not a window could be
    /// shown: a DoAll that destroys a view that never opened and creates one
    /// that is refused still reaches the state change after them.
    /// </summary>
    [Fact]
    public void RefusedViewActionsDoNotStopTheRestOfADoAll()
    {
        var host = new StubHost(new RecordingUiRegistry());
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                new MetaRule
                {
                    Condition = MetaCondition.Always(),
                    Action = new MetaAction
                    {
                        Kind = MetaActionKind.All,
                        Children =
                        [
                            new MetaAction
                            {
                                Kind = MetaActionKind.DestroyView,
                                Text = "NeverOpened",
                            },
                            new MetaAction
                            {
                                Kind = MetaActionKind.CreateView,
                                Text = "Broken",
                                SecondaryText = "not xml",
                            },
                            new MetaAction
                            {
                                Kind = MetaActionKind.SetMetaState,
                                Text = "Next",
                            },
                        ],
                    },
                },
            ],
        };
        // The default services refuse every view, like a host that cannot
        // show one.
        var engine = new MetaEngine(host, expressions, profile, new MetaServices());
        engine.SetEnabled(true);

        engine.EvaluatePass();

        Assert.Equal("Next", engine.CurrentState);
    }

    /// <summary>
    /// Written for these tests in the shape the published multi-client metas
    /// use: a declaration, a titled view, one fixed layout, rows of 25-pixel
    /// buttons, a name shared by two buttons, a button with no action, one
    /// with an expression, one with a state, one with both.
    /// </summary>
    private const string PartyView = """
        <?xml version="1.0"?> <view width="240" height="100" title="PartyBoard">	<control type="layout" name="Grid" text="Command">
        		<control type="button" name="Member1" left="0" top="0" width="90" height="25" text="" />
        		<control type="button" name="Member1State" left="90" top="0" width="150" height="25" text="" actionexpr="setvar[`pinged`,1]" />
        		<control type="button" name="Footer" left="0" top="25" width="120" height="25" text="Reset" setstate="" actionexpr="setvar[`order`,`first`]" />
        		<control type="button" name="Footer" left="120" top="25" width="120" height="25" text="Leave" setstate="Leaving" actionexpr="setvar[`order`,`expression`]" />
        		<control type="checkbox" name="Unknown" left="0" top="50" width="20" height="20" />
        		<control type="button" name="Home" left="0" top="75" width="240" height="25" text="Home" setstate="Default" />
        	</control></view>
        """;

    [Fact]
    public void ViewXmlParsesItsLayoutAndButtonsWithTheLastNameWinning()
    {
        MetaView view = Assert.IsType<MetaView>(MetaView.Parse(PartyView, out _));

        Assert.Equal("PartyBoard", view.Title);
        Assert.Equal((240, 100), (view.Width, view.Height));
        MetaViewControl root = Assert.IsType<MetaViewControl>(view.Root);
        Assert.Equal(MetaViewControlKind.Layout, root.Kind);
        // The unknown control type is skipped; five buttons remain.
        Assert.Equal(
            ["Member1", "Member1State", "Footer", "Footer", "Home"],
            root.Children.Select(static child => child.Name));
        MetaViewControl footer = Assert.IsType<MetaViewControl>(view.Find("Footer"));
        Assert.Equal("Leave", footer.Text);
        Assert.Equal((120, 25, 120, 25), (footer.Left, footer.Top, footer.Width, footer.Height));
        Assert.Equal("Leaving", footer.SetState);
        Assert.Equal(MetaViewControlKind.Layout, view.Find("Grid")?.Kind);
        Assert.Null(view.Find("Unknown"));
        Assert.Null(MetaView.Parse("<view title=\"x\" />", out string error));
        Assert.NotEmpty(error);
    }

    [Fact]
    public void AHostWithoutAWindowKeepsTheViewAndItsControls()
    {
        var host = new StubHost(NoOpUiRegistry.Instance, hasUi: false);
        using var expressions = new MossTankExpressionRuntime(host);
        var manager = new MetaViewManager(host);
        expressions.Policy.MetaViews = manager;

        Assert.False(expressions.Evaluate("uiviewexists[`Party`]").IsTruthy);
        Assert.True(manager.Create("Party", PartyView));

        Assert.True(expressions.Evaluate("uiviewexists[`Party`]").IsTruthy);
        Assert.True(expressions.Evaluate("uiviewvisible[`Party`]").IsTruthy);
        Assert.Equal(1d, expressions.Evaluate(
            "uisetlabel[uigetcontrol[`Party`,`Member1`],`Horan`]").AsNumber());
        Assert.Equal("Horan", manager.Find("Party")?.Find("Member1")?.Text);
        Assert.Equal(0d, expressions.Evaluate(
            "uisetvisible[uigetcontrol[`Party`,`Member1State`],0]").AsNumber());
        Assert.False(manager.Find("Party")?.Find("Member1State")?.Visible);
        Assert.Equal(0d, expressions.Evaluate(
            "uigetcontrol[`Party`,`Missing`]").AsNumber());
        Assert.Throws<ExpressionEvaluationException>(() => expressions.Evaluate(
            "uisetlabel[uigetcontrol[`Party`,`Grid`],`x`]"));

        Assert.True(manager.Destroy("Party"));
        Assert.False(expressions.Evaluate("uiviewexists[`Party`]").IsTruthy);
        Assert.Equal(0d, expressions.Evaluate(
            "uigetcontrol[`Party`,`Member1`]").AsNumber());
    }

    [Fact]
    public void AButtonRunsItsExpressionAndThenItsState()
    {
        var host = new StubHost(NoOpUiRegistry.Instance, hasUi: false);
        using var expressions = new MossTankExpressionRuntime(host);
        var states = new List<(string State, string Order)>();
        var manager = new MetaViewManager(
            host,
            expression => expressions.Evaluate(expression),
            state => states.Add(
                (state, expressions.Evaluate("getvar[`order`]").ToDisplayString())));
        Assert.True(manager.Create("Party", PartyView));

        // "Footer" is the second of the two buttons with that name.
        Assert.True(manager.Click("Party", "Footer"));
        Assert.True(manager.Click("Party", "Member1State"));
        Assert.True(manager.Click("Party", "Member1"));
        Assert.False(manager.Click("Party", "Grid"));
        Assert.False(manager.Click("Party", "Missing"));

        Assert.Equal([("Leaving", "expression")], states);
        Assert.Equal(1d, expressions.Evaluate("getvar[`pinged`]").AsNumber());
    }

    [Fact]
    public void AFailingButtonExpressionStillChangesStateAndWarnsOnce()
    {
        const string view = """
            <view width="100" height="25" title="Broken"><control type="layout">
            <control type="button" name="Go" left="0" top="0" width="100" height="25" text="Go" actionexpr="nosuchfunction[]" setstate="Next" />
            </control></view>
            """;
        var host = new StubHost(NoOpUiRegistry.Instance, hasUi: false);
        using var expressions = new MossTankExpressionRuntime(host);
        var states = new List<string>();
        var manager = new MetaViewManager(
            host,
            expression => expressions.Evaluate(expression),
            states.Add);
        Assert.True(manager.Create("Broken", view));

        Assert.True(manager.Click("Broken", "Go"));
        Assert.True(manager.Click("Broken", "Go"));

        Assert.Equal(["Next", "Next"], states);
        string warning = Assert.Single(host.Log.Warnings);
        Assert.StartsWith("Error in Button meta expression", warning, StringComparison.Ordinal);
    }

    /// <summary>
    /// The published metas enter a state with DoAll { DestroyView; CreateView;
    /// SetState } and leave it from a button; without a window both still work.
    /// </summary>
    [Fact]
    public void AMetaCreatesAViewAndItsButtonMovesTheMetaWithoutAWindow()
    {
        var host = new StubHost(NoOpUiRegistry.Instance, hasUi: false);
        using var expressions = new MossTankExpressionRuntime(host);
        MetaEngine? engine = null;
        var manager = new MetaViewManager(
            host,
            expression => expressions.Evaluate(expression),
            state => engine!.Transition(state));
        expressions.Policy.MetaViews = manager;
        var profile = new MetaProfile
        {
            Rules =
            [
                new MetaRule
                {
                    Condition = MetaCondition.Always(),
                    Action = new MetaAction
                    {
                        Kind = MetaActionKind.All,
                        Children =
                        [
                            new MetaAction { Kind = MetaActionKind.DestroyView, Text = "Party" },
                            new MetaAction
                            {
                                Kind = MetaActionKind.CreateView,
                                Text = "Party",
                                SecondaryText = PartyView,
                            },
                            new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Waiting" },
                        ],
                    },
                },
            ],
        };
        engine = new MetaEngine(host, expressions, profile, new MetaServices
        {
            CreateView = manager.Create,
            DestroyView = manager.Destroy,
            DestroyAllViews = manager.DestroyAll,
        });
        engine.SetEnabled(true);

        engine.EvaluatePass();
        Assert.Equal("Waiting", engine.CurrentState);
        Assert.True(expressions.Evaluate("uiviewexists[`Party`]").IsTruthy);

        Assert.True(manager.Click("Party", "Footer"));
        Assert.Equal("Leaving", engine.CurrentState);
        Assert.Equal("expression", expressions.Evaluate("getvar[`order`]").ToDisplayString());
    }

    [Fact]
    public void AWindowHostDrawsTheViewAtItsAuthoredLayout()
    {
        var ui = new RecordingUiRegistry();
        var manager = new MetaViewManager(new StubHost(ui));

        Assert.True(manager.Create("Party", PartyView));

        Registration registration = Assert.Single(ui.Registrations);
        Assert.Equal("Meta - PartyBoard", registration.Descriptor.Title);
        Assert.True(registration.Descriptor.StartVisible);
        XElement panel = XElement.Parse(registration.Markup);
        Assert.Equal("panel", panel.Name.LocalName);
        Assert.Equal("Meta - PartyBoard", (string?)panel.Attribute("title"));
        // The view's 240 x 100 content, a 4-pixel margin at each side and a
        // 24-pixel title strip above.
        Assert.Equal((248, 128), ((int)panel.Attribute("w")!, (int)panel.Attribute("h")!));
        XElement group = Assert.Single(panel.Elements());
        Assert.Equal("group", group.Name.LocalName);
        Assert.Equal(
            ("4", "24", "240", "100", "{Visible0}"),
            ((string?)group.Attribute("x"), (string?)group.Attribute("y"),
             (string?)group.Attribute("w"), (string?)group.Attribute("h"),
             (string?)group.Attribute("visible")));
        Assert.Equal(
            [
                "Member1 0,0 90x25 {Text1} {Click1} {Visible1}",
                "Member1State 90,0 150x25 {Text2} {Click2} {Visible2}",
                "Footer 0,25 120x25 {Text3} {Click3} {Visible3}",
                "Footer 120,25 120x25 {Text4} {Click4} {Visible4}",
                "Home 0,75 240x25 {Text5} {Click5} {Visible5}",
            ],
            group.Elements().Select(static button =>
                $"{(string?)button.Attribute("name")} "
                + $"{(string?)button.Attribute("x")},{(string?)button.Attribute("y")} "
                + $"{(string?)button.Attribute("w")}x{(string?)button.Attribute("h")} "
                + $"{(string?)button.Attribute("text")} {(string?)button.Attribute("onclick")} "
                + $"{(string?)button.Attribute("visible")}"));
        Assert.All(group.Elements(), static button =>
            Assert.Equal("button", button.Name.LocalName));
    }

    /// <summary>
    /// The host binds onclick to an Action property, visible to a bool and
    /// text to a string, by name on the binding object, and a missing
    /// onclick or visible fails the whole window.
    /// </summary>
    [Fact]
    public void EveryBindingOfTheDrawnViewResolvesOnItsBindingObject()
    {
        var ui = new RecordingUiRegistry();
        var manager = new MetaViewManager(new StubHost(ui));
        Assert.True(manager.Create("Party", PartyView));
        Registration registration = Assert.Single(ui.Registrations);
        Type bindingType = registration.Binding.GetType();

        int bound = 0;
        foreach (XAttribute attribute in XElement.Parse(registration.Markup)
            .DescendantsAndSelf().Attributes())
        {
            string value = attribute.Value;
            if (!value.StartsWith('{') || !value.EndsWith('}'))
                continue;
            System.Reflection.PropertyInfo? property =
                bindingType.GetProperty(value[1..^1]);
            Type expected = attribute.Name.LocalName switch
            {
                "onclick" => typeof(Action),
                "visible" => typeof(bool),
                "text" => typeof(string),
                _ => throw new InvalidOperationException(attribute.ToString()),
            };
            Assert.True(property?.PropertyType == expected, attribute.ToString());
            bound++;
        }
        Assert.Equal(16, bound);
    }

    [Fact]
    public void ADrawnButtonRunsTheControlItWasDrawnFromAndShowsUiChanges()
    {
        var ui = new RecordingUiRegistry();
        var host = new StubHost(ui);
        using var expressions = new MossTankExpressionRuntime(host);
        var states = new List<string>();
        var manager = new MetaViewManager(
            host,
            expression => expressions.Evaluate(expression),
            states.Add);
        expressions.Policy.MetaViews = manager;
        Assert.True(manager.Create("Party", PartyView));
        object binding = Assert.Single(ui.Registrations).Binding;

        // Both "Footer" buttons are drawn; each runs its own actions.
        Read<Action>(binding, "Click3")();
        Assert.Empty(states);
        Assert.Equal("first", expressions.Evaluate("getvar[`order`]").ToDisplayString());
        Read<Action>(binding, "Click4")();
        Assert.Equal(["Leaving"], states);
        Assert.Equal("expression", expressions.Evaluate("getvar[`order`]").ToDisplayString());

        Assert.Equal("Reset", Read<string>(binding, "Text3"));
        expressions.Evaluate("uisetlabel[uigetcontrol[`Party`,`Member1`],`Horan`]");
        Assert.Equal("Horan", Read<string>(binding, "Text1"));
        Assert.True(Read<bool>(binding, "Visible2"));
        expressions.Evaluate("uisetvisible[uigetcontrol[`Party`,`Member1State`],0]");
        Assert.False(Read<bool>(binding, "Visible2"));
        // The meta's view answers ahead of the host, which knows no "Party".
        Assert.Equal("Horan", manager.Find("Party")?.Find("Member1")?.Text);
        Assert.Equal(string.Empty, ui.Label);
    }

    [Fact]
    public void TheBindingHasASlotForEveryControlItCanDraw()
    {
        Type type = typeof(MetaViewBinding);
        for (int slot = 0; slot < MetaViewBinding.Capacity; slot++)
        {
            Assert.Equal(typeof(Action), type.GetProperty($"Click{slot}")?.PropertyType);
            Assert.Equal(typeof(string), type.GetProperty($"Text{slot}")?.PropertyType);
            Assert.Equal(typeof(bool), type.GetProperty($"Visible{slot}")?.PropertyType);
        }
        Assert.Null(type.GetProperty($"Click{MetaViewBinding.Capacity}"));

        string buttons = string.Concat(Enumerable.Range(0, MetaViewBinding.Capacity + 1)
            .Select(static index =>
                $"<control type=\"button\" name=\"B{index}\" left=\"0\" top=\"0\" width=\"10\" height=\"10\" text=\"\" />"));
        var ui = new RecordingUiRegistry();
        var host = new StubHost(ui);
        var manager = new MetaViewManager(host);
        Assert.True(manager.Create(
            "Wide",
            $"<view width=\"10\" height=\"10\" title=\"Wide\"><control type=\"layout\">{buttons}</control></view>"));

        XElement panel = XElement.Parse(Assert.Single(ui.Registrations).Markup);
        // The layout takes slot 0, so one button fewer than the capacity is drawn.
        Assert.Equal(MetaViewBinding.Capacity - 1, panel.Descendants("button").Count());
        Assert.Single(host.Log.Warnings);
        Assert.True(manager.ControlExists("Wide", $"B{MetaViewBinding.Capacity}"));
    }

    private static T Read<T>(object binding, string property) =>
        (T)binding.GetType().GetProperty(property)!.GetValue(binding)!;

    private sealed class StubHost(IUiRegistry ui, bool hasUi = true) : IPluginHost
    {
        public bool HasUi => hasUi;
        public StubLogger Log { get; } = new();
        IPluginLogger IPluginHost.Log => Log;
        public IGameState State { get; } = new StubState();
        public IEvents Events { get; } = new StubEvents();
        public ISelectionService Selection { get; } = new StubSelection();
        public IUiRegistry Ui { get; } = ui;
        public IAutomationSurface Automation => NoOpAutomationSurface.Instance;
    }

    private sealed class RecordingUiRegistry : IUiRegistry
    {
        public List<Registration> Registrations { get; } = [];
        public string Label { get; private set; } = string.Empty;
        public bool ControlVisible { get; private set; } = true;

        public void AddMarkupPanel(string markupPath, object binding)
        {
        }

        public IDisposable RegisterPanelContent(
            PluginPanelDescriptor descriptor,
            string markupContent,
            object binding)
        {
            var registration = new Registration(descriptor, markupContent, binding);
            Registrations.Add(registration);
            return registration;
        }

        public bool ViewExists(string viewName) => viewName == "Status";
        public bool IsViewVisible(string viewName) => viewName == "Status";
        public bool ControlExists(string viewName, string controlName) =>
            viewName == "Status" && controlName == "Action";
        public bool SetControlLabel(
            string viewName,
            string controlName,
            string label)
        {
            if (!ControlExists(viewName, controlName))
                return false;
            Label = label;
            return true;
        }
        public bool SetControlVisible(
            string viewName,
            string controlName,
            bool visible)
        {
            if (!ControlExists(viewName, controlName))
                return false;
            ControlVisible = visible;
            return true;
        }
    }

    private sealed class Registration(
        PluginPanelDescriptor descriptor,
        string markup,
        object binding) : IDisposable
    {
        public PluginPanelDescriptor Descriptor { get; } = descriptor;
        public object Binding { get; } = binding;
        public string Markup { get; } = markup;
        public bool Disposed { get; private set; }
        public void Dispose() => Disposed = true;
    }

    private sealed class StubLogger : IPluginLogger
    {
        public List<string> Warnings { get; } = [];
        public void Info(string message) { }
        public void Warn(string message) => Warnings.Add(message);
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class StubState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class StubEvents : IEvents
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

    private sealed class StubSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }
}
