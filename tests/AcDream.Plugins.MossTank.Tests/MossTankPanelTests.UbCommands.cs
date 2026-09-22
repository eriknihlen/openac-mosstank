using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The commands a meta author types. Each one is pinned on the grammar it
/// accepts and on what reaches the client afterwards, because a command that
/// parses and then calls nothing is exactly the failure a macro cannot see.
/// </summary>
public sealed partial class MossTankPanelTests
{
    // ── /vt ub, /vt help ────────────────────────────────────────────────

    [Fact]
    public void TheCompatibilityVersionLineNamesThePluginVersion()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "ub");

        Assert.Equal(
            $"MossTank UB compatibility {MossTankPanel.PluginVersion}",
            automation.Messages[0]);
    }

    [Fact]
    public void HelpForOneCommandPrintsItsUsageLine()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "help jump");

        Assert.Equal(
            "Syntax: /vt jump[swzxc] [heading] [holdtime]",
            automation.Messages[0]);
    }

    [Fact]
    public void HelpForAnUnknownCommandSaysSoRatherThanPrintingEverything()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "help notacommand");

        Assert.Equal(
            "No help found for command: notacommand",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Three commands the plugin answers to had no published usage line, so
    /// "/vt help vendor" said there was no such command while "/vt vendor"
    /// worked. The vendor lines repeat the reference's wording; netclients
    /// has a usage line and says plainly that this client cannot do it yet,
    /// which is a different answer from "no such command".
    /// Mutation: drop any of the three entries and its help is refused.
    /// </summary>
    [Theory]
    [InlineData("vendor", "/vt vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> | addsell[p] <item>}")]
    [InlineData("autovendor", "/vt autovendor <cancel|lootProfile>")]
    [InlineData("netclients", "/vt netclients <tag>")]
    public void TheVendorAndNetworkCommandsHaveTheirOwnUsageLines(
        string command,
        string usage)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "help " + command);

        Assert.Equal("Syntax: " + usage, automation.Messages[0]);
        Assert.NotEmpty(automation.Messages[1]);
    }

    /// <summary>
    /// The network commands work now, so the help describes them rather than
    /// warning the player off. The two broadcasts need usage lines of their
    /// own: a command with none is answered as though it did not exist.
    /// Mutation: drop either entry and its help is refused; leave the old
    /// warning on netclients and the first assertion fails.
    /// </summary>
    [Fact]
    public void TheNetworkCommandsAreDescribedRatherThanWarnedAbout()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "help netclients");
        Assert.DoesNotContain(
            "not available on this client yet",
            automation.Messages[1],
            StringComparison.Ordinal);

        automation.Messages.Clear();
        Command(panel, "help bc");
        Assert.Equal(
            "Syntax: /vt bc [millisecondDelay] <command>",
            automation.Messages[0]);

        automation.Messages.Clear();
        Command(panel, "help bct");
        Assert.Equal(
            "Syntax: /vt bct <tags> [millisecondDelay] <command>",
            automation.Messages[0]);
    }

    // ── /vt ig ──────────────────────────────────────────────────────────

    [Fact]
    public void TheItemGiverFindsItsTargetByTheWholeName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "ig muledItems to Zero Cool");

        // The target is resolved before the profile is read, so a missing
        // profile is proof the target was found.
        Assert.Contains(
            "Item giver profile not found: muledItems.",
            automation.Messages);
    }

    [Fact]
    public void TheItemGiversPartialFlagMatchesPartOfTheTargetName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "igp muledItems to Zero");

        Assert.Contains(
            "Item giver profile not found: muledItems.",
            automation.Messages);
    }

    [Fact]
    public void TheItemGiverWithoutThePartialFlagRefusesAHalfName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "ig muledItems to Zero");

        Assert.Contains("Item giver target not found: Zero.", automation.Messages);
    }

    [Fact]
    public void TheItemGiverWithoutATargetPrintsItsUsage()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "ig muledItems");

        Assert.Equal(
            "Syntax: /vt ig[p] <lootProfile> to <target>, or /vt ig stop",
            Assert.Single(automation.Messages));
    }

    // ── /vt jump, /vt simplejump ────────────────────────────────────────

    [Fact]
    public void ARunningForwardJumpPressesForwardAndShift()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jumpsw 180 500");
        panel.OnTick(0.1d);

        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.Forward);
        Assert.True(intent.Run);
        Assert.True(intent.Jump);
    }

    [Fact]
    public void ABackwardJumpPressesBackwardAndNotForward()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jumpsx 300");
        panel.OnTick(0.1d);

        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.Backward);
        Assert.False(intent.Forward);
    }

    [Fact]
    public void TheStrafeFlagsPressTheirOwnKeys()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var left = new MossTankPanel(new FakeHost(automation));

        Command(left, "jumpz 100");
        left.OnTick(0.1d);
        PluginMovementIntent leftIntent = automation.MovementIntents[^1];

        var rightAutomation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var right = new MossTankPanel(new FakeHost(rightAutomation));
        Command(right, "jumpc 100");
        right.OnTick(0.1d);
        PluginMovementIntent rightIntent = rightAutomation.MovementIntents[^1];

        Assert.True(leftIntent.StrafeLeft);
        Assert.False(leftIntent.StrafeRight);
        Assert.True(rightIntent.StrafeRight);
        Assert.False(rightIntent.StrafeLeft);
    }

    /// <summary>
    /// A jump verb with no direction letter is a tap straight up. Pressing
    /// forward for it would walk the character off whatever it is standing
    /// on, which is the whole difference between a loot jump and a fall.
    /// </summary>
    [Fact]
    public void ABareJumpPressesNoMovementKeyAtAll()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump");
        // Short of the shortest hold, so the charge is still being held and
        // the intent on the wire is the jump itself.
        panel.OnTick(0.01d);

        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.Jump);
        Assert.False(intent.Forward);
        Assert.False(intent.Backward);
        Assert.False(intent.StrafeLeft);
        Assert.False(intent.StrafeRight);
    }

    /// <summary>
    /// One number on its own is the hold time, not a heading: a jump in place
    /// is the common case, and a macro that meant a heading types two.
    /// </summary>
    [Fact]
    public void OneNumberIsTheHoldTimeAndNotAHeading()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(45f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump 300");

        Assert.Equal("Turning to heading 45 for jump.", automation.Messages[0]);
    }

    [Fact]
    public void AHoldTimeOverASecondIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jumpw 0 2000");

        Assert.Equal(
            "holdtime should be a number between 0 and 1000",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void AHeadingOutsideTheCompassIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jumpw 400 100");

        Assert.Equal(
            "direction should be a number between 0 and 359",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void ASecondJumpWhileOneIsInTheAirIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jumpw 0 100");
        automation.Messages.Clear();
        Command(panel, "jumpw 0 100");

        Assert.Equal(
            "You are already jumping. try again later.",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The older grammar still runs: a second word of true or false cannot be
    /// a hold time, so routes and macros written against it keep working.
    /// </summary>
    [Fact]
    public void TheOlderJumpGrammarStillStrafes()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(90f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump 90 true 800 strafeleft");
        panel.OnTick(0.1d);

        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.StrafeLeft);
        Assert.True(intent.Run);
    }

    [Fact]
    public void SimpleJumpJumpsWhereTheCharacterAlreadyStands()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(123f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "simplejump 200");
        panel.OnTick(0.1d);

        Assert.Equal("Turning to heading 123 for jump.", automation.Messages[0]);
        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.Jump);
        Assert.False(intent.Forward);
    }

    [Fact]
    public void SimpleJumpRefusesAHoldTimeOverASecond()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "simplejump 5000");

        Assert.Equal(
            "holdtime should be a number between 0 and 1000",
            Assert.Single(automation.Messages));
    }

    // ── /vt calcdamage ──────────────────────────────────────────────────

    [Fact]
    public void CalcDamageAddsTheCantripAndTheTinkersStillAvailable()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    70u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u)
                {
                    HasAppraisalData = true,
                    // Major Blood Thirst: +4 to the maximum damage.
                    SpellIds = [2586u],
                },
            ],
        };
        automation.Properties[70u] = new PluginItemProperties(
            new Dictionary<uint, int>
            {
                [204u] = 6,  // elemental damage bonus
                [171u] = 2,  // times tinkered
                [179u] = 0,  // not imbued
            },
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>())
        {
            WeaponProfile = new PluginWeaponProfile(
                0, 20, 0u, 50, 0.5d, 2d, 1d, 1d, 1d, 0),
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(70u);

        Command(panel, "calcdamage");

        // Ten tinkers less two used and one reserved for the imbue: seven,
        // each worth 0.04 on the damage modifier.
        Assert.Contains(
            "7 mahogany salvage adds 0.28 to DamageModifier",
            automation.Messages);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "Calculated Formula: (50(+4 from cantrips) + 6)",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// Each cantrip that lifts the maximum damage is named, in the order the
    /// item carries its spells and before the sums that use them: the total
    /// "+6 from cantrips" says nothing about which two spells made it.
    /// Mutation: drop the per-spell lines and only the total is printed.
    /// </summary>
    [Fact]
    public void CalcDamageNamesEveryCantripThatLiftsTheMaximumDamage()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    72u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u)
                {
                    HasAppraisalData = true,
                    // Major Blood Thirst (+4), then Minor Blood Thirst (+2).
                    SpellIds = [2586u, 1u, 2598u],
                },
            ],
        };
        automation.CatalogSpells =
        [
            NamedSpell(2586u, 1u, "Major Blood Thirst", 1u),
            NamedSpell(2598u, 1u, "Minor Blood Thirst", 1u),
        ];
        automation.Properties[72u] = new PluginItemProperties(
            new Dictionary<uint, int>
            {
                [204u] = 0,
                [171u] = 10,  // no tinkers left, so no salvage line
                [179u] = 1,
            },
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>())
        {
            WeaponProfile = new PluginWeaponProfile(
                0, 20, 0u, 50, 0.5d, 2d, 1d, 1d, 1d, 0),
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(72u);

        Command(panel, "calcdamage");

        Assert.Equal(
            [
                "Spell Major Blood Thirst buffs MaxDamage by 4",
                "Spell Minor Blood Thirst buffs MaxDamage by 2",
            ],
            automation.Messages.Take(2));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "Calculated Formula: (50(+6 from cantrips) + 0)",
                StringComparison.Ordinal));
    }

    [Fact]
    public void CalcDamageRefusesAnItemThatHasNotBeenExamined()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    71u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(71u);

        Command(panel, "calcdamage");

        Assert.Equal(
            "Ice Bow does not have id data, please examine it first.",
            Assert.Single(automation.Messages));
    }

    // ── /vt opt ─────────────────────────────────────────────────────────

    [Fact]
    public void AnOptionNameWithADotReachesTheUbSettings()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "opt set AutoVendor.Enabled true");
        automation.Messages.Clear();
        Command(panel, "opt get AutoVendor.Enabled");

        Assert.Equal(
            "AutoVendor.Enabled (Bool) = True",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void ToggleFlipsAUbSwitch()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "opt get DungeonMaps.Enabled");
        string before = Assert.Single(automation.Messages);
        automation.Messages.Clear();

        Command(panel, "opt toggle DungeonMaps.Enabled");

        string after = Assert.Single(automation.Messages);
        Assert.NotEqual(before, after);
        Assert.Equal(
            before.EndsWith("True", StringComparison.Ordinal)
                ? "DungeonMaps.Enabled (Bool) = False"
                : "DungeonMaps.Enabled (Bool) = True",
            after);
    }

    /// <summary>
    /// Where the two groups could both answer a name, the macro's own option
    /// wins: a meta that has always meant EnableCombat must keep meaning it.
    /// </summary>
    [Fact]
    public void ToggleFlipsTheMacrosOwnOptionByItsPlainName()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        bool before = panel.GetMetaOptionForTest("EnableCombat");

        Command(panel, "opt toggle EnableCombat");

        Assert.Equal(!before, panel.GetMetaOptionForTest("EnableCombat"));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("Set option EnableCombat", StringComparison.Ordinal));
    }

    [Fact]
    public void TogglingSomethingThatIsNotASwitchSaysSo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "opt toggle DungeonMaps.Opacity");

        Assert.Equal(
            "Unable to toggle setting DungeonMaps.Opacity: it is not a switch.",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void TheOptionListShowsBothGroups()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "opt list");

        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("Available options:", StringComparison.Ordinal));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("UB settings:", StringComparison.Ordinal));
    }

    // ── /vt pos, /vt id, /vt vitae, /vt combatstate, /vt date ───────────

    [Fact]
    public void PosPrintsTheSelectedObjectsCoordinatesAndCell()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                new PluginWorldObject(
                    80u, 0u, "Lifestone", PluginObjectClass.Lifestone, 0u, 0u, 0u)
                {
                    HasPosition = true,
                    IsLandscape = true,
                    Position = new PluginNavigationPosition(
                        0x00AB0102u, 3.5d, -1.25d, 12d, 0f, IsOutdoor: true),
                },
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(80u);

        Command(panel, "pos");

        Assert.Equal("Id: 80 ( 0x00000050 )", automation.Messages[0]);
        Assert.Equal("Coords: 1.2S, 3.5E", automation.Messages[1]);
        Assert.Equal("Landcell: 0x00AB0102", automation.Messages[2]);
    }

    [Fact]
    public void PosWithNothingSelectedSaysSo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "pos");

        Assert.Equal("pos: No object selected", Assert.Single(automation.Messages));
    }

    [Fact]
    public void IdPrintsTheSelectedObjectsIdBothWaysRound()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    0x50000123u, 0u, "Mule", PluginObjectClass.Player, 0u, 0u, 0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x50000123u);

        Command(panel, "id");

        Assert.Equal(
            "Id: 1342177571 ( 0x50000123 )",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void VitaeIsThoughtAsAPercentage()
    {
        var automation = new FakeAutomation { ObjectId = 1u };
        automation.Properties[1u] = new PluginItemProperties(
            new Dictionary<uint, int>(),
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double> { [129u] = 0.95d },
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "vitae");

        Assert.Contains("/t Test Character, My vitae is 95%", automation.Submitted);
    }

    [Fact]
    public void CombatStateAsksTheClientToChangeStance()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "combatstate melee");

        Assert.Equal(PluginCombatMode.Melee, automation.CombatSnapshot.Mode);
    }

    [Fact]
    public void AnUnknownCombatStateIsRefusedByName()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "combatstate sideways");

        Assert.Equal(
            "sideways is not a valid option",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The date is formatted in the invariant culture, so a macro reading its
    /// own chat sees the same words on a Swedish machine as on a US one.
    /// </summary>
    [Fact]
    public void TheUtcDateIsFormattedInTheInvariantCulture()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "dateutc dddd");

        Assert.Equal(
            "Current Date: "
            + DateTime.UtcNow.ToString("dddd", CultureInfo.InvariantCulture),
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void ABareDateUsesTheDefaultFormat()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "date");

        Assert.Equal(
            "Current Date: "
            + DateTime.Now.ToString(
                "dddd dd MMMM HH:mm:ss", CultureInfo.InvariantCulture),
            Assert.Single(automation.Messages));
    }

    // ── /vt delay ───────────────────────────────────────────────────────

    [Fact]
    public void ADelayedCommandRunsOnlyOnceItsWaitIsOver()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "delay 1000 /say hello");
        panel.OnTick(0.5d);
        Assert.DoesNotContain("/say hello", automation.Submitted);

        panel.OnTick(0.6d);

        Assert.Contains("/say hello", automation.Submitted);
    }

    [Fact]
    public void ADelayWithoutACommandPrintsItsUsage()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "delay 1000");

        Assert.Equal(
            "Syntax: /vt delay <milliseconds> <command>",
            Assert.Single(automation.Messages));
    }

    // ── /vt closestportal, /vt portal ───────────────────────────────────

    [Fact]
    public void ClosestPortalUsesTheNearestPortal()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                Landscape(90u, "Far Portal", PluginObjectClass.Portal, 40d),
                Landscape(91u, "Near Portal", PluginObjectClass.Portal, 5d),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "closestportal");

        Assert.Equal(91u, Assert.Single(automation.UsedItemIds));
        Assert.Contains("Attempting to use portal: Near Portal", automation.Messages);
    }

    [Fact]
    public void PortalByPartialNameUsesTheMatchingPortal()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                Landscape(92u, "Gateway", PluginObjectClass.Portal, 5d),
                Landscape(93u, "Holtburg Portal", PluginObjectClass.Portal, 40d),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "portalp Holt");

        Assert.Equal(93u, Assert.Single(automation.UsedItemIds));
    }

    [Fact]
    public void APortalNobodyCanSeeIsReported()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "portal Gateway");

        Assert.Equal("Could not find a portal", Assert.Single(automation.Messages));
    }

    // ── /vt follow ──────────────────────────────────────────────────────

    [Fact]
    public void FollowByPartialNameNamesThePlayerItLockedOn()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Player(0x50000ABCu, "Zero Cool")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "followp Zero");

        Assert.Contains("Following Zero Cool[0x50000ABC]", automation.Messages);
    }

    [Fact]
    public void FollowingNobodySaysWhoWasNotFound()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "follow Zero Cool");

        Assert.Contains("Could not find player Zero Cool", automation.Messages);
    }

    // ── /vt use, /vt select, /vt close ──────────────────────────────────

    [Fact]
    public void UseOnItsOwnUsesTheNamedItem()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(100u, "Prismatic Taper")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "use Prismatic Taper");

        Assert.Equal(100u, Assert.Single(automation.UsedItemIds));
        Assert.Contains("using Prismatic Taper", automation.Messages);
    }

    [Fact]
    public void UseOnASecondItemAppliesTheFirstToTheSecond()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(101u, "Splitting Tool"), Carried(102u, "Gold Pea")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "usep splitting on gold pea");

        Assert.Equal((101u, 102u), Assert.Single(automation.Applied));
        Assert.Contains("using Splitting Tool on Gold Pea", automation.Messages);
    }

    [Fact]
    public void UseRefusesBothPlaceFlagsAtOnce()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "useli Cake");

        Assert.Equal(
            "l and i cannot be used in the same command",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The inventory flag really is a filter: an item of that name standing
    /// on the landscape must not answer a command that asked the packs.
    /// </summary>
    [Fact]
    public void TheInventoryFlagWillNotReachTheLandscape()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Landscape(103u, "Cake", PluginObjectClass.Food, 3d)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "usei Cake");

        Assert.Empty(automation.UsedItemIds);
        Assert.Contains("Could not find object: Cake", automation.Messages);
    }

    [Fact]
    public void SelectPicksTheNamedItem()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(104u, "Gold Scarab")],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "selectp gold");

        Assert.Equal(104u, host.Selection.SelectedObjectId);
    }

    [Fact]
    public void CloseCorpseClosesTheOpenCorpse()
    {
        var loot = new UbFakeLoot();
        var automation = new FakeAutomation
        {
            LootSurface = loot,
            OpenContainerObjectId = 110u,
            WorldObjects =
            [
                new PluginWorldObject(
                    110u, 0u, "Corpse of a Drudge", PluginObjectClass.Corpse,
                    0u, 0u, 0u),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "close corpse");

        Assert.Equal(110u, Assert.Single(loot.Closed));
    }

    [Fact]
    public void CloseCorpseWithNothingOpenSaysSo()
    {
        var loot = new UbFakeLoot();
        var automation = new FakeAutomation { LootSurface = loot };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "close corpse");

        Assert.Equal(
            "No container is currently open.",
            Assert.Single(automation.Messages));
        Assert.Empty(loot.Closed);
    }

    // ── /vt swearallegiance, /vt breakallegiance ────────────────────────

    /// <summary>
    /// The client's answer to the two allegiance commands, so a test can say
    /// what came back and read which object the command named.
    /// </summary>
    private sealed class FakeAllegiance(PluginAllegianceCommandResult result)
        : IAllegianceAutomation
    {
        public bool IsAvailable => true;
        public List<uint> Sworn { get; } = [];
        public List<uint> Broken { get; } = [];

        public PluginAllegianceCommandResult Swear(uint patronObjectId)
        {
            Sworn.Add(patronObjectId);
            return result;
        }

        public PluginAllegianceCommandResult Break(uint targetObjectId)
        {
            Broken.Add(targetObjectId);
            return result;
        }
    }

    private static FakeAutomation AllegianceRig(
        PluginAllegianceCommandResult result,
        out FakeAllegiance allegiance)
    {
        allegiance = new FakeAllegiance(result);
        return new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Player(0x50000AAAu, "Yonneh")],
            AllegianceSurface = allegiance,
        };
    }

    /// <summary>
    /// The command resolves who it means and hands that object to the client.
    /// Mutation: pass the selected object instead of the resolved one and the
    /// recorded id is wrong; drop the call and nothing is recorded at all.
    /// </summary>
    [Fact]
    public void SwearingAllegianceSendsTheResolvedPlayerToTheClient()
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(PluginAllegianceCommandStatus.Sent),
            out FakeAllegiance allegiance);
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "swearallegiancep Yon");

        Assert.Equal(0x50000AAAu, Assert.Single(allegiance.Sworn));
        Assert.Equal(
            "Swearing allegiance to Yonneh[0x50000AAA].",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void BreakingAllegianceSendsTheResolvedPlayerToTheClient()
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(PluginAllegianceCommandStatus.Sent),
            out FakeAllegiance allegiance);
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "breakallegiance Yonneh");

        Assert.Equal(0x50000AAAu, Assert.Single(allegiance.Broken));
        Assert.Equal(
            "Breaking allegiance from Yonneh[0x50000AAA].",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Each of the four answers reads differently, because a macro author
    /// watching chat has to tell "it went out" from "it never will".
    /// Mutation: fold any two statuses onto one line and its row fails.
    /// </summary>
    [Theory]
    [InlineData(
        PluginAllegianceCommandStatus.InvalidTarget,
        null,
        "Cannot swear allegiance to Yonneh[0x50000AAA].",
        "Yonneh[0x50000AAA] is not in your allegiance.")]
    [InlineData(
        PluginAllegianceCommandStatus.Refused,
        "You are already sworn to somebody.",
        "You are already sworn to somebody.",
        "You are already sworn to somebody.")]
    [InlineData(
        PluginAllegianceCommandStatus.Refused,
        null,
        "Refused to swear allegiance to Yonneh[0x50000AAA].",
        "Refused to break allegiance from Yonneh[0x50000AAA].")]
    [InlineData(
        PluginAllegianceCommandStatus.Unavailable,
        null,
        "Cannot swear allegiance right now.",
        "Cannot break allegiance right now.")]
    public void EveryAllegianceAnswerHasItsOwnLine(
        PluginAllegianceCommandStatus status,
        string? notice,
        string swearLine,
        string breakLine)
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(status, notice),
            out _);
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "swearallegiance Yonneh");
        Assert.Equal(swearLine, Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Command(panel, "breakallegiance Yonneh");
        Assert.Equal(breakLine, Assert.Single(automation.Messages));
    }

    // ── /vt printcolors ─────────────────────────────────────────────────

    [Fact]
    public void PrintColorsWritesOneLineInEveryTextClass()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "printcolors");

        Assert.Equal(UbChatMessageTypes.All.Count, automation.Posted.Count);
        Assert.Equal(("[PrintColors]Broadcast (0)", 0), automation.Posted[0]);
        Assert.Contains(("[PrintColors]Magic (7)", 7), automation.Posted);
    }

    // ── /vt listvars, listpvars, listgvars ──────────────────────────────

    [Fact]
    public void ListVarsPrintsTheSessionStore()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "mexec setvar[myvar,1]");
        automation.Messages.Clear();
        Command(panel, "listvars");

        Assert.Equal("Defined variables:", automation.Messages[0]);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("myvar (", StringComparison.Ordinal));
    }

    /// <summary>
    /// The three stores are separate: a session variable must not appear in
    /// the character's list, or a macro would read a value it never saved.
    /// </summary>
    [Fact]
    public void ListPvarsAndListGvarsReadTheirOwnStores()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "mexec setvar[sessiononly,1]");
        Command(panel, "mexec setgvar[serverwide,2]");
        automation.Messages.Clear();
        Command(panel, "listpvars");
        Command(panel, "listgvars");

        Assert.Equal("Defined persistent variables:", automation.Messages[0]);
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.StartsWith("sessiononly ", StringComparison.Ordinal));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("serverwide (", StringComparison.Ordinal));
    }

    // ── /vt translateroute ──────────────────────────────────────────────

    [Fact]
    public void TranslateRouteShiftsEveryPointByTheLandblockDistance()
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));

        Command(panel, "addnavpt 10.0N, 20.0E");
        Command(panel, "nav save eo-east");
        automation.Messages.Clear();

        // One landblock step east is 192 m, which is 0.8 coordinates. The
        // east/west block is the top byte of the landblock id.
        Command(panel, "translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav");

        string saved = Assert.Single(
            vtank.Text,
            static pair => pair.Key.Contains("eo-main", StringComparison.Ordinal))
            .Value;
        Assert.Contains("pnt 20.8 10 0", saved, StringComparison.Ordinal);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("Translated ", StringComparison.Ordinal));
    }

    [Fact]
    public void TranslateRouteRefusesToOverwriteWithoutTheForceFlag()
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));

        Command(panel, "addnavpt 10.0N, 20.0E");
        Command(panel, "nav save eo-east");
        Command(panel, "nav save eo-main");
        automation.Messages.Clear();

        Command(panel, "translateroute 0x00640371 eo-east.nav 0x00650371 eo-main.nav");

        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "Output path already exists!", StringComparison.Ordinal));
    }

    [Fact]
    public void TranslateRouteRefusesALandblockThatIsNotHex()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "translateroute zzz a.nav 0x00650371 b.nav");

        Assert.Equal(
            "Could not parse hex value from StartLandblock: zzz",
            Assert.Single(automation.Messages));
    }

    // ── fixtures ────────────────────────────────────────────────────────

    private static PluginWorldObject Player(uint objectId, string name) =>
        new(objectId, 0u, name, PluginObjectClass.Player, 0u, 0u, 0u)
        {
            HasPosition = true,
            IsLandscape = true,
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
        };

    private static PluginWorldObject Landscape(
        uint objectId,
        string name,
        PluginObjectClass objectClass,
        double eastWest) =>
        new(objectId, 0u, name, objectClass, 0u, 0u, 0u)
        {
            HasPosition = true,
            IsLandscape = true,
            Position = new PluginNavigationPosition(
                0x00010001u, eastWest, 0d, 0d, 0f, IsOutdoor: true),
        };

    /// <summary>A loot surface that only records the opens and closes.</summary>
    private sealed class UbFakeLoot : ILootAutomation
    {
        public List<uint> Opened { get; } = [];
        public List<uint> Closed { get; } = [];

        public bool IsAvailable => true;
        public bool IsBusy => false;
        public uint RequestedContainerId => 0u;
        public uint CurrentContainerId => 0u;
        public bool CurrentContentsReady => false;
        public PluginItemUseCompletion LastItemUseCompletion => default;
        public PluginInventoryCompletion LastInventoryCompletion => default;
        public PluginAppraisalState Appraisal => default;

        public IReadOnlyList<PluginLootContainer> CaptureCorpses(
            float maximumDistance) => [];

        public IReadOnlyList<PluginInventoryItem> CaptureCurrentContents() => [];

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            properties = default;
            return false;
        }

        public PluginItemCommandResult Open(uint objectId)
        {
            Opened.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public PluginItemCommandResult Close(uint objectId)
        {
            Closed.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public PluginItemCommandResult Identify(uint objectId) =>
            new(PluginItemCommandStatus.Started);

        public PluginItemCommandResult Pickup(uint objectId, bool toMainPack) =>
            new(PluginItemCommandStatus.Started);
    }
}
