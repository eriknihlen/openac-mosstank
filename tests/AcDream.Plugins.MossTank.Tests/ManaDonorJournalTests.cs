using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ManaDonorJournalTests
{
    private static readonly PluginInventoryItem Donor = new() { ObjectId = 20, WeenieClassId = 48, Name = "Wand", ContainerObjectId = 1 };
    private static readonly PluginInventoryItem Stone = new() { ObjectId = 10, WeenieClassId = 47, Name = "Stone", ContainerObjectId = 1 };
    private static readonly ManaStoneTransferPlan Plan = new(10, 20, "Stone", "Wand");

    [Fact]
    public void RelogRetainsPermissionButAnIncompleteInventoryCannotRemoveIt()
    {
        var storage = new Storage();
        var journal = Open(storage);
        Assert.True(journal.Remember(Donor));
        journal = Open(storage);
        journal.Reconcile([], complete: false);
        Assert.True(journal.DonorReady(20));
        journal.Reconcile([Donor], complete: true);
        Assert.True(journal.DonorReady(20));
    }

    [Theory]
    [InlineData("drop")]
    [InlineData("sell")]
    [InlineData("give")]
    [InlineData("consume")]
    public void ConfirmedOwnershipLossRevokesPermissionPermanently(string operation)
    {
        Assert.NotEmpty(operation);
        var storage = new Storage();
        var journal = Open(storage);
        journal.Remember(Donor);
        journal.Reconcile([], complete: true);
        journal.Reconcile([Donor], complete: true);
        Assert.False(journal.DonorReady(20));
        Assert.Empty(Open(storage).Entries);
    }

    [Fact]
    public void MissingDonorAfterRelogIsRemovedWhenInventoryCompletes()
    {
        var storage = new Storage();
        Open(storage).Remember(Donor);
        var restored = Open(storage);
        restored.Reconcile([], complete: true);
        Assert.Empty(restored.Entries);
    }

    [Fact]
    public void EachCharacterAndServerHasSeparatePermission()
    {
        var storage = new Storage();
        var journal = Open(storage);
        journal.Remember(Donor);
        journal.Bind("Other server", "Tester");
        Assert.Empty(journal.Entries);
        journal.Bind("Coldeve", "Other character");
        Assert.Empty(journal.Entries);
        journal.Bind("Coldeve", "Tester");
        Assert.True(journal.DonorReady(20));
    }

    [Fact]
    public void InterruptedSendStaysHeldAcrossRelogAndLateInventorySettlesIt()
    {
        var storage = new Storage();
        var journal = Open(storage);
        journal.Remember(Donor);
        Assert.True(journal.Begin(Plan));
        journal = Open(storage);
        journal.Reconcile([Stone, Donor], complete: true);
        Assert.False(journal.DonorReady(20));
        Assert.True(journal.StoneHeld(10));
        journal.Reconcile([Stone with { Effects = 1 }], complete: true);
        Assert.False(journal.StoneHeld(10));
        Assert.Empty(journal.Entries);
    }

    [Fact]
    public void DroppingDonorWhileRequestIsUncertainDoesNotFreeTheStoneToRepeatIt()
    {
        var journal = Open(new Storage());
        journal.Remember(Donor);
        journal.Begin(Plan);
        journal.Reconcile([Stone], complete: true);
        Assert.False(journal.DonorReady(20));
        Assert.True(journal.StoneHeld(10));
        journal.Reconcile([Stone, Donor], complete: true);
        Assert.False(journal.DonorReady(20));
    }

    [Fact]
    public void RecycledObjectIdentityIsNotPermissionToDrainAnotherItem()
    {
        var journal = Open(new Storage());
        journal.Remember(Donor);
        journal.Reconcile([Donor with { WeenieClassId = 99 }], complete: true);
        Assert.False(journal.DonorReady(20));
    }

    [Fact]
    public void PlaceholderDuringLoginDoesNotRevokeTheSavedIdentity()
    {
        var journal = Open(new Storage());
        journal.Remember(Donor);
        journal.Reconcile([Donor with { WeenieClassId = 0, Name = "" }]);
        Assert.True(journal.DonorReady(20));
    }

    [Fact]
    public void FailureToPersistPreventsSendingAndCorruptHistoryIsNotOverwritten()
    {
        var storage = new Storage();
        var journal = Open(storage);
        journal.Remember(Donor);
        storage.FailWrites = true;
        Assert.False(journal.Begin(Plan));
        Assert.False(journal.CanOperate);
        storage.FailWrites = false;
        string key = Assert.Single(storage.Values.Keys);
        storage.Values[key] = "broken";
        journal = Open(storage);
        Assert.False(journal.CanOperate);
        Assert.False(journal.Remember(Donor));
        Assert.Equal("broken", storage.Values[key]);
    }

    private static ManaDonorJournal Open(Storage storage)
    {
        var journal = new ManaDonorJournal(storage, new Logger());
        journal.Bind("Coldeve", "Tester");
        return journal;
    }

    private sealed class Storage : IPluginStorage
    {
        public bool IsAvailable => true;
        public Dictionary<string, string> Values { get; } = [];
        public bool FailWrites { get; set; }
        public string? ReadText(string key) => Values.GetValueOrDefault(key);
        public void WriteText(string key, string content)
        {
            if (FailWrites) throw new IOException("Test write failure");
            Values[key] = content;
        }
    }
    private sealed class Logger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }
}

