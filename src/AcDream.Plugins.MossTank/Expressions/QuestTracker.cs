using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// The character's quest flags, read from the server's <c>/myquests</c>
/// answer. A refresh this tracker asks for itself -- the one at login and
/// <c>/ub myquests</c> -- hides the quest lines it reads, as UtilityBelt
/// does, and says once when the list is in; a <c>/myquests</c> the player
/// types is read the same way but left on screen.
/// </summary>
internal sealed partial class QuestTracker : IDisposable
{
    private const double CompletionSilenceSeconds = 1d;
    private const double RetrySeconds = 15d;
    private const int MaximumAttempts = 3;

    private readonly Dictionary<string, QuestFlag> _flags =
        new(StringComparer.OrdinalIgnoreCase);
    private ulong _chatSequence;
    private string _identity = string.Empty;
    private double _silenceSeconds;
    private int _attemptsRemaining;
    private bool _receivedFlag;
    private bool _hideOutput;
    private readonly IPluginHost host;
    private readonly IDisposable _filter;

    public QuestTracker(IPluginHost host)
    {
        this.host = host ?? throw new ArgumentNullException(nameof(host));
        _filter = host.Automation.Chat.RegisterFilter(HideQuestLine);
    }

    public bool IsRefreshing { get; private set; }
    public int Count => _flags.Count;

    public void BindIdentity(string identity)
    {
        if (identity.Equals(_identity, StringComparison.Ordinal))
            return;
        _identity = identity;
        _flags.Clear();
        IsRefreshing = false;
        _hideOutput = false;
        _receivedFlag = false;
        _silenceSeconds = 0d;
        if (!string.IsNullOrWhiteSpace(identity))
            Refresh();
    }

    /// <summary>
    /// Asks the server for the quest list. A refresh already under way is
    /// left to finish unless <paramref name="restart"/> asks for a new one.
    /// </summary>
    public void Refresh(bool restart = false)
    {
        if (IsRefreshing && !restart)
            return;
        _flags.Clear();
        _attemptsRemaining = MaximumAttempts;
        _receivedFlag = false;
        _silenceSeconds = 0d;
        IsRefreshing = true;
        _hideOutput = true;
        SubmitRequest();
    }

    public void Dispose() => _filter.Dispose();

    public void OnTick(double elapsedSeconds)
    {
        CaptureChat();
        if (!IsRefreshing)
            return;
        _silenceSeconds += elapsedSeconds;
        if (_receivedFlag && _silenceSeconds > CompletionSilenceSeconds)
        {
            Finish();
            UbChat.Post(host.Automation.Chat, UbChat.Line("Quest data updated."));
            return;
        }
        if (!_receivedFlag && _silenceSeconds > RetrySeconds)
            SubmitRequest();
    }

    public bool HasCompleted(string key) =>
        _flags.ContainsKey(Normalize(key));

    public bool IsReady(string key)
    {
        if (!_flags.TryGetValue(Normalize(key), out QuestFlag flag))
            return true;
        DateTimeOffset next = flag.CompletedOn.AddSeconds(flag.RepeatSeconds);
        if (next > DateTimeOffset.UtcNow)
            return false;
        return !IsOnce(Normalize(key), flag);
    }

    /// <summary>
    /// A flag the reference counts as done once and never again: one solve
    /// of one allowed. A kill task's flag (its key names a kill count) is
    /// never that, whatever its counts say, so it is ready again once its
    /// timer runs out.
    /// </summary>
    private static bool IsOnce(string key, in QuestFlag flag) =>
        !(KillTaskKey().IsMatch(key) && flag.MaxSolves >= 0)
        && flag.MaxSolves == 1
        && flag.Solves <= 1;

    public int Progress(string key) =>
        _flags.TryGetValue(Normalize(key), out QuestFlag flag) ? flag.Solves : 0;

    public int Required(string key) =>
        _flags.TryGetValue(Normalize(key), out QuestFlag flag) ? flag.MaxSolves : 0;

    private void CaptureChat()
    {
        foreach (PluginChatMessage message in host.Automation.Chat
            .CaptureMessages(_chatSequence).OrderBy(static message => message.Sequence))
        {
            _chatSequence = Math.Max(_chatSequence, message.Sequence);
            string text = message.Text.Trim();
            if (text.Equals("Quest list is empty.", StringComparison.Ordinal)
                || text.Equals(
                    "The command \"myquests\" is not currently enabled on this server.",
                    StringComparison.Ordinal))
            {
                Finish();
                continue;
            }
            TryRecord(text);
        }
    }

    /// <summary>
    /// The chat filter: while a refresh of our own is under way, a quest line
    /// is read here and kept off the screen. A line dropped here never
    /// reaches the transcript <see cref="CaptureChat"/> reads, which is why
    /// the reading happens in the filter too.
    /// </summary>
    private bool HideQuestLine(PluginChatMessage message) =>
        _hideOutput && IsRefreshing && TryRecord(message.Text.Trim());

    /// <summary>Records one quest line. False when the text is not one.</summary>
    private bool TryRecord(string text)
    {
        Match match = MyQuestLine().Match(text);
        if (!match.Success)
            return false;
        if (!int.TryParse(match.Groups["solves"].Value,
                NumberStyles.Integer, CultureInfo.InvariantCulture, out int solves)
            || !long.TryParse(match.Groups["completedOn"].Value,
                NumberStyles.Integer, CultureInfo.InvariantCulture, out long completed))
        {
            return false;
        }
        _ = int.TryParse(match.Groups["maxSolves"].Value,
            NumberStyles.Integer, CultureInfo.InvariantCulture, out int maximum);
        _ = long.TryParse(match.Groups["repeatTime"].Value,
            NumberStyles.Integer, CultureInfo.InvariantCulture, out long repeat);
        string key = Normalize(match.Groups["key"].Value);
        _flags[key] = new QuestFlag(
            solves,
            maximum,
            DateTimeOffset.FromUnixTimeSeconds(Math.Max(0L, completed)),
            Math.Max(0L, repeat));
        _receivedFlag = true;
        _silenceSeconds = 0d;
        return true;
    }

    /// <summary>
    /// Ends a refresh, and with it the hiding: a quest list the player asks
    /// for afterwards is theirs to read.
    /// </summary>
    private void Finish()
    {
        IsRefreshing = false;
        _hideOutput = false;
    }

    private void SubmitRequest()
    {
        if (_attemptsRemaining <= 0)
        {
            Finish();
            return;
        }
        _attemptsRemaining--;
        _silenceSeconds = 0d;
        host.Automation.Chat.Submit("/myquests");
    }

    private static string Normalize(string key) => key.Trim().ToLowerInvariant();

    [GeneratedRegex(
        "(?<key>\\S+) \\- (?<solves>\\d+) solves \\((?<completedOn>\\d{0,11})\\)\"?((?<description>.*)\" (?<maxSolves>.*) (?<repeatTime>\\d{0,11}))?.*$",
        RegexOptions.CultureInvariant,
        100)]
    private static partial Regex MyQuestLine();

    [GeneratedRegex(
        "(killtask|killcount|slayerquest|totalgolem.*dead|(kills$))",
        RegexOptions.CultureInvariant,
        100)]
    private static partial Regex KillTaskKey();

    private readonly record struct QuestFlag(
        int Solves,
        int MaxSolves,
        DateTimeOffset CompletedOn,
        long RepeatSeconds);
}
