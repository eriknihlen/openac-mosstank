using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Golden data for the Rust conversion's stores crate
/// (rust/crates/mosstank-stores): the settings profile store run as a black
/// box over the scenarios named in
/// rust/crates/mosstank-stores/tests/golden/cases.txt, each leaving files,
/// log lines and answers behind. Called from
/// <see cref="RustPortGolden.WriteGoldenData"/>, so it runs only when
/// MOSSTANK_RUST_GOLDEN is set. The scenarios and the text format are the
/// Rust test's (tests/golden.rs in that crate); change both together.
/// </summary>
internal static class RustPortStoresGolden
{
    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = new(StringComparer.Ordinal);

        public bool IsAvailable => true;

        public string? RootPath => "C:\\memory";

        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;

        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix.EndsWith('/') ? prefix : prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.OrdinalIgnoreCase)
            .ToArray();

        public void WriteText(string key, string content) => Text[key] = content;

        public bool EnsureDirectory(string prefix) => true;

        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class Log : IPluginLogger
    {
        public List<string> Lines { get; } = [];

        public void Info(string message) => Lines.Add("info " + message);

        public void Warn(string message) => Lines.Add("warn " + message);

        public void Error(string message, Exception? exception = null) =>
            Lines.Add("error " + message);
    }

    private sealed class Character : ICharacterInfo
    {
        public string WorldName { get; set; } = string.Empty;
        public bool IsInWorld => true;
        public uint ObjectId => 0;
        public uint CurrentHealth => 0;
        public uint MaxHealth => 0;
        public uint CurrentStamina => 0;
        public uint MaxStamina => 0;
        public uint CurrentMana => 0;
        public uint MaxMana => 0;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }
    }

    private sealed class Automation : IAutomationSurface
    {
        public Character Info { get; } = new();
        public bool IsAvailable => true;
        public ICharacterInfo Character => Info;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => NoOpAutomationSurface.Instance;
    }

    private sealed class Host : IPluginHost
    {
        public MemoryStorage Plugin { get; } = new();
        public MemoryStorage Profiles { get; } = new();
        public Log Lines { get; } = new();
        public Automation World { get; } = new();
        public bool HasUi => false;
        public IPluginLogger Log => Lines;
        public IGameState State => throw new NotSupportedException();
        public IEvents Events => throw new NotSupportedException();
        public ISelectionService Selection => throw new NotSupportedException();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => Plugin;
        public IAutomationSurface Automation => World;
        public IPluginStorage VtankProfiles => Profiles;
    }

    private sealed class Live
    {
        public VtankSettingsProfileSerializer.AllSettings Settings { get; } = new()
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };

        public HashSet<string> NoBuff { get; } = new(StringComparer.Ordinal);

        public HashSet<string> Channels { get; } = new(StringComparer.OrdinalIgnoreCase);

        public MossTankProfileLoad Load(MossTankProfileStore store) =>
            store.LoadCurrent(Settings, NoBuff, Channels);

        public bool Save(MossTankProfileStore store) =>
            store.SaveCurrent(Settings, NoBuff, Channels);
    }

    internal static string Run(string name)
    {
        var sb = new StringBuilder();
        try
        {
            switch (name)
            {
                case "first-run": FirstRun(sb); break;
                case "side-car-values": SideCarValues(sb); break;
                case "create-and-delete": CreateAndDelete(sb); break;
                case "corrupt": Corrupt(sb); break;
                case "set-in-all": SetInAll(sb); break;
                case "legacy": Legacy(sb); break;
                case "roster": Roster(sb); break;
                case "preferences": Preferences(sb); break;
                case "loot-first-run": LootFirstRun(sb); break;
                case "loot-named": LootNamed(sb); break;
                case "loot-partial": LootPartial(sb); break;
                case "loot-import": LootImport(sb); break;
                case "loot-legacy": LootLegacy(sb); break;
                case "loot-unsupported": LootUnsupported(sb); break;
                default: throw new InvalidOperationException("unknown scenario " + name);
            }
        }
        catch (Exception error)
        {
            sb.Append("!! ").Append(error.Message).Append('\n');
        }
        return sb.ToString();
    }

    private static Host NewHost()
    {
        var host = new Host();
        host.World.Info.WorldName = "Coldeve";
        return host;
    }

    private static string B(bool value) => value ? "True" : "False";

    private static string Show(string s) =>
        s.Replace("\r", "\\r").Replace("\n", "\\n\n").Replace("\t", "\\t");

    private static string Fnv(string text)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(text);
        ulong hash = 0xcbf29ce484222325UL;
        foreach (byte b in bytes)
        {
            hash ^= b;
            hash *= 0x100000001b3UL;
        }
        return "fnv64=" + hash.ToString("x16", CultureInfo.InvariantCulture)
            + " bytes=" + bytes.Length.ToString(CultureInfo.InvariantCulture) + "\n";
    }

    private static void Dump(StringBuilder sb, string root, MemoryStorage storage)
    {
        foreach (string key in storage.Text.Keys.OrderBy(static k => k, StringComparer.Ordinal))
        {
            string text = storage.Text[key];
            sb.Append("--- ").Append(root).Append(' ').Append(key).Append('\n');
            if (key.EndsWith(".usd", StringComparison.OrdinalIgnoreCase))
                sb.Append(Fnv(text));
            else
                sb.Append(Show(text)).Append('\n');
        }
    }

    private static void Files(StringBuilder sb, Host host)
    {
        Dump(sb, "plugin", host.Plugin);
        Dump(sb, "profiles", host.Profiles);
        sb.Append("--- log\n");
        foreach (string line in host.Lines.Lines)
            sb.Append(line).Append('\n');
    }

    private static string D(double value) => value.ToString(CultureInfo.InvariantCulture);

    private static void Summary(StringBuilder sb, Live live)
    {
        VtankSettingsProfileSerializer.AllSettings s = live.Settings;
        CombatSettings c = s.Combat;
        sb.Append("items ").Append(string.Join("|", c.CombatItemNames)).Append('\n');
        sb.Append("order ").Append(string.Join("|", c.CombatItemOrder)).Append('\n');
        sb.Append("byid ").Append(string.Join(";", c.CombatItemNamesById.Select(
            static pair => pair.Key.ToString(CultureInfo.InvariantCulture) + "=" + pair.Value))).Append('\n');
        sb.Append("consumables ").Append(string.Join("|", c.ConsumableNames)).Append('\n');
        sb.Append("categories ").Append(string.Join(";", c.ConsumableCategories.Select(
            static pair => pair.Key + "=" + ((int)pair.Value).ToString(CultureInfo.InvariantCulture)))).Append('\n');
        sb.Append("nobuff ").Append(string.Join("|", live.NoBuff)).Append('\n');
        sb.Append("logs ").Append(string.Join("|", live.Channels)).Append('\n');
        sb.Append("power ").Append(c.AttackPower.ToString(CultureInfo.InvariantCulture)).Append('\n');
        sb.Append("meta ").Append(c.MetaState).Append('\n');
        sb.Append("dynamic ").Append(string.Join(";", c.DynamicSettings.Select(static pair =>
            pair.Key + "=" + ((int)pair.Value.Kind).ToString(CultureInfo.InvariantCulture)
            + ":" + D(pair.Value.Number) + ":" + pair.Value.Text + ":" + B(pair.Value.Boolean)))).Append('\n');
        sb.Append("rules ").Append(string.Join(";", c.Rules.Select(static rule =>
            rule.Expression + "[" + rule.Actions.WeaponName + "/" + rule.Actions.OffhandName + "/"
            + rule.Actions.Priority.ToString(CultureInfo.InvariantCulture) + "/"
            + ((int)rule.Actions.Flags).ToString(CultureInfo.InvariantCulture) + "]"))).Append('\n');
        sb.Append("extra ").Append(string.Join("|", s.Buffs.ExtraBuffSpellNames)).Append('\n');
        sb.Append("enchant ").Append(string.Join(";", s.Buffs.ItemEnchantRows.Select(
            static row => row.ItemName + "=" + row.SpellName))).Append('\n');
        sb.Append("peers ").Append(B(s.Vitals.HelpNetworkPeers)).Append('\n');
        sb.Append("give ").Append(D(s.Inventory.GiveRangeMeters)).Append('\n');
        sb.Append("classifier ").Append(s.Inventory.Loot.ExternalClassifierId).Append('\n');
        sb.Append("loot ").Append(string.Join(";", s.Inventory.Loot.Rules.Select(static rule =>
            rule.Name + "|" + rule.Expression + "|" + ((int)rule.Action).ToString(CultureInfo.InvariantCulture)
            + "|" + rule.KeepCount.ToString(CultureInfo.InvariantCulture)))).Append('\n');
        sb.Append("pathing ").Append(s.Navigation.ClientPathing.ToString()).Append('\n');
        sb.Append("jumps ").Append(s.Navigation.JumpAttempts.ToString(CultureInfo.InvariantCulture)).Append('\n');
        sb.Append("range ").Append(D(c.MaximumRange)).Append('\n');
    }

    private static void FirstRun(StringBuilder sb)
    {
        Host host = NewHost();
        var live = new Live();
        var store = new MossTankProfileStore(host);
        sb.Append("bind ").Append(B(store.BindCharacter("+Acdream"))).Append('\n');
        sb.Append("load ").Append(live.Load(store)).Append('\n');
        sb.Append("save ").Append(B(live.Save(store))).Append('\n');
        sb.Append("selected ").Append(store.Selected).Append('\n');
        Files(sb, host);
    }

    private static void SetValues(Live live)
    {
        CombatSettings c = live.Settings.Combat;
        c.CombatItemNames.Add("Sword");
        c.CombatItemNames.Add("Bow");
        c.CombatItemOrder.Add("Bow");
        c.CombatItemObjectIds.Add(0x5001);
        c.CombatItemNamesById[0x5001] = "Bow";
        c.CombatItemNamesById[0x5002] = "Gone";
        c.ConsumableNames.Add("Mana Potion");
        c.ConsumableNames.Add("Healing Kit");
        c.ConsumableCategories["Healing Kit"] = ConsumableCategory.HealthKit;
        c.ConsumableCategories["Mana Potion"] = ConsumableCategory.ManaKit;
        live.NoBuff.Add("Wand");
        live.Channels.Add("Tells");
        live.Channels.Add("Fellow");
        c.AttackPower = 0.75f;
        c.MetaState = "Hunting";
        c.DynamicSettings["MaximumRange"] = MonsterValue.FromNumber(25.5);
        c.DynamicSettings["UseRecklessness"] = MonsterValue.FromBoolean(true);
        c.Rules.Clear();
        c.Rules.Add(MonsterRule.Fresh("DEFAULT"));
        c.Rules.Add(new MonsterRule("Drudge", new MonsterRuleActions { WeaponName = "Bow" }));
        live.Settings.Buffs.ExtraBuffSpellNames.Add("Strength Self VII");
        live.Settings.Buffs.ItemEnchantRows.Add(new BuffItemEnchantRow("Bow", "Blood Drinker Self VII"));
        live.Settings.Vitals.HelpNetworkPeers = true;
        live.Settings.Inventory.GiveRangeMeters = 20.5;
        live.Settings.Inventory.Loot.ExternalClassifierId = "  vtc  ";
        live.Settings.Inventory.Loot.Rules.Add(new LootRule
        {
            Name = "Keep peas",
            Expression = "Pea",
            Action = LootAction.Salvage,
            KeepCount = 3,
        });
        live.Settings.Navigation.ClientPathing = ClientPathing.Always;
        live.Settings.Navigation.JumpAttempts = 5;
    }

    private static void SideCarValues(StringBuilder sb)
    {
        Host host = NewHost();
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        live.Load(store);
        SetValues(live);
        sb.Append("save ").Append(B(live.Save(store))).Append('\n');
        Files(sb, host);
        sb.Append("--- reload\n");
        var fresh = new Live();
        var again = new MossTankProfileStore(host);
        again.BindCharacter("Acdream");
        sb.Append("load ").Append(fresh.Load(again)).Append('\n');
        Summary(sb, fresh);
    }

    private static void CreateAndDelete(StringBuilder sb)
    {
        Host host = NewHost();
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        live.Load(store);
        live.Save(store);
        bool created = store.Create("raid", copyCurrent: false, live.Settings, live.NoBuff, live.Channels, out string notice);
        sb.Append("create ").Append(B(created)).Append(' ').Append(notice).Append('\n');
        sb.Append("selected ").Append(store.Selected).Append('\n');
        sb.Append("exists raid ").Append(B(store.Exists("raid")))
            .Append(" nope ").Append(B(store.Exists("nope"))).Append('\n');
        sb.Append("select bychar ").Append(B(store.Select("By char"))).Append('\n');
        sb.Append("select raid ").Append(B(store.Select("--Acdream_Coldeve_raid.usd"))).Append('\n');
        sb.Append("selected ").Append(store.Selected).Append('\n');
        sb.Append("names ").Append(string.Join("|", store.AvailableNames)).Append('\n');
        bool deleted = store.Delete(out string deleteNotice);
        sb.Append("delete ").Append(B(deleted)).Append(' ').Append(deleteNotice).Append('\n');
        sb.Append("selected ").Append(store.Selected).Append('\n');
        Files(sb, host);
    }

    private static void Corrupt(StringBuilder sb)
    {
        Host host = NewHost();
        host.Profiles.Text["mosstank/profiles/--Acdream_Coldeve.usd"] = "not a usd\r\n";
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(live.Load(store)).Append('\n');
        sb.Append("failure ").Append(store.LoadFailureNotice).Append('\n');
        sb.Append("recovery ").Append(store.RecoveryNotice).Append('\n');
        sb.Append("save ").Append(B(live.Save(store))).Append('\n');
        Files(sb, host);
    }

    private static void SetInAll(StringBuilder sb)
    {
        Host host = NewHost();
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        live.Load(store);
        live.Save(store);
        store.Create("raid", copyCurrent: false, live.Settings, live.NoBuff, live.Channels, out _);
        live.Settings.Combat.MaximumRange = 42d;
        live.Settings.Combat.DynamicSettings["AttackDistance"] = MonsterValue.FromNumber(42d);
        int count = store.SetOptionInAll("attackdistance", live.Settings);
        sb.Append("setinall ").Append(count.ToString(CultureInfo.InvariantCulture)).Append('\n');
        Files(sb, host);
    }

    private static string Sha(string text) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(text)));

    private static void Legacy(StringBuilder sb)
    {
        Host host = NewHost();
        host.Plugin.Text["profiles/macro/" + Sha("char:ACDREAM") + ".json"] =
            "{\"Version\":6,\"ItemNames\":[\"Axe\"],\"ConsumableNames\":[\"Pea\"],\"ConsumableCategories\":{\"Pea\":7},\"Combat\":{\"MaximumRange\":30,\"Rules\":[{\"Expression\":\"Drudge\",\"Priority\":9}]}}";
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(live.Load(store)).Append('\n');
        Summary(sb, live);
        Files(sb, host);
    }

    private static void Roster(StringBuilder sb)
    {
        Host host = NewHost();
        host.Plugin.Text["profiles/index.json"] =
            "{\"Profiles\":[{\"Name\":\"Hunt\",\"Owner\":\"Acdream\"},{\"Name\":\"Other\",\"Owner\":\"Someone\"}]}";
        host.Plugin.Text["profiles/macro/" + Sha("named:HUNT") + ".json"] = "{\"ItemNames\":[\"Bow\"]}";
        var live = new Live();
        var store = new MossTankProfileStore(host);
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(live.Load(store)).Append('\n');
        sb.Append("names ").Append(string.Join("|", store.AvailableNames)).Append('\n');
        Files(sb, host);
    }

    private static void Preferences(StringBuilder sb)
    {
        Host host = NewHost();
        var store = new MossTankProfileStore(host);
        store.SetMetaIntervalMilliseconds(10);
        store.SetGameDbCheckIntervalHours(500);
        store.BindCharacter("Acdream");
        store.SetMineOnly(false);
        sb.Append("meta ").Append(store.MetaIntervalMilliseconds.ToString(CultureInfo.InvariantCulture))
            .Append(" db ").Append(store.GameDbCheckIntervalHours.ToString(CultureInfo.InvariantCulture))
            .Append(" mine ").Append(B(store.MineOnly)).Append('\n');
        Files(sb, host);
    }

    private static string Rules(IEnumerable<LootRule> rules) => string.Join(";", rules.Select(static rule =>
        rule.Name + "|" + rule.Expression + "|" + ((int)rule.Action).ToString(CultureInfo.InvariantCulture)
        + "|" + rule.KeepCount.ToString(CultureInfo.InvariantCulture)
        + "|" + rule.VtankRequirements.Count.ToString(CultureInfo.InvariantCulture)));

    private static void LootState(StringBuilder sb, MossTankLootProfileStore store)
    {
        sb.Append("selected ").Append(store.Selected)
            .Append(" active ").Append(B(store.HasActiveProfile))
            .Append(" partial ").Append(B(store.ActiveProfileIsPartial)).Append('\n');
        sb.Append("names ").Append(string.Join("|", store.AvailableNames)).Append('\n');
    }

    private static LootRule Peas() => new()
    {
        Name = "Keep peas",
        Expression = "Pea",
        Action = LootAction.KeepUpTo,
        KeepCount = 25,
    };

    private static void LootFirstRun(StringBuilder sb)
    {
        Host host = NewHost();
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        var settings = new LootSettings();
        sb.Append("bind ").Append(B(store.BindCharacter("Acdream"))).Append('\n');
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append('\n');
        rules.Add(Peas());
        store.SaveCurrent(rules, settings);
        LootState(sb, store);
        Files(sb, host);
    }

    private static void LootNamed(StringBuilder sb)
    {
        Host host = NewHost();
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        var settings = new LootSettings();
        store.BindCharacter("Acdream");
        store.LoadCurrent(rules, settings);
        rules.Add(Peas());
        store.SaveCurrent(rules, settings);
        bool created = store.Create("Shared", copyCurrent: true, rules, out string notice, settings);
        sb.Append("create ").Append(B(created)).Append(' ').Append(notice).Append('\n');
        LootState(sb, store);
        sb.Append("select bychar ").Append(B(store.Select("By char"))).Append('\n');
        rules.Clear();
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append(' ').Append(Rules(rules)).Append('\n');
        sb.Append("exists ").Append(B(store.Exists("shared"))).Append(' ').Append(B(store.Exists("nope"))).Append('\n');
        sb.Append("select shared ").Append(B(store.Select("Shared"))).Append('\n');
        rules.Clear();
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append(' ').Append(Rules(rules)).Append('\n');
        var named = new List<LootRule>();
        sb.Append("named ").Append(B(store.TryLoadNamed("By char", named))).Append(' ').Append(Rules(named)).Append('\n');
        bool deleted = store.Delete(out string deleteNotice);
        sb.Append("delete ").Append(B(deleted)).Append(' ').Append(deleteNotice).Append('\n');
        LootState(sb, store);
        rules.Clear();
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append(' ').Append(Rules(rules)).Append('\n');
        LootState(sb, store);
        Files(sb, host);
    }

    private static void LootPartial(StringBuilder sb)
    {
        Host host = NewHost();
        string text = MossTankLootProfileStore.SerializeRules(
            [Peas(), new LootRule { Name = "Second rule", Expression = "Gem" }]);
        host.Profiles.Text["mosstank/loot/--Acdream_Coldeve.utl"] =
            text[..(text.IndexOf("Second", StringComparison.Ordinal) + 3)];
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        var settings = new LootSettings();
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append(' ').Append(Rules(rules)).Append('\n');
        sb.Append("notice ").Append(store.LoadNotice).Append('\n');
        rules.Add(new LootRule { Name = "Added" });
        store.SaveCurrent(rules, settings);
        bool created = store.Create("Copy", copyCurrent: true, rules, out string notice, settings);
        sb.Append("create ").Append(B(created)).Append(' ').Append(notice).Append('\n');
        LootState(sb, store);
        Files(sb, host);
    }

    private static void LootImport(StringBuilder sb)
    {
        Host host = NewHost();
        host.Plugin.Text["imports/VtcOld.utl"] = MossTankLootProfileStore.SerializeRules([Peas()]);
        host.Plugin.Text["imports/Broken.utl"] = "not a loot profile";
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        var settings = new LootSettings();
        store.BindCharacter("Acdream");
        store.LoadCurrent(rules, settings);
        bool imported = store.TryImportLegacy("vtcold.utl", rules, settings, out string notice);
        sb.Append("import ").Append(B(imported)).Append(' ').Append(notice).Append(' ').Append(Rules(rules)).Append('\n');
        bool broken = store.TryImportLegacy("Broken", rules, settings, out string brokenNotice);
        sb.Append("import ").Append(B(broken)).Append(' ').Append(brokenNotice).Append('\n');
        bool missing = store.TryImportLegacy("Nope", rules, settings, out string missingNotice);
        sb.Append("import ").Append(B(missing)).Append(' ').Append(missingNotice).Append('\n');
        LootState(sb, store);
        Files(sb, host);
    }

    private static void LootLegacy(StringBuilder sb)
    {
        Host host = NewHost();
        host.Plugin.Text["profiles/loot/" + Sha("char:ACDREAM") + ".json"] =
            "{\"Rules\":[{\"Name\":\"Old\",\"Expression\":\"Salvage\",\"Action\":2}],\"SalvageCombine\":{\"DefaultCombineString\":\"1-10\",\"MaterialValueModeValues\":{\"5\":3}}}";
        host.Plugin.Text["profiles/loot/index.json"] = "{\"Names\":[\"Hunt\",\"By char\",\"Gone\"]}";
        host.Plugin.Text["profiles/loot/" + Sha("named:HUNT") + ".json"] =
            "{\"Rules\":[{\"Name\":\"Hunt rule\",\"Requirements\":[{\"Type\":1,\"Payload\":\"x\\r\\n\"}],\"HasImportedRequirements\":true}]}";
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        var settings = new LootSettings();
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(store.LoadCurrent(rules, settings)).Append(' ').Append(Rules(rules)).Append('\n');
        sb.Append("salvage ").Append(settings.SalvageCombine.DefaultCombineString).Append('\n');
        LootState(sb, store);
        Files(sb, host);
    }

    private static void LootUnsupported(StringBuilder sb)
    {
        Host host = NewHost();
        var radar = new LootRule
        {
            Name = "Radar",
            VtankRequirements = [new VtankLootRequirement { Type = 2, Payload = "0\r\n95\r\n1\r\n" }],
            HasImportedRequirements = true,
        };
        host.Profiles.Text["mosstank/loot/--Acdream_Coldeve.utl"] =
            MossTankLootProfileStore.SerializeRules([radar]);
        var store = new MossTankLootProfileStore(host);
        var rules = new List<LootRule>();
        store.BindCharacter("Acdream");
        sb.Append("load ").Append(store.LoadCurrent(rules)).Append(' ').Append(Rules(rules)).Append('\n');
        Files(sb, host);
    }
}
