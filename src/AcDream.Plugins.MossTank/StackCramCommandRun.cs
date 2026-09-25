using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// One stack or cram pass asked for from the command line, whether or not
/// the macro's own stack and cram switches are on. It says it is running,
/// works one move at a time on the same stack-and-cram owner the macro
/// uses, and says it is complete once nothing is left to do; with nothing
/// to do at the start it says so and does not start.
/// </summary>
internal sealed class StackCramCommandRun
{
    private readonly IPluginHost _host;
    private readonly Action<string> _write;
    private InventoryMaintenanceController? _run;
    private string _name = string.Empty;

    public StackCramCommandRun(IPluginHost host, Action<string> write)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _write = write ?? throw new ArgumentNullException(nameof(write));
    }

    /// <summary>Whether a pass is under way.</summary>
    public bool IsRunning => _run is not null;

    /// <summary>
    /// Starts a stack pass, or a cram pass into the side packs, replacing
    /// any pass already under way.
    /// </summary>
    public void Start(bool stack)
    {
        string name = stack ? "AutoStack" : "AutoCram";
        var settings = new InventorySettings { AutoStack = stack, AutoCram = !stack };
        _run = null;
        if (!_host.Automation.IsAvailable
            || InventoryMaintenancePlanner.Plan(
                _host.Automation.Items.CaptureOwnedItems(),
                _host.Automation.Character.ObjectId,
                settings) is null)
        {
            _write($"{name} - nothing to do");
            return;
        }
        _run = new InventoryMaintenanceController(_host, settings);
        _name = name;
        _write($"{name} running");
    }

    /// <summary>
    /// Drives the pass. True while it owns the character for this tick.
    /// </summary>
    public bool Tick(double elapsedSeconds, bool canAct)
    {
        if (_run is null)
            return false;
        bool owns = _run.Tick(elapsedSeconds, canAct);
        if (!owns && _run.FoundNothingToDo)
        {
            _run = null;
            _write($"{_name} complete.");
        }
        return owns;
    }

    /// <summary>Drops a pass without a word, as a new session does.</summary>
    public void Reset() => _run = null;
}
