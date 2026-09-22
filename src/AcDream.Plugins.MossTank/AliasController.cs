using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Puts the alias list between the chat bar and the world: one interceptor
/// on the chat surface holds every typed line against the list, stores what
/// the patterns captured where the expressions read it, and runs the
/// actions through the expression runtime and the chat bar.
/// </summary>
/// <remarks>
/// <para>
/// The interceptor is consulted for what the player types and for what the
/// actions themselves send. An action's output is let through untouched:
/// an alias whose output matched itself would otherwise call itself until
/// the stack ran out, since sending is synchronous. A rewrite is different;
/// the host feeds it back through the interceptor, so one alias may rewrite
/// into another. That chain is watched here: the moment it produces a line
/// it has produced before, the line is let through as it stands and the
/// player is told once, rather than leaving it to the host's cut-off.
/// </para>
/// <para>
/// The list and the switch are read live, on every line, so an edit on the
/// page counts on the next thing typed; the list is parsed again only when
/// its lines change, and a line that does not parse is reported once per
/// change rather than once per keystroke.
/// </para>
/// </remarks>
internal sealed class AliasController : IAliasActionHost, IDisposable
{
    private readonly IPluginChat _chat;
    private readonly MossTankExpressionRuntime _expressions;
    private readonly IPluginLogger _log;
    private readonly Func<bool> _enabled;
    private readonly Func<IReadOnlyList<string>> _lines;
    private readonly HashSet<string> _rewriteChain = new(StringComparer.Ordinal);
    private IDisposable? _registration;
    private IReadOnlyList<string> _parsedLines = [];
    private AliasTable _table = AliasTable.Empty;
    private string? _lastRewrite;
    private bool _dispatching;
    private bool _disposed;

    internal AliasController(
        IPluginChat chat,
        MossTankExpressionRuntime expressions,
        IPluginLogger log,
        Func<bool> enabled,
        Func<IReadOnlyList<string>> lines)
    {
        _chat = chat ?? throw new ArgumentNullException(nameof(chat));
        _expressions = expressions ?? throw new ArgumentNullException(nameof(expressions));
        _log = log ?? throw new ArgumentNullException(nameof(log));
        _enabled = enabled ?? throw new ArgumentNullException(nameof(enabled));
        _lines = lines ?? throw new ArgumentNullException(nameof(lines));
    }

    /// <summary>Installs the one interceptor; a second call is a no-op.</summary>
    internal void Attach()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        _registration ??= _chat.RegisterInputInterceptor(Intercept);
    }

    /// <summary>The list as last parsed, for whoever wants to show it.</summary>
    internal AliasTable Table => CurrentTable();

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed)
            return;
        _disposed = true;
        _registration?.Dispose();
        _registration = null;
    }

    /// <inheritdoc />
    /// <remarks>
    /// The same session variables a meta chat capture fills, written the
    /// same way, so an alias expression reads them as a meta does.
    /// </remarks>
    public void SetCapture(string name, string? value)
    {
        if (value is null)
            _expressions.State.Clear(ExpressionVariableScope.Session, name);
        else
            _expressions.State.Set(
                ExpressionVariableScope.Session,
                name,
                ExpressionValue.String(value));
    }

    /// <inheritdoc />
    public string Evaluate(string expression) =>
        _expressions.Evaluate(expression).ToDisplayString();

    private PluginChatInputDecision Intercept(string line)
    {
        if (_disposed || _dispatching || !_enabled())
            return PluginChatInputDecision.Pass;

        AliasTable table = CurrentTable();
        if (table.Aliases.Count == 0)
            return PluginChatInputDecision.Pass;

        // A line that is the rewrite just handed back is the next lap of
        // the same chain; anything else starts a chain of its own.
        if (!string.Equals(line, _lastRewrite, StringComparison.Ordinal))
        {
            _rewriteChain.Clear();
            _rewriteChain.Add(line);
        }
        _lastRewrite = null;

        AliasResolution result = table.Resolve(line, this);
        foreach (string problem in result.Problems)
            _log.Error(problem);

        switch (result.Outcome)
        {
            case AliasOutcome.Suppress:
                Dispatch(result.Submit);
                return PluginChatInputDecision.Suppress;
            case AliasOutcome.Rewrite:
                string rewrite = result.Rewrite!;
                if (!_rewriteChain.Add(rewrite))
                {
                    _log.Warn(
                        $"An alias rewrote '{line}' into '{rewrite}', which the same "
                        + "chain of aliases has already produced; it is sent as it "
                        + $"stands. Check the alias list for '{Culprit(table, rewrite)}'.");
                    Dispatch(result.Submit);
                    return PluginChatInputDecision.Pass;
                }
                _lastRewrite = rewrite;
                Dispatch(result.Submit);
                return PluginChatInputDecision.Rewrite(rewrite);
            default:
                return PluginChatInputDecision.Pass;
        }
    }

    /// <summary>
    /// Sends the outputs that go out on their own. They pass through this
    /// same interceptor on the way; the flag lets them through untouched.
    /// </summary>
    private void Dispatch(IReadOnlyList<string> outputs)
    {
        if (outputs.Count == 0)
            return;
        _dispatching = true;
        try
        {
            foreach (string output in outputs)
                _chat.Submit(output);
        }
        finally
        {
            _dispatching = false;
        }
    }

    private AliasTable CurrentTable()
    {
        IReadOnlyList<string> lines = _lines();
        if (ReferenceEquals(lines, _parsedLines) || lines.SequenceEqual(_parsedLines, StringComparer.Ordinal))
            return _table;
        _parsedLines = [.. lines];
        _table = AliasTable.Parse(_parsedLines);
        foreach (string problem in _table.Problems)
            _log.Warn("Alias list, " + problem);
        return _table;
    }

    /// <summary>The first alias that matches the line the chain came back to.</summary>
    private static string Culprit(AliasTable table, string line)
    {
        foreach (Alias alias in table.Aliases)
        {
            try
            {
                if (alias.Regex.IsMatch(line))
                    return alias.Pattern;
            }
            catch (System.Text.RegularExpressions.RegexMatchTimeoutException)
            {
                // A pattern that runs away here is already reported by the
                // match that got the chain this far.
            }
        }
        return line;
    }
}
