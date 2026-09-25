using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The ".nav" and ".met" writers: every real file read and written again is
/// the same bytes, what the reader takes the writer gives back, and what the
/// forms cannot hold is refused rather than dropped.
/// </summary>
public sealed class VtankFileWriterTests
{
    private static readonly string FixturesRoot = Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank");

    public static TheoryData<string> NavFixtures()
    {
        var data = new TheoryData<string>();
        foreach (string path in Directory.GetFiles(FixturesRoot, "*.nav", SearchOption.AllDirectories))
            data.Add(Path.GetRelativePath(FixturesRoot, path));
        return data;
    }

    public static TheoryData<string> MetFixtures()
    {
        var data = new TheoryData<string>();
        foreach (string path in Directory.GetFiles(FixturesRoot, "*.met", SearchOption.AllDirectories))
            data.Add(Path.GetRelativePath(FixturesRoot, path));
        string proof = Path.Combine(AppContext.BaseDirectory, "Fixtures", "vt-proof");
        foreach (string path in Directory.GetFiles(proof, "*.met"))
            data.Add(Path.GetRelativePath(FixturesRoot, path));
        return data;
    }

    /// <summary>
    /// Every .nav fixture, real files the route tool wrote, loads and saves
    /// back to the same bytes, line breaks and all. Mutation: printing
    /// numbers the runtime's shortest way ("59.156377220153809" for
    /// "59.1563772201538") or dropping the "0" line after each point breaks
    /// every one of them.
    /// </summary>
    [Theory]
    [MemberData(nameof(NavFixtures))]
    public void EveryNavFixtureSavesBackToTheSameBytes(string fixture)
    {
        string original = File.ReadAllText(Path.Combine(FixturesRoot, fixture));
        var route = new NavigationSettings();
        Assert.True(
            VtankNavRouteSerializer.TryLoad(original, route, MetafSerializer.NoOpSpells.Instance, out string error),
            error);

        string saved = ProfileLineEndings.Match(VtankNavRouteSerializer.Save(route), original);

        Assert.Equal(original, saved);
    }

    /// <summary>
    /// Every .met fixture loads and saves back to the same bytes: the rule
    /// order, the tables, the embedded routes with their character counts
    /// and the view markup that runs on into the next line. Mutation:
    /// ending the view markup with a line break, or counting an embedded
    /// route's line breaks as one character, breaks the files that have
    /// them.
    /// </summary>
    [Theory]
    [MemberData(nameof(MetFixtures))]
    public void EveryMetFixtureSavesBackToTheSameBytes(string fixture)
    {
        string original = File.ReadAllText(Path.Combine(FixturesRoot, fixture));
        Assert.True(
            VtankMetaProfileSerializer.TryLoad(original, out MetaProfile profile, out string error),
            error);

        string saved = ProfileLineEndings.Match(VtankMetaProfileSerializer.Save(profile), original);

        Assert.Equal(original, saved);
    }

    private const string EveryWaypointNav =
        "uTank2 NAV 1.2\r\n4\r\n9\r\n"
        + "0\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n"
        + "1\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n-2147483000\r\n"
        + "2\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n48\r\n"
        + "3\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n2500\r\n"
        + "4\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n/say hi\r\n"
        + "5\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n1342431769\r\nArchmage\r\n"
        + "8\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n"
        + "9\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n12.3456\r\nTrue\r\n150.00004\r\n"
        + "9\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n270\r\nFalse\r\n5E-05\r\n";

    /// <summary>
    /// The waypoint types no real fixture carries, the portal and the jump
    /// among them, save back as they were read. A jump's heading keeps every
    /// digit the file gave it, and its direction rides in the charge's fifth
    /// decimal, even a charge of nothing. Mutation: holding the heading in
    /// single precision writes "12.3456001281738"; reading "5E-05" as a
    /// plain number loses the strafe.
    /// </summary>
    [Fact]
    public void EveryWaypointTypeSavesBackAsItWasRead()
    {
        var route = new NavigationSettings();
        Assert.True(
            VtankNavRouteSerializer.TryLoad(EveryWaypointNav, route, MetafSerializer.NoOpSpells.Instance, out string error),
            error);
        Assert.Equal(RouteJumpDirection.StrafeLeft, route.Waypoints[7].JumpDirection);
        Assert.Equal(150, route.Waypoints[7].JumpChargeMilliseconds);
        Assert.Equal(RouteJumpDirection.StrafeRight, route.Waypoints[8].JumpDirection);
        Assert.Equal(0, route.Waypoints[8].JumpChargeMilliseconds);

        Assert.Equal(EveryWaypointNav, VtankNavRouteSerializer.Save(route));
    }

    /// <summary>
    /// A jump added here is written the way the route tool writes one: the
    /// shift flag as the word "True", the charge with its direction code in
    /// the fifth decimal and no trailing zeros. Mutation: writing the charge
    /// to four fixed decimals plus the code gives "50.00003" here but
    /// "0.00003" for a charge of nothing, where the route tool writes
    /// "3E-05".
    /// </summary>
    [Fact]
    public void AJumpMadeHereIsWrittenAsTheRouteToolWritesOne()
    {
        var route = new NavigationSettings { Mode = RouteMode.Circular };
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Jump,
            Position = new PluginNavigationPosition(0u, 10d, 20d, 0d, 0f, true),
            JumpHeadingDegrees = 90d,
            JumpHoldShift = true,
            JumpChargeMilliseconds = 50,
            JumpDirection = RouteJumpDirection.Forward,
        });
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Jump,
            Position = new PluginNavigationPosition(0u, 10d, 20d, 0d, 0f, true),
            JumpHeadingDegrees = 180d,
            JumpChargeMilliseconds = 0,
            JumpDirection = RouteJumpDirection.Forward,
        });

        Assert.Equal(
            "uTank2 NAV 1.2\r\n1\r\n2\r\n"
            + "9\r\n10\r\n20\r\n0\r\n0\r\n90\r\nTrue\r\n50.00003\r\n"
            + "9\r\n10\r\n20\r\n0\r\n0\r\n180\r\nFalse\r\n3E-05\r\n",
            VtankNavRouteSerializer.Save(route));
    }

    /// <summary>
    /// A number the route tool's fifteen digits do not bring back, from a
    /// file some other tool wrote or a point measured here, keeps every
    /// digit it needs, and zero is written without a sign. Mutation: always
    /// printing fifteen digits moves the point; printing the runtime's "-0"
    /// writes a zero the route tool never does.
    /// </summary>
    [Fact]
    public void ANumberFifteenDigitsCannotHoldKeepsItsDigits()
    {
        const string nav = "uTank2 NAV 1.2\r\n1\r\n1\r\n0\r\n-34.89208150285805\r\n1\r\n0\r\n0\r\n";
        var route = new NavigationSettings();
        Assert.True(VtankNavRouteSerializer.TryLoad(nav, route, MetafSerializer.NoOpSpells.Instance, out _));

        Assert.Equal(nav, VtankNavRouteSerializer.Save(route));

        var measured = new NavigationSettings { Mode = RouteMode.Once };
        measured.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Point,
            Position = new PluginNavigationPosition(0u, 12.3f, -0d, 1d, 0f, true),
        });
        Assert.Equal(
            "uTank2 NAV 1.2\r\n4\r\n1\r\n0\r\n12.300000190734863\r\n0\r\n1\r\n0\r\n",
            VtankNavRouteSerializer.Save(measured));
    }

    [Fact]
    public void AFollowRouteSavesAsItsTarget()
    {
        const string nav = "uTank2 NAV 1.2\r\n3\r\nBarris\r\n1342431769\r\n";
        var route = new NavigationSettings();
        Assert.True(VtankNavRouteSerializer.TryLoad(nav, route, MetafSerializer.NoOpSpells.Instance, out _));

        Assert.Equal(nav, VtankNavRouteSerializer.Save(route));
    }

    /// <summary>
    /// A backward jump has no code in the form, so a route holding one is
    /// refused rather than written with the jump turned forward. Mutation:
    /// writing it as forward saves a route that jumps the wrong way.
    /// </summary>
    [Fact]
    public void ARouteWithABackwardJumpIsRefused()
    {
        var route = new NavigationSettings();
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Jump,
            JumpDirection = RouteJumpDirection.Backward,
        });

        InvalidOperationException refused = Assert.Throws<InvalidOperationException>(
            () => VtankNavRouteSerializer.Save(route));
        Assert.Contains("backward", refused.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void AChatCommandOverTwoLinesIsRefused()
    {
        var route = new NavigationSettings();
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.ChatCommand,
            Text = "/say one\n/say two",
        });

        Assert.Throws<InvalidOperationException>(() => VtankNavRouteSerializer.Save(route));
    }

    /// <summary>
    /// The condition and action types no real fixture small enough to keep
    /// carries, and a route inside a meta with a jump, a checkpoint and a
    /// follow, come back from a write and a read as they went in, and a
    /// second write is the same bytes. Mutation: leaving a type out of the
    /// writer's table refuses the meta; writing a follow route's node count
    /// as its waypoint count (none) makes the reader reject the file.
    /// </summary>
    [Fact]
    public void EveryConditionAndActionTypeSurvivesAWriteAndARead()
    {
        var jumpRoute = new NavigationSettings { Mode = RouteMode.Once };
        jumpRoute.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Checkpoint,
            Position = new PluginNavigationPosition(0u, 1d, 2d, 3d, 0f, true),
        });
        jumpRoute.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Jump,
            Position = new PluginNavigationPosition(0u, 1d, 2d, 3d, 0f, true),
            JumpHeadingDegrees = 45d,
            JumpChargeMilliseconds = 300,
            JumpDirection = RouteJumpDirection.StrafeRight,
        });
        var followRoute = new NavigationSettings
        {
            Mode = RouteMode.Target,
            FollowTargetName = "Barris",
            FollowTargetObjectId = 0x50000ABCu,
        };
        var profile = new MetaProfile();
        profile.Rules.Add(Rule(
            new MetaCondition
            {
                Kind = MetaConditionKind.All,
                Children =
                [
                    new MetaCondition { Kind = MetaConditionKind.NeedToBuff },
                    new MetaCondition { Kind = MetaConditionKind.PersistentSecondsInStateGreaterThanOrEqual, Number = 30 },
                    new MetaCondition { Kind = MetaConditionKind.TimeLeftOnSpellGreaterThanOrEqual, Number = 2081, SecondaryNumber = 120 },
                    new MetaCondition { Kind = MetaConditionKind.LandblockEquals, Number = -1275133952 },
                    new MetaCondition { Kind = MetaConditionKind.PortalspaceEntered },
                    new MetaCondition
                    {
                        Kind = MetaConditionKind.Not,
                        Children = [new MetaCondition { Kind = MetaConditionKind.Expression, Text = "getcharvital_current[2]<50" }],
                    },
                    new MetaCondition { Kind = MetaConditionKind.MonsterPriorityCountWithinDistance, TertiaryNumber = 3, Number = 2, SecondaryNumber = 12.5 },
                    new MetaCondition { Kind = MetaConditionKind.ChatMessageCapture, Text = "^(?<who>.+) tells you", SecondaryText = "3,4" },
                ],
            },
            new MetaAction
            {
                Kind = MetaActionKind.All,
                Children =
                [
                    new MetaAction { Kind = MetaActionKind.GetVtankOption, Text = "EnableCombat", SecondaryText = "combat" },
                    new MetaAction { Kind = MetaActionKind.SetVtankOption, Text = "EnableLooting", SecondaryText = "1" },
                    new MetaAction { Kind = MetaActionKind.SetWatchdog, Text = "Stuck", Number = 5, SecondaryNumber = 60 },
                    new MetaAction { Kind = MetaActionKind.ClearWatchdog },
                    new MetaAction { Kind = MetaActionKind.CallMetaState, Text = "Buff", SecondaryText = "Default" },
                    new MetaAction { Kind = MetaActionKind.DestroyAllViews },
                ],
            }));
        profile.Rules.Add(Rule(
            MetaCondition.Always(),
            new MetaAction { Kind = MetaActionKind.LoadEmbeddedNavigationRoute, SecondaryText = "jumps", EmbeddedRoute = jumpRoute }));
        profile.Rules.Add(Rule(
            new MetaCondition { Kind = MetaConditionKind.Never },
            new MetaAction { Kind = MetaActionKind.LoadEmbeddedNavigationRoute, SecondaryText = "[None]", EmbeddedRoute = followRoute }));
        profile.Rules.Add(Rule(
            new MetaCondition { Kind = MetaConditionKind.Always },
            new MetaAction { Kind = MetaActionKind.CreateView, Text = "v", SecondaryText = "<view/>" }));

        string first = VtankMetaProfileSerializer.Save(profile);
        Assert.True(
            VtankMetaProfileSerializer.TryLoad(first, out MetaProfile reread, out string error),
            error);

        Assert.Equal(first, VtankMetaProfileSerializer.Save(reread));
        RouteWaypoint jump = reread.Rules[1].Action.EmbeddedRoute!.Waypoints[1];
        Assert.Equal(RouteJumpDirection.StrafeRight, jump.JumpDirection);
        Assert.Equal(300, jump.JumpChargeMilliseconds);
        Assert.Equal("Barris", reread.Rules[2].Action.EmbeddedRoute!.FollowTargetName);
        Assert.Equal("<view/>", reread.Rules[3].Action.SecondaryText);
    }

    /// <summary>
    /// What a .met cannot hold is refused, never dropped: a rule switched
    /// off, a condition only this plugin knows, a fraction in a whole-number
    /// field. Mutation: skipping switched-off rules writes a meta that
    /// silently lost them.
    /// </summary>
    [Fact]
    public void WhatAMetCannotHoldIsRefused()
    {
        var disabled = new MetaProfile();
        disabled.Rules.Add(Rule(MetaCondition.Always(), new MetaAction()));
        disabled.Rules[0].Enabled = false;
        Assert.Contains(
            "switched off",
            Assert.Throws<InvalidOperationException>(() => VtankMetaProfileSerializer.Save(disabled)).Message,
            StringComparison.Ordinal);

        var edge = new MetaProfile();
        edge.Rules.Add(Rule(new MetaCondition { Kind = MetaConditionKind.LoginComplete }, new MetaAction()));
        Assert.Throws<InvalidOperationException>(() => VtankMetaProfileSerializer.Save(edge));

        var fraction = new MetaProfile();
        fraction.Rules.Add(Rule(
            new MetaCondition { Kind = MetaConditionKind.SecondsInStateGreaterThanOrEqual, Number = 2.5 },
            new MetaAction()));
        Assert.Throws<InvalidOperationException>(() => VtankMetaProfileSerializer.Save(fraction));
    }

    private static MetaRule Rule(MetaCondition condition, MetaAction action) => new()
    {
        State = "Default",
        Condition = condition,
        Action = action,
    };
}
