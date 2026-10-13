using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Golden data for the Rust conversion's loot crate (rust/crates/mosstank-loot):
/// the loot rule engine run as a black box over the real .utl fixtures and
/// MossTank's own expressions, against the items described in
/// rust/crates/mosstank-loot/tests/golden/cases.txt. Writes cases.out beside
/// it, which tests/golden.rs in that crate compares. Runs only when
/// MOSSTANK_RUST_GOLDEN is set:
///
///     MOSSTANK_RUST_GOLDEN=1 dotnet test --filter RustPortLootGolden
///
/// The text format is the Rust test's; change both together.
/// </summary>
public sealed class RustPortLootGolden
{
    private static readonly CultureInfo Invariant = CultureInfo.InvariantCulture;

    private static string RepositoryRoot()
    {
        string? directory = AppContext.BaseDirectory;
        while (directory is not null && !File.Exists(Path.Combine(directory, "MossTank.slnx")))
            directory = Path.GetDirectoryName(directory);
        return directory ?? throw new InvalidOperationException("repository root not found");
    }

    [Fact]
    public void WriteLootGoldenData()
    {
        if (Environment.GetEnvironmentVariable("MOSSTANK_RUST_GOLDEN") is null)
            return;
        string root = RepositoryRoot();
        string golden = Path.Combine(root, "rust", "crates", "mosstank-loot", "tests", "golden");
        string fixtures = Path.Combine(root, "tests", "AcDream.Plugins.MossTank.Tests", "Fixtures");
        var items = new List<(string Name, PluginInventoryItem Item, PluginItemProperties Properties)>();
        var output = new StringBuilder();
        foreach (string raw in File.ReadAllLines(Path.Combine(golden, "cases.txt")))
        {
            if (raw.Length == 0 || raw.StartsWith('#'))
                continue;
            string[] parts = raw.Split(" ::: ", 3);
            string kind = parts[0];
            string name = parts[1];
            string field = parts.Length > 2 ? parts[2] : string.Empty;
            switch (kind)
            {
                case "item":
                    (PluginInventoryItem item, PluginItemProperties properties) = Item(field);
                    items.Add((name, item, properties));
                    break;
                case "profile":
                    output.Append("== profile ").Append(name).Append('\n');
                    Profile(output, File.ReadAllText(Path.Combine(fixtures, field[1..])), items);
                    break;
                case "expr":
                    output.Append("== expr ").Append(name).Append('\n');
                    Expression(output, field, items);
                    break;
                default:
                    throw new InvalidOperationException("unknown case kind " + kind);
            }
        }
        File.WriteAllText(Path.Combine(golden, "cases.out"), output.ToString());
    }

    private static void Profile(
        StringBuilder sb,
        string text,
        List<(string Name, PluginInventoryItem Item, PluginItemProperties Properties)> items)
    {
        if (!VtankLootProfileSerializer.TryRead(text, out VtankLootProfile profile, out string error))
        {
            sb.Append("!! ").Append(error).Append('\n');
            return;
        }
        List<LootRule> rules = profile.Rules;
        foreach ((string name, PluginInventoryItem item, PluginItemProperties properties) in items)
        {
            sb.Append(name).Append(": ");
            LootDecision? decision = LootRuleEngine.Decide(
                item, properties, rules, [], out LootRefusal refusal);
            if (decision is { } found)
            {
                sb.Append("decision=").Append(found.RuleIndex.ToString(Invariant))
                    .Append('|').Append(found.RuleName)
                    .Append('|').Append(found.Action.ToString());
            }
            else
            {
                sb.Append("refused=").Append(refusal.Describe());
            }
            sb.Append("; needs=")
                .Append(LootRuleEngine.NeedsIdentify(item, properties, rules) ? "true" : "false")
                .Append("; rules=");
            for (int index = 0; index < rules.Count; index++)
            {
                rules[index].EarlyMatch(item, properties, null, out bool decided, out bool early);
                bool matched = rules[index].IsMatch(item, properties, null, out string? failure);
                if (index > 0)
                    sb.Append(' ');
                sb.Append(decided ? 'd' : 'u')
                    .Append(early ? 'm' : '-')
                    .Append(matched ? 'M' : '.')
                    .Append(failure is null ? '_' : '!');
            }
            sb.Append('\n');
        }
    }

    private static void Expression(
        StringBuilder sb,
        string source,
        List<(string Name, PluginInventoryItem Item, PluginItemProperties Properties)> items)
    {
        LootRuleExpression compiled;
        try
        {
            compiled = LootRuleExpression.Compile(source);
        }
        catch (Exception error) when (error is FormatException or ArgumentException)
        {
            sb.Append("!! compile\n");
            return;
        }
        foreach ((string name, PluginInventoryItem item, PluginItemProperties properties) in items)
        {
            sb.Append(name).Append(": ");
            try
            {
                sb.Append(compiled.IsMatch(item, properties) ? "true" : "false");
            }
            catch (FormatException)
            {
                sb.Append("error");
            }
            sb.Append('\n');
        }
    }

    private static (PluginInventoryItem, PluginItemProperties) Item(string spec)
    {
        var item = new PluginInventoryItem(
            0u, 0u, string.Empty, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);
        var ints = new Dictionary<uint, int>();
        var floats = new Dictionary<uint, double>();
        var strings = new Dictionary<uint, string>();
        PluginWeaponProfile? weapon = null;
        PluginArmorProfile? armor = null;
        foreach (string part in spec.Split(';'))
        {
            int open = part.IndexOf('(', StringComparison.Ordinal);
            int equals = part.IndexOf('=', StringComparison.Ordinal);
            if (open > 0 && part.EndsWith(')') && (equals < 0 || open < equals))
            {
                string key = part[..open];
                string[] values = part[(open + 1)..^1].Split(',');
                switch (key)
                {
                    case "spells":
                        item = item with { AppraisedSpellIds = values.Select(U).ToArray() };
                        break;
                    case "gear":
                        item = item with
                        {
                            GearDamage = I(values[0]),
                            GearDamageResistance = I(values[1]),
                            GearCriticalChance = I(values[2]),
                            GearCriticalResistance = I(values[3]),
                            GearCriticalDamage = I(values[4]),
                            GearCriticalDamageResistance = I(values[5]),
                        };
                        break;
                    case "ints":
                        foreach (string pair in values)
                            ints[U(pair.Split(':')[0])] = I(pair.Split(':')[1]);
                        break;
                    case "floats":
                        foreach (string pair in values)
                            floats[U(pair.Split(':')[0])] = D(pair.Split(':')[1]);
                        break;
                    case "strings":
                        foreach (string pair in values)
                            strings[U(pair.Split(':', 2)[0])] = pair.Split(':', 2)[1];
                        break;
                    case "weapon":
                        weapon = new PluginWeaponProfile(
                            0, I(values[0]), U(values[1]), I(values[2]), D(values[3]),
                            D(values[4]), D(values[5]), D(values[6]), D(values[7]), 0);
                        break;
                    case "armor":
                        armor = new PluginArmorProfile(
                            I(values[0]), F(values[1]), F(values[2]), F(values[3]),
                            F(values[4]), F(values[5]), F(values[6]), F(values[7]),
                            F(values[8]));
                        break;
                    default:
                        throw new InvalidOperationException("unknown item list " + key);
                }
                continue;
            }
            string field = part[..equals];
            string value = part[(equals + 1)..];
            item = field switch
            {
                "id" => item with { ObjectId = U(value) },
                "wcid" => item with { WeenieClassId = U(value) },
                "name" => item with { Name = value },
                "class" => item with { ObjectClass = (PluginObjectClass)I(value) },
                "itemtype" => item with { ItemType = U(value) },
                "value" => item with { Value = I(value) },
                "burden" => item with { Burden = I(value) },
                "workmanship" => item with { Workmanship = F(value) },
                "material" => item with { MaterialType = U(value) },
                "damage" => item with { Damage = I(value) },
                "variance" => item with { DamageVariance = D(value) },
                "effects" => item with { Effects = U(value) },
                "underlay" => item with { IconUnderlayId = U(value) },
                "overlay" => item with { IconOverlayId = U(value) },
                "icon" => item with { IconId = U(value) },
                "coverage" => item with { CoverageMask = U(value) },
                "stack" => item with { StackSize = I(value) },
                "maxstack" => item with { MaximumStackSize = I(value) },
                "equipped" => item with { EquippedLocation = U(value) },
                "wielder" => item with { WielderObjectId = U(value) },
                "container" => item with { ContainerObjectId = U(value) },
                "structure" => item with { Structure = I(value) },
                "maxstructure" => item with { MaximumStructure = I(value) },
                "mana" => item with { ItemCurrentMana = I(value) },
                "maxmana" => item with { ItemMaximumMana = I(value) },
                "tinkered" => item with { NumTimesTinkered = I(value) },
                "combatuse" => item with { CombatUse = I(value) },
                "weaponskill" => item with { WeaponSkill = I(value) },
                "damagetype" => item with { DamageType = I(value) },
                "ammo" => item with { AmmoType = U(value) },
                "target" => item with { TargetType = U(value) },
                "publicflags" => item with { PublicFlags = U(value) },
                "useability" => item with { Useability = U(value) },
                "valid" => item with { ValidLocations = U(value) },
                "plural" => item with { PluralName = value },
                "radius" => item with { UseRadius = F(value) },
                "slot" => item with { ContainerSlot = I(value) },
                _ => throw new InvalidOperationException("unknown item field " + field),
            };
        }
        var properties = new PluginItemProperties(
            ints,
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            floats,
            strings,
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>())
        {
            WeaponProfile = weapon,
            ArmorProfile = armor,
        };
        return (item, properties);
    }

    private static uint U(string text) => text.StartsWith("0x", StringComparison.Ordinal)
        ? uint.Parse(text[2..], NumberStyles.HexNumber, Invariant)
        : uint.Parse(text, Invariant);

    private static int I(string text) => text.StartsWith("0x", StringComparison.Ordinal)
        ? unchecked((int)uint.Parse(text[2..], NumberStyles.HexNumber, Invariant))
        : int.Parse(text, Invariant);

    private static double D(string text) => double.Parse(text, Invariant);

    private static float F(string text) => float.Parse(text, Invariant);
}
