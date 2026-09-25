using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// <c>prepclick {stop|yes &lt;secondstowatch&gt;|no &lt;secondstowatch&gt;}</c>:
/// answers the first yes/no confirmation the server raises within a window,
/// once, then lets go. It listens only while armed, so a confirmation raised
/// before the command is never its business.
/// </summary>
/// <remarks>
/// The reference watched the screen for the dialog; this client says when a
/// confirmation arrives and answers it through its own dialog automation, the
/// same path the Yes and No buttons take. The wording of every line repeats
/// the reference's, tag included ("[UB] PrepClick: Will click yes ..."),
/// since a macro may be waiting on it.
/// </remarks>
internal sealed partial class PrepClickController : IDisposable
{
    internal const string Usage =
        "/ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}";

    /// <summary>
    /// The tag the reference puts before every line this tool prints: the
    /// plugin's tag, then the tool's name.
    /// </summary>
    internal const string LinePrefix = UbChat.Tag + UbChat.Tools.PrepClick + ": ";

    /// <summary>The longest window the command will watch, in seconds.</summary>
    private const double MaximumSeconds = 3600d;

    private readonly IPluginHost _host;
    private readonly Action<string> _write;
    private Action<PluginConfirmation>? _confirmationHandler;
    private bool _answerYes;
    private double _seconds;
    private double _remaining;

    internal PrepClickController(IPluginHost host, Action<string> write)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _write = write ?? throw new ArgumentNullException(nameof(write));
    }

    /// <summary>Whether a confirmation would be answered now.</summary>
    internal bool IsArmed => _confirmationHandler is not null;

    [GeneratedRegex(
        @"^(?<choice>yes|no|stop)( (?<seconds>\d+))?$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant)]
    private static partial Regex CommandPattern();

    /// <summary>Runs one <c>prepclick</c> line.</summary>
    internal void Command(string arguments)
    {
        Match match = CommandPattern().Match(arguments.Trim());
        if (!match.Success)
        {
            _write("Bad command syntax");
            _write("Usage: " + Usage);
            return;
        }

        string choice = match.Groups["choice"].Value.ToLowerInvariant();
        if (choice == "stop")
        {
            if (!IsArmed)
            {
                Say("Message boxes are not currently being watched");
                return;
            }
            double passed = Math.Round(_seconds - _remaining, 2);
            Say(
                "Stopping... "
                + passed.ToString(CultureInfo.InvariantCulture)
                + "s passed out of expected "
                + _seconds.ToString(CultureInfo.InvariantCulture)
                + "s");
            Disarm();
            return;
        }

        // The window is optional in the grammar, and the reference reads a
        // missing one as zero: armed, and over at the next tick.
        double seconds = match.Groups["seconds"].Success
            ? double.Parse(
                match.Groups["seconds"].Value,
                NumberStyles.None,
                CultureInfo.InvariantCulture)
            : 0d;
        if (seconds > MaximumSeconds)
        {
            Say(match.Groups["seconds"].Value
                + " is not a valid number of seconds to wait");
            return;
        }

        _seconds = seconds;
        _remaining = seconds;
        _answerYes = choice == "yes";
        if (_confirmationHandler is null)
        {
            _confirmationHandler = OnConfirmationRequested;
            _host.Events.ConfirmationRequested += _confirmationHandler;
        }
        Say(
            $"Will click {choice} on the next dialog to appear within "
            + $"{seconds.ToString(CultureInfo.InvariantCulture)} seconds");
    }

    /// <summary>Counts the window down; at its end the watch is dropped.</summary>
    internal void OnTick(double elapsedSeconds)
    {
        if (!IsArmed)
            return;
        _remaining -= Math.Max(0d, elapsedSeconds);
        if (_remaining >= 0d)
            return;
        Say(
            "Time has expired: "
            + _seconds.ToString(CultureInfo.InvariantCulture));
        Disarm();
    }

    /// <summary>
    /// Drops the watch without a word: the session it was armed in is gone.
    /// </summary>
    internal void Reset() => Disarm();

    public void Dispose() => Disarm();

    private void OnConfirmationRequested(PluginConfirmation confirmation)
    {
        // The reference answers only inside a window that has length, and
        // stops watching whichever way it went: one confirmation per command.
        bool answer = _seconds > 0d;
        bool accept = _answerYes;
        Disarm();
        if (!answer)
            return;
        Say((accept ? "Click Yes on " : "Click No on ") + confirmation.Text);
        _host.Automation.Dialogs.Answer(confirmation.ContextId, accept);
    }

    /// <summary>One of the tool's own lines, under the tool's tag.</summary>
    private void Say(string text) => _write(LinePrefix + text);

    private void Disarm()
    {
        if (_confirmationHandler is not null)
            _host.Events.ConfirmationRequested -= _confirmationHandler;
        _confirmationHandler = null;
        _seconds = 0d;
        _remaining = 0d;
        _answerYes = false;
    }
}
