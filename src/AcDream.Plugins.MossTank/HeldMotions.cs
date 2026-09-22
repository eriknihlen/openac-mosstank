using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>The movement keys a macro can hold down on its own.</summary>
internal enum HeldMotion
{
    Forward,
    Backward,
    TurnRight,
    TurnLeft,
    StrafeRight,
    StrafeLeft,
    Walk,
}

/// <summary>
/// The movement keys the macro wants held, one switch per key, and the one
/// place that turns them into the client's movement intent. Both the
/// <c>setmotion</c> command and the <c>setmotion[]</c> expression go through
/// here, so a key pressed by one and released by the other is the same key.
/// </summary>
/// <remarks>
/// The keys combine: forward and a turn held together walk a circle. Walk is
/// not a direction but the run key let go, so holding it only changes how the
/// other held keys move; on its own it moves nothing. Releasing one key keeps
/// the others, and releasing the last lets go of the movement altogether.
/// </remarks>
internal sealed class HeldMotions
{
    internal const string Usage =
        "/vt setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>";

    private static readonly HeldMotion[] AllMotions = Enum.GetValues<HeldMotion>();

    private readonly IPluginHost _host;
    private readonly bool[] _held = new bool[AllMotions.Length];

    internal HeldMotions(IPluginHost host)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
    }

    /// <summary>The motion names in the order the usage line lists them.</summary>
    internal static string ValidNames => string.Join(", ", Enum.GetNames<HeldMotion>());

    /// <summary>
    /// Reads a motion by its name, ignoring letter case. A number is not a
    /// name, even though the enum would take one.
    /// </summary>
    internal static bool TryParse(string? text, out HeldMotion motion)
    {
        string name = text?.Trim() ?? string.Empty;
        foreach (HeldMotion candidate in AllMotions)
        {
            if (candidate.ToString().Equals(name, StringComparison.OrdinalIgnoreCase))
            {
                motion = candidate;
                return true;
            }
        }
        motion = default;
        return false;
    }

    /// <summary>Whether the macro has asked for this key to be held.</summary>
    internal bool IsHeld(HeldMotion motion) => _held[(int)motion];

    /// <summary>The intent the held keys add up to.</summary>
    internal PluginMovementIntent Intent => new(
        Forward: IsHeld(HeldMotion.Forward),
        Backward: IsHeld(HeldMotion.Backward),
        StrafeLeft: IsHeld(HeldMotion.StrafeLeft),
        StrafeRight: IsHeld(HeldMotion.StrafeRight),
        TurnLeft: IsHeld(HeldMotion.TurnLeft),
        TurnRight: IsHeld(HeldMotion.TurnRight),
        Run: !IsHeld(HeldMotion.Walk));

    private bool AnyKeyHeld
    {
        get
        {
            foreach (HeldMotion motion in AllMotions)
            {
                if (motion != HeldMotion.Walk && _held[(int)motion])
                    return true;
            }
            return false;
        }
    }

    /// <summary>
    /// Presses or releases one key and hands the client what is held now.
    /// Walk with nothing else held has nothing to change on the client, so it
    /// is only remembered for the next key pressed.
    /// </summary>
    internal PluginNavigationCommandStatus Set(HeldMotion motion, bool on)
    {
        _held[(int)motion] = on;
        if (motion == HeldMotion.Walk && !AnyKeyHeld)
            return PluginNavigationCommandStatus.Accepted;
        return Apply();
    }

    /// <summary>Releases every key, walk included.</summary>
    internal PluginNavigationCommandStatus Clear()
    {
        Array.Clear(_held);
        return _host.Automation.Navigation.ClearMovementIntent();
    }

    /// <summary>
    /// Forgets the held keys without telling the client: the session they were
    /// held in is gone, and the next one starts with nothing pressed.
    /// </summary>
    internal void Reset() => Array.Clear(_held);

    /// <summary>
    /// Lets go of the held keys when there are any, and leaves the client's
    /// movement alone when the macro was holding nothing.
    /// </summary>
    internal void ReleaseIfHeld()
    {
        if (AnyKeyHeld)
            Clear();
        else
            Reset();
    }

    private PluginNavigationCommandStatus Apply()
    {
        INavigationAutomation navigation = _host.Automation.Navigation;
        if (!AnyKeyHeld)
            return navigation.ClearMovementIntent();
        PluginMovementIntent intent = Intent;
        return navigation.SetMovementIntent(intent);
    }
}
