namespace AcDream.Plugins.MossTank.Expressions;

internal enum MetaViewLabelResult
{
    NotFound,
    Set,
    TakesNoLabel,
}

/// <summary>
/// The views a meta created, as the ui* functions see them. A view built
/// from view XML answers its own control queries, drawn or not; anything
/// else is left to the host's own windows.
/// </summary>
internal interface IMetaViewControls
{
    /// <summary>Whether a meta created a view with this name, in any form.</summary>
    bool ViewExists(string view);

    /// <summary>Whether this view's controls are held here rather than by the host.</summary>
    bool HoldsControls(string view);

    bool IsViewVisible(string view);

    bool ControlExists(string view, string control);

    MetaViewLabelResult SetControlLabel(string view, string control, string label);

    bool SetControlVisible(string view, string control, bool visible);
}
