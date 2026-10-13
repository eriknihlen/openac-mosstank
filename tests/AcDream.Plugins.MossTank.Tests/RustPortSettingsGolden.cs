using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Golden data for the Rust conversion's settings crate
/// (rust/crates/mosstank-settings): the settings profile serializer, the
/// default settings document, monster specs and the UB settings catalogue,
/// run as a black box over rust/crates/mosstank-settings/tests/golden/cases.txt.
/// Called from <see cref="RustPortGolden.WriteGoldenData"/>, so it runs only
/// when MOSSTANK_RUST_GOLDEN is set. The text format is the Rust test's
/// (tests/golden.rs in that crate); change both together.
/// </summary>
internal static class RustPortSettingsGolden
{
    /// <summary>Whether a case's result is kept only as hashes.</summary>
    internal static bool IsLarge(string kind, string field) =>
        field.StartsWith('@') || kind is "defaults" or "create-new";

    internal static string Run(string kind, string name, string input)
    {
        var sb = new StringBuilder();
        try
        {
            switch (kind)
            {
                case "defaults":
                    sb.Append(Show(VtankDefaultSettingsDatabase.Create().Render())).Append('\n');
                    break;
                case "create-new":
                    sb.Append(Show(VtankSettingsProfileSerializer.CreateNew(NewSettings()).Render())).Append('\n');
                    break;
                case "usd":
                    Usd(sb, input);
                    break;
                case "monster":
                    Monster(sb, input);
                    break;
                case "ub":
                    Ub(sb);
                    break;
                case "ubparse":
                    UbParse(sb, input);
                    break;
                default:
                    throw new InvalidOperationException("unknown case kind " + kind);
            }
        }
        catch (Exception error)
        {
            return "!! " + error.Message + "\n";
        }
        return sb.ToString();
    }

    private static VtankSettingsProfileSerializer.AllSettings NewSettings() => new()
    {
        Combat = new CombatSettings(),
        Buffs = new BuffSettings(),
        Vitals = new VitalSettings(),
        Inventory = new InventorySettings(),
        Navigation = new NavigationSettings(),
    };

    private static void Usd(StringBuilder sb, string input)
    {
        VtankSettingsProfileSerializer.AllSettings s = NewSettings();
        var warnings = new List<string>();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(input, s, warnings.Add);
        foreach (string warning in warnings)
            sb.Append("warn ").Append(warning).Append('\n');
        Dump(sb, s);
        sb.Append("--- saved:\n").Append(Show(VtankSettingsProfileSerializer.Save(document, s))).Append('\n');

        s.Combat.MaximumRange = 30d;
        s.Vitals.NormalHealth = 0.6d;
        s.Buffs.ProtectionElements = "AB";
        s.Combat.SelectionMethod = TargetSelectionMethod.Range;
        s.Navigation.MinimumDistanceMeters = 3.3d;
        s.Buffs.IdleBuffTopoffSeconds = 1234.5d;
        s.Combat.AttackHeight = (PluginAttackHeight)3;
        s.Meta.Enabled = true;
        s.Combat.ConsumableNames.Add("Golden Kit");
        s.Combat.ConsumableCategories["Golden Kit"] = ConsumableCategory.HealthKit;
        s.Combat.CombatItemOrderIds.Add(0x50000001u);
        s.Buffs.GemFoodItems.Clear();
        s.Buffs.GemFoodItems.Add(new GemFoodItem("Golden Gem", 4321u));
        s.Buffs.ExtraBuffSpellIds.Add(777u);
        s.Combat.Rules.Add(new MonsterRule(
            "name==Golden Drudge",
            new MonsterRuleActions { Priority = 3, DamageType = MonsterDamageType.Fire }));
        sb.Append("--- edited:\n").Append(Show(VtankSettingsProfileSerializer.Save(document, s))).Append('\n');
    }

    private static void Dump(StringBuilder sb, VtankSettingsProfileSerializer.AllSettings s)
    {
        foreach (string name in VtankOptionCatalog.CommandNames)
        {
            VtankCell? cell = VtankSettingsProfileSerializer.Capture(name, s);
            sb.Append(name).Append(" = ")
              .Append(cell is null ? "-" : cell.Tag + " " + Esc(cell.AsString()))
              .Append('\n');
        }
        foreach (MonsterRule rule in s.Combat.Rules)
        {
            MonsterRuleActions a = rule.Actions;
            sb.Append("rule ").Append(Esc(rule.Expression))
              .Append(" flags=").Append((int)a.Flags)
              .Append(" p=").Append(a.Priority)
              .Append(" dmg=").Append(a.DamageType)
              .Append(" extra=").Append(a.ExtraVulnerability)
              .Append(" pet=").Append(a.PetDamageType)
              .Append(" w=").Append(a.WeaponObjectId.ToString("X8", CultureInfo.InvariantCulture))
              .Append(" o=").Append(a.OffhandObjectId.ToString("X8", CultureInfo.InvariantCulture))
              .Append(" wr=").Append(a.WeaponToUseRaw)
              .Append(" sr=").Append(a.SecondaryEquipRaw)
              .Append(" ignored=").Append(rule.IsIgnoredSpec)
              .Append(" default=").Append(rule.IsDefault)
              .Append(" dynamic=").Append(rule.IsDynamic)
              .Append('\n');
        }
        sb.Append("items ").Append(string.Join(",", s.Combat.CombatItemOrderIds
            .Select(static id => id.ToString("X8", CultureInfo.InvariantCulture)))).Append('\n');
        foreach (BuffItemEnchantRow row in s.Buffs.ItemEnchantRows)
        {
            sb.Append("enchant ")
              .Append(row.ObjectId?.ToString("X8", CultureInfo.InvariantCulture) ?? "-").Append(' ')
              .Append(row.SpellId?.ToString(CultureInfo.InvariantCulture) ?? "-")
              .Append(" nothing=").Append(row.CastsNothing).Append('\n');
        }
        foreach (KeyValuePair<uint, int> uses in s.Combat.ItemUseSpecifiers)
            sb.Append("uses ").Append(uses.Key.ToString("X8", CultureInfo.InvariantCulture)).Append('=').Append(uses.Value).Append('\n');
        foreach (AssistItem item in s.Combat.ImportedAssistItems)
            sb.Append("assist ").Append(Esc(item.Name)).Append(' ').Append(item.Category).Append('\n');
        foreach (GemFoodItem item in s.Buffs.GemFoodItems)
            sb.Append("gem ").Append(Esc(item.Name)).Append(' ').Append(item.SpellId).Append('\n');
        sb.Append("extra ").Append(string.Join(",", s.Buffs.ExtraBuffSpellIds)).Append('\n');
        sb.Append("anti ").Append(string.Join(",", s.Buffs.AntiExtraBuffSpellIds)).Append('\n');
        foreach (RechargeHandlerRow row in s.Vitals.RechargeHandlerRows)
        {
            sb.Append("recharge ").Append(row.Vital).Append('|').Append(Esc(row.HandlerString))
              .Append('|').Append(row.MinPercent).Append('|').Append(row.MaxPercent)
              .Append('|').Append(row.Stance).Append('\n');
        }
        sb.Append("meta ").Append(s.Meta.Enabled).Append('\n');
    }

    private static void Monster(StringBuilder sb, string input)
    {
        MonsterExpression expression;
        try
        {
            expression = MonsterExpression.Compile(input);
        }
        catch (MonsterExpressionException compileError)
        {
            sb.Append("ERR ").Append(compileError.Message).Append('\n');
            return;
        }
        var settings = new Dictionary<string, MonsterValue>(StringComparer.Ordinal)
        {
            ["DoJiggle"] = MonsterValue.FromBoolean(true),
            ["Count"] = MonsterValue.FromNumber(3d),
            ["Label"] = MonsterValue.FromText("Hunting"),
        };
        var context = new MonsterExpressionContext(
            "Drudge Lurker",
            TypeId: 1234u,
            Species: "Drudge",
            MaximumHealth: 450,
            Range: 8.5f,
            HasShield: true,
            MetaState: "Hunting",
            Setting: name => settings.TryGetValue(name, out MonsterValue value) ? value : null);
        sb.Append("source ").Append(Esc(expression.Source)).Append(" dynamic=").Append(expression.IsDynamic).Append('\n');
        if (expression.TryEvaluate(context, out MonsterValue result, out string? error))
            sb.Append("value ").Append(result.Kind).Append(' ').Append(Esc(result.ToString())).Append('\n');
        else
            sb.Append("eval-error ").Append(error).Append('\n');
        bool match = expression.IsMatch(context, out string? matchError);
        sb.Append("match=").Append(match);
        if (matchError is not null)
            sb.Append(" error=").Append(matchError);
        sb.Append('\n');
    }

    private static void Ub(StringBuilder sb)
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());
        foreach (UbSettingDefinition d in UbSettingDefinitions.All.Concat(UbSettingDefinitions.AcceptedOnly))
        {
            UbSetting row = catalog.Require(d.Name);
            sb.Append(d.Name).Append('|').Append(d.Kind).Append('|').Append(d.Scope)
              .Append('|').Append(d.Category)
              .Append('|').Append(Esc(d.Default.ToStorageString()))
              .Append('|').Append(Esc(d.Display(d.Default)))
              .Append('|').Append(Esc(row.FullDisplayValue()))
              .Append('|').Append(Esc(d.Summary))
              .Append('|').Append(string.Join(",", d.Choices.Select(static c =>
                  c.Value.ToString(CultureInfo.InvariantCulture) + ":" + c.Label)))
              .Append('\n');
        }
        sb.Append("categories ").Append(string.Join(",", catalog.Categories)).Append('\n');
        foreach ((string spelling, string _) in UbSettingDefinitions.OriginalSpellings)
            sb.Append("alias ").Append(spelling).Append(" -> ").Append(catalog.Require(spelling).Name).Append('\n');
        foreach (string probe in new[] { "nametags.enabled", "AutoXp.Policy", "Plugin.SettingsProfile", "No.Such" })
            sb.Append("kind ").Append(probe).Append(' ').Append(UbSettingDefinitions.KindOf(probe)).Append('\n');
    }

    private static void UbParse(StringBuilder sb, string input)
    {
        int bar = input.IndexOf('|', StringComparison.Ordinal);
        UbSettingKind kind = Enum.Parse<UbSettingKind>(input[..bar]);
        string text = input[(bar + 1)..];
        if (!UbSettingValue.TryParse(kind, text, out UbSettingValue value))
        {
            sb.Append("FAIL\n");
            return;
        }
        sb.Append("OK ").Append(value.Kind).Append(' ').Append(Esc(value.ToStorageString()))
          .Append(" int=").Append(value.AsInt32())
          .Append(" color=").Append(value.AsColor().ToString("X8", CultureInfo.InvariantCulture))
          .Append(" items=").Append(value.Items.Count)
          .Append('\n');
    }

    internal static string Show(string s) =>
        s.Replace("\r", "\\r").Replace("\n", "\\n\n").Replace("\t", "\\t");

    private static string Esc(string s)
    {
        var sb = new StringBuilder("\"");
        foreach (char c in s)
        {
            if (c == '\\') sb.Append("\\\\");
            else if (c == '"') sb.Append("\\\"");
            else if (c == '\r') sb.Append("\\r");
            else if (c == '\n') sb.Append("\\n");
            else if (c == '\t') sb.Append("\\t");
            else sb.Append(c);
        }
        return sb.Append('"').ToString();
    }
}
