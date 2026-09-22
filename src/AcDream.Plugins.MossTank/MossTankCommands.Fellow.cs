using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// <c>/vt fellow</c>: the reference's fellowship command, one line per thing
/// the client's own social panel can do. Every line only asks the client; the
/// fellowship state it reads is what the server last said.
/// </summary>
/// <remarks>
/// The quiet refusals (recruiting while neither leader nor open, handing the
/// lead to someone outside the fellowship, opening an open fellowship) are
/// the reference's: it checks and sends nothing, and says nothing either.
/// </remarks>
internal sealed partial class MossTankPanel
{
    internal const string FellowUsage =
        "/vt fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]";

    /// <summary>
    /// How far away a player can be and still be recruited, in meters. The
    /// reference sends nothing for anyone farther.
    /// </summary>
    private const double FellowRecruitRangeMeters = 75d;

    [GeneratedRegex(
        @"^(create .+|quit|disband|open|close|status|recruit(p)?( .+)?|dismiss(p)?( .+)?|leader(p)?( .+)?)$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant)]
    private static partial Regex FellowPattern();

    private void HandleFellowCommand(string arguments)
    {
        string line = arguments.Trim();
        if (!FellowPattern().IsMatch(line))
        {
            WriteVtank("Bad command syntax");
            WriteVtank("Usage: " + FellowUsage);
            return;
        }

        (string head, string name) = SplitHead(line);
        string verb = head.ToLowerInvariant();
        IFellowshipAutomation fellowship = _host.Automation.Fellowship;
        if (!fellowship.IsInFellowship && verb != "create")
        {
            WriteVtank("Your are not currently in a fellowship.");
            return;
        }

        uint self = _host.Automation.Character.ObjectId;
        bool isLeader = fellowship.LeaderObjectId == self;
        switch (verb)
        {
            case "create":
                if (fellowship.IsInFellowship)
                {
                    WriteVtank("You are already in a fellowship.");
                    return;
                }
                // The reference passes the character's own share-experience
                // option; this client does not expose it, and the option is
                // on unless the player turned it off.
                ReportFellow(fellowship.Create(name, shareExperience: true), "create a fellowship");
                return;
            case "quit":
                ReportFellow(fellowship.Quit(disband: false), "quit the fellowship");
                return;
            case "disband":
                if (!isLeader)
                {
                    WriteVtank("You are not the fellowship leader!");
                    return;
                }
                ReportFellow(fellowship.Quit(disband: true), "disband the fellowship");
                return;
            case "open":
            case "close":
                bool open = verb == "open";
                if (!isLeader || fellowship.IsOpen == open)
                    return;
                ReportFellow(fellowship.SetOpen(open), open ? "open the fellowship" : "close the fellowship");
                return;
            case "status":
                WriteFellowStatus(fellowship, self);
                return;
            case "recruit":
            case "recruitp":
                RecruitFellow(fellowship, isLeader, name, partial: verb == "recruitp");
                return;
            case "dismiss":
            case "dismissp":
                DismissFellow(fellowship, self, isLeader, name, partial: verb == "dismissp");
                return;
            default:
                AssignFellowLeader(fellowship, self, isLeader, name, partial: verb == "leaderp");
                return;
        }
    }

    private void RecruitFellow(
        IFellowshipAutomation fellowship,
        bool isLeader,
        string name,
        bool partial)
    {
        if (!UbObjectSearch.TryFindNearest(_host, name, partial, PlayerClasses, out PluginWorldObject player))
        {
            WriteVtank(name.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {name}");
            return;
        }
        if (!isLeader && !fellowship.IsOpen)
            return;
        if (fellowship.CaptureRoster().Any(member => member.ObjectId == player.ObjectId))
            return;
        if (UbObjectSearch.Distance(_host.Automation.Navigation.Snapshot, player)
            > FellowRecruitRangeMeters)
        {
            return;
        }
        WriteVtank(Invariant($"Recruiting {player.Name}[0x{player.ObjectId:X8}]"));
        ReportFellow(fellowship.Recruit(player.ObjectId), "recruit " + player.Name);
    }

    private void DismissFellow(
        IFellowshipAutomation fellowship,
        uint self,
        bool isLeader,
        string name,
        bool partial)
    {
        if (!UbObjectSearch.TryFindFellow(_host, fellowship.CaptureRoster(), name, partial, out PluginFellowMember member))
        {
            WriteVtank(name.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {name}");
            return;
        }
        WriteVtank(Invariant($"Dismissing {member.Name}[0x{member.ObjectId:X8}]"));
        // Dismissing yourself is leaving: the reference turns it into a quit,
        // which is the only form the server answers sensibly.
        if (member.ObjectId == self)
        {
            ReportFellow(fellowship.Quit(disband: false), "quit the fellowship");
            return;
        }
        if (!isLeader)
            return;
        ReportFellow(fellowship.Dismiss(member.ObjectId), "dismiss " + member.Name);
    }

    private void AssignFellowLeader(
        IFellowshipAutomation fellowship,
        uint self,
        bool isLeader,
        string name,
        bool partial)
    {
        if (!UbObjectSearch.TryFindFellow(_host, fellowship.CaptureRoster(), name, partial, out PluginFellowMember member))
        {
            WriteVtank(name.Length == 0
                ? "Could not find closest player"
                : $"Could not find player {name}");
            return;
        }
        WriteVtank(Invariant($"Transferring leader to {member.Name}[0x{member.ObjectId:X8}]"));
        if (!isLeader || member.ObjectId == self)
            return;
        ReportFellow(fellowship.AssignLeader(member.ObjectId), "give the lead to " + member.Name);
    }

    /// <summary>
    /// The reference's status line and one line per member. The client does
    /// not say whether experience is shared or split evenly, so that clause of
    /// the reference's line is left out rather than guessed.
    /// </summary>
    private void WriteFellowStatus(IFellowshipAutomation fellowship, uint self)
    {
        int count = fellowship.MemberCount;
        uint leader = fellowship.LeaderObjectId;
        string plural = count != 1 ? "s" : string.Empty;
        string open = fellowship.IsOpen ? "Open" : "Closed";
        string locked = fellowship.IsLocked ? "**LOCKED**" : "Not Locked";
        WriteVtank(Invariant(
            $"{leader:X8} {self:X8} Your current fellowship, \"{fellowship.Name}\", has {count} member{plural}. {open}, {locked}."));
        foreach (PluginFellowMember member in fellowship.CaptureRoster())
        {
            WriteVtank(Invariant(
                $" {member.Name} H:{member.CurrentHealth}/{member.MaxHealth}{(member.ObjectId == leader ? " (Leader) " : "")}"));
        }
    }

    /// <summary>Says so when the client did not send a fellowship command.</summary>
    private void ReportFellow(PluginFellowshipCommandResult result, string what)
    {
        if (result.Accepted)
            return;
        WriteVtank(result.Status == PluginFellowshipCommandStatus.Rejected
            ? $"The client refused to {what}."
            : $"Cannot {what} right now.");
    }
}
