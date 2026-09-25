using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The UB settings page: one list of settings, one description, one edit field, and
/// two sub-pages for the two value shapes a single field cannot hold.
/// </summary>
/// <remarks>
/// A hundred and fifty-four settings cannot be drawn as a hundred and
/// fifty-four controls in a window this size; at this project's own
/// conventions that is upwards of five hundred elements against the two
/// hundred odd in the whole rest of the window. So the page is the shape the
/// advanced page already proved: a filtered list, a category filter, and one
/// shared draft field, with the catalogue supplying every row.
/// </remarks>
internal sealed partial class MossTankPanel
{
    /// <summary>The first row of the category list, which filters nothing.</summary>
    private const string UbAllCategories = "(all)";

    private UbSettingStore _ubStore = null!;
    private UbSettingCatalog _ubCatalog = null!;
    private IReadOnlyList<UbSetting> _ubRows = [];
    private IReadOnlyList<string> _ubRowNames = [];
    private IReadOnlyList<string> _ubRowValues = [];
    private IReadOnlyList<string> _ubCategoryNames = [];
    private int _selectedUbCategory;
    private int _selectedUbRow;
    private string _ubFilter = string.Empty;
    private string _ubDraft = string.Empty;
    private string _ubNotice = string.Empty;
    private string _ubProfileNameDraft = string.Empty;
    private string _ubDescriptionText = string.Empty;
    private IReadOnlyList<string> _ubDescriptionLines = [];

    private bool _ubColorVisible;
    private string _ubColorName = string.Empty;
    private uint _ubColorValue;

    private bool _ubListVisible;
    private string _ubListName = string.Empty;
    private readonly List<string> _ubListEntries = [];
    private int _selectedUbListRow;
    private string _ubListDraft = string.Empty;
    private string _ubListNotice = string.Empty;

    private void InitializeUbSettings(IPluginHost host)
    {
        _ubStore = new UbSettingStore(host.VtankProfiles, new UbSettingHostLog(host));
        _ubStore.Bind(host.Automation.Character.WorldName, host.Automation.Character.Name);
        PrepareCharacterStorageFolders(
            host.Automation.Character.WorldName,
            host.Automation.Character.Name);
        _ubCatalog = new UbSettingCatalog(_ubStore, UbLiveOwners());
        _ubCategoryNames = [UbAllCategories, .. _ubCatalog.Categories];
        // The sharing switch and the peer tag are read from the catalogue on
        // every frame, so an edit on this page applies at once, and a value
        // saved at a wider tier shows through the same way it does here.
        _combat.BindCastSharing(
            () => _ubCatalog.Require("Sharing.Vitals").Get().Boolean,
            () => _ubCatalog.Require("Sharing.CastTag").Get().Text);
        // The vendor run reads its rows the same way, on every tick, so a
        // switch flipped mid-visit is honoured on the next round.
        _vendorTrade = new VendorTradeController(host, _ubStore);
        _vendorTrade.BindActionLocks(_actionLocks);
        _vendorTrade.BindSettings(new VendorTradeSettings
        {
            Enabled = () => _ubCatalog.Require("AutoVendor.Enabled").Get().Boolean,
            EnableBuying = () => _ubCatalog.Require("AutoVendor.EnableBuying").Get().Boolean,
            EnableSelling = () => _ubCatalog.Require("AutoVendor.EnableSelling").Get().Boolean,
            TestMode = () => _ubCatalog.Require("AutoVendor.TestMode").Get().Boolean,
            Think = () => _ubCatalog.Require("AutoVendor.Think").Get().Boolean,
            ShowMerchantInfo = () => _ubCatalog.Require("AutoVendor.ShowMerchantInfo").Get().Boolean,
            OnlyFromMainPack = () => _ubCatalog.Require("AutoVendor.OnlyFromMainPack").Get().Boolean,
            Tries = () => _ubCatalog.Require("AutoVendor.Tries").Get().AsInt32(),
            TriesTimeMilliseconds = () => _ubCatalog.Require("AutoVendor.TriesTime").Get().AsInt32(),
        });
        // The one stack-and-cram owner does the run's opening pass: the
        // scheduler is suspended while the run owns the character, so the
        // two never drive it at once.
        _vendorTrade.BindStackCramPass((elapsed, first) =>
        {
            if (first)
                _inventoryMaintenance.Reset();
            return _inventoryMaintenance.Tick(elapsed, canAct: true);
        });
        // While the macro follows its route, the route's open-vendor
        // waypoint is the one thing that opens vendors.
        _vendorTrade.BindRouteOwnsVendorOpen(
            () => _scheduler.IsRunning && _navigationSettings.Enabled);
        _vendorTrade.BindProtectedItems(
            () => _combatSettings.CombatItemObjectIds as IReadOnlySet<uint>
                ?? new HashSet<uint>(_combatSettings.CombatItemObjectIds));
        // Spending experience has nothing to do with the macro, so it is
        // not a rule and takes no lock; it simply refuses to run while the
        // macro is enabled. Its policy is the catalogue's lines row, read
        // and written in place so an import shows on the page at once.
        _experienceSpend = new ExperienceSpendController(host, () => _combat.Enabled);
        _experienceSpend.BindSettings(new ExperienceSpendSettings
        {
            StopBeforeMax = () => _ubCatalog.Require("AutoXp.StopBeforeMax").Get().AsInt32(),
            TriesTimeMilliseconds = () => _ubCatalog.Require("AutoXp.TriesTime").Get().AsInt32(),
            MaxXpChunk = () => (long)_ubCatalog.Require("AutoXp.MaxXpChunk").Get().Number,
            PolicyLines = () => _ubCatalog.Require("AutoXp.Policy").Get().Items,
            SetPolicyLines = lines =>
            {
                UbSetting row = _ubCatalog.Require("AutoXp.Policy");
                row.Set(UbSettingValue.FromCollection(lines));
                RefreshUbSettings();
            },
        });
        // The name tags read their thirty-seven rows the same way, as one
        // value polled a few times a second, so an edit on this page shows
        // over the next object to be refreshed.
        _nametags = new NametagController(
            host.Automation.Objects,
            host.Automation.Labels,
            () => host.Automation.Character.ObjectId,
            () => host.Automation.Navigation.Snapshot is { IsAvailable: true } snapshot
                ? snapshot.Position
                : null,
            () => host.Automation.Allegiance.Snapshot.MonarchObjectId,
            ReadNametagSettings);
        _nametags.Attach(host.Events);
        InitializeDisplayTools(host);
        // Dressing by profile is a prologue owner beside the give, count and
        // vendor runs: it refuses while the combat controller has a target,
        // since that controller owns what is wielded during a fight, and it
        // holds the item-use lock for its whole run.
        _equipProfile = new EquipProfileController(host, _ubStore, () => _combat.HasTarget);
        _equipProfile.BindActionLocks(_actionLocks);
        _equipProfile.BindSettings(new EquipProfileSettings
        {
            Think = () => _ubCatalog.Require("EquipmentManager.Think").Get().Boolean,
            Debug = () => UbDebug,
        });
        InitializeDungeonMap(host);
        InitializeAliases(host);
        InitializeTinkering(host);
        RefreshUbSettings();
    }

    private NametagController _nametags = null!;

    private NametagSettings ReadNametagSettings() => new(
        _ubCatalog.Require("Nametags.Enabled").Get().Boolean,
        _ubCatalog.Require("Nametags.MaxRange").Get().AsSingle(),
        ReadNametagGroup("Player"),
        ReadNametagGroup("Pet"),
        ReadNametagGroup("AllegiancePlayer"),
        ReadNametagGroup("Portal"),
        ReadNametagGroup("Npc"),
        ReadNametagGroup("Vendor"),
        ReadNametagGroup("Monster"));

    private NametagGroupSettings ReadNametagGroup(string group)
    {
        string prefix = $"Nametags.{group}.";
        return new NametagGroupSettings(
            _ubCatalog.Require(prefix + "Enabled").Get().Boolean,
            _ubCatalog.Require(prefix + "TagColor").Get().AsColor(),
            _ubCatalog.Require(prefix + "TagSize").Get().AsSingle(),
            _ubCatalog.Require(prefix + "TickerColor").Get().AsColor(),
            _ubCatalog.Require(prefix + "TickerSize").Get().AsSingle());
    }

    /// <summary>
    /// Points the three files at whoever is logged in now. The same
    /// character returning is no change; a different one opens that
    /// character's own file and whichever profile they last chose.
    /// </summary>
    private void BindUbSettingsToCharacter(string characterName)
    {
        _ubStore.Bind(_host.Automation.Character.WorldName, characterName);
        PrepareCharacterStorageFolders(_host.Automation.Character.WorldName, characterName);
        RefreshUbSettings();
    }

    public bool UbSettingsVisible => UbSettingsSelected;

    public bool UbSettingsTabEnabled => true;

    public IReadOnlyList<string> UbCategoryNames => _ubCategoryNames;

    public int SelectedUbCategoryIndex => _selectedUbCategory;

    public Action<int> SelectUbCategory => index =>
    {
        _selectedUbCategory = ClampRow(index, _ubCategoryNames.Count);
        _selectedUbRow = 0;
        RefreshUbSettings();
    };

    public IReadOnlyList<string> UbSettingNames => _ubRowNames;

    public IReadOnlyList<string> UbSettingValues => _ubRowValues;

    public int SelectedUbSettingIndex => _selectedUbRow;

    public Action<int> SelectUbSetting => index =>
    {
        _selectedUbRow = ClampRow(index, _ubRows.Count);
        RefreshUbSettings();
    };

    /// <summary>
    /// A click on the value itself does the shortest thing that shape
    /// allows: a switch flips, a choice steps on, a colour and a list open
    /// the sub-page they need, and anything else simply selects the row so
    /// it can be typed over.
    /// </summary>
    public Action<int> ClickUbSettingValue => index =>
    {
        if ((uint)index >= (uint)_ubRows.Count)
            return;
        _selectedUbRow = index;
        UbSetting row = _ubRows[index];
        switch (row.Kind)
        {
            case UbSettingKind.Bool:
                row.Set(UbSettingValue.FromBool(!row.Get().Boolean));
                NoteUbChange(row, $"is now {row.Display()}");
                break;
            case UbSettingKind.Enum:
                CycleUbChoice(row);
                break;
            case UbSettingKind.Color:
                OpenUbColorEditor(row);
                break;
            case UbSettingKind.Collection:
                OpenUbListEditor(row);
                break;
        }
        RefreshUbSettings();
    };

    public string UbFilterText => _ubFilter;

    public Action<string> SetUbFilterText => text =>
    {
        _ubFilter = text;
        _selectedUbRow = 0;
        RefreshUbSettings();
    };

    /// <summary>
    /// The width of the description labels beside the list, which the
    /// description is wrapped to: a label draws one line and neither wraps
    /// nor clips, so the page hands it lines that fit.
    /// </summary>
    internal const double UbDescriptionWidth = 308d;

    /// <summary>The selected setting's description, one line a label, wrapped to the label's width.</summary>
    public string UbSettingDescription => UbDescriptionLine(0);

    public string UbSettingDescriptionLine2 => UbDescriptionLine(1);

    public string UbSettingDescriptionLine3 => UbDescriptionLine(2);

    public string UbSettingDescriptionLine4 => UbDescriptionLine(3);

    private string UbDescriptionLine(int index)
    {
        string text = SelectedUbRow() is { } row
            ? $"{row.Name}: {row.Definition.Summary}"
            : "No setting matches the filter.";
        // The labels read their line every frame; the wrap is redone only
        // when the sentence changes.
        if (!string.Equals(text, _ubDescriptionText, StringComparison.Ordinal))
        {
            _ubDescriptionText = text;
            _ubDescriptionLines = InterfaceFontMetrics.Wrap(text, UbDescriptionWidth);
        }
        return index < _ubDescriptionLines.Count ? _ubDescriptionLines[index] : string.Empty;
    }

    /// <summary>Which of the three files an edit to this row lands in.</summary>
    public string UbSettingScopeText => SelectedUbRow() is { } row
        ? row.Scope switch
        {
            UbSettingScope.Global => "Saved for this installation.",
            UbSettingScope.Character => "Saved for this character only.",
            _ => $"Saved in profile '{_ubStore.ProfileName}'.",
        }
        : string.Empty;

    public string UbSettingNotice => _ubNotice;

    public string UbSettingValueDraft => _ubDraft;

    public Action<string> SetUbSettingValueDraft => text => _ubDraft = text;

    public Action<string> SubmitUbSetting => text =>
    {
        _ubDraft = text;
        ApplyUbDraft();
    };

    public Action SubmitUbSettingDraft => ApplyUbDraft;

    /// <summary>
    /// Puts a row back to what the catalogue says, by taking this row's own
    /// value out of its file rather than writing the default into it: a
    /// value set for the whole installation should show through again, not
    /// be shadowed by a copy of the default.
    /// </summary>
    public Action ResetUbSettingToDefault => () =>
    {
        if (SelectedUbRow() is not { } row)
            return;
        _ubCatalog.ResetToDefault(row);
        NoteUbChange(row, $"is back to {row.Display()}");
        RefreshUbSettings();
    };

    public IReadOnlyList<string> UbProfileNames => _ubStore.AvailableProfiles();

    public string SelectedUbProfile => _ubStore.ProfileName;

    public Action<string> SelectUbProfile => name =>
    {
        _ubStore.SelectProfile(name);
        _ubNotice = $"Profile '{_ubStore.ProfileName}' is open.";
        RefreshUbSettings();
    };

    public string UbProfileNameDraft => _ubProfileNameDraft;

    public Action<string> SetUbProfileNameDraft => text => _ubProfileNameDraft = text;

    public Action<string> CreateNamedUbProfile => text =>
    {
        _ubProfileNameDraft = text;
        CreateUbProfileCore(copyCurrent: false);
    };

    public Action CreateUbProfile => () => CreateUbProfileCore(copyCurrent: false);

    public Action CopyUbProfile => () => CreateUbProfileCore(copyCurrent: true);

    /// <summary>
    /// Makes the named profile and opens it, the way the other profile
    /// pickers on this window do: a name field, one button that starts an
    /// empty set and one that carries the open set into it. Without these
    /// the menu can only ever offer what already exists, which on a fresh
    /// installation is nothing but the default.
    /// </summary>
    private void CreateUbProfileCore(bool copyCurrent)
    {
        if (!_ubStore.CreateProfile(_ubProfileNameDraft, copyCurrent, out string notice))
        {
            _ubNotice = notice;
            return;
        }
        _ubProfileNameDraft = string.Empty;
        _ubNotice = notice;
        RefreshUbSettings();
    }

    private UbSetting? SelectedUbRow() =>
        _ubRows.Count == 0
            ? null
            : _ubRows[Math.Clamp(_selectedUbRow, 0, _ubRows.Count - 1)];

    private void ApplyUbDraft()
    {
        if (SelectedUbRow() is not { } row)
            return;
        if (!UbSettingValue.TryParse(row.Kind, _ubDraft, out UbSettingValue value))
        {
            _ubNotice = $"'{_ubDraft}' is not a {UbShapeName(row.Kind)}; "
                + $"{row.Name} is unchanged.";
            return;
        }
        row.Set(value);
        NoteUbChange(row, $"is now {row.Display()}");
        RefreshUbSettings();
    }

    /// <summary>
    /// What a change to one row reports. A row whose file did not read
    /// still changes for this session, but saying only that it changed
    /// would be a lie about what survives the client closing, so the
    /// sentence says which of the two happened. Nothing is written over a
    /// file that would not read: it is the only copy of what somebody
    /// configured, and it can be repaired.
    /// </summary>
    private void NoteUbChange(UbSetting row, string clause) =>
        _ubNotice = row.CanSave
            ? $"{row.Name} {clause}."
            : $"{row.Name} {clause} for this session only; its settings "
                + "file could not be read, so nothing is saved until that "
                + "file is repaired.";

    private void CycleUbChoice(UbSetting row)
    {
        IReadOnlyList<VtankEnumValue> choices = row.Definition.Choices;
        if (choices.Count == 0)
            return;
        int current = row.Get().AsInt32();
        int at = 0;
        for (int i = 0; i < choices.Count; i++)
            if (choices[i].Value == current)
            {
                at = i;
                break;
            }
        row.Set(UbSettingValue.FromChoice(choices[(at + 1) % choices.Count].Value));
        NoteUbChange(row, $"is now {row.Display()}");
    }

    private static string UbShapeName(UbSettingKind kind) => kind switch
    {
        UbSettingKind.Bool => "True or False",
        UbSettingKind.Int or UbSettingKind.Enum => "whole number",
        UbSettingKind.Single or UbSettingKind.Double => "number",
        UbSettingKind.Color => "colour",
        _ => "value",
    };

    private void RefreshUbSettings()
    {
        string category = _selectedUbCategory > 0
            && _selectedUbCategory < _ubCategoryNames.Count
                ? _ubCategoryNames[_selectedUbCategory]
                : string.Empty;
        string filter = _ubFilter.Trim();

        _ubRows = _ubCatalog.Settings
            .Where(row =>
                (category.Length == 0
                    || string.Equals(
                        row.Definition.Category, category, StringComparison.Ordinal))
                && (filter.Length == 0
                    || row.Name.Contains(filter, StringComparison.OrdinalIgnoreCase)
                    || row.Definition.Summary.Contains(
                        filter, StringComparison.OrdinalIgnoreCase)))
            .ToArray();
        _selectedUbRow = ClampRow(_selectedUbRow, _ubRows.Count);
        _ubRowNames = _ubRows.Select(static row => row.Name).ToArray();
        _ubRowValues = _ubRows.Select(static row => row.Display()).ToArray();
        _ubDraft = SelectedUbRow()?.Get().ToStorageString() ?? string.Empty;
    }

    // ---- The colour sub-page -------------------------------------------
    //
    // The markup has no colour control and this plugin may not add one to
    // the client, so a colour is edited as its four channels plus the same
    // #AARRGGBB text the rest of the window is written in. The swatch is a
    // group whose background is bound to the draft: a colour attribute
    // takes a binding to a uint holding 0xAARRGGBB, re-read every frame.

    public bool UbColorVisible => _ubColorVisible;

    /// <summary>The draft as the markup's colour binding reads it: 0xAARRGGBB.</summary>
    public uint UbColorSwatch => _ubColorValue;

    public string UbColorTarget => _ubColorName.Length == 0
        ? string.Empty
        : $"{_ubColorName} = #{_ubColorValue:X8}";

    public float UbColorAlpha => (_ubColorValue >> 24) & 0xFFu;

    public float UbColorRed => (_ubColorValue >> 16) & 0xFFu;

    public float UbColorGreen => (_ubColorValue >> 8) & 0xFFu;

    public float UbColorBlue => _ubColorValue & 0xFFu;

    public string UbColorAlphaText => UbChannelText(UbColorAlpha);

    public string UbColorRedText => UbChannelText(UbColorRed);

    public string UbColorGreenText => UbChannelText(UbColorGreen);

    public string UbColorBlueText => UbChannelText(UbColorBlue);

    public Action<float> SetUbColorAlpha => value => SetUbChannel(24, value);

    public Action<float> SetUbColorRed => value => SetUbChannel(16, value);

    public Action<float> SetUbColorGreen => value => SetUbChannel(8, value);

    public Action<float> SetUbColorBlue => value => SetUbChannel(0, value);

    public string UbColorHex => $"#{_ubColorValue:X8}";

    public Action<string> SetUbColorHex => text => ReadUbColorHex(text);

    public Action<string> SubmitUbColorHex => text => ReadUbColorHex(text);

    public Action ApplyUbColor => () =>
    {
        if (_ubColorName.Length != 0 && _ubCatalog.TryGet(_ubColorName, out UbSetting row))
        {
            row.Set(UbSettingValue.FromColor(_ubColorValue));
            NoteUbChange(row, $"is now {row.Display()}");
        }
        _ubColorVisible = false;
        RefreshUbSettings();
    };

    public Action HideUbColorEditor => () => _ubColorVisible = false;

    public Action ShowUbColorEditor => () =>
    {
        if (SelectedUbRow() is { Kind: UbSettingKind.Color } row)
            OpenUbColorEditor(row);
        else
            _ubNotice = "That setting is not a colour.";
    };

    public Action ResetUbColorToDefault => () =>
    {
        if (_ubColorName.Length != 0 && _ubCatalog.TryGet(_ubColorName, out UbSetting row))
            _ubColorValue = row.Definition.Default.AsColor();
    };

    private void OpenUbColorEditor(UbSetting row)
    {
        _ubColorName = row.Name;
        _ubColorValue = row.Get().AsColor();
        _ubColorVisible = true;
        _ubListVisible = false;
    }

    private static string UbChannelText(float channel) =>
        ((int)channel).ToString(CultureInfo.InvariantCulture);

    private void SetUbChannel(int shift, float value)
    {
        uint channel = (uint)Math.Clamp((int)Math.Round(value), 0, 255);
        _ubColorValue = (_ubColorValue & ~(0xFFu << shift)) | (channel << shift);
    }

    private void ReadUbColorHex(string text)
    {
        if (UbSettingValue.TryParse(UbSettingKind.Color, text, out UbSettingValue value))
            _ubColorValue = value.AsColor();
    }

    // ---- The list sub-page ---------------------------------------------
    //
    // One sub-page serves every list setting -- the experience policy, the
    // tracked items, this character's tags, the aliases and the event
    // handlers -- because each of them is a list of lines and a second copy
    // of this page per list would be five pages to keep in step.

    public bool UbListVisible => _ubListVisible;

    public string UbListTitle => _ubListName.Length == 0
        ? string.Empty
        : $"{_ubListName} ({_ubListEntries.Count})";

    public IReadOnlyList<string> UbListRows => _ubListEntries;

    public int SelectedUbListIndex => _selectedUbListRow;

    public Action<int> SelectUbListRow => index =>
    {
        _selectedUbListRow = ClampRow(index, _ubListEntries.Count);
        _ubListDraft = _ubListEntries.Count == 0
            ? string.Empty
            : _ubListEntries[_selectedUbListRow];
    };

    public string UbListDraft => _ubListDraft;

    public Action<string> SetUbListDraft => text => _ubListDraft = text;

    public Action<string> AddUbListEntry => text =>
    {
        _ubListDraft = text;
        AddUbListEntryCore();
    };

    public Action AddUbListEntryFromDraft => AddUbListEntryCore;

    /// <summary>
    /// What is wrong with the open list, or nothing. The alias list is the
    /// one list with a shape: a line that is not an alias is refused here
    /// with the reason, and one already in the file is named by its line,
    /// so it can be repaired rather than lost.
    /// </summary>
    public string UbListNotice => _ubListNotice;

    public Action ReplaceUbListEntry => () =>
    {
        if (_ubListEntries.Count == 0 || _ubListDraft.Trim().Length == 0)
            return;
        string entry = _ubListDraft.Trim();
        if (!AdmitUbListEntry(entry))
            return;
        _ubListEntries[
            Math.Clamp(_selectedUbListRow, 0, _ubListEntries.Count - 1)] = entry;
        SaveUbList();
    };

    public Action RemoveUbListEntry => () =>
    {
        if (_ubListEntries.Count == 0)
            return;
        _ubListEntries.RemoveAt(Math.Clamp(_selectedUbListRow, 0, _ubListEntries.Count - 1));
        _selectedUbListRow = ClampRow(_selectedUbListRow, _ubListEntries.Count);
        SaveUbList();
    };

    public Action HideUbListEditor => () => _ubListVisible = false;

    public Action ShowUbListEditor => () =>
    {
        if (SelectedUbRow() is { Kind: UbSettingKind.Collection } row)
            OpenUbListEditor(row);
        else
            _ubNotice = "That setting is not a list.";
    };

    private void OpenUbListEditor(UbSetting row)
    {
        _ubListName = row.Name;
        _ubListEntries.Clear();
        _ubListEntries.AddRange(row.Get().Items);
        _selectedUbListRow = 0;
        _ubListDraft = string.Empty;
        _ubListVisible = true;
        _ubColorVisible = false;
        NoteUbListShape();
    }

    private void AddUbListEntryCore()
    {
        string entry = _ubListDraft.Trim();
        if (entry.Length == 0 || !AdmitUbListEntry(entry))
            return;
        _ubListEntries.Add(entry);
        _selectedUbListRow = _ubListEntries.Count - 1;
        _ubListDraft = string.Empty;
        SaveUbList();
    }

    /// <summary>
    /// A list is written the moment it changes, the way every other row on
    /// this page is: a sub-page that only saved on the way out would lose an
    /// edit to a client that closed unexpectedly.
    /// </summary>
    private void SaveUbList()
    {
        if (_ubListName.Length == 0 || !_ubCatalog.TryGet(_ubListName, out UbSetting row))
            return;
        row.Set(UbSettingValue.FromCollection(_ubListEntries));
        NoteUbChange(row, $"has {_ubListEntries.Count} entries");
        NoteUbListShape();
        RefreshUbSettings();
    }

    private bool UbListIsTheAliasList =>
        string.Equals(_ubListName, "Aliases.DefinedAliases", StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// Whether one typed line may go into the open list. Only the alias
    /// list has a shape to hold it to; refusing says why on the page.
    /// </summary>
    private bool AdmitUbListEntry(string entry)
    {
        if (!UbListIsTheAliasList || AliasTable.TryParseLine(entry, out _, out string? problem))
            return true;
        _ubListNotice = problem;
        return false;
    }

    /// <summary>
    /// Names the lines of the open list that are not what the list holds,
    /// so a file edited by hand is repaired on the page rather than lost.
    /// </summary>
    private void NoteUbListShape()
    {
        if (!UbListIsTheAliasList)
        {
            _ubListNotice = string.Empty;
            return;
        }
        IReadOnlyList<string> problems = AliasTable.Parse(_ubListEntries).Problems;
        _ubListNotice = problems.Count == 0
            ? string.Empty
            : problems.Count == 1
                ? problems[0]
                : $"{problems[0]} ({problems.Count - 1} more)";
    }

    /// <summary>
    /// The rows whose value is already owned by something running, rather
    /// than by the settings files. Their get/set pair reaches that owner and
    /// saves the profile it belongs to, so the page and the macro can never
    /// hold two different numbers.
    /// </summary>
    /// <remarks>
    /// These five shipped with no editor at all: the four numbers that pace
    /// handing items over, and how many times a jump is tried. They live on
    /// the running settings and are written with the macro profile, so the
    /// catalogue must not keep a second copy of them in its own files.
    /// </remarks>
    private IReadOnlyDictionary<string, UbSettingBinding> UbLiveOwners() =>
        new Dictionary<string, UbSettingBinding>(StringComparer.OrdinalIgnoreCase)
        {
            ["ItemGiver.Range"] = LiveUbDecimal(
                () => _inventorySettings.GiveRangeMeters,
                value => _inventorySettings.GiveRangeMeters = value,
                0d,
                1000d),
            ["ItemGiver.Delay"] = LiveUbDecimal(
                () => _inventorySettings.GiveDelaySeconds,
                value => _inventorySettings.GiveDelaySeconds = value,
                0d,
                60d),
            ["ItemGiver.BusyRetryLimit"] = LiveUbWhole(
                () => _inventorySettings.GiveBusyRetryLimit,
                value => _inventorySettings.GiveBusyRetryLimit = value,
                1,
                100),
            ["ItemGiver.FailureLimit"] = LiveUbWhole(
                () => _inventorySettings.GiveFailureLimit,
                value => _inventorySettings.GiveFailureLimit = value,
                0,
                100),
            ["Jumper.Attempts"] = LiveUbWhole(
                () => _navigationSettings.JumpAttempts,
                value => _navigationSettings.JumpAttempts = value,
                1,
                20),
            // The event handlers are rules in the meta profile, shown here
            // as lines; there is no second copy in the settings files. A
            // line that cannot be read is said so in chat and dropped.
            ["GameEvents.Handlers"] = new(
                () => UbSettingValue.FromCollection(GameEventHandlers.Lines(_metaProfile)),
                value =>
                {
                    foreach (string rejected in GameEventHandlers.Apply(_metaProfile, value.Items))
                        _host.Automation.Chat.PostSystemMessage(rejected);
                    SaveMetaProfile();
                    _selectedMetaRule = ClampRow(_selectedMetaRule, _metaProfile.Rules.Count);
                    RefreshMetaEditor();
                }),
            // The handler set IS the meta profile, so its profile row is the
            // meta profile picker: [character] is the per-character profile.
            ["GameEvents.Profile"] = new(
                () => UbSettingValue.FromText(
                    _metaProfiles.Selected.Equals(
                        MossTankMetaProfileStore.ByCharacter, StringComparison.OrdinalIgnoreCase)
                        ? "[character]"
                        : _metaProfiles.Selected),
                value => SelectMetaProfileCore(
                    value.Text.Trim().Equals("[character]", StringComparison.OrdinalIgnoreCase)
                        ? MossTankMetaProfileStore.ByCharacter
                        : value.Text.Trim())),
            // The alias list is a file of its own, named by the profile row.
            ["Aliases.DefinedAliases"] = AliasListBinding(),
            // Two choices already made here: they read what is in force,
            // and a write leaves it.
            ["VTank.PatchExpressionEngine"] = new(
                static () => UbSettingValue.FromBool(true),
                static _ => { }),
            ["InventoryManager.TreatStackAsSingleItem"] = new(
                static () => UbSettingValue.FromBool(false),
                static _ => { }),
            // The profile name is the store's choice for this character.
            ["Plugin.SettingsProfile"] = new(
                () => UbSettingValue.FromText(_ubStore.ProfileName),
                value =>
                {
                    _ubStore.SelectProfile(value.Text.Trim());
                    RefreshUbSettings();
                }),
        };

    /// <summary>
    /// A row over a running decimal setting. The bound is the one the macro
    /// itself is held to, applied here so the page shows the value that will
    /// be used rather than the one that was typed.
    /// </summary>
    private UbSettingBinding LiveUbDecimal(
        Func<double> get,
        Action<double> set,
        double lowest,
        double highest) =>
        new(
            () => UbSettingValue.FromDouble(get()),
            value =>
            {
                set(Math.Clamp(value.AsDouble(), lowest, highest));
                SaveProfile();
            });

    /// <summary>A row over a running whole-number setting.</summary>
    private UbSettingBinding LiveUbWhole(
        Func<int> get,
        Action<int> set,
        int lowest,
        int highest) =>
        new(
            () => UbSettingValue.FromInt(get()),
            value =>
            {
                set(Math.Clamp(value.AsInt32(), lowest, highest));
                SaveProfile();
            });

    private sealed class UbSettingHostLog(IPluginHost host) : IUbSettingLog
    {
        public void Warn(string message) => host.Log.Warn(message);
    }
}
