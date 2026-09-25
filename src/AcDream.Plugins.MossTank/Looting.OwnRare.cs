using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// MossTank's own rule for this character's rare, behind the
/// <see cref="LootSettings.WalkToOwnRareCorpses"/> option, which is off unless
/// turned on. It is not the reference's: the reference drops every corpse
/// further off than the corpse approach range, rare or not, and nothing waits
/// for a corpse after a kill beyond the short hold while a nearby corpse's
/// description is on its way. With the option off that is exactly what
/// happens here too.
/// <para>
/// A kill beside a route point can end the fight at once, and the route walks
/// on in the second or so before the server drops the corpse and announces
/// the rare; by the time the corpse is described it can lie just past a short
/// approach range, and the rare rots where it fell. So, with the option on,
/// for this character's own rare only:
/// </para>
/// <list type="bullet">
/// <item>the corpse whose description says this character's kill generated a
/// rare is walked to out to <see cref="RareCorpseReachMeters"/>, however
/// short the profile's approach range;</item>
/// <item>after this character's rare announcement, a corpse that appeared
/// with it and is still waiting for its description holds every walk off out
/// to the same reach, as a nearby undescribed corpse always does;</item>
/// <item>while that rare corpse is waiting, unopened, where the walk or the
/// open will reach it, the route stands down even where the profile puts the
/// route above looting, so the walk and the open get their turn;</item>
/// <item>a walk to it that stops covering ground, or has not arrived after
/// <see cref="CorpseApproachController.OwnRareWalkGiveUpSeconds"/>, is given
/// up and the corpse set aside for the corpse blacklist's time, which
/// releases the route.</item>
/// </list>
/// Everything else — the open's own reach, which corpses may be looted, the
/// order of the rules — is as the reference has it.
/// </summary>
internal sealed partial class LootController
{
    /// <summary>
    /// How far this character's own rare is walked to: the longest corpse
    /// approach range a MossTank profile can hold.
    /// </summary>
    internal const double RareCorpseReachMeters = 100d;

    /// <summary>
    /// How far the walk reaches for this corpse: the profile's range, or the
    /// rare reach when the option is on and the corpse holds this
    /// character's own rare.
    /// </summary>
    internal double ApproachReachFor(
        in PluginLootContainer corpse,
        double rangeMeters) =>
        IsOwnRareWalkGoal(corpse)
            ? Math.Max(rangeMeters, RareCorpseReachMeters)
            : rangeMeters;

    /// <summary>
    /// Whether this corpse is one the option walks to and holds the route
    /// for: the option is on and the corpse holds this character's own rare.
    /// </summary>
    internal bool IsOwnRareWalkGoal(in PluginLootContainer corpse) =>
        _settings.WalkToOwnRareCorpses && IsOwnRareCorpse(corpse);

    /// <summary>
    /// Whether the walk's next pick is a corpse holding this character's own
    /// rare that the walk or the open will reach. It is the walk's own pick,
    /// so the route only stands down for a corpse the walk will actually go
    /// to, and stops standing down the moment that corpse is emptied,
    /// refused, given up on or out of reach. A corpse inside the walk's
    /// inner stop but outside the open's reach is reached by neither — the
    /// walk declines there and the open cannot touch it — so it does not
    /// hold the route either. Always false with the option off.
    /// </summary>
    /// <param name="rangeMeters">The approach range, as the profile sets it.</param>
    internal bool HasOwnRareCorpseToFetch(double rangeMeters) =>
        _settings.WalkToOwnRareCorpses
        && TrySelectApproachCorpse(rangeMeters, out PluginLootContainer corpse)
        && corpse.HasPosition
        && IsOwnRareCorpse(corpse)
        && (corpse.Distance > _settings.CorpseMinimumApproachRange
            || corpse.Distance <= CorpseOpenRangeMeters);

    /// <summary>
    /// Gives up walking to this character's rare corpse: it is set aside for
    /// the corpse blacklist's time, which takes it out of the walk's pick and
    /// so releases the route.
    /// </summary>
    internal void GiveUpOwnRareWalk(uint corpseId, string reason)
    {
        if (corpseId == 0u)
            return;
        _corpseBlacklistedAt[corpseId] = _lifetime;
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"LootCorpse: giving up the walk to this character's rare 0x{corpseId:X8}: {reason}");
    }

    /// <summary>
    /// A described corpse whose description says this character killed it
    /// and the kill generated a rare.
    /// </summary>
    private bool IsOwnRareCorpse(in PluginLootContainer corpse) =>
        corpse.IsIdentified && IsRare(corpse) && IsOwnKill(corpse);

    /// <summary>
    /// Whether the corpse's description names this character as its killer.
    /// The local server writes the killer's name without the plus sign an
    /// admin character carries, so the character's own plus is not part of
    /// the comparison. That is a deliberate deviation.
    /// </summary>
    private bool IsOwnKill(in PluginLootContainer corpse)
    {
        string killer = KillerName(corpse.LongDescription);
        string character = _host.Automation.Character.Name.TrimStart('+');
        return killer.Length != 0
            && character.Length != 0
            && string.Equals(killer, character, StringComparison.OrdinalIgnoreCase);
    }
}
