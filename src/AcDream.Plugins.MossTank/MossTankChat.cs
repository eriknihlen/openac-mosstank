using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class MossTankChat
{
    internal static void Post(IPluginChat chat, string text) =>
        chat.PostSystemMessage(text.StartsWith("[MossTank]", StringComparison.Ordinal)
            ? text : "[MossTank] " + text);
}
