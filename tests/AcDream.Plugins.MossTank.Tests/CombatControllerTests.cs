using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class CombatControllerTests
{
    /// <summary>
    /// Mutation: put the approach back inside the attack (build the attack's
    /// candidates out to the approach range and walk from there) and the
    /// second half fails — the attack would claim the pass with the monster
    /// still twelve metres away, so nothing below it would ever run.
    /// </summary>
    [Fact]
    public void WalkingToAMonsterIsItsOwnJobBelowTheAttack()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", distance: 12, angle: 0)],
            NavigationSnapshot = NavigationAt(heading: 0f),
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        surface.NavigationObjects[10u] = new PluginNavigationObject(
            10u,
            "Drudge",
            new PluginNavigationPosition(0x7F7F0001, 0d, 0.1d, 0d, 0f, true));
        var settings = new CombatSettings
        {
            MaximumRange = 5f,
            ApproachDistance = 20f,
            ScanIntervalSeconds = 0.05d,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.05d);

        // The attack has nothing to do: the monster is out of weapon range, so
        // it is not one of its candidates at all.
        Assert.False(controller.HasTarget);
        Assert.Empty(surface.MovementIntents);
        Assert.Equal(0, surface.BeginCount);

        Assert.True(controller.TickMonsterApproach(0.05d, canAct: true));
        PluginMovementIntent intent = Assert.Single(surface.MovementIntents);
        Assert.True(intent.Forward);
        Assert.Contains("Approaching", controller.Status, StringComparison.Ordinal);

        surface.Targets = [Target(10, "Drudge", distance: 4, angle: 0)];
        controller.OnTick(0.05d);

        Assert.Equal(10u, surface.LastBeginTarget);
        // Nothing left to walk to.
        Assert.False(controller.TickMonsterApproach(0.05d, canAct: true));
    }

    /// <summary>
    /// Mutation: drop the approach rule's own candidate pick and reuse the
    /// attack's target and this fails — the attack has no target at all here.
    /// </summary>
    [Fact]
    public void TheApproachPicksItsOwnTargetAtTheApproachRange()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets =
            [
                Target(10, "Drudge", distance: 12, angle: 0),
                Target(20, "Olthoi Soldier", distance: 18, angle: 30),
            ],
            NavigationSnapshot = NavigationAt(heading: 0f),
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        surface.NavigationObjects[20u] = new PluginNavigationObject(
            20u,
            "Olthoi Soldier",
            new PluginNavigationPosition(0x7F7F0001, 0.1d, 0d, 0d, 0f, true));
        var settings = new CombatSettings
        {
            MaximumRange = 5f,
            ApproachDistance = 20f,
            ScanIntervalSeconds = 0.05d,
            SelectionMethod = TargetSelectionMethod.Range,
        };
        settings.Rules.Insert(0, new MonsterRule("name#^Olthoi", 4));
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.05d);

        Assert.True(controller.TickMonsterApproach(0.05d, canAct: true));
        Assert.Contains(
            "Olthoi Soldier",
            controller.Status,
            StringComparison.Ordinal);
    }

    private static (FakeAutomation Surface, CombatController Controller)
        ApproachRig(float selfHeading, double targetEastWest, double targetNorthSouth)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", distance: 12, angle: 0)],
            NavigationSnapshot = NavigationAt(selfHeading),
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        surface.NavigationObjects[10u] = new PluginNavigationObject(
            10u,
            "Drudge",
            new PluginNavigationPosition(
                0x7F7F0001,
                targetEastWest,
                targetNorthSouth,
                0d,
                0f,
                true));
        var settings = new CombatSettings
        {
            MaximumRange = 5f,
            ApproachDistance = 20f,
            ScanIntervalSeconds = 0.05d,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        return (surface, controller);
    }

    [Fact]
    public void ApproachStopsAndFacesTheTargetOutsideTheFourDegreeBand()
    {
        (FakeAutomation surface, CombatController controller) =
            ApproachRig(selfHeading: 0f, targetEastWest: 0.0839d, targetNorthSouth: 0.1d);

        controller.TickMonsterApproach(0.05d, canAct: true);

        Assert.Empty(surface.MovementIntents);
        Assert.Equal(1, surface.ClearMovementCount);
        float faced = Assert.Single(surface.FacedHeadings);
        Assert.InRange(faced, 39f, 41f);
        Assert.Contains("Turning to", controller.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void ApproachDoesNotReissueFaceHeadingInsideSevenTenthsOfASecond()
    {
        (FakeAutomation surface, CombatController controller) =
            ApproachRig(selfHeading: 0f, targetEastWest: 0.0839d, targetNorthSouth: 0.1d);

        // Two 293 ms passes fall inside the 0.7 s re-issue window.
        controller.TickMonsterApproach(0.293d, canAct: true);
        controller.TickMonsterApproach(0.293d, canAct: true);

        Assert.Single(surface.FacedHeadings);

        controller.TickMonsterApproach(0.293d, canAct: true);
        Assert.Single(surface.FacedHeadings);
        controller.TickMonsterApproach(0.293d, canAct: true);
        Assert.Equal(2, surface.FacedHeadings.Count);
    }

    [Fact]
    public void ApproachRunsForwardInsideTheFourDegreeBandWithoutFacingAgain()
    {
        // Target ~1.7° east of north.
        (FakeAutomation surface, CombatController controller) =
            ApproachRig(selfHeading: 0f, targetEastWest: 0.003d, targetNorthSouth: 0.1d);

        controller.TickMonsterApproach(0.05d, canAct: true);

        PluginMovementIntent intent = Assert.Single(surface.MovementIntents);
        Assert.True(intent.Forward);
        Assert.True(intent.Run);
        Assert.Empty(surface.FacedHeadings);
        Assert.Contains("Approaching", controller.Status, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData(0f)]
    [InlineData(40f)]
    [InlineData(-40f)]
    [InlineData(140f)]
    public void ApproachNeverIssuesHeldTurnKeyIntents(float selfHeading)
    {
        (FakeAutomation surface, CombatController controller) =
            ApproachRig(selfHeading, targetEastWest: 0d, targetNorthSouth: 0.1d);

        for (int pass = 0; pass < 6; pass++)
            controller.TickMonsterApproach(0.293d, canAct: true);

        Assert.DoesNotContain(
            surface.MovementIntents,
            static intent => intent.TurnLeft || intent.TurnRight);
    }


    [Fact]
    public void HigherPriorityRuleWinsEvenWhenTargetIsFarther()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets =
            [
                Target(10, "Drudge", distance: 2, angle: 0),
                Target(20, "Olthoi Soldier", distance: 12, angle: 30),
            ],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 20f,
            SelectionMethod = TargetSelectionMethod.Range,
        };
        settings.Rules.Insert(0, new MonsterRule("name#^Olthoi", 4));
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(20u, surface.LastBeginTarget);
        Assert.Contains("Olthoi Soldier", controller.TargetText, StringComparison.Ordinal);
    }

    [Fact]
    public void SamePriorityTargetsDoNotFlipFlopBetweenScans()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets =
            [
                Target(10, "Drudge", distance: 3, angle: 0),
                Target(20, "Drudge", distance: 9, angle: 0),
            ],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 20f,
            SelectionMethod = TargetSelectionMethod.Range,
            ScanIntervalSeconds = 0.05d,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(10u, surface.LastBeginTarget);

        surface.CombatSnapshot = Physical();
        surface.Targets =
        [
            Target(10, "Drudge", distance: 9, angle: 0),
            Target(20, "Drudge", distance: 2, angle: 0),
        ];
        controller.OnTick(0.25);

        Assert.Equal(10u, surface.LastBeginTarget);
        Assert.Contains("9.0m", controller.TargetText, StringComparison.Ordinal);
    }

    [Fact]
    public void TargetTextDistanceIsInvariantUnderASwedishCulture()
    {
        CultureInfo previous = CultureInfo.CurrentCulture;
        CultureInfo previousUi = CultureInfo.CurrentUICulture;
        try
        {
            CultureInfo.CurrentCulture = new CultureInfo("sv-SE");
            CultureInfo.CurrentUICulture = new CultureInfo("sv-SE");
            var surface = new FakeAutomation
            {
                CombatSnapshot = Physical(),
                Targets = [Target(10, "Drudge", distance: 9, angle: 0)],
                EquipmentItems = [WieldedPlannedWeapon()],
            };
            var settings = new CombatSettings
            {
                MaximumRange = 20f,
                SelectionMethod = TargetSelectionMethod.Range,
                ScanIntervalSeconds = 0.05d,
            };
            ProfileFixtureWeapon(settings);
            var controller = new CombatController(new FakeHost(surface), settings);

            controller.Toggle();
            controller.OnTick(0.25);

            Assert.Contains("9.0m", controller.TargetText, StringComparison.Ordinal);
            Assert.DoesNotContain("9,0m", controller.TargetText, StringComparison.Ordinal);
        }
        finally
        {
            CultureInfo.CurrentCulture = previous;
            CultureInfo.CurrentUICulture = previousUi;
        }
    }

    [Fact]
    public void StickyTargetStillYieldsToAHigherPriorityRule()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", distance: 3, angle: 0)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 20f,
            SelectionMethod = TargetSelectionMethod.Range,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Insert(0, new MonsterRule("name#^Olthoi", 4));
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(10u, surface.LastBeginTarget);

        surface.CombatSnapshot = Physical();
        surface.Targets =
        [
            Target(10, "Drudge", distance: 3, angle: 0),
            Target(20, "Olthoi Soldier", distance: 15, angle: 60),
        ];
        controller.OnTick(0.25);

        Assert.Equal(20u, surface.LastBeginTarget);
    }

    [Fact]
    public void TargetLockKeepsCurrentTargetAcrossRescan()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "First", 4, 20)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            TargetLock = true,
            ScanIntervalSeconds = 0.1,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.1);

        surface.CombatSnapshot = Physical();
        surface.Targets =
        [
            Target(10, "First", 4, 20),
            Target(11, "Closer", 1, 0),
        ];
        controller.OnTick(0.1);

        Assert.Equal(10u, surface.LastBeginTarget);
        Assert.Contains("First", controller.TargetText, StringComparison.Ordinal);
    }

    [Fact]
    public void PreviousValidTargetWinsAngleTieBreakWithoutTargetLock()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets =
            [
                Target(10, "First", 4, 1),
                Target(11, "Second", 4, 20),
            ],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            SelectionMethod = TargetSelectionMethod.Angle,
            TargetLock = false,
            ScanIntervalSeconds = 0.1,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.1);

        surface.CombatSnapshot = Physical();
        surface.Targets =
        [
            Target(10, "First", 4, 20),
            Target(11, "Second", 4, 1),
        ];
        controller.OnTick(0.1);

        Assert.Equal(10u, surface.LastBeginTarget);
        Assert.Contains("First", controller.TargetText, StringComparison.Ordinal);
    }

    [Fact]
    public void PreviousTargetDoesNotBeatNewHigherPriorityRule()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            SelectionMethod = TargetSelectionMethod.Range,
            ScanIntervalSeconds = 0.1,
        };
        settings.Rules.Insert(0, new MonsterRule("name#^Olthoi", 4));
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.1);

        surface.CombatSnapshot = Physical();
        surface.Targets =
        [
            Target(10, "Drudge", 2, 0),
            Target(20, "Olthoi Soldier", 4, 20),
        ];
        controller.OnTick(0.1);

        Assert.Equal(20u, surface.LastBeginTarget);
        Assert.Contains("Olthoi Soldier", controller.TargetText, StringComparison.Ordinal);
    }

    [Fact]
    public void BothSelectionUsesAngleForNearTargetsAndRangeWhenNoneAreNear()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets =
            [
                Target(10, "Near side", 3, 80),
                Target(11, "Near ahead", 8, 5),
                Target(12, "Far", 20, 0),
            ],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 20f,
            SelectionMethod = TargetSelectionMethod.Both,
            TargetSelectAngleRange = 10,
        };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(11u, surface.LastBeginTarget);
    }

    [Fact]
    public void PhysicalAttackWaitsForConfiguredPowerBeforeRelease()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings { AttackPower = 0.6f };
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(1, surface.BeginCount);

        surface.CombatSnapshot = Physical(
            request: true, build: true, bar: 0.59f) with
        {
            DesiredPower = 0.6f,
        };
        controller.OnTick(0.01);
        Assert.Equal(0, surface.ReleaseCount);

        surface.CombatSnapshot = Physical(
            request: true, build: true, bar: 0.60f) with
        {
            DesiredPower = 0.6f,
        };
        controller.OnTick(0.01);
        Assert.Equal(1, surface.ReleaseCount);
    }

    [Fact]
    public void CombatController_SummonsConfiguredPetBeforeStartingAttack()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            ItemEntries = [PetDevice(88, 49387)],
        };
        var settings = new CombatSettings { SummonPets = true };
        settings.CombatItemObjectIds.Add(88u);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(88u, surface.LastUsedItem);
        Assert.Equal(0, surface.BeginCount);
        Assert.Contains("Summoning", controller.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void MagicModeCastsBestProjectedAttackOnExplicitTarget()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells = [Spell(100, "Incantation of Flame Bolt")],
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface), FireAttackRule(new CombatSettings()));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
        Assert.Contains("Flame Bolt", controller.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void BreakableTurnFacesTargetBeforeDispatchingTargetedSpell()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    RequiresTurnTo = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
            NavigationSnapshot = NavigationAt(heading: 0f),
        };
        surface.NavigationObjects[10u] = new PluginNavigationObject(
            10u,
            "Drudge",
            new PluginNavigationPosition(
                0x7F7F0001u,
                0.1d,
                0d,
                0d,
                0f,
                true));
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { UseBreakableTurnTo = true }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Empty(surface.MovementIntents);
        Assert.Equal(90f, Assert.Single(surface.FacedHeadings));
        Assert.Empty(surface.CastSpellIds);

        surface.NavigationSnapshot = NavigationAt(heading: 90f);
        controller.OnTick(0.25);

        Assert.Equal([100u], surface.CastSpellIds);
        Assert.Equal(2, surface.ClearMovementCount);
    }

    [Fact]
    public void BreakableTurnDoesNotReissueFaceHeadingInsideSevenTenthsOfASecond()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    RequiresTurnTo = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
            NavigationSnapshot = NavigationAt(heading: 0f),
        };
        surface.NavigationObjects[10u] = new PluginNavigationObject(
            10u,
            "Drudge",
            new PluginNavigationPosition(
                0x7F7F0001u,
                0.1d,
                0d,
                0d,
                0f,
                true));
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { UseBreakableTurnTo = true }));

        controller.Toggle();
        controller.OnTick(0.293);
        controller.OnTick(0.293);

        Assert.Equal(90f, Assert.Single(surface.FacedHeadings));
        Assert.Empty(surface.CastSpellIds);

        controller.OnTick(0.293);
        Assert.Single(surface.FacedHeadings);
        controller.OnTick(0.293);
        Assert.Equal(2, surface.FacedHeadings.Count);
    }

    [Fact]
    public void ProjectileAwarenessBlocksSpellBeforeCastDispatch()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            ProjectilePath = new(
                PluginProjectilePathStatus.Blocked,
                CollisionChecks: 3,
                BlockingObjectId: 0x50000001u),
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { UseProjectileAwareness = true }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Empty(surface.CastSpellIds);
        Assert.Equal(10u, surface.LastProjectileTarget);
        // Nothing can be delivered to it, so it is out of the running for this
        // pass and there is nothing else to pick.
        Assert.False(controller.HasTarget);
    }

    [Fact]
    public void ProjectileAwarenessCanBeExplicitlyDisabled()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            ProjectilePath = new(PluginProjectilePathStatus.Blocked),
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { UseProjectileAwareness = false }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Contains(100u, surface.CastSpellIds);
        Assert.Equal(0u, surface.LastProjectileTarget);
    }

    [Fact]
    public void CollisionDebugPublishesDiagnosticSamplesToTheGraphicalHost()
    {
        PluginProjectileDebugSample[] samples =
        [new(new System.Numerics.Vector3(1f, 2f, 3f), false, 0.4f)];
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            ProjectilePath = new(
                PluginProjectilePathStatus.Blocked,
                CollisionChecks: 1)
            {
                DebugSamples = samples,
            },
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings
            {
                UseProjectileAwareness = true,
                ShowCollisionDebug = true,
            }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(samples, surface.ShownProjectileDebugSamples);
    }

    [Fact]
    public void DoJiggleUsesRetailSelectionCycleInsteadOfMovingTheCharacter()
    {
        PluginSpellInfo attack = Spell(100, "Incantation of Flame Bolt") with
        {
            IsProjectile = false,
            School = 34,
            Difficulty = 300,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells = [attack],
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            new CombatSettings
            {
                UseProjectileAwareness = false,
                DoJiggle = true,
            });
        controller.Toggle();
        controller.OnTick(0.25);
        surface.LastCastCompletion = new PluginCastCompletion(1, 100, 10, 0);

        controller.OnTick(0.01);
        controller.OnTick(0.131);

        Assert.Equal(
            [
                PluginSelectionAction.PreviousSelection,
                PluginSelectionAction.NextPlayer,
                PluginSelectionAction.PreviousPlayer,
            ],
            surface.SelectionActions);
        Assert.Empty(surface.MovementIntents);
    }

    /// <summary>
    /// Mutation: drop the nudge window's deadline and this fails — the macro
    /// would go on cycling its selection for ever between casts instead of
    /// for one short window after each one.
    /// </summary>
    [Fact]
    public void TheNudgeStopsAfterItsOwnWindow()
    {
        PluginSpellInfo attack = Spell(100, "Incantation of Flame Bolt") with
        {
            IsProjectile = false,
            School = 34,
            Difficulty = 300,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells = [attack],
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            new CombatSettings
            {
                UseProjectileAwareness = false,
                DoJiggle = true,
            });
        controller.Toggle();
        controller.OnTick(0.25);
        surface.LastCastCompletion = new PluginCastCompletion(1, 100, 10, 0);
        controller.OnTick(0.01);

        for (int tick = 0; tick < 10; tick++)
            controller.OnTick(0.131);
        int afterTheWindow = surface.SelectionActions.Count;

        for (int tick = 0; tick < 20; tick++)
            controller.OnTick(0.131);

        Assert.Equal(afterTheWindow, surface.SelectionActions.Count);
        // One opening pulse plus the seven 0.131 s beats inside 0.907 s.
        Assert.InRange(afterTheWindow, 2, 9);
    }

    [Fact]
    public void MagicRuleDebuffsAndWaitsForServerReceiptBeforeAttack()
    {
        PluginSpellInfo imperil = Spell(90, "Imperil Other VII") with
        {
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [imperil],
            KnownAttackSpells = [Spell(100, "Incantation of Flame Bolt")],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Imperil | MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((90u, 10u), surface.LastTargetedCast);
        Assert.DoesNotContain(100u, surface.CastSpellIds);

        surface.LastCastCompletion = new PluginCastCompletion(1, 90, 10, 0);
        controller.OnTick(0.25);
        Assert.Contains(100u, surface.CastSpellIds);
    }

    [Fact]
    public void RingArmAlsoRequiresNoStreakColumnAndANonZeroTally()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(110, "Cassius' Ring of Fire", difficulty: 300) with
            {
                TargetMask = 0u,
                IsUntargeted = true,
            },
            MagicSpell(102, "Flame Streak VII", difficulty: 350),
            MagicSpell(100, "Flame Bolt VII", difficulty: 300),
        ];

        (uint untargeted, (uint, uint) targeted) = CastRingScenario(
            known,
            MonsterActionFlags.Ring | MonsterActionFlags.Streak,
            distance: 3f,
            ringDistance: 5d);
        Assert.Equal(0u, untargeted);
        Assert.Equal(102u, targeted.Item1);

        // Ring only, but the monster is beyond RingDistance so the tally is
        // zero (dz.cs:736-739): the ring arm fails and the pass bolts.
        (untargeted, targeted) = CastRingScenario(
            known,
            MonsterActionFlags.Ring,
            distance: 12f,
            ringDistance: 5d);
        Assert.Equal(0u, untargeted);
        Assert.Equal(100u, targeted.Item1);
    }

    /// <summary>
    /// The ring arm asks one question — are the components for the family's
    /// first rung in the pack — and commits. It does not also ask whether the
    /// client would start the cast this instant; when it would not, the ring
    /// is refused where every other refusal is reported, rather than quietly
    /// becoming a bolt at a monster the profile wanted ringed.
    /// Mutation: put the cast-gate test back in front of the ring choice and
    /// the bolt goes out instead.
    /// </summary>
    [Fact]
    public void TheRingArmAsksForComponentsAndNothingElse()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(110, "Cassius' Ring of Fire", difficulty: 300) with
            {
                TargetMask = 0u,
                IsUntargeted = true,
            },
            MagicSpell(100, "Flame Bolt VII", difficulty: 300),
        ];
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 3f, 0)],
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        // The client is not ready to start the ring this instant.
        surface.CastGates[110u] = PluginCastGate.Refused;
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            RingDistance = 5d,
            MinimumRingTargets = 1,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Ring,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(0u, surface.LastUntargetedCast);
        Assert.Equal((0u, 0u), surface.LastTargetedCast);
        Assert.Contains(
            "Cassius' Ring of Fire",
            controller.Status,
            StringComparison.Ordinal);
    }

    [Fact]
    public void OnlyAMonsterThePassFollowsAndHasNotGivenUpOnIsPointable()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        var controller = new CombatController(new FakeHost(surface), settings);

        // Nothing has been scanned yet, so nothing is pointable.
        Assert.False(controller.IsTrackedAndNotBlacklisted(10u));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.True(controller.IsTrackedAndNotBlacklisted(10u));
        Assert.False(controller.IsTrackedAndNotBlacklisted(11u));
        Assert.False(controller.IsTrackedAndNotBlacklisted(0u));
    }

    [Fact]
    public void WithNoEquipmentProjectionTheAttackStillWaitsForCombatMode()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Peace },
            EquipmentAvailable = false,
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", 300)],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);

        // Nothing is cast out of peace mode, and the mode is asked for.
        Assert.Empty(surface.CastSpellIds);
        Assert.Contains("EnterMode:Magic", surface.CallLog);
    }

    [Fact]
    public void TheDrainArmCastsWhatThePlannerChose()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets =
            [
                new PluginCombatTarget(
                    10u, "Olthoi Slasher", 1010u, 5f, 0f, true, 0.05f)
                {
                    HealthRevision = 4,
                },
            ],
            KnownCombatSpells =
            [
                MagicSpell(1237, "Drain Health Other I", difficulty: 100),
                MagicSpell(1238, "Drain Health Other II", difficulty: 150),
                MagicSpell(1239, "Drain Health Other III", difficulty: 200),
                MagicSpell(2760, "Martyr's Hecatomb I", difficulty: 100),
                MagicSpell(2761, "Martyr's Hecatomb II", difficulty: 150),
                MagicSpell(2762, "Martyr's Hecatomb III", difficulty: 200),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            MonsterFacts = new MonsterFactTable(GameInfo),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.DrainAuto,
            }));
        var controller = new CombatController(
            new FakeHost(surface),
            settings,
            vitalSettings: null,
            gameInfo: GameInfo);
        controller.Toggle();
        controller.OnTick(0.25);

        // The database lists this monster as unaffectable by magic, so no
        // drain may be planned at all; among the martyrs the plan takes the
        // best monster health taken off per millisecond spent.
        Assert.Equal((2761u, 10u), surface.LastTargetedCast);
    }

    [Fact]
    public void CastsNoOneEverAnswersGetTheMonsterDeleted()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Olthoi Slasher", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            GhostMonsterSpellAttemptCount = 3,
            DeleteGhostMonsters = true,
            MonsterFacts = new MonsterFactTable(GameInfo),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();

        // One pass issues the cast; the rule track is then held while it is in
        // flight, which is what the whole re-issue mechanism exists for.
        controller.OnTick(0.25);
        Assert.Contains(100u, surface.CastSpellIds);
        Assert.Equal(
            SpellCastTrackerState.AwaitingLaunch,
            controller.CastTracker.State);

        // The server never acknowledges it: the tracker re-sends every 200 ms,
        // and each re-send is one more unanswered attempt.
        for (int tick = 0; tick < 10; tick++)
            controller.CastTracker.Advance(0.1);

        Assert.Equal(10u, Assert.Single(surface.DismissedGhosts));
        Assert.Contains(
            surface.PostedSystemMessages,
            line => line.Contains("Deleting ghost monster Olthoi Slasher"));

        // Deleting it is the whole consequence. There is no permanent verdict
        // on the monster, so the very next pass picks it again.
        surface.CastSpellIds.Clear();
        controller.OnTick(0.25);
        Assert.Contains(100u, surface.CastSpellIds);
    }

    [Fact]
    public void OnlyAMonsterWithAKnownHealthCeilingCanGoStaleIntoAGhost()
    {
        FakeAutomation listed = StalledHealthScenario("Olthoi Slasher");
        Assert.NotEmpty(listed.DismissedGhosts);
        Assert.All(listed.DismissedGhosts, id => Assert.Equal(10u, id));
        Assert.Contains(
            listed.PostedSystemMessages,
            line => line.Contains("due to HP tracker notification"));

        // The database does not list "Drudge", so the client is never told
        // this monster's health in points and its silence means nothing.
        Assert.Empty(StalledHealthScenario("Drudge").DismissedGhosts);
    }

    private static FakeAutomation StalledHealthScenario(string name)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, name, 5, 0) with { HealthRevision = 3 }],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            DeleteGhostMonstersByHealthTracker = true,
            GhostDeleteHealthTrackerSeconds = 10d,
            MonsterFacts = new MonsterFactTable(GameInfo),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        for (int tick = 0; tick < 30; tick++)
            controller.OnTick(1d);
        return surface;
    }

    [Fact]
    public void TheRingTallyCountsOnlyValidCandidatesStrictlyInsideTheRing()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(110, "Cassius' Ring of Fire", difficulty: 300) with
            {
                TargetMask = 0u,
                IsUntargeted = true,
            },
            MagicSpell(100, "Flame Bolt VII", difficulty: 300),
        ];

        // Two monsters, both within RingDistance by the old inclusive test:
        // one exactly ON the ring boundary, which the strict comparison
        // excludes, so the tally is one and the pass bolts instead.
        Assert.Equal(
            100u,
            RingTallyScenario(
                known,
                [Target(10, "Drudge", 3f, 0), Target(11, "Drudge", 5f, 0)],
                minimumRange: 0d).Targeted.Item1);

        // Same, but the second monster is nearer than AttackMinimumDistance,
        // so it is not a candidate at all and cannot be tallied.
        Assert.Equal(
            100u,
            RingTallyScenario(
                known,
                [Target(10, "Drudge", 3f, 0), Target(11, "Drudge", 0.5f, 0)],
                minimumRange: 1d).Targeted.Item1);

        // Two valid candidates strictly inside the ring: the ring fires.
        Assert.Equal(
            110u,
            RingTallyScenario(
                known,
                [Target(10, "Drudge", 3f, 0), Target(11, "Drudge", 4f, 0)],
                minimumRange: 0d).Untargeted);
    }

    private static (uint Untargeted, (uint, uint) Targeted) RingTallyScenario(
        IReadOnlyList<PluginSpellInfo> known,
        IReadOnlyList<PluginCombatTarget> targets,
        double minimumRange)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = targets,
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            RingDistance = 5d,
            MinimumRange = minimumRange,
            MinimumRingTargets = 2,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Ring | MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return (surface.LastUntargetedCast, surface.LastTargetedCast);
    }

    private static (uint Untargeted, (uint, uint) Targeted) CastRingScenario(
        IReadOnlyList<PluginSpellInfo> known,
        MonsterActionFlags flags,
        float distance,
        double ringDistance)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", distance, 0)],
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            RingDistance = ringDistance,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = flags,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return (surface.LastUntargetedCast, surface.LastTargetedCast);
    }

    [Fact]
    public void StrongerBoltBeatsTheArcEvenWithUseArcsYes()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 20, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
                MagicSpell(101, "Flame Arc IV", difficulty: 150),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            UseArcs = UseArcsMode.Yes,
            ArcRange = 1d,
            MaximumRange = 40d,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// A profile saved before the arc default was corrected has no arc key at
    /// all, and loading one must not put arcing back: a fresh character bolts
    /// at five metres where an arcing one would throw over the monster's head.
    /// There are two defaults on this road, and each load below takes one of
    /// them: a stored combat section with no arc key reads the sidecar
    /// record's, and a stored profile with no combat section at all reads the
    /// live settings object's.
    /// Mutation: set the sidecar record's default back to "at range" and the
    /// first pair of assertions fails; set the settings object's own back and
    /// the last one does.
    /// </summary>
    [Fact]
    public void AProfileWithNoArcSettingLoadsWithArcsOff()
    {
        var storage = new MemoryStorage();
        storage.WriteText(
            "profile.json",
            """{ "combat": { "maximumRange": 5.0 } }""");
        var store = new MossTankProfileStore(
            new StorageHost(storage, "Acdream", "Fixture"));
        store.BindCharacter("Acdream");
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };

        store.LoadCurrent(
            settings,
            new HashSet<string>(StringComparer.Ordinal),
            new HashSet<string>(StringComparer.OrdinalIgnoreCase));

        Assert.Equal(UseArcsMode.No, settings.Combat.UseArcs);
        Assert.Equal(5d, settings.Combat.ArcRange, precision: 6);

        // The other road: nothing combat-shaped is stored at all, so the load
        // falls back on the settings object's own declared default.
        var bareStorage = new MemoryStorage();
        bareStorage.WriteText("profile.json", "{ }");
        var bareStore = new MossTankProfileStore(
            new StorageHost(bareStorage, "Acdream", "Fixture"));
        bareStore.BindCharacter("Acdream");
        var bare = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings { UseArcs = UseArcsMode.Yes },
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };

        bareStore.LoadCurrent(
            bare,
            new HashSet<string>(StringComparer.Ordinal),
            new HashSet<string>(StringComparer.OrdinalIgnoreCase));

        Assert.Equal(UseArcsMode.No, bare.Combat.UseArcs);
    }

    [Fact]
    public void UseArcsDecidesOnlyAnExactQualityTie()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(100, "Flame Bolt VII", difficulty: 300),
            MagicSpell(101, "Flame Arc VII", difficulty: 300),
        ];

        Assert.Equal((101u, 10u), CastWithUseArcs(known, UseArcsMode.Yes, distance: 20));
        Assert.Equal((100u, 10u), CastWithUseArcs(known, UseArcsMode.No, distance: 20));
        // "At range" arcs only from the arc range outwards; inside it the
        // bolt still wins.
        Assert.Equal(
            (101u, 10u),
            CastWithUseArcs(known, UseArcsMode.AtRange, distance: 20, arcRange: 10));
        Assert.Equal(
            (100u, 10u),
            CastWithUseArcs(known, UseArcsMode.AtRange, distance: 5, arcRange: 10));
    }

    private static (uint, uint) CastWithUseArcs(
        IReadOnlyList<PluginSpellInfo> known,
        UseArcsMode mode,
        double distance,
        double arcRange = 1d)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", (float)distance, 0)],
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings
            {
                UseArcs = mode,
                ArcRange = arcRange,
                MaximumRange = 40d,
            }));
        controller.Toggle();
        controller.OnTick(0.25);
        return surface.LastTargetedCast;
    }

    /// <summary>
    /// Mutation: drop the streak's flight test and the first assertion fails
    /// — the streak is cast straight into the wall, every pass, for ever.
    /// </summary>
    [Fact]
    public void AStreakIntoCoverIsRefusedAndTakesItsColumnWithIt()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(102, "Flame Streak VII", difficulty: 350),
            ],
            ProjectilePath = new(
                PluginProjectilePathStatus.Blocked,
                CollisionChecks: 3,
                BlockingObjectId: 0x50000001u),
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Streak,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Empty(surface.CastSpellIds);
        // The monster stops being a candidate once its only column is off, so
        // the pass stops rather than spinning against it.
        Assert.False(controller.HasTarget);
        Assert.InRange(surface.ProjectilePathChecks, 1, 4);
    }

    /// <summary>
    /// Mutation: pass <c>settings.AttackHeight</c> for either shape and this
    /// fails — the ray would start at the configured melee height instead of
    /// the height the shape itself flies at.
    /// </summary>
    [Fact]
    public void BoltAndArcClearanceUseTheShapesOwnHeight()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(100, "Flame Bolt VII", difficulty: 300) with
            {
                IsProjectile = true,
            },
            MagicSpell(101, "Flame Arc VII", difficulty: 300) with
            {
                IsProjectile = true,
            },
        ];

        Assert.Equal(
            PluginAttackHeight.Medium,
            ClearanceHeightFor(known, UseArcsMode.No));
        Assert.Equal(
            PluginAttackHeight.High,
            ClearanceHeightFor(known, UseArcsMode.Yes));
    }

    /// <summary>
    /// A debuff's way to the monster is tested at the height its own flight
    /// takes, exactly as an attack's is: a thrown phial is tested at the
    /// profile's swing height, not at a fixed level one.
    /// Mutation: hard-code <c>PluginAttackHeight.Medium</c> in the debuff
    /// clearance check again and this fails.
    /// </summary>
    [Fact]
    public void ADebuffsClearanceUsesItsOwnFlightHeight()
    {
        PluginSpellInfo imperil = Spell(1323, "Imperil Other I") with
        {
            School = 31,
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
        };
        PluginInventoryItem phial = InventoryItem(
            200, "Iron Phial of Imperil", 0x100, 0, equipped: false)
            with { CombatUse = 0 };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Missile },
            Targets = [Target(10, "Drudge", 5, 0)],
            SpellLookup = [imperil],
            ItemEntries = [phial],
            CharacterSkills = [new(38u, "Alchemy", PluginSkillTraining.Trained, 400)],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.ConsumableNames.Add("Iron Phial of Imperil");
        settings.UseProjectileAwareness = true;
        // Deliberately neither of the two shape heights.
        settings.AttackHeight = PluginAttackHeight.Low;
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(
            PluginProjectilePathKind.Missile,
            surface.LastProjectileKind);
        Assert.Equal(PluginAttackHeight.Low, surface.LastProjectileHeight);
    }

    private static PluginAttackHeight ClearanceHeightFor(
        IReadOnlyList<PluginSpellInfo> known,
        UseArcsMode mode)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
            UseArcs = mode,
            // Deliberately neither of the two shape heights.
            AttackHeight = PluginAttackHeight.Low,
        });
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return surface.LastProjectileHeight;
    }

    [Fact]
    public void AttackPlusStreakUsesTheStreakOnlyAsAFinishingBlow()
    {
        PluginSpellInfo[] known =
        [
            MagicSpell(100, "Flame Bolt VII", difficulty: 300),
            MagicSpell(102, "Flame Streak VII", difficulty: 350),
        ];

        // "Olthoi Slasher" is listed in the fixture database at 3190 health,
        // and the streak's difficulty of 350 sets the bar at 50 points until a
        // real blow is seen: 2871 left bolts, 32 left finishes.
        Assert.Equal(100u, CastAgainstHealth(known, healthFraction: 0.9f).Item1);
        Assert.Equal(102u, CastAgainstHealth(known, healthFraction: 0.01f).Item1);

        // A monster the database does not list has no health in points at
        // all, so the finishing move can never be chosen for it.
        Assert.Equal(
            100u,
            CastAgainstHealth(known, healthFraction: 0.01f, name: "Drudge").Item1);
    }

    private static (uint, uint) CastAgainstHealth(
        IReadOnlyList<PluginSpellInfo> known,
        float healthFraction,
        string name = "Olthoi Slasher")
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets =
            [
                new PluginCombatTarget(
                    10u, name, 1010u, 5f, 0f, true, healthFraction)
                {
                    HealthRevision = 7,
                },
            ],
            KnownCombatSpells = known,
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            MonsterFacts = new MonsterFactTable(GameInfo),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack | MonsterActionFlags.Streak,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return surface.LastTargetedCast;
    }

    private static PluginSpellInfo MagicSpell(
        uint id,
        string name,
        int difficulty) => new(
            id,
            name,
            Family: id,
            Tier: 7,
            Difficulty: difficulty,
            ManaCost: 30,
            DurationSeconds: 0,
            School: 34,
            Description: string.Empty,
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            IsOffensive = true,
            TargetMask = 0x10,
        };

    [Fact]
    public void DebuffsGoOutInRetailsTwelveStepOrder()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                Debuff(80, "Fester Other VII"),
                Debuff(81, "Broadside of a Barn"),
                Debuff(82, "Fire Vulnerability Other VII"),
                Debuff(83, "Imperil Other VII"),
                Debuff(84, "Magic Yield Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack
                    | MonsterActionFlags.Yield
                    | MonsterActionFlags.Imperil
                    | MonsterActionFlags.Vulnerability
                    | MonsterActionFlags.Broadside
                    | MonsterActionFlags.Fester,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();

        long revision = 0;
        foreach (uint expected in new uint[] { 84u, 83u, 82u, 81u, 80u })
        {
            controller.OnTick(0.25);
            Assert.Equal((expected, 10u), surface.LastTargetedCast);
            surface.LastCastCompletion = new PluginCastCompletion(
                ++revision, expected, 10u, 0);
            controller.OnTick(0.25);
        }
    }

    [Fact]
    public void OnlyOneDebuffKindIsDispatchedPerPass()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                Debuff(83, "Imperil Other VII"),
                Debuff(84, "Magic Yield Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack
                    | MonsterActionFlags.Yield
                    | MonsterActionFlags.Imperil,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal([84u], surface.CastSpellIds);
    }

    [Fact]
    public void BlockedDebuffPathDropsThatColumnAndTheChainMovesOn()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                // The first step of the debuff chain, and the only spell here
                // whose family declares a flight (117 is a bolt family).
                Debuff(84, "Magic Yield Other VII") with { Family = 117u },
                // Step 7.
                Debuff(83, "Imperil Other VII"),
            ],
            ProjectilePath = new(
                PluginProjectilePathStatus.Blocked,
                CollisionChecks: 3,
                BlockingObjectId: 0x50000001u),
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
        };
        Assert.False(settings.AllowDebuffFallback);
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack
                    | MonsterActionFlags.Yield
                    | MonsterActionFlags.Imperil,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((83u, 10u), surface.LastTargetedCast);
        Assert.DoesNotContain(84u, surface.CastSpellIds);
    }

    /// <summary>
    /// Mutation: put the bare mode change back in place of the wield gate and
    /// this fails — the debuff goes out with the sword still in hand, so the
    /// wand's own spellcraft and mana never pay for it.
    /// </summary>
    [Fact]
    public void ALearnedDebuffWieldsAWandBeforeItIsCast()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Melee },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [Debuff(83, "Imperil Other VII")],
            EquipmentItems =
            [
                WieldedPlannedWeapon(),
                Equipment(
                    990u,
                    "Fixture Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.MaximumRange = 40d;
        settings.CombatItemNames.Add("Fixture Wand");
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        // Peace first, then the wand: nothing is cast while the sword is in
        // hand.
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(990u, surface.LastEquipObjectId);
        Assert.Empty(surface.CastSpellIds);

        // The server confirms the swap: the sword is away, the wand is in.
        surface.EquipmentItems =
        [
            Equipment(
                990u,
                "Fixture Wand",
                damageType: 0,
                itemType: CombatModeGate.CasterItemType,
                equippedLocation: 0x00100000u),
        ];
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal((83u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// Mutation: drop the <c>SwitchWandsToDebuff</c> branch and this fails —
    /// with the setting off the debuff always reaches for the first profiled
    /// wand, so the two arms must pick different wands here.
    /// </summary>
    [Fact]
    public void SwitchWandsToDebuffDebuffsWithTheCasterAttackWeapon()
    {
        Assert.Equal(991u, DebuffWandFor(switchWandsToDebuff: true));
        Assert.Equal(990u, DebuffWandFor(switchWandsToDebuff: false));
    }

    private static uint DebuffWandFor(bool switchWandsToDebuff)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [Debuff(83, "Imperil Other VII")],
            EquipmentItems =
            [
                Equipment(
                    991u,
                    "Attack Wand",
                    damageType: 0x0010,
                    itemType: CombatModeGate.CasterItemType),
                Equipment(
                    990u,
                    "Spare Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            SwitchWandsToDebuff = switchWandsToDebuff,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Imperil,
                WeaponName = "Attack Wand",
            }));
        settings.CombatItemNames.Add("Attack Wand");
        settings.CombatItemNames.Add("Spare Wand");
        settings.CombatItemOrder.Add("Spare Wand");
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return surface.LastEquipObjectId;
    }

    /// <summary>
    /// Mutation: drop the range term from the debuff source walk and the
    /// first half fails — an out-of-reach spell would be chosen and refused.
    /// </summary>
    [Fact]
    public void ADebuffSpellOutOfReachIsNotChosen()
    {
        Assert.Equal((0u, 0u), DebuffAtDistance(reach: 4f, distance: 6f));
        Assert.Equal((83u, 10u), DebuffAtDistance(reach: 20f, distance: 6f));
    }

    private static (uint, uint) DebuffAtDistance(float reach, float distance)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", distance, 0)],
            KnownCombatSpells =
            [
                Debuff(83, "Imperil Other VII") with
                {
                    BaseRangeConstant = reach,
                },
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.MaximumRange = 40d;
        settings.SpellRangeFudge = 0d;
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);
        return surface.LastTargetedCast;
    }

    /// <summary>
    /// Mutation: restore the walk over every source and this fails — the item
    /// would be used after the out-of-reach spell dropped out AND after the
    /// winner failed, instead of exactly one source being chosen per decision.
    /// </summary>
    [Fact]
    public void AnOutOfReachSpellLeavesTheItemAsTheDebuffSource()
    {
        PluginSpellInfo imperil = Spell(90, "Imperil Other VII") with
        {
            School = 31,
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
            BaseRangeConstant = 4f,
        };
        PluginInventoryItem lens = InventoryItem(
            800, "Imperil Lens", 0x8000, 90, equipped: true) with
        {
            ItemSpellcraft = 400,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 6, 0)],
            KnownCombatSpells = [imperil],
            SpellLookup = [imperil],
            ItemEntries = [lens],
            // The learned spell would out-rank the lens on skill; only its
            // reach keeps it out of the choice.
            CharacterSkills = [new(31u, "Creature", PluginSkillTraining.Trained, 500)],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.MaximumRange = 40d;
        settings.SpellRangeFudge = 0d;
        settings.CombatItemObjectIds.Add(lens.ObjectId);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((800u, 10u), surface.LastAppliedItem);
        Assert.Empty(surface.CastSpellIds);
    }

    [Fact]
    public void BlockedDebuffPathStillLetsTheAttackGoOutOnStockSettings()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                Debuff(84, "Magic Yield Other VII") with { Family = 117u },
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
            ],
            ProjectilePath = new(PluginProjectilePathStatus.Blocked),
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack | MonsterActionFlags.Yield,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
    }

    [Fact]
    public void AnUnknownVulnScoresNoUrgencyForTheTargetItCannotDebuff()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets =
            [
                Target(10, "Drudge", distance: 9, angle: 0),
                Target(20, "Rat", distance: 2, angle: 0),
            ],
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
                MagicSpell(101, "Acid Stream VII", difficulty: 300),
                // The Fire Vuln line is deliberately absent.
                Debuff(85, "Acid Vulnerability Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 40d,
            SelectionMethod = TargetSelectionMethod.Range,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "name#^Drudge",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        settings.Rules.Add(new MonsterRule(
            "name#^Rat",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Acid,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((101u, 20u), surface.LastTargetedCast);
    }

    [Fact]
    public void ThePlannedWeaponsOwnElementPreemptsTheDamageTable()
    {
        PluginEquipmentItem fireWand = Equipment(
            990u,
            "Flame Wand",
            damageType: 0x0010,
            itemType: CombatModeGate.CasterItemType,
            equippedLocation: 0x00100000u);
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Magma Golem", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
                MagicSpell(102, "Frost Bolt VII", difficulty: 300),
            ],
            EquipmentItems = [fireWand],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Auto,
                WeaponName = "Flame Wand",
            }));
        var controller = new CombatController(
            new FakeHost(surface),
            settings,
            vitalSettings: null,
            GameInfo);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// Mutation: read only the weapon's plain damage again and the first half
    /// fails; apply the caster training cascade whatever the weapon is and the
    /// second fails — a martyr mage's melee build would be handed a drain.
    /// </summary>
    [Fact]
    public void AnImbuedMeleeWeaponKeepsItsOwnElementForALifeOnlyCaster()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                Debuff(70, "Fire Vulnerability Other VII"),
                Debuff(71, "Cold Vulnerability Other VII"),
            ],
            EquipmentItems =
            [
                Equipment(
                    990u,
                    "Spare Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
                Equipment(
                    991u,
                    "Imbued Sword",
                    // Plainly a slashing sword; its imbue rends fire.
                    damageType: 0x0001,
                    equippedLocation: 0x00100000u) with
                {
                    ImbuedEffect = 0x0200,
                },
            ],
            // Life magic only: no war, no void.
            CharacterSkills =
                [new(33u, "Life Magic", PluginSkillTraining.Trained, 300)],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.CombatItemNames.Add("Spare Wand");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Vulnerability,
                DamageType = MonsterDamageType.Auto,
                ExtraVulnerability = MonsterDamageType.None,
                WeaponName = "Imbued Sword",
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 5; tick++)
            controller.OnTick(0.25);

        Assert.Equal((70u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// Mutation: make "can this element be delivered" mean "do I know a spell
    /// or own a weapon of it" again and this fails — that question can always
    /// be answered yes by some spell, so the empty-quiver warning would never
    /// be reached. Only a launcher can fail to deliver, and only for want of
    /// ammunition.
    /// </summary>
    [Fact]
    public void ABowWithAnEmptyPackCanDeliverNoElementAtAll()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Magma Golem", 5, 0)],
            EquipmentItems =
            [
                Equipment(
                    900u,
                    "Yumi",
                    damageType: 0,
                    itemType: 0x100,
                    equippedLocation: 0x00100000u,
                    ammoType: 0x001u),
            ],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.CombatItemNames.Add("Yumi");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Auto,
            }));
        var controller = new CombatController(
            new FakeHost(surface),
            settings,
            vitalSettings: null,
            GameInfo);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Contains(
            surface.PostedSystemMessages,
            message => message.Contains(
                "no ammunition available!!!",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation: skip a preference the character has no spell for and this
    /// fails — a wand can always deliver, so the monster's FIRST listed
    /// weakness is the one that is used even when nothing is known for it.
    /// </summary>
    [Fact]
    public void TheLoadedGameInfoDatabaseDrivesTheAutoAttackElement()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Magma Golem", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(103, "Force Bolt VII", difficulty: 300),
                MagicSpell(105, "Shock Wave VII", difficulty: 300),
                // The Magma Golem's first listed weakness is cold.
                MagicSpell(104, "Frost Bolt VII", difficulty: 300),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Auto,
            }));
        var controller = new CombatController(
            new FakeHost(surface),
            settings,
            vitalSettings: null,
            GameInfo);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((104u, 10u), surface.LastTargetedCast);
    }

    [Fact]
    public void AFistsRowCastsTuskerFistsBeforeTheEnchantmentIsUp()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(0x0B76u, "Tusker Fists", difficulty: 300),
                MagicSpell(105, "Shock Wave VII", difficulty: 300),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fists,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((0x0B76u, 10u), surface.LastTargetedCast);
    }

    [Fact]
    public void AFistsRowWithoutTuskerFistsAttacksWithBludgeon()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(105, "Shock Wave VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fists,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((105u, 10u), surface.LastTargetedCast);
    }

    private static PluginSpellInfo Debuff(uint id, string name) => new(
        id,
        name,
        Family: id,
        Tier: 7,
        Difficulty: 250,
        ManaCost: 30,
        DurationSeconds: 60,
        School: 31,
        Description: string.Empty,
        IsSelfTargeted: false,
        IsBeneficial: false)
    {
        IsDebuff = true,
        IsOffensive = true,
        TargetMask = 0x10,
        // Real spells carry a reach; a fixture with none would be refused by
        // the debuff range gate before anything else could be observed.
        BaseRangeConstant = 80f,
    };

    [Fact]
    public void UnknownMonsterFallsToRetailsUnlistedElementAndCastsNoExtraVuln()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "New Server Creature", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Force Bolt VII", difficulty: 300),
                Debuff(85, "Piercing Vulnerability Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Auto,
                // Ex. Vuln = Auto resolves through the damage table, which has
                // nothing for this monster, so step 9 never runs.
                ExtraVulnerability = MonsterDamageType.Auto,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        // Pierce is eDamageElement 0, the first entry of ga.cs:772-774's walk.
        Assert.Equal((100u, 10u), surface.LastTargetedCast);
        Assert.DoesNotContain(85u, surface.CastSpellIds);
        Assert.Contains(
            surface.PostedSystemMessages,
            message => message.Contains(
                "Using unlisted damage type: Pierce",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// Every rule shares one cast tracker, so a buff cast at the character's
    /// own guid reaches the combat controller's result-timeout arm like any
    /// other. Giving a target up is a verdict about a creature the pass
    /// follows: the caster is not one, so nothing is suppressed and nothing
    /// is announced. A monster beside it still gets both.
    /// Mutation: drop the creature guard from
    /// <c>CombatFailureTracker.RecordMiss</c> and the first assertion fails
    /// with "Blacklisting unhittable target ??? (1342177290) for 120
    /// seconds." — the exact line the live run printed.
    /// </summary>
    [Fact]
    public void ASelfCastThatTimesOutIsNotAnnouncedAsAnUnhittableTarget()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
            BlacklistMonsterAttemptCount = 0,
            BlacklistMonsterTimeoutSeconds = 120,
        });
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.Toggle();
        controller.OnTick(0.25);

        const uint self = 1342177290u;
        TimeOutACast(controller, self, 0x1131u, "Incantation of Flame Bane", "MalarQuaTak");
        Assert.DoesNotContain(
            surface.PostedSystemMessages,
            message => message.Contains(
                "Blacklisting unhittable target",
                StringComparison.Ordinal));

        TimeOutACast(controller, 10u, 100u, "Flame Bolt VII", "ZojakQuazael");
        Assert.Contains(
            surface.PostedSystemMessages,
            message => message.Contains(
                "Blacklisting unhittable target Drudge (10)",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// Drives one cast all the way to the result timeout: begin it, let the
    /// gesture echo move it on to waiting for a result, then spend the whole
    /// result budget.
    /// </summary>
    private static void TimeOutACast(
        CombatController controller,
        uint targetObjectId,
        uint spellId,
        string spellName,
        string saying)
    {
        controller.CastTracker.Reset();
        controller.CastTracker.Begin(
            spellId,
            spellName,
            targetObjectId,
            string.Empty,
            hitsMultipleTargets: false,
            issueRevision: 0,
            saying);
        controller.CastTracker.ObserveChat(
            0uL,
            saying,
            ownSpeech: true,
            logTextType: 0x11u);
        controller.CastTracker.Advance(SpellCastTracker.ResultTimeoutSeconds + 0.1d);
    }

    [Fact]
    public void PermanentFailResultTextForceBlacklistsTheTarget()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
            BlacklistMonsterTimeoutSeconds = 300,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((100u, 10u), surface.LastTargetedCast);

        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty, "Drudge is an invalid target.", string.Empty)
            {
                LogTextType = 0x07u,
            },
        ];
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.Contains(
            "Waiting for a target",
            controller.Status,
            StringComparison.Ordinal);
        Assert.False(controller.HasTarget);
    }

    [Fact]
    public void PermanentFailIsIgnoredWhileAMultiTargetSpellIsInFlight()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300) with
                {
                    Family = 638u,
                },
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
            BlacklistMonsterTimeoutSeconds = 300,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((100u, 10u), surface.LastTargetedCast);

        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty, "Drudge is an invalid target.", string.Empty)
            {
                LogTextType = 0x07u,
            },
        ];
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.True(controller.HasTarget);
    }

    [Fact]
    public void AKillLineEndsTheTargetBeforeTheWorldRemovesIt()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((100u, 10u), surface.LastTargetedCast);
        int castsBeforeTheKill = surface.CastSpellIds.Count;

        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty, "You killed Drudge!", string.Empty),
        ];
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.False(controller.HasTarget);
        Assert.Equal(castsBeforeTheKill, surface.CastSpellIds.Count);
        Assert.Contains(
            "Waiting for a target",
            controller.Status,
            StringComparison.Ordinal);
    }

    [Fact]
    public void TheResultWaitTimeoutBumpsTheBlacklistAttemptCount()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
            // No attempts allowed, so the first one that records trips.
            BlacklistMonsterAttemptCount = 0,
            BlacklistMonsterTimeoutSeconds = 300,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((100u, 10u), surface.LastTargetedCast);

        controller.CastTracker.ObserveCompletion(
            new PluginCastCompletion(1, 100u, 10u, 0u));
        Assert.True(controller.HasTarget);

        // 4 x 907 ms of the result timer (gj.cs:254-255).
        controller.CastTracker.Advance(3.7d);
        controller.OnTick(0.25);

        Assert.False(controller.HasTarget);
    }

    [Fact]
    public void AServerRefusedCastDoesNotCountAgainstTheTarget()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            ScanIntervalSeconds = 0.05d,
            BlacklistMonsterAttemptCount = 1,
            BlacklistMonsterTimeoutSeconds = 300,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal((100u, 10u), surface.LastTargetedCast);

        surface.LastCastCompletion = new PluginCastCompletion(1, 100u, 10u, 0x1Du);
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.True(controller.HasTarget);
    }

    [Fact]
    public void ATwentyCandidateScanDoesNotRebuildThePlanPerCandidate()
    {
        var targets = new List<PluginCombatTarget>();
        for (uint i = 0; i < 20u; i++)
            targets.Add(Target(10u + i, "Drudge", distance: 3f + i, angle: 0));
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = targets,
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
                Debuff(80, "Fester Other VII"),
                Debuff(81, "Broadside of a Barn"),
                Debuff(82, "Fire Vulnerability Other VII"),
                Debuff(83, "Imperil Other VII"),
                Debuff(84, "Magic Yield Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack
                    | MonsterActionFlags.Yield
                    | MonsterActionFlags.Imperil
                    | MonsterActionFlags.Vulnerability
                    | MonsterActionFlags.Broadside
                    | MonsterActionFlags.Fester,
                DamageType = MonsterDamageType.Auto,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        // Measured: 2 and 145 with the memos, 169 and 211 without.
        Assert.InRange(surface.CaptureOwnedEquipmentCount, 1, 20);
        Assert.InRange(surface.KnownCombatSpellReads, 1, 180);
    }

    /// <summary>
    /// A pass that has to choose again — every monster in range unreachable —
    /// re-runs the whole selection up to five hundred times. The host builds
    /// the equipment and inventory projections by walking every object it
    /// knows and sorting the result, so they are read once for the pass, not
    /// once per attempt.
    /// Mutation: read the host directly in <c>RefreshTarget</c> (or in
    /// <c>TryPrepareAttack</c>) instead of the pass memo and the counts run
    /// into the dozens.
    /// </summary>
    [Fact]
    public void APassThatChoosesAgainStillCapturesTheCharacterOnlyOnce()
    {
        var targets = new List<PluginCombatTarget>();
        for (uint i = 0; i < 8u; i++)
            targets.Add(Target(10u + i, "Drudge", distance: 3f + i, angle: 0));
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = targets,
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
            // Nothing can be reached, so every attempt turns a column off and
            // the pass chooses again until it runs out of monsters.
            ProjectilePath = new(PluginProjectilePathStatus.Blocked),
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.True(
            surface.ProjectilePathChecks >= 2,
            "the pass never had to choose again, so nothing is being measured");
        Assert.InRange(surface.CaptureOwnedEquipmentCount, 1, 2);
        Assert.InRange(surface.CaptureOwnedItemsCount, 1, 2);
    }

    /// <summary>
    /// A kill or a success starts the attempt count over; it does NOT lift a
    /// blacklist that is still running. Mutation: make <c>ResetAttempts</c>
    /// clear the deadline too and the second assertion fails.
    /// </summary>
    [Fact]
    public void KillAndSuccessResultTextResetTheAttemptCountOnly()
    {
        var settings = new CombatSettings { BlacklistMonsterTimeoutSeconds = 300 };
        var tracker = new CombatFailureTracker();

        tracker.ObserveTargets([Target(10, "Drudge", 5, 0)], 0d, settings);
        tracker.ForceBlacklist(10u, now: 0d, settings);
        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10u, now: 1d));

        tracker.ResetAttempts(10u);
        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10u, now: 1d));
    }

    [Fact]
    public void RingOnlyRuleCastsUntargetedRingWithOneNearbyRingTarget()
    {
        PluginSpellInfo ring = Spell(110, "Cassius' Ring of Fire") with
        {
            IsOffensive = true,
            TargetMask = 0,
            Description = "Shoots waves of fire outward from the caster.",
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 3, 0)],
            KnownCombatSpells = [ring],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Ring,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(110u, surface.LastUntargetedCast);
        Assert.Equal(default, surface.LastTargetedCast);
    }

    /// <summary>
    /// Two wands of the same name, neither of them named by the rule, both on
    /// the Items page: the automatic pick has to name the same one on the pass
    /// after it is wielded. The list the host hands out puts what is held
    /// first, so the pick has to come from the page's order rather than that
    /// one, or the two wands take turns and the character swaps for ever
    /// without ever attacking.
    ///
    /// Mutation: hand <c>VtankWeaponLadder.Select</c> the projection itself
    /// instead of <c>InProfileOrder</c> and this fails with two equips.
    /// </summary>
    [Fact]
    public void TwoWandsOfOneNameDoNotTakeTurnsBeingWielded()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Peace },
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems =
            [
                Equipment(
                    0x80000A4Cu,
                    "Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
                Equipment(
                    0x80000B34u,
                    "Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Wand");
        settings.CombatItemOrder.Add("Wand");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Auto,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        string[] first = surface.CallLog
            .Where(static entry => entry.StartsWith("Equip:", StringComparison.Ordinal))
            .ToArray();
        Assert.Single(first);

        controller.OnTick(0.25);
        controller.OnTick(0.25);

        string[] every = surface.CallLog
            .Where(static entry => entry.StartsWith("Equip:", StringComparison.Ordinal))
            .Distinct(StringComparer.Ordinal)
            .ToArray();
        Assert.Equal(first, every);
    }

    [Fact]
    public void ExplicitMonsterWeaponUsesCanonicalEquipmentCommandBeforeAttack()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems =
            [
                Equipment(700, "Fire Sword", damageType: 0x10),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                WeaponObjectId = 700,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(700u, surface.LastEquipObjectId);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void ExplicitMonsterWeaponResolvesDurableNameAfterRelogChangesObjectId()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems =
            [
                Equipment(900, "Fire Sword", damageType: 0x10),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                WeaponObjectId = 700,
                WeaponName = "Fire Sword",
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(900u, surface.LastEquipObjectId);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void AutomaticDamageSelectionChoosesStrongestMatchingWeapon()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems =
            [
                Equipment(700, "Weak fire", damageType: 0x10, damage: 20),
                Equipment(701, "Strong fire", damageType: 0x10, damage: 35),
                Equipment(702, "Acid", damageType: 0x20, damage: 99),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Strong fire");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(701u, surface.LastEquipObjectId);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void AttackRoutesTheCombatModeThroughTheSharedGateNotEnterDefaultMode()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems =
            [
                Equipment(
                    700,
                    "War Wand",
                    damageType: 0,
                    equippedLocation: 0x00100000u,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions { Flags = MonsterActionFlags.Attack }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        // The wand is already wielded; only the mode is wrong. The gate
        // recomputes the mode the wielded item implies and asks for it
        // (ga.cs:1556-1565).
        Assert.Contains("EnterMode:Magic", surface.CallLog);
        Assert.Equal(PluginCombatMode.Magic, surface.CombatSnapshot.Mode);
    }

    [Fact]
    public void AttackWithNothingWieldedAndAnEmptyItemsProfileStopsWithTheWandNotice()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems = [],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions { Flags = MonsterActionFlags.Attack }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(
            "[MossTank] " + CombatModeGate.NoWandNotice,
            Assert.Single(
                surface.PostedSystemMessages,
                message => message.Contains(
                    CombatModeGate.NoWandNotice,
                    StringComparison.Ordinal)));
        Assert.False(controller.Enabled);
        Assert.DoesNotContain("EnterDefaultMode", surface.CallLog);
        Assert.DoesNotContain("EnterMode:Melee", surface.CallLog);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void AttackWithNothingWieldedWieldsTheProfiledWandAndRequestsMagic()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems =
            [
                Equipment(
                    700,
                    "War Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("War Wand");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions { Flags = MonsterActionFlags.Attack }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        int equip = surface.CallLog.IndexOf("Equip:000002BC");
        int magic = surface.CallLog.IndexOf("EnterMode:Magic");
        Assert.True(equip >= 0, "the profiled wand was never wielded");
        Assert.True(magic > equip, "Magic was requested before the wand: "
            + string.Join(" | ", surface.CallLog));
        Assert.True(controller.Enabled);
        Assert.DoesNotContain(
            surface.PostedSystemMessages,
            message => message.Contains(
                CombatModeGate.NoWandNotice,
                StringComparison.Ordinal));
        Assert.DoesNotContain("EnterDefaultMode", surface.CallLog);
    }

    [Fact]
    public void AttackWithAWieldedSwordButNoProfiledWeaponStillTakesTheMagicArm()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems =
            [
                // Wielded, but in NO profile — SelectAutomaticWeapon skips
                // every unprofiled item, so the plan is 0.
                Equipment(
                    900,
                    "Unprofiled Sword",
                    damageType: 0x0001,
                    equippedLocation: 0x00100000u),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions { Flags = MonsterActionFlags.Attack }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(
            "[MossTank] " + CombatModeGate.NoWandNotice,
            Assert.Single(
                surface.PostedSystemMessages,
                message => message.Contains(
                    CombatModeGate.NoWandNotice,
                    StringComparison.Ordinal)));
        Assert.False(controller.Enabled);
        Assert.DoesNotContain("EnterDefaultMode", surface.CallLog);
        Assert.DoesNotContain("EnterMode:Melee", surface.CallLog);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void AttackWithAProfiledMeleeWeaponTakesThePhysicalArmThroughTheGate()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems = [Equipment(900, "Fire Sword", damageType: 0x10)],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Fire Sword");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 6; tick++)
            controller.OnTick(0.25);

        int equip = surface.CallLog.IndexOf("Equip:00000384");
        int melee = surface.CallLog.IndexOf("EnterMode:Melee");
        Assert.True(equip >= 0, "the planned weapon was never wielded");
        Assert.True(melee > equip, "Melee was requested before the weapon: "
            + string.Join(" | ", surface.CallLog));
        Assert.True(controller.Enabled);
        Assert.DoesNotContain(
            surface.PostedSystemMessages,
            message => message.Contains(
                CombatModeGate.NoWandNotice,
                StringComparison.Ordinal));
        Assert.DoesNotContain("EnterDefaultMode", surface.CallLog);
    }

    [Fact]
    public void MissileLauncherSelectsOfficialBestAvailableAmmunition()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            CharacterSkills =
            [
                new PluginSkillInfo(
                    47u,
                    "Missile Weapons",
                    PluginSkillTraining.Trained,
                    300u)
                {
                    Base = 300u,
                },
            ],
            EquipmentItems =
            [
                Equipment(
                    700,
                    "Fire Bow",
                    damageType: 0x10,
                    equippedLocation: 0x00100000u,
                    ammoType: 0x001u),
                Equipment(
                    801,
                    "Deadly Fire Arrow",
                    damageType: 0x10,
                    combatUse: 3,
                    ammoType: 0x001u,
                    stackSize: 20,
                    // A quiver goes in the ammunition slot; it is not
                    // something the character can be wielding.
                    validLocations: AmmunitionSlot),
            ],
            ItemEntries =
            [
                InventoryItem(
                    801,
                    "Deadly Fire Arrow",
                    itemType: 0x100,
                    spellId: 0,
                    equipped: false),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
                WeaponObjectId = 700,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 4; tick++)
            controller.OnTick(0.25);

        Assert.Equal(801u, surface.LastEquipObjectId);
        Assert.Equal(0, surface.BeginCount);
    }

    [Fact]
    public void AmmunitionWieldGoesThroughRetailsDropToPeacePrologue()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Missile },
            Targets = [Target(10, "Drudge", 2, 0)],
            CharacterSkills =
            [
                new PluginSkillInfo(
                    47u, "Missile Weapons", PluginSkillTraining.Trained, 300u)
                {
                    Base = 300u,
                },
            ],
            EquipmentItems =
            [
                Equipment(
                    700,
                    "Fire Bow",
                    damageType: 0x10,
                    equippedLocation: 0x00100000u,
                    ammoType: 0x001u),
                Equipment(
                    801,
                    "Deadly Fire Arrow",
                    damageType: 0x10,
                    combatUse: 3,
                    ammoType: 0x001u,
                    stackSize: 20,
                    // A quiver goes in the ammunition slot; it is not
                    // something the character can be wielding.
                    validLocations: AmmunitionSlot),
            ],
            ItemEntries =
            [
                InventoryItem(801, "Deadly Fire Arrow", 0x100, 0, equipped: false),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
                WeaponObjectId = 700,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        for (int tick = 0; tick < 6; tick++)
            controller.OnTick(0.25);

        // bv.cs:194-195 then :204 — Peace is asked for BEFORE the arrow is
        // wielded, never after.
        int peace = surface.CallLog.IndexOf("EnterMode:Peace");
        int equip = surface.CallLog.IndexOf("Equip:00000321");
        Assert.True(peace >= 0, "Peace was never requested: "
            + string.Join(" | ", surface.CallLog));
        Assert.True(equip > peace, "The arrow was wielded before Peace: "
            + string.Join(" | ", surface.CallLog));
        Assert.Equal(801u, surface.LastEquipObjectId);
    }

    [Fact]
    public void AmmunitionStillWieldsThroughThePanelsExternallyBoundGate()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
            CharacterSkills =
            [
                new PluginSkillInfo(
                    47u, "Missile Weapons", PluginSkillTraining.Trained, 300u)
                {
                    Base = 300u,
                },
            ],
            EquipmentItems =
            [
                Equipment(
                    700,
                    "Fire Bow",
                    damageType: 0x10,
                    equippedLocation: 0x00100000u,
                    ammoType: 0x001u),
                Equipment(
                    801,
                    "Deadly Fire Arrow",
                    damageType: 0x10,
                    combatUse: 3,
                    ammoType: 0x001u,
                    stackSize: 20,
                    // A quiver goes in the ammunition slot; it is not
                    // something the character can be wielding.
                    validLocations: AmmunitionSlot),
            ],
            ItemEntries =
            [
                InventoryItem(801, "Deadly Fire Arrow", 0x100, 0, equipped: false),
            ],
        };
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
                WeaponObjectId = 700,
            }));
        var vitals = new VitalSettings();
        var host = new FakeHost(surface);
        var controller = new CombatController(host, settings, vitals);

        // Exactly MossTankPanel.cs:420-425 — the one shared gate, injected.
        var gate = new CombatModeGate(host, settings, vitals, _ => { });
        controller.BindCombatModeGate(gate);

        controller.Toggle();
        for (int tick = 0; tick < 6; tick++)
            controller.OnTick(0.25);

        Assert.Equal(801u, surface.LastEquipObjectId);
        Assert.NotNull(gate.AmmunitionStale);
        Assert.NotNull(gate.WieldAmmunition);
    }

    [Fact]
    public void StuckCombatModeUsesProfiledCasterAfterRetailRetryCount()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            IgnoreModeChanges = true,
            Targets = [Target(10, "Drudge", 2, 0)],
            EquipmentItems =
            [
                Equipment(700, "Fire Sword", damageType: 0x10),
                Equipment(
                    800,
                    "Recovery Wand",
                    damageType: 0,
                    itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                WeaponObjectId = 700,
            }));
        var controller = new CombatController(
            new FakeHost(surface),
            settings,
            new VitalSettings { DropToPeaceModeRetryCount = 2 });

        controller.Toggle();
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.Equal(1, surface.ModeChangeRequests);
        Assert.Equal(800u, surface.LastUsedItem);
        Assert.Equal(0u, surface.LastEquipObjectId);
        Assert.Contains(
            "bugged combat state",
            controller.Status,
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void SessionLossDisablesAndAborts()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 2, 0)],
        };
        var controller = new CombatController(
            new FakeHost(surface), new CombatSettings());
        controller.Toggle();
        surface.IsAvailable = false;

        controller.OnTick(0.1);

        Assert.False(controller.Enabled);
        Assert.Equal(1, surface.AbortCount);
        Assert.Equal("Session ended", controller.Status);
    }

    [Fact]
    public void CasterItemDebuffWaitsForUseDoneAndConfirmedCastChat()
    {
        PluginSpellInfo imperil = Spell(90, "Imperil Other VII") with
        {
            School = 31,
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
        };
        PluginInventoryItem lens = InventoryItem(
            800, "Imperil Lens", 0x8000, 90, equipped: true) with
        {
            ItemSpellcraft = 400,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            SpellLookup = [imperil],
            ItemEntries = [lens],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.CombatItemObjectIds.Add(lens.ObjectId);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((800u, 10u), surface.LastAppliedItem);
        Assert.Equal(0, surface.BeginCount);

        Assert.True(controller.HasPendingItemDebuff);

        surface.LastItemCompletion = new PluginItemUseCompletion(
            1, 800, 10, 0);
        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty,
                "You cast Imperil Other VII on Drudge.",
                string.Empty)
            {
                LogTextType = 0x07u,
            },
        ];
        controller.OnTick(0.25);

        Assert.Equal(1, surface.ApplyCount);
        Assert.Contains("Waiting for a target", controller.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// Using a wand on a monster holds the item slot for the whole cast, so
    /// the attack rule (whose first refusal is that slot) cannot swing inside
    /// the wand's own animation, and the pass line's item column says so.
    /// Mutation: delete the <c>Arm(ActionLockKind.ItemUse, ...)</c> beside the
    /// wand's <c>Apply</c> and the first assertion fails; delete the
    /// <c>Release</c> in <c>ClearPendingItemDebuff</c> and the last one does.
    /// </summary>
    [Fact]
    public void AWandDebuffHoldsTheItemSlotForItsWholeCast()
    {
        PluginSpellInfo imperil = Spell(90, "Imperil Other VII") with
        {
            School = 31,
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
        };
        PluginInventoryItem lens = InventoryItem(
            800, "Imperil Lens", 0x8000, 90, equipped: true) with
        {
            ItemSpellcraft = 400,
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            SpellLookup = [imperil],
            ItemEntries = [lens],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.CombatItemObjectIds.Add(lens.ObjectId);
        var locks = new ActionLockTable();
        var controller = new CombatController(new FakeHost(surface), settings);
        controller.BindActionLocks(locks, () => false);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((800u, 10u), surface.LastAppliedItem);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        // Still held most of the way through the cast window.
        locks.Advance(11d);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        surface.LastItemCompletion = new PluginItemUseCompletion(
            1, 800, 10, 0);
        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty,
                "You cast Imperil Other VII on Drudge.",
                string.Empty)
            {
                LogTextType = 0x07u,
            },
        ];
        controller.OnTick(0.25);

        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    [Fact]
    public void ProcWeaponChargesAtZeroAndRequiresCastChatNotAttackDone()
    {
        PluginSpellInfo imperil = Spell(91, "Imperil Other VII") with
        {
            School = 31,
            IsDebuff = true,
            IsOffensive = true,
            DurationSeconds = 60,
            TargetMask = 0x10,
        };
        PluginInventoryItem sword = InventoryItem(
            801, "Imperil Sword", 1, 0, equipped: true) with
        {
            ItemSpellcraft = 400,
            AppraisedSpellIds = [91u],
        };
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 5, 0)],
            SpellLookup = [imperil],
            ItemEntries = [sword],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.CombatItemObjectIds.Add(sword.ObjectId);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(10u, surface.LastBeginTarget);
        Assert.Equal(0f, surface.LastBeginPower);

        surface.CombatSnapshot = Physical(request: true, build: true, bar: 0);
        controller.OnTick(0.1);
        Assert.Equal(1, surface.ReleaseCount);

        surface.CombatSnapshot = Physical() with
        {
            CompletionRevision = 1,
            CompletionWeenieError = 0,
        };
        controller.OnTick(0.1);
        Assert.DoesNotContain("Waiting for a target", controller.Status, StringComparison.Ordinal);

        surface.ChatMessages =
        [
            new PluginChatMessage(
                1, 0, 0, string.Empty,
                "You cast Imperil Other VII on Drudge.",
                string.Empty)
            {
                LogTextType = 0x07u,
            },
        ];
        controller.OnTick(0.1);
        Assert.Contains("Waiting for a target", controller.Status, StringComparison.Ordinal);
    }


    private static CombatModeGate Gate(
        FakeAutomation surface,
        CombatSettings? settings = null,
        VitalSettings? vitals = null,
        List<string>? stops = null) =>
        new(
            new FakeHost(surface),
            settings ?? new CombatSettings(),
            vitals ?? new VitalSettings(),
            notice => (stops ?? []).Add(notice));

    [Fact]
    public void GateWieldedCasterInPeaceReAsksForMagicUntilTheClientAgrees()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Peace },
            DeferModeConfirmation = true,
            EquipmentItems =
            [
                Equipment(
                    800,
                    "Recovery Wand",
                    damageType: 0,
                    itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
            ],
        };
        CombatModeGate gate = Gate(surface);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(1, surface.ModeChangeRequests);
        Assert.Equal(["EnterMode:Magic"], surface.CallLog);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(2, surface.ModeChangeRequests);

        surface.ConfirmPendingModeChange();
        gate.AdvancePass(0.1);
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(2, surface.ModeChangeRequests);
    }

    [Fact]
    public void GateProfiledCasterNotWieldedWieldsThenEntersMagicInOrder()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(), // Mode = Melee
            DeferModeConfirmation = true,
            SimulateAsyncEquip = true,
            EquipmentItems =
            [
                Equipment(800, "Recovery Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        CombatModeGate gate = Gate(surface, settings);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(["EnterMode:Peace"], surface.CallLog);

        surface.ConfirmPendingModeChange();
        // R2-15: the ack RESTARTS the 600 ms window (f9.cs:322-328 stamps
        // m_h again), so f9.e() still reports the PRE-request mode for one
        // more pass and the drop-to-peace branch re-asks. That second request
        // re-stamps m_g from the now-Peace live mode, which is what lets the
        // pass after it proceed.
        gate.AdvancePass(1.0);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(
            ["EnterMode:Peace", "EnterMode:Peace"],
            surface.CallLog);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        // Equip only happens after the snapshot reports Peace, never before.
        Assert.Equal(
            ["EnterMode:Peace", "EnterMode:Peace", "Equip:00000320"],
            surface.CallLog);

        surface.ConfirmPendingEquip();
        gate.AdvancePass(1.0);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(
            [
                "EnterMode:Peace", "EnterMode:Peace", "Equip:00000320",
                "EnterMode:Magic",
            ],
            surface.CallLog);

        surface.ConfirmPendingModeChange();
        gate.AdvancePass(1.0);
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(
            [
                "EnterMode:Peace", "EnterMode:Peace", "Equip:00000320",
                "EnterMode:Magic",
            ],
            surface.CallLog);
    }

    [Fact]
    public void GateLogsRequestingPeaceThenEquipThenRequestingMagicAndNeverReDropsAfterEquip()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(), // Mode = Melee, empty hands
            EquipmentItems =
            [
                Equipment(800, "Recovery Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        CombatModeGate gate = Gate(surface, settings);
        var log = new List<(MacroLogChannel Channel, string Message)>();
        gate.Log = (channel, message) => log.Add((channel, message));

        bool ready = false;
        for (int pass = 0; pass < 8 && !ready; pass++)
        {
            gate.AdvancePass(1.0);
            ready = gate.TryPrepare(PluginCombatMode.Magic);
        }
        Assert.True(ready, "gate never converged");

        Assert.Equal(
            [
                (MacroLogChannel.BusyState, "(FCM) requesting Peace"),
                (MacroLogChannel.BusyState, "(FCM) requesting Peace"),
                (MacroLogChannel.BusyState, "(FCM) equip Recovery Wand"),
                (MacroLogChannel.BusyState, "(FCM) requesting Magic"),
            ],
            log);

        int equipIndex = log.FindIndex(entry => entry.Message.Contains("equip"));
        Assert.DoesNotContain(
            log.Skip(equipIndex + 1),
            entry => entry.Message.Contains("requesting Peace"));
    }

    [Fact]
    public void GateNoWandAnywherePostsTheNoticeOnceAndStopsTheMacro()
    {
        var surface = new FakeAutomation { CombatSnapshot = Physical() };
        var stops = new List<string>();
        CombatModeGate gate = Gate(surface, stops: stops);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(
            "You must add at least one wand to your profile.",
            Assert.Single(surface.PostedSystemMessages).Replace(
                "[MossTank] ",
                string.Empty,
                StringComparison.Ordinal));
        Assert.Equal(
            "You must add at least one wand to your profile.",
            Assert.Single(stops));

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Single(surface.PostedSystemMessages);
        Assert.Equal(2, stops.Count);

        gate.Reset();
        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(2, surface.PostedSystemMessages.Count);
    }

    [Fact]
    public void GateModeRequestNeverConfirmedKeepsAskingWithoutGivingUp()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Peace },
            DeferModeConfirmation = true,
            EquipmentItems =
            [
                Equipment(
                    800,
                    "Recovery Wand",
                    damageType: 0,
                    itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
            ],
        };
        var stops = new List<string>();
        CombatModeGate gate = Gate(
            surface,
            vitals: new VitalSettings { DropToPeaceModeRetryCount = 2 },
            stops: stops);

        for (int pass = 0; pass < 6; pass++)
        {
            gate.AdvancePass(1.0);
            Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        }

        Assert.Equal(6, surface.ModeChangeRequests);
        Assert.Empty(stops);
        Assert.Empty(surface.PostedSystemMessages);
    }

    [Fact]
    public void GateDropToPeaceBudgetIsCountedInPassesAndEndsInWandUseRecovery()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            DeferModeConfirmation = true,
            EquipmentItems =
            [
                Equipment(800, "Recovery Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        var stops = new List<string>();
        CombatModeGate gate = Gate(
            surface,
            settings,
            new VitalSettings { DropToPeaceModeRetryCount = 3 },
            stops);

        // Three passes, each well past the 600 ms window so the Peace request
        // is genuinely re-issued; the third exhausts the budget.
        gate.AdvancePass(1.0);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        gate.AdvancePass(1.0);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(2, surface.ModeChangeRequests);
        Assert.Equal(0u, surface.LastUsedItem);

        gate.AdvancePass(1.0);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(800u, surface.LastUsedItem);
        Assert.Equal(2, surface.ModeChangeRequests);
        Assert.Empty(stops);
        Assert.Contains(
            "bugged combat state",
            Assert.Single(surface.PostedSystemMessages),
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void GateFallbackWandIsItemsPageInsertionOrderNotNameOrder()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Peace },
            EquipmentItems =
            [
                Equipment(801, "Zephyr Wand", damageType: 0, itemType: 0x00008000u),
                Equipment(802, "Adamant Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Zephyr Wand");
        settings.CombatItemNames.Add("Adamant Wand");
        settings.CombatItemOrder.Add("Zephyr Wand");
        settings.CombatItemOrder.Add("Adamant Wand");
        CombatModeGate gate = Gate(surface, settings);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));

        Assert.Equal(["Equip:00000321"], surface.CallLog);
    }

    [Fact]
    public void GateKeepsAnAlreadyWieldedCasterRatherThanReEquipping()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            EquipmentItems =
            [
                Equipment(
                    801,
                    "Zephyr Wand",
                    damageType: 0,
                    itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
                Equipment(802, "Adamant Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Adamant Wand");
        settings.CombatItemOrder.Add("Adamant Wand");
        CombatModeGate gate = Gate(surface, settings);

        gate.AdvancePass(0.1);
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Empty(surface.CallLog);
    }

    [Fact]
    public void GateIsHandsOffWhileTheCombatModeIsUnknown()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = new PluginCombatSnapshot
            {
                Mode = PluginCombatMode.Unknown,
            },
            EquipmentItems =
            [
                Equipment(800, "Recovery Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        CombatModeGate gate = Gate(surface, settings);

        gate.AdvancePass(0.1);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Empty(surface.CallLog);
    }

    [Fact]
    public void GateWithNoEquipmentProjectionStillRequiresTheMode()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            EquipmentAvailable = false,
        };
        CombatModeGate gate = Gate(surface);

        gate.AdvancePass(0.1d);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(["EnterMode:Magic"], surface.CallLog);

        gate.AdvancePass(1d);
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal("Ready", gate.Status);
    }

    [Fact]
    public void GateTreatsAnUnavailableModeCommandAsReadyRatherThanDeadlock()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            ModeCommandUnavailable = true,
            EquipmentItems =
            [
                Equipment(800, "War Wand", damageType: 0, itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("War Wand");
        settings.CombatItemOrder.Add("War Wand");
        CombatModeGate gate = Gate(surface, settings);

        gate.AdvancePass(0.1d);
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
    }

    [Fact]
    public void OncePerRunWarningLatchClearsAtTheMacroStartEdge()
    {
        var surface = new FakeAutomation { CombatSnapshot = Physical() };
        CombatModeGate gate = Gate(surface);

        gate.AdvancePass(0.1d);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Single(surface.PostedSystemMessages);

        gate.AdvancePass(0.1d);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Single(surface.PostedSystemMessages);

        gate.ResetOncePerRunWarnings();
        gate.AdvancePass(0.1d);
        Assert.False(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Equal(2, surface.PostedSystemMessages.Count);
    }

    [Fact]
    public void FcmIgnoringItemWarningReachesChatOncePerRun()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            EquipmentItems =
            [
                Equipment(900, "Bread", damageType: 0, itemType: 1) with
                {
                    ValidLocations = 0u,
                },
                Equipment(800, "War Wand", damageType: 0, itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
            ],
        };
        CombatModeGate gate = Gate(surface);

        gate.AdvancePass(0.1d);
        gate.TryPrepare(PluginCombatMode.Magic, overrideItemId: 900u);
        string warning = Assert.Single(surface.PostedSystemMessages);
        Assert.Contains(
            "Warning: FCM ignoring item Bread because it cannot currently be "
            + "wielded.",
            warning,
            StringComparison.Ordinal);

        gate.AdvancePass(0.1d);
        gate.TryPrepare(PluginCombatMode.Magic, overrideItemId: 900u);
        Assert.Single(surface.PostedSystemMessages);

        gate.ResetOncePerRunWarnings();
        gate.AdvancePass(0.1d);
        gate.TryPrepare(PluginCombatMode.Magic, overrideItemId: 900u);
        Assert.Equal(2, surface.PostedSystemMessages.Count);
    }

    [Fact]
    public void TheModeWindowIsTimeBasedAndTheAckRestartsIt()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(), // Melee
            DeferModeConfirmation = true,
            EquipmentItems =
            [
                Equipment(800, "Recovery Wand", damageType: 0, itemType: 0x00008000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        CombatModeGate gate = Gate(surface, settings);

        // Ask for Peace so the wand can be wielded.
        gate.AdvancePass(0.1d);
        Assert.False(gate.TryDropToPeace(surface.EquipmentItems, "Recovery Wand"));
        Assert.Equal(["EnterMode:Peace"], surface.CallLog);

        // The ack arrives. The window RESTARTS, so f9.e() still reports the
        // pre-request Melee and the branch re-asks — which re-stamps the
        // saved mode from the now-Peace live one.
        surface.ConfirmPendingModeChange();
        gate.AdvancePass(0.1d);
        Assert.False(gate.TryDropToPeace(surface.EquipmentItems, "Recovery Wand"));
        Assert.Equal(["EnterMode:Peace", "EnterMode:Peace"], surface.CallLog);

        gate.AdvancePass(0.1d);
        Assert.True(gate.TryDropToPeace(surface.EquipmentItems, "Recovery Wand"));
        Assert.Equal(["EnterMode:Peace", "EnterMode:Peace"], surface.CallLog);
    }

    [Fact]
    public void ThreeGateCallsWithinOnePassAdvanceTheClockExactlyOnce()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            EquipmentItems =
            [
                Equipment(
                    800,
                    "Recovery Wand",
                    damageType: 0,
                    itemType: 0x00008000u,
                    equippedLocation: 0x00100000u),
            ],
        };
        var settings = new CombatSettings();
        settings.CombatItemNames.Add("Recovery Wand");
        settings.CombatItemOrder.Add("Recovery Wand");
        CombatModeGate gate = Gate(surface, settings);

        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Empty(surface.CallLog);

        gate.AdvancePass(0.25);
        double afterOneAdvance = gate.SinceModeRequestSecondsForTests;

        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.True(gate.TryPrepare(PluginCombatMode.Magic));
        Assert.Empty(surface.CallLog);

        Assert.Equal(afterOneAdvance, gate.SinceModeRequestSecondsForTests);
    }


    private static MacroPassContext IdleTurn(bool canAct = true) =>
        new(0.3d, canAct);

    [Fact]
    public void IdlePeaceRequestsOnEveryPassItWins()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(), // Mode = Melee
            IgnoreModeChanges = true, // keep the rule valid across passes
        };
        var rule = new IdlePeaceRule(
            new FakeHost(surface), new CombatSettings { IdlePeaceMode = true });

        Assert.True(rule.ValidNow(IdleTurn()));
        rule.Running = true;
        Assert.Equal(1, surface.ModeChangeRequests);
        Assert.Contains("peace", rule.Status!, StringComparison.OrdinalIgnoreCase);

        Assert.True(rule.ValidNow(IdleTurn()));
        rule.Running = true;
        Assert.Equal(2, surface.ModeChangeRequests);
    }

    [Fact]
    public void IdlePeaceIsLastInTheListAndIsInertWhenAskedWithCanActFalse()
    {
        var surface = new FakeAutomation { CombatSnapshot = Physical() };
        var rule = new IdlePeaceRule(
            new FakeHost(surface), new CombatSettings { IdlePeaceMode = true });

        IReadOnlyList<MacroRuleEntry> entries = MacroRuleTable.Entries;
        Assert.Equal(MacroRuleSlot.IdlePeace, entries[^1].Slot);
        Assert.DoesNotContain(
            entries.Take(entries.Count - 1),
            static entry => entry.Slot == MacroRuleSlot.IdlePeace);

        Assert.False(rule.ValidNow(IdleTurn(canAct: false)));

        rule.Running = false;
        Assert.Equal(0, surface.ModeChangeRequests);
        Assert.Null(rule.Status);
        Assert.Equal(PluginCombatMode.Melee, surface.CombatSnapshot.Mode);
    }

    [Fact]
    public void IdlePeaceModeOffNeverRequests()
    {
        var surface = new FakeAutomation { CombatSnapshot = Physical() };
        var rule = new IdlePeaceRule(
            new FakeHost(surface), new CombatSettings { IdlePeaceMode = false });

        Assert.False(rule.ValidNow(IdleTurn()));
        Assert.Equal(0, surface.ModeChangeRequests);
        Assert.Null(rule.Status);
    }

    /// <summary>
    /// The rule has nothing to do when the character is already at peace.
    /// </summary>
    [Fact]
    public void IdlePeaceIsInvalidWhenAlreadyInPeace()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = new PluginCombatSnapshot { Mode = PluginCombatMode.Peace },
        };
        var rule = new IdlePeaceRule(
            new FakeHost(surface), new CombatSettings { IdlePeaceMode = true });

        Assert.Equal(PluginCombatMode.Peace, surface.CombatSnapshot.Mode);
        Assert.False(rule.ValidNow(IdleTurn()));
    }

    [Fact]
    public void IdlePeaceStillFiresWhenCombatPolicyIsDisabled()
    {
        var surface = new FakeAutomation { CombatSnapshot = Physical() };
        var settings = new CombatSettings { IdlePeaceMode = true, Enabled = false };
        var controller = new CombatController(new FakeHost(surface), settings);
        var rule = new IdlePeaceRule(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.False(controller.HasTarget);
        Assert.Contains("disabled", controller.Status, StringComparison.OrdinalIgnoreCase);

        Assert.True(rule.ValidNow(IdleTurn()));
        rule.Running = true;

        Assert.Equal(1, surface.ModeChangeRequests);
        Assert.Equal(PluginCombatMode.Peace, surface.CombatSnapshot.Mode);
    }

    /// <summary>
    /// Mutation: restore <c>RepeatAttackInProgress</c> (or any of the three
    /// request flags) to the target-refresh hold and this fails — the macro
    /// stays locked onto the drudge for the whole auto-repeat engagement.
    /// </summary>
    [Fact]
    public void AnAutoRepeatSwingDoesNotFreezeTargetSelection()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", distance: 3, angle: 0)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        var settings = new CombatSettings
        {
            MaximumRange = 20f,
            SelectionMethod = TargetSelectionMethod.Range,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Insert(0, new MonsterRule("name#^Olthoi", 4));
        ProfileFixtureWeapon(settings);
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(10u, surface.LastBeginTarget);

        // The swing loop is running. It runs BESIDE the rule pass, so it must
        // not stop the pass re-picking.
        surface.CombatSnapshot = Physical() with
        {
            RepeatAttackInProgress = true,
        };
        surface.Targets =
        [
            Target(10, "Drudge", distance: 3, angle: 0),
            Target(20, "Olthoi Soldier", distance: 15, angle: 60),
        ];
        controller.OnTick(0.25);

        Assert.Contains(
            "Olthoi Soldier",
            controller.TargetText,
            StringComparison.Ordinal);
    }

    /// <summary>
    /// Mutation: delete the <c>_suspendPass()</c> call in
    /// <c>HoldPassForTurn</c> and the first assertion fails; delete the
    /// <c>_resumePass()</c> call in <c>StopBreakableTurnMovement</c> and the
    /// last one does.
    /// </summary>
    [Fact]
    public void ATurnInFlightHoldsTheWholePassUntilTheCharacterHasFaced()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    RequiresTurnTo = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
            NavigationSnapshot = NavigationAt(heading: 0f),
        };
        surface.NavigationObjects[10u] = new PluginNavigationObject(
            10u,
            "Drudge",
            new PluginNavigationPosition(0x7F7F0001u, 0.1d, 0d, 0d, 0f, true));
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { UseBreakableTurnTo = true }));
        int suspends = 0;
        int resumes = 0;
        controller.BindPassSuspension(() => suspends++, () => resumes++);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(1, suspends);
        Assert.Equal(0, resumes);
        Assert.Empty(surface.CastSpellIds);

        // The pass is frozen; the turn is driven beside it and does not raise
        // a second hold.
        controller.AdvanceHeldTurn(0.25);
        controller.AdvanceHeldTurn(0.25);
        Assert.Equal(1, suspends);
        Assert.Equal(0, resumes);

        surface.NavigationSnapshot = NavigationAt(heading: 90f);
        controller.AdvanceHeldTurn(0.25);

        Assert.Equal(1, resumes);

        controller.OnTick(0.25);
        Assert.Equal([100u], surface.CastSpellIds);
    }

    private static (FakeAutomation Surface, CombatController Controller, ActionLockTable Locks)
        MeleeKillRig(CombatSettings? settings = null)
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { SelectedObjectId = 10u },
            Targets = [Target(10, "Drudge", distance: 2, angle: 0)],
            EquipmentItems = [WieldedPlannedWeapon()],
        };
        CombatSettings resolved = settings ?? new CombatSettings();
        resolved.ScanIntervalSeconds = 0.05d;
        ProfileFixtureWeapon(resolved);
        var locks = new ActionLockTable();
        var controller = new CombatController(new FakeHost(surface), resolved);
        controller.BindActionLocks(locks, () => true);
        controller.Toggle();
        controller.OnTick(0.25);
        return (surface, controller, locks);
    }

    /// <summary>
    /// A line the client logged. <paramref name="logTextType"/> names the log
    /// it came from: 0 for a plain line, 0x16 for the character's own combat
    /// log, 0x07 for a spell result.
    /// </summary>
    private static PluginChatMessage ChatLine(
        ulong sequence,
        string text,
        uint logTextType = 0u) =>
        new(sequence, 0u, 0, string.Empty, text, string.Empty)
        {
            LogTextType = logTextType,
        };

    /// <summary>
    /// Mutation: delete the <c>ObservePhysicalResultText</c> call from the
    /// chat walk and this fails — a swung-down monster stays the target until
    /// the world stops listing it, so the bot keeps hitting the corpse.
    /// </summary>
    [Fact]
    public void AMeleeKillLineEndsTheTargetAndHoldsNavigation()
    {
        (FakeAutomation surface, CombatController controller, ActionLockTable locks) =
            MeleeKillRig();
        Assert.Equal(10u, surface.LastBeginTarget);

        surface.ChatMessages = [ChatLine(1, "You killed Drudge!")];
        controller.OnTick(0.25);

        Assert.False(controller.HasTarget);
        Assert.True(locks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// The reader keys on which of the client's logs a line came from, not on
    /// its words: a player typing the kill sentence, or the damage sentence,
    /// in chat must not end the fight or clear the give-up count.
    /// Mutation: drop either log-type test in the physical result reader and
    /// the matching half fails.
    /// </summary>
    [Fact]
    public void SomebodyTypingTheKillSentenceInChatChangesNothing()
    {
        (FakeAutomation surface, CombatController controller, ActionLockTable locks) =
            MeleeKillRig();

        // Speech carries the local-speech log type, not the plain one.
        surface.ChatMessages =
        [
            ChatLine(1, "You killed Drudge!", logTextType: 0x02u),
        ];
        controller.OnTick(0.25);

        Assert.True(controller.HasTarget);
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));

        surface.ChatMessages = [ChatLine(2, "You killed Drudge!")];
        controller.OnTick(0.25);

        Assert.False(controller.HasTarget);
    }

    /// <summary>
    /// Mutation: drop the slain-name comparison and this fails — a fellow's
    /// kill of something else would end our own target.
    /// </summary>
    [Fact]
    public void AKillLineNamingAnotherCreatureDoesNotEndOurTarget()
    {
        (FakeAutomation surface, CombatController controller, _) = MeleeKillRig();

        surface.ChatMessages = [ChatLine(1, "You killed Mosswart!")];
        controller.OnTick(0.25);

        Assert.True(controller.HasTarget);
        Assert.Contains("Drudge", controller.TargetText, StringComparison.Ordinal);
    }

    /// <summary>
    /// Mutation: feed the give-up counter from anything other than the
    /// shot-hit-the-world line — for instance from every completed swing whose
    /// target health did not move — and the second half of this fails, because
    /// an ordinary miss would count.
    /// </summary>
    [Fact]
    public void OnlyAShotIntoTheSceneryCountsTowardsGivingUpOnAMonster()
    {
        (FakeAutomation surface, CombatController controller, _) = MeleeKillRig(
            new CombatSettings
            {
                BlacklistMonsterAttemptCount = 1,
                BlacklistMonsterTimeoutSeconds = 300,
            });

        surface.ChatMessages =
        [
            ChatLine(1, "Your missile attack hit the environment."),
            ChatLine(2, "Your missile attack hit the environment."),
        ];
        controller.OnTick(0.25);
        Assert.False(controller.HasTarget);

        (surface, controller, _) = MeleeKillRig(new CombatSettings
        {
            BlacklistMonsterAttemptCount = 1,
            BlacklistMonsterTimeoutSeconds = 300,
        });
        surface.ChatMessages =
        [
            ChatLine(1, "You evade the Drudge!"),
            ChatLine(2, "The Drudge evades your attack!"),
            ChatLine(3, "You miss the Drudge!"),
        ];
        controller.OnTick(0.25);
        Assert.True(controller.HasTarget);
    }

    /// <summary>
    /// Mutation: delete the damage-report arm and this fails — the two shots
    /// into the scenery either side of a landed hit would add up and retire a
    /// monster the character is demonstrably hitting.
    /// </summary>
    [Fact]
    public void ALandedHitStartsTheGiveUpCountOver()
    {
        (FakeAutomation surface, CombatController controller, _) = MeleeKillRig(
            new CombatSettings
            {
                BlacklistMonsterAttemptCount = 1,
                BlacklistMonsterTimeoutSeconds = 300,
            });

        surface.ChatMessages =
        [
            ChatLine(1, "Your missile attack hit the environment."),
            ChatLine(
                2,
                "You slash Drudge for 43 points of slashing damage!",
                logTextType: 0x16u),
            ChatLine(3, "Your missile attack hit the environment."),
        ];
        controller.OnTick(0.25);

        Assert.True(controller.HasTarget);
    }

    /// <summary>
    /// Mutation: take the retry out of the pass (return instead of choosing
    /// again after an undeliverable decision) and this fails — the macro
    /// stands there staring at the monster behind cover while a reachable one
    /// is next to it.
    /// </summary>
    [Fact]
    public void AMonsterBehindCoverIsPassedOverForAReachableOneInTheSamePass()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets =
            [
                Target(10, "Drudge", distance: 3, angle: 0),
                Target(20, "Mosswart", distance: 6, angle: 10),
            ],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
        };
        // The near one cannot be reached; the far one can.
        surface.ProjectilePaths[10u] = new(
            PluginProjectilePathStatus.Blocked,
            CollisionChecks: 3,
            BlockingObjectId: 0x50000001u);
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            SelectionMethod = TargetSelectionMethod.Range,
            UseProjectileAwareness = true,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 20u), surface.LastTargetedCast);
    }

    /// <summary>
    /// The walk is its own pass. What the attack's pass learned and turned
    /// off belongs to that pass and must not narrow the walk's choice of
    /// monster: here the attack turns the near monster's attack column off
    /// because nothing can be thrown at it, and the walk must still see it —
    /// see it, and stop, because it is already inside weapon range.
    /// Mutation: delete <c>ClearPassMemos()</c> from the head of
    /// <c>CombatController.TickMonsterApproach</c> and both assertions fail —
    /// the near monster is still excluded by the attack's cleared column, so
    /// the walk picks the far one and sets off towards it.
    /// </summary>
    [Fact]
    public void TheWalkDoesNotInheritTheColumnTheAttackTurnedOff()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets =
            [
                Target(10, "Drudge", distance: 4, angle: 0),
                Target(20, "Mosswart", distance: 15, angle: 10),
            ],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            EquipmentItems = [WieldedCaster()],
            NavigationSnapshot = NavigationAt(heading: 0f),
        };
        // Nothing can be thrown at the near one.
        surface.ProjectilePaths[10u] = new(
            PluginProjectilePathStatus.Blocked,
            CollisionChecks: 3,
            BlockingObjectId: 0x50000001u);
        // Somewhere for the walk to go if it wrongly picks the far one.
        surface.NavigationObjects[20u] = new PluginNavigationObject(
            20u,
            "Mosswart",
            new PluginNavigationPosition(0x7F7F0001, 0.1d, 0d, 0d, 0f, true));
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 5d,
            ApproachDistance = 20d,
            SelectionMethod = TargetSelectionMethod.Range,
            UseProjectileAwareness = true,
            ScanIntervalSeconds = 0.05d,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        // The attack pass ran and found nothing it could do.
        Assert.Equal((0u, 0u), surface.LastTargetedCast);

        // Same pass, the walk's turn. The near monster is back in the running
        // and it is already close enough, so there is nowhere to walk.
        Assert.False(controller.TickMonsterApproach(0.05d, canAct: true));
        Assert.Empty(surface.MovementIntents);
    }

    /// <summary>
    /// Mutation: make the retry unbounded (drop the budget) and a rule whose
    /// remaining column keeps failing would spin forever; make the budget the
    /// per-path sample cap again and this test's single blocked monster would
    /// cost 500 rebuilds. Either way the count below moves.
    /// </summary>
    [Fact]
    public void TheRetryStopsAsSoonAsNothingIsLeftToChooseFrom()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", distance: 3, angle: 0)],
            KnownAttackSpells =
            [
                Spell(100, "Incantation of Flame Bolt") with
                {
                    IsProjectile = true,
                },
            ],
            ProjectilePath = new(PluginProjectilePathStatus.Blocked),
            EquipmentItems = [WieldedCaster()],
        };
        var settings = FireAttackRule(new CombatSettings
        {
            MaximumRange = 40d,
            UseProjectileAwareness = true,
        });
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Empty(surface.CastSpellIds);
        Assert.False(controller.HasTarget);
        // Three shapes tried against the one monster, then it is out of the
        // running and there is nothing left: no spinning.
        Assert.InRange(surface.ProjectilePathChecks, 1, 8);
    }

    /// <summary>
    /// Mutation: drop the component term from <c>IsUsableAttackSpell</c> and
    /// this fails — the pick lands on the best tier known, the client refuses
    /// the cast for want of components, and every pass picks it again.
    /// </summary>
    [Fact]
    public void ATierThePackCannotPayForIsNotPicked()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Flame Bolt VII", difficulty: 300),
                MagicSpell(101, "Flame Bolt IV", difficulty: 150),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        surface.MissingComponentSpellIds.Add(100u);
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { MaximumRange = 40d }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((101u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// Mutation: same as above — with EVERY tier unpayable the arm must fall
    /// through to the "no usable attack spell" warning instead of casting.
    /// </summary>
    [Fact]
    public void NoTierIsPickedWhenThePackHasNoComponentsAtAll()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems = [WieldedCaster()],
        };
        surface.MissingComponentSpellIds.Add(100u);
        var controller = new CombatController(
            new FakeHost(surface),
            FireAttackRule(new CombatSettings { MaximumRange = 40d }));

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Empty(surface.CastSpellIds);
    }

    /// <summary>
    /// Mutation: classify from the numbers again (ammunition type, damage,
    /// weapon skill) and this fails — a thrown weapon takes no ammunition and
    /// an unarmed weapon lists no damage, so both would be mis-stanced.
    /// </summary>
    [Fact]
    public void AWeaponsStanceComesFromItsClassNotItsNumbers()
    {
        PluginEquipmentItem thrown = Equipment(
            1u,
            "Throwing Dagger",
            damageType: 0x0002,
            itemType: 0x100,
            ammoType: 0);
        PluginEquipmentItem fists = Equipment(
            2u,
            "Training Wraps",
            damageType: 0x0001,
            damage: 0,
            itemType: 0x1) with
        {
            WeaponSkill = 0,
        };

        Assert.Equal(PluginCombatMode.Missile, CombatModeGate.ModeFor(in thrown));
        Assert.Equal(PluginCombatMode.Melee, CombatModeGate.ModeFor(in fists));
    }

    /// <summary>
    /// Mutation: roll the random element inside the magic arm again and this
    /// fails — a pass that only casts a debuff would leave the cursor where it
    /// was, and the vulnerability would be for last pass's element.
    /// </summary>
    [Fact]
    public void ARolledElementAdvancesEvenOnAPassThatOnlyDebuffs()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                Debuff(70, "Piercing Vulnerability Other VII"),
                Debuff(71, "Bludgeoning Vulnerability Other VII"),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack
                    | MonsterActionFlags.Vulnerability,
                DamageType = MonsterDamageType.Random,
                ExtraVulnerability = MonsterDamageType.None,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(70u, surface.LastTargetedCast.Item1);

        // The first debuff is still in flight, so let it finish.
        surface.LastCastCompletion = new PluginCastCompletion(1, 70, 10, 0);
        controller.OnTick(0.25);
        controller.OnTick(0.25);

        Assert.Equal(71u, surface.LastTargetedCast.Item1);
    }

    /// <summary>
    /// Mutation: auto-select for a zero weapon column again and this fails —
    /// the rule means "no weapon, use a wand", and a melee weapon would be
    /// wielded instead.
    /// </summary>
    [Fact]
    public void AZeroWeaponColumnMeansAWandAndSelectsNothing()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems =
            [
                Equipment(
                    990u,
                    "Fixture Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
                Equipment(991u, "Fire Sword", damageType: 0x0010),
            ],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.CombatItemNames.Add("Fixture Wand");
        settings.CombatItemNames.Add("Fire Sword");
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
                WeaponToUseRaw = 0,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(990u, surface.LastEquipObjectId);
    }

    /// <summary>
    /// The first wand on the Items page wins, whatever it is called. The page
    /// puts the Zephyr Wand first even though the host hands the two of them
    /// back the other way round — the host's list is alphabetical among
    /// unheld items, so this only says anything at all because the two orders
    /// disagree.
    ///
    /// Mutation: drop the page walk from <c>FindFirstProfiledWand</c> and this
    /// fails — the Acid Wand comes back instead.
    /// </summary>
    [Fact]
    public void TheFirstProfiledWandWinsWithoutAnAlphabeticalTieBreak()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Peaceful(),
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [Debuff(83, "Imperil Other VII")],
            EquipmentItems =
            [
                Equipment(
                    990u,
                    "Zephyr Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
                Equipment(
                    991u,
                    "Acid Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
            ],
        };
        var settings = DebuffOnly(MonsterActionFlags.Imperil);
        settings.MaximumRange = 40d;
        settings.CombatItemNames.Add("Zephyr Wand");
        settings.CombatItemNames.Add("Acid Wand");
        settings.CombatItemOrder.Add("Zephyr Wand");
        settings.CombatItemOrder.Add("Acid Wand");
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal(990u, surface.LastEquipObjectId);
    }

    /// <summary>
    /// Mutation: walk the tiers for a rolled element and this fails — the
    /// rolled arm names its spell outright, and what it names is the first
    /// rung of the family.
    /// </summary>
    [Fact]
    public void ARolledElementThrowsTheFirstRungOfItsWarFamily()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical() with { Mode = PluginCombatMode.Magic },
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells =
            [
                MagicSpell(100, "Force Bolt I", difficulty: 50),
                MagicSpell(101, "Force Bolt VII", difficulty: 300),
            ],
            EquipmentItems = [WieldedCaster()],
        };
        var settings = new CombatSettings { MaximumRange = 40d };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Random,
            }));
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
    }

    /// <summary>
    /// Mutation: drop the swing teardown from the cast and this fails — a
    /// physical attack armed a moment ago would keep running underneath the
    /// spell.
    /// </summary>
    [Fact]
    public void ACastTearsDownAnArmedSwingFirst()
    {
        var surface = new FakeAutomation
        {
            CombatSnapshot = Physical(),
            Targets = [Target(10, "Drudge", 5, 0)],
            KnownCombatSpells = [MagicSpell(100, "Flame Bolt VII", difficulty: 300)],
            EquipmentItems =
            [
                Equipment(
                    990u,
                    "Fixture Wand",
                    damageType: 0,
                    itemType: CombatModeGate.CasterItemType),
                WieldedPlannedWeapon(),
            ],
        };
        var settings = FireAttackRule(new CombatSettings { MaximumRange = 40d });
        ProfileFixtureWeapon(settings);
        settings.CombatItemNames.Add("Fixture Wand");
        var controller = new CombatController(new FakeHost(surface), settings);

        controller.Toggle();
        controller.OnTick(0.25);
        Assert.Equal(1, surface.BeginCount);
        int abortsAfterTheSwing = surface.AbortCount;

        // The rule flips to magic; the swing must be torn down as the cast
        // goes out.
        surface.CombatSnapshot = surface.CombatSnapshot with
        {
            Mode = PluginCombatMode.Magic,
        };
        surface.EquipmentItems =
        [
            Equipment(
                990u,
                "Fixture Wand",
                damageType: 0,
                itemType: CombatModeGate.CasterItemType,
                equippedLocation: 0x00100000u),
        ];
        settings.Rules[0] = new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
                WeaponName = "Fixture Wand",
            });
        controller.OnTick(0.25);

        Assert.Equal((100u, 10u), surface.LastTargetedCast);
        Assert.True(surface.AbortCount > abortsAfterTheSwing);
    }

    private static CombatSettings FireAttackRule(CombatSettings settings)
    {
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                DamageType = MonsterDamageType.Fire,
            }));
        return settings;
    }

    /// <summary>
    /// Mutation: delete the <c>ArmPostKillNavigationLock()</c> call from the
    /// kill arm of the cast outcome and this fails — the bot walks off the
    /// corpse it just made instead of standing still for the looting window.
    /// </summary>
    [Fact]
    public void ASpellKillHoldsNavigationForThreeSecondsWhileLootingIsOn()
    {
        var tracker = new SpellCastTracker();
        var locks = new ActionLockTable();
        var controller = new CombatController(
            new FakeHost(new FakeAutomation()),
            new CombatSettings(),
            castTracker: tracker);
        controller.BindActionLocks(locks, () => true);

        tracker.Begin(
            1u,
            "Flame Bolt VII",
            30u,
            "Drudge",
            false,
            0L,
            school: SpellCastTracker.WarMagicSchool,
            canKill: true);
        tracker.ObserveChat(1uL, "You killed Drudge!");

        Assert.True(locks.IsLocked(ActionLockKind.Navigation));

        locks.Advance(2.9d);
        Assert.True(locks.IsLocked(ActionLockKind.Navigation));

        locks.Advance(0.2d);
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// Mutation: drop the looting term from <c>ArmPostKillNavigationLock</c>
    /// and this fails — a bot that never loots would stand still after a kill.
    /// </summary>
    [Fact]
    public void ASpellKillDoesNotHoldNavigationWhileLootingIsOff()
    {
        var tracker = new SpellCastTracker();
        var locks = new ActionLockTable();
        var controller = new CombatController(
            new FakeHost(new FakeAutomation()),
            new CombatSettings(),
            castTracker: tracker);
        controller.BindActionLocks(locks, () => false);

        tracker.Begin(
            1u,
            "Flame Bolt VII",
            30u,
            "Drudge",
            false,
            0L,
            school: SpellCastTracker.WarMagicSchool,
            canKill: true);
        tracker.ObserveChat(1uL, "You killed Drudge!");

        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
    }

    private static CombatSettings DebuffOnly(MonsterActionFlags flag)
    {
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions { Flags = flag }));
        return settings;
    }

    private static PluginCombatSnapshot Peaceful() => Physical() with
    {
        Mode = PluginCombatMode.Peace,
    };

    private static PluginCombatSnapshot Physical(
        bool request = false,
        bool build = false,
        float bar = 0f) => new(
            SelectedObjectId: 0,
            PluginCombatMode.Melee,
            PluginAttackHeight.Medium,
            DesiredPower: 0.5f,
            PowerBarLevel: bar,
            BuildInProgress: build,
            RequestInProgress: request,
            ServerResponsePending: false,
            RepeatAttackInProgress: false);

    /// <summary>
    /// A real excerpt of the owner's own <c>gameinfodb.ugd</c> — VTank's
    /// official GameInfoDB, which <c>e0</c> loads from the profile directory
    /// (<c>e0.cs:53-79</c>). Any pin whose subject is a monster's damage
    /// preferences needs one, because acdream ships no embedded default.
    /// </summary>
    private static readonly VtankGameInfoDatabase GameInfo =
        VtankGameInfoDatabase.Parse(
            File.ReadAllText(
                Path.Combine(
                    AppContext.BaseDirectory,
                    "Fixtures",
                    "vtank",
                    "gameinfodb-excerpt.ugd")));

    private static PluginCombatTarget Target(
        uint id, string name, float distance, float angle) => new(
            id, name, id + 1000, distance, angle, true, 1f);

    private static PluginSpellInfo Spell(uint id, string name) => new(
        id, name, Family: 1, Tier: 8, Difficulty: 350, ManaCost: 30,
        DurationSeconds: 0, School: 34, Description: string.Empty,
        IsSelfTargeted: false, IsBeneficial: false)
    {
        BaseRangeConstant = 80f,
    };

    /// <summary>The slot a quiver of ammunition goes in, not a weapon slot.</summary>
    private const uint AmmunitionSlot = 0x00800000u;

    private static PluginEquipmentItem Equipment(
        uint id,
        string name,
        int damageType,
        int damage = 20,
        uint equippedLocation = 0,
        uint itemType = 1,
        byte combatUse = 1,
        uint ammoType = 0,
        int stackSize = 1,
        uint validLocations = 0x00100000u) => new(
            id,
            name,
            ItemType: itemType,
            ValidLocations: validLocations,
            EquippedLocation: equippedLocation,
            ContainerObjectId: 1,
            WielderObjectId: 0,
            CombatUse: combatUse,
            DamageType: damageType,
            WeaponSkill: 44,
            Damage: damage,
            DamageVariance: 0.25)
        {
            AmmoType = ammoType,
            StackSize = stackSize,
        };

    private static PluginEquipmentItem WieldedCaster(uint id = 990u) =>
        Equipment(
            id,
            "Fixture Wand",
            damageType: 0,
            itemType: CombatModeGate.CasterItemType,
            equippedLocation: 0x00100000u);

    private static PluginEquipmentItem WieldedPlannedWeapon(uint id = 991u) =>
        Equipment(
            id,
            "Fixture Weapon",
            damageType: 0x007F,
            equippedLocation: 0x00100000u);

    private static CombatSettings ProfileFixtureWeapon(CombatSettings settings)
    {
        settings.CombatItemNames.Add("Fixture Weapon");
        return settings;
    }

    private static PluginInventoryItem PetDevice(uint id, uint wcid) => new(
        id, wcid, "Frost Pet", 0, 1, 0, 0, 0, 0, 0, 0, 1, 50, 50,
        0, 49000, 3, 0, false, 0, 0, 0, 0, 0, 54, 100, 0);

    private static PluginInventoryItem InventoryItem(
        uint id,
        string name,
        uint itemType,
        uint spellId,
        bool equipped) => new(
            id, 0, name, itemType, 1, 0, 0,
            equipped ? 0x00100000u : 0u,
            0, 0, 0, 1, 0, 0, spellId, 0, 0, 0, false, 0,
            0, 0, 0, 0, 0, 0, 0);

    /// <summary>A host with real storage, for profile round-trips.</summary>
    private sealed class StorageHost(
        IPluginStorage storage,
        string characterName,
        string worldName) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new FakeLogger();
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation { get; } = new FakeAutomation
        {
            CharacterName = characterName,
            World = worldName,
        };
        public IPluginStorage Storage => storage;
        public IPluginStorage VtankProfiles => storage;
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text =
            new(StringComparer.Ordinal);

        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            _text.TryGetValue(key, out string? value) ? value : null;
        public IReadOnlyList<string> List(string prefix) => _text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
        public void WriteText(string key, string content) => _text[key] = content;
        public bool Delete(string key) => _text.Remove(key);
    }

    private sealed class FakeHost(FakeAutomation automation) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new FakeLogger();
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation => automation;
    }

    private sealed class FakeAutomation :
        IAutomationSurface, ICharacterInfo, ISpellCatalog, IMagicCommands,
        IPluginChat, ICombatAutomation
        , IEquipmentAutomation, IItemAutomation, INavigationAutomation,
        IProjectileAutomation, ISelectionAutomation
    {
        public bool IsAvailable { get; set; } = true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;
        public ICombatAutomation Combat => this;
        public IEquipmentAutomation Equipment => this;
        public IItemAutomation Items => this;
        public INavigationAutomation Navigation => this;
        public IProjectileAutomation Projectiles => this;
        public ISelectionAutomation Selection => this;
        public PluginCombatSnapshot CombatSnapshot { get; set; }
        public PluginCombatSnapshot Snapshot => CombatSnapshot;
        public IReadOnlyList<PluginCombatTarget> Targets { get; set; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownAttackSpells { get; set; } = [];
        private IReadOnlyList<PluginSpellInfo> _knownCombatSpells = [];
        public int KnownCombatSpellReads { get; private set; }
        public IReadOnlyList<PluginSpellInfo> KnownCombatSpells
        {
            get
            {
                KnownCombatSpellReads++;
                return _knownCombatSpells;
            }
            set => _knownCombatSpells = value;
        }
        public IReadOnlyList<PluginSpellInfo> SpellLookup { get; set; } = [];
        public PluginCastCompletion LastCastCompletion { get; set; }
        public PluginCastCompletion LastCompletion => LastCastCompletion;
        public uint LastBeginTarget { get; private set; }
        public int BeginCount { get; private set; }
        public int ReleaseCount { get; private set; }
        public int AbortCount { get; private set; }
        public (uint Spell, uint Target) LastTargetedCast { get; private set; }
        public uint LastUntargetedCast { get; private set; }
        public List<uint> CastSpellIds { get; } = [];
        public IReadOnlyList<PluginEquipmentItem> EquipmentItems { get; set; } = [];
        public uint LastEquipObjectId { get; private set; }
        public IReadOnlyList<PluginInventoryItem> ItemEntries { get; set; } = [];
        public uint LastUsedItem { get; private set; }
        public (uint Item, uint Target) LastAppliedItem { get; private set; }
        public int ApplyCount { get; private set; }
        public PluginItemUseCompletion LastItemCompletion { get; set; }
        public IReadOnlyList<PluginChatMessage> ChatMessages { get; set; } = [];
        public float LastBeginPower { get; private set; }
        public PluginNavigationSnapshot NavigationSnapshot { get; set; }
        public bool IgnoreModeChanges { get; set; }
        public int ModeChangeRequests { get; private set; }
        PluginNavigationSnapshot INavigationAutomation.Snapshot =>
            NavigationSnapshot;
        public Dictionary<uint, PluginNavigationObject> NavigationObjects { get; } = [];
        public List<PluginMovementIntent> MovementIntents { get; } = [];
        public int ClearMovementCount { get; private set; }
        public PluginProjectilePathResult ProjectilePath { get; set; } =
            new(PluginProjectilePathStatus.Clear);
        public uint LastProjectileTarget { get; private set; }
        public PluginAttackHeight LastProjectileHeight { get; private set; }
        public PluginProjectilePathKind LastProjectileKind { get; private set; }
        public int ProjectilePathChecks { get; private set; }

        /// <summary>Per-target overrides for the flight-path check.</summary>
        public Dictionary<uint, PluginProjectilePathResult> ProjectilePaths
        { get; } = [];
        public IReadOnlyList<PluginProjectileDebugSample>
            ShownProjectileDebugSamples { get; private set; } = [];
        public List<PluginSelectionAction> SelectionActions { get; } = [];

        public List<string> CallLog { get; } = [];

        public bool SimulateAsyncEquip { get; set; }
        private uint? _pendingEquipObjectId;
        public bool EquipmentAvailable { get; set; } = true;
        bool IEquipmentAutomation.IsAvailable => EquipmentAvailable;
        bool IEquipmentAutomation.IsBusy =>
            SimulateAsyncEquip && _pendingEquipObjectId is not null;
        public int CaptureOwnedEquipmentCount { get; private set; }

        /// <summary>
        /// The host hands this list out held-first, then by name, then by
        /// object id — an order that turns over the moment something is
        /// wielded. A fixture that hands back its own insertion order instead
        /// cannot see anything that goes wrong because of that.
        /// </summary>
        public IReadOnlyList<PluginEquipmentItem> CaptureOwnedEquipment()
        {
            CaptureOwnedEquipmentCount++;
            var projected = new List<PluginEquipmentItem>(EquipmentItems);
            projected.Sort(static (left, right) =>
            {
                int equipped = right.IsEquipped.CompareTo(left.IsEquipped);
                if (equipped != 0)
                    return equipped;
                int name = string.CompareOrdinal(left.Name, right.Name);
                return name != 0
                    ? name
                    : left.ObjectId.CompareTo(right.ObjectId);
            });
            return projected;
        }
        public PluginEquipmentCommandResult Equip(
            uint objectId,
            uint requestedLocation = 0u)
        {
            LastEquipObjectId = objectId;
            CallLog.Add($"Equip:{objectId:X8}");
            if (SimulateAsyncEquip)
                _pendingEquipObjectId = objectId;
            else
                EquipmentItems = MarkEquipped(EquipmentItems, objectId);
            return new(PluginEquipmentCommandStatus.Started);
        }

        public void ConfirmPendingEquip()
        {
            if (_pendingEquipObjectId is not { } objectId)
                return;
            EquipmentItems = MarkEquipped(EquipmentItems, objectId);
            _pendingEquipObjectId = null;
        }

        /// <summary>
        /// A slot holds one thing: wielding this item puts it where it goes
        /// and takes whatever was already there out of the character's hands.
        /// A fixture that lets two items share the weapon slot hides every
        /// bug that turns on which of them the host calls the wielded one.
        /// </summary>
        private static IReadOnlyList<PluginEquipmentItem> MarkEquipped(
            IReadOnlyList<PluginEquipmentItem> items,
            uint objectId)
        {
            uint slot = 0u;
            foreach (PluginEquipmentItem item in items)
            {
                if (item.ObjectId == objectId)
                {
                    slot = item.ValidLocations;
                    break;
                }
            }
            return items
                .Select(item => item.ObjectId == objectId
                    ? item with { EquippedLocation = slot }
                    : (item.EquippedLocation & slot) != 0u
                        ? item with { EquippedLocation = 0u }
                        : item)
                .ToArray();
        }
        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => false;
        int IItemAutomation.ActiveOwnedPetCount => 0;
        PluginItemUseCompletion IItemAutomation.LastCompletion => LastItemCompletion;
        public int CaptureOwnedItemsCount { get; private set; }
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems()
        {
            CaptureOwnedItemsCount++;
            return ItemEntries;
        }
        public PluginItemCommandResult Use(uint objectId)
        {
            LastUsedItem = objectId;
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Apply(uint objectId, uint targetObjectId)
        {
            LastAppliedItem = (objectId, targetObjectId);
            ApplyCount++;
            return new(PluginItemCommandStatus.Started);
        }

        public IReadOnlyList<PluginCombatTarget> CaptureHostileTargets(
            float maximumDistance) => Targets;
        public PluginCombatCommandResult EnterDefaultMode()
        {
            CallLog.Add("EnterDefaultMode");
            return new(PluginCombatCommandStatus.ModeChangeSent);
        }

        public bool DeferModeConfirmation { get; set; }
        private PluginCombatMode? _pendingMode;
        public bool ModeCommandUnavailable { get; set; }

        public PluginCombatCommandResult EnterMode(PluginCombatMode mode)
        {
            ModeChangeRequests++;
            CallLog.Add($"EnterMode:{mode}");
            if (ModeCommandUnavailable)
                return new(PluginCombatCommandStatus.Unavailable);
            if (IgnoreModeChanges)
                return new(PluginCombatCommandStatus.ModeChangeSent);
            if (DeferModeConfirmation)
            {
                _pendingMode = mode;
                return new(PluginCombatCommandStatus.ModeChangeSent);
            }
            CombatSnapshot = CombatSnapshot with { Mode = mode };
            return new(PluginCombatCommandStatus.ModeChangeSent);
        }

        public void ConfirmPendingModeChange()
        {
            if (_pendingMode is not { } mode)
                return;
            CombatSnapshot = CombatSnapshot with { Mode = mode };
            _pendingMode = null;
        }
        public PluginCombatCommandResult BeginPhysicalAttack(
            uint targetObjectId, PluginAttackHeight height, float power)
        {
            LastBeginTarget = targetObjectId;
            LastBeginPower = power;
            BeginCount++;
            return new(PluginCombatCommandStatus.Started);
        }
        public PluginCombatCommandResult ReleasePhysicalAttack()
        {
            ReleaseCount++;
            return new(PluginCombatCommandStatus.Released);
        }
        public PluginCombatCommandResult AbortPhysicalAttack()
        {
            AbortCount++;
            return new(PluginCombatCommandStatus.Stopped);
        }

        public List<uint> DismissedGhosts { get; } = [];
        public bool GhostDismissalAccepted { get; set; } = true;
        public PluginCombatCommandResult DismissGhostTarget(uint targetObjectId)
        {
            DismissedGhosts.Add(targetObjectId);
            return new(GhostDismissalAccepted
                ? PluginCombatCommandStatus.Stopped
                : PluginCombatCommandStatus.Unavailable);
        }

        public bool TryGetObject(
            uint objectId,
            out PluginNavigationObject value) =>
            NavigationObjects.TryGetValue(objectId, out value);
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent)
        {
            MovementIntents.Add(intent);
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            ClearMovementCount++;
            return PluginNavigationCommandStatus.Accepted;
        }

        public List<float> FacedHeadings { get; } = [];

        public PluginNavigationCommandStatus FaceHeading(float headingDegrees)
        {
            FacedHeadings.Add(headingDegrees);
            return PluginNavigationCommandStatus.Accepted;
        }

        bool IProjectileAutomation.IsAvailable => true;
        public PluginProjectilePathResult EvaluatePath(
            uint targetObjectId,
            PluginProjectilePathKind kind,
            PluginAttackHeight targetHeight,
            float projectileRadius,
            float stepDistance,
            int maximumCollisionChecks)
        {
            LastProjectileTarget = targetObjectId;
            LastProjectileHeight = targetHeight;
            LastProjectileKind = kind;
            ProjectilePathChecks++;
            return ProjectilePaths.TryGetValue(
                targetObjectId,
                out PluginProjectilePathResult specific)
                ? specific
                : ProjectilePath;
        }

        public PluginProjectilePathResult EvaluatePathWithDiagnostics(
            uint targetObjectId,
            PluginProjectilePathKind kind,
            PluginAttackHeight targetHeight,
            float projectileRadius,
            float stepDistance,
            int maximumCollisionChecks) => EvaluatePath(
                targetObjectId,
                kind,
                targetHeight,
                projectileRadius,
                stepDistance,
                maximumCollisionChecks);

        public void ShowDebugSamples(
            IReadOnlyList<PluginProjectileDebugSample> samples) =>
            ShownProjectileDebugSamples = samples.ToArray();

        public bool Execute(PluginSelectionAction action)
        {
            SelectionActions.Add(action);
            return true;
        }

        public bool IsCasting { get; set; }

        /// <summary>
        /// Spell ids the pack cannot pay for. Everything else has components.
        /// </summary>
        public HashSet<uint> MissingComponentSpellIds { get; } = [];

        public bool HasComponents(uint spellId) =>
            !MissingComponentSpellIds.Contains(spellId);

        /// <summary>Spells the client would refuse to start right now.</summary>
        public Dictionary<uint, PluginCastGate> CastGates { get; } = [];

        public PluginCastGate EvaluateGate(uint spellId) =>
            CastGates.TryGetValue(spellId, out PluginCastGate gate)
                ? gate
                : PluginCastGate.Ready;
        public PluginCastGate EvaluateGate(uint spellId, uint targetObjectId) =>
            EvaluateGate(spellId);
        public bool Cast(uint spellId)
        {
            LastUntargetedCast = spellId;
            CastSpellIds.Add(spellId);
            return true;
        }
        public bool Cast(uint spellId, uint targetObjectId)
        {
            LastTargetedCast = (spellId, targetObjectId);
            CastSpellIds.Add(spellId);
            return true;
        }
        public List<string> PostedSystemMessages { get; } = [];
        public void PostSystemMessage(string text) =>
            PostedSystemMessages.Add(text);
        public IReadOnlyList<PluginChatMessage> CaptureMessages(
            ulong afterSequence) => ChatMessages
                .Where(message => message.Sequence > afterSequence)
                .ToArray();

        public bool IsInWorld => IsAvailable;
        public string CharacterName { get; init; } = "Fixture";
        string ICharacterInfo.Name => CharacterName;
        public string World { get; init; } = "FixtureWorld";
        string ICharacterInfo.WorldName => World;
        public uint ObjectId => 1;
        public uint CurrentHealth => 100;
        public uint MaxHealth => 100;
        public uint CurrentStamina => 100;
        public uint MaxStamina => 100;
        public uint CurrentMana => 100;
        public uint MaxMana => 100;
        public int SummoningMastery => 3;
        public IReadOnlyList<PluginSkillInfo> CharacterSkills { get; set; } =
            [new(54, "Summoning", PluginSkillTraining.Trained, 300)];
        public IReadOnlyList<PluginSkillInfo> Skills => CharacterSkills;
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in CharacterSkills)
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
        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            foreach (PluginSpellInfo spell in KnownAttackSpells)
            {
                if (spell.SpellId == spellId)
                {
                    info = spell;
                    return true;
                }
            }
            foreach (PluginSpellInfo spell in KnownCombatSpells.Concat(SpellLookup))
            {
                if (spell.SpellId == spellId)
                {
                    info = spell;
                    return true;
                }
            }
            info = default;
            return false;
        }
    }

    private static PluginNavigationSnapshot NavigationAt(float heading) => new(
        IsAvailable: true,
        IsPortalSpace: false,
        LocalObjectId: 1u,
        Position: new PluginNavigationPosition(
            0x7F7F0001,
            0d,
            0d,
            0d,
            heading,
            true),
        IsMoving: false,
        IsAirborne: false);

    private sealed class FakeLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }
    private sealed class FakeState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }
    private sealed class FakeEvents : IEvents
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
    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId) => true;
        public bool Clear() => true;
    }
}
