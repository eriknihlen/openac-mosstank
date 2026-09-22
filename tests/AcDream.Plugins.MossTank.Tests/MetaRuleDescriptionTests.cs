namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The meta window's one-line description of a rule. Seen live: compound
/// conditions read "All 0" and "Not 0" and a do-all action read "All",
/// with nothing of what they held.
/// </summary>
public sealed class MetaRuleDescriptionTests
{
    [Fact]
    public void ACompoundConditionListsWhatItHolds()
    {
        var condition = new MetaCondition
        {
            Kind = MetaConditionKind.All,
            Children =
            [
                new MetaCondition { Kind = MetaConditionKind.SecondsInStateGreaterThanOrEqual, Number = 2d },
                new MetaCondition
                {
                    Kind = MetaConditionKind.Not,
                    Children = [new MetaCondition { Kind = MetaConditionKind.Expression, Text = "testvar[x]" }],
                },
                new MetaCondition { Kind = MetaConditionKind.NavigationRouteEmpty },
            ],
        };

        Assert.Equal(
            "All (SecondsInStateGreaterThanOrEqual 2; Not Expression: testvar[x]; NavigationRouteEmpty)",
            MossTankPanel.DescribeMetaCondition(condition));
    }

    [Fact]
    public void ADoAllActionListsItsActionsInOrder()
    {
        var action = new MetaAction
        {
            Kind = MetaActionKind.All,
            Children =
            [
                new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/vt opt set enablenav false" },
                new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "loot" },
            ],
        };

        Assert.Equal(
            "All (ChatCommand: /vt opt set enablenav false; SetMetaState: loot)",
            MossTankPanel.DescribeMetaAction(action));
    }
}
