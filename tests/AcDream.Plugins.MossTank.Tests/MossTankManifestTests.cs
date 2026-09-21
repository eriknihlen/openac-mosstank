using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>The plugin ships a real <c>plugin.json</c> beside its project and the build copies it
/// into the plugin's output folder. This suite reads the file the build actually produced, never a
/// fixture that could drift from it. The launcher's own reader and its install rules are checked
/// separately, by <c>acdream-plugincheck</c> against the release zip in CI.</summary>
public sealed class MossTankManifestTests
{
    private const string ExpectedId = "acdream.mosstank";
    private const string ExpectedEntryDll = "AcDream.Plugins.MossTank.dll";

    [Fact]
    public void TheBuiltPluginFolderCarriesItsManifestAndEntryAssembly()
    {
        string directory = PluginOutputDirectory();

        Assert.True(
            File.Exists(Path.Combine(directory, "plugin.json")),
            $"No plugin.json in the plugin's build output: {directory}");
        Assert.True(
            File.Exists(Path.Combine(directory, ExpectedEntryDll)),
            $"No {ExpectedEntryDll} in the plugin's build output: {directory}");
    }

    /// <summary>The fields a launcher install requires, plus the two a host gates on: the contract
    /// version has to be one the referenced contract provides, and both hosts have to be admitted.
    /// </summary>
    [Fact]
    public void TheBuiltManifestDeclaresWhatAnInstallRequires()
    {
        JsonElement manifest = BuiltManifest();

        Assert.Equal(ExpectedId, manifest.GetProperty("id").GetString());
        Assert.Equal(ExpectedEntryDll, manifest.GetProperty("entryDll").GetString());
        Assert.Equal("MossTank", manifest.GetProperty("displayName").GetString());
        Assert.Equal(["Gameplay"], StringArray(manifest, "kinds"));
        Assert.Equal(["Graphical", "Headless"], StringArray(manifest, "hosts"));
        Assert.False(
            string.IsNullOrWhiteSpace(manifest.GetProperty("minHostVersion").GetString()),
            "minHostVersion is required for any plugin the launcher runs.");

        int apiVersion = manifest.GetProperty("apiVersion").GetInt32();
        Assert.True(
            PluginApi.IsSupported(apiVersion),
            $"apiVersion {apiVersion} is outside the contract range this build provides.");
    }

    /// <summary>The manifest's version and the assembly's both come from the repository's single
    /// version property, but nothing substitutes that property into the checked-in manifest, so
    /// this is what stops a version bump that touched only one of them going unnoticed.</summary>
    [Fact]
    public void TheManifestVersionMatchesTheBuiltAssemblyVersion()
    {
        string? manifestVersion = BuiltManifest().GetProperty("version").GetString();

        string? assemblyVersion = FileVersionInfo
            .GetVersionInfo(Path.Combine(PluginOutputDirectory(), ExpectedEntryDll))
            .ProductVersion;

        Assert.Equal(manifestVersion, VersionCore(assemblyVersion));
    }

    /// <summary>The other half of the contract boundary: the plugin compiles against one contract
    /// assembly, and its dependency list has to say the same, or a host would be asked to load a
    /// client assembly out of a plugin folder.</summary>
    [Fact]
    public void TheBuiltPluginFolderShipsNoClientAssemblyBesideThePlugin()
    {
        string directory = PluginOutputDirectory();

        string[] clientAssemblies = Directory
            .EnumerateFiles(directory, "AcDream.*.dll", SearchOption.AllDirectories)
            .Select(path => Path.GetFileName(path)!)
            .Where(name => !string.Equals(name, ExpectedEntryDll, StringComparison.Ordinal))
            .OrderBy(name => name, StringComparer.Ordinal)
            .ToArray();

        Assert.Empty(clientAssemblies);

        string dependencyList = File.ReadAllText(
            Path.Combine(directory, "AcDream.Plugins.MossTank.deps.json"));
        Assert.DoesNotContain("AcDream.Core", dependencyList);
        Assert.DoesNotContain("AcDream.Runtime", dependencyList);
        Assert.DoesNotContain("AcDream.Plugin.Abstractions", dependencyList);
    }

    private static string[] StringArray(JsonElement manifest, string property) =>
        manifest.GetProperty(property)
            .EnumerateArray()
            .Select(value => value.GetString()!)
            .ToArray();

    private static JsonElement BuiltManifest() =>
        JsonDocument
            .Parse(File.ReadAllText(Path.Combine(PluginOutputDirectory(), "plugin.json")))
            .RootElement;

    /// <summary>Reduces an informational version to the <c>MAJOR.MINOR.PATCH</c> core a manifest
    /// version names, the way the hosts reduce their own.</summary>
    private static string VersionCore(string? informationalVersion)
    {
        Assert.NotNull(informationalVersion);
        int cut = informationalVersion.IndexOfAny(['-', '+']);
        return cut < 0 ? informationalVersion : informationalVersion[..cut];
    }

    /// <summary>This suite's own output path ends in bin/[configuration]/[framework], and both
    /// projects share that layout, so the configuration and framework the run was built in are
    /// read from it rather than guessed.</summary>
    private static string PluginOutputDirectory()
    {
        var output = new DirectoryInfo(
            AppContext.BaseDirectory.TrimEnd(
                Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
        return Path.Combine(
            FindRepositoryRoot(),
            "src",
            "AcDream.Plugins.MossTank",
            "bin",
            output.Parent!.Name,
            output.Name);
    }

    private static string FindRepositoryRoot([CallerFilePath] string sourcePath = "")
    {
        string[] starts =
        [
            Path.GetDirectoryName(sourcePath) ?? string.Empty,
            Directory.GetCurrentDirectory(),
            AppContext.BaseDirectory,
        ];
        foreach (string start in starts)
        {
            if (string.IsNullOrEmpty(start))
            {
                continue;
            }

            var directory = new DirectoryInfo(start);
            while (directory is not null)
            {
                if (File.Exists(Path.Combine(directory.FullName, "MossTank.slnx")))
                {
                    return directory.FullName;
                }

                directory = directory.Parent;
            }
        }

        throw new DirectoryNotFoundException(
            "Could not find MossTank.slnx above the source, working, or output directory.");
    }
}
