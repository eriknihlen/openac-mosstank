namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private readonly ActionHistory _actionHistory = new();

    public bool ActionHistoryVisible { get; private set; }
    public IReadOnlyList<string> ActionHistoryLines => _actionHistory.Lines;
    public int ActionHistoryFirstIndex => _actionHistory.FirstIndex;
    public Action ShowActionHistory => () => ActionHistoryVisible = true;
    public Action HideActionHistory => () => ActionHistoryVisible = false;
    public Action ClearActionHistory => _actionHistory.Clear;

    private void RecordActionHistory(string status) => _actionHistory.Record(status);
}
