using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private string _advancedCustomBuffDraft = string.Empty;

    public string AdvancedCustomBuffName => AdvancedOptionName switch
    {
        "BuffProfile_Prots" => "BuffProfile-Prots",
        "BuffProfile_Banes" => "BuffProfile-Banes",
        _ => string.Empty,
    };

    public bool AdvancedCustomBuffVisible => AdvancedCustomBuffName.Length != 0
        && GetMetaOption(AdvancedOptionName).AsInt32() == 1;

    public string AdvancedCustomBuffDraft => _advancedCustomBuffDraft;
    public Action<string> SetAdvancedCustomBuffDraft => value => _advancedCustomBuffDraft = value;
    public Action<string> SubmitAdvancedCustomBuff => value =>
    {
        _advancedCustomBuffDraft = value;
        ApplyAdvancedCustomBuff();
    };

    public Action ApplyAdvancedCustomBuff => () =>
    {
        if (!AdvancedCustomBuffVisible)
            return;
        string value = _advancedCustomBuffDraft.Trim();
        if (value.Any(letter => "ALFCBPS".IndexOf(char.ToUpperInvariant(letter)) < 0))
        {
            _advancedOptionNotice = "Use only A, L, F, C, B, P, S (or leave empty for none).";
            return;
        }
        string name = AdvancedCustomBuffName;
        if (!SetMetaOption(name, ExpressionValue.String(value)))
        {
            _advancedOptionNotice = $"{name} is unavailable.";
            return;
        }
        RefreshAdvancedOptions();
        LoadAdvancedOptionDraft();
        _advancedOptionNotice = $"Applied {name}.";
    };
}
