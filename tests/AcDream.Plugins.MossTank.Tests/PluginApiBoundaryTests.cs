using System.Reflection;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>A plugin sees the client through one contract assembly and nothing else. That is what
/// lets the same build run in the graphical client and in the windowless host, lets the client
/// change underneath without breaking the plugin, and lets the plugin live in its own repository
/// against a published contract instead of a checkout. A reference to any other client assembly
/// would take all three away quietly, so it is pinned here.</summary>
public sealed class PluginApiBoundaryTests
{
    private const string ContractAssembly = "AcDream.Plugin.Abstractions";
    private const string ClientAssemblyPrefix = "AcDream.";

    [Fact]
    public void ThePluginReferencesTheContractAssemblyAndNoOtherClientAssembly()
    {
        Assembly plugin = typeof(MossTankPlugin).Assembly;

        string[] clientReferences = plugin.GetReferencedAssemblies()
            .Select(reference => reference.Name ?? string.Empty)
            .Where(name => name.StartsWith(ClientAssemblyPrefix, StringComparison.Ordinal))
            .OrderBy(name => name, StringComparer.Ordinal)
            .ToArray();

        Assert.Equal([ContractAssembly], clientReferences);
    }
}