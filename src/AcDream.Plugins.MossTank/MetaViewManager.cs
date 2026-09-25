using System.Security.Cryptography;
using System.Text;
using System.Xml;
using System.Xml.Linq;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The views metas create. A view is VTank view XML (see
/// <see cref="MetaView"/>) or, as a MossTank extension, the host's own panel
/// markup. Every view is kept here whether or not the host can draw it, so
/// the ui* functions and button actions behave the same with and without a
/// window.
/// </summary>
internal sealed class MetaViewManager : IMetaViewControls
{
    private const int OfficialViewLimit = 5;
    private readonly IPluginHost _host;
    private readonly Action<string> _runExpression;
    private readonly Action<string> _setState;
    private readonly Dictionary<string, Entry> _views =
        new(StringComparer.Ordinal);
    private readonly HashSet<string> _warnings = new(StringComparer.Ordinal);

    /// <param name="host">The host the views are drawn on, when it draws.</param>
    /// <param name="runExpression">Runs a button's action expression.</param>
    /// <param name="setState">Moves the meta to a button's state.</param>
    public MetaViewManager(
        IPluginHost host,
        Action<string>? runExpression = null,
        Action<string>? setState = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _runExpression = runExpression ?? (static _ => { });
        _setState = setState ?? (static _ => { });
    }

    public int Count => _views.Count;

    /// <summary>
    /// Creates or replaces a view. False when it was refused: no name or
    /// markup, the view limit reached, or markup that does not parse. A view
    /// with the same name is destroyed first, even when the new one is then
    /// refused.
    /// </summary>
    public bool Create(string name, string markup)
    {
        if (string.IsNullOrEmpty(name) || string.IsNullOrEmpty(markup))
            return false;

        // The limit counts before the replaced view goes, so six views may
        // exist and a seventh, or a replacement at six, is refused.
        if (_views.Count > OfficialViewLimit)
            return false;

        Destroy(name);
        if (IsPanelMarkup(markup))
        {
            _views.Add(name, new Entry(null, RegisterPanelMarkup(name, markup)));
            return true;
        }

        MetaView? view = MetaView.Parse(markup, out string error);
        if (view is null)
        {
            _host.Log.Warn($"MossTank Meta view '{name}' is invalid: {error}");
            return false;
        }

        _views.Add(name, new Entry(view, RegisterView(name, view)));
        return true;
    }

    public bool Destroy(string name)
    {
        if (!_views.Remove(name, out Entry? entry))
            return false;
        entry.Registration.Dispose();
        return true;
    }

    public void DestroyAll()
    {
        Entry[] entries = _views.Values.ToArray();
        _views.Clear();
        foreach (Entry entry in entries)
            entry.Registration.Dispose();
    }

    /// <summary>The parsed view behind a name, or null for none or a panel-markup view.</summary>
    internal MetaView? Find(string name) =>
        _views.TryGetValue(name, out Entry? entry) ? entry.View : null;

    /// <summary>
    /// Hits a button the way a click on the drawn window does, for hosts
    /// and tests with no window. False when there is no such button.
    /// </summary>
    internal bool Click(string viewName, string controlName)
    {
        MetaViewControl? control = Find(viewName)?.Find(controlName);
        if (control is null || control.Kind != MetaViewControlKind.Button)
            return false;
        Hit(control);
        return true;
    }

    public bool ViewExists(string view) => _views.ContainsKey(view);

    public bool HoldsControls(string view) => Find(view) is not null;

    /// <summary>
    /// A view is shown as soon as it is created. A drawn one is visible
    /// while its window is, which the player can close.
    /// </summary>
    public bool IsViewVisible(string view) =>
        HoldsControls(view)
        && (!_host.HasUi || _host.Ui.IsViewVisible(WindowId(view)));

    public bool ControlExists(string view, string control) =>
        Find(view)?.Find(control) is not null;

    public MetaViewLabelResult SetControlLabel(string view, string control, string label)
    {
        MetaViewControl? found = Find(view)?.Find(control);
        if (found is null)
            return MetaViewLabelResult.NotFound;
        if (found.Kind != MetaViewControlKind.Button)
            return MetaViewLabelResult.TakesNoLabel;
        found.Text = label;
        return MetaViewLabelResult.Set;
    }

    public bool SetControlVisible(string view, string control, bool visible)
    {
        MetaViewControl? found = Find(view)?.Find(control);
        if (found is null)
            return false;
        found.Visible = visible;
        return true;
    }

    /// <summary>
    /// A button runs its expression first and then changes state, each only
    /// when it has one. An expression that fails is reported once per
    /// distinct error and does not stop the state change.
    /// </summary>
    private void Hit(MetaViewControl button)
    {
        string? failure = null;
        if (button.ActionExpression.Length != 0)
        {
            try
            {
                _runExpression(button.ActionExpression);
            }
            catch (Exception error)
            {
                failure = error.Message;
            }
        }

        if (button.SetState.Length != 0)
            _setState(button.SetState);

        if (failure is not null)
        {
            string message = $"Error in Button meta expression: {failure}";
            if (_warnings.Add(message))
                _host.Log.Warn(message);
        }
    }

    private static bool IsPanelMarkup(string markup)
    {
        try
        {
            return XDocument.Parse(markup).Root?.Name.LocalName
                .Equals("panel", StringComparison.OrdinalIgnoreCase) == true;
        }
        catch (XmlException)
        {
            return false;
        }
    }

    private IDisposable RegisterView(string name, MetaView view)
    {
        if (!_host.HasUi)
            return NoOpUiRegistration.Instance;
        if (view.Controls.Count > MetaViewBinding.Capacity)
        {
            _host.Log.Warn(
                $"MossTank Meta view '{name}' has {view.Controls.Count} controls; "
                + $"only the first {MetaViewBinding.Capacity} are drawn.");
        }
        return _host.Ui.RegisterPanelContent(
            new PluginPanelDescriptor(WindowId(name), MetaViewBinding.WindowTitle(view))
            {
                IconText = Initials(name),
                StartVisible = true,
                ShowInSidePanel = true,
            },
            MetaViewBinding.BuildMarkup(view),
            new MetaViewBinding(view, Hit));
    }

    private IDisposable RegisterPanelMarkup(string name, string markup)
    {
        if (!_host.HasUi)
            return NoOpUiRegistration.Instance;
        return _host.Ui.RegisterPanelContent(
            new PluginPanelDescriptor(WindowId(name), name)
            {
                IconText = Initials(name),
                StartVisible = true,
                ShowInSidePanel = true,
            },
            markup,
            PanelMarkupBinding.Instance);
    }

    private static string WindowId(string name)
    {
        byte[] hash = SHA256.HashData(Encoding.UTF8.GetBytes(name));
        return "meta-" + Convert.ToHexString(hash.AsSpan(0, 8)).ToLowerInvariant();
    }

    private static string Initials(string name)
    {
        string[] words = name.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (words.Length == 0)
            return "M";
        return string.Concat(words.Take(2).Select(static word => word[0])).ToUpperInvariant();
    }

    private sealed record Entry(MetaView? View, IDisposable Registration);

    private sealed class PanelMarkupBinding
    {
        internal static PanelMarkupBinding Instance { get; } = new();
        public bool WindowAvailable => true;
    }
}
