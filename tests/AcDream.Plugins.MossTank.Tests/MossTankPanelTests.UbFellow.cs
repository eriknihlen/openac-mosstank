using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// /ub fellow: each sub-command's grammar and the one call
/// it hands the client, the name forms, and the client's refusals.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const uint Self = 1u;
    private const uint Yonneh = 0x50000AAAu;
    private const uint Sawato = 0x50000BBBu;

    /// <summary>
    /// The owner's meta line. Mutation: passing only the first word of the
    /// name, or sharing off, fails the recorded call.
    /// </summary>
    [Fact]
    public void FellowCreateAsksForTheWholeNameSharingExperience()
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) =
            FellowRig(inFellowship: false);

        UbCommand(panel, "fellow create Sawato Rockstyle");

        Assert.Equal(["create Sawato Rockstyle share=True"], fellowship.Calls);
        Assert.Empty(automation.Messages);
    }

    [Fact]
    public void FellowCreateInsideAFellowshipSendsNothing()
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();

        UbCommand(panel, "fellow create Again");

        Assert.Empty(fellowship.Calls);
        Assert.Equal("[UB] You are already in a fellowship.", Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Mutation: dropping the membership check sends a quit from nowhere.
    /// </summary>
    [Theory]
    [InlineData("fellow quit")]
    [InlineData("fellow recruit Yonneh")]
    [InlineData("fellow status")]
    public void FellowCommandsOutsideAFellowshipSayWhy(string line)
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) =
            FellowRig(inFellowship: false);

        UbCommand(panel, line);

        Assert.Empty(fellowship.Calls);
        Assert.Equal(
            "[UB] Your are not currently in a fellowship.",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Quit leaves; disband ends it for everyone. Mutation: swapping the
    /// disband flag fails both rows.
    /// </summary>
    [Theory]
    [InlineData("fellow quit", "quit disband=False")]
    [InlineData("fellow disband", "quit disband=True")]
    [InlineData("fellow QUIT", "quit disband=False")]
    public void FellowQuitAndDisbandSendTheRightQuit(string line, string call)
    {
        (_, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();

        UbCommand(panel, line);

        Assert.Equal([call], fellowship.Calls);
    }

    [Fact]
    public void FellowDisbandNeedsTheLead()
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) =
            FellowRig(leader: Sawato);

        UbCommand(panel, "fellow disband");

        Assert.Empty(fellowship.Calls);
        Assert.Equal("[UB] You are not the fellowship leader!", Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Open and close change the fellowship only when it is the other way
    /// and the character leads. Mutation: dropping either check sends a
    /// call on the refused rows.
    /// </summary>
    [Theory]
    [InlineData("fellow open", false, Self, "open=True")]
    [InlineData("fellow close", true, Self, "open=False")]
    [InlineData("fellow open", true, Self, null)]
    [InlineData("fellow close", false, Self, null)]
    [InlineData("fellow open", false, Sawato, null)]
    public void FellowOpenAndCloseAskOnlyWhenItWouldChange(
        string line,
        bool isOpen,
        uint leader,
        string? call)
    {
        (_, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig(leader: leader, isOpen: isOpen);

        UbCommand(panel, line);

        Assert.Equal(call is null ? [] : [call], fellowship.Calls);
    }

    /// <summary>
    /// The reference's status: whether experience is shared and split
    /// evenly in the heading, and each member's level beside its name.
    /// Mutation: leaving out the sharing clause or the level fails every
    /// row; testing the split only while sharing fails the last row.
    /// </summary>
    [Theory]
    [InlineData(true, true, "Sharing XP")]
    [InlineData(true, false, "Sharing XP, Uneven Split")]
    [InlineData(false, true, "NOT Sharing XP")]
    [InlineData(false, false, "NOT Sharing XP, Uneven Split")]
    public void FellowStatusPrintsTheFellowshipAndEachMember(
        bool shares,
        bool even,
        string clause)
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();
        fellowship.SharesExperience = shares;
        fellowship.SplitsExperienceEvenly = even;
        fellowship.Roster = [.. fellowship.Roster.Select(static (member, index) =>
            member with { Level = (uint)(100 + index) })];

        UbCommand(panel, "fellow status");

        Assert.Empty(fellowship.Calls);
        Assert.Equal(
            [
                "[UB] 00000001 00000001 Your current fellowship, \"Mosswart Hunters\", has 3 members, "
                    + clause + ". Open, Not Locked.",
                "[UB]  Me[100] H:100/120 (Leader) ",
                "[UB]  Yonneh[101] H:100/120",
                "[UB]  Sawato Rockstyle[102] H:100/120",
            ],
            automation.Messages);
    }

    /// <summary>
    /// Recruit takes the whole name, part of it with p, the nearest player
    /// with no name, and the selection with "selected". Mutation: matching
    /// part of a name without p recruits on the "recruit Yon" row; ignoring
    /// the selection recruits the nearest.
    /// </summary>
    [Theory]
    [InlineData("fellow recruit Yonneh", Yonneh)]
    [InlineData("fellow recruitp yon", Yonneh)]
    [InlineData("fellow recruit", Sawato)]
    [InlineData("fellow recruit selected", Yonneh)]
    [InlineData("fellow recruit Yon", 0u)]
    public void FellowRecruitResolvesThePlayer(string line, uint recruited)
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) =
            FellowRig(roster: [Fellow(Self, "Me", 0f)]);
        automation.WorldObjects =
        [
            PlayerAt(Yonneh, "Yonneh", 20d),
            PlayerAt(Sawato, "Sawato Rockstyle", 5d),
        ];
        automation.NavigationSnapshot = NavigationAt(0f);
        host.Selection.Select(Yonneh);

        UbCommand(panel, line);

        if (recruited == 0u)
        {
            // Silent, as the reference is without its debug output: a
            // "could not find player" error would be read by profiles that
            // wait on that line from the follow command.
            Assert.Empty(fellowship.Calls);
            Assert.Empty(automation.Messages);
            return;
        }
        Assert.Equal([Invariant($"recruit {recruited:X8}")], fellowship.Calls);
    }

    /// <summary>
    /// With Plugin.Debug on the reference names whom it recruits, dismisses
    /// or hands the lead to, and whom it could not find: its ordinary line
    /// (System, 5) and its error (Help, 15), not debug lines. Mutation:
    /// ignoring the setting prints nothing; printing them as debug lines
    /// fails the classes.
    /// </summary>
    [Fact]
    public void FellowTargetsAreNamedOnlyWithDebugOn()
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, _) = FellowRig();
        automation.WorldObjects = [PlayerAt(Yonneh, "Yonneh", 20d)];
        automation.NavigationSnapshot = NavigationAt(0f);
        UbCommand(panel, "opt set Plugin.Debug true");
        automation.Posted.Clear();

        UbCommand(panel, "fellow recruit Yonneh");
        UbCommand(panel, "fellow recruit Nobody");
        UbCommand(panel, "fellow dismiss Sawato Rockstyle");
        UbCommand(panel, "fellow leader Yonneh");
        UbCommand(panel, "fellow leader Nobody");

        Assert.Equal(
            [
                ("[UB] Recruiting Yonneh[0x50000AAA]", UbChat.GenericChatType),
                ("[UB] Error: Could not find player Nobody", UbChat.ErrorChatType),
                ("[UB] Dismissing Sawato Rockstyle[0x50000BBB]", UbChat.GenericChatType),
                ("[UB] Transferring leader to Yonneh[0x50000AAA]", UbChat.GenericChatType),
                ("[UB] Error: Could not find player Nobody", UbChat.ErrorChatType),
            ],
            automation.Posted);
    }

    /// <summary>
    /// Nobody already in the fellowship, and nobody farther than 75 meters,
    /// is recruited. Mutation: dropping the range check recruits the far one.
    /// </summary>
    [Theory]
    [InlineData(20d, true, false)]
    [InlineData(80d, false, false)]
    [InlineData(20d, false, true)]
    public void FellowRecruitSkipsMembersAndFarPlayers(
        double eastWestMeters,
        bool alreadyMember,
        bool sent)
    {
        List<PluginFellowMember> roster = [Fellow(Self, "Me", 0f)];
        if (alreadyMember)
            roster.Add(Fellow(Yonneh, "Yonneh", 20f));
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) =
            FellowRig(roster: roster);
        automation.WorldObjects = [PlayerAt(Yonneh, "Yonneh", eastWestMeters)];
        automation.NavigationSnapshot = NavigationAt(0f);

        UbCommand(panel, "fellow recruit Yonneh");

        Assert.Equal(sent ? 1 : 0, fellowship.Calls.Count);
    }

    /// <summary>
    /// Dismiss and leader look among the members: whole name, part with p,
    /// nearest other member with no name, an id or "selected".
    /// Mutation: letting a blank name pick the character itself dismisses 1.
    /// </summary>
    [Theory]
    [InlineData("fellow dismiss Yonneh", "dismiss 50000AAA")]
    [InlineData("fellow dismissp rock", "dismiss 50000BBB")]
    [InlineData("fellow dismiss", "dismiss 50000BBB")]
    [InlineData("fellow dismiss 0x50000AAA", "dismiss 50000AAA")]
    [InlineData("fellow dismiss selected", "dismiss 50000AAA")]
    [InlineData("fellow leader Sawato Rockstyle", "leader 50000BBB")]
    [InlineData("fellow leaderp yon", "leader 50000AAA")]
    [InlineData("fellow leader", "leader 50000BBB")]
    public void FellowDismissAndLeaderResolveTheMember(string line, string call)
    {
        (_, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();
        host.Selection.Select(Yonneh);

        UbCommand(panel, line);

        Assert.Equal([call], fellowship.Calls);
    }

    [Fact]
    public void FellowDismissByPartOfANameNeedsP()
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();

        UbCommand(panel, "fellow dismiss rock");

        Assert.Empty(fellowship.Calls);
        Assert.Empty(automation.Messages);
    }

    /// <summary>
    /// Dismissing yourself is quitting, as in the reference.
    /// Mutation: sending a dismiss for the character's own id.
    /// </summary>
    [Fact]
    public void FellowDismissingYourselfQuits()
    {
        (_, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();

        UbCommand(panel, "fellow dismiss 1");

        Assert.Equal(["quit disband=False"], fellowship.Calls);
    }

    [Theory]
    [InlineData("fellow dismiss Yonneh")]
    [InlineData("fellow leader Yonneh")]
    public void FellowDismissAndLeaderNeedTheLead(string line)
    {
        (_, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig(leader: Sawato);

        UbCommand(panel, line);

        Assert.Empty(fellowship.Calls);
    }

    /// <summary>
    /// A command the client did not send is reported. Mutation: returning
    /// without looking at the result leaves the chat silent.
    /// </summary>
    [Theory]
    [InlineData(PluginFellowshipCommandStatus.Rejected, "[UB] Error: The client refused to dismiss Yonneh.")]
    [InlineData(PluginFellowshipCommandStatus.Unavailable, "[UB] Error: Cannot dismiss Yonneh right now.")]
    public void AFellowCommandTheClientDidNotSendIsReported(
        PluginFellowshipCommandStatus status,
        string said)
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();
        fellowship.Result = new PluginFellowshipCommandResult(status);

        UbCommand(panel, "fellow dismiss Yonneh");

        Assert.Equal(["dismiss 50000AAA"], fellowship.Calls);
        Assert.Equal(said, automation.Messages[^1]);
    }

    [Theory]
    [InlineData("fellow")]
    [InlineData("fellow create")]
    [InlineData("fellow invite Yonneh")]
    [InlineData("fellow quit now")]
    public void BadFellowInputPrintsTheUsageLine(string line)
    {
        (FakeAutomation automation, FakeFellowship fellowship, MossTankPanel panel, FakeHost host) = FellowRig();

        UbCommand(panel, line);

        Assert.Empty(fellowship.Calls);
        Assert.Equal(
            [
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]",
                "Description: UB Fellowship Commands.",
                "Examples:",
            ],
            automation.Messages);
    }

    [Fact]
    public void FellowHasAUsageLine()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help fellow");

        Assert.Equal(
            "[UB] Usage: /ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]",
            automation.Messages[0]);
    }

    private static (FakeAutomation, FakeFellowship, MossTankPanel, FakeHost) FellowRig(
        bool inFellowship = true,
        uint leader = Self,
        bool isOpen = true,
        List<PluginFellowMember>? roster = null)
    {
        var fellowship = new FakeFellowship
        {
            IsInFellowship = inFellowship,
            LeaderObjectId = leader,
            IsOpen = isOpen,
            Roster = roster ??
            [
                Fellow(Self, "Me", 0f),
                Fellow(Yonneh, "Yonneh", 20f),
                Fellow(Sawato, "Sawato Rockstyle", 5f),
            ],
        };
        var automation = new FakeAutomation
        {
            ObjectId = Self,
            FellowshipSurface = fellowship,
        };
        var host = new FakeHost(automation);
        return (automation, fellowship, new MossTankPanel(host), host);
    }

    private static PluginFellowMember Fellow(uint id, string name, float distance) =>
        new(id, name, 100u, 120u, 100u, 100u, 100u, 100u, distance);

    private static PluginWorldObject PlayerAt(uint objectId, string name, double eastWestMeters) =>
        Player(objectId, name) with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, eastWestMeters / 240d, 0d, 0d, 0f, IsOutdoor: true),
        };

    private static string Invariant(FormattableString text) =>
        text.ToString(CultureInfo.InvariantCulture);

    /// <summary>A fellowship a test sets up, recording each call it is sent.</summary>
    private sealed class FakeFellowship : IFellowshipAutomation
    {
        public bool IsInFellowship { get; set; }
        public string Name { get; set; } = "Mosswart Hunters";
        public uint LeaderObjectId { get; set; }
        public bool IsOpen { get; set; }
        public bool IsLocked { get; set; }
        public bool SharesExperience { get; set; }
        public bool SplitsExperienceEvenly { get; set; }
        public int MemberCount => Roster.Count;
        public List<PluginFellowMember> Roster { get; set; } = [];
        public List<string> Calls { get; } = [];
        public PluginFellowshipCommandResult Result { get; set; } =
            new(PluginFellowshipCommandStatus.Accepted);

        public IReadOnlyList<PluginFellowMember> CaptureMembers() =>
            Roster.Where(member => member.ObjectId != Self).ToArray();

        public IReadOnlyList<PluginFellowMember> CaptureRoster() => Roster.ToArray();

        public PluginFellowshipCommandResult Create(string name, bool shareExperience) =>
            Record($"create {name} share={shareExperience}");

        public PluginFellowshipCommandResult Recruit(uint targetObjectId) =>
            Record(Invariant($"recruit {targetObjectId:X8}"));

        public PluginFellowshipCommandResult Dismiss(uint targetObjectId) =>
            Record(Invariant($"dismiss {targetObjectId:X8}"));

        public PluginFellowshipCommandResult Quit(bool disband) =>
            Record($"quit disband={disband}");

        public PluginFellowshipCommandResult AssignLeader(uint targetObjectId) =>
            Record(Invariant($"leader {targetObjectId:X8}"));

        public PluginFellowshipCommandResult SetOpen(bool isOpen) =>
            Record($"open={isOpen}");

        private PluginFellowshipCommandResult Record(string call)
        {
            Calls.Add(call);
            return Result;
        }
    }
}
