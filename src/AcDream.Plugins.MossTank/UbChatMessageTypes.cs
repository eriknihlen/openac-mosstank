namespace AcDream.Plugins.MossTank;

/// <summary>
/// The client's own text classes, by the names a macro author knows them by.
/// Each one carries its own colour in the chat window, so printing a line in
/// every class is how the reference shows which number gives which colour.
/// </summary>
internal readonly record struct UbChatMessageType(string Name, int Value);

/// <summary>The catalogue <c>/ub printcolors</c> walks.</summary>
internal static class UbChatMessageTypes
{
    /// <summary>
    /// Every text class, in ascending order of the number the client keys
    /// them by. The gaps (0x01, 0x1A) are classes the client does not use.
    /// </summary>
    internal static IReadOnlyList<UbChatMessageType> All { get; } =
    [
        new("Broadcast", 0x00),
        new("Speech", 0x02),
        new("Tell", 0x03),
        new("OutgoingTell", 0x04),
        new("System", 0x05),
        new("Combat", 0x06),
        new("Magic", 0x07),
        new("Channel", 0x08),
        new("ChannelSend", 0x09),
        new("Social", 0x0A),
        new("SocialSend", 0x0B),
        new("Emote", 0x0C),
        new("Advancement", 0x0D),
        new("Abuse", 0x0E),
        new("Help", 0x0F),
        new("Appraisal", 0x10),
        new("Spellcasting", 0x11),
        new("Allegiance", 0x12),
        new("Fellowship", 0x13),
        new("WorldBroadcast", 0x14),
        new("CombatEnemy", 0x15),
        new("CombatSelf", 0x16),
        new("Recall", 0x17),
        new("Craft", 0x18),
        new("Salvaging", 0x19),
        new("General", 0x1B),
        new("Trade", 0x1C),
        new("LFG", 0x1D),
        new("Roleplay", 0x1E),
        new("AdminTell", 0x1F),
        new("Olthoi", 0x20),
    ];
}
