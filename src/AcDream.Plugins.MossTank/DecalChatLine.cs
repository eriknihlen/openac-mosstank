using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The chat line the reference macro reads: the host's own wording of it, or,
/// from an older host that does not word lines, rebuilt from the parts it
/// hands a plugin.
/// </summary>
/// <remarks>
/// The reference macro matches its chat conditions against the whole line the
/// game's chat window prints: the channel prefix, the speaker wrapped in the
/// client's tell link (<c>&lt;Tell:IIDString:id:Name&gt;Name&lt;\Tell&gt;</c>),
/// the verb and the quoted words. The host hands that exact line over as
/// <see cref="PluginChatMessage.DisplayText"/>, and it is used as it stands.
/// An older host leaves it empty and delivers speech, tells and channel lines
/// as the speaker and the words apart; for that host the sentence is put back
/// together here, worded the way the game client words each kind of line.
/// Every other line (system text, combat, a creature's emote) arrives whole
/// and is used as it stands.
/// </remarks>
internal static class DecalChatLine
{
    // The host's line kinds, as it numbers them on PluginChatMessage.Kind.
    private const int LocalSpeechKind = 0;
    private const int RangedSpeechKind = 1;
    private const int ChannelKind = 2;
    private const int TellKind = 3;
    private const int EmoteKind = 6;
    private const int SoulEmoteKind = 7;

    /// <summary>The client's text type of the fellowship channel.</summary>
    private const int FellowshipLogTextType = 0x13;

    // A speaker whose object id falls in this range is a player: the game
    // client links a player's name so it can be clicked to answer, and leaves
    // a creature's name plain.
    private const uint FirstPlayerObjectId = 0x5000_0001u;
    private const uint LastPlayerObjectId = 0x6FFF_FFFFu;

    /// <summary>
    /// The line as the game's chat window prints it, without its closing
    /// line break (the reference macro trims that off before matching).
    /// </summary>
    /// <param name="message">The line as the host delivered it.</param>
    /// <param name="ownObjectId">The character's own object id.</param>
    /// <param name="ownName">The character's own name.</param>
    public static string Compose(in PluginChatMessage message, uint ownObjectId, string ownName)
    {
        if (!string.IsNullOrEmpty(message.DisplayText))
            return message.DisplayText;
        string text = message.Text ?? string.Empty;
        string sender = message.Sender ?? string.Empty;
        return message.Kind switch
        {
            LocalSpeechKind => IsOwn(message, ownObjectId)
                ? $"You say, \"{text}\""
                : $"{Speaker(sender, message.SenderObjectId)} says, \"{text}\"",
            // Shouted speech has no sentence of its own for the speaker: the
            // game client prints a shout the character made under its own
            // name, the way anyone else hears it.
            RangedSpeechKind =>
                $"{Speaker(IsOwn(message, ownObjectId) ? ownName : sender, message.SenderObjectId)} says, \"{text}\"",
            ChannelKind => ChannelLine(message, sender, text),
            TellKind => TellLine(message, sender, text, ownObjectId),
            // Someone else's emote is their name and the words; the
            // character's own pose line arrives as its own sentence.
            EmoteKind or SoulEmoteKind when !IsOwnSpeaker(sender) => $"{sender} {text}",
            _ => text,
        };
    }

    private static string ChannelLine(in PluginChatMessage message, string sender, string text)
    {
        bool own = IsOwnSpeaker(sender);
        string channel = message.ChannelName ?? string.Empty;
        if (channel.Length == 0 && message.LogTextType == FellowshipLogTextType)
            channel = "Fellowship";
        if (channel.Length == 0)
        {
            // A numbered channel other than the fellowship (patron, vassals,
            // co-vassals, the advocate channels) is worded by its number, and
            // a host that does not word its lines does not hand the number to
            // a plugin either. The words are the most that can be matched.
            return text;
        }
        // A channel speaker is linked with id 0: the channel message carries
        // only the name.
        return own
            ? $"[{channel}] You say, \"{text}\""
            : $"[{channel}] {Link(0u, sender)} says, \"{text}\"";
    }

    private static string TellLine(
        in PluginChatMessage message,
        string sender,
        string text,
        uint ownObjectId)
    {
        if (message.SenderObjectId == 0u)
            return $"You tell {sender}, \"{text}\"";
        // A tell the character sent to itself comes back from itself.
        if (ownObjectId != 0u && message.SenderObjectId == ownObjectId)
            return $"You think, \"{text}\"";
        return $"{Speaker(sender, message.SenderObjectId)} tells you, \"{text}\"";
    }

    private static string Speaker(string name, uint objectId) =>
        objectId >= FirstPlayerObjectId && objectId <= LastPlayerObjectId
            ? Link(objectId, name)
            : name;

    /// <summary>The client's tell link; the id is written in decimal.</summary>
    private static string Link(uint objectId, string name) =>
        $"<Tell:IIDString:{objectId}:{name}>{name}<\\Tell>";

    private static bool IsOwn(in PluginChatMessage message, uint ownObjectId) =>
        IsOwnSpeaker(message.Sender)
        || (ownObjectId != 0u && message.SenderObjectId == ownObjectId);

    // The host names the character's own speech "You", or leaves the speaker
    // out on a line the server sends back to its sender.
    private static bool IsOwnSpeaker(string? sender) =>
        string.IsNullOrEmpty(sender) || sender == "You";
}
