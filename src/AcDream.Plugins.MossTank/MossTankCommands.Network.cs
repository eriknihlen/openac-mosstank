using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The three commands that talk to the other clients this computer is
/// running: broadcast a line to all of them, broadcast it to the ones
/// carrying a label, and list who is out there.
/// </summary>
/// <remarks>
/// <para>
/// The reference plugin matched these with regular expressions. The same
/// grammar is written out here by hand -- leading digits are the delay, the
/// rest of the line is the command, and a tag list is comma separated with
/// quotes around any tag holding a space -- because the argument text
/// reaches this plugin already split from its verb.
/// </para>
/// <para>
/// One difference from the reference is worth naming: the client delivers a
/// received line to its own command bus by itself, and the sending client is
/// never handed back its own broadcast. So the sender runs the line here,
/// locally, exactly as the reference did, and the host call carries it to
/// everybody else.
/// </para>
/// </remarks>
internal sealed partial class MossTankPanel
{
    /// <summary>
    /// <c>/ub bc [millisecondDelay] &lt;command&gt;</c>: runs a command line
    /// on every client on this computer, this one included.
    /// </summary>
    private void HandleBroadcastCommand(string arguments)
    {
        if (!TryReadDelayAndCommand(arguments, out int delay, out string command))
            return;
        if (command.Length == 0)
        {
            WriteUbBadSyntax("bc");
            return;
        }

        WriteUb(Invariant(
            $"Broadcasting command to all clients: \"{command}\" with delay inbetween of {delay}ms"));
        QueueLocalCommand(command);
        if (!_host.Automation.Network.BroadcastCommand(command, [], delay))
        {
            WriteUbError("Unable to broadcast command to the other clients.");
            return;
        }
        foreach (PluginNetworkClient client in OtherClients())
        {
            WriteUb(Invariant(
                $"Sending {client.Name}: \"{command}\" with delay inbetween of {delay}ms"));
        }
    }

    /// <summary>
    /// <c>/ub bct &lt;tags&gt; [millisecondDelay] &lt;command&gt;</c>: the
    /// same, aimed at the clients answering to one of the named labels. This
    /// client runs the line itself only when one of those labels is its own,
    /// which is what the reference asked before it ran the line locally.
    /// </summary>
    private void HandleTaggedBroadcastCommand(string arguments)
    {
        if (!TrySplitTagList(arguments, out List<string> tags, out string rest))
        {
            WriteUbBadSyntax("bct");
            return;
        }
        if (!TryReadDelayAndCommand(rest, out int delay, out string command))
            return;
        if (tags.Count == 0)
        {
            WriteUbError("You must specify at least one tag to send the command to.");
            return;
        }
        if (command.Length == 0)
        {
            WriteUbBadSyntax("bct");
            return;
        }

        WriteUb(Invariant(
            $"Broadcasting command to clients with tags ({string.Join(",", tags)}): \"{command}\" with delay inbetween of {delay}ms"));
        // The labels are compared ignoring case, as the client's own delivery
        // does, so that the sender's decision to run the line and a peer's
        // decision to run it cannot disagree over the spelling of one tag.
        IReadOnlyList<string> own = OwnNetworkTags();
        if (tags.Any(tag => own.Contains(tag, StringComparer.OrdinalIgnoreCase)))
            QueueLocalCommand(command);
        if (!_host.Automation.Network.BroadcastCommand(command, tags, delay))
            WriteUbError("Unable to broadcast command to the other clients.");
    }

    /// <summary>
    /// <c>/ub netclients [tag]</c>: who is on this computer, optionally only
    /// those answering to one of the named labels. This character is listed
    /// too -- the host reports the others only.
    /// </summary>
    private void HandleNetClientsCommand(string arguments)
    {
        List<string> tags = [.. SplitTags(arguments.Trim())];
        bool showedClients = false;
        ICharacterInfo me = _host.Automation.Character;
        if (me.ObjectId != 0u)
        {
            IReadOnlyList<string> own = OwnNetworkTags();
            if (Matches(tags, own))
            {
                WriteUb(NetClientLine(0u, me.Name, own));
                showedClients = true;
            }
        }
        foreach (PluginNetworkClient client in OtherClients())
        {
            if (!Matches(tags, client.Tags))
                continue;
            WriteUb(NetClientLine(client.ClientId, client.Name, client.Tags));
            showedClients = true;
        }
        if (!showedClients)
            WriteUb("No net clients to show");

        static bool Matches(List<string> wanted, IReadOnlyList<string> held) =>
            wanted.Count == 0
            || wanted.Any(tag => held.Contains(tag, StringComparer.OrdinalIgnoreCase));
    }

    /// <summary>
    /// The other clients, in the order they will run a broadcast line: the
    /// host orders them by client id, with the sender first.
    /// </summary>
    private IEnumerable<PluginNetworkClient> OtherClients() =>
        _host.Automation.Network.CaptureClients().OrderBy(static client => client.ClientId);

    /// <summary>
    /// One client as the list prints it. The reference printed whatever its
    /// own client record said; the labels are spelled out here because a tag
    /// is the only reason to ask for this list in the first place.
    /// </summary>
    private static string NetClientLine(
        uint clientId,
        string name,
        IReadOnlyList<string> tags) =>
        tags.Count == 0
            ? Invariant($"ClientData<{clientId}, {name}>")
            : Invariant($"ClientData<{clientId}, {name}> [{string.Join(", ", tags)}]");

    /// <summary>The labels this character answers to, as the page holds them.</summary>
    private IReadOnlyList<string> OwnNetworkTags() =>
        _ubCatalog.Require("Networking.Tags").Get().Items;

    /// <summary>
    /// Runs a line on this client now, down the same path a scheduled line
    /// takes, so that a broadcast and a <c>/ub delay 0</c> reach the command
    /// bus the same way.
    /// </summary>
    private void QueueLocalCommand(string command)
    {
        ScheduleUbCommand(command, 0d);
    }

    /// <summary>
    /// Reads the leading digits as a delay in milliseconds and the rest of
    /// the line as the command. False once the refusal has been printed,
    /// which is a delay that is digits but not a number this client can hold.
    /// </summary>
    private bool TryReadDelayAndCommand(
        string arguments,
        out int delay,
        out string command)
    {
        delay = 0;
        int digits = 0;
        while (digits < arguments.Length && char.IsAsciiDigit(arguments[digits]))
            digits++;
        string delayText = arguments[..digits];
        command = arguments[digits..];
        if (command.StartsWith(' '))
            command = command[1..];
        if (delayText.Length != 0
            && !int.TryParse(
                delayText,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out delay))
        {
            WriteUbError($"Unable to broadcast command, invalid delay: {delayText}");
            return false;
        }
        // The reference also refused a delay below zero. Only digits are read
        // as the delay here, as there, so that refusal cannot be reached and
        // is not written out.
        return true;
    }

    /// <summary>
    /// Splits a leading comma-separated tag list off the front of a line. A
    /// tag holding a space is written in quotes, so the list ends at the
    /// first space outside a pair of them. False when the line carries no
    /// space at all, which means it is a tag list and nothing else.
    /// </summary>
    private static bool TrySplitTagList(
        string arguments,
        out List<string> tags,
        out string rest)
    {
        tags = [];
        rest = string.Empty;
        bool quoted = false;
        int end = -1;
        for (int index = 0; index < arguments.Length; index++)
        {
            char letter = arguments[index];
            if (letter == '"')
                quoted = !quoted;
            else if (letter == ' ' && !quoted)
            {
                end = index;
                break;
            }
        }
        if (end < 0)
            return false;
        rest = arguments[(end + 1)..];
        foreach (string piece in SplitTags(arguments[..end]))
            tags.Add(piece);
        return true;
    }

    /// <summary>The tags in one list, unquoted, with the empty ones dropped.</summary>
    private static IEnumerable<string> SplitTags(string list)
    {
        bool quoted = false;
        int start = 0;
        for (int index = 0; index <= list.Length; index++)
        {
            if (index < list.Length)
            {
                char letter = list[index];
                if (letter == '"')
                    quoted = !quoted;
                if (letter != ',' || quoted)
                    continue;
            }
            string piece = list[start..index].Trim().Trim('"').Trim();
            start = index + 1;
            if (piece.Length != 0)
                yield return piece;
        }
    }
}
