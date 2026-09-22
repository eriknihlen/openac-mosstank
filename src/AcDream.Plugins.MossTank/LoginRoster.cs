using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Why a selector named nobody on the account's character list.</summary>
internal enum LoginRosterFailure
{
    /// <summary>It named somebody.</summary>
    None,

    /// <summary>The account's character list has not arrived yet.</summary>
    Empty,

    /// <summary>No character's name contains the text given.</summary>
    NoSuchName,

    /// <summary>
    /// An index counted from where the character stands now needs the current
    /// character to be on the list, and it is not — which is what a character
    /// created since the list arrived looks like.
    /// </summary>
    NoCurrentCharacter,

    /// <summary>
    /// The index landed off either end of the list and no wrap was asked for.
    /// </summary>
    OutOfRange,

    /// <summary>
    /// The character named is scheduled for deletion, so it cannot be played.
    /// </summary>
    PendingDelete,
}

/// <summary>
/// The account's characters in the one order everything here counts them in:
/// alphabetically by name. Two numbers describe the same character and they
/// are not the same number — its position in this alphabetical order, which is
/// the index every selector speaks in, and
/// <see cref="PluginLoginCharacter.ActiveIndex"/>, the slot the account's own
/// list keeps it in. Both are shown to the player so an index can be checked
/// against what the character-select screen shows.
/// </summary>
/// <remarks>
/// One place resolves a selector so the chat command and the expression
/// functions cannot come to disagree about which character a name or an index
/// picks out. Only the defaults differ between them, and those are arguments.
/// </remarks>
internal static class LoginRoster
{
    /// <summary>
    /// The account's characters sorted by name. Ordinal, case-insensitive, so
    /// two players on different machines see the same order for the same
    /// account; ties fall back on the account slot so the order is total.
    /// </summary>
    public static IReadOnlyList<PluginLoginCharacter> Capture(ILoginAutomation login)
    {
        ArgumentNullException.ThrowIfNull(login);
        IReadOnlyList<PluginLoginCharacter> roster = login.CaptureRoster();
        if (roster.Count < 2)
            return roster;
        var sorted = new List<PluginLoginCharacter>(roster);
        sorted.Sort(static (left, right) =>
        {
            int byName = string.Compare(
                left.Name,
                right.Name,
                StringComparison.OrdinalIgnoreCase);
            return byName != 0
                ? byName
                : left.ActiveIndex.CompareTo(right.ActiveIndex);
        });
        return sorted;
    }

    /// <summary>
    /// Where the first character whose name contains <paramref name="name"/>
    /// sits in the sorted roster, or -1 when none does. Part of a name is
    /// enough, which is what lets one selector serve a whole stable of mules.
    /// </summary>
    public static int IndexOfName(
        IReadOnlyList<PluginLoginCharacter> roster,
        string? name)
    {
        ArgumentNullException.ThrowIfNull(roster);
        string text = name ?? string.Empty;
        for (int index = 0; index < roster.Count; index++)
        {
            if (roster[index].Name.Contains(
                text,
                StringComparison.OrdinalIgnoreCase))
            {
                return index;
            }
        }
        return -1;
    }

    /// <summary>
    /// Turns a selector into a position in the sorted roster. A selector that
    /// parses as a whole number is an index; anything else is part of a name,
    /// and a name is always absolute — <paramref name="relative"/> and
    /// <paramref name="looping"/> only ever apply to an index.
    /// </summary>
    /// <param name="roster">The sorted roster, from <see cref="Capture"/>.</param>
    /// <param name="selector">An index, or part of a character's name.</param>
    /// <param name="currentName">
    /// The character the client is on, which is where a relative index counts
    /// from.
    /// </param>
    /// <param name="relative">
    /// True to count the index on from the current character rather than from
    /// the start of the list.
    /// </param>
    /// <param name="looping">
    /// True to wrap an index that runs off either end back around the list.
    /// </param>
    /// <param name="index">The position resolved, or -1.</param>
    /// <param name="failure">Why nothing was resolved, else
    /// <see cref="LoginRosterFailure.None"/>.</param>
    /// <returns>True when the selector named a character that can be played.</returns>
    public static bool TryResolve(
        IReadOnlyList<PluginLoginCharacter> roster,
        string? selector,
        string? currentName,
        bool relative,
        bool looping,
        out int index,
        out LoginRosterFailure failure)
    {
        ArgumentNullException.ThrowIfNull(roster);
        index = -1;
        if (roster.Count == 0)
        {
            failure = LoginRosterFailure.Empty;
            return false;
        }

        string text = selector?.Trim() ?? string.Empty;
        int resolved;
        if (int.TryParse(
            text,
            NumberStyles.Integer,
            CultureInfo.InvariantCulture,
            out int requested))
        {
            if (relative)
            {
                int current = IndexOfName(roster, currentName);
                if (current < 0)
                {
                    failure = LoginRosterFailure.NoCurrentCharacter;
                    return false;
                }
                requested += current;
            }
            if (looping)
                requested = ((requested % roster.Count) + roster.Count) % roster.Count;
            if (requested < 0 || requested >= roster.Count)
            {
                failure = LoginRosterFailure.OutOfRange;
                return false;
            }
            resolved = requested;
        }
        else
        {
            resolved = IndexOfName(roster, text);
            if (resolved < 0)
            {
                failure = LoginRosterFailure.NoSuchName;
                return false;
            }
        }

        if (roster[resolved].IsPendingDelete || roster[resolved].ObjectId == 0u)
        {
            failure = LoginRosterFailure.PendingDelete;
            return false;
        }
        index = resolved;
        failure = LoginRosterFailure.None;
        return true;
    }
}
