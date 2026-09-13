namespace AcDream.Plugins.MossTank.Tests;

public sealed class ActionLockTableTests
{
    /// <summary>
    /// Mutation: change <c>Arm</c>'s <c>deadline &gt; existing</c> to an
    /// unconditional assignment and this fails — the 0.5 s arm would cut the
    /// 3 s one short.
    /// </summary>
    [Fact]
    public void ArmKeepsTheLaterDeadline()
    {
        var locks = new ActionLockTable();

        locks.Arm(ActionLockKind.Navigation, 3d);
        locks.Arm(ActionLockKind.Navigation, 0.5d);
        locks.Advance(1d);

        Assert.True(locks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// Mutation: change <c>Arm</c> so a longer arm cannot extend an existing
    /// lock and this fails at the 2.5 s sample.
    /// </summary>
    [Fact]
    public void ArmExtendsAShorterLock()
    {
        var locks = new ActionLockTable();

        locks.Arm(ActionLockKind.ItemUse, 0.75d);
        locks.Arm(ActionLockKind.ItemUse, 3d);
        locks.Advance(2.5d);

        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
    }

    /// <summary>
    /// Mutation: change <c>IsLocked</c>'s <c>deadline &gt;= _now</c> to
    /// <c>&gt;</c> and the exact-deadline sample fails.
    /// </summary>
    [Fact]
    public void ADeadlineOnThisExactInstantIsStillLocked()
    {
        var locks = new ActionLockTable();

        locks.Arm(ActionLockKind.DoorOpening, 1d);
        locks.Advance(1d);

        Assert.True(locks.IsLocked(ActionLockKind.DoorOpening));

        locks.Advance(0.001d);
        Assert.False(locks.IsLocked(ActionLockKind.DoorOpening));
    }

    /// <summary>
    /// Mutation: drop the <c>_deadlines.Remove(kind)</c> in <c>IsLocked</c>'s
    /// expiry arm and the re-arm below would keep the STALE longer deadline,
    /// so the second sample would report locked.
    /// </summary>
    [Fact]
    public void AnExpiredLockIsPrunedSoALaterShortArmIsNotExtended()
    {
        var locks = new ActionLockTable();

        locks.Arm(ActionLockKind.Salvage, 10d);
        locks.Advance(11d);
        Assert.False(locks.IsLocked(ActionLockKind.Salvage));

        locks.Arm(ActionLockKind.Salvage, 1d);
        locks.Advance(2d);

        Assert.False(locks.IsLocked(ActionLockKind.Salvage));
    }

    [Fact]
    public void ReleaseDropsALiveLockAndClearAllDropsEveryOne()
    {
        var locks = new ActionLockTable();
        locks.Arm(ActionLockKind.Navigation, 5d);
        locks.Arm(ActionLockKind.ItemUse, 5d);

        locks.Release(ActionLockKind.Navigation);

        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        locks.ClearAll();
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    [Fact]
    public void AnUnarmedSlotIsNeverLocked()
    {
        var locks = new ActionLockTable();
        locks.Advance(100d);

        Assert.False(locks.IsLocked(ActionLockKind.WarSpellLockedOut));
    }
}
