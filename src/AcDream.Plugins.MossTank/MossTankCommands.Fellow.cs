using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// <c>/ub fellow</c>: the reference's fellowship command, one line per thing
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
        "/ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]";

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
            WriteUbBadSyntax("fellow");
            return;
        }

        (string head, string name) = SplitHead(line);
        string verb = head.ToLowerInvariant();
        IFellowshipAutomation fellowship = _host.Automation.Fellowship;
        if (!fellowship.IsInFellowship && verb != "create")
        {
            WriteUb("Your are not currently in a fellowship.");
            return;
        }

        uint self = _host.Automation.Character.ObjectId;
        bool isLeader = fellowship.LeaderObjectId == self;
        switch (verb)
        {
            case "create":
                if (fellowship.IsInFellowship)
                {
                    WriteUb("You are already in a fellowship.");
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
                    WriteUb("You are not the fellowship leader!");
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
        // The reference names whom it recruits, dismisses or hands the lead
        // to, and whom it could not find, only with its debug setting on;
        // otherwise these commands work in silence. Those lines are its
        // ordinary line and its error, not debug lines: a "could not find
        // player" error here is read by the profiles that wait on that line
        // from the follow command, so it must not appear with debug off.
        if (!UbObjectSearch.TryFindNearest(_host, name, partial, PlayerClasses, out PluginWorldObject player))
        {
            WriteFellowNotFound(name);
            return;
        }
        if (UbDebug)
            WriteUb(Invariant($"Recruiting {player.Name}[0x{player.ObjectId:X8}]"));
        if (!isLeader && !fellowship.IsOpen)
            return;
        if (fellowship.CaptureRoster().Any(member => member.ObjectId == player.ObjectId))
            return;
        if (UbObjectSearch.Distance(_host.Automation.Navigation.Snapshot, player)
            > FellowRecruitRangeMeters)
        {
            return;
        }
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
            WriteFellowNotFound(name);
            return;
        }
        if (UbDebug)
            WriteUb(Invariant($"Dismissing {member.Name}[0x{member.ObjectId:X8}]"));
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
            WriteFellowNotFound(name);
            return;
        }
        if (UbDebug)
            WriteUb(Invariant($"Transferring leader to {member.Name}[0x{member.ObjectId:X8}]"));
        if (!isLeader || member.ObjectId == self)
            return;
        ReportFellow(fellowship.AssignLeader(member.ObjectId), "give the lead to " + member.Name);
    }

    /// <summary>
    /// The reference's status line and one line per member, each member with
    /// its level. The split clause follows the reference's own test: it is
    /// printed whenever the split is not even, sharing or not.
    /// </summary>
    private void WriteFellowStatus(IFellowshipAutomation fellowship, uint self)
    {
        int count = fellowship.MemberCount;
        uint leader = fellowship.LeaderObjectId;
        string plural = count != 1 ? "s" : string.Empty;
        string open = fellowship.IsOpen ? "Open" : "Closed";
        string locked = fellowship.IsLocked ? "**LOCKED**" : "Not Locked";
        string sharing = fellowship.SharesExperience ? string.Empty : "NOT ";
        string split = fellowship.SplitsExperienceEvenly ? string.Empty : ", Uneven Split";
        WriteUb(Invariant(
            $"{leader:X8} {self:X8} Your current fellowship, \"{fellowship.Name}\", has {count} member{plural}, {sharing}Sharing XP{split}. {open}, {locked}."));
        foreach (PluginFellowMember member in fellowship.CaptureRoster())
        {
            // Each member is a message of its own in the reference, so each
            // carries the tag before its leading space.
            WriteUb(Invariant(
                $" {member.Name}[{member.Level}] H:{member.CurrentHealth}/{member.MaxHealth}{(member.ObjectId == leader ? " (Leader) " : "")}"));
        }
    }

    /// <summary>
    /// The reference's error for a name it could not find, written only while
    /// its debug setting is on. It names the player as typed, even when
    /// nothing was typed.
    /// </summary>
    private void WriteFellowNotFound(string name)
    {
        if (UbDebug)
            WriteUbError($"Could not find player {name}");
    }

    /// <summary>Says so when the client did not send a fellowship command.</summary>
    private void ReportFellow(PluginFellowshipCommandResult result, string what)
    {
        if (result.Accepted)
            return;
        WriteUbError(result.Status == PluginFellowshipCommandStatus.Rejected
            ? $"The client refused to {what}."
            : $"Cannot {what} right now.");
    }
}
