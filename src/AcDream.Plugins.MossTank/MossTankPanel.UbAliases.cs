using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The aliases that came over with the UB page. The list row on the page is
/// the alias file the profile row names, and one interceptor on the chat
/// surface holds what is typed against that list, read live.
/// </summary>
internal sealed partial class MossTankPanel
{
    private AliasController _aliases = null!;

    private void InitializeAliases(IPluginHost host)
    {
        // The switch and the list are read on every typed line, so an edit
        // on the page counts on the next thing typed, the way the other
        // rows the page binds are read live by their owners.
        _aliases = new AliasController(
            host.Automation.Chat,
            _expressions,
            host.Log,
            () => _ubCatalog.Require("Aliases.Enabled").Get().Boolean,
            () => _ubCatalog.Require("Aliases.DefinedAliases").Get().Items);
        _aliases.Attach();
    }

    private void DisposeAliases() => _aliases.Dispose();

    /// <summary>
    /// The list row over the alias file: the character's own until the
    /// profile row names a shared one. It is not kept in the settings files,
    /// so the catalogue must not keep a second copy of it there.
    /// </summary>
    private UbSettingBinding AliasListBinding() =>
        new(
            () => UbSettingValue.FromCollection(_ubStore.ReadLines(AliasFileKey())),
            value => _ubStore.WriteLines(AliasFileKey(), value.Items))
        {
            CanSave = () => AliasFileKey() is not null && _host.VtankProfiles.IsAvailable,
        };

    private string? AliasFileKey() =>
        _ubStore.AliasFileKey(_ubCatalog.Require("Aliases.Profile").Get().Text);
}
