using System.Reflection;
using System.Xml.Linq;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MossTankMarkupContractTests
{
    private static readonly string[] InteractiveElementNames =
    [
        "tab", "button", "toggle", "slider", "field", "menu", "list", "column",
    ];

    [Fact]
    public void InteractiveElementNames_IncludesColumn()
    {
        Assert.Contains("column", InteractiveElementNames);
    }

    /// <summary>
    /// The extra vulnerability column is the one column whose effect is not
    /// obvious from its heading: it casts on its own account, with no check
    /// box of its own to tick, so a row set to anything but None debuffs
    /// targets while every check box on the row is clear. Its heading has to
    /// say so, or the setting is invisible to whoever inherits the profile.
    /// </summary>
    [Fact]
    public void TheExtraVulnerabilityHeadingExplainsThatItCastsWithoutTheVulnCheck()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement heading = Assert.Single(
            root.Descendants("label"),
            static label => (string?)label.Attribute("text") == "Ex. Vuln");
        string tooltip = (string?)heading.Attribute("tooltip") ?? string.Empty;

        Assert.Contains("None", tooltip, StringComparison.Ordinal);
        Assert.Contains("Auto", tooltip, StringComparison.Ordinal);
        Assert.Contains("does NOT need V ticked", tooltip, StringComparison.Ordinal);
    }

    [Fact]
    public void VtankTabOrderAndEveryBindingResolveAgainstTheLivePanel()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.Equal(
            [
                "Options", "Profiles", "Vitals", "Monsters", "Items",
                "Consumables", "Buffs", "Route", "Meta", "UB", "UB settings",
                "Tinker",
            ],
            root.Elements("tab")
                .Select(static tab => (string?)tab.Attribute("text")));

        PropertyInfo[] properties = typeof(MossTankPanel).GetProperties(
            BindingFlags.Instance | BindingFlags.Public);
        var byName = properties.ToDictionary(
            static property => property.Name,
            StringComparer.Ordinal);

        foreach (XAttribute attribute in root.DescendantsAndSelf().Attributes())
        {
            string value = attribute.Value;
            if (!value.Contains('{', StringComparison.Ordinal))
                continue;
            Assert.Matches("^\\{[^{}]+\\}$", value);
            string name = value[1..^1];
            Assert.True(
                byName.ContainsKey(name),
                $"Markup binding {value} on <{attribute.Parent?.Name}> has no "
                + $"public MossTankPanel property.");
        }
    }

    [Fact]
    public void EveryInteractiveBindingMatchesTheRetainedUiDelegateShape()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        PropertyInfo[] properties = typeof(MossTankPanel).GetProperties(
            BindingFlags.Instance | BindingFlags.Public);
        var byName = properties.ToDictionary(
            static property => property.Name,
            StringComparer.Ordinal);

        foreach (XElement element in root.DescendantsAndSelf())
            AssertElementBindingsMatchRetainedUiDelegateShape(element, byName);
    }

    private static void AssertElementBindingsMatchRetainedUiDelegateShape(
        XElement element,
        IReadOnlyDictionary<string, PropertyInfo> byName)
    {
        if (element.Name.LocalName == "column")
        {
            AssertBindingType(element, "onchange", typeof(Action<int>), byName);
            AssertBindingType(element, "onclick", typeof(Action<int>), byName);
            return;
        }

        AssertBindingType(element, "onclick", typeof(Action), byName);
        AssertBindingType(
            element,
            "onsubmit",
            typeof(Action<string>),
            byName);

        Type? changeType = element.Name.LocalName switch
        {
            "field" or "menu" => typeof(Action<string>),
            "slider" => typeof(Action<float>),
            "list" => typeof(Action<int>),
            _ => null,
        };
        if (changeType is not null)
            AssertBindingType(element, "onchange", changeType, byName);
    }

    private sealed class ColumnBindingProbe
    {
        public Action<int> RowAction { get; } = _ => { };
        public Action PlainAction { get; } = () => { };
    }

    [Fact]
    public void Column_OnchangeAndOnclick_MustBeActionOfInt()
    {
        var byName = typeof(ColumnBindingProbe)
            .GetProperties(BindingFlags.Instance | BindingFlags.Public)
            .ToDictionary(static property => property.Name, StringComparer.Ordinal);

        var goodColumn = new XElement(
            "column",
            new XAttribute("type", "check"),
            new XAttribute("onchange", "{RowAction}"));
        AssertElementBindingsMatchRetainedUiDelegateShape(goodColumn, byName);

        var badColumn = new XElement(
            "column",
            new XAttribute("type", "icon"),
            new XAttribute("onclick", "{PlainAction}"));
        Assert.Throws<Xunit.Sdk.EqualException>(
            () => AssertElementBindingsMatchRetainedUiDelegateShape(badColumn, byName));
    }

    [Fact]
    public void EveryInteractiveControlDeclaresARealHandlerBinding()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        HashSet<string> interactive = new(InteractiveElementNames, StringComparer.Ordinal);

        XElement[] controls = root.Descendants()
            .Where(element => interactive.Contains(element.Name.LocalName))
            .ToArray();
        Assert.NotEmpty(controls);

        foreach (XElement control in controls)
        {
            XAttribute? handler = control.Attribute("onclick")
                ?? control.Attribute("onchange")
                ?? control.Attribute("onsubmit");
            Assert.NotNull(handler);
            Assert.NotEqual("false", (string?)control.Attribute("enabled"));
        }
    }

    [Fact]
    public void EveryVtankTabIsBackedByALivePanelSurface()
    {
        var panel = new MossTankPanel(new StubHost());

        Assert.True(panel.OptionsTabEnabled);
        Assert.True(panel.VitalsTabEnabled);
        Assert.True(panel.MonstersTabEnabled);
        Assert.True(panel.BuffsTabEnabled);
        Assert.True(panel.ProfilesTabEnabled);
        Assert.True(panel.ItemsTabEnabled);
        Assert.True(panel.ConsumablesTabEnabled);
        Assert.True(panel.RouteTabEnabled);
        Assert.True(panel.MetaTabEnabled);
    }

    [Fact]
    public void MonstersGridHasVtanksTwentyThreeColumnsInOrderWithAuthenticHeaderTooltips()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement monstersGroup = root.Elements("group")
            .Single(static g => (string?)g.Attribute("visible") == "{MonstersVisible}");

        XElement list = Assert.Single(monstersGroup.Elements("list"));
        XElement[] columns = list.Elements("column").ToArray();
        Assert.Equal(24, columns.Length);

        string[] expectedTypes =
        [
            "check", "check", "check", "check", "check", "check", "check",
            "check", "check", "check", "check", "check", "check", "check",
            "text", "text", "text", "text", "text", "text", "text",
            "icon", "icon", "text",
        ];
        Assert.Equal(expectedTypes, columns.Select(c => (string?)c.Attribute("type")));

        Dictionary<string, string?> tooltipsByHeaderText = monstersGroup.Elements("label")
            .ToDictionary(
                static l => (string?)l.Attribute("text") ?? string.Empty,
                static l => (string?)l.Attribute("tooltip"));
        Assert.Equal("Fester", tooltipsByHeaderText["F"]);
        Assert.Equal("Broadside of a Barn", tooltipsByHeaderText["B"]);
        Assert.Equal("Gravity Well", tooltipsByHeaderText["G"]);
        Assert.Equal("Imperil", tooltipsByHeaderText["I"]);
        Assert.Equal("Yield", tooltipsByHeaderText["Y"]);
        Assert.Equal("Vuln (Element)", tooltipsByHeaderText["V"]);
        Assert.Equal("Attack", tooltipsByHeaderText["A"]);
        Assert.Equal("Ring Spell", tooltipsByHeaderText["R"]);
        Assert.Equal("Streak", tooltipsByHeaderText["S"]);
        Assert.Equal("Weakening Curse", tooltipsByHeaderText["WC"]);
        Assert.Equal("Festering Curse", tooltipsByHeaderText["FC"]);
        Assert.Equal("Corruption", tooltipsByHeaderText["Cp"]);
        Assert.Equal("Destructive Curse", tooltipsByHeaderText["DC"]);
        Assert.Equal("Corrosion", tooltipsByHeaderText["Cs"]);
        Assert.Equal("Priority", tooltipsByHeaderText["P"]);
    }

    [Fact]
    public void ItemsTabIsVtankOnlyPlusTheAcceptedSliceOneRemoveButton()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement itemsGroup = root.Elements("group")
            .Single(static g => (string?)g.Attribute("visible") == "{ItemsVisible}");

        Assert.Single(itemsGroup.Elements("list"));
        string[] buttonTexts = itemsGroup.Elements("button")
            .Select(static b => (string?)b.Attribute("text") ?? string.Empty)
            .ToArray();
        Assert.Equal(["Add", "Add (no buffs)", "Remove"], buttonTexts);

        // Nothing else — no toggle, no slider, and no label reads a
        // MossTank-only status/notice property.
        Assert.Empty(itemsGroup.Elements("toggle"));
        Assert.Empty(itemsGroup.Elements("slider"));
        string[] labelBindings = itemsGroup.Elements("label")
            .Select(static l => (string?)l.Attribute("text") ?? string.Empty)
            .ToArray();
        Assert.DoesNotContain("{MonsterEquipmentText}", labelBindings);
        Assert.DoesNotContain("{RefillWornManaText}", labelBindings);
        Assert.DoesNotContain("{ItemManaRechargeStatus}", labelBindings);
        Assert.DoesNotContain("{ProfileNotice}", labelBindings);
    }

    [Fact]
    public void BuffsTabIsVtankOnlySixControls()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement buffsGroup = root.Elements("group")
            .Single(static g => (string?)g.Attribute("visible") == "{BuffsVisible}");

        Assert.Equal(2, buffsGroup.Elements("list").Count());
        string[] buttonTexts = buffsGroup.Elements("button")
            .Select(static b => (string?)b.Attribute("text") ?? string.Empty)
            .ToArray();
        Assert.Equal(["Add...", "Add..."], buttonTexts);
        string[] labelTexts = buffsGroup.Elements("label")
            .Select(static l => (string?)l.Attribute("text") ?? string.Empty)
            .ToArray();
        Assert.Equal(["Extra Buff Spells", "Blacklisted Buff Families"], labelTexts);

        Assert.Empty(buffsGroup.Elements("toggle"));
        Assert.Empty(buffsGroup.Elements("slider"));
        Assert.DoesNotContain("{BuffButtonText}", labelTexts);
        Assert.DoesNotContain("{BuffStatus}", labelTexts);
        Assert.DoesNotContain("{DifficultyText}", labelTexts);
        Assert.DoesNotContain("{RebuffText}", labelTexts);
        Assert.DoesNotContain("{Coverage}", labelTexts);
    }

    [Fact]
    public void LootEditorHasNoLeftoverBackButton()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank-loot-editor.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.DoesNotContain(
            root.Elements("button"),
            static b => (string?)b.Attribute("text") == "Back");
        Assert.NotNull(typeof(MossTankPanel).GetProperty("CloseLootEditor"));
    }

    [Fact]
    public void AuthoredShellFitsTheMinimumCanvasAndEverySizedChildFitsItsParent()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.Equal(984f, Number(root, "w"));
        Assert.Equal(271f, Number(root, "h"));
        AssertWithinParent(root);
    }

    /// <summary>
    /// The window resizes upwards from the size everything is laid out for,
    /// and never below it: the floor is the authored rectangle, so the look
    /// at the smallest size is the fixed-size look it has always had.
    /// Mutation: drop minw/minh and the floor becomes whatever the client
    /// happens to default to, letting a drag crush every page.
    /// </summary>
    [Fact]
    public void TheWindowResizesUpwardsFromItsAuthoredSizeAndNoSmaller()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.Equal("true", (string?)root.Attribute("resizable"));
        Assert.Equal(Number(root, "w"), Number(root, "minw"));
        Assert.Equal(Number(root, "h"), Number(root, "minh"));
        Assert.Equal("both", (string?)root.Attribute("resize"));
    }

    /// <summary>
    /// A resize only reaches a page through the page's own anchor, so every
    /// page has to hold all four edges or it sits in the corner of a bigger
    /// window with its own anchored rows going nowhere.
    /// Mutation: take the anchor off one page group and that whole tab stops
    /// growing.
    /// </summary>
    [Fact]
    public void EveryPageOfTheWindowHoldsAllFourEdges()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement[] pages = root.Elements("group")
            .Where(static group => ((string?)group.Attribute("visible"))?.EndsWith("Visible}", StringComparison.Ordinal) == true)
            .ToArray();
        Assert.Equal(12, pages.Length);

        foreach (XElement page in pages)
        {
            string anchor = (string?)page.Attribute("anchor") ?? string.Empty;
            foreach (string edge in new[] { "left", "top", "right", "bottom" })
            {
                Assert.True(
                    anchor.Contains(edge, StringComparison.OrdinalIgnoreCase),
                    $"The {(string?)page.Attribute("visible")} page does not hold its "
                    + $"{edge} edge, so it will not follow a resize.");
            }
        }
    }

    /// <summary>
    /// Whatever sits on a page's bottom edge has to be anchored to it, or a
    /// taller window leaves it stranded halfway up with empty space beneath.
    /// Mutation: put one of those rows back to the default left/top anchor
    /// and it stops following the bottom.
    /// </summary>
    [Fact]
    public void EveryRowOnAPagesBottomEdgeIsAnchoredToThatEdge()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        var stranded = new List<string>();
        foreach (XElement page in root.Elements("group"))
        {
            float pageHeight = Number(page, "h");
            foreach (XElement child in page.Elements())
            {
                if (child.Attribute("h") is null || child.Attribute("y") is null)
                    continue;
                float bottom = Number(child, "y") + Number(child, "h");
                if (bottom < pageHeight - 24f)
                    continue;
                string anchor = (string?)child.Attribute("anchor") ?? string.Empty;
                if (!anchor.Contains("bottom", StringComparison.OrdinalIgnoreCase))
                {
                    stranded.Add(
                        $"{(string?)page.Attribute("visible")} <{child.Name.LocalName} "
                        + $"text=\"{(string?)child.Attribute("text")}\"> at y={Number(child, "y")}");
                }
            }
        }

        Assert.Empty(stranded);
    }

    [Fact]
    public void EveryStretchingListDeclaresARealAnchor()
    {
        (string GroupVisible, int ExpectedListCount)[] stretchingListGroups =
        [
            ("MonstersVisible", 1),
            ("ItemsVisible", 1),
            ("ConsumablesVisible", 2),
            ("BuffsVisible", 2),
            ("RouteVisible", 1),
            ("MetaVisible", 1),
            ("UbSettingsVisible", 2),
        ];

        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        foreach ((string groupVisible, int expectedListCount) in stretchingListGroups)
        {
            XElement group = root.Elements("group")
                .Single(g => (string?)g.Attribute("visible") == $"{{{groupVisible}}}");
            XElement[] lists = group.Elements("list").ToArray();
            Assert.Equal(expectedListCount, lists.Length);
            foreach (XElement list in lists)
            {
                string? anchor = (string?)list.Attribute("anchor");
                Assert.False(
                    string.IsNullOrEmpty(anchor),
                    $"A <list> in the {groupVisible} tab has no anchor attribute — "
                    + "it will never grow with the window.");
            }
        }
    }

    /// <summary>
    /// The colour page's swatch is a drawn element whose background is
    /// bound, and the binding names a property of one of the three shapes
    /// a colour attribute resolves from, so the modal shows what it edits.
    /// Mutation: bind the background to a float and the swatch falls back
    /// to opaque white without a word.
    /// </summary>
    [Fact]
    public void TheColourPageHasASwatchBoundToAColourShapedProperty()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank-ub-color.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement swatch = Assert.Single(
            root.Descendants("group"),
            static group => ((string?)group.Attribute("background"))?.StartsWith('{') == true);
        string binding = ((string?)swatch.Attribute("background"))![1..^1];

        PropertyInfo? property = typeof(MossTankPanel).GetProperty(
            binding, BindingFlags.Instance | BindingFlags.Public);
        Assert.NotNull(property);
        Assert.Contains(property.PropertyType, new[] { typeof(uint), typeof(int), typeof(string) });
        Assert.False(string.IsNullOrWhiteSpace((string?)swatch.Attribute("border")));
    }

    /// <summary>
    /// A settings profile is described as a set someone can hand on, and
    /// all but eleven of the UB rows are written into one. A menu can only
    /// ever offer a profile that already exists, so the page needs the two
    /// buttons every other profile picker on this window has, or the tier
    /// is stuck on its default for good.
    /// </summary>
    [Fact]
    public void TheUbProfilePickerOffersANameAndBothWaysOfMakingOne()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement group = root.Elements("group")
            .Single(static g => (string?)g.Attribute("visible") == "{UbSettingsVisible}");

        Assert.Single(
            group.Descendants("field"),
            static field => (string?)field.Attribute("onsubmit") == "{CreateNamedUbProfile}"
                && (string?)field.Attribute("text") == "{UbProfileNameDraft}");
        Assert.Single(
            group.Descendants("button"),
            static button => (string?)button.Attribute("onclick") == "{CreateUbProfile}");
        Assert.Single(
            group.Descendants("button"),
            static button => (string?)button.Attribute("onclick") == "{CopyUbProfile}");
        Assert.Single(
            group.Descendants("menu"),
            static menu => (string?)menu.Attribute("onchange") == "{SelectUbProfile}");
    }

    [Fact]
    public void AssertWithinParent_CatchesAnUnsizedLabelNearTheBottomEdge()
    {
        var group = new XElement("group",
            new XAttribute("w", "848"), new XAttribute("h", "194"),
            new XElement("label", new XAttribute("x", "4"), new XAttribute("y", "190"),
                new XAttribute("text", "Notice")));

        Assert.Throws<Xunit.Sdk.TrueException>(() => AssertWithinParent(group));
    }

    [Fact]
    public void AuthoredControlsInTheSameContainerNeverOverlapASibling()
    {
        foreach (string path in Directory.GetFiles(AppContext.BaseDirectory, "mosstank*.xml"))
        {
            XDocument document = XDocument.Load(path);
            XElement root = Assert.IsType<XElement>(document.Root);
            AssertNoSiblingOverlap(root);
        }
    }

    [Fact]
    public void AssertNoSiblingOverlap_CatchesARealOverlapAndIgnoresGroupPagesAndTouchingEdges()
    {
        var overlapping = new XElement("panel",
            new XAttribute("w", "848"), new XAttribute("h", "194"),
            new XElement("button", new XAttribute("x", "336"), new XAttribute("y", "102"),
                new XAttribute("w", "120"), new XAttribute("h", "16")),
            new XElement("button", new XAttribute("x", "338"), new XAttribute("y", "108"),
                new XAttribute("w", "26"), new XAttribute("h", "20")));
        Assert.Throws<Xunit.Sdk.TrueException>(() => AssertNoSiblingOverlap(overlapping));

        var touchingEdges = new XElement("panel",
            new XAttribute("w", "848"), new XAttribute("h", "194"),
            new XElement("button", new XAttribute("x", "0"), new XAttribute("y", "0"),
                new XAttribute("w", "100"), new XAttribute("h", "20")),
            new XElement("button", new XAttribute("x", "100"), new XAttribute("y", "0"),
                new XAttribute("w", "100"), new XAttribute("h", "20")));
        AssertNoSiblingOverlap(touchingEdges); // must not throw

        var twoGroupPages = new XElement("panel",
            new XAttribute("w", "848"), new XAttribute("h", "236"),
            new XElement("group", new XAttribute("x", "8"), new XAttribute("y", "42"),
                new XAttribute("w", "848"), new XAttribute("h", "194")),
            new XElement("group", new XAttribute("x", "8"), new XAttribute("y", "42"),
                new XAttribute("w", "848"), new XAttribute("h", "194")));
        AssertNoSiblingOverlap(twoGroupPages); // must not throw
    }

    private static void AssertNoSiblingOverlap(XElement container)
    {
        XElement[] children = container.Elements()
            .Where(static child => child.Name.LocalName != "column")
            .ToArray();
        for (int i = 0; i < children.Length; i++)
        {
            for (int j = i + 1; j < children.Length; j++)
            {
                XElement a = children[i], b = children[j];
                if (a.Name.LocalName == "group" && b.Name.LocalName == "group")
                    continue;
                Assert.True(
                    !RectanglesOverlap(a, b),
                    $"<{a.Name}> text='{(string?)a.Attribute("text")}' @ "
                    + $"({Number(a, "x")},{Number(a, "y")},{Number(a, "w")},{Number(a, "h")}) "
                    + $"overlaps sibling <{b.Name}> text='{(string?)b.Attribute("text")}' @ "
                    + $"({Number(b, "x")},{Number(b, "y")},{Number(b, "w")},{Number(b, "h")}).");
            }
        }
        foreach (XElement child in children)
            AssertNoSiblingOverlap(child);
    }

    private static bool RectanglesOverlap(XElement a, XElement b)
    {
        float aw = Number(a, "w"), ah = Number(a, "h");
        float bw = Number(b, "w"), bh = Number(b, "h");
        if (aw <= 0f || ah <= 0f || bw <= 0f || bh <= 0f)
            return false; // an element with no declared size never "occupies" space
        float ax = Number(a, "x"), ay = Number(a, "y");
        float bx = Number(b, "x"), by = Number(b, "y");
        return ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
    }

    private static readonly Dictionary<string, (float Width, float Height)> ExpectedPopupBounds =
        new(StringComparer.OrdinalIgnoreCase)
        {
            ["mosstank-remote.xml"] = (166f, 209f),
            ["mosstank-advanced.xml"] = (650f, 560f),
            ["mosstank-loot-editor.xml"] = (268f, 300f),
            ["mosstank-buffpicker.xml"] = (268f, 236f),
            ["mosstank-metaeditor.xml"] = (630f, 174f),
            ["mosstank-ub-color.xml"] = (268f, 182f),
            ["mosstank-ub-list.xml"] = (268f, 300f),
            ["mosstank-ub-map.xml"] = (320f, 76f),
            ["mosstank-ub-clients.xml"] = (268f, 200f),
            ["mosstank-ub-dungeon.xml"] = (320f, 96f),
        };

    [Fact]
    public void SecondaryPopupPanelsFitTheirOwnBoundsAndEveryBindingResolves()
    {
        PropertyInfo[] properties = typeof(MossTankPanel).GetProperties(
            BindingFlags.Instance | BindingFlags.Public);
        var byName = properties.ToDictionary(
            static property => property.Name,
            StringComparer.Ordinal);

        string[] popupFileNames = Directory.GetFiles(AppContext.BaseDirectory, "mosstank*.xml")
            .Select(static path => Path.GetFileName(path)!)
            .Where(static name => !string.Equals(
                name, "mosstank.xml", StringComparison.OrdinalIgnoreCase))
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.NotEmpty(popupFileNames);

        foreach (string fileName in popupFileNames)
        {
            Assert.True(
                ExpectedPopupBounds.TryGetValue(fileName, out (float Width, float Height) expected),
                $"{fileName} has no expected width/height entry in "
                + $"{nameof(ExpectedPopupBounds)} — add one (VTank's own "
                + "secondary-view footprint, from the UI-views notes) before "
                + "this popup can be trusted.");

            XDocument document = XDocument.Load(
                Path.Combine(AppContext.BaseDirectory, fileName));
            XElement root = Assert.IsType<XElement>(document.Root);

            Assert.Equal(expected.Width, Number(root, "w"));
            Assert.Equal(expected.Height, Number(root, "h"));
            AssertWithinParent(root);
            foreach (XElement element in root.DescendantsAndSelf())
                AssertElementBindingsMatchRetainedUiDelegateShape(element, byName);

            foreach (XAttribute attribute in root.DescendantsAndSelf().Attributes())
            {
                string value = attribute.Value;
                if (!value.Contains('{', StringComparison.Ordinal))
                    continue;
                Assert.Matches("^\\{[^{}]+\\}$", value);
                string name = value[1..^1];
                Assert.True(
                    byName.ContainsKey(name),
                    $"Markup binding {value} on <{attribute.Parent?.Name}> in "
                    + $"{fileName} has no public MossTankPanel property.");
            }
        }
    }

    [Fact]
    public void NoButtonAnywhereUsesTheUnrenderableArrowGlyphs()
    {
        foreach (string path in Directory.GetFiles(AppContext.BaseDirectory, "mosstank*.xml"))
        {
            string fileName = Path.GetFileName(path);
            XDocument document = XDocument.Load(path);
            XElement root = Assert.IsType<XElement>(document.Root);
            foreach (XElement button in root.Descendants("button"))
            {
                string? text = (string?)button.Attribute("text");
                Assert.False(
                    text is "↑" or "↓",
                    $"<button> in {fileName} still uses the unrenderable "
                    + $"'{text}' glyph instead of a DAT icon.");
            }
        }
    }

    [Fact]
    public void TextlessAndAbbreviatedControlsHaveAccessibleAuthenticTooltips()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        string[] interactive = InteractiveElementNames;
        HashSet<string> terse = new(
        [
            "+", "-", "↑", "↓", "F", "B", "G", "I", "Y", "V", "A",
            "R", "S", "W", "FC", "Cp", "DC", "Cs",
        ], StringComparer.Ordinal);

        foreach (XElement element in root.Descendants()
                     .Where(element => interactive.Contains(
                         element.Name.LocalName,
                         StringComparer.Ordinal)))
        {
            if (element.Name.LocalName == "column")
                continue;
            string? text = (string?)element.Attribute("text");
            if (!string.IsNullOrWhiteSpace(text) && !terse.Contains(text))
                continue;
            Assert.False(
                string.IsNullOrWhiteSpace((string?)element.Attribute("tooltip")),
                $"<{element.Name}> text='{text}' needs a tooltip.");
        }
    }

    /// <summary>
    /// A text field draws its own background and nothing else: no border, no
    /// focus outline. Against a dark page that leaves nothing to see and
    /// nowhere obvious to type, which is what the Monsters page and the
    /// advanced options looked like. Each field therefore sits one pixel
    /// inside a bordered well that draws the background for it.
    /// Mutation: give a field its own opaque background again, or drop the
    /// well's border, and the field stops reading as a field.
    /// </summary>
    [Fact]
    public void EveryEditableFieldSitsInABorderedWellThatMakesItLookLikeAField()
    {
        foreach (string path in Directory.GetFiles(
            AppContext.BaseDirectory, "mosstank*.xml"))
        {
            string fileName = Path.GetFileName(path);
            XDocument document = XDocument.Load(path);
            XElement root = Assert.IsType<XElement>(document.Root);
            XElement[] fields = root.Descendants("field").ToArray();
            // A popup of buttons and readouts alone, such as the map's
            // control strip, has no field to check; the rule is about how
            // a field is drawn, not that every file has one.
            if (fields.Length == 0)
                continue;

            foreach (XElement field in fields)
            {
                XElement well = Assert.IsType<XElement>(field.Parent);
                Assert.Equal("group", well.Name.LocalName);
                Assert.Equal(
                    "#00000000",
                    (string?)field.Attribute("background"));
                Assert.False(
                    string.IsNullOrWhiteSpace((string?)well.Attribute("background")),
                    $"A field well in {fileName} draws no background.");
                Assert.False(
                    string.IsNullOrWhiteSpace((string?)well.Attribute("border")),
                    $"A field well in {fileName} draws no border, so its field "
                    + "has no visible edge.");

                // The well is the field's own rectangle, and the field sits a
                // pixel inside it so the border stays visible.
                Assert.Single(well.Elements());
                Assert.Equal(1f, Number(field, "x"));
                Assert.Equal(1f, Number(field, "y"));
                Assert.Equal(Number(well, "w") - 2f, Number(field, "w"));
                Assert.Equal(Number(well, "h") - 2f, Number(field, "h"));

                // A well that only moves with the window takes the anchor on
                // its own: the field inside it is positioned against the well
                // and needs none. The exception is a well that widens, where
                // the field has to widen inside it or it keeps its authored
                // width in a wider box.
                XAttribute? fieldAnchor = field.Attribute("anchor");
                if (fieldAnchor is null)
                    continue;
                string wellAnchor = (string?)well.Attribute("anchor") ?? string.Empty;
                Assert.True(
                    wellAnchor.Contains("left", StringComparison.OrdinalIgnoreCase)
                    && wellAnchor.Contains("right", StringComparison.OrdinalIgnoreCase),
                    $"A field in {fileName} carries an anchor although its well "
                    + $"does not widen (well anchor: '{wellAnchor}').");
                Assert.Equal("left top right", fieldAnchor.Value);
            }
        }
    }

    /// <summary>
    /// Every page of the main window is laid out for exactly one rectangle,
    /// the one under the tab row, and every page is laid out for the same
    /// one. That is what lets the window resize at all: one anchor on each
    /// page carries a resize to all twelve of them.
    /// Mutation: give one page a rectangle of its own and it no longer
    /// covers the window it is drawn in.
    /// </summary>
    [Fact]
    public void EveryPageFillsTheSameRectangleUnderTheTabRow()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.Equal(984f, Number(root, "w"));
        Assert.Equal(271f, Number(root, "h"));

        foreach (XElement group in root.Elements("group"))
        {
            Assert.Equal(8f, Number(group, "x"));
            Assert.Equal(42f, Number(group, "y"));
            Assert.Equal(976f, Number(group, "w"));
            Assert.Equal(229f, Number(group, "h"));
        }
        AssertWithinParent(root);
    }

    /// <summary>
    /// The window's own title is drawn inside the same rectangle as its
    /// contents, so a control placed in the top rows is drawn over -- and two
    /// popups had their first row of fields there, hiding the title that says
    /// which editor is open.
    /// Mutation: move any first row back up into the title's rows.
    /// </summary>
    [Fact]
    public void OnlyWindowChromeSitsInTheTitleRowsOfAnyWindow()
    {
        const float TitleRowBottom = 20f;
        foreach (string path in Directory.GetFiles(
            AppContext.BaseDirectory, "mosstank*.xml"))
        {
            XDocument document = XDocument.Load(path);
            XElement root = Assert.IsType<XElement>(document.Root);
            foreach (XElement child in root.Elements())
            {
                // The main window's title is itself a label, so that it can
                // carry the version the build was made as.
                if ((string?)child.Attribute("text") == "{WindowTitle}")
                    continue;
                if (Path.GetFileName(path) == "mosstank-advanced.xml"
                    && child.Name == "button"
                    && (string?)child.Attribute("onclick") == "{HideAdvancedOptions}")
                {
                    Assert.InRange(Number(child, "x"), Number(root, "w") - 24f, Number(root, "w"));
                    Assert.True(Number(child, "x") + Number(child, "w") <= Number(root, "w"));
                    Assert.Equal("right top", (string?)child.Attribute("anchor"));
                    continue;
                }
                if (child.Name == "button"
                    && ((Path.GetFileName(path) == "mosstank.xml"
                            && (string?)child.Attribute("onclick") == "{ShowRemote}")
                        || (Path.GetFileName(path) == "mosstank-remote.xml"
                            && (string?)child.Attribute("onclick") == "{ShowMainWindow}")))
                {
                    Assert.InRange(Number(child, "x"), Number(root, "w") - 50f, Number(root, "w"));
                    Assert.True(Number(child, "x") + Number(child, "w") <= Number(root, "w"));
                    Assert.Equal("right top", (string?)child.Attribute("anchor"));
                    continue;
                }
                Assert.True(
                    Number(child, "y") >= TitleRowBottom,
                    $"<{child.Name}> at y={Number(child, "y")} in "
                    + $"{Path.GetFileName(path)} is drawn under the window "
                    + "title.");
            }
        }
    }

    /// <summary>
    /// The Items page adds the item the player has selected, so each of its
    /// buttons has to say what it acts on: a page of unlabelled buttons that
    /// answer only in chat reads as a page with no Add button at all.
    /// </summary>
    [Fact]
    public void EveryItemsPageButtonSaysWhatItActsOn()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement group = Assert.Single(
            root.Elements("group"),
            static candidate =>
                (string?)candidate.Attribute("visible") == "{ItemsVisible}");

        foreach (string action in new[]
        {
            "{AddSelectedItem}", "{AddSelectedItemNoBuffs}", "{RemoveSelectedItem}",
        })
        {
            XElement button = Assert.Single(
                group.Elements("button"),
                candidate => (string?)candidate.Attribute("onclick") == action);
            Assert.False(
                string.IsNullOrWhiteSpace((string?)button.Attribute("tooltip")),
                $"The Items page button {action} says nothing about what it "
                + "acts on.");
        }

    }

    /// <summary>
    /// A follow route can only be aimed from the window, so the navigation
    /// page needs the control that aims it and the readout that says who is
    /// followed. Without them the mode menu offers Follow with no way to name
    /// a target.
    /// </summary>
    [Fact]
    public void TheNavigationPageCanAimAFollowRouteAndSaysWhoIsFollowed()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement group = Assert.Single(
            root.Elements("group"),
            static candidate =>
                (string?)candidate.Attribute("visible") == "{RouteVisible}");

        XElement follow = Assert.Single(
            group.Elements("button"),
            static button =>
                (string?)button.Attribute("onclick") == "{SetFollowTarget}");
        Assert.Equal("Follow", (string?)follow.Attribute("text"));

        Assert.Single(
            group.Elements("label"),
            static label =>
                (string?)label.Attribute("text") == "{RouteFollowTargetText}");
        Assert.Contains(
            "Follow",
            Assert.IsType<string>((string?)Assert.Single(
                group.Elements("menu"),
                static menu =>
                    (string?)menu.Attribute("items") == "{RouteModeNames}")
                .Attribute("tooltip")),
            StringComparison.Ordinal);
    }

    /// <summary>
    /// Glyph advances of the captions used by the monster table's headings,
    /// in the client's default interface font. Measured offline by building
    /// this markup with the client's own markup engine and the font the client
    /// loads for it; the engine draws a heading's text from the heading's left
    /// edge, with no way to centre it, so the authored x is the only place the
    /// centring can live.
    /// </summary>
    private static readonly Dictionary<string, float> CaptionWidths = new(
        StringComparer.Ordinal)
    {
        ["F"] = 6f, ["B"] = 7f, ["G"] = 9f, ["I"] = 3f, ["Y"] = 7f,
        ["V"] = 9f, ["A"] = 9f, ["R"] = 7f, ["S"] = 7f,
        ["WC"] = 19f, ["FC"] = 14f, ["Cp"] = 14f, ["DC"] = 17f, ["Cs"] = 13f,
    };

    /// <summary>
    /// The lamp a check cell draws is centred in its column, so its heading
    /// has to be centred over the same point or the table reads one column
    /// out. The cell centres the lamp inside a one-pixel inset, which is the
    /// arithmetic repeated here.
    /// Mutation: shift any heading back to its column's left edge, or change
    /// a check column's width, and the heading no longer sits over its lamp.
    /// </summary>
    [Fact]
    public void EveryCheckColumnHeadingIsCentredOverTheLampItsColumnDraws()
    {
        const float LampSize = 11f;
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement group = Assert.Single(
            root.Elements("group"),
            static candidate =>
                (string?)candidate.Attribute("visible") == "{MonstersVisible}");
        XElement table = Assert.Single(group.Elements("list"));
        XElement[] checkColumns = table.Elements("column")
            .Where(static column => (string?)column.Attribute("type") == "check")
            .ToArray();
        string[] captions = group.Elements("label")
            .Select(static label => (string?)label.Attribute("text") ?? string.Empty)
            .Where(CaptionWidths.ContainsKey)
            .ToArray();

        Assert.Equal(checkColumns.Length, captions.Length);

        float columnLeft = 0f;
        for (int index = 0; index < checkColumns.Length; index++)
        {
            float columnWidth = Number(checkColumns[index], "width");
            Assert.True(columnWidth > 0f, "A check column needs a fixed width.");
            float lampCentre = columnLeft + 1f
                + MathF.Max(0f, columnWidth - 2f - LampSize) * 0.5f
                + LampSize * 0.5f;

            string caption = captions[index];
            XElement heading = Assert.Single(
                group.Elements("label"),
                label => (string?)label.Attribute("text") == caption);
            float headingCentre =
                Number(heading, "x") + CaptionWidths[caption] * 0.5f;

            Assert.True(
                MathF.Abs(headingCentre - lampCentre) <= 1f,
                $"Heading '{caption}' is centred at {headingCentre} but its "
                + $"column's lamp at {lampCentre}.");
            columnLeft += columnWidth;
        }
    }

    /// <summary>
    /// A text column draws its cell text one padding step in from the
    /// column's left edge, so its heading starts there too.
    /// Mutation: put a heading back on the column edge and it sits left of
    /// the values underneath it.
    /// </summary>
    [Fact]
    public void EveryTextColumnHeadingStartsWhereItsCellTextDoes()
    {
        const float CellPadding = 3f;
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement group = Assert.Single(
            root.Elements("group"),
            static candidate =>
                (string?)candidate.Attribute("visible") == "{MonstersVisible}");
        XElement table = Assert.Single(group.Elements("list"));

        // Heading caption -> the column it belongs to, counted in document
        // order across the whole column list.
        (string Caption, int Column)[] headings =
        [
            ("Name", 14), ("P", 15), ("Dmg type", 16), ("Ex. Vuln", 17),
            ("Weapon", 18), ("Offhand", 19), ("PetDmg", 20),
        ];
        XElement[] columns = table.Elements("column").ToArray();

        foreach ((string caption, int columnIndex) in headings)
        {
            float columnLeft = columns.Take(columnIndex)
                .Sum(column => Number(column, "width"));
            XElement heading = Assert.Single(
                group.Elements("label"),
                label => (string?)label.Attribute("text") == caption);

            Assert.Equal(columnLeft + CellPadding, Number(heading, "x"));
        }
    }

    private static void AssertBindingType(
        XElement element,
        string attributeName,
        Type expectedType,
        IReadOnlyDictionary<string, PropertyInfo> properties)
    {
        string? expression = (string?)element.Attribute(attributeName);
        if (expression is null)
            return;
        Assert.StartsWith("{", expression, StringComparison.Ordinal);
        Assert.EndsWith("}", expression, StringComparison.Ordinal);
        string name = expression[1..^1];
        Assert.True(
            properties.TryGetValue(name, out PropertyInfo? property),
            $"Markup binding {expression} on <{element.Name}> has no public "
            + "MossTankPanel property.");
        Assert.Equal(expectedType, property.PropertyType);
    }

    private static void AssertWithinParent(XElement parent)
    {
        float parentWidth = Number(parent, "w");
        float parentHeight = Number(parent, "h");
        foreach (XElement child in parent.Elements())
        {
            float width = Number(child, "w");
            float height = EffectiveHeight(child);
            if (width > 0f)
            {
                Assert.True(
                    Number(child, "x") + width <= parentWidth,
                    $"<{child.Name}> crosses the right edge of <{parent.Name}>.");
            }
            if (height > 0f)
            {
                Assert.True(
                    Number(child, "y") + height <= parentHeight,
                    $"<{child.Name}> crosses the bottom edge of <{parent.Name}>.");
            }
            AssertWithinParent(child);
        }
    }

    private static float EffectiveHeight(XElement element)
    {
        float declared = Number(element, "h");
        if (declared > 0f)
            return declared;

        return element.Name.LocalName switch
        {
            "label" => 16f,
            "field" => 16f,
            "toggle" => 20f,
            "button" => 16f,
            _ => 0f,
        };
    }

    /// <summary>
    /// A list's column headings have to travel with the list's columns. The
    /// columns start at the list's left edge, so a heading sitting over them
    /// must be pinned to the same edge the list is: a heading anchored to the
    /// right of a window that is anchored to the left slides away from its
    /// column the moment the window is widened, and the page reads as if the
    /// values had moved under the wrong heading.
    /// A heading is a label in the same group whose bottom sits within a row's
    /// height above the list and whose left edge lies inside the list.
    /// Mutation: anchor one of the imbue headings "right top" again and it is
    /// named here.
    /// </summary>
    [Fact]
    public void EveryListHeadingIsPinnedToTheSameHorizontalEdgeAsItsList()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        foreach (XElement list in root.Descendants("list"))
        {
            XElement group = Assert.IsType<XElement>(list.Parent);
            (bool listLeft, bool listRight) = HorizontalAnchor(list);
            float x = Number(list, "x");
            float right = x + Number(list, "w");
            float top = Number(list, "y");
            foreach (XElement label in group.Elements("label"))
            {
                float bottom = Number(label, "y") + Number(label, "h");
                float labelX = Number(label, "x");
                if (bottom > top || bottom <= top - 24f)
                    continue;
                if (labelX < x || labelX >= right)
                    continue;

                (bool labelLeft, bool labelRight) = HorizontalAnchor(label);
                string where =
                    $"Heading '{(string?)label.Attribute("text")}' at x={labelX} over the "
                    + $"list at x={x} (list anchor '{(string?)list.Attribute("anchor")}', "
                    + $"heading anchor '{(string?)label.Attribute("anchor")}')";
                Assert.True(
                    labelLeft == listLeft,
                    $"{where} is not pinned to the same left edge as its list.");
                Assert.True(
                    !labelRight || listRight,
                    $"{where} is pinned to a right edge its list is not.");
            }
        }
    }

    /// <summary>
    /// Which horizontal edges an element is pinned to. Nothing said means the
    /// left edge, which is what the layout does with an element that names no
    /// anchor.
    /// </summary>
    private static (bool Left, bool Right) HorizontalAnchor(XElement element)
    {
        string anchor = (string?)element.Attribute("anchor") ?? string.Empty;
        string[] parts = anchor.Split(
            ' ',
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        bool left = parts.Contains("left", StringComparer.OrdinalIgnoreCase);
        bool right = parts.Contains("right", StringComparer.OrdinalIgnoreCase);
        return (!left && !right ? (true, false) : (left, right));
    }

    private static float Number(XElement element, string attribute) =>
        float.TryParse(
            (string?)element.Attribute(attribute),
            System.Globalization.NumberStyles.Float,
            System.Globalization.CultureInfo.InvariantCulture,
            out float value)
            ? value
            : 0f;

    private sealed class StubHost : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new StubLogger();
        public IGameState State { get; } = new StubState();
        public IEvents Events { get; } = new StubEvents();
        public ISelectionService Selection { get; } = new StubSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation => NoOpAutomationSurface.Instance;
    }

    private sealed class StubLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class StubState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class StubEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }
        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }

    private sealed class StubSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }

    /// <summary>
    /// No secondary window is authored over the main window: each one's
    /// own rectangle lies clear of the main window's, the way the editors
    /// and pickers sit below it, so a window opened from a page does not
    /// cover the page it was opened from. Mutation: author one back over
    /// the main window and it is named.
    /// </summary>
    [Fact]
    public void NoSecondaryWindowIsAuthoredOverTheMainWindow()
    {
        XDocument main = XDocument.Load(Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement mainRoot = Assert.IsType<XElement>(main.Root);

        foreach (string path in Directory.GetFiles(AppContext.BaseDirectory, "mosstank-*.xml"))
        {
            XDocument document = XDocument.Load(path);
            XElement root = Assert.IsType<XElement>(document.Root);
            Assert.False(
                RectanglesOverlap(mainRoot, root),
                $"{Path.GetFileName(path)} at {Number(root, "x")},{Number(root, "y")} "
                + $"({Number(root, "w")}x{Number(root, "h")}) is authored over the main window at "
                + $"{Number(mainRoot, "x")},{Number(mainRoot, "y")} "
                + $"({Number(mainRoot, "w")}x{Number(mainRoot, "h")}).");
        }
    }
}
