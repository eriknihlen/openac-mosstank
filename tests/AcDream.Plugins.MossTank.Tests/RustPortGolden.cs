using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Golden data for the Rust conversion (rust/, branch rust-port). Reads the
/// cases in rust/crates/mosstank-formats/tests/golden/cases.txt, runs each
/// through this plugin's readers and writers, and writes what they gave to
/// cases.out beside it, which the Rust tests compare against. It only runs
/// when MOSSTANK_RUST_GOLDEN is set (to anything), so the normal test run
/// never rewrites the file:
///
///     MOSSTANK_RUST_GOLDEN=1 dotnet test --filter RustPortGolden
///
/// The text format (dumps, escapes) is the Rust test's; change both together.
/// The settings crate's cases (rust/crates/mosstank-settings/tests/golden)
/// are written by the same run, through <see cref="RustPortSettingsGolden"/>.
/// </summary>
public sealed class RustPortGolden
{
    private static string RepositoryRoot()
    {
        string? directory = AppContext.BaseDirectory;
        while (directory is not null && !File.Exists(Path.Combine(directory, "MossTank.slnx")))
            directory = Path.GetDirectoryName(directory);
        return directory ?? throw new InvalidOperationException("repository root not found");
    }

    private sealed class NoSpells : ISpellCatalog
    {
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];

        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            info = default;
            return false;
        }
    }

    [Fact]
    public void WriteGoldenData()
    {
        if (Environment.GetEnvironmentVariable("MOSSTANK_RUST_GOLDEN") is null)
            return;
        string root = RepositoryRoot();
        string golden = Path.Combine(root, "rust", "crates", "mosstank-formats", "tests", "golden");
        string fixtures = Path.Combine(root, "tests", "AcDream.Plugins.MossTank.Tests", "Fixtures");
        var output = new StringBuilder();
        var full = new StringBuilder();
        foreach (string raw in File.ReadAllLines(Path.Combine(golden, "cases.txt")))
        {
            if (raw.Length == 0 || raw.StartsWith('#'))
                continue;
            string[] parts = raw.Split(" ::: ", 3);
            string kind = parts[0];
            string name = parts[1];
            string field = parts.Length > 2 ? parts[2] : string.Empty;
            bool fixture = field.StartsWith('@');
            string input = fixture
                ? File.ReadAllText(Path.Combine(fixtures, field[1..]))
                : Unescape(field);
            string result = Run(kind, input);
            output.Append("##### ").Append(kind).Append(' ').Append(name).Append('\n');
            output.Append(fixture ? Hashed(result) : result);
            output.Append('\n');
            full.Append("##### ").Append(kind).Append(' ').Append(name).Append('\n');
            full.Append(result);
            full.Append('\n');
        }
        File.WriteAllText(Path.Combine(golden, "cases.out"), output.ToString(), new UTF8Encoding(false));
        // The fixtures' results in full, for comparing a failing Rust run
        // by eye (the committed file keeps only their hashes).
        string fullDirectory = Path.Combine(root, "rust", "target", "golden");
        Directory.CreateDirectory(fullDirectory);
        File.WriteAllText(Path.Combine(fullDirectory, "cases.full.out"), full.ToString(), new UTF8Encoding(false));
        WriteSettingsGoldenData(root, fixtures, fullDirectory);
    }

    /// <summary>
    /// The same for the settings crate: its cases in
    /// rust/crates/mosstank-settings/tests/golden/cases.txt, run by
    /// <see cref="RustPortSettingsGolden"/>; large results kept as hashes.
    /// </summary>
    private static void WriteSettingsGoldenData(string root, string fixtures, string fullDirectory)
    {
        string golden = Path.Combine(root, "rust", "crates", "mosstank-settings", "tests", "golden");
        var output = new StringBuilder();
        var full = new StringBuilder();
        foreach (string raw in File.ReadAllLines(Path.Combine(golden, "cases.txt")))
        {
            if (raw.Length == 0 || raw.StartsWith('#'))
                continue;
            string[] parts = raw.Split(" ::: ", 3);
            string kind = parts[0];
            string name = parts[1];
            string field = parts.Length > 2 ? parts[2] : string.Empty;
            string input = field.StartsWith('@')
                ? File.ReadAllText(Path.Combine(fixtures, field[1..]))
                : Unescape(field);
            string result = RustPortSettingsGolden.Run(kind, name, input);
            output.Append("##### ").Append(kind).Append(' ').Append(name).Append('\n');
            output.Append(RustPortSettingsGolden.IsLarge(kind, field) ? Hashed(result) : result);
            output.Append('\n');
            full.Append("##### ").Append(kind).Append(' ').Append(name).Append('\n');
            full.Append(result);
            full.Append('\n');
        }
        File.WriteAllText(Path.Combine(golden, "cases.out"), output.ToString(), new UTF8Encoding(false));
        File.WriteAllText(Path.Combine(fullDirectory, "settings.full.out"), full.ToString(), new UTF8Encoding(false));
    }

    /// <summary>
    /// A fixture's result, each part (the dump, then each "--- " section)
    /// replaced by its length and FNV-1a 64 hash over its UTF-8 bytes.
    /// </summary>
    private static string Hashed(string result)
    {
        var sb = new StringBuilder();
        var part = new StringBuilder();
        void Flush()
        {
            byte[] bytes = Encoding.UTF8.GetBytes(part.ToString());
            ulong hash = 0xcbf29ce484222325UL;
            foreach (byte b in bytes)
            {
                hash ^= b;
                hash *= 0x100000001b3UL;
            }
            sb.Append("fnv64=").Append(hash.ToString("x16", CultureInfo.InvariantCulture))
              .Append(" bytes=").Append(bytes.Length.ToString(CultureInfo.InvariantCulture)).Append('\n');
            part.Clear();
        }
        foreach (string line in result.Split('\n'))
        {
            if (line.StartsWith("--- ", StringComparison.Ordinal))
            {
                Flush();
                sb.Append(line).Append('\n');
                continue;
            }
            part.Append(line).Append('\n');
        }
        Flush();
        return sb.ToString();
    }

    private static string Run(string kind, string input)
    {
        var spells = new NoSpells();
        var sb = new StringBuilder();
        try
        {
            switch (kind)
            {
                case "met":
                case "af-meta":
                {
                    bool loaded = kind == "met"
                        ? VtankMetaProfileSerializer.TryLoad(input, out MetaProfile profile, out string error)
                        : MetafSerializer.TryLoadMeta(input, spells, out profile, out error);
                    if (!loaded)
                        return "ERR " + error + "\n";
                    sb.Append(Profile(profile));
                    sb.Append("--- met:\n").Append(Show(Try(() => VtankMetaProfileSerializer.Save(profile)))).Append('\n');
                    sb.Append("--- af:\n").Append(Show(Try(() => StripHeader(MetafSerializer.SaveMeta(profile))))).Append('\n');
                    break;
                }
                case "nav":
                case "af-nav":
                {
                    var route = new NavigationSettings();
                    bool loaded = kind == "nav"
                        ? VtankNavRouteSerializer.TryLoad(input, route, spells, out string error)
                        : MetafSerializer.TryLoadNav(input, route, spells, out error);
                    if (!loaded)
                        return "ERR " + error + "\n";
                    Nav(sb, route, 0);
                    sb.Append("--- nav:\n").Append(Show(Try(() => VtankNavRouteSerializer.Save(route)))).Append('\n');
                    sb.Append("--- af:\n").Append(Show(Try(() => StripHeader(MetafSerializer.SaveNav(route))))).Append('\n');
                    break;
                }
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

    private static string Try(Func<string> run)
    {
        try
        {
            return run();
        }
        catch (Exception error)
        {
            return "!! " + error.Message;
        }
    }

    private static string StripHeader(string af)
    {
        string metaHeader = MetafSerializer.SaveMeta(new MetaProfile());
        metaHeader = metaHeader[..^2];
        string navOnly = MetafSerializer.SaveNav(new NavigationSettings { Mode = RouteMode.Once });
        string navHeader = navOnly[..navOnly.IndexOf("NAV:", StringComparison.Ordinal)];
        if (af.StartsWith(navHeader, StringComparison.Ordinal))
            return "<nav-header>" + af[navHeader.Length..];
        if (af.StartsWith(metaHeader, StringComparison.Ordinal))
            return "<meta-header>" + af[metaHeader.Length..];
        return af;
    }

    private static string Unescape(string text)
    {
        var sb = new StringBuilder();
        for (int i = 0; i < text.Length; i++)
        {
            char c = text[i];
            if (c != '\\' || i + 1 >= text.Length)
            {
                sb.Append(c);
                continue;
            }
            char next = text[++i];
            switch (next)
            {
                case 'r': sb.Append('\r'); break;
                case 'n': sb.Append('\n'); break;
                case 't': sb.Append('\t'); break;
                case '\\': sb.Append('\\'); break;
                case 'u':
                    int close = text.IndexOf('}', i);
                    sb.Append(char.ConvertFromUtf32(int.Parse(text[(i + 2)..close], NumberStyles.HexNumber, CultureInfo.InvariantCulture)));
                    i = close;
                    break;
                default: sb.Append('\\').Append(next); break;
            }
        }
        return sb.ToString();
    }

    private static string Show(string s) => s.Replace("\r", "\\r").Replace("\n", "\\n\n").Replace("\t", "\\t");

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

    private static string R(double d) => d.ToString("R", CultureInfo.InvariantCulture);

    private static void Cond(StringBuilder sb, MetaCondition c, int depth)
    {
        sb.Append(' ', depth).Append("C ").Append(c.Kind).Append(" t=").Append(Esc(c.Text)).Append(" s=").Append(Esc(c.SecondaryText))
          .Append(" n=").Append(R(c.Number)).Append(" n2=").Append(R(c.SecondaryNumber)).Append(" n3=").Append(R(c.TertiaryNumber)).Append('\n');
        foreach (MetaCondition child in c.Children)
            Cond(sb, child, depth + 1);
    }

    private static void Act(StringBuilder sb, MetaAction a, int depth)
    {
        sb.Append(' ', depth).Append("A ").Append(a.Kind).Append(" t=").Append(Esc(a.Text)).Append(" s=").Append(Esc(a.SecondaryText))
          .Append(" n=").Append(R(a.Number)).Append(" n2=").Append(R(a.SecondaryNumber)).Append('\n');
        if (a.EmbeddedRoute is { } nav)
            Nav(sb, nav, depth + 1);
        foreach (MetaAction child in a.Children)
            Act(sb, child, depth + 1);
    }

    private static string Pos(PluginNavigationPosition p) =>
        $"{p.CellId:X8},{R(p.EastWest)},{R(p.NorthSouth)},{R(p.Elevation)},{p.HeadingDegrees.ToString("R", CultureInfo.InvariantCulture)},{p.IsOutdoor}";

    private static void Nav(StringBuilder sb, NavigationSettings n, int depth)
    {
        sb.Append(' ', depth).Append("route mode=").Append(n.Mode).Append(" follow=").Append(n.FollowTargetObjectId.ToString("X8", CultureInfo.InvariantCulture))
          .Append(' ').Append(Esc(n.FollowTargetName)).Append(" count=").Append(n.Waypoints.Count).Append('\n');
        foreach (RouteWaypoint w in n.Waypoints)
        {
            sb.Append(' ', depth + 1).Append("wp ").Append(w.Type).Append(" pos=").Append(Pos(w.Position)).Append(" ref=").Append(Pos(w.ReferencePosition))
              .Append(" id=").Append(w.ObjectId.ToString("X8", CultureInfo.InvariantCulture)).Append(" name=").Append(Esc(w.ObjectName))
              .Append(" class=").Append(w.LegacyObjectClass).Append(" valid=").Append(w.LegacyReferenceValid).Append(" text=").Append(Esc(w.Text))
              .Append(" dur=").Append(w.DurationMilliseconds).Append(" recall=").Append(w.Recall).Append(" rid=").Append(w.RecallSpellId)
              .Append(" rname=").Append(Esc(w.RecallSpellName)).Append(" jh=").Append(R(w.JumpHeadingDegrees)).Append(" js=").Append(w.JumpHoldShift)
              .Append(" jc=").Append(w.JumpChargeMilliseconds).Append(" jd=").Append(w.JumpDirection).Append('\n');
        }
    }

    private static string Profile(MetaProfile p)
    {
        var sb = new StringBuilder();
        foreach (MetaRule r in p.Rules)
        {
            sb.Append("rule state=").Append(Esc(r.State)).Append(" enabled=").Append(r.Enabled).Append('\n');
            Cond(sb, r.Condition, 1);
            Act(sb, r.Action, 1);
        }
        return sb.ToString();
    }
}
