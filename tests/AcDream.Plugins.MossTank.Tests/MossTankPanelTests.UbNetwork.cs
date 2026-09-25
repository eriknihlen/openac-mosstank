using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The three commands that reach the other clients on this computer, and the
/// setting that decides which broadcasts this one answers to. The grammar is
/// pinned branch by branch, because a delay read as part of the command line
/// is a silent wrong command on every client at once.
/// </summary>
public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// The peer surface, recording both directions: what was broadcast, and
    /// what labels this client was told to answer to.
    /// </summary>
    private sealed class BroadcastProbe : INetworkAutomation
    {
        public bool IsAvailable => true;
        public List<PluginNetworkClient> Clients { get; } = [];
        public List<(string Line, string[] Tags, int Delay)> Sent { get; } = [];
        public List<string[]> TagPushes { get; } = [];
        public bool Accepts { get; set; } = true;

        public IReadOnlyList<PluginNetworkClient> CaptureClients() => [.. Clients];

        public bool BroadcastCommand(
            string line,
            IReadOnlyList<string> tags,
            int delayMilliseconds)
        {
            Sent.Add((line, [.. tags], delayMilliseconds));
            return Accepts;
        }

        public bool SetTags(IReadOnlyList<string> tags)
        {
            TagPushes.Add([.. tags]);
            return true;
        }
    }

    private static PluginNetworkClient NetClient(
        uint clientId,
        string name,
        params string[] tags) =>
        new(
            clientId,
            0x50000000u + clientId,
            name,
            "Coldeve",
            new PluginNavigationPosition(0xA9B40001u, 0d, 0d, 0d, 0f, true),
            tags,
            100u,
            100u,
            100u,
            100u,
            100u,
            100u,
            0f);

    private static MossTankPanel NetworkPanel(
        out FakeAutomation automation,
        out BroadcastProbe peers,
        MemoryStorage? storage = null)
    {
        peers = new BroadcastProbe();
        automation = new FakeAutomation
        {
            Name = "Acdream",
            WorldName = "Coldeve",
            ObjectId = 0x50000001u,
            PeerNetwork = peers,
        };
        return new MossTankPanel(new FakeHost(automation, storage ?? new MemoryStorage()));
    }

    /// <summary>Puts labels on this character through the settings page.</summary>
    private static void SetOwnTags(MossTankPanel panel, params string[] tags)
    {
        panel.SetUbFilterText("Networking.Tags");
        panel.ClickUbSettingValue(0);
        foreach (string tag in tags)
            panel.AddUbListEntry(tag);
        panel.HideUbListEditor();
    }

    // ── /ub bc ──────────────────────────────────────────────────────────

    /// <summary>
    /// The delay is the leading digits and everything after them is the
    /// command, exactly as the reference read it.
    /// Mutation: split the line on its first space instead and the bare form
    /// broadcasts "hello" with no verb.
    /// </summary>
    [Theory]
    [InlineData("bc 5000 /say hello", "/say hello", 5000)]
    [InlineData("bc /say hello", "/say hello", 0)]
    [InlineData("bc 0 /vt start", "/vt start", 0)]
    public void ABroadcastReadsItsDelayOffTheFrontOfTheLine(
        string typed,
        string expected,
        int delay)
    {
        MossTankPanel panel = NetworkPanel(out _, out BroadcastProbe peers);

        UbCommand(panel, typed);

        (string line, string[] tags, int sentDelay) = Assert.Single(peers.Sent);
        Assert.Equal(expected, line);
        Assert.Empty(tags);
        Assert.Equal(delay, sentDelay);
    }

    /// <summary>
    /// A broadcast says what it is doing, and names each client it reaches,
    /// as the reference did.
    /// Mutation: drop the per-client loop and only the header is written.
    /// </summary>
    [Fact]
    public void ABroadcastNamesEveryClientItReaches()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);
        peers.Clients.Add(NetClient(2u, "Horan"));
        peers.Clients.Add(NetClient(3u, "Yonneh"));

        UbCommand(panel, "bc 250 /say hello");

        Assert.Equal(
            [
                "[UB] Broadcasting command to all clients: \"/say hello\" with delay inbetween of 250ms",
                "[UB] Sending Horan: \"/say hello\" with delay inbetween of 250ms",
                "[UB] Sending Yonneh: \"/say hello\" with delay inbetween of 250ms",
            ],
            automation.Messages);
    }

    /// <summary>
    /// The client never hands a sender back its own broadcast, so the sender
    /// runs the line itself, down the same path a scheduled command takes.
    /// Mutation: drop the local run and the sending character alone sits out
    /// the command it typed.
    /// </summary>
    [Fact]
    public void TheSenderRunsItsOwnBroadcastLocally()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out _);

        UbCommand(panel, "bc 5000 /say hello");
        Assert.DoesNotContain("/say hello", automation.Submitted);

        panel.OnTick(0.05d);

        Assert.Contains("/say hello", automation.Submitted);
    }

    /// <summary>
    /// A delay this client cannot hold is refused rather than truncated.
    /// Mutation: fall back to zero on an overflow and a mistyped delay
    /// becomes a broadcast with no delay at all.
    /// </summary>
    [Fact]
    public void ADelayTooLargeToHoldIsRefused()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);

        UbCommand(panel, "bc 99999999999 /say hello");

        Assert.Equal(
            "[UB] Error: Unable to broadcast command, invalid delay: 99999999999",
            Assert.Single(automation.Messages));
        Assert.Empty(peers.Sent);
    }

    [Fact]
    public void ABroadcastWithoutACommandPrintsItsUsage()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);

        UbCommand(panel, "bc 500");

        Assert.Equal(
            ["[UB] Error: Bad command syntax", "[UB] Usage: /ub bc [millisecondDelay] <command>"],
            automation.Messages.Take(2));
        Assert.Empty(peers.Sent);
    }

    /// <summary>
    /// A refused broadcast says so rather than printing a list of clients it
    /// never reached.
    /// Mutation: ignore the return value and the failure is announced as a
    /// success.
    /// </summary>
    [Fact]
    public void ABroadcastTheClientRefusesSaysSo()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);
        peers.Accepts = false;
        peers.Clients.Add(NetClient(2u, "Horan"));

        UbCommand(panel, "bc /say hello");

        Assert.Equal(
            "[UB] Error: Unable to broadcast command to the other clients.",
            automation.Messages[^1]);
        Assert.DoesNotContain(
            automation.Messages,
            message => message.StartsWith("[UB] Sending ", StringComparison.Ordinal));
    }

    // ── /ub bct ─────────────────────────────────────────────────────────

    /// <summary>
    /// The tag list is comma separated, a tag holding a space is quoted, and
    /// the delay comes after it.
    /// Mutation: split the line on its first space and the quoted form loses
    /// half of its second tag into the command.
    /// </summary>
    [Theory]
    [InlineData("bct one,two 5000 /say hello", "/say hello", 5000, "one", "two")]
    [InlineData("bct three /say hello", "/say hello", 0, "three")]
    [InlineData(
        "bct \"some tag\",\"another tag\" /say hello",
        "/say hello",
        0,
        "some tag",
        "another tag")]
    public void ATaggedBroadcastReadsItsTagsThenItsDelay(
        string typed,
        string expectedLine,
        int expectedDelay,
        params string[] expectedTags)
    {
        MossTankPanel panel = NetworkPanel(out _, out BroadcastProbe peers);

        UbCommand(panel, typed);

        (string line, string[] tags, int delay) = Assert.Single(peers.Sent);
        Assert.Equal(expectedLine, line);
        Assert.Equal(expectedTags, tags);
        Assert.Equal(expectedDelay, delay);
    }

    [Fact]
    public void ATaggedBroadcastNamesTheTagsItIsAimedAt()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out _);

        UbCommand(panel, "bct one,two 250 /say hello");

        Assert.Equal(
            "[UB] Broadcasting command to clients with tags (one,two): \"/say hello\" "
            + "with delay inbetween of 250ms",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The sender runs a tagged line itself only when one of the tags is its
    /// own, which is what the reference asked before running it locally.
    /// Mutation: run it unconditionally and a tank answers the healers' line.
    /// </summary>
    [Fact]
    public void ATaggedBroadcastRunsLocallyOnlyWhenOneOfTheTagsIsOurs()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out _);
        SetOwnTags(panel, "tank");

        UbCommand(panel, "bct healer /say hello");
        panel.OnTick(0.05d);
        Assert.DoesNotContain("/say hello", automation.Submitted);

        UbCommand(panel, "bct healer,TANK /say hello");
        panel.OnTick(0.05d);

        Assert.Contains("/say hello", automation.Submitted);
    }

    [Fact]
    public void ATaggedBroadcastWithoutATagSaysSo()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);

        UbCommand(panel, "bct , /say hello");

        Assert.Equal(
            "[UB] Error: You must specify at least one tag to send the command to.",
            Assert.Single(automation.Messages));
        Assert.Empty(peers.Sent);
    }

    [Fact]
    public void ATaggedBroadcastWithNothingAfterTheTagsPrintsItsUsage()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);

        UbCommand(panel, "bct one");

        Assert.Equal(
            ["[UB] Error: Bad command syntax", "[UB] Usage: /ub bct <tags> [millisecondDelay] <command>"],
            automation.Messages.Take(2));
        Assert.Empty(peers.Sent);
    }

    // ── /ub netclients ──────────────────────────────────────────────────

    /// <summary>
    /// The list carries this character too: the client reports the others
    /// only, and a player counting who is logged in would come up one short.
    /// Mutation: list only what the client reports and this character is
    /// missing from its own list.
    /// </summary>
    [Fact]
    public void TheClientListCarriesThisCharacterAndTheOthers()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);
        peers.Clients.Add(NetClient(3u, "Yonneh", "healer"));
        peers.Clients.Add(NetClient(2u, "Horan"));

        UbCommand(panel, "netclients");

        Assert.Equal(
            [
                "[UB] ClientData<0, Acdream>",
                "[UB] ClientData<2, Horan>",
                "[UB] ClientData<3, Yonneh> [healer]",
            ],
            automation.Messages);
    }

    /// <summary>
    /// A tag narrows the list, this character included.
    /// Mutation: drop the filter and every client is listed under every tag.
    /// </summary>
    [Fact]
    public void AClientListWithATagListsOnlyThatTag()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out BroadcastProbe peers);
        peers.Clients.Add(NetClient(2u, "Horan", "tank"));
        peers.Clients.Add(NetClient(3u, "Yonneh", "healer"));
        SetOwnTags(panel, "healer");

        UbCommand(panel, "netclients healer");

        Assert.Equal(
            [
                "[UB] ClientData<0, Acdream> [healer]",
                "[UB] ClientData<3, Yonneh> [healer]",
            ],
            automation.Messages);
    }

    [Fact]
    public void AClientListWithNothingToShowSaysSo()
    {
        MossTankPanel panel = NetworkPanel(
            out FakeAutomation automation,
            out _);

        UbCommand(panel, "netclients raid");

        Assert.Equal("[UB] No net clients to show", Assert.Single(automation.Messages));
    }

    // ── Networking.Tags ─────────────────────────────────────────────────

    /// <summary>
    /// The labels row is what the client answers to, so an edit on the page
    /// has to reach the client. Nothing is pushed while the row is empty on
    /// load: the client may have been started with labels of its own.
    /// Mutation: push on every frame and the pushes pile up; push the empty
    /// row on load and the labels the client started with are cleared.
    /// </summary>
    [Fact]
    public void TheLabelsRowIsPushedToTheClientWhenItChanges()
    {
        MossTankPanel panel = NetworkPanel(out _, out BroadcastProbe peers);
        panel.OnTick(0.05d);
        Assert.Empty(peers.TagPushes);

        SetOwnTags(panel, "tank");
        panel.OnTick(0.05d);
        panel.OnTick(0.05d);

        Assert.Equal(["tank"], Assert.Single(peers.TagPushes));
    }

    /// <summary>
    /// A row that is filled before the page is built is pushed as the page
    /// is built, so a character logging back in answers to its own labels
    /// without anyone touching the page.
    /// Mutation: push only from the tick and the labels reach the client one
    /// frame late, or not at all on a host that never ticks the page.
    /// </summary>
    [Fact]
    public void ALabelsRowSavedEarlierIsPushedAsThePageIsBuilt()
    {
        var storage = new MemoryStorage();
        MossTankPanel first = NetworkPanel(out _, out _, storage);
        SetOwnTags(first, "tank");

        NetworkPanel(out _, out BroadcastProbe peers, storage);

        Assert.Equal(["tank"], Assert.Single(peers.TagPushes));
    }
}
