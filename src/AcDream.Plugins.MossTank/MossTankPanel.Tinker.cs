using System.Globalization;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Tinkering;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The Tinker page: an item, a salvage, a floor under the odds, and the run
/// that spends one bag after another on it -- with the bulk rend of every
/// weapon that can still take one underneath.
/// </summary>
/// <remarks>
/// The page owns nothing but what is typed into it. The plan and the run
/// live in <see cref="TinkerJobManager"/>, and every number on the page is
/// read back out of it, so what is listed is what will happen.
/// </remarks>
internal sealed partial class MossTankPanel
{
    /// <summary>The colour an ordinary row draws in.</summary>
    private const uint TinkerRowColor = 0xE8DEC3u;

    /// <summary>The colour a row that came off draws in.</summary>
    private const uint TinkerSuccessColor = 0x38C038u;

    /// <summary>The colour a row that failed draws in.</summary>
    private const uint TinkerFailureColor = 0xC03838u;

    private TinkerJobManager _tinker = null!;

    private uint _tinkerItemId;
    private string _tinkerItemLabel = "[None]";
    private string _tinkerNotice =
        "Select an item in your inventory, then add it here.";
    private IReadOnlyList<string> _tinkerSalvageNames = [];
    private string _tinkerSalvageChoice = string.Empty;
    private string _tinkerMinimumPercentDraft = string.Empty;
    private int _tinkerRowIndex = -1;

    private IReadOnlyList<string> _imbueSalvageNames = [];
    private string _imbueSalvageChoice = string.Empty;
    private string _imbueDamageTypeChoice = string.Empty;
    private int _imbueRowIndex = -1;

    private void InitializeTinkering(IPluginHost host)
    {
        _tinker = new TinkerJobManager(
            host,
            WriteVtank,
            () => _ubCatalog.Require("AutoTinker.CharmedSmith").Get().Boolean);

        var salvage = new List<string>();
        foreach (int material in TinkerMaterials.Named)
            salvage.Add(TinkerMaterials.Name(material));
        salvage.Sort(StringComparer.Ordinal);
        salvage.Add(TinkerJobManager.GraniteIron);
        _tinkerSalvageNames = salvage;
        _tinkerSalvageChoice = salvage[0];

        var imbueSalvage = new List<string>();
        foreach (int material in TinkerMaterials.Named)
        {
            if (TinkerType.SalvageKind(material) == TinkerType.ImbueSalvage)
                imbueSalvage.Add(TinkerMaterials.Name(material));
        }
        imbueSalvage.Sort(StringComparer.Ordinal);
        _imbueSalvageNames = imbueSalvage;

        _imbueDamageTypeChoice = TinkerJobManager.ImbueDamageTypes[0].Name;
        _imbueSalvageChoice =
            TinkerJobManager.DefaultSalvageName(_imbueDamageTypeChoice);

        _tinkerMinimumPercentDraft = MinimumTinkerPercent()
            .ToString(CultureInfo.InvariantCulture);
    }

    private void TickTinkering(double elapsedSeconds) =>
        _tinker.OnTick(elapsedSeconds);

    private void DisposeTinkering() => _tinker.Dispose();

    private float MinimumTinkerPercent() =>
        _ubCatalog.Require("AutoTinker.MinPercentage").Get().AsSingle();

    private int MaximumTinkerAttempts() =>
        Math.Clamp(
            _ubCatalog.Require("AutoTinker.MaxTinks").Get().AsInt32(),
            1,
            TinkerCalc.MaximumAttempts);

    // ── the tab itself ──────────────────────────────────────────────────

    /// <summary>Switches to the Tinker page.</summary>
    public Action ShowTinker => () => SelectTab(TankTab.Tinker);

    /// <summary>True while the Tinker page is the one being shown.</summary>
    public bool TinkerSelected => _activeTab == TankTab.Tinker;

    /// <summary>The Tinker page is always reachable.</summary>
    public bool TinkerTabEnabled => true;

    /// <summary>True when the Tinker page's controls should be drawn.</summary>
    public bool TinkerVisible => TinkerSelected;

    // ── the Tinker half ─────────────────────────────────────────────────

    /// <summary>The item the run will tinker, or "[None]".</summary>
    public string TinkerItemName => _tinkerItemLabel;

    /// <summary>What the page last had to say.</summary>
    public string TinkerNotice => _tinkerNotice;

    /// <summary>
    /// Takes whatever the client has selected as the item to tinker, and
    /// narrows the salvage choice to what is worth putting on it.
    /// </summary>
    public Action AddSelectedTinkerItem => () =>
    {
        _tinker.Stop();
        _tinker.ClearPlan();
        _tinker.ScanInventory();
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected == 0u
            || !_tinker.TryOwned(selected, out PluginInventoryItem item))
        {
            _tinkerNotice = "Select an item first";
            return;
        }
        if (!TinkerJobManager.CanBeTinkered(item))
        {
            _tinkerItemId = 0u;
            _tinkerItemLabel = "[None]";
            _tinkerNotice = "Item cannot be tinkered";
            return;
        }
        _tinkerItemId = item.ObjectId;
        _tinkerItemLabel = item.Name;
        _tinkerNotice = string.Create(
            CultureInfo.InvariantCulture,
            $"{item.Name} has taken {item.NumTimesTinkered} of {TinkerCalc.MaximumAttempts} tinks.");

        IReadOnlyList<string> usable =
            TinkerJobManager.UsableSalvage(item.ObjectClass);
        if (usable.Count == 0)
            return;
        _tinkerSalvageNames = usable;
        if (!usable.Contains(_tinkerSalvageChoice))
            _tinkerSalvageChoice = usable[0];
    };

    /// <summary>The salvage the run will spend, as the choice offers them.</summary>
    public IReadOnlyList<string> TinkerSalvageNames => _tinkerSalvageNames;

    /// <summary>Which salvage is chosen.</summary>
    public string SelectedTinkerSalvage => _tinkerSalvageChoice;

    /// <summary>Chooses the salvage a populate will plan with.</summary>
    public Action<string> SelectTinkerSalvage => name =>
    {
        if (_tinkerSalvageNames.Contains(name))
            _tinkerSalvageChoice = name;
    };

    /// <summary>The floor under an attempt's odds, as typed.</summary>
    public string TinkerMinimumPercentText => _tinkerMinimumPercentDraft;

    /// <summary>
    /// Takes a new floor. An empty field means no floor at all, as the
    /// reference has it; anything that is not a number is left alone.
    /// </summary>
    public Action<string> SetTinkerMinimumPercentText => text =>
    {
        _tinkerMinimumPercentDraft = text;
        float percent = 0f;
        if (text.Trim().Length != 0
            && !float.TryParse(
                text,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out percent))
        {
            return;
        }
        _ubCatalog.Require("AutoTinker.MinPercentage")
            .Set(UbSettingValue.FromSingle(percent));
        RefreshUbSettings();
    };

    /// <summary>The attempt counts the run may stop at.</summary>
    public IReadOnlyList<string> TinkerMaxTinksChoices { get; } =
        Enumerable.Range(1, TinkerCalc.MaximumAttempts)
            .Select(static count => count.ToString(CultureInfo.InvariantCulture))
            .ToArray();

    /// <summary>Which attempt the run stops at.</summary>
    public string SelectedTinkerMaxTinks =>
        MaximumTinkerAttempts().ToString(CultureInfo.InvariantCulture);

    /// <summary>
    /// Changes where the run stops. The plan was built against the old
    /// number, so it is dropped, exactly as the reference drops it.
    /// </summary>
    public Action<string> SelectTinkerMaxTinks => text =>
    {
        if (!int.TryParse(
                text,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out int count))
        {
            return;
        }
        count = Math.Clamp(count, 1, TinkerCalc.MaximumAttempts);
        _ubCatalog.Require("AutoTinker.MaxTinks").Set(UbSettingValue.FromInt(count));
        RefreshUbSettings();
        WriteVtank("Updated max tink value to " + count.ToString(CultureInfo.InvariantCulture));
        _tinker.Stop();
        _tinkerNotice = "Populate the list again for the new stop-at count.";
    };

    /// <summary>Plans the run on the item the page holds.</summary>
    public Action PopulateTinkerList => () =>
    {
        if (_tinkerItemId == 0u)
        {
            _tinkerNotice = "select an item first";
            return;
        }
        _tinker.PopulateTinkerList(
            _tinkerItemId,
            _tinkerSalvageChoice,
            MaximumTinkerAttempts(),
            MinimumTinkerPercent());
        _tinkerRowIndex = -1;
        _tinkerNotice = string.Create(
            CultureInfo.InvariantCulture,
            $"{_tinker.TinkerRows.Count} tink(s) planned.");
    };

    /// <summary>Starts the planned run.</summary>
    public Action StartTinker => () =>
    {
        if (_tinkerItemId == 0u)
        {
            _tinkerNotice = "select an item first";
            return;
        }
        _tinker.Start();
    };

    /// <summary>Stops the run and drops what is left of the plan.</summary>
    public Action StopTinker => () =>
    {
        _tinker.Stop();
        _tinkerNotice = "Stopped.";
    };

    /// <summary>Which attempt each row is, counting from one.</summary>
    public IReadOnlyList<string> TinkerRowNumbers =>
        _tinker.TinkerRows
            .Select(static row => row.Number.ToString(CultureInfo.InvariantCulture))
            .ToArray();

    /// <summary>The item each row spends its bag on.</summary>
    public IReadOnlyList<string> TinkerRowItems =>
        _tinker.TinkerRows.Select(static row => row.ItemName).ToArray();

    /// <summary>The bag each row spends, with its workmanship.</summary>
    public IReadOnlyList<string> TinkerRowSalvage =>
        _tinker.TinkerRows.Select(static row => row.SalvageName).ToArray();

    /// <summary>The odds each row was planned at.</summary>
    public IReadOnlyList<string> TinkerRowChances =>
        _tinker.TinkerRows.Select(static row => row.SuccessText).ToArray();

    /// <summary>Green for an attempt that came off, red for one that did not.</summary>
    public IReadOnlyList<uint> TinkerRowColors =>
        _tinker.TinkerRows.Select(RowColor).ToArray();

    /// <summary>Which row is highlighted.</summary>
    public int SelectedTinkerRowIndex => _tinkerRowIndex;

    /// <summary>Selects the row's item in the client, as the reference does.</summary>
    public Action<int> SelectTinkerRow => index =>
    {
        if ((uint)index >= (uint)_tinker.TinkerRows.Count)
            return;
        _tinkerRowIndex = index;
        _host.Selection.Select(_tinker.TinkerRows[index].ItemObjectId);
    };

    // ── the Imbue half ──────────────────────────────────────────────────

    /// <summary>The damage types a rend can be planned for.</summary>
    public IReadOnlyList<string> ImbueDamageTypeNames { get; } =
        TinkerJobManager.ImbueDamageTypes
            .Select(static row => row.Name)
            .ToArray();

    /// <summary>Which damage type is chosen.</summary>
    public string SelectedImbueDamageType => _imbueDamageTypeChoice;

    /// <summary>
    /// Chooses the damage type, and with it the salvage that rends it -- the
    /// reference moves the salvage choice along with the damage type.
    /// </summary>
    public Action<string> SelectImbueDamageType => name =>
    {
        if (!ImbueDamageTypeNames.Contains(name))
            return;
        _imbueDamageTypeChoice = name;
        string salvage = TinkerJobManager.DefaultSalvageName(name);
        if (_imbueSalvageNames.Contains(salvage))
            _imbueSalvageChoice = salvage;
        RefreshImbuePlan();
    };

    /// <summary>The salvages that imbue.</summary>
    public IReadOnlyList<string> ImbueSalvageNames => _imbueSalvageNames;

    /// <summary>Which of them is chosen.</summary>
    public string SelectedImbueSalvage => _imbueSalvageChoice;

    /// <summary>Chooses the salvage a rend will use.</summary>
    public Action<string> SelectImbueSalvage => name =>
    {
        if (!_imbueSalvageNames.Contains(name))
            return;
        _imbueSalvageChoice = name;
        RefreshImbuePlan();
    };

    /// <summary>Re-reads the packs and re-plans the chosen rend.</summary>
    public Action RefreshImbueList => () => RefreshImbuePlan();

    /// <summary>
    /// Plans one rend for every weapon that can still take one, each with the
    /// salvage that matches what it strikes with.
    /// </summary>
    public Action RendAllImbue => () =>
    {
        _tinker.PopulateRendAll();
        _imbueRowIndex = -1;
        _tinkerNotice = string.Create(
            CultureInfo.InvariantCulture,
            $"{_tinker.ImbueRows.Count} rend(s) planned.");
    };

    /// <summary>Starts the planned rends.</summary>
    public Action StartImbue => () => _tinker.Start();

    /// <summary>Stops the run and drops what is left of the plan.</summary>
    public Action StopImbue => () =>
    {
        _tinker.Stop();
        _tinkerNotice = "Stopped.";
    };

    /// <summary>Which rend each row is, counting from one.</summary>
    public IReadOnlyList<string> ImbueRowNumbers =>
        _tinker.ImbueRows
            .Select(static row => row.Number.ToString(CultureInfo.InvariantCulture))
            .ToArray();

    /// <summary>The item each row rends.</summary>
    public IReadOnlyList<string> ImbueRowItems =>
        _tinker.ImbueRows.Select(static row => row.ItemName).ToArray();

    /// <summary>The bag each row spends, with its workmanship.</summary>
    public IReadOnlyList<string> ImbueRowSalvage =>
        _tinker.ImbueRows.Select(static row => row.SalvageName).ToArray();

    /// <summary>The odds each row was planned at.</summary>
    public IReadOnlyList<string> ImbueRowChances =>
        _tinker.ImbueRows.Select(static row => row.SuccessText).ToArray();

    /// <summary>Green for a rend that came off, red for one that did not.</summary>
    public IReadOnlyList<uint> ImbueRowColors =>
        _tinker.ImbueRows.Select(RowColor).ToArray();

    /// <summary>Which row is highlighted.</summary>
    public int SelectedImbueRowIndex => _imbueRowIndex;

    /// <summary>Selects the row's item in the client, as the reference does.</summary>
    public Action<int> SelectImbueRow => index =>
    {
        if ((uint)index >= (uint)_tinker.ImbueRows.Count)
            return;
        _imbueRowIndex = index;
        _host.Selection.Select(_tinker.ImbueRows[index].ItemObjectId);
    };

    private void RefreshImbuePlan()
    {
        _tinker.PopulateImbueList(_imbueDamageTypeChoice, _imbueSalvageChoice);
        _imbueRowIndex = -1;
        _tinkerNotice = string.Create(
            CultureInfo.InvariantCulture,
            $"{_tinker.ImbueRows.Count} rend(s) planned.");
    }

    // ── the commands ────────────────────────────────────────────────────

    /// <summary>
    /// <c>/vt autotinker</c>: work through whatever the page has planned.
    /// </summary>
    private void StartAutoTinker() => _tinker.Start();

    /// <summary>
    /// <c>/vt getjob</c>: the queue as it stands, one line per item and one
    /// per bag still waiting on it.
    /// </summary>
    private void PrintTinkerJobs()
    {
        _tinker.ScanInventory();
        foreach (string line in _tinker.DescribeJobs())
            WriteVtank(line);
    }

    /// <summary>
    /// <c>/vt tinkcalc</c>: what the chosen salvage is worth on the item in
    /// hand, and for a melee weapon how the ten attempts split between
    /// granite and iron.
    /// </summary>
    private void PrintTinkerCalculation()
    {
        _tinker.ScanInventory();
        if (!TryTinkerCalcItem(out PluginInventoryItem item))
        {
            WriteVtank("Nothing selected");
            return;
        }

        if (TryTinkerCalcSalvage(item, out PluginInventoryItem bag))
        {
            double chance = _tinker.ChanceFor(bag, item, item.NumTimesTinkered);
            WriteVtank(string.Create(
                CultureInfo.InvariantCulture,
                $"{item.NumTimesTinkered + 1}: Applying ws {bag.SalvageWorkmanship} "
                + $"{TinkerJobManager.SalvageName(bag)} {bag.ObjectId} to {item.Name} "
                + $"{item.ObjectId} with a successChance of {chance.ToString("P", CultureInfo.InvariantCulture)}"));
        }
        else
        {
            WriteVtank("no salvage matches...  quitting");
        }

        if (item.ObjectClass != PluginObjectClass.MeleeWeapon)
        {
            WriteVtank("tinkcalc only currently works with melee weapons");
            return;
        }
        (int granite, int iron, double finalDamage) =
            TinkerJobManager.BestGraniteIron(item);
        WriteVtank(string.Create(
            CultureInfo.InvariantCulture,
            $"{item.Name}: Final max damage: {finalDamage:N2}, {granite} granite, {iron} iron"));
    }

    /// <summary>
    /// The item a calculation runs against: the one on the page when it has
    /// one, otherwise whatever is selected.
    /// </summary>
    private bool TryTinkerCalcItem(out PluginInventoryItem item)
    {
        if (_tinkerItemId != 0u && _tinker.TryOwned(_tinkerItemId, out item))
            return true;
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected != 0u
            && _tinker.TryOwned(selected, out item)
            && item.ObjectClass != PluginObjectClass.Salvage)
        {
            return true;
        }
        item = default;
        return false;
    }

    /// <summary>
    /// The salvage a calculation runs with: the selected bag when one is
    /// selected, otherwise the cheapest bag of the material the page has
    /// chosen. Granite/Iron resolves to whichever is worth more right now.
    /// </summary>
    private bool TryTinkerCalcSalvage(
        in PluginInventoryItem item,
        out PluginInventoryItem bag)
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        if (selected != 0u
            && _tinker.TryOwned(selected, out bag)
            && bag.ObjectClass == PluginObjectClass.Salvage)
        {
            return true;
        }
        bag = default;
        int material = string.Equals(
                _tinkerSalvageChoice,
                TinkerJobManager.GraniteIron,
                StringComparison.Ordinal)
            ? TinkerCalc.IronBeatsGranite(
                TinkerJobManager.DamageCeiling(item),
                item.DamageVariance)
                ? (int)TinkerMaterial.Iron
                : (int)TinkerMaterial.Granite
            : TinkerMaterials.Id(_tinkerSalvageChoice);
        return material != 0 && _tinker.TryCheapestBag(material, out bag);
    }

    private static uint RowColor(TinkerListRow row) => row.Succeeded switch
    {
        true => TinkerSuccessColor,
        false => TinkerFailureColor,
        _ => TinkerRowColor,
    };
}
