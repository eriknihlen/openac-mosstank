namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The tree is laid out by the panel itself, on both roots, before anything
/// has been written into it.
/// </summary>
public sealed partial class MossTankPanelTests
{
    [Fact]
    public void AStartLaysOutEveryFixedFolderAndTheLoggedInCharactersOwn()
    {
        var storage = new MemoryStorage { RootPath = Path.Combine("data", "vtank") };
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };

        _ = new MossTankPanel(new FakeHost(automation, storage));

        foreach (string folder in StorageLayout.VtankProfileFolders)
            Assert.Contains(folder, storage.Directories);
        foreach (string folder in StorageLayout.PluginStorageFolders)
            Assert.Contains(folder, storage.Directories);
        foreach (string folder in StorageLayout.CharacterFolders("Coldeve", "Barris"))
            Assert.Contains(folder, storage.Directories);
    }
}
