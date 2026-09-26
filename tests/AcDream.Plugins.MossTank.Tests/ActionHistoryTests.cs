namespace AcDream.Plugins.MossTank.Tests;

public sealed class ActionHistoryTests
{
    [Fact]
    public void CapturesTimestampAndOnlyConsecutiveChangesWithoutRebuildingOnRead()
    {
        var history = new ActionHistory(new FixedClock());
        history.Record("Scanning for targets");
        IReadOnlyList<string> first = history.Lines;
        history.Record("Scanning for targets");
        Assert.Same(first, history.Lines);
        Assert.Equal(["[12:34:56] Scanning for targets"], history.Lines);
        history.Record("Attacking target");
        history.Record("Scanning for targets");
        Assert.Equal(3, history.Lines.Count);
        Assert.Equal(0, history.FirstIndex);
    }

    [Fact]
    public void TrimmingAndClearKeepAbsolutePositionsAndDoNotInventAnAction()
    {
        var history = new ActionHistory(new FixedClock());
        for (int i = 0; i < ActionHistory.Capacity + 5; i++)
            history.Record($"Action {i}");
        Assert.Equal(ActionHistory.Capacity, history.Lines.Count);
        Assert.Equal(5, history.FirstIndex);
        Assert.Equal("[12:34:56] Action 5", history.Lines[0]);
        history.Clear();
        Assert.Empty(history.Lines);
        Assert.Equal(1005, history.FirstIndex);
        history.Record("Action 1004");
        Assert.Empty(history.Lines);
        history.Record("Next action");
        Assert.Equal("[12:34:56] Next action", Assert.Single(history.Lines));
        Assert.Equal(1005, history.FirstIndex);
    }

    private sealed class FixedClock : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => new(2026, 9, 26, 12, 34, 56, TimeSpan.Zero);
        public override TimeZoneInfo LocalTimeZone => TimeZoneInfo.Utc;
    }
}
