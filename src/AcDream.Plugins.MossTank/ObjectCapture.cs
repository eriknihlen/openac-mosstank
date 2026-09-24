using System.Runtime.CompilerServices;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// One capture of the objects the client knows, shared by everything a meta
/// pass evaluates. A pass checks dozens of conditions and many of them list
/// every object (inventory counts, nearest-object finders); capturing once
/// per pass instead of once per condition stops each pass from copying the
/// whole object list dozens of times. A rule's action can change what the
/// client holds, so the capture is dropped after every action, and outside a
/// pass every call captures afresh.
/// </summary>
internal sealed class ObjectCapture
{
    private static readonly ConditionalWeakTable<IPluginHost, ObjectCapture> ByHost = new();

    private readonly IPluginHost _host;
    private IReadOnlyList<PluginWorldObject>? _captured;
    private int _passDepth;

    private ObjectCapture(IPluginHost host) => _host = host;

    public static ObjectCapture For(IPluginHost host)
    {
        ArgumentNullException.ThrowIfNull(host);
        return ByHost.GetValue(host, static owner => new ObjectCapture(owner));
    }

    /// <summary>How many times the host was asked for a capture.</summary>
    internal int CaptureCount { get; private set; }

    public IReadOnlyList<PluginWorldObject> Objects()
    {
        if (_passDepth == 0)
            return Capture();
        return _captured ??= Capture();
    }

    public PassScope BeginPass()
    {
        _passDepth++;
        return new PassScope(this);
    }

    /// <summary>Drops the pass capture; the next read captures again.</summary>
    public void Invalidate() => _captured = null;

    private IReadOnlyList<PluginWorldObject> Capture()
    {
        CaptureCount++;
        return _host.Automation.Objects.CaptureObjects();
    }

    private void EndPass()
    {
        if (_passDepth > 0 && --_passDepth == 0)
            _captured = null;
    }

    internal readonly struct PassScope : IDisposable
    {
        private readonly ObjectCapture? _owner;

        internal PassScope(ObjectCapture owner) => _owner = owner;

        public void Dispose() => _owner?.EndPass();
    }
}
