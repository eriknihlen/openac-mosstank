using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class HostExpressionFunctionsTests
{
    [Fact]
    public void CharacterAndNavigationFunctionsReadCanonicalAutomationFacts()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Coldeve", runtime.Evaluate("getworldname[]").AsString());
        Assert.Equal(2d, runtime.Evaluate("getcharacterindex[]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getcharvital_current[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharvital_buffedmax[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharattribute_base[1]").AsNumber());
        Assert.Equal(110d, runtime.Evaluate("getcharattribute_buffed[1]").AsNumber());
        Assert.Equal(275d, runtime.Evaluate("getcharskill_base[34]").AsNumber());
        Assert.Equal(300d, runtime.Evaluate("getcharskill_buffed[34]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("getcharskill_traininglevel[34]").AsNumber());
        Assert.Equal(0x7F7Fu, runtime.Evaluate("getplayerlandblock[]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getheading[wobjectgetplayer[]]").AsNumber());
    }

    /// <summary>
    /// The three vital reads are three DIFFERENT numbers — unbuffed maximum,
    /// current, buffed maximum — and each is floored at 1. Mutation: pointing
    /// base and buffed maximum at the same field makes the first two
    /// assertions equal, and dropping the floor makes the last one 0.
    /// </summary>
    [Fact]
    public void VitalReadsSeparateBaseCurrentAndBuffedMaximumAndFloorAtOne()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(80d, runtime.Evaluate("getcharvital_base[1]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getcharvital_current[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharvital_buffedmax[1]").AsNumber());
        Assert.Equal(70d, runtime.Evaluate("getcharvital_base[2]").AsNumber());
        Assert.Equal(60d, runtime.Evaluate("getcharvital_base[3]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_current[9]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_base[9]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_buffedmax[9]").AsNumber());
    }

    [Fact]
    public void ObjectDiscoveryCountsPropertiesAndNearestMatchUtilityBeltShape()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2d, runtime.Evaluate(
            "listcount[wobjectfindallinventory[]]").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "getitemcountininventorybyname['Health Elixir']").AsNumber());
        Assert.Equal(9d, runtime.Evaluate("getfreeitemslots[]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getfreecontainerslots[]").AsNumber());
        Assert.Equal("Drudge", runtime.Evaluate(
            "wobjectgetname[wobjectfindnearestmonster[]]").AsString());
        Assert.Equal(77d, runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[20],25]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx['(?i)elixir|drudge']]").AsNumber());
        Assert.Equal((double)PluginObjectClass.Monster, runtime.Evaluate(
            "wobjectgetobjectclass[wobjectfindbyid[20]]").AsNumber());
        Assert.Equal(54321d, runtime.Evaluate(
            "wobjectlastidtime[wobjectfindbyid[20]]").AsNumber());
    }

    /// <summary>
    /// Inventory lookups by name are exact and case-SENSITIVE, and the regex
    /// variant compiles its pattern without IgnoreCase. Mutation: comparing
    /// with OrdinalIgnoreCase, or adding RegexOptions.IgnoreCase, makes the
    /// lower-case probes find the item.
    /// </summary>
    [Fact]
    public void InventoryNameLookupsAreCaseSensitive()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(10d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname['Health Elixir']]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybyname['health elixir']").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx['^Health']]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybynamerx['^health']").AsNumber());
    }

    /// <summary>
    /// A name PATTERN is matched against the display name — the material in
    /// front of the bare name — while the exact-name finder keeps reading the
    /// bare name. A material id that only names a group adds no prefix.
    /// Mutation: matching the bare name in either regex finder makes the
    /// `^Silver ` and `^Oak ` probes answer 0; prefixing inside the exact
    /// finder makes the `Long Sword` probe answer 0; giving the grouping id 9
    /// a name of its own turns "Shard" into "Gem Shard".
    /// </summary>
    [Fact]
    public void NamePatternsMatchTheMaterialPrefixedDisplayName()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            12, 102, "Long Sword", PluginObjectClass.MeleeWeapon, 0x1, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            13, 103, "Shard", PluginObjectClass.Gem, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            22, 104, "Chest", PluginObjectClass.Container, 0x200, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 10.2d },
        });
        automation.Properties[12] = Properties(
            ints: new Dictionary<uint, int> { [131] = 63 });
        automation.Properties[13] = Properties(
            ints: new Dictionary<uint, int> { [131] = 9 });
        automation.Properties[22] = Properties(
            ints: new Dictionary<uint, int> { [131] = 75 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Silver Long Sword", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[12]]").AsString());
        Assert.Equal(12d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx['^Silver ']]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybyname['Silver Long Sword']").AsNumber());
        Assert.Equal(12d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname['Long Sword']]").AsNumber());
        Assert.Equal("Shard", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[13]]").AsString());
        Assert.Equal(13d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx['^Shard$']]").AsNumber());
        Assert.Equal(22d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[10,'^Oak ']]")
            .AsNumber());
    }

    /// <summary>
    /// The nearest-by-name-and-class lookup takes the object class FIRST and a
    /// case-sensitive REGEX second. Mutation: the previous argument order plus
    /// literal case-insensitive equality fails every assertion here — the
    /// regex probe outright throws, because argument 0 is a number.
    /// </summary>
    [Fact]
    public void NearestByNameAndObjectClassTakesClassThenCaseSensitiveRegex()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,'^Dru']]").AsNumber());
        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,'Drudge']]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbynameandobjectclass[5,'^dru']").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbynameandobjectclass[6,'^Dru']").AsNumber());
    }

    /// <summary>
    /// The nearest lookups skip the player's own object and rank by the 3-D
    /// distance. Mutation: dropping the self-exclusion returns the player for
    /// the player class, and ranking on the flat distance picks the object
    /// that is closer on the map but a hundred metres up.
    /// </summary>
    [Fact]
    public void NearestLookupsExcludeSelfAndRankInThreeDimensions()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            21, 200, "Skywards Drudge", PluginObjectClass.Monster, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with
            {
                EastWest = 10.05d,
                Elevation = automation.Position.Elevation + 100d,
            },
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbyobjectclass[24]").AsNumber());
        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestmonster[]]").AsNumber());
    }

    /// <summary>
    /// A nearest lookup searches every object of the class the client knows,
    /// not only the ones lying loose on the ground: an object in a pack or in
    /// the character's hand is found too, and an object with no known
    /// position still answers when it is the only match. Mutation: requiring
    /// the object to be loose on the ground and to have a position makes both
    /// probes here answer 0.
    /// </summary>
    [Fact]
    public void NearestLookupsSearchOwnedAndPositionlessObjectsToo()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            30, 300, "Wielded Wand", PluginObjectClass.WandStaffOrb, 0x1, 0, 1)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            31, 301, "Packed Lockpick", PluginObjectClass.Lockpick, 0x1, 11, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(30d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbyobjectclass[31]]").AsNumber());
        Assert.Equal(31d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbytemplatetype[301]]").AsNumber());
    }

    /// <summary>
    /// The nearest-monster lookup honours the combat pass's blacklist.
    /// Mutation: matching on the object class alone still returns the
    /// blacklisted drudge.
    /// </summary>
    [Fact]
    public void NearestMonsterSkipsMonstersTheCombatPassHasBlacklisted()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestmonster[]]").AsNumber());

        runtime.Policy.MonsterEligibility = objectId => objectId != 20u;

        Assert.Equal(0d, runtime.Evaluate("wobjectfindnearestmonster[]").AsNumber());
    }

    [Fact]
    public void ActionFunctionsUseSharedSelectionInventoryMagicAndMovementCommands()
    {
        var automation = CreateAutomation();
        var host = new Host(automation);
        using var runtime = new MossTankExpressionRuntime(host);

        runtime.Evaluate("actiontryselect[20]");
        Assert.Equal(20u, host.Selection.SelectedObjectId);
        Assert.True(runtime.Evaluate("actiontryuseitem[10]").IsTruthy);
        Assert.Equal(10u, automation.UsedObject);
        Assert.True(runtime.Evaluate("setmotion['Forward',1]").IsTruthy);
        Assert.True(automation.LastIntent.Forward);
        Assert.True(runtime.Evaluate("clearmotion[]").IsTruthy);
        Assert.Equal(1, automation.ClearMovementCount);
    }

    /// <summary>
    /// Selecting from an expression always reports false, whatever the
    /// selection did — a profile branching on the result relies on it.
    /// Mutation: returning the real selection result flips the branch.
    /// </summary>
    [Fact]
    public void SelectingAlwaysReportsFalseEvenWhenItSucceeded()
    {
        var automation = CreateAutomation();
        var host = new Host(automation);
        using var runtime = new MossTankExpressionRuntime(host);

        Assert.False(runtime.Evaluate("actiontryselect[20]").IsTruthy);
        Assert.Equal(20u, host.Selection.SelectedObjectId);
    }

    /// <summary>
    /// The castability questions differ: hunting and buffing each apply their
    /// own skill margin over the spell's difficulty, on top of the spellbook
    /// and component checks. Mutation: answering both from one host gate
    /// makes the two calls agree at every skill level.
    /// </summary>
    [Fact]
    public void CastabilityAppliesTheHuntingAndBuffingSkillMarginsSeparately()
    {
        var automation = CreateAutomation();
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 250);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        // The fake's War Magic is 300 buffed.
        runtime.Policy.SkillMargin = hunting => hunting ? 75 : 25;

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
        Assert.True(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);

        automation.HasComponents = false;
        Assert.False(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);
        automation.HasComponents = true;
        Assert.False(runtime.Evaluate("getcancastspell_buff[2002]").IsTruthy);
    }

    /// <summary>
    /// A spell whose school the host has not reported yet is NOT castable:
    /// the unknown skill reads 0 and fails the difficulty comparison. This is
    /// the state a fresh session is in before the skill table arrives.
    /// Mutation: skipping the comparison when the skill is unknown makes both
    /// castability questions answer true, and the cast answers 0 instead of 2.
    /// </summary>
    [Fact]
    public void CastabilityFailsClosedWhenTheSchoolSkillIsUnknown()
    {
        var automation = CreateAutomation();
        // The fake reports War Magic (34) only; Life Magic (33) is unknown.
        automation.Spells[1001] = Spell(1001, school: 33, difficulty: 1);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
        Assert.False(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());

        automation.Skills =
        [
            .. automation.Skills,
            new PluginSkillInfo(33, "Life Magic", PluginSkillTraining.Trained, 300),
        ];
        Assert.True(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
    }

    /// <summary>
    /// A spell the host reports with no school at all is not castable either,
    /// even when nothing else stands in the way: there is no skill to compare
    /// it against, and no school-less spell a character can actually cast.
    /// The difficulty here is 0, so a comparison against an unknown skill of 0
    /// would PASS — only the closed door answers false. Mutation: skipping the
    /// comparison for a school of 0, or dropping the school test and comparing
    /// anyway, makes both castability questions answer true and the cast
    /// answer 0.
    /// </summary>
    [Fact]
    public void CastabilityIsClosedForASpellWithNoSchool()
    {
        var automation = CreateAutomation();
        automation.Spells[1003] = Spell(1003, school: 0, difficulty: 0);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1003]").IsTruthy);
        Assert.False(runtime.Evaluate("getcancastspell_buff[1003]").IsTruthy);
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1003]").AsNumber());
    }

    /// <summary>
    /// Casting from an expression answers 2 (impossible), 0 (not attempted
    /// yet) or 1 (begun) — never a bare boolean. An untargeted spell is only
    /// castable through `actiontrycastbyid`, a targeted one only through
    /// `actiontrycastbyidontarget`, and neither casts until a wand is wielded
    /// and the character is in magic mode. Mutation: gating on the host cast
    /// gate alone returns 1 straight away with no weapon and no mode.
    /// </summary>
    [Fact]
    public void CastingReportsImpossibleNotYetAndBegunAndTakesOneStepFirst()
    {
        var automation = CreateAutomation();
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 10);
        automation.Spells[1002] = Spell(1002, school: 34, difficulty: 10) with
        {
            IsUntargeted = true,
        };
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[9999]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate(
            "actiontrycastbyidontarget[1002,20]").AsNumber());

        // Step one: the wand is not wielded yet.
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(50u, automation.EquippedObject);

        // Step two: wielded, but still in peace mode.
        automation.Equipment =
        [
            automation.Equipment[0] with { EquippedLocation = 0x01000000u },
        ];
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal(PluginCombatMode.Magic, automation.RequestedMode);

        // Ready: the cast begins.
        automation.Mode = PluginCombatMode.Magic;
        Assert.True(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal((1002u, 0u), automation.LastCast);
        Assert.Equal(1d, runtime.Evaluate(
            "actiontrycastbyidontarget[1001,20]").AsNumber());
        Assert.Equal((1001u, 20u), automation.LastCast);
    }

    /// <summary>
    /// A busy character takes NO step towards magic mode: no equip request,
    /// no stance request, and the step answers false. An expression rule ticks
    /// every pass, so without the gate the same two requests are re-issued
    /// while an action is already in flight. Mutation: dropping the busy gate
    /// makes `automation.EquippedObject` 50 on the first evaluation.
    /// </summary>
    [Fact]
    public void MagicModeStepTakesNoStepWhileTheHostIsBusy()
    {
        var automation = CreateAutomation();
        automation.Spells[1002] = Spell(1002, school: 34, difficulty: 10) with
        {
            IsUntargeted = true,
        };
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d),
        ];
        automation.IsBusy = true;
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("getbusystate[]").AsNumber());
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(0u, automation.EquippedObject);
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal(0u, automation.EquippedObject);
        Assert.Equal((0u, 0u), automation.LastCast);

        automation.IsBusy = false;
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(50u, automation.EquippedObject);
    }

    [Fact]
    public void LoginFunctionsUseTheAuthoritativeSortedRosterAndOneShotOwner()
    {
        var automation = CreateAutomation();
        automation.LoginRoster =
        [
            new PluginLoginCharacter(10u, "Beta", 2, false),
            new PluginLoginCharacter(1u, "Expression Tester", 0, false),
            new PluginLoginCharacter(30u, "Mule", 1, false),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("getcharacterindex[]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "getcharacterindex['bet']").AsNumber());
        Assert.True(runtime.Evaluate("setnextlogin[1]").IsTruthy);
        Assert.Equal(30u, automation.NextLoginObjectId);
        Assert.True(runtime.Evaluate("setnextlogin['bet']").IsTruthy);
        Assert.Equal(10u, automation.NextLoginObjectId);
        Assert.True(runtime.Evaluate("clearnextlogin[]").IsTruthy);
        Assert.Equal(0u, automation.NextLoginObjectId);
    }

    /// <summary>
    /// `chatbox` sends and gives its argument back; `chatboxpaste` stages the
    /// text in the chat entry WITHOUT sending it, strips control characters,
    /// and reports false when there is nothing to paste or the player is
    /// already typing. Mutation: pointing chatboxpaste at the same submit
    /// call sends the text and leaves the entry empty.
    /// </summary>
    [Fact]
    public void ChatboxSendsAndChatboxPasteOnlyStagesTheText()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        automation.SubmittedChat.Clear();

        Assert.Equal("/say hi", runtime.Evaluate("chatbox['/say hi']").AsString());
        Assert.Equal(["/say hi"], automation.SubmittedChat);

        Assert.True(runtime.Evaluate("chatboxpaste['/tell Bob, ']").IsTruthy);
        Assert.Equal("/tell Bob, ", automation.ComposedChat);
        Assert.Equal(["/say hi"], automation.SubmittedChat);

        Assert.True(runtime.Evaluate(
            "chatboxpaste[chr[9]+'a'+chr[10]+'b']").IsTruthy);
        Assert.Equal("ab", automation.ComposedChat);
        Assert.False(runtime.Evaluate("chatboxpaste[chr[9]]").IsTruthy);

        automation.CanCompose = false;
        Assert.False(runtime.Evaluate("chatboxpaste['/tell Bob, ']").IsTruthy);
    }

    /// <summary>
    /// Both chat verbs take a STRING; a number is a type error, not a number
    /// rendered as text. A status-hud colour, on the other hand, is a 32-bit
    /// pattern and a negative one is legal. Mutation: rendering the chat
    /// argument through the display form makes the first two probes pass
    /// "5" and "6" to chat, and taking the colour as a checked unsigned
    /// number makes the third throw an overflow.
    /// </summary>
    [Fact]
    public void ChatVerbsRequireStringsAndTheStatusColourAcceptsNegatives()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        automation.SubmittedChat.Clear();

        Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate("chatbox[5]"));
        Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate("chatboxpaste[6]"));
        Assert.Empty(automation.SubmittedChat);
        Assert.Equal(string.Empty, automation.ComposedChat);

        Assert.False(runtime.Evaluate("statushudcolored['k','v',0-1]").IsTruthy);
    }

    /// <summary>
    /// `uisetvisible` treats ANY non-zero number as visible and hands back its
    /// second argument; `uisetlabel` answers 1 and raises an error for a
    /// control that cannot take a label. Mutation: the previous ">= 1" test
    /// hides a control at 0.5, and returning the host's bool loses both the
    /// echoed argument and the error.
    /// </summary>
    [Fact]
    public void UiVisibilityUsesNonZeroTruthAndLabelFailureIsAnError()
    {
        var automation = CreateAutomation();
        var ui = new RecordingUiRegistry();
        using var runtime = new MossTankExpressionRuntime(new Host(automation, ui: ui));

        Assert.Equal(0.5d, runtime.Evaluate(
            "uisetvisible[uigetcontrol['view','control'],0.5]").AsNumber());
        Assert.True(ui.LastVisible);
        Assert.Equal(-1d, runtime.Evaluate(
            "uisetvisible[uigetcontrol['view','control'],0-1]").AsNumber());
        Assert.True(ui.LastVisible);
        Assert.Equal(0d, runtime.Evaluate(
            "uisetvisible[uigetcontrol['view','control'],0]").AsNumber());
        Assert.False(ui.LastVisible);

        Assert.Equal(1d, runtime.Evaluate(
            "uisetlabel[uigetcontrol['view','control'],'Go']").AsNumber());
        ui.LabelAccepted = false;
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate(
            "uisetlabel[uigetcontrol['view','control'],'Go']"));
    }

    [Fact]
    public void NetworkClientsReturnUtilityBeltDictionariesAndTagFiltering()
    {
        var automation = CreateAutomation();
        automation.NetworkClients =
        [
            new PluginNetworkClient(
                7u,
                70u,
                "Remote Mule",
                "Coldeve",
                automation.Position,
                ["mules", "trade"],
                90u,
                70u,
                80u,
                100u,
                100u,
                100u,
                90f),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        ExpressionList clients = runtime.Evaluate("netclients['mules']").AsList();
        ExpressionDictionary client = Assert.Single(clients.Items).AsDictionary();
        Assert.Equal("Remote Mule", client.Items["Name"].AsString());
        Assert.Equal(70d, client.Items["PlayerId"].AsNumber());
        Assert.Equal(2, client.Items["Tags"].AsList().Items.Count);
        Assert.Empty(runtime.Evaluate("netclients['combat']").AsList().Items);
    }

    [Fact]
    public void DelayedExecutionUsesTickAndCanBeCancelled()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation()));

        double id = runtime.Evaluate(
            "delayexec[100,\"setvar['done',1]\"]").AsNumber();
        runtime.OnTick(0.099);
        Assert.Equal(0d, runtime.Evaluate("getvar['done']").AsNumber());
        runtime.OnTick(0.001);
        Assert.Equal(1d, runtime.Evaluate("getvar['done']").AsNumber());

        double cancelled = runtime.Evaluate(
            "delayexec[1,\"setvar['bad',1]\"]").AsNumber();
        Assert.True(runtime.Evaluate($"clearexec[{cancelled}]").IsTruthy);
        runtime.OnTick(1d);
        Assert.Equal(0d, runtime.Evaluate("getvar['bad']").AsNumber());
        Assert.NotEqual(id, cancelled);
    }

    [Fact]
    public void PersistentAndGlobalVariablesRoundTripThroughPluginStorage()
    {
        var storage = new MemoryStorage();
        var automation = CreateAutomation();
        using (var first = new MossTankExpressionRuntime(new Host(automation, storage)))
        {
            first.Evaluate("setpvar['count',42];setgvar['names',listcreate['a','b']]");
        }

        using var second = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));
        Assert.Equal(42d, second.Evaluate("getpvar['count']").AsNumber());
        Assert.Equal(2d, second.Evaluate("listcount[getgvar['names']]").AsNumber());
        Assert.Contains(
            storage.Text.Keys,
            static key => key.StartsWith("expressions/persistent/", StringComparison.Ordinal));
        Assert.Contains(
            storage.Text.Keys,
            static key => key.StartsWith("expressions/global/", StringComparison.Ordinal));
    }

    [Fact]
    public void FellowshipFunctionsExposeTheAuthoritativeCompleteRoster()
    {
        var automation = CreateAutomation();
        automation.FellowRoster =
        [
            new PluginFellowMember(1, "Expression Tester", 90, 100, 80, 100, 70, 100, 0),
            new PluginFellowMember(22, "Fellow Two", 50, 60, 40, 60, 30, 60, 1),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate("getfellowshipstatus[]").IsTruthy);
        Assert.Equal("Test Fellowship", runtime.Evaluate("getfellowshipname[]").AsString());
        Assert.Equal(2d, runtime.Evaluate("getfellowshipcount[]").AsNumber());
        Assert.True(runtime.Evaluate("getfellowshipisleader[]").IsTruthy);
        Assert.True(runtime.Evaluate("getfellowshipcanrecruit[]").IsTruthy);
        Assert.Equal("Fellow Two", runtime.Evaluate("getfellowname[1]").AsString());
        Assert.Equal(22d, runtime.Evaluate("getfellowid[1]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("listcount[getfellowids[]]").AsNumber());
    }

    [Fact]
    public void DerethTimeFunctionsUseTheRuntimeWorldClockProjection()
    {
        var automation = CreateAutomation();
        automation.Time = new PluginWorldTimeSnapshot(
            true,
            123456d,
            142,
            6,
            17,
            10,
            "HarvestGain",
            "Warmtide",
            true,
            0d,
            12.5d);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(142d, runtime.Evaluate("getgameyear[]").AsNumber());
        Assert.Equal(6d, runtime.Evaluate("getgamemonth[]").AsNumber());
        Assert.Equal("HarvestGain", runtime.Evaluate("getgamemonthname[6]").AsString());
        Assert.Equal(17d, runtime.Evaluate("getgameday[]").AsNumber());
        Assert.Equal("Warmtide", runtime.Evaluate("getgamehourname[10]").AsString());
        Assert.Equal(123456d, runtime.Evaluate("getgameticks[]").AsNumber());
        Assert.True(runtime.Evaluate("getisday[]").IsTruthy);
        Assert.Equal(12.5d, runtime.Evaluate("getminutesuntilnight[]").AsNumber());
    }

    [Fact]
    public void ExperienceMeterAccumulatesCanonicalXpAndLuminanceDeltas()
    {
        var automation = CreateAutomation();
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            int64s: new Dictionary<uint, long> { [1] = 1000, [6] = 10 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        runtime.OnTick(1d);
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            int64s: new Dictionary<uint, long> { [1] = 1100, [6] = 15 });
        runtime.OnTick(1d);

        Assert.Equal(100d, runtime.Evaluate("xptotal[]").AsNumber());
        Assert.Equal(5d, runtime.Evaluate("lumtotal[]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("xpduration[]").AsNumber());
        Assert.Equal(180000d, runtime.Evaluate("xpavg[]").AsNumber());
        Assert.Contains("100 XP", runtime.Evaluate("xpmeter[]").AsString());
        Assert.True(runtime.Evaluate("xpreset[]").IsTruthy);
        Assert.Equal(0d, runtime.Evaluate("xptotal[]").AsNumber());
    }

    [Fact]
    public void QuestFunctionsParseTheAuthoritativeMyquestsTranscript()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        Assert.Equal(["/myquests"], automation.SubmittedChat);
        Assert.True(runtime.Evaluate("isrefreshingquests[]").IsTruthy);

        automation.ChatMessages.Add(new PluginChatMessage(
            1, 0, 0, string.Empty,
            "killtaskdrudges - 3 solves (0)\"Drudges killed\" 10 0",
            string.Empty));
        automation.ChatMessages.Add(new PluginChatMessage(
            2, 0, 0, string.Empty,
            "onevisit - 1 solves (1700000000)\"Visited once\" 1 0",
            string.Empty));
        runtime.OnTick(0d);

        Assert.True(runtime.Evaluate("testquestflag['killtaskdrudges']").IsTruthy);
        Assert.Equal(3d, runtime.Evaluate(
            "getquestktprogress['killtaskdrudges']").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "getquestktrequired['killtaskdrudges']").AsNumber());
        Assert.False(runtime.Evaluate("getqueststatus['onevisit']").IsTruthy);
        Assert.True(runtime.Evaluate("getqueststatus['unknown']").IsTruthy);

        runtime.OnTick(1.001d);
        Assert.False(runtime.Evaluate("isrefreshingquests[]").IsTruthy);
    }

    [Fact]
    public void CorpseFunctionsUseTheCanonicalExternalContainerHistory()
    {
        var automation = CreateAutomation();
        automation.Corpses =
        [
            new PluginLootContainer(30, 300, "Corpse", 2f, false, false, false),
            new PluginLootContainer(31, 301, "Corpse", 3f, true, false, false),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("hascorpsebeenopenedbyme[30]").IsTruthy);
        Assert.True(runtime.Evaluate("hascorpsebeenopenedbyme[31]").IsTruthy);
        Assert.Equal(30d, runtime.Evaluate(
            "listgetitem[getcorpsesunopenedbyme[],0]").AsNumber());
    }

    [Fact]
    public void ComponentFunctionsUseTheRetailDatCatalogProjection()
    {
        var automation = CreateAutomation();
        automation.Components[7] = new PluginSpellComponentInfo(
            7, 101, "Malar Herb", 0.25, 0x13000001, 0.75,
            0x06000010, 4, "Herb", "Malar");
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Malar Herb", runtime.Evaluate("componentname[7]").AsString());
        Assert.Equal(0.25d, runtime.Evaluate(
            "dictgetitem[componentdata[7],'BurnRate']").AsNumber());
        Assert.Equal("Malar", runtime.Evaluate(
            "dictgetitem[componentdata[7],'Word']").AsString());
    }

    [Fact]
    public void UstFunctionsStageAndSubmitThroughTheCanonicalSalvageCommand()
    {
        var automation = CreateAutomation();
        automation.InventoryItems =
        [
            InventoryItem(40, "Ust"),
            InventoryItem(41, "Salvage One"),
            InventoryItem(42, "Salvage Two"),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate("ustopen[]").IsTruthy);
        Assert.Equal(40u, automation.UsedObject);
        Assert.True(runtime.Evaluate("ustadd[41]").IsTruthy);
        Assert.True(runtime.Evaluate("ustadd[42]").IsTruthy);
        Assert.True(runtime.Evaluate("ustsalvage[]").IsTruthy);
        Assert.Equal(40u, automation.LastSalvage.Tool);
        Assert.Equal([41u, 42u], automation.LastSalvage.Items);
        Assert.False(runtime.Evaluate("ustsalvage[]").IsTruthy);
    }

    private static Automation CreateAutomation()
    {
        var automation = new Automation
        {
            Position = new PluginNavigationPosition(
                0x7F7F0001u, 10d, 20d, 3d, 90f, true),
            Attributes =
            [
                new PluginAttributeInfo(0, "Strength", 110) { Base = 100 },
            ],
            Skills =
            [
                new PluginSkillInfo(34, "War Magic", PluginSkillTraining.Trained, 300)
                {
                    Base = 275,
                },
            ],
        };
        automation.WorldObjects.Add(new PluginWorldObject(
            1, 1, "Expression Tester", PluginObjectClass.Player, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position,
            ItemsCapacity = 10,
            ContainersCapacity = 2,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            10, 100, "Health Elixir", PluginObjectClass.Food, 0x20, 1, 0)
        {
            IsOwned = true,
            StackSize = 10,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            11, 101, "Small Pack", PluginObjectClass.Container, 0x200, 1, 0)
        {
            IsOwned = true,
            ItemsCapacity = 8,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            20, 200, "Drudge", PluginObjectClass.Monster, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 10.1d },
            HasAppraisalData = true,
            LastIdTime = 54321,
        });
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            strings: new Dictionary<uint, string> { [1] = "Expression Tester" });
        automation.Properties[20] = Properties(
            ints: new Dictionary<uint, int> { [25] = 77 });
        return automation;
    }

    private static PluginItemProperties Properties(
        IReadOnlyDictionary<uint, int>? ints = null,
        IReadOnlyDictionary<uint, long>? int64s = null,
        IReadOnlyDictionary<uint, string>? strings = null) => new(
            ints ?? new Dictionary<uint, int>(),
            int64s ?? new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            strings ?? new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());

    private static PluginSpellInfo Spell(uint id, uint school, int difficulty) => new(
        id,
        $"Spell {id}",
        1u,
        1,
        difficulty,
        10,
        30f,
        school,
        string.Empty,
        false,
        true);

    private static PluginInventoryItem InventoryItem(uint id, string name) => new(
        id, 0u, name, 0u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
        1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private sealed class Host(
        Automation automation,
        IPluginStorage? storage = null,
        IUiRegistry? ui = null) : IPluginHost
    {
        public bool HasUi => ui is not null;
        public IPluginLogger Log { get; } = new Logger();
        public IGameState State { get; } = new State();
        public IEvents Events { get; } = new Events();
        public Selection Selection { get; } = new();
        ISelectionService IPluginHost.Selection => Selection;
        public IUiRegistry Ui => ui ?? NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } = storage ?? NoOpPluginStorage.Instance;
        public IAutomationSurface Automation { get; } = automation;
    }

    private sealed class Automation :
        IAutomationSurface,
        ICharacterInfo,
        ISpellCatalog,
        IMagicCommands,
        IPluginChat,
        IWorldObjectAutomation,
        IItemAutomation,
        INavigationAutomation,
        IFellowshipAutomation,
        IWorldTimeAutomation,
        ILootAutomation,
        ILoginAutomation,
        INetworkAutomation,
        IEquipmentAutomation,
        ICombatAutomation
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        ISpellCatalog IAutomationSurface.Spells => this;
        public IMagicCommands Magic => this;
        IEquipmentAutomation IAutomationSurface.Equipment => this;
        public ICombatAutomation Combat => this;
        public IPluginChat Chat => this;
        public IWorldObjectAutomation Objects => this;
        public IItemAutomation Items => this;
        public INavigationAutomation Navigation => this;
        public IFellowshipAutomation Fellowship => this;
        public ILootAutomation Loot => this;
        public IWorldTimeAutomation WorldTime => this;
        public ILoginAutomation Login => this;
        public INetworkAutomation Network => this;
        public bool IsInWorld => true;
        public string Name => "Expression Tester";
        public string WorldName => "Coldeve";
        public string AccountName => "testaccount";
        public int CharacterIndex => 2;
        public uint ObjectId => 1;
        public uint CurrentHealth => 90;
        public uint MaxHealth => 100;
        public uint BaseHealth => 80;
        public uint CurrentStamina => 80;
        public uint MaxStamina => 100;
        public uint BaseStamina => 70;
        public uint CurrentMana => 70;
        public uint MaxMana => 100;
        public uint BaseMana => 60;
        public IReadOnlyList<PluginSkillInfo> Skills { get; set; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; set; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];
        public IReadOnlyList<PluginSpellInfo> KnownAttackSpells => [];
        public IReadOnlyList<PluginSpellInfo> KnownCombatSpells => [];
        public List<PluginWorldObject> WorldObjects { get; } = [];
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];
        public Dictionary<uint, PluginSpellComponentInfo> Components { get; } = [];
        public PluginNavigationPosition Position { get; set; }
        public uint UsedObject { get; private set; }
        public (uint SpellId, uint TargetId) LastCast { get; private set; }
        public PluginMovementIntent LastIntent { get; private set; }
        public int ClearMovementCount { get; private set; }
        public List<PluginChatMessage> ChatMessages { get; } = [];
        public List<string> SubmittedChat { get; } = [];
        public IReadOnlyList<PluginInventoryItem> InventoryItems { get; set; } = [];
        public (uint Tool, IReadOnlyList<uint> Items) LastSalvage { get; private set; }
        public IReadOnlyList<PluginFellowMember> FellowRoster { get; set; } = [];
        public IReadOnlyList<PluginLoginCharacter> LoginRoster { get; set; } = [];
        public uint NextLoginObjectId { get; private set; }
        public IReadOnlyList<PluginNetworkClient> NetworkClients { get; set; } = [];
        public IReadOnlyList<PluginLootContainer> Corpses { get; set; } = [];
        public IReadOnlyList<PluginLootContainer> CaptureCorpses(float maximumDistance) =>
            Corpses.Where(corpse => corpse.Distance <= maximumDistance).ToArray();
        public bool IsInFellowship => FellowRoster.Count != 0;
        string IFellowshipAutomation.Name => "Test Fellowship";
        public uint LeaderObjectId => 1;
        public bool IsOpen => true;
        public bool IsLocked => false;
        public int MemberCount => FellowRoster.Count;
        IReadOnlyList<PluginFellowMember> IFellowshipAutomation.CaptureRoster() =>
            FellowRoster;
        bool ILoginAutomation.IsAvailable => true;
        IReadOnlyList<PluginLoginCharacter> ILoginAutomation.CaptureRoster() =>
            LoginRoster;
        public bool SetNextLogin(uint characterObjectId)
        {
            if (!LoginRoster.Any(character =>
                character.ObjectId == characterObjectId
                && !character.IsPendingDelete))
            {
                return false;
            }
            NextLoginObjectId = characterObjectId;
            return true;
        }
        public bool ClearNextLogin()
        {
            NextLoginObjectId = 0u;
            return true;
        }
        bool INetworkAutomation.IsAvailable => true;
        IReadOnlyList<PluginNetworkClient> INetworkAutomation.CaptureClients() =>
            NetworkClients;
        public PluginWorldTimeSnapshot Time { get; set; }
        PluginWorldTimeSnapshot IWorldTimeAutomation.Snapshot => Time;

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in Skills)
            {
                if (candidate.SkillId == skillId)
                {
                    skill = candidate;
                    return true;
                }
            }
            skill = default;
            return false;
        }

        public Dictionary<uint, PluginSpellInfo> Spells { get; } = [];
        public bool HasComponents { get; set; } = true;
        public IReadOnlyList<PluginEquipmentItem> Equipment { get; set; } = [];
        public uint EquippedObject { get; private set; }
        public PluginCombatMode Mode { get; set; } = PluginCombatMode.Peace;
        public PluginCombatMode RequestedMode { get; private set; }

        public bool IsBusy { get; set; }

        public bool IsKnown(uint spellId) => spellId == 1001 || Spells.ContainsKey(spellId);
        public bool TryGet(uint spellId, out PluginSpellInfo info) =>
            Spells.TryGetValue(spellId, out info);
        bool IMagicCommands.HasComponents(uint spellId) => HasComponents;

        IReadOnlyList<PluginEquipmentItem> IEquipmentAutomation.CaptureOwnedEquipment() =>
            Equipment;
        PluginEquipmentCommandResult IEquipmentAutomation.Equip(
            uint objectId,
            uint requestedLocation)
        {
            EquippedObject = objectId;
            return new PluginEquipmentCommandResult(
                PluginEquipmentCommandStatus.Started);
        }

        PluginCombatSnapshot ICombatAutomation.Snapshot => new(
            0u, Mode, PluginAttackHeight.Medium, 0f, 0f, false, false, false, false);
        IReadOnlyList<PluginCombatTarget> ICombatAutomation.CaptureHostileTargets(
            float maximumDistance) => [];
        PluginCombatCommandResult ICombatAutomation.EnterDefaultMode() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.EnterMode(PluginCombatMode mode)
        {
            RequestedMode = mode;
            return new PluginCombatCommandResult(
                PluginCombatCommandStatus.ModeChangeSent);
        }
        PluginCombatCommandResult ICombatAutomation.BeginPhysicalAttack(
            uint targetObjectId,
            PluginAttackHeight height,
            float power) => new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.ReleasePhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.AbortPhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        public bool TryGetComponent(uint componentId, out PluginSpellComponentInfo info) =>
            Components.TryGetValue(componentId, out info);
        public bool IsCasting => false;
        public PluginCastGate EvaluateGate(uint spellId) =>
            spellId == 1001 ? PluginCastGate.Ready : PluginCastGate.NotKnown;
        public PluginCastGate EvaluateGate(uint spellId, uint targetObjectId) =>
            EvaluateGate(spellId);
        public bool Cast(uint spellId) => Cast(spellId, 0);
        public bool Cast(uint spellId, uint targetObjectId)
        {
            LastCast = (spellId, targetObjectId);
            return true;
        }

        public IReadOnlyList<PluginWorldObject> CaptureObjects() => WorldObjects;
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginWorldObject candidate in WorldObjects)
            {
                if (candidate.ObjectId == objectId)
                {
                    value = candidate;
                    return true;
                }
            }
            value = default;
            return false;
        }
        public bool TryCaptureProperties(uint objectId, out PluginItemProperties value) =>
            Properties.TryGetValue(objectId, out value);

        public PluginItemCommandResult Use(uint objectId)
        {
            UsedObject = objectId;
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => InventoryItems;
        public PluginItemCommandResult Salvage(
            uint toolObjectId,
            IReadOnlyList<uint> itemObjectIds)
        {
            LastSalvage = (toolObjectId, itemObjectIds.ToArray());
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        public PluginNavigationSnapshot Snapshot => new(
            true,
            false,
            ObjectId,
            Position,
            false,
            false);
        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            if (TryGet(objectId, out PluginWorldObject obj) && obj.HasPosition)
            {
                value = new PluginNavigationObject(obj.ObjectId, obj.Name, obj.Position);
                return true;
            }
            value = default;
            return false;
        }
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent)
        {
            LastIntent = intent;
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            ClearMovementCount++;
            return PluginNavigationCommandStatus.Accepted;
        }
        public void PostSystemMessage(string text) { }
        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            ChatMessages.Where(message => message.Sequence > afterSequence).ToArray();
        public bool Submit(string text)
        {
            SubmittedChat.Add(text);
            return true;
        }
        public bool CanCompose { get; set; } = true;
        public string ComposedChat { get; private set; } = string.Empty;
        public bool Compose(string text)
        {
            if (!CanCompose)
                return false;
            ComposedChat = text;
            return true;
        }
    }

    private sealed class RecordingUiRegistry : IUiRegistry
    {
        public bool LabelAccepted { get; set; } = true;
        public bool LastVisible { get; private set; }

        public void AddMarkupPanel(string markupPath, object binding) { }
        public bool ViewExists(string viewName) => true;
        public bool IsViewVisible(string viewName) => true;
        public bool ControlExists(string viewName, string controlName) => true;
        public bool SetControlLabel(string viewName, string controlName, string label) =>
            LabelAccepted;
        public bool SetControlVisible(string viewName, string controlName, bool visible)
        {
            LastVisible = visible;
            return true;
        }
    }

    private sealed class Selection : ISelectionService
    {
        public uint? SelectedObjectId { get; private set; }
        public uint? PreviousObjectId { get; private set; }
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId)
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = objectId;
            return true;
        }
        public bool Clear()
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = null;
            return true;
        }
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = [];
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class Logger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class State : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class Events : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }
        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }
}
