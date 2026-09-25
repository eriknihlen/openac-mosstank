using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// /vt and /ub are two command sets, as the two plugins MossTank stands in
/// for keep them: the macro's commands (and MossTank's own additions to
/// them) answer on /vt only, the UtilityBelt commands on /ub only, and each
/// word answers the other's commands the way it answers any word it does not
/// know. The few commands both plugins have answer on both, each in its own
/// plugin's way.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const string VtWord = "vt";
    private const string UbWord = "ub";

    /// <summary>
    /// Every command word MossTank answers, and the word it answers on. A
    /// command that answers on both is pinned by its own tests below.
    /// Mutation: one dispatcher for both words (the old shape) answers every
    /// row on the wrong word too, and fails every row.
    /// </summary>
    [Theory]
    // The macro's own commands.
    [InlineData(VtWord, "start")]
    [InlineData(VtWord, "stop")]
    [InlineData(VtWord, "forcebuff")]
    [InlineData(VtWord, "cancelforcebuff")]
    [InlineData(VtWord, "settings")]
    [InlineData(VtWord, "nav")]
    [InlineData(VtWord, "loot")]
    [InlineData(VtWord, "meta")]
    [InlineData(VtWord, "setmetastate")]
    [InlineData(VtWord, "echo hello")]
    [InlineData(VtWord, "setattackbar 0.5")]
    [InlineData(VtWord, "tapjump")]
    [InlineData(VtWord, "addnavjump")]
    [InlineData(VtWord, "addnavpt")]
    [InlineData(VtWord, "addnavcheckpoint")]
    [InlineData(VtWord, "reverseroute")]
    [InlineData(VtWord, "reverseroutequery")]
    [InlineData(VtWord, "deletemonster")]
    [InlineData(VtWord, "equipitemsfor")]
    [InlineData(VtWord, "testitem")]
    [InlineData(VtWord, "testmonster")]
    [InlineData(VtWord, "testspell")]
    [InlineData(VtWord, "testpet")]
    [InlineData(VtWord, "listmonstervariables")]
    [InlineData(VtWord, "dumpmetavars")]
    [InlineData(VtWord, "listmetafunctions")]
    [InlineData(VtWord, "metafunchelp")]
    [InlineData(VtWord, "deathrestore")]
    [InlineData(VtWord, "fakedeath")]
    [InlineData(VtWord, "pscount")]
    [InlineData(VtWord, "refresh")]
    [InlineData(VtWord, "log")]
    [InlineData(VtWord, "lockdump")]
    [InlineData(VtWord, "dumptracker")]
    [InlineData(VtWord, "clearlocks")]
    [InlineData(VtWord, "clearbusy")]
    [InlineData(VtWord, "fakeimp")]
    [InlineData(VtWord, "dumpspells")]
    [InlineData(VtWord, "dumpspecies")]
    [InlineData(VtWord, "dumpmats")]
    [InlineData(VtWord, "getdb")]
    // MossTank's own additions, which belong with the macro's.
    [InlineData(VtWord, "gamedb")]
    [InlineData(VtWord, "nextwp")]
    [InlineData(VtWord, "prevwp")]
    [InlineData(VtWord, "metainterval")]
    [InlineData(VtWord, "ub")]
    // The UtilityBelt commands.
    [InlineData(UbWord, "count item taper")]
    [InlineData(UbWord, "login list")]
    [InlineData(UbWord, "autovendor")]
    [InlineData(UbWord, "vendor buyall")]
    [InlineData(UbWord, "xp test")]
    [InlineData(UbWord, "equip list")]
    [InlineData(UbWord, "simplejump")]
    [InlineData(UbWord, "calcdamage")]
    [InlineData(UbWord, "pos")]
    [InlineData(UbWord, "id")]
    [InlineData(UbWord, "vitae")]
    [InlineData(UbWord, "combatstate peace")]
    [InlineData(UbWord, "date")]
    [InlineData(UbWord, "dateutc")]
    [InlineData(UbWord, "delay 100 /say hi")]
    [InlineData(UbWord, "bc /say hi")]
    [InlineData(UbWord, "bct tag /say hi")]
    [InlineData(UbWord, "netclients")]
    [InlineData(UbWord, "closestportal")]
    [InlineData(UbWord, "close corpse")]
    [InlineData(UbWord, "printcolors")]
    [InlineData(UbWord, "autotinker")]
    [InlineData(UbWord, "getjob")]
    [InlineData(UbWord, "tinkcalc")]
    [InlineData(UbWord, "listvars")]
    [InlineData(UbWord, "listpvars")]
    [InlineData(UbWord, "listgvars")]
    [InlineData(UbWord, "myquests")]
    [InlineData(UbWord, "quit")]
    [InlineData(UbWord, "autostack")]
    [InlineData(UbWord, "autocram")]
    [InlineData(UbWord, "clearbugged")]
    [InlineData(UbWord, "playeroption list")]
    [InlineData(UbWord, "translateroute")]
    [InlineData(UbWord, "face 90")]
    [InlineData(UbWord, "setmotion forward 0")]
    [InlineData(UbWord, "clearmotion")]
    [InlineData(UbWord, "prepclick stop")]
    [InlineData(UbWord, "fellow status")]
    [InlineData(UbWord, "jumpsw 90 100")]
    [InlineData(UbWord, "ig muled to Bob")]
    [InlineData(UbWord, "igp muled to Bob")]
    [InlineData(UbWord, "portal Town")]
    [InlineData(UbWord, "portalp Town")]
    [InlineData(UbWord, "follow Bob")]
    [InlineData(UbWord, "followp Bob")]
    [InlineData(UbWord, "use Taper")]
    [InlineData(UbWord, "usel Taper")]
    [InlineData(UbWord, "select Taper")]
    [InlineData(UbWord, "swearallegiance Bob")]
    [InlineData(UbWord, "breakallegiance Bob")]
    [InlineData(UbWord, "give Taper to Bob")]
    [InlineData(UbWord, "givep Taper to Bob")]
    [InlineData(UbWord, "giveP Taper to Bob")]
    [InlineData(UbWord, "giver Taper to Bob")]
    public void EachCommandAnswersOnItsOwnWordOnly(string word, string line)
    {
        (FakeAutomation own, MossTankPanel ownPanel) = NamespacePanel();
        (FakeAutomation other, MossTankPanel otherPanel) = NamespacePanel();
        string otherWord = word == VtWord ? UbWord : VtWord;

        Issue(ownPanel, word, line);
        Issue(otherPanel, otherWord, line);

        Assert.DoesNotContain(own.Messages, IsUnknownReply);
        Assert.DoesNotContain(own.Messages, static message =>
            message.StartsWith("Command failed", StringComparison.Ordinal));
        Assert.NotEmpty(other.Messages);
        Assert.Equal(UnknownReply(otherWord), other.Messages[0]);
    }

    /// <summary>
    /// The commands both plugins have answer on both words.
    /// Mutation: dropping any of them from either dispatcher answers it as
    /// unknown on that word.
    /// </summary>
    [Theory]
    [InlineData("help")]
    [InlineData("opt list")]
    [InlineData("mexec 1")]
    [InlineData("propertydump")]
    [InlineData("dumpskills")]
    [InlineData("jump 90 true 100")]
    [InlineData("jump 100")]
    [InlineData("")]
    public void TheCommandsBothPluginsHaveAnswerOnBothWords(string line)
    {
        foreach (string word in (string[])[VtWord, UbWord])
        {
            (FakeAutomation automation, MossTankPanel panel) = NamespacePanel();

            Issue(panel, word, line);

            Assert.DoesNotContain(automation.Messages, IsUnknownReply);
        }
    }

    /// <summary>
    /// Each word answers a word it does not know in its own plugin's words:
    /// /vt with its one line, /ub with the reference's "command not found"
    /// and the commands within two edits of what was typed.
    /// Mutation: sending /ub's unknown words to the /vt answer fails the
    /// second half; dropping the guesses fails its last line; printing the
    /// first line as a plain system line fails the class and the switch.
    /// </summary>
    [Fact]
    public void EachWordAnswersAnUnknownWordInItsOwnWords()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = NamespacePanel();
        (FakeAutomation ub, MossTankPanel ubPanel) = NamespacePanel();

        Issue(vtPanel, VtWord, "autostak");
        Issue(ubPanel, UbWord, "autostak");

        Assert.Equal(MossTankPanel.UnknownVtankCommand, Assert.Single(vt.Messages));
        Assert.Equal(
            [
                "[UB] Error: Command not found! Type \"ub help\" for a list of commands.",
                "[UB] Error: Did you mean one of these? autostack",
            ],
            ub.Messages);
        // Both are errors: in the error class, and behind the error switch.
        Assert.Equal(2, ub.Posted.Count);
        Assert.All(ub.Posted, static line => Assert.Equal(UbChat.ErrorChatType, line.Kind));

        ub.Messages.Clear();
        Issue(ubPanel, UbWord, "zzzzzzzz");
        Assert.Equal(MossTankPanel.UnknownUbCommand, Assert.Single(ub.Messages));

        Issue(ubPanel, UbWord, "opt set Plugin.ErrorMessageDisplay.Enabled false");
        ub.Messages.Clear();
        Issue(ubPanel, UbWord, "zzzzzzzz");
        Assert.Empty(ub.Messages);
    }

    /// <summary>
    /// The guesses use the reference's edit count: a swap of two neighbours
    /// is one edit. Mutation: dropping the swap step makes "fcae" two edits
    /// from "face".
    /// </summary>
    [Theory]
    [InlineData("fcae", "face", 1)]
    [InlineData("autostak", "autostack", 1)]
    [InlineData("face", "face", 0)]
    [InlineData("bc", "fac", 2)]
    public void TheEditCountMatchesTheReference(string one, string two, int distance) =>
        Assert.Equal(distance, MossTankPanel.EditDistance(one, two));

    /// <summary>
    /// /ub help lists only the UtilityBelt commands, in the reference's
    /// words, and a word it has no command for gets the whole list, as the
    /// reference does; /vt help lists only the macro's. Mutation: a shared
    /// help lists both groups on both words.
    /// </summary>
    [Fact]
    public void EachHelpListsOnlyItsOwnCommands()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = NamespacePanel();
        (FakeAutomation ub, MossTankPanel ubPanel) = NamespacePanel();

        Issue(vtPanel, VtWord, "help");
        Issue(ubPanel, UbWord, "help");

        string ubList = ub.Messages[0];
        Assert.StartsWith("[UB] All available UB commands: /ub {", ubList, StringComparison.Ordinal);
        Assert.Equal("For help with a specific command, use `/ub help [command]`", ub.Messages[1]);
        foreach (string ubOnly in (string[])["autostack", "fellow", "give", "setmotion", "vitae"])
        {
            Assert.Contains(ubOnly, ubList, StringComparison.Ordinal);
            Assert.DoesNotContain(vt.Messages, line => Regex.IsMatch(line, $@"\b{ubOnly}\b"));
        }
        foreach (string vtOnly in (string[])["setmetastate", "gamedb", "nextwp", "forcebuff"])
        {
            Assert.DoesNotContain(vtOnly, ubList, StringComparison.Ordinal);
            Assert.Contains(vt.Messages, line => Regex.IsMatch(line, $@"\b{vtOnly}\b"));
        }

        ub.Messages.Clear();
        Issue(ubPanel, UbWord, "help setmetastate");
        Assert.Equal(ubList, ub.Messages[0]);

        ub.Messages.Clear();
        Issue(ubPanel, UbWord, "help face");
        Assert.Equal(
            [
                "[UB] Usage: /ub face <heading>",
                "Description: Face heading commands with built in navigation pausing and retries.",
                "Examples:",
                " /ub face 180",
                "  Faces your character towards 180 degrees (south).",
            ],
            ub.Messages);

        vt.Messages.Clear();
        Issue(vtPanel, VtWord, "help face");
        Assert.Equal("No help found for command: face", Assert.Single(vt.Messages));
    }

    /// <summary>
    /// /vt opt is the macro's options and /ub opt the UB settings; neither
    /// reaches the other's group. Mutation: the old fallback from a /vt name
    /// to a UB setting sets the dotted name through /vt.
    /// </summary>
    [Fact]
    public void EachOptReachesOnlyItsOwnGroup()
    {
        (FakeAutomation automation, MossTankPanel panel) = NamespacePanel();
        bool combat = panel.GetMetaOptionForTest("EnableCombat");

        Issue(panel, VtWord, "opt set DungeonMaps.Enabled false");
        Assert.Equal("Option set: Invalid option specified.", Assert.Single(automation.Messages));
        Assert.True(panel.EvaluateExpression("uboptget[`DungeonMaps.Enabled`]").IsTruthy);

        automation.Messages.Clear();
        Issue(panel, VtWord, "opt get DungeonMaps.Enabled");
        Assert.Equal("Option get: Invalid option specified.", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Issue(panel, VtWord, "opt toggle DungeonMaps.Enabled");
        Assert.Equal("Option toggle: Invalid option specified.", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Issue(panel, UbWord, $"opt set EnableCombat {!combat}");
        Assert.Equal("[UB] Error: Invalid option: enablecombat", automation.Messages[0]);
        Assert.Equal(combat, panel.GetMetaOptionForTest("EnableCombat"));

        automation.Messages.Clear();
        Issue(panel, UbWord, "opt toggle EnableCombat");
        Assert.Equal("[UB] Error: Invalid option: enablecombat", automation.Messages[0]);

        automation.Messages.Clear();
        Issue(panel, UbWord, "opt set DungeonMaps.Enabled false");
        Assert.Equal("[UB] DungeonMaps.Enabled (Profile) = False", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Issue(panel, UbWord, "opt frobnicate");
        Assert.Equal(
            [
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub opt {list | get <option> | set <option> <newValue> | toggle <option>}",
            ],
            automation.Messages.Take(2));
    }

    /// <summary>
    /// mexec answers on both words, each in its own plugin's words: /vt as
    /// the macro prints it, /ub with the result's type and time, a true or
    /// false being a number there. Mutation: sending /ub mexec to the macro's
    /// printer prints "MExec evaluating expression".
    /// </summary>
    [Fact]
    public void EachMexecAnswersInItsOwnWords()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = NamespacePanel();
        (FakeAutomation ub, MossTankPanel ubPanel) = NamespacePanel();

        Issue(vtPanel, VtWord, "mexec 1+2");
        Issue(ubPanel, UbWord, "mexec 1+2");
        Issue(ubPanel, UbWord, "mexec 1==1");
        Issue(ubPanel, UbWord, "mexec `a`");

        Assert.Equal(["MExec evaluating expression: \"1+2\"", "Result: 3"], vt.Messages);
        Assert.Equal("[UB] Evaluating expression: \"1+2\"", ub.Messages[0]);
        Assert.Matches(@"^\[UB\] Result: \[number\] 3 \(\d+(\.\d+)?ms\)$", ub.Messages[1]);
        Assert.Matches(@"^\[UB\] Result: \[number\] 1 \(\d+(\.\d+)?ms\)$", ub.Messages[3]);
        Assert.Matches(@"^\[UB\] Result: \[string\] a \(\d+(\.\d+)?ms\)$", ub.Messages[5]);
    }

    /// <summary>
    /// A meta's chat line reaches the handler its word names: a /vt line the
    /// macro, a /ub line UtilityBelt. Mutation: routing the two words to one
    /// dispatcher answers the /vt face line.
    /// </summary>
    [Fact]
    public void MetaChatLinesReachTheWordTheyName()
    {
        (FakeAutomation automation, MossTankPanel panel) = NamespacePanel();
        var registry = new Dictionary<string, Action<PluginCommand>>(StringComparer.OrdinalIgnoreCase)
        {
            [MossTankPanel.VtankVerb] = panel.ExecuteVtankCommand,
            [MossTankPanel.UbVerb] = panel.ExecuteUbCommand,
        };

        foreach (string line in (string[])["/ub face 90", "/vt face 90", "/vt reverseroutequery"])
        {
            string[] parts = line[1..].Split(' ', 2);
            registry[parts[0]](new PluginCommand(parts[0], parts[1], line));
        }
        panel.OnTick(0.05d);

        Assert.Equal(90f, automation.FacedHeadings[^1]);
        Assert.Equal(
            [MossTankPanel.UnknownVtankCommand, "Nav backwards is: False"],
            automation.Messages.Where(static line =>
                line == MossTankPanel.UnknownVtankCommand || line.StartsWith("Nav ", StringComparison.Ordinal)));
    }

    private static (FakeAutomation, MossTankPanel) NamespacePanel()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        return (automation, new MossTankPanel(new FakeHost(automation, new MemoryStorage())));
    }

    private static void Issue(MossTankPanel panel, string word, string line)
    {
        var command = new PluginCommand(
            word,
            line,
            line.Length == 0 ? "/" + word : $"/{word} {line}");
        if (word == VtWord)
            panel.ExecuteVtankCommand(command);
        else
            panel.ExecuteUbCommand(command);
    }

    private static string UnknownReply(string word) =>
        word == VtWord ? MossTankPanel.UnknownVtankCommand : MossTankPanel.UnknownUbCommand;

    private static bool IsUnknownReply(string message) =>
        message == MossTankPanel.UnknownVtankCommand
        || message == MossTankPanel.UnknownUbCommand;
}
