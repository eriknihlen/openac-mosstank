using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VitalRechargeTests
{
    [Fact]
    public void RetailDefaultsAreExactNineThresholds()
    {
        var settings = new VitalSettings();

        Assert.Equal(0.75, settings.NormalHealth);
        Assert.Equal(0.50, settings.NormalStamina);
        Assert.Equal(0.50, settings.NormalMana);
        Assert.Equal(0.01, settings.NoTargetHealth);
        Assert.Equal(0.01, settings.NoTargetStamina);
        Assert.Equal(0.01, settings.NoTargetMana);
        Assert.Equal(0.20, settings.HelperHealth);
        Assert.Equal(0.01, settings.HelperStamina);
        Assert.Equal(0.01, settings.HelperMana);
    }

    [Fact]
    public void NoTargetTopoffExtendsRatherThanReplacesCombatThreshold()
    {
        var settings = new VitalSettings { NoTargetHealth = 0.90 };
        var surface = new Surface { CurrentHealth = 80 };

        Assert.Null(VitalPlan.DecideNeed(surface, settings, noTarget: false));
        Assert.Equal(
            VitalKind.Health,
            VitalPlan.DecideNeed(surface, settings, noTarget: true));
    }

    [Fact]
    public void MagicHealthHandlerOrderMatchesOfficialDefaultTable()
    {
        Assert.Equal(
            [
                VitalRechargeMethod.StaminaToHealth,
                VitalRechargeMethod.ManaToHealth,
                VitalRechargeMethod.RegularSpell,
                VitalRechargeMethod.Food,
                VitalRechargeMethod.Kit,
            ],
            VitalRechargePlanner.Handlers(VitalKind.Health, true, 15));
        Assert.Equal(
            [
                VitalRechargeMethod.Kit,
                VitalRechargeMethod.StaminaToHealth,
                VitalRechargeMethod.ManaToHealth,
                VitalRechargeMethod.RegularSpell,
                VitalRechargeMethod.Food,
            ],
            VitalRechargePlanner.Handlers(VitalKind.Health, true, 16));
    }

    [Fact]
    public void NonMagicHealthConversionExistsOnlyInOfficialEmergencyBand()
    {
        Assert.Equal(
            [
                VitalRechargeMethod.Food,
                VitalRechargeMethod.Kit,
                VitalRechargeMethod.StaminaToHealth,
                VitalRechargeMethod.RegularSpell,
            ],
            VitalRechargePlanner.Handlers(VitalKind.Health, false, 10));
        Assert.Equal(
            [
                VitalRechargeMethod.Food,
                VitalRechargeMethod.Kit,
                VitalRechargeMethod.RegularSpell,
            ],
            VitalRechargePlanner.Handlers(VitalKind.Health, false, 15));
        Assert.Equal(
            [
                VitalRechargeMethod.Kit,
                VitalRechargeMethod.Food,
                VitalRechargeMethod.RegularSpell,
            ],
            VitalRechargePlanner.Handlers(VitalKind.Health, false, 16));
    }

    [Fact]
    public void RechargeHandlerSetCanOverrideOneStanceVitalBand()
    {
        const string profile =
            "combat-health-critical=Regular Spell > Kit Recharge; "
            + "magic-mana-normal=Recharge With Food > Regular Spell";

        Assert.Equal(
            [VitalRechargeMethod.RegularSpell, VitalRechargeMethod.Kit],
            VitalRechargePlanner.Handlers(
                VitalKind.Health,
                false,
                5,
                profile));
        Assert.Equal(
            [VitalRechargeMethod.Food, VitalRechargeMethod.RegularSpell],
            VitalRechargePlanner.Handlers(
                VitalKind.Mana,
                true,
                50,
                profile));
    }

    [Fact]
    public void RechargeBoostAdjustmentTemporarilyRaisesNeed()
    {
        var surface = new Surface { CurrentHealth = 80 };
        var settings = new VitalSettings { NormalHealth = 0.75 };

        Assert.Null(VitalPlan.DecideNeed(surface, settings, noTarget: false));
        Assert.Equal(
            VitalKind.Health,
            VitalPlan.DecideNeed(
                surface,
                settings,
                noTarget: false,
                healthCurrentAdjustment: 40));
    }

    [Fact]
    public void IncantationHealthAliasParticipatesInRegularSpellHandler()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            CurrentHealth = 50,
            Spells =
            [
                Spell(
                    (uint)SpellId.AdjaSIntervention,
                    "Adja's Intervention",
                    1u,
                    400),
            ],
        };

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings { MinimumHealKitSuccessChance = 100 },
            new CombatSettings(),
            out VitalRechargeChoice choice));
        Assert.Equal((uint)SpellId.AdjaSIntervention, choice.SpellId);
    }

    /// <summary>
    /// The reference never appraises a kit: its kind is the profile's and its
    /// bonuses come from the game-info table by name. An unappraised kit the
    /// profile calls a health kit is used. Mutation: require the appraisal
    /// again and the plan finds nothing.
    /// </summary>
    [Fact]
    public void AnUnappraisedKitIsUsedByItsProfileKindAndTheTable()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Peace,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Item(10u, "Plentiful Healing Kit")],
        };
        surface.Unassessed.Add(10u);
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");
        combat.ConsumableCategories["Plentiful Healing Kit"] = ConsumableCategory.HealthKit;
        combat.HealKits = new Dictionary<string, VtankHealKit>(StringComparer.OrdinalIgnoreCase)
        {
            ["Plentiful Healing Kit"] = new("Plentiful Healing Kit", 1.2, 0, 2),
        };

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings(),
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.Kit, choice.SourceKind);
        Assert.Equal(10u, choice.ItemObjectId);
    }

    /// <summary>Food likewise: the profile's kind is enough.</summary>
    [Fact]
    public void AnUnappraisedFoodItemIsUsedByItsProfileKind()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Peace,
            CurrentHealth = 50,
            Items = [Item(11u, "Bread")],
        };
        surface.Unassessed.Add(11u);
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        combat.ConsumableCategories["Bread"] = ConsumableCategory.HealthFood;

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings(),
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.Food, choice.SourceKind);
        Assert.Equal(11u, choice.ItemObjectId);
    }

    /// <summary>
    /// Mutation <c>SkipAssistItemsApply</c>: do not apply loaded AssistItems;
    /// neither imported row reaches the existing recharge selector.
    /// </summary>
    [Fact]
    public void ImportedAssistKindsReachKitAndFoodRechargeSelection()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = database.Find("AssistItems")!;
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Plentiful Healing Kit"), VtankCell.Int(0) } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Bread"), VtankCell.Int(1) } });
        var combat = new CombatSettings
        {
            HealKits = new Dictionary<string, VtankHealKit>(StringComparer.OrdinalIgnoreCase)
            {
                ["Plentiful Healing Kit"] = new("Plentiful Healing Kit", 1.2, 0, 2),
            },
        };
        VtankSettingsProfileSerializer.Load(database.Render(),
            new VtankSettingsProfileSerializer.AllSettings
            {
                Combat = combat, Buffs = new BuffSettings(), Vitals = new VitalSettings(),
                Inventory = new InventorySettings(), Navigation = new NavigationSettings(),
            });
        VtankAssistItems.Apply(combat);

        var kitSurface = new Surface
        {
            Mode = PluginCombatMode.Peace,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Item(10u, "Plentiful Healing Kit")],
        };
        kitSurface.Unassessed.Add(10u);
        Assert.True(VitalRechargePlanner.TryPlan(VitalKind.Health, kitSurface,
            new VitalSettings(), combat, out VitalRechargeChoice kit));
        Assert.Equal(VitalRechargeSourceKind.Kit, kit.SourceKind);
        Assert.Equal(10u, kit.ItemObjectId);

        var foodSurface = new Surface
        {
            Mode = PluginCombatMode.Peace,
            CurrentHealth = 50,
            Items = [Item(11u, "Bread")],
        };
        foodSurface.Unassessed.Add(11u);
        Assert.True(VitalRechargePlanner.TryPlan(VitalKind.Health, foodSurface,
            new VitalSettings(), combat, out VitalRechargeChoice food));
        Assert.Equal(VitalRechargeSourceKind.Food, food.SourceKind);
        Assert.Equal(11u, food.ItemObjectId);
    }
    [Fact]
    public void MagicModeUsesProfiledViableKitBeforeRegularHealAboveEmergencyBand()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Kit(10u, "Plentiful Healing Kit", booster: 2)],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");
        surface.Spells = [Spell(100u, "Heal Self VII", family: 1u, quality: 300)];

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings(),
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.Kit, choice.SourceKind);
        Assert.Equal(10u, choice.ItemObjectId);
    }

    [Fact]
    public void EmergencyMagicHealthPrefersStaminaConversionBeforeKit()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            CurrentHealth = 10,
            Skills = [Skill(21u, 400u), Skill(33u, 400u)],
            Items = [Kit(10u, "Plentiful Healing Kit", booster: 2)],
            Spells =
            [
                Spell(101u, "Stamina to Health Self VII", 5u, 300),
                Spell(102u, "Heal Self VII", 1u, 300),
            ],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings(),
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.LearnedSpell, choice.SourceKind);
        Assert.Equal(101u, choice.SpellId);
    }

    [Fact]
    public void DirectLearnedSpellWinsFinalQualityTieAgainstCasterItem()
    {
        PluginSpellInfo learned = Spell(101u, "Heal Self VII", 1u, 300);
        PluginSpellInfo itemSpell = Spell(102u, "Heal Self VII", 1u, 300);
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            CurrentHealth = 50,
            Spells = [learned],
            Lookup = [itemSpell],
            Items = [Caster(20u, "Healing Lens", itemSpell.SpellId)],
        };
        var combat = new CombatSettings();
        combat.CombatItemNames.Add("Healing Lens");

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings { MinimumHealKitSuccessChance = 100 },
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.LearnedSpell, choice.SourceKind);
        Assert.Equal(learned.SpellId, choice.SpellId);
    }

    [Fact]
    public void HelperChoosesLowestInRangeFellowAndStrongestFamilySpell()
    {
        PluginSpellInfo basis = Spell(
            (uint)SpellId.AdjaSGift,
            "Adja's Gift",
            900u,
            100);
        PluginSpellInfo strong = Spell(300u, "Adja's Grace", 900u, 350);
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [strong],
            Lookup = [basis],
            InFellowship = true,
            Fellows =
            [
                Fellow(70u, "Near", health: 19, distance: 10f),
                Fellow(71u, "Lowest", health: 5, distance: 20f),
                Fellow(72u, "Out of range", health: 1, distance: 100f),
            ],
        };

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface,
            new VitalSettings(),
            out VitalRechargeChoice choice));
        Assert.Equal(VitalKind.Health, choice.Vital);
        Assert.Equal(71u, choice.TargetObjectId);
        Assert.Equal(strong.SpellId, choice.SpellId);
    }

    /// <summary>
    /// One family holds both the Self and the Other line (Revitalize Self
    /// and Revitalize Other share family 81); the reference is the Other
    /// spell and only the component set keeps the pick on its line.
    /// Mutation: drop the component-set term from the tier accept and the
    /// higher-quality Self spell wins — and is cast "at" the fellow.
    /// </summary>
    [Fact]
    public void HelperSpellStaysOnTheReferencesOwnLine()
    {
        var yew = new PluginSpellComponentSet(7u, 26u, 39u, 51u);
        var willow = new PluginSpellComponentSet(7u, 26u, 39u, 61u);
        PluginSpellInfo replenish = Spell(
            (uint)SpellId.Replenish, "Replenish", 81u, 300)
            with { ComponentSet = yew, IsSelfTargeted = false };
        PluginSpellInfo revitalizeSelf = Spell(
            0x10E1u, "Incantation of Revitalize Self", 81u, 400)
            with { ComponentSet = willow, Tier = 8 };
        PluginSpellInfo revitalizeOther = Spell(
            0x04A4u, "Revitalize Other VI", 81u, 250)
            with { ComponentSet = yew, Tier = 6, IsSelfTargeted = false };
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [revitalizeSelf, revitalizeOther],
            Lookup = [replenish],
            InFellowship = true,
            Skills = [Skill(33u, 500u)],
            Fellows =
            [
                Fellow(71u, "Winded", health: 100, distance: 10f)
                    with { CurrentStamina = 5u },
            ],
        };

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface,
            new VitalSettings { HelperStamina = 0.5 },
            out VitalRechargeChoice choice));
        Assert.Equal(VitalKind.Stamina, choice.Vital);
        Assert.Equal(revitalizeOther.SpellId, choice.SpellId);
        Assert.Equal(71u, choice.TargetObjectId);
    }

    /// <summary>
    /// The helper's tier walk is only as gateable as its trace: a connected
    /// run has to show which tiers were skipped and why, the way the buff
    /// pass already does. Mutation: drop the trace pass-through and the
    /// sink stays empty while the walk silently settles for gen 6.
    /// </summary>
    [Fact]
    public void HelperTierWalkTracesItsPickAndRejections()
    {
        var yew = new PluginSpellComponentSet(7u, 26u, 39u, 51u);
        var willow = new PluginSpellComponentSet(7u, 26u, 39u, 61u);
        PluginSpellInfo replenish = Spell(
            (uint)SpellId.Replenish, "Replenish", 81u, 300)
            with { ComponentSet = yew, IsSelfTargeted = false };
        PluginSpellInfo revitalizeSelf = Spell(
            0x10E1u, "Incantation of Revitalize Self", 81u, 400)
            with { ComponentSet = willow, Tier = 8 };
        PluginSpellInfo revitalizeOther8 = Spell(
            0x10E0u, "Incantation of Revitalize Other", 81u, 400)
            with { ComponentSet = yew, Tier = 8, IsSelfTargeted = false };
        PluginSpellInfo revitalizeOther6 = Spell(
            0x04A4u, "Revitalize Other VI", 81u, 250)
            with { ComponentSet = yew, Tier = 6, IsSelfTargeted = false };
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [revitalizeSelf, revitalizeOther8, revitalizeOther6],
            Lookup = [replenish],
            InFellowship = true,
            Skills = [Skill(33u, 300u)],
            Fellows =
            [
                Fellow(71u, "Winded", health: 100, distance: 10f)
                    with { CurrentStamina = 5u },
            ],
        };
        var trace = new List<string>();

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface,
            new VitalSettings { HelperStamina = 0.5 },
            new CombatSettings(),
            trace.Add,
            out VitalRechargeChoice choice));

        Assert.Equal(revitalizeOther6.SpellId, choice.SpellId);
        string line = Assert.Single(trace);
        Assert.StartsWith("Helping: Replenish", line, StringComparison.Ordinal);
        Assert.Contains("picked Revitalize Other VI (gen 6)", line);
        Assert.Contains("Incantation of Revitalize Other skill 300 < 425", line);
        Assert.Contains("Incantation of Revitalize Self comp set", line);
    }

    /// <summary>
    /// A fellow's vitals are only as good as the server's last stream of
    /// them: none yet, or older than the trust window, means unknown.
    /// Mutation: drop the age gate and the stale 5 %-health fellow is healed
    /// forever.
    /// </summary>
    [Fact]
    public void HelperTrustsOnlyFreshlyStreamedVitals()
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [Spell(300u, "Adja's Grace", 900u, 100)],
            Lookup = [Spell((uint)SpellId.AdjaSGift, "Adja's Gift", 900u, 100)],
            InFellowship = true,
            Skills = [Skill(33u, 400u)],
            Fellows =
            [
                Fellow(70u, "Stale", health: 5, distance: 10f)
                    with { VitalsAgeSeconds = VitalRechargePlanner.FellowVitalsTrustSeconds },
                Fellow(71u, "Never", health: 5, distance: 10f)
                    with { VitalsAgeSeconds = null },
                Fellow(72u, "Fresh", health: 15, distance: 10f)
                    with { VitalsAgeSeconds = 2d },
            ],
        };

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface,
            new VitalSettings(),
            out VitalRechargeChoice choice));
        Assert.Equal(72u, choice.TargetObjectId);
    }

    /// <summary>
    /// The helper controller holds the host's vitals subscription exactly
    /// while it is helping others, so the stream flows without the
    /// fellowship panel; the self-recharge controller never touches it.
    /// Mutation: delete the sync and no request is ever made.
    /// </summary>
    [Fact]
    public void HelperControllerHoldsTheVitalsSubscriptionWhileHelpingOthers()
    {
        var surface = new Surface { InFellowship = true };
        var settings = new VitalSettings { HelpOthers = true };
        var helper = new VitalRechargeController(
            new Host(surface), settings, new CombatSettings());
        var self = new VitalRechargeController(
            new Host(surface), settings, new CombatSettings());

        self.Tick(0.3d, enabled: true, noTarget: true, helpers: false);
        Assert.Empty(surface.VitalsRequests);

        helper.Tick(0.3d, enabled: true, noTarget: true, helpers: true);
        helper.Tick(0.3d, enabled: true, noTarget: true, helpers: true);
        Assert.Equal([true], surface.VitalsRequests);

        settings.HelpOthers = false;
        helper.Tick(0.3d, enabled: true, noTarget: true, helpers: true);
        Assert.Equal([true, false], surface.VitalsRequests);

        settings.HelpOthers = true;
        helper.Tick(0.3d, enabled: true, noTarget: true, helpers: true);
        helper.Tick(0.3d, enabled: false, noTarget: true, helpers: true);
        Assert.Equal([true, false, true, false], surface.VitalsRequests);
    }

    /// <summary>
    /// Eating, drinking or applying a kit holds the shared item slot for as
    /// long as the macro waits on the server, and drops it the moment the
    /// transaction ends. The attack rule refuses while that slot is up, which
    /// is what keeps a swing out of the kit's own animation.
    /// Mutation: delete the <c>Arm(ActionLockKind.ItemUse, ...)</c> beside the
    /// new pending and the first assertion fails; delete
    /// <c>ReleaseItemUse()</c> and the last one does.
    /// </summary>
    [Fact]
    public void AnItemRechargeHoldsTheItemSlotUntilTheServerAnswers()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var locks = new ActionLockTable();
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(locks);

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);

        Assert.Equal([10u], surface.UsedItemIds);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        locks.Advance(1d);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        surface.LastItemCompletion = new PluginItemUseCompletion(1L, 10u, 0u, 0u);
        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);

        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    /// <summary>
    /// A use the server never answered is watched to its end, but it stops
    /// holding the character when the window it armed runs out: waiting out
    /// a lost answer is this owner's business, not everybody else's. Left
    /// claiming the pass, a single unanswered use kept every rule below it --
    /// the looter among them -- off the character for the whole watchdog.
    /// Mutation: return true unconditionally while something is pending and
    /// the second pass is claimed again.
    /// </summary>
    [Fact]
    public void AnUnansweredUseStopsClaimingThePassWhenItsWindowRunsOut()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var locks = new ActionLockTable();
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(
            0.3d, enabled: true, noTarget: false, helpers: false));
        Assert.Equal([10u], surface.UsedItemIds);

        // Still inside the window: the use owns the character.
        locks.Advance(1d);
        Assert.True(controller.Tick(
            0.3d, enabled: true, noTarget: false, helpers: false));

        // Past it, with no answer: the slot is free and so is the pass.
        locks.Advance(1d);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.False(controller.Tick(
            0.3d, enabled: true, noTarget: false, helpers: false));
        Assert.Equal([10u], surface.UsedItemIds);
    }

    /// <summary>
    /// The reference's two self-recharge rows read one named setting each:
    /// the normal thresholds up the list, the no-target thresholds far
    /// below loot and the monster approach. Merging the two sets lets the
    /// upper row claim at the lower row's threshold, after which the lower
    /// row can never fire at all. Mutation: return
    /// <c>Math.Max(NormalHealth, NoTargetHealth)</c> from
    /// <c>VitalPlan.Threshold</c> again and the lower row uses a kit at a
    /// vital only the upper row's threshold covers.
    /// </summary>
    [Fact]
    public void TheNoTargetRowReadsItsOwnThresholdAndNotTheNormalOne()
    {
        // Half health: below the normal threshold (75%), far above the
        // no-target one (1%).
        var surface = new Surface
        {
            CurrentHealth = 50,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(new ActionLockTable());

        controller.Tick(0.3d, enabled: true, noTarget: true, helpers: false);
        Assert.Empty(surface.UsedItemIds);

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.Equal([10u], surface.UsedItemIds);
    }

    /// <summary>
    /// A kit or food item the server has yet to answer for is what the
    /// reference's kit sequencer raises the global busy count for; the
    /// controller reports it for exactly that long. Mutation: make
    /// <c>ItemUseInFlight</c> answer false and the first assertion fails.
    /// </summary>
    [Fact]
    public void AKitOrFoodUseIsReportedInFlightUntilTheServerAnswers()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(new ActionLockTable());
        Assert.False(controller.ItemUseInFlight);

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.Equal([10u], surface.UsedItemIds);
        Assert.True(controller.ItemUseInFlight);

        surface.LastItemCompletion = new PluginItemUseCompletion(1L, 10u, 0u, 0u);
        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.False(controller.ItemUseInFlight);
    }

    /// <summary>
    /// A use in flight holds the macro pass, and the pass is the only thing
    /// that asks this controller anything — so if the pass were also the only
    /// place the server's answer were read, the hold would be waiting on the
    /// very thing it had stopped and could end only on its watchdog, many
    /// seconds after the character had already drunk the elixir. The answer
    /// is read on the host frame instead, with no turn of any kind.
    /// Mutation: make <c>ObservePendingReceipt</c> return without observing
    /// and the second assertion fails — the use stays in flight forever.
    /// </summary>
    [Fact]
    public void TheServersAnswerIsReadOnTheFrameWithoutAPass()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(new ActionLockTable());

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.True(controller.ItemUseInFlight);

        // No Tick at all from here: the pass is held, only frames run.
        surface.LastItemCompletion = new PluginItemUseCompletion(1L, 10u, 0u, 0u);
        controller.ObservePendingReceipt(0.05d);

        Assert.False(controller.ItemUseInFlight);
    }

    /// <summary>
    /// Receipts are read on every frame, beside every other owner that has
    /// one outstanding, so a receipt on its own says nothing about whose it
    /// is: a door the walk opened or an item the mode gate used raises the
    /// same stamp. Taking one of those for the answer would let go of the
    /// item slot while the character still had the elixir in hand, and the
    /// next turn would drink a second one.
    ///
    /// Mutation: complete on any newer stamp, as before, and the foreign
    /// receipt ends the wait.
    /// </summary>
    [Fact]
    public void AReceiptForSomebodyElsesUseDoesNotEndThisOne()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(new ActionLockTable());

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.True(controller.ItemUseInFlight);

        // Somebody else's item answers first.
        surface.LastItemCompletion = new PluginItemUseCompletion(1L, 777u, 0u, 0u);
        controller.ObservePendingReceipt(0.05d);
        Assert.True(controller.ItemUseInFlight);

        // And then this one's.
        surface.LastItemCompletion = new PluginItemUseCompletion(2L, 10u, 0u, 0u);
        controller.ObservePendingReceipt(0.05d);
        Assert.False(controller.ItemUseInFlight);
    }

    /// <summary>
    /// The frame and the turn share one clock for the give-up timer: time the
    /// frame has already watched off is not charged again when the turn comes
    /// back. Mutation: drop the <c>_frameObservedSeconds</c> subtraction in
    /// <c>Tick</c> and the use is given up on the first turn instead.
    /// </summary>
    [Fact]
    public void TheFrameAndTheTurnDoNotBothChargeTheSameSecondsToTheTimeout()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(new ActionLockTable());

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.True(controller.ItemUseInFlight);

        // Eight seconds of frames, then the turn is handed the same eight.
        for (int frame = 0; frame < 80; frame++)
            controller.ObservePendingReceipt(0.1d);
        controller.Tick(8d, enabled: true, noTarget: false, helpers: false);

        // Sixteen seconds would have been past the give-up point; eight is not.
        Assert.True(controller.ItemUseInFlight);
        Assert.DoesNotContain("Timed out", controller.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// The reference rule is valid whenever a vital is below its threshold,
    /// whatever it then finds to use: a vital with no answer holds the pass
    /// (with a warning), it does not yield it. Mutation: make the "no
    /// recharge available" path return false and the assertion fails.
    /// </summary>
    [Fact]
    public void AVitalBelowThresholdWithNothingToUseStillHoldsThePass()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
        };
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            new CombatSettings());
        controller.BindActionLocks(new ActionLockTable());

        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));
        Assert.Contains("No Health recharge available", controller.Status);
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));
    }

    /// <summary>
    /// A pass the rule does not win is not a reason to abandon an item the
    /// server has yet to answer for: the transaction keeps its slot and keeps
    /// waiting, so it cannot drop a window somebody else is holding and it
    /// cannot restart the same use over and over.
    /// Mutation: put <c>!enabled ||</c> back in front of
    /// <c>!_settings.Enabled</c> in <c>Tick</c>'s stand-down test and the
    /// second and third assertions fail — the losing tick drops the slot and
    /// the next winning tick eats a second piece of bread.
    /// </summary>
    [Fact]
    public void ALosingTickKeepsTheItemTheServerHasYetToAnswerFor()
    {
        var surface = new Surface
        {
            CurrentHealth = 20,
            MaxHealth = 100,
            Items = [Food(10u, "Bread")],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Bread");
        var locks = new ActionLockTable();
        var controller = new VitalRechargeController(
            new Host(surface),
            new VitalSettings(),
            combat);
        controller.BindActionLocks(locks);

        controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false);
        Assert.Equal([10u], surface.UsedItemIds);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        // Two passes some other rule won.
        controller.Tick(0.3d, enabled: false, noTarget: false, helpers: false);
        controller.Tick(0.3d, enabled: false, noTarget: false, helpers: false);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.Equal([10u], surface.UsedItemIds);

        surface.LastItemCompletion = new PluginItemUseCompletion(1L, 10u, 0u, 0u);
        controller.Tick(0.3d, enabled: false, noTarget: false, helpers: false);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    [Fact]
    public void HealKitChanceUsesRetailLogisticDifficultyFormula()
    {
        var surface = new Surface { CurrentHealth = 50 };
        double chance = VitalRechargePlanner.HealKitChance(
            90u,
            10,
            surface,
            VitalKind.Health,
            PluginCombatMode.Peace);

        Assert.Equal(0.5d, chance, precision: 10);
    }

    [Fact]
    public void TheHealersHeartMustBeInTheItemsProfileBeforeItIsUsed()
    {
        Surface surface = HealersHeartSurface();
        var settings = new VitalSettings { UseHealersHeart = true };

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface, settings, new CombatSettings(), out VitalRechargeChoice bare));
        Assert.Equal(VitalRechargeSourceKind.LearnedSpell, bare.SourceKind);

        var profiled = new CombatSettings();
        profiled.CombatItemNames.Add("The Healer's Heart");
        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface, settings, profiled, out VitalRechargeChoice used));
        Assert.Equal(VitalRechargeSourceKind.CasterItem, used.SourceKind);
        Assert.Equal("The Healer's Heart", used.Name);
        Assert.Equal(71u, used.TargetObjectId);
    }

    /// <summary>
    /// "The Healer's Heart" takes rank 1 and "Legendary
    /// Seed of Mornings" rank 2, and each arm only overwrites when
    /// <c>num &lt; rank</c>, so the Seed wins whichever order the scan meets
    /// them in.
    /// Mutation: swap the two ranks and this fails.
    /// </summary>
    [Fact]
    public void TheLegendarySeedOutranksTheHealersHeart()
    {
        Surface surface = HealersHeartSurface();
        surface.Items =
        [
            .. surface.Items,
            Item(31u, "Legendary Seed of Mornings"),
        ];
        var profiled = new CombatSettings();
        profiled.CombatItemNames.Add("The Healer's Heart");
        profiled.CombatItemNames.Add("Legendary Seed of Mornings");

        Assert.True(VitalRechargePlanner.TryPlanHelper(
            surface,
            new VitalSettings { UseHealersHeart = true },
            profiled,
            out VitalRechargeChoice choice));
        Assert.Equal("Legendary Seed of Mornings", choice.Name);
    }

    [Fact]
    public void TheHealersHeartNeedsLifeMagic245AndArcaneLore105()
    {
        var profiled = new CombatSettings();
        profiled.CombatItemNames.Add("The Healer's Heart");
        var settings = new VitalSettings { UseHealersHeart = true };

        Surface shortOnLife = HealersHeartSurface();
        shortOnLife.Skills = [Skill(33u, 244u), Skill(14u, 400u)];
        Assert.True(VitalRechargePlanner.TryPlanHelper(
            shortOnLife, settings, profiled, out VitalRechargeChoice noLife));
        Assert.Equal(VitalRechargeSourceKind.LearnedSpell, noLife.SourceKind);

        Surface shortOnLore = HealersHeartSurface();
        shortOnLore.Skills = [Skill(33u, 400u), Skill(14u, 104u)];
        Assert.True(VitalRechargePlanner.TryPlanHelper(
            shortOnLore, settings, profiled, out VitalRechargeChoice noLore));
        Assert.Equal(VitalRechargeSourceKind.LearnedSpell, noLore.SourceKind);

        Surface ready = HealersHeartSurface();
        Assert.True(VitalRechargePlanner.TryPlanHelper(
            ready, settings, profiled, out VitalRechargeChoice ok));
        Assert.Equal(VitalRechargeSourceKind.CasterItem, ok.SourceKind);
    }

    /// <summary>
    /// The reference gates the helper rows on the item-use slot, the slot a
    /// kit or a heart holds while its animation runs. It never gates them on
    /// the host's inventory transaction state, which is a different thing: a
    /// cast raises that on its way out, so a rule watching it blocks itself
    /// after its own casts. Mutation: gate the planner on
    /// <c>automation.Items.IsBusy</c> again and the first pick is the spell,
    /// not the heart.
    /// </summary>
    [Fact]
    public void TheItemSlotGatesTheHealersHeart_NotTheHostsTransactionFlag()
    {
        var profiled = new CombatSettings();
        profiled.CombatItemNames.Add("The Healer's Heart");
        var settings = new VitalSettings { UseHealersHeart = true };

        // The host flag is up; the heart is still the right pick.
        Surface busy = HealersHeartSurface();
        busy.ItemsBusy = true;
        Assert.True(VitalRechargePlanner.TryPlanHelper(
            busy, settings, profiled, out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.CasterItem, choice.SourceKind);

        // The item slot is what holds the row off.
        Surface ready = HealersHeartSurface();
        var controller = new VitalRechargeController(
            new Host(ready), settings, profiled);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);
        locks.Arm(ActionLockKind.ItemUse, 1d);

        Assert.False(controller.Tick(
            0.3d, enabled: true, noTarget: false, helpers: true));
        Assert.Empty(ready.UsedItemIds);
    }

    /// <summary>
    /// Kits in magic stance are a profile choice: with the option on, a kit is
    /// the plan while the character is mid-spell-stance; with it off there is
    /// no plan at all, and the character keeps casting rather than bandaging.
    ///
    /// Mutation: drop the stance test from the kit step and the second row
    /// plans the kit as well.
    /// </summary>
    [Theory]
    [InlineData(true, true)]
    [InlineData(false, false)]
    public void KitsInMagicStanceAreAProfileChoice(bool allowed, bool plans)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Kit(10u, "Plentiful Healing Kit", booster: 2)],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");

        Assert.Equal(
            plans,
            VitalRechargePlanner.TryPlan(
                VitalKind.Health,
                surface,
                new VitalSettings { UseKitsInMagicMode = allowed },
                combat,
                out VitalRechargeChoice choice));
        if (plans)
            Assert.Equal(VitalRechargeSourceKind.Kit, choice.SourceKind);
    }

    /// <summary>
    /// The same option never touches a stance the character is not in: in
    /// melee stance the kit is planned whichever way the option is set.
    ///
    /// Mutation: widen the stance test to every stance and the kit is refused
    /// with the option off.
    /// </summary>
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void KitsOutsideMagicStanceAreUnaffectedByThatChoice(bool allowed)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Melee,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Kit(10u, "Plentiful Healing Kit", booster: 2)],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings { UseKitsInMagicMode = allowed },
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.Kit, choice.SourceKind);
    }

    /// <summary>
    /// A kit can be asked for from any stance: with the option on the plan
    /// carries a stance requirement the recharge honours before it uses the
    /// kit, and with it off the kit is used from whatever stance the character
    /// is already in.
    ///
    /// Mutation: always attach the peace requirement, or never attach it, and
    /// one of the two rows fails.
    /// </summary>
    [Theory]
    [InlineData(true, PluginCombatMode.Peace)]
    [InlineData(false, null)]
    public void GoingToPeaceForAKitIsAProfileChoice(
        bool goToPeace,
        PluginCombatMode? required)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Melee,
            CurrentHealth = 50,
            Skills = [Skill(21u, 400u)],
            Items = [Kit(10u, "Plentiful Healing Kit", booster: 2)],
        };
        var combat = new CombatSettings();
        combat.ConsumableNames.Add("Plentiful Healing Kit");

        Assert.True(VitalRechargePlanner.TryPlan(
            VitalKind.Health,
            surface,
            new VitalSettings { GoToPeaceModeToUseKits = goToPeace },
            combat,
            out VitalRechargeChoice choice));
        Assert.Equal(VitalRechargeSourceKind.Kit, choice.SourceKind);
        Assert.Equal(required, choice.RequiredMode);
    }

    /// <summary>
    /// The boost is hysteresis, counted in vital points: while it is armed the
    /// recharge reads the character as that many points worse off than it is,
    /// so a recharge already under way is not abandoned the moment the vital
    /// creeps back over the threshold. Here the character is above the
    /// threshold with nothing left to use, and the boost is the only reason
    /// the recharge still says it has work.
    ///
    /// Mutation: pass zero instead of the profile's amount and the second pass
    /// already reports the vitals ready.
    /// </summary>
    [Fact]
    public void TheBoostAmountKeepsANeedAliveAboveTheThreshold()
    {
        var surface = new Surface
        {
            CurrentHealth = 50,
            MaxHealth = 100,
            Skills = [Skill(33u, 400u)],
            Spells = [Spell(100u, "Heal Self VII", family: 1u, quality: 300)],
        };
        var settings = new VitalSettings
        {
            NormalHealth = 0.75d,
            RechargeBoostAmount = 40,
            RechargeBoostTimeSeconds = 5d,
        };
        var controller = new VitalRechargeController(
            new Host(surface),
            settings,
            new CombatSettings());
        controller.BindActionLocks(new ActionLockTable());

        // Peace stance with only a spell to recharge with: the stance is asked
        // for, and the wait for it is what arms the boost.
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));

        // Back above the threshold, with nothing left to use. Read straight,
        // the character is fine; read through the boost it is still short.
        surface.CurrentHealth = 80u;
        surface.Spells = [];
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));
        Assert.Contains("No Health", controller.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// The boost is a window, and the profile sets how long it is: past it the
    /// character is read straight again and the recharge is done.
    ///
    /// Mutation: arm the window from a constant and the pass past the
    /// profile's five seconds still reports work to do.
    /// </summary>
    [Fact]
    public void TheBoostWindowIsTheProfilesOwnLength()
    {
        var surface = new Surface
        {
            CurrentHealth = 50,
            MaxHealth = 100,
            Skills = [Skill(33u, 400u)],
            Spells = [Spell(100u, "Heal Self VII", family: 1u, quality: 300)],
        };
        var settings = new VitalSettings
        {
            NormalHealth = 0.75d,
            RechargeBoostAmount = 40,
            RechargeBoostTimeSeconds = 5d,
        };
        var controller = new VitalRechargeController(
            new Host(surface),
            settings,
            new CombatSettings());
        controller.BindActionLocks(new ActionLockTable());

        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));

        surface.CurrentHealth = 80u;
        surface.Spells = [];
        Assert.False(controller.Tick(6d, enabled: true, noTarget: false, helpers: false));
        Assert.Equal("Vitals ready", controller.Status);
    }

    /// <summary>
    /// The boost exists to carry a recharge as far as its cast; this option
    /// decides whether landing that cast ends it. With the flag set the window
    /// is dropped the moment the spell lands and the character is read
    /// straight again; with it clear the window runs its own length out.
    ///
    /// Mutation: drop the window unconditionally on a landed cast and the
    /// second row reports the vitals ready.
    /// </summary>
    [Theory]
    [InlineData(true, false)]
    [InlineData(false, true)]
    public void LandingTheBoostSpellEndsTheWindowOnlyWhenTheProfileSaysSo(
        bool clearOnCast,
        bool stillBusy)
    {
        var surface = new Surface
        {
            CurrentHealth = 50,
            MaxHealth = 100,
            Skills = [Skill(33u, 400u)],
            Spells = [Spell(100u, "Adja's Intervention", family: 1u, quality: 300)],
        };
        var settings = new VitalSettings
        {
            NormalHealth = 0.75d,
            RechargeBoostAmount = 40,
            RechargeBoostTimeSeconds = 5d,
            ClearLevelBoostFlagOnCast = clearOnCast,
        };
        var controller = new VitalRechargeController(
            new Host(surface),
            settings,
            new CombatSettings());
        controller.BindActionLocks(new ActionLockTable());

        // Peace stance: the stance is asked for and the boost is armed.
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));

        // In stance the cast goes out, and the server answers it.
        surface.Mode = PluginCombatMode.Magic;
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));
        surface.LastCastCompletion = new PluginCastCompletion(1L, 100u, 0u, 0u);
        Assert.True(controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));

        // Back above the threshold with nothing left to use: only a window
        // still standing can report work.
        surface.CurrentHealth = 80u;
        surface.Spells = [];
        Assert.Equal(
            stillBusy,
            controller.Tick(0.3d, enabled: true, noTarget: false, helpers: false));
    }

    /// <summary>
    /// Each vital the helper watches has its own reach, and a fellow beyond
    /// that vital's reach is not a candidate for it. Health first: the same
    /// hurt fellow forty metres off is helped at fifty and ignored at thirty.
    ///
    /// Mutation: share one reach across the three vitals, or drop the
    /// distance test, and the short row helps anyway.
    /// </summary>
    [Theory]
    [InlineData(50d, true)]
    [InlineData(30d, false)]
    public void TheHelpersHealthReachIsItsOwnProfileNumber(double reach, bool helps)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [Spell(300u, "Adja's Grace", 900u, 350)],
            Lookup = [Spell((uint)SpellId.AdjaSGift, "Adja's Gift", 900u, 100)],
            InFellowship = true,
            Fellows = [Fellow(71u, "Hurt", health: 5, distance: 40f)],
        };

        Assert.Equal(
            helps,
            VitalRechargePlanner.TryPlanHelper(
                surface,
                new VitalSettings { HelperHealthDistance = reach },
                out VitalRechargeChoice choice));
        if (helps)
            Assert.Equal(VitalKind.Health, choice.Vital);
    }

    /// <summary>
    /// Stamina carries its own reach, and shortening it drops the fellow from
    /// the stamina row while the profile's other reaches are untouched.
    ///
    /// Mutation: read the health reach for the stamina row and the short row
    /// helps anyway.
    /// </summary>
    [Theory]
    [InlineData(50d, true)]
    [InlineData(30d, false)]
    public void TheHelpersStaminaReachIsItsOwnProfileNumber(double reach, bool helps)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [Spell(301u, "Replenish Greater", 81u, 350)],
            Lookup = [Spell((uint)SpellId.Replenish, "Replenish", 81u, 100)],
            InFellowship = true,
            Fellows =
            [
                Fellow(71u, "Winded", health: 100, distance: 40f)
                    with { CurrentStamina = 5u },
            ],
        };

        Assert.Equal(
            helps,
            VitalRechargePlanner.TryPlanHelper(
                surface,
                new VitalSettings
                {
                    HelperStamina = 0.5d,
                    HelperHealthDistance = 50d,
                    HelperStaminaDistance = reach,
                },
                out VitalRechargeChoice choice));
        if (helps)
            Assert.Equal(VitalKind.Stamina, choice.Vital);
    }

    /// <summary>
    /// Mana's reach is shorter than the other two out of the box, and it is
    /// read for the mana row alone.
    ///
    /// Mutation: read the health reach for the mana row and the short row
    /// helps anyway.
    /// </summary>
    [Theory]
    [InlineData(50d, true)]
    [InlineData(30d, false)]
    public void TheHelpersManaReachIsItsOwnProfileNumber(double reach, bool helps)
    {
        var surface = new Surface
        {
            Mode = PluginCombatMode.Magic,
            Spells = [Spell(302u, "Gift of Essence Greater", 950u, 350)],
            Lookup =
                [Spell((uint)SpellId.GiftOfEssence, "Gift of Essence", 950u, 100)],
            InFellowship = true,
            Fellows =
            [
                Fellow(71u, "Drained", health: 100, distance: 40f)
                    with { CurrentMana = 5u },
            ],
        };

        Assert.Equal(
            helps,
            VitalRechargePlanner.TryPlanHelper(
                surface,
                new VitalSettings
                {
                    HelperMana = 0.5d,
                    HelperHealthDistance = 50d,
                    HelperStaminaDistance = 50d,
                    HelperManaDistance = reach,
                },
                out VitalRechargeChoice choice));
        if (helps)
            Assert.Equal(VitalKind.Mana, choice.Vital);
    }

    private static Surface HealersHeartSurface() => new()
    {
        Mode = PluginCombatMode.Magic,
        Spells = [Spell(300u, "Adja's Grace", 900u, 100)],
        Lookup = [Spell((uint)SpellId.AdjaSGift, "Adja's Gift", 900u, 100)],
        InFellowship = true,
        Skills = [Skill(33u, 400u), Skill(14u, 400u)],
        Items = [Item(30u, "The Healer's Heart")],
        Fellows = [Fellow(71u, "Hurt", health: 5, distance: 10f)],
    };

    private static PluginSkillInfo Skill(uint id, uint current) =>
        new(id, string.Empty, PluginSkillTraining.Trained, current);

    private static PluginSpellInfo Spell(
        uint id,
        string name,
        uint family,
        int quality) =>
        new(id, name, family, 7, quality, 10, 0f, 33u, string.Empty, true, true);

    private static PluginInventoryItem Kit(
        uint id,
        string name,
        int booster) => Item(id, name) with
        {
            BoosterVital = booster,
            BoostValue = 0,
            HealKitModifier = 1.2,
            PublicFlags = 0x00010000u,
        };

    /// <summary>Food needs no Healing skill, which is what tells it from a kit.</summary>
    private static PluginInventoryItem Food(
        uint id,
        string name) => (Item(id, name) with
        {
            BoosterVital = (int)VitalKind.Health,
            BoostValue = 20,
        }) with
        {
            UseRequiresSkill = 0,
        };

    private static PluginInventoryItem Caster(
        uint id,
        string name,
        uint spellId) => new(
            id, 1u, name, 0x8000u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, spellId, 0, 0, 0u, false, 0d, 0, 0, 0, 0d,
            0, 0, 0);

    private static PluginInventoryItem Item(uint id, string name) => new(
        id, 1u, name, 0x80u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
        1, 1, 1, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d,
        21, 0, 0);

    private static PluginFellowMember Fellow(
        uint id,
        string name,
        uint health,
        float distance) => new(
            id, name, health, 100u, 100u, 100u, 100u, 100u, distance)
        {
            VitalsAgeSeconds = 0d,
        };

    private sealed class Host(Surface surface) : IPluginHost
    {
        public bool HasUi => false;
        public FakeLogger Logger { get; } = new();
        public IPluginLogger Log => Logger;
        public IGameState State => null!;
        public IEvents Events => null!;
        public ISelectionService Selection => null!;
        public IUiRegistry Ui => null!;
        public IAutomationSurface Automation => surface;
    }

    private sealed class FakeLogger : IPluginLogger
    {
        public List<string> Infos { get; } = [];
        public List<string> Warnings { get; } = [];
        public void Info(string message) => Infos.Add(message);
        public void Warn(string message) => Warnings.Add(message);
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class Surface :
        IAutomationSurface,
        IWorldObjectAutomation,
        ICharacterInfo,
        ISpellCatalog,
        IMagicCommands,
        IPluginChat,
        ICombatAutomation,
        IItemAutomation,
        IFellowshipAutomation
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog SpellsCatalog => this;
        ISpellCatalog IAutomationSurface.Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;
        public ICombatAutomation Combat => this;
        public IItemAutomation ItemsAutomation => this;
        IItemAutomation IAutomationSurface.Items => this;
        public IFellowshipAutomation Fellowship => this;
        public bool IsInWorld => true;
        public uint ObjectId => 1u;
        public uint CurrentHealth { get; set; } = 100u;
        public uint MaxHealth { get; init; } = 100u;
        public uint CurrentStamina { get; init; } = 100u;
        public uint MaxStamina { get; init; } = 100u;
        public uint CurrentMana { get; init; } = 100u;
        public uint MaxMana { get; init; } = 100u;
        public IReadOnlyList<PluginSkillInfo> Skills { get; set; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginSpellInfo> Spells { get; set; } = [];
        public IReadOnlyList<PluginSpellInfo> Lookup { get; init; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => Spells;
        public IReadOnlyList<PluginInventoryItem> Items { get; set; } = [];
        // An assessed item answers a property capture. The bag carries what
        // the item's own projection already knows, its mana above all, so a
        // reader that keys on the appraised properties sees the same numbers.
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];

        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            if (Properties.TryGetValue(objectId, out properties))
                return true;
            foreach (PluginInventoryItem item in Items)
            {
                if (item.ObjectId != objectId)
                    continue;
                var ints = new Dictionary<uint, int>();
                if (item.ItemMaximumMana > 0)
                {
                    ints[107u] = item.ItemCurrentMana;
                    ints[108u] = item.ItemMaximumMana;
                }
                properties = new PluginItemProperties(
                    ints,
                    new Dictionary<uint, long>(),
                    new Dictionary<uint, bool>(),
                    new Dictionary<uint, double>(),
                    new Dictionary<uint, string>(),
                    new Dictionary<uint, uint>(),
                    new Dictionary<uint, uint>());
                return true;
            }
            if (((IWorldObjectAutomation)this).TryGet(objectId, out _))
            {
                properties = new PluginItemProperties(
                    new Dictionary<uint, int>(),
                    new Dictionary<uint, long>(),
                    new Dictionary<uint, bool>(),
                    new Dictionary<uint, double>(),
                    new Dictionary<uint, string>(),
                    new Dictionary<uint, uint>(),
                    new Dictionary<uint, uint>());
                return true;
            }
            properties = default;
            return false;
        }
        // The host's object table, as far as these tests need it: every owned
        // item is there and already assessed, the state a kit or stone is in
        // before the macro may use it.
        public IWorldObjectAutomation Objects => this;
        bool IWorldObjectAutomation.IsAvailable => true;
        bool IWorldObjectAutomation.TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginInventoryItem item in Items)
            {
                if (item.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    item.ObjectId, item.WeenieClassId, item.Name, PluginObjectClass.Unknown,
                    item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                {
                    LastIdTime = Unassessed.Contains(item.ObjectId) ? 0 : 1,
                };
                return true;
            }
            value = default;
            return false;
        }

        /// <summary>Items the client has not appraised.</summary>
        public HashSet<uint> Unassessed { get; } = [];

        /// <summary><c>ActionLockType.ItemUse</c>.</summary>
        public bool ItemsBusy { get; set; }
        public bool InFellowship { get; init; }
        public IReadOnlyList<PluginFellowMember> Fellows { get; init; } = [];
        public PluginCombatMode Mode { get; set; } = PluginCombatMode.Peace;
        public PluginCombatSnapshot Snapshot => new(0u, Mode, default, 0f, 0f,
            false, false, false, false);
        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => ItemsBusy;
        bool IFellowshipAutomation.IsInFellowship => InFellowship;
        public bool IsCasting => false;
        public PluginCastCompletion LastCastCompletion { get; set; }
        PluginCastCompletion IMagicCommands.LastCompletion => LastCastCompletion;

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

        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            foreach (PluginSpellInfo candidate in Spells.Concat(Lookup))
            {
                if (candidate.SpellId == spellId)
                {
                    info = candidate;
                    return true;
                }
            }
            info = default;
            return false;
        }

        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Items;
        public List<uint> UsedItemIds { get; } = [];
        public PluginItemUseCompletion LastItemCompletion { get; set; }
        PluginItemUseCompletion IItemAutomation.LastCompletion =>
            LastItemCompletion;

        PluginItemCommandResult IItemAutomation.Use(uint objectId)
        {
            UsedItemIds.Add(objectId);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        PluginItemCommandResult IItemAutomation.Apply(
            uint objectId,
            uint targetObjectId)
        {
            UsedItemIds.Add(objectId);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        public IReadOnlyList<PluginFellowMember> CaptureMembers() => Fellows;
        public List<bool> VitalsRequests { get; } = [];
        public PluginFellowshipCommandResult RequestVitals(bool requested)
        {
            VitalsRequests.Add(requested);
            return new(PluginFellowshipCommandStatus.Accepted);
        }
        public IReadOnlyList<PluginCombatTarget> CaptureHostileTargets(float maximumDistance) => [];
        public PluginCombatCommandResult EnterDefaultMode() => new(PluginCombatCommandStatus.AlreadyReady);
        public PluginCombatCommandResult BeginPhysicalAttack(uint targetObjectId, PluginAttackHeight height, float power) => new(PluginCombatCommandStatus.Started);
        public PluginCombatCommandResult ReleasePhysicalAttack() => new(PluginCombatCommandStatus.Released);
        public PluginCombatCommandResult AbortPhysicalAttack() => new(PluginCombatCommandStatus.Stopped);
        public PluginCastGate EvaluateGate(uint spellId) => PluginCastGate.Ready;
        public bool Cast(uint spellId) => true;
        public void PostSystemMessage(string text) { }
    }
}
