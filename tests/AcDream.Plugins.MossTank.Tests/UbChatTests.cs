namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The four shapes of a UtilityBelt line, as the reference's logger makes
/// them. Profiles match these byte for byte, so the tag, "Error: ", the
/// tool name and each separator are pinned.
/// </summary>
public sealed class UbChatTests
{
    /// <summary>
    /// Mutation: dropping the space after the tag, putting "Error:" after the
    /// tool name, or tagging a line twice fails a row.
    /// </summary>
    [Fact]
    public void EachShapeIsTheReferencesOwn()
    {
        Assert.Equal("[UB] Quitting Client", UbChat.Line("Quitting Client"));
        Assert.Equal("[UB] Error: Could not find player Bob", UbChat.Error("Could not find player Bob"));
        Assert.Equal(
            "[UB] PrepClick: Will click yes on the next dialog to appear within 5 seconds",
            UbChat.Tool(UbChat.Tools.PrepClick, "Will click yes on the next dialog to appear within 5 seconds"));
        Assert.Equal(
            "[UB] Error: Jumper: You are already jumping. try again later.",
            UbChat.ToolError(UbChat.Tools.Jumper, "You are already jumping. try again later."));
        // The logger leaves a line that already carries the tag alone.
        Assert.Equal("[UB] Already tagged", UbChat.Line("[UB] Already tagged"));
    }

    /// <summary>
    /// The reference's logger prints an ordinary line as System (5), an
    /// error as Help (15) and a debug line as Abuse (14), and a meta's chat
    /// capture reads that number as the line's colour. Mutation: printing
    /// through the plain system-message path (class 0) fails every row, and
    /// a debug line that ignores the setting fails the last.
    /// </summary>
    [Fact]
    public void EachKindIsPrintedInTheReferencesTextClass()
    {
        var chat = new RecordingChat();

        UbChat.Post(chat, UbChat.Line("Quitting Client"));
        UbChat.Post(chat, UbChat.ToolError(UbChat.Tools.Jumper, "You are already jumping. try again later."));
        UbChat.Post(chat, "Untagged");
        UbChat.PostDebug(chat, debug: true, "Scheduling command `x` with delay of 5ms");
        UbChat.PostToolDebug(chat, debug: true, UbChat.Tools.Plugin, "Attempting to use portal Gate");
        UbChat.PostDebug(chat, debug: false, "never printed");

        Assert.Equal(
            [
                ("[UB] Quitting Client", 5),
                ("[UB] Error: Jumper: You are already jumping. try again later.", 15),
                ("[UB] Untagged", 5),
                ("[UB] Scheduling command `x` with delay of 5ms", 14),
                ("[UB] Plugin: Attempting to use portal Gate", 14),
            ],
            chat.Posted);
    }

    private sealed class RecordingChat : AcDream.Plugin.Abstractions.IPluginChat
    {
        public List<(string Text, int Kind)> Posted { get; } = [];

        public void PostSystemMessage(string text) => Posted.Add((text, 0));

        public void PostMessage(string text, int logTextType) => Posted.Add((text, logTextType));

        public bool Submit(string text) => true;

        public bool Compose(string text) => true;
    }
}
