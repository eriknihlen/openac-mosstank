namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private void DiscardEmbeddedRoute()
    {
        if (_embeddedRouteLabel is null)
            return;
        _navigation.Reset();
        VtankNavRouteSerializer.Apply(new NavigationSettings(), _navigationSettings);
        _embeddedRouteLabel = null;
        _selectedRouteWaypoint = 0;
        RefreshRouteEditor();
    }

    private void ReloadEmbeddedRoute(string label)
    {
        MetaProfile fresh = _metaProfiles.LoadCurrent();
        if (!_metaProfiles.LastLoadSucceeded)
        {
            _routeNotice = "Could not reload the embedded route: "
                + (_metaProfiles.LastLoadError ?? "the meta file is missing.");
            return;
        }
        MetaAction[] matches = fresh.Rules.SelectMany(rule => EmbeddedActions(rule.Action))
            .Where(action => EmbeddedRouteLabel(action.SecondaryText) == label).ToArray();
        if (matches.Length != 1 || matches[0].EmbeddedRoute is null)
        {
            _routeNotice = "Could not reload the embedded route: its name is missing or ambiguous in the meta file.";
            return;
        }
        LoadEmbeddedNavigationRoute(matches[0].EmbeddedRoute, matches[0].SecondaryText);
    }

    private static IEnumerable<MetaAction> EmbeddedActions(MetaAction action)
    {
        if (action.Kind == MetaActionKind.LoadEmbeddedNavigationRoute)
            yield return action;
        foreach (MetaAction child in action.Children)
            foreach (MetaAction embedded in EmbeddedActions(child))
                yield return embedded;
    }
}
