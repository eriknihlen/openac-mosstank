using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

public sealed class MossTankPlugin : IAcDreamPlugin
{
    private IPluginHost? _host;
    private MossTankPanel? _panel;
    private Action<double>? _tick;
    private Action<double>? _autostartTick;
    private IDisposable? _commandRegistration;
    private IDisposable? _ubCommandRegistration;
    private IDisposable? _goToPause;
    private IDisposable? _lootClassifierRegistration;

    public void Initialize(IPluginHost host)
    {
        _host = host;
        // The one place the real game-database download is handed over:
        // every host that loads the plugin, windowed or headless, comes
        // through here.
        _panel = new MossTankPanel(host, new HttpVtankGameInfoTransport());
        host.Log.Info("MossTank initialized");
    }

    public void Enable()
    {
        if (_host is null || _panel is null)
            return;

        // Markup ships beside the plugin assembly, so it is found relative to
        // this DLL rather than the host's working directory -- plugins are
        // loaded from their own directory and the two are not the same.
        string directory =
            Path.GetDirectoryName(typeof(MossTankPlugin).Assembly.Location) ?? ".";

        _host.Ui.AddPanel(
            new PluginPanelDescriptor("main", "MossTank")
            {
                IconText = "MT",
                // A real installed portal.dat RenderSurface — see
                // MossTankIconInstalledDatTests for the DAT-presence proof.
                // IconText remains the fallback if this id is ever absent
                // from an install.
                IconSurfaceId = 0x06002C41u,
                StartVisible = false,
                ShowInSidePanel = true,
            },
            Path.Combine(directory, "mosstank.xml"),
            _panel);

        _host.Ui.AddPanel(
            new PluginPanelDescriptor("remote", "MossTank Remote")
            {
                StartVisible = false,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-remote.xml"),
            _panel);

        _host.Ui.AddPanel(
            new PluginPanelDescriptor("action-history", "MossTank Action History")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-action-history.xml"),
            _panel);

        _host.Ui.AddPanel(
            new PluginPanelDescriptor("advanced-options", "MossTank Advanced Options")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-advanced.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("loot-editor", "MossTank Loot Editor")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-loot-editor.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("buff-picker", "MossTank Add Buff")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-buffpicker.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("meta-editor", "MossTank Meta Rule Editor")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-metaeditor.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("ub-color", "MossTank Colour")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-ub-color.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("ub-list", "MossTank List")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-ub-list.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("ub-map", "MossTank Map")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-ub-map.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("ub-clients", "MossTank Clients")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-ub-clients.xml"),
            _panel);
        _host.Ui.AddPanel(
            new PluginPanelDescriptor("ub-dungeon", "MossTank Dungeon")
            {
                StartVisible = true,
                ShowInSidePanel = false,
            },
            Path.Combine(directory, "mosstank-ub-dungeon.xml"),
            _panel);

        // Two words, two command sets, as the two plugins MossTank stands in
        // for keep them: the macro's commands answer on /vt, the UtilityBelt
        // ones on /ub, and neither word answers the other's.
        _commandRegistration = _host.Commands.Register(
            MossTankPanel.VtankVerb,
            _panel.ExecuteVtankCommand);
        _ubCommandRegistration = _host.Commands.Register(
            MossTankPanel.UbVerb,
            _panel.ExecuteUbCommand);

        _lootClassifierRegistration = _host.LootClassifiers.Register(
            "moss-tank",
            "MossTank",
            new MossTankLootClassifier(_host, () => _panel.LiveLootRules));

        _tick = _panel.OnTick;
        _host.Events.Tick += _tick;

        MossTankPanel panel = _panel;
        _goToPause = _host.Automation.Navigation.PauseGoToWhile(() => panel.WalkPauseReason);

        _autostartTick = _ => _panel.TickAutostart();
        _host.Events.Tick += _autostartTick;

        _host.Log.Info(
            _host.Automation.IsAvailable
                ? "MossTank enabled"
                : "MossTank enabled (no live session yet; the Buff button will "
                  + "report 'Not in world' until one is up)");
    }

    public void Disable()
    {
        if (_host is not null && _tick is not null)
            _host.Events.Tick -= _tick;
        if (_host is not null && _autostartTick is not null)
            _host.Events.Tick -= _autostartTick;
        _commandRegistration?.Dispose();
        _commandRegistration = null;
        _ubCommandRegistration?.Dispose();
        _ubCommandRegistration = null;
        _goToPause?.Dispose();
        _goToPause = null;
        _lootClassifierRegistration?.Dispose();
        _lootClassifierRegistration = null;
        _tick = null;
        _autostartTick = null;
        _panel?.Dispose();
        _panel = null;
        _host?.Log.Info("MossTank disabled");
        _host = null;
    }
}
