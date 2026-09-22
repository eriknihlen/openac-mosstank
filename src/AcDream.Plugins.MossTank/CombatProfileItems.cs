namespace AcDream.Plugins.MossTank;

/// <summary>
/// Whether an owned object is one of the profile's items.
///
/// A profile that records its items by object id means THAT object. A second
/// object carrying the same name is a different object and must never stand in
/// for it: the character ends up fighting with gear the profile was never set
/// up for, while the item the profile does name is quietly never used, and the
/// two take it in turns as the choice is remade against each new monster.
///
/// Older profiles record their items by name alone and hold no ids at all.
/// Those are still matched by name, because the name is the whole of the row.
/// </summary>
internal static class CombatProfileItems
{
    public static bool IsProfiled(
        CombatSettings settings, uint objectId, string name)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (settings.CombatItemObjectIds.Contains(objectId))
            return true;
        // An id list that names anything at all is the whole list: a name is
        // read only while there are no ids to read instead.
        return settings.CombatItemObjectIds.Count == 0
            && settings.CombatItemNames.Contains(name ?? string.Empty);
    }
}
