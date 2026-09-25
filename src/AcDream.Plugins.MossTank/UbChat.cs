using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The shape of every line the UtilityBelt side prints, in the reference's
/// exact words: its logger puts "[UB] " before the text (unless the text
/// already begins with the tag), an error is "Error: " then the text, and a
/// line a tool prints for itself carries the tool's name and a colon first.
/// </summary>
/// <remarks>
/// <para>
/// A tool's name is the name its settings are stored under, not the display
/// name the tool declares: the prepclick tool prints "[UB] PrepClick: ...",
/// the equipment tool "[UB] EquipmentManager: ...". Macros wait on these
/// lines with chat triggers such as <c>^\[UB\] Added item to buy list</c>
/// and <c>\[UB\] Error\: Could not find player</c>, so the tag, the colon
/// and the space after it are part of what the line means.
/// </para>
/// <para>
/// Each line is printed in the text class the reference's logger gives its
/// kind, and a meta's chat capture sees that same class as the line's
/// colour: an ordinary line is System, an error Help, a debug line Abuse.
/// A debug line is printed only while <c>Plugin.Debug</c> is on; with it
/// off the reference only writes it to its log file.
/// </para>
/// <para>
/// Each kind of line has its own display: a switch that keeps its lines out
/// of chat and the text class they are printed in, the reference's
/// <c>Plugin.GenericMessageDisplay</c>, <c>DebugMessageDisplay</c>,
/// <c>ExpressionMessageDisplay</c> and <c>ErrorMessageDisplay</c>. The
/// classes above are their defaults. A chat with no displays bound to it
/// (<see cref="Bind"/>) prints every line in its default class.
/// </para>
/// </remarks>
internal static class UbChat
{
    /// <summary>The four kinds of line, each with its own display.</summary>
    internal enum Kind
    {
        Generic,
        Debug,
        Expression,
        Error,
    }

    /// <summary>Whether a kind's lines are shown, and the class they print in.</summary>
    internal readonly record struct Display(bool Show, int ChatType);

    private static readonly System.Runtime.CompilerServices.ConditionalWeakTable<
        IPluginChat,
        Func<Kind, Display>> Displays = new();

    /// <summary>
    /// Makes every line posted to <paramref name="chat"/> follow
    /// <paramref name="display"/>, read afresh for each line.
    /// </summary>
    internal static void Bind(IPluginChat chat, Func<Kind, Display> display)
    {
        ArgumentNullException.ThrowIfNull(chat);
        ArgumentNullException.ThrowIfNull(display);
        Displays.AddOrUpdate(chat, display);
    }

    /// <summary>A kind's display as the reference ships it.</summary>
    internal static Display DefaultDisplay(Kind kind) => new(true, kind switch
    {
        Kind.Debug => DebugChatType,
        Kind.Error => ErrorChatType,
        _ => GenericChatType,
    });

    private static Display DisplayOf(IPluginChat chat, Kind kind) =>
        Displays.TryGetValue(chat, out Func<Kind, Display>? display)
            ? display(kind)
            : DefaultDisplay(kind);

    /// <summary>
    /// The name a kind's display settings are held under, e.g.
    /// "Plugin.ErrorMessageDisplay".
    /// </summary>
    internal static string DisplaySetting(Kind kind) => $"Plugin.{kind}MessageDisplay";
    /// <summary>The plugin's tag, with the space that follows it.</summary>
    internal const string Tag = "[UB] ";

    /// <summary>The text class of an ordinary line (System).</summary>
    internal const int GenericChatType = 0x05;

    /// <summary>The text class of a debug line (Abuse).</summary>
    internal const int DebugChatType = 0x0E;

    /// <summary>The text class of an error line (Help).</summary>
    internal const int ErrorChatType = 0x0F;

    /// <summary>The setting that shows the debug lines.</summary>
    internal const string DebugSetting = "Plugin.Debug";

    private const string ErrorLead = Tag + "Error: ";

    /// <summary>A plain line: the tag, then the text.</summary>
    internal static string Line(string text) =>
        text.StartsWith("[UB]", StringComparison.Ordinal) ? text : Tag + text;

    /// <summary>An error line: the tag, "Error: ", then the text.</summary>
    internal static string Error(string text) => Line("Error: " + text);

    /// <summary>A line a tool prints for itself: the tag, the tool, the text.</summary>
    internal static string Tool(string tool, string text) => Line(tool + ": " + text);

    /// <summary>An error a tool reports for itself.</summary>
    internal static string ToolError(string tool, string text) => Error(tool + ": " + text);

    /// <summary>
    /// Prints a finished line through its kind's display: an error line or
    /// a generic one, by its lead. Only the logger's error path starts a line
    /// with the error lead, so the lead is the kind.
    /// </summary>
    internal static void Post(IPluginChat chat, string line)
    {
        ArgumentNullException.ThrowIfNull(chat);
        string tagged = Line(line);
        PostAs(chat, tagged.StartsWith(ErrorLead, StringComparison.Ordinal)
            ? Kind.Error
            : Kind.Generic, tagged);
    }

    /// <summary>
    /// Prints an expression line, what /ub mexec says, through the
    /// expression display.
    /// </summary>
    internal static void PostExpression(IPluginChat chat, string text)
    {
        ArgumentNullException.ThrowIfNull(chat);
        PostAs(chat, Kind.Expression, Line(text));
    }

    /// <summary>
    /// Prints one message of several lines, as the reference's logger prints
    /// a message with line breaks in it: the tag on the first line only, and
    /// every line in the kind's class and behind the kind's switch, read once
    /// for the whole message so that its lines cannot part company. An error
    /// message's first line carries the error lead; a blank line is not
    /// printed.
    /// </summary>
    /// <param name="chat">The chat to print to.</param>
    /// <param name="kind">The kind the whole message is.</param>
    /// <param name="heading">The first line, without the tag.</param>
    /// <param name="continuation">The lines below it, exactly as written.</param>
    internal static void PostMessage(
        IPluginChat chat,
        Kind kind,
        string heading,
        IEnumerable<string> continuation)
    {
        ArgumentNullException.ThrowIfNull(chat);
        ArgumentNullException.ThrowIfNull(continuation);
        Display display = DisplayOf(chat, kind);
        if (!display.Show)
            return;
        chat.PostMessage(
            kind == Kind.Error ? Error(heading) : Line(heading),
            display.ChatType);
        foreach (string line in continuation)
        {
            if (line.Length != 0)
                chat.PostMessage(line, display.ChatType);
        }
    }

    /// <summary>
    /// Says something to yourself the way the reference does: a real tell to
    /// the character's own name, handed to the chat bar, so what reaches the
    /// chat window -- and every chat trigger watching it -- is the server's
    /// own "You think" line in the tell class. False when the client took no
    /// line, which is also when the reference's tell goes nowhere; nothing is
    /// printed in its place.
    /// </summary>
    internal static bool Think(IAutomationSurface automation, string text)
    {
        ArgumentNullException.ThrowIfNull(automation);
        string name = automation.Character.Name;
        return name.Length != 0 && automation.Chat.Submit($"/t {name}, {text}");
    }

    /// <summary>
    /// A line a tool either thinks or prints, as its think switch says: a
    /// tell to yourself when <paramref name="think"/> is on, a plain tagged
    /// line otherwise.
    /// </summary>
    internal static void ThinkOrWrite(IAutomationSurface automation, string text, bool think)
    {
        ArgumentNullException.ThrowIfNull(automation);
        if (think)
            Think(automation, text);
        else
            Post(automation.Chat, Line(text));
    }

    private static void PostAs(IPluginChat chat, Kind kind, string tagged)
    {
        Display display = DisplayOf(chat, kind);
        if (display.Show)
            chat.PostMessage(tagged, display.ChatType);
    }

    /// <summary>
    /// A debug line: printed, tagged and in the debug text class, only while
    /// the debug setting is on.
    /// </summary>
    internal static void PostDebug(IPluginChat chat, bool debug, string text)
    {
        ArgumentNullException.ThrowIfNull(chat);
        if (debug)
            PostAs(chat, Kind.Debug, Line(text));
    }

    /// <summary>A tool's debug line: the tool's name and a colon first.</summary>
    internal static void PostToolDebug(IPluginChat chat, bool debug, string tool, string text) =>
        PostDebug(chat, debug, tool + ": " + text);

    /// <summary>
    /// The tools' names, as their lines carry them: each is the name the
    /// tool's settings are held under.
    /// </summary>
    internal static class Tools
    {
        internal const string Assessor = "Assessor";
        internal const string AutoTinker = "AutoTinker";
        internal const string AutoVendor = "AutoVendor";
        internal const string Counter = "Counter";
        internal const string EquipmentManager = "EquipmentManager";
        internal const string InventoryManager = "InventoryManager";
        internal const string Jumper = "Jumper";
        internal const string Plugin = "Plugin";
        internal const string PrepClick = "PrepClick";
        internal const string QuestTracker = "QuestTracker";
        internal const string VTank = "VTank";
    }
}
