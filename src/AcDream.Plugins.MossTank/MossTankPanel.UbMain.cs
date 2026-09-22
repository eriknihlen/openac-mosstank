namespace AcDream.Plugins.MossTank;

/// <summary>
/// The UB page: the short front page of on/off buttons for the tools
/// themselves, with the catalogue one button away.
/// </summary>
/// <remarks>
/// <para>
/// The tool this is brought over from opens on a page of push buttons, one
/// a tool, each showing whether that tool is running and flipping it when
/// pressed. It is the page people actually use; the hundred and fifty-odd
/// settings behind it are opened once and then left alone. So the page is
/// reproduced here as its own tab, with the buttons in the same three
/// columns and the same reading order.
/// </para>
/// <para>
/// Every button here is a view of a catalogue row, not a copy of one: it
/// reads through <see cref="UbSettingCatalog"/> on every frame and writes
/// back through the same <c>Set</c> the settings page's value column calls,
/// so the row's scope decides which of the three files an edit lands in and
/// an unwritable file is reported here in the same words. Nothing on this
/// page holds a second copy of a switch, which is why flipping one here and
/// looking at it on the settings page can never disagree.
/// </para>
/// <para>
/// The original marks a button's state with a green tick or a red cross
/// drawn over its caption. The markup here draws no overlay on a button, so
/// the state is said in the caption and repeated in its colour, which is
/// the same information without a sprite.
/// </para>
/// </remarks>
internal sealed partial class MossTankPanel
{
    /// <summary>A switch that is on: the same green the rest of the window uses for "running".</summary>
    private const uint UbSwitchOnColor = 0xFF7CCB6Bu;

    /// <summary>A switch that is off: the window's ordinary dimmed label colour.</summary>
    private const uint UbSwitchOffColor = 0xFFB0A184u;

    public bool UbVisible => UbSelected;

    public bool UbTabEnabled => true;

    public string UbAutoVendorButtonText => UbSwitchCaption("Auto Vendor", "AutoVendor.Enabled");

    public uint UbAutoVendorButtonColor => UbSwitchColor("AutoVendor.Enabled");

    public Action ToggleUbAutoVendor => () => FlipUbSwitch("AutoVendor.Enabled");

    /// <summary>
    /// The test-mode button beside the vendor button, as narrow as the
    /// original's: a vendor run in test mode says what it would buy and sell
    /// and touches nothing.
    /// </summary>
    public string UbAutoVendorTestButtonText => UbShortSwitchCaption("T", "AutoVendor.TestMode");

    public uint UbAutoVendorTestButtonColor => UbSwitchColor("AutoVendor.TestMode");

    public Action ToggleUbAutoVendorTestMode => () => FlipUbSwitch("AutoVendor.TestMode");

    public string UbDungeonMapsButtonText => UbSwitchCaption("Dungeon Maps", "DungeonMaps.Enabled");

    public uint UbDungeonMapsButtonColor => UbSwitchColor("DungeonMaps.Enabled");

    public Action ToggleUbDungeonMaps => () => FlipUbSwitch("DungeonMaps.Enabled");

    public string UbLandscapeMapsButtonText =>
        UbSwitchCaption("Landscape Maps", "LandscapeMaps.Enabled");

    public uint UbLandscapeMapsButtonColor => UbSwitchColor("LandscapeMaps.Enabled");

    public Action ToggleUbLandscapeMaps => () => FlipUbSwitch("LandscapeMaps.Enabled");

    public string UbNametagsButtonText => UbSwitchCaption("Name Tags", "Nametags.Enabled");

    public uint UbNametagsButtonColor => UbSwitchColor("Nametags.Enabled");

    public Action ToggleUbNametags => () => FlipUbSwitch("Nametags.Enabled");

    /// <summary>
    /// One switch for both directions of sharing, as the catalogue has it:
    /// what this character tells the other clients on this computer and what
    /// it takes in from them.
    /// </summary>
    public string UbVitalSharingButtonText => UbSwitchCaption("Vital Sharing", "Sharing.Vitals");

    public uint UbVitalSharingButtonColor => UbSwitchColor("Sharing.Vitals");

    public Action ToggleUbVitalSharing => () => FlipUbSwitch("Sharing.Vitals");

    public string UbNetworkUiButtonText => UbSwitchCaption("Network UI", "NetworkUI.Enabled");

    public uint UbNetworkUiButtonColor => UbSwitchColor("NetworkUI.Enabled");

    public Action ToggleUbNetworkUi => () => FlipUbSwitch("NetworkUI.Enabled");

    public string UbAliasesButtonText => UbSwitchCaption("Aliases", "Aliases.Enabled");

    public uint UbAliasesButtonColor => UbSwitchColor("Aliases.Enabled");

    public Action ToggleUbAliases => () => FlipUbSwitch("Aliases.Enabled");

    public string UbGameEventsButtonText => UbSwitchCaption("Game Events", "GameEvents.Enabled");

    public uint UbGameEventsButtonColor => UbSwitchColor("GameEvents.Enabled");

    public Action ToggleUbGameEvents => () => FlipUbSwitch("GameEvents.Enabled");

    /// <summary>Whether one catalogue switch is on, read afresh every time.</summary>
    private bool UbSwitchIsOn(string name) => _ubCatalog.Require(name).Get().Boolean;

    private string UbSwitchCaption(string label, string name) =>
        $"{label}: {(UbSwitchIsOn(name) ? "On" : "Off")}";

    private string UbShortSwitchCaption(string label, string name) =>
        $"{label} {(UbSwitchIsOn(name) ? "On" : "Off")}";

    private uint UbSwitchColor(string name) =>
        UbSwitchIsOn(name) ? UbSwitchOnColor : UbSwitchOffColor;

    /// <summary>
    /// Flips one catalogue switch through the catalogue itself, so the write
    /// lands in the file the row's scope names and the notice says whether it
    /// was saved. This is the value column's own path, reached from a button
    /// rather than from a row click.
    /// </summary>
    private void FlipUbSwitch(string name)
    {
        UbSetting row = _ubCatalog.Require(name);
        row.Set(UbSettingValue.FromBool(!row.Get().Boolean));
        NoteUbChange(row, $"is now {row.Display()}");
        RefreshUbSettings();
    }
}
