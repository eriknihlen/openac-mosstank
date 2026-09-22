using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tinkering;

/// <summary>
/// Plans and runs tinkering: it reads what the character owns, picks the
/// cheapest salvage that still clears the odds the player asked for, queues
/// one job per item, and then feeds those bags to the server one at a time,
/// saying yes to the client's own "this may fail" prompt and reading the
/// answer back out of chat.
/// </summary>
/// <remarks>
/// <para>
/// A tinker is a round trip: apply, answer the prompt, wait for the line that
/// says it worked or did not. Nothing here overlaps two of those, and a
/// failure ends the job on that item, because the attempt is spent either
/// way and the next bag in the queue was chosen against the old attempt
/// count.
/// </para>
/// <para>
/// Two things the plan depends on cannot be read from this client: which
/// augmentations the character carries (Jack of All Trades, which lifts every
/// tinkering skill by five, and Charmed Smith, which lifts an imbue's odds by
/// five points), and whether the client's own craft-confirmation option is
/// switched on. The skill this reads is the effective one, so an augmentation
/// already folded into it is counted; Charmed Smith is a setting the player
/// ticks.
/// </para>
/// </remarks>
internal sealed class TinkerJobManager : IDisposable
{
    /// <summary>
    /// The confirmation type the client raises for a crafting attempt that
    /// may fail. It is the one this answers; everything else is left alone.
    /// </summary>
    private const int CraftConfirmationType = 5;

    /// <summary>
    /// A full bag of salvage. A part-used bag cannot be applied, so only
    /// these are ever queued.
    /// </summary>
    private const int FullSalvageStructure = 100;

    /// <summary>
    /// How long one attempt may go unanswered before the run gives up. The
    /// reference waits for its chat line forever, which strands a run when
    /// the line never comes; the wait is bounded here so a stuck run reports
    /// itself instead of looking busy.
    /// </summary>
    private const double AttemptTimeoutSeconds = 20d;

    /// <summary>
    /// The lowest odds the bulk imbue will queue anything at, as a share.
    /// </summary>
    private const double BulkImbueMinimumChance = 0.30d;

    /// <summary>
    /// Bonuses to a weapon's damage ceiling that the damage estimate has to
    /// count before it weighs iron against granite.
    /// </summary>
    private static readonly (uint SpellId, int Bonus)[] DamageCeilingCantrips =
    [
        (2598u, 2),   // Minor Blood Thirst
        (2586u, 4),   // Major Blood Thirst
        (4661u, 7),   // Epic Blood Thirst
        (6089u, 10),  // Legendary Blood Thirst
    ];

    /// <summary>
    /// The flat allowance the damage estimate adds on top of the weapon's own
    /// ceiling before it compares two salvages.
    /// </summary>
    private const int DamageCeilingAllowance = 24;

    /// <summary>The item classes that can be tinkered at all.</summary>
    private static readonly PluginObjectClass[] TinkerableClasses =
    [
        PluginObjectClass.Armor,
        PluginObjectClass.Clothing,
        PluginObjectClass.Jewelry,
        PluginObjectClass.MeleeWeapon,
        PluginObjectClass.MissileWeapon,
        PluginObjectClass.WandStaffOrb,
    ];

    /// <summary>
    /// The only clothing slots worth tinkering: the three that take armour of
    /// their own. Head, hands and feet, as slot masks.
    /// </summary>
    private static readonly uint[] TinkerableClothingSlots = [1u, 32u, 256u];

    /// <summary>
    /// The damage types an imbue can be chosen for, the bit the server uses
    /// for each, and the salvage that rends it. Alphabetical, which is the
    /// order the choice offers them in.
    /// </summary>
    internal static IReadOnlyList<(string Name, int Bit, int Material)> ImbueDamageTypes { get; } =
    [
        ("Acid", 32, (int)TinkerMaterial.Emerald),
        ("Bludgeoning", 4, (int)TinkerMaterial.WhiteSapphire),
        ("Cold", 8, (int)TinkerMaterial.Aquamarine),
        ("Electric", 64, (int)TinkerMaterial.Jet),
        ("Fire", 16, (int)TinkerMaterial.RedGarnet),
        ("Nether", 1024, (int)TinkerMaterial.BlackOpal),
        ("Normal", 0, (int)TinkerMaterial.BlackOpal),
        ("Piercing", 2, (int)TinkerMaterial.BlackGarnet),
        ("SlashPierce", 3, (int)TinkerMaterial.ImperialTopaz),
        ("Slashing", 1, (int)TinkerMaterial.ImperialTopaz),
    ];

    /// <summary>The choice of salvage offered for an item of this kind.</summary>
    internal static IReadOnlyList<string> UsableSalvage(PluginObjectClass objectClass) =>
        objectClass switch
        {
            PluginObjectClass.MissileWeapon =>
                ["Mahogany", "Brass", "Gold", "Moonstone", "Linen", "Pine"],
            PluginObjectClass.MeleeWeapon =>
            [
                GraniteIron, "Brass", "Granite", "Iron", "Gold", "Moonstone",
                "Linen", "Pine",
            ],
            PluginObjectClass.WandStaffOrb =>
                ["Brass", "Green Garnet", "Gold", "Moonstone", "Linen", "Pine", "Opal"],
            PluginObjectClass.Armor or PluginObjectClass.Clothing =>
                ["Steel", "Moonstone", "Linen", "Pine", "Gold"],
            PluginObjectClass.Jewelry =>
                ["Gold", "Moonstone", "Linen", "Pine"],
            _ => [],
        };

    /// <summary>
    /// The salvage choice that is not one material but a decision taken per
    /// attempt: whichever of granite and iron is worth more on the weapon as
    /// it then stands.
    /// </summary>
    internal const string GraniteIron = "Granite/Iron";

    private readonly IPluginHost _host;
    private readonly Action<string> _write;
    private readonly Func<bool> _charmedSmith;

    private readonly List<TinkerJob> _jobs = [];
    private readonly List<TinkerListRow> _tinkerRows = [];
    private readonly List<TinkerListRow> _imbueRows = [];

    /// <summary>Full bags the character owns, lowest workmanship first.</summary>
    private readonly List<PluginInventoryItem> _salvageBags = [];

    /// <summary>Items the character owns that could take a tinker.</summary>
    private readonly List<PluginInventoryItem> _tinkerableItems = [];

    /// <summary>Bags already spoken for by a queued attempt.</summary>
    private readonly HashSet<uint> _salvageUsed = [];

    /// <summary>
    /// The materials the plan still wants, one entry per remaining attempt.
    /// A bag is matched against this list and the entry struck off.
    /// </summary>
    private readonly List<int> _wantedMaterials = [];

    /// <summary>The materials a bag may be drawn from at all.</summary>
    private readonly List<int> _possibleMaterials = [];

    private IReadOnlyList<PluginInventoryItem> _owned = [];

    private bool _running;
    private TinkerJob? _activeJob;
    private int _attemptsTaken = -1;
    private int _lastSkill;
    private uint _lastSkillId;
    private uint _pendingSalvageId;
    private string _pendingSalvageName = string.Empty;
    private double _pendingSalvageWorkmanship;
    private string _pendingItemName = string.Empty;
    private bool _awaitingOutcome;
    private double _waitElapsed;
    private ulong _chatSequence;
    private Action<PluginConfirmation>? _confirmationHandler;

    /// <summary>
    /// Binds the manager to the client it drives.
    /// </summary>
    /// <param name="host">The client surfaces the run works through.</param>
    /// <param name="write">Where the run's own lines go.</param>
    /// <param name="charmedSmith">
    /// Whether the character carries the Charmed Smith augmentation, which
    /// this client cannot read for itself.
    /// </param>
    internal TinkerJobManager(
        IPluginHost host,
        Action<string> write,
        Func<bool> charmedSmith)
    {
        _host = host;
        _write = write;
        _charmedSmith = charmedSmith;
        _confirmationHandler = OnConfirmationRequested;
        _host.Events.ConfirmationRequested += _confirmationHandler;
    }

    /// <summary>The jobs queued, in the order they will run.</summary>
    internal IReadOnlyList<TinkerJob> Jobs => _jobs;

    /// <summary>The Tinker page's list.</summary>
    internal IReadOnlyList<TinkerListRow> TinkerRows => _tinkerRows;

    /// <summary>The Imbue page's list.</summary>
    internal IReadOnlyList<TinkerListRow> ImbueRows => _imbueRows;

    /// <summary>True while a queued job is being worked through.</summary>
    internal bool IsRunning => _running;

    /// <summary>
    /// Re-reads what the character owns: the full bags of salvage, cheapest
    /// first, and every item that could still take a tinker.
    /// </summary>
    internal void ScanInventory()
    {
        _owned = _host.Automation.Items.CaptureOwnedItems();
        _salvageBags.Clear();
        _tinkerableItems.Clear();
        foreach (PluginInventoryItem item in _owned)
        {
            if (item.ObjectClass == PluginObjectClass.Salvage
                && item.Structure == FullSalvageStructure)
            {
                _salvageBags.Add(item);
            }
            if (CanBeTinkered(item))
                _tinkerableItems.Add(item);
        }
        _salvageBags.Sort(static (left, right) =>
            left.SalvageWorkmanship.CompareTo(right.SalvageWorkmanship));
        _tinkerableItems.Sort(static (left, right) =>
            left.SalvageWorkmanship.CompareTo(right.SalvageWorkmanship));
    }

    /// <summary>
    /// Whether this item can take a tinker at all: the right kind of thing,
    /// a workmanship to roll against, and attempts left. Clothing has to be
    /// one of the three armoured slots and carry armour of its own, or
    /// tinkering it does nothing.
    /// </summary>
    internal static bool CanBeTinkered(in PluginInventoryItem item)
    {
        if (!TinkerableClasses.Contains(item.ObjectClass))
            return false;
        if (item.SalvageWorkmanship <= 0d)
            return false;
        if (item.NumTimesTinkered >= TinkerCalc.MaximumAttempts)
            return false;
        if (item.ObjectClass == PluginObjectClass.Clothing
            && (!TinkerableClothingSlots.Contains(item.ValidLocations)
                || item.ArmorLevel == 0))
        {
            return false;
        }
        return true;
    }

    /// <summary>
    /// Whether this item can still take an imbue: it has a workmanship, it
    /// carries no imbue already, and it has attempts left.
    /// </summary>
    internal static bool CanBeImbued(in PluginInventoryItem item)
    {
        if (item.Workmanship <= 0f)
            return false;
        if (item.ImbuedEffect >= 1)
            return false;
        return item.NumTimesTinkered < TinkerCalc.MaximumAttempts;
    }

    /// <summary>The item, when the character owns it.</summary>
    internal bool TryOwned(uint objectId, out PluginInventoryItem item)
    {
        foreach (PluginInventoryItem candidate in _owned)
        {
            if (candidate.ObjectId != objectId)
                continue;
            item = candidate;
            return true;
        }
        item = default;
        return false;
    }

    // ── planning ────────────────────────────────────────────────────────

    /// <summary>
    /// Plans a run of ordinary tinkers on one item: every attempt from the
    /// one it is on up to <paramref name="maximumAttempts"/>, each spending
    /// the cheapest bag that still beats <paramref name="minimumPercent"/>.
    /// </summary>
    /// <param name="itemObjectId">The item to tinker.</param>
    /// <param name="salvageChoice">
    /// The material chosen on the page, or <see cref="GraniteIron"/> to pick
    /// between those two per attempt.
    /// </param>
    /// <param name="maximumAttempts">The attempt to stop at, 1 through 10.</param>
    /// <param name="minimumPercent">
    /// The odds an attempt has to clear, as a percentage.
    /// </param>
    internal void PopulateTinkerList(
        uint itemObjectId,
        string salvageChoice,
        int maximumAttempts,
        double minimumPercent)
    {
        ClearPlan();
        _tinkerRows.Clear();
        ScanInventory();
        if (!TryOwned(itemObjectId, out PluginInventoryItem item))
        {
            _write("Select an item first");
            return;
        }
        if (!CanBeTinkered(item))
        {
            _write("Item cannot be tinkered");
            return;
        }

        foreach (string name in UsableSalvage(item.ObjectClass))
            AddPossibleMaterial(name);

        BuildWantedMaterials(item, salvageChoice, maximumAttempts);
        BuildJob(item, minimumPercent, maximumAttempts, _tinkerRows);
    }

    /// <summary>
    /// Plans a rend of one damage type with one salvage across everything
    /// that can still take it: one attempt per item.
    /// </summary>
    internal void PopulateImbueList(string damageTypeName, string salvageName)
    {
        ClearPlan();
        _imbueRows.Clear();
        ScanInventory();
        int salvageMaterial = TinkerMaterials.Id(salvageName);
        if (salvageMaterial == 0)
        {
            _write("no salvage matches...  quitting");
            return;
        }
        AddPossibleMaterial(salvageName);

        int damageBit = DamageBit(damageTypeName);
        IReadOnlyList<PluginObjectClass> allowed =
            TinkerType.ImbueTargets(salvageMaterial);
        int number = 0;
        foreach (PluginInventoryItem item in _tinkerableItems)
        {
            if (!CanBeImbued(item))
                continue;
            if (!allowed.Contains(item.ObjectClass))
                continue;
            if (item.DamageType != damageBit
                && item.WandElementalDamageType != damageBit)
            {
                continue;
            }
            _wantedMaterials.Clear();
            _wantedMaterials.Add(salvageMaterial);
            // A rend is one attempt, whatever has already gone into the item:
            // it is normally the last thing put in, so planning it against the
            // attempts left would leave every part-tinkered weapon out.
            if (!TryTakeSalvage(
                item,
                item.NumTimesTinkered,
                minimumPercent: 0d,
                out PluginInventoryItem bag,
                out double chance))
            {
                continue;
            }
            _salvageUsed.Add(bag.ObjectId);
            _jobs.Add(new TinkerJob
            {
                ItemObjectId = item.ObjectId,
                SalvageToApply = [bag.ObjectId],
            });
            number++;
            _imbueRows.Add(Row(number, item, bag, chance));
        }
    }

    /// <summary>
    /// Plans one rend for every weapon that can still take one, choosing the
    /// salvage that matches what the weapon already strikes with. Anything
    /// whose odds fall below thirty percent is left out.
    /// </summary>
    internal void PopulateRendAll()
    {
        ClearPlan();
        _imbueRows.Clear();
        ScanInventory();
        foreach (int material in TinkerMaterials.Named)
            _possibleMaterials.Add(material);

        int number = 0;
        foreach (PluginInventoryItem item in _tinkerableItems)
        {
            if (!CanBeImbued(item))
                continue;
            int damageBit = item.ObjectClass switch
            {
                PluginObjectClass.WandStaffOrb => item.WandElementalDamageType,
                PluginObjectClass.MeleeWeapon or PluginObjectClass.MissileWeapon =>
                    item.DamageType,
                _ => -1,
            };
            if (damageBit == -1)
                continue;
            int salvageMaterial = DefaultSalvage(damageBit);
            if (salvageMaterial == -1)
                continue;
            if (!TryNextBag(salvageMaterial, out PluginInventoryItem bag))
                continue;

            double chance = ChanceFor(bag, item, item.NumTimesTinkered);
            if (chance < BulkImbueMinimumChance)
                continue;

            _salvageUsed.Add(bag.ObjectId);
            _jobs.Add(new TinkerJob
            {
                ItemObjectId = item.ObjectId,
                SalvageToApply = [bag.ObjectId],
            });
            number++;
            _imbueRows.Add(Row(number, item, bag, chance));
        }
    }

    /// <summary>
    /// The materials the plan wants, one per remaining attempt. Granite and
    /// iron are decided attempt by attempt against a running estimate of the
    /// weapon's damage, because each of them changes what the next one is
    /// worth.
    /// </summary>
    private void BuildWantedMaterials(
        in PluginInventoryItem item,
        string salvageChoice,
        int maximumAttempts)
    {
        _wantedMaterials.Clear();
        double maximumDamage = DamageCeiling(item);
        double variance = item.DamageVariance;
        int remaining = maximumAttempts - item.NumTimesTinkered;
        for (int attempt = 1; attempt <= remaining; attempt++)
        {
            if (!string.Equals(salvageChoice, GraniteIron, StringComparison.Ordinal))
            {
                _wantedMaterials.Add(TinkerMaterials.Id(salvageChoice));
                continue;
            }
            if (TinkerCalc.IronBeatsGranite(maximumDamage, variance))
            {
                maximumDamage += 1d;
                _wantedMaterials.Add((int)TinkerMaterial.Iron);
            }
            else
            {
                variance *= 0.8d;
                _wantedMaterials.Add((int)TinkerMaterial.Granite);
            }
        }
    }

    /// <summary>
    /// Turns the wanted materials into one job on this item: for each
    /// remaining attempt, the cheapest unspoken-for bag of a wanted material
    /// whose odds clear the floor.
    /// </summary>
    private void BuildJob(
        in PluginInventoryItem item,
        double minimumPercent,
        int maximumAttempts,
        List<TinkerListRow> rows)
    {
        var queued = new List<uint>();
        int attemptsTaken = item.NumTimesTinkered;
        int remaining = maximumAttempts - item.NumTimesTinkered;
        for (int attempt = 1; attempt <= remaining; attempt++)
        {
            if (!TryTakeSalvage(item, attemptsTaken, minimumPercent, out PluginInventoryItem bag, out double chance))
                continue;
            _salvageUsed.Add(bag.ObjectId);
            queued.Add(bag.ObjectId);
            rows.Add(Row(attemptsTaken + 1, item, bag, chance));
            attemptsTaken++;
        }
        if (queued.Count == 0)
            return;
        _jobs.Add(new TinkerJob
        {
            ItemObjectId = item.ObjectId,
            SalvageToApply = queued,
        });
    }

    /// <summary>
    /// The cheapest bag of a still-wanted material that clears the floor, and
    /// what it is worth. Taking one strikes its material off the want list.
    /// </summary>
    private bool TryTakeSalvage(
        in PluginInventoryItem item,
        int attemptsTaken,
        double minimumPercent,
        out PluginInventoryItem bag,
        out double chance)
    {
        foreach (PluginInventoryItem candidate in _salvageBags)
        {
            int material = (int)candidate.MaterialType;
            if (!_wantedMaterials.Contains(material))
                continue;
            if (_salvageUsed.Contains(candidate.ObjectId))
                continue;
            double candidateChance = ChanceFor(candidate, item, attemptsTaken);
            if (candidateChance <= minimumPercent / 100d)
                continue;
            _wantedMaterials.Remove(material);
            bag = candidate;
            chance = candidateChance;
            return true;
        }
        bag = default;
        chance = 0d;
        return false;
    }

    /// <summary>
    /// The cheapest unspoken-for bag of this material, when the plan allows
    /// the material at all.
    /// </summary>
    private bool TryNextBag(int material, out PluginInventoryItem bag)
    {
        foreach (PluginInventoryItem candidate in _salvageBags)
        {
            if (!_possibleMaterials.Contains((int)candidate.MaterialType))
                continue;
            if (_salvageUsed.Contains(candidate.ObjectId))
                continue;
            if (candidate.MaterialType != (uint)material)
                continue;
            bag = candidate;
            return true;
        }
        bag = default;
        return false;
    }

    /// <summary>
    /// The cheapest full bag of this material the character owns, by the
    /// packs as they were last read.
    /// </summary>
    internal bool TryCheapestBag(int material, out PluginInventoryItem bag)
    {
        foreach (PluginInventoryItem candidate in _salvageBags)
        {
            if (candidate.MaterialType != (uint)material)
                continue;
            bag = candidate;
            return true;
        }
        bag = default;
        return false;
    }

    private void AddPossibleMaterial(string name)
    {
        if (string.Equals(name, GraniteIron, StringComparison.Ordinal))
        {
            _possibleMaterials.Add((int)TinkerMaterial.Granite);
            _possibleMaterials.Add((int)TinkerMaterial.Iron);
            return;
        }
        int material = TinkerMaterials.Id(name);
        if (material != 0)
            _possibleMaterials.Add(material);
    }

    /// <summary>Clears the plan, leaving the lists on the page alone.</summary>
    internal void ClearPlan()
    {
        _jobs.Clear();
        _salvageUsed.Clear();
        _wantedMaterials.Clear();
        _possibleMaterials.Clear();
    }

    /// <summary>
    /// What one bag is worth on one item at that attempt count, with the
    /// character's own skill in whichever tinkering skill the material calls
    /// for.
    /// </summary>
    internal double ChanceFor(
        in PluginInventoryItem bag,
        in PluginInventoryItem item,
        int attemptsTaken)
    {
        int material = (int)bag.MaterialType;
        return TinkerCalc.SuccessChance(
            material,
            bag.SalvageWorkmanship,
            item.Workmanship,
            attemptsTaken,
            Skill(TinkerType.SkillId(material)),
            _charmedSmith());
    }

    /// <summary>
    /// The damage ceiling the granite-against-iron comparison works from: the
    /// weapon's own, plus what its cantrips add, plus the flat allowance.
    /// </summary>
    internal static double DamageCeiling(in PluginInventoryItem item)
    {
        double ceiling = item.MaxDamage;
        foreach ((uint spellId, int bonus) in DamageCeilingCantrips)
        {
            if (item.AppraisedSpellIds.Contains(spellId))
                ceiling += bonus;
        }
        return ceiling + DamageCeilingAllowance;
    }

    /// <summary>
    /// How many granite and how many iron an untouched melee weapon should
    /// take, and what its damage per second ends at. The run of choices is
    /// made once, each one against the estimate the previous ones left.
    /// </summary>
    internal static (int Granite, int Iron, double FinalDamagePerSecond)
        BestGraniteIron(in PluginInventoryItem item)
    {
        double maximumDamage = DamageCeiling(item);
        double variance = item.DamageVariance;
        int granite = 0;
        int iron = 0;
        double finalDamage = 0d;
        for (int attempt = item.NumTimesTinkered + 1;
             attempt <= TinkerCalc.MaximumAttempts;
             attempt++)
        {
            if (TinkerCalc.IronBeatsGranite(maximumDamage, variance))
            {
                finalDamage = TinkerCalc.DamagePerSecond(maximumDamage + 1d, variance);
                maximumDamage += 1d;
                iron++;
            }
            else
            {
                finalDamage = TinkerCalc.DamagePerSecond(maximumDamage, variance * 0.8d);
                variance *= 0.8d;
                granite++;
            }
        }
        return (granite, iron, finalDamage);
    }

    /// <summary>The salvage that rends this damage type; -1 when none does.</summary>
    internal static int DefaultSalvage(int damageType)
    {
        foreach ((string _, int bit, int material) in ImbueDamageTypes)
        {
            if (bit == damageType)
                return material;
        }
        return -1;
    }

    /// <summary>The bit the server uses for a damage type named on the page.</summary>
    internal static int DamageBit(string damageTypeName)
    {
        foreach ((string name, int bit, int _) in ImbueDamageTypes)
        {
            if (string.Equals(name, damageTypeName.Trim(), StringComparison.OrdinalIgnoreCase))
                return bit;
        }
        return -1;
    }

    /// <summary>
    /// The salvage a damage type is rended with, by the name the choice
    /// offers; an empty string when the name is not one of them.
    /// </summary>
    internal static string DefaultSalvageName(string damageTypeName)
    {
        int bit = DamageBit(damageTypeName);
        int material = bit == -1 ? -1 : DefaultSalvage(bit);
        return material <= 0 ? string.Empty : TinkerMaterials.Name(material);
    }

    private TinkerListRow Row(
        int number,
        in PluginInventoryItem item,
        in PluginInventoryItem bag,
        double chance) =>
        new()
        {
            Number = number,
            ItemObjectId = item.ObjectId,
            ItemName = item.Name,
            SalvageObjectId = bag.ObjectId,
            SalvageName = string.Create(
                CultureInfo.InvariantCulture,
                $"{SalvageName(bag)}({Math.Round(bag.SalvageWorkmanship, 2, MidpointRounding.AwayFromZero)}) "),
            SuccessChance = chance,
            SuccessText = chance.ToString("P", CultureInfo.InvariantCulture),
        };

    // ── running ─────────────────────────────────────────────────────────

    /// <summary>
    /// Starts working through the queued jobs. Says so and does nothing when
    /// the queue is empty or a run is already going.
    /// </summary>
    internal void Start()
    {
        if (_running)
        {
            _write("already running");
            return;
        }
        if (_jobs.Count == 0)
        {
            _write("i'm out of jobs");
            return;
        }
        _running = true;
        _chatSequence = LatestChatSequence();
        StartNextJob();
    }

    /// <summary>
    /// Stops the run and drops the queue. The lists on the page stay as they
    /// are, so what happened is still readable.
    /// </summary>
    internal void Stop()
    {
        _running = false;
        _activeJob = null;
        _awaitingOutcome = false;
        _waitElapsed = 0d;
        _attemptsTaken = -1;
        _lastSkill = 0;
        _lastSkillId = 0u;
        _pendingSalvageId = 0u;
        _pendingSalvageName = string.Empty;
        _pendingSalvageWorkmanship = 0d;
        _pendingItemName = string.Empty;
        ClearPlan();
    }

    /// <summary>
    /// One frame of the run: read chat for the answer to the attempt in
    /// flight, and start the next attempt when there is none.
    /// </summary>
    internal void OnTick(double elapsedSeconds)
    {
        if (!_running)
            return;
        if (!_awaitingOutcome)
        {
            DoNextTink();
            return;
        }

        _waitElapsed += elapsedSeconds;
        ObserveChat();
        if (!_awaitingOutcome || _waitElapsed < AttemptTimeoutSeconds)
            return;
        _write("AutoTinker: no word back on the last tink... stopping tinkering.");
        Stop();
    }

    private void StartNextJob()
    {
        if (_jobs.Count == 0)
        {
            _write("i'm out of jobs");
            Stop();
            return;
        }
        _activeJob = _jobs[0];
        _attemptsTaken = -1;
        DoNextTink();
    }

    /// <summary>
    /// Hands the next bag to the server: re-reads the item, checks the skill
    /// has not dropped, selects the bag as the client would and applies it.
    /// </summary>
    private void DoNextTink()
    {
        if (_activeJob is not { } job)
        {
            Stop();
            return;
        }
        if (job.SalvageToApply.Count == 0)
        {
            FinishJob();
            return;
        }

        _owned = _host.Automation.Items.CaptureOwnedItems();
        if (!TryOwned(job.ItemObjectId, out PluginInventoryItem item))
        {
            _write("AutoTinker: the item is gone... stopping tinkering.");
            Stop();
            return;
        }
        _attemptsTaken = _attemptsTaken == -1
            ? item.NumTimesTinkered
            : _attemptsTaken + 1;

        uint nextSalvage = job.SalvageToApply[0];
        if (!TryOwned(nextSalvage, out PluginInventoryItem bag))
        {
            _write("AutoTinker: the salvage is gone... stopping tinkering.");
            Stop();
            return;
        }

        uint skillId = TinkerType.SkillId((int)bag.MaterialType);
        int skill = Skill(skillId);
        if (skill < _lastSkill && skillId == _lastSkillId)
        {
            _write("tinkering skill decreased... stopping tinkering.");
            FinishJob();
            return;
        }
        _lastSkillId = skillId;
        _lastSkill = skill;

        _pendingItemName = TinkeredItemName(item);
        _pendingSalvageId = bag.ObjectId;
        _pendingSalvageName = SalvageName(bag);
        _pendingSalvageWorkmanship = bag.SalvageWorkmanship;

        _host.Selection.Select(nextSalvage);
        if (_host.Selection.SelectedObjectId != nextSalvage)
        {
            _write("AutoTinker: could not select the salvage... stopping tinkering.");
            Stop();
            return;
        }

        job.SalvageToApply.RemoveAt(0);
        job.SalvageApplied.Add(nextSalvage);
        _awaitingOutcome = true;
        _waitElapsed = 0d;
        _host.Automation.Items.Apply(nextSalvage, job.ItemObjectId);
    }

    private void FinishJob()
    {
        if (_activeJob is { } job)
            _jobs.Remove(job);
        _activeJob = null;
        _awaitingOutcome = false;
        if (_jobs.Count > 0)
        {
            StartNextJob();
            return;
        }
        _write("Done tinkering");
        Stop();
    }

    /// <summary>
    /// Says yes to the client's craft prompt while a run is waiting on one.
    /// Every other prompt is left for whoever raised it.
    /// </summary>
    private void OnConfirmationRequested(PluginConfirmation confirmation)
    {
        if (!_running || !_awaitingOutcome)
            return;
        if (confirmation.Type != CraftConfirmationType)
            return;
        _write($"AutoTinker: Clicking Yes on {confirmation.Text}");
        _host.Automation.Dialogs.Answer(confirmation.ContextId, accept: true);
    }

    /// <summary>
    /// Reads the lines the server sent since the last look for the one that
    /// answers the attempt in flight.
    /// </summary>
    private void ObserveChat()
    {
        string character = _host.Automation.Character.Name;
        Regex success = SuccessPattern(character);
        Regex failure = FailurePattern(character);

        foreach (PluginChatMessage message in
                 _host.Automation.Chat.CaptureMessages(_chatSequence))
        {
            _chatSequence = Math.Max(_chatSequence, message.Sequence);
            string text = message.Text.Trim();
            if (!_awaitingOutcome)
                continue;

            // One line answers one attempt, and recording the outcome starts
            // the next one. The rest of this batch was sent before that
            // attempt existed, so reading on would let an older line -- the
            // same words twice, which identical bags produce -- answer an
            // attempt the server has not heard about yet. Whatever is left
            // keeps its sequence and is read on the next look.
            Match match = success.Match(text);
            if (match.Success && MatchesPendingAttempt(match))
            {
                RecordOutcome(succeeded: true);
                break;
            }
            Match foolproof = FoolproofPattern.Match(text);
            if (foolproof.Success && MatchesFoolproof(foolproof))
            {
                RecordOutcome(succeeded: true);
                break;
            }
            match = failure.Match(text);
            if (match.Success && MatchesPendingAttempt(match))
            {
                RecordOutcome(succeeded: false);
                break;
            }
        }
    }

    private void RecordOutcome(bool succeeded)
    {
        MarkRow(_pendingSalvageId, succeeded);
        _awaitingOutcome = false;
        _waitElapsed = 0d;
        if (!succeeded)
        {
            // The attempt is spent either way, and every bag still queued was
            // chosen against the attempt count this one has just used up.
            FinishJob();
            return;
        }
        if (_activeJob is { SalvageToApply.Count: > 0 })
        {
            DoNextTink();
            return;
        }
        FinishJob();
    }

    private void MarkRow(uint salvageObjectId, bool succeeded)
    {
        foreach (TinkerListRow row in _tinkerRows)
        {
            if (row.SalvageObjectId == salvageObjectId)
                row.Succeeded = succeeded;
        }
        foreach (TinkerListRow row in _imbueRows)
        {
            if (row.SalvageObjectId == salvageObjectId)
                row.Succeeded = succeeded;
        }
    }

    private bool MatchesPendingAttempt(Match match) =>
        string.Equals(
            match.Groups["item"].Value,
            _pendingItemName,
            StringComparison.Ordinal)
        && string.Equals(
            match.Groups["workmanship"].Value,
            _pendingSalvageWorkmanship.ToString("0.00", CultureInfo.InvariantCulture),
            StringComparison.Ordinal)
        && string.Equals(
            match.Groups["salvage"].Value.Replace(" Salvage", string.Empty, StringComparison.Ordinal).Trim(),
            _pendingSalvageName.Trim(),
            StringComparison.Ordinal);

    private bool MatchesFoolproof(Match match) =>
        string.Equals(
            StripFoolproof(match.Groups["salvage"].Value),
            StripFoolproof(_pendingSalvageName),
            StringComparison.OrdinalIgnoreCase);

    private static string StripFoolproof(string name) =>
        name.Replace("foolproof", string.Empty, StringComparison.OrdinalIgnoreCase).Trim();

    /// <summary>
    /// The item's name as the server says it in the crafting line: the
    /// material once, in front.
    /// </summary>
    internal static string TinkeredItemName(in PluginInventoryItem item)
    {
        string material = TinkerMaterials.Name((int)item.MaterialType);
        if (material.Length == 0)
            return item.Name;
        string bare = item.Name.Contains(material, StringComparison.Ordinal)
            ? item.Name.Replace(material, string.Empty, StringComparison.Ordinal)
            : item.Name;
        return $"{material.Trim()} {bare.Trim()}";
    }

    /// <summary>The bag's name with the salvage wording stripped off it.</summary>
    internal static string SalvageName(in PluginInventoryItem bag) =>
        bag.Name
            .Replace("Salvaged ", string.Empty, StringComparison.Ordinal)
            .Replace("Salvage ", string.Empty, StringComparison.Ordinal)
            .Replace("Salvage", string.Empty, StringComparison.Ordinal)
            .Replace("(100)", string.Empty, StringComparison.Ordinal)
            .Trim();

    private static Regex SuccessPattern(string character) => new(
        "^" + Regex.Escape(character)
        + @" successfully applies the (?<salvage>[\w\s\-]+?)(\sSalvage.*)(\s?\(100\))?\s\(workmanship (?<workmanship>\d+\.\d+)\) to the (?<item>[\w\s'\-]+)\.$",
        RegexOptions.CultureInvariant);

    private static Regex FailurePattern(string character) => new(
        "^" + Regex.Escape(character)
        + @" fails to apply the (?<salvage>[\w\s\-]+?)(\sSalvage.*)(\s?\(100\))?\s\(workmanship (?<workmanship>\d+\.\d+)\) to the (?<item>[\w\s'\-]+)\..*$",
        RegexOptions.CultureInvariant);

    private static readonly Regex FoolproofPattern =
        new(@"^You apply the (?<salvage>.*)\.$", RegexOptions.CultureInvariant);

    private ulong LatestChatSequence()
    {
        ulong latest = 0uL;
        foreach (PluginChatMessage message in _host.Automation.Chat.CaptureMessages(0uL))
            latest = Math.Max(latest, message.Sequence);
        return latest;
    }

    private int Skill(uint skillId)
    {
        if (skillId == 0u)
            return 0;
        foreach (PluginSkillInfo skill in _host.Automation.Character.Skills)
        {
            if (skill.SkillId == skillId)
                return (int)skill.Current;
        }
        return 0;
    }

    /// <summary>
    /// The queue in words, as the job command prints it: one line naming the
    /// item, then one line per bag waiting on it.
    /// </summary>
    internal IEnumerable<string> DescribeJobs()
    {
        if (_jobs.Count == 0)
        {
            yield return "i'm out of jobs";
            yield break;
        }
        foreach (TinkerJob job in _jobs)
        {
            yield return "Target item: " + OwnedName(job.ItemObjectId);
            foreach (uint salvage in job.SalvageToApply)
            {
                yield return string.Create(
                    CultureInfo.InvariantCulture,
                    $"salvage: {OwnedName(salvage)} {salvage}");
            }
        }
    }

    private string OwnedName(uint objectId) =>
        TryOwned(objectId, out PluginInventoryItem item) ? item.Name : objectId.ToString(CultureInfo.InvariantCulture);

    /// <summary>Stops the run and lets go of the confirmation hook.</summary>
    public void Dispose()
    {
        Stop();
        if (_confirmationHandler is not null)
            _host.Events.ConfirmationRequested -= _confirmationHandler;
        _confirmationHandler = null;
    }
}
