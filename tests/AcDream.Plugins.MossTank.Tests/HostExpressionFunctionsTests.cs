using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class HostExpressionFunctionsTests
{
    [Fact]
    public void CharacterAndNavigationFunctionsReadCanonicalAutomationFacts()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Coldeve", runtime.Evaluate("getworldname[]").AsString());
        Assert.Equal(2d, runtime.Evaluate("getcharacterindex[``]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getcharvital_current[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharvital_buffedmax[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharattribute_base[1]").AsNumber());
        Assert.Equal(110d, runtime.Evaluate("getcharattribute_buffed[1]").AsNumber());
        Assert.Equal(275d, runtime.Evaluate("getcharskill_base[34]").AsNumber());
        Assert.Equal(300d, runtime.Evaluate("getcharskill_buffed[34]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("getcharskill_traininglevel[34]").AsNumber());
        Assert.Equal(0x7F7F0000u, runtime.Evaluate("getplayerlandblock[]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getheading[wobjectgetplayer[]]").AsNumber());
    }

    /// <summary>
    /// The landblock is the cell id with the low sixteen bits cleared, not
    /// shifted down: profiles compare it with numbers such as 11468800
    /// (0x00AF0000) and with `cstrf[...,X8]` strings such as `B3450000`.
    /// Mutation: returning the cell id shifted right by 16 gives 0x7F7F and
    /// fails the first three assertions.
    /// </summary>
    [Fact]
    public void PlayerLandblockKeepsTheHighWordInPlace()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2139029504d, runtime.Evaluate("getplayerlandblock[]").AsNumber());
        Assert.Equal("7F7F0000", runtime.Evaluate(
            "cstrf[getplayerlandblock[],`X8`]").AsString());
        Assert.True(runtime.Evaluate("getplayerlandblock[]==2139029504").IsTruthy);
        Assert.Equal(0x7F7F0001u, runtime.Evaluate("getplayerlandcell[]").AsNumber());
    }

    /// <summary>
    /// A hex format renders a signed object id as its eight-digit unsigned
    /// hex, as the reference does: the id 0x800008B5, which reads as
    /// -2147481419, formats as `800008B5`. Mutation: a checked unsigned
    /// conversion throws an overflow on the negative id.
    /// </summary>
    [Fact]
    public void HexFormatWrapsASignedObjectId()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x800008B5u, 105, "Tome", PluginObjectClass.Book, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("800008B5", runtime.Evaluate(
            "cstrf[wobjectgetid[wobjectfindbyid[-2147481419]],`X8`]").AsString());
        Assert.Equal("FFFFFFFF", runtime.Evaluate("cstrf[-1,`X8`]").AsString());
        Assert.Equal("FFFFFFFE", runtime.Evaluate("cstrf[-2.7,`X`]").AsString());
    }

    /// <summary>
    /// Object ids travel through expressions as signed 32-bit numbers, the
    /// way profiles store them: an item id 0x800008B5 reads as -2147481419,
    /// and the finder takes it back in that form (rounded to the nearest
    /// whole number). A number outside the signed range is refused.
    /// Mutation: the unsigned conversion throws on the negative id, and an
    /// unsigned read gives 2147485877.
    /// </summary>
    [Fact]
    public void ObjectIdsAreSignedInExpressions()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x800008B5u, 105, "Tome", PluginObjectClass.Book, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[-2147481419]]").AsString());
        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[-2147481419.4]]").AsString());
        Assert.Equal(-2147481419d, runtime.Evaluate(
            "wobjectgetid[wobjectfindbyid[-2147481419]]").AsNumber());
        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[wobjectgetid[wobjectfindbyid[-2147481419]]]]")
                .AsString());
        // A bare number stands for the object wherever an object is expected.
        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[-2147481419]").AsString());
        Assert.Equal(0d, runtime.Evaluate("wobjectfindbyid[-5]").AsNumber());
        Assert.ThrowsAny<Exception>(() => runtime.Evaluate("wobjectfindbyid[4294967296]"));
        Assert.ThrowsAny<Exception>(() => runtime.Evaluate("wobjectfindbyid[-2147483649]"));
    }

    /// <summary>
    /// UtilityBelt converts a number handed to an object parameter to an id
    /// before the call, and a NaN or a number past 32 bits fails that
    /// conversion with the runtime's own error, which ends the run. Here it
    /// is the engine's evaluation error, so a meta condition reading it is
    /// false. Mutation: letting the argument check's raw overflow out fails
    /// the type check.
    /// </summary>
    [Theory]
    [InlineData("wobjectgetname[0/0]")]
    [InlineData("wobjectgetname[99999999999*99999999999]")]
    public void AnObjectArgumentThatIsNoIdIsAnEvaluationErrorAsInUtilityBelt(string source)
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        var error = Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate(source));
        Assert.IsType<OverflowException>(error.InnerException);
    }

    /// <summary>
    /// UtilityBelt's date functions catch a format they cannot write, print
    /// its message to chat as a plain UtilityBelt line and answer the empty
    /// string, so the expression goes on. Mutation: letting the format error
    /// out fails the expression.
    /// </summary>
    [Theory]
    [InlineData("getdatetimelocal[`%`]")]
    [InlineData("getdatetimeutc[`%`]")]
    public void ADateFormatThatCannotBeWrittenAnswersEmptyAsInUtilityBelt(string source)
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("after", runtime.Evaluate(source + "+`after`").AsString());
        (string text, int chatType) = Assert.Single(automation.Posted);
        Assert.StartsWith("[UB] ", text, StringComparison.Ordinal);
        Assert.True(text.Length > "[UB] ".Length);
        Assert.Equal(UbChat.GenericChatType, chatType);
    }

    /// <summary>
    /// Earlier MossTank versions handed ids out unsigned (0x800008B5 as
    /// 2147485877) and saved them that way in persistent and global
    /// variables. Those saved numbers still name the object after the
    /// upgrade, both in the finder and wherever a number stands for an
    /// object. Mutation: reading numbers only in the signed range throws
    /// on the saved value.
    /// </summary>
    [Fact]
    public void UnsignedIdsSavedByEarlierVersionsStillNameTheObject()
    {
        var storage = new MemoryStorage();
        using (var earlier = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
            earlier.Evaluate("setpvar[`pack`,2147485877];setgvar[`tome`,2147485877]");
        }

        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x800008B5u, 105, "Tome", PluginObjectClass.Book, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation, storage));

        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[getpvar[`pack`]]]").AsString());
        Assert.Equal("Tome", runtime.Evaluate(
            "wobjectgetname[getgvar[`tome`]]").AsString());
        Assert.True(runtime.Evaluate("actiontryuseitem[getpvar[`pack`]]").IsTruthy);
        Assert.Equal(0x800008B5u, automation.UsedObject);
        // The object found from the old number gives the current form back.
        Assert.Equal(-2147481419d, runtime.Evaluate(
            "wobjectgetid[wobjectfindbyid[getpvar[`pack`]]]").AsNumber());
    }

    /// <summary>
    /// A peer's cast names its caster and target the way <c>wobjectgetid</c>
    /// does, so a meta can compare a shared target with an object it sees.
    /// Mutation: handing the target out unsigned makes the comparison false
    /// for any id at or above 0x80000000.
    /// </summary>
    [Fact]
    public void NetworkCastIdsCompareEqualToObjectIds()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x80001234u, 105, "Drudge", PluginObjectClass.Monster, 0x2, 1, 0));
        automation.NetworkCasts =
        [
            new PluginPeerCast(3, 7u, 0x500000AAu, 0x80001234u, 2074u, 310, 42.5d, true),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate(
            "dictgetitem[listgetitem[netcasts[],0],`TargetId`]"
            + "==wobjectgetid[wobjectfindbyid[-2147478988]]").IsTruthy);
        Assert.Equal(-2147478988d, runtime.Evaluate(
            "dictgetitem[listgetitem[netcasts[],0],`TargetId`]").AsNumber());
        Assert.Equal((double)0x500000AAu, runtime.Evaluate(
            "dictgetitem[listgetitem[netcasts[],0],`CasterId`]").AsNumber());
    }

    /// <summary>
    /// An object's property reads answer from what the client learned when
    /// the object appeared, before any appraisal: its name (string 1), its
    /// class id, container, wielder, capacities and landcell under their
    /// object-description keys. A missing string is the empty string. An
    /// appraised value wins where there is one. Mutation: answering only
    /// from the appraisal tables reads the jaw's name as 0, so the name
    /// comparison a looting profile makes is false.
    /// </summary>
    [Fact]
    public void ObjectPropertiesAnswerFromTheObjectBeforeAppraisal()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            30, 7001, "Insatiable Eater Jaw", PluginObjectClass.Misc, 0x80, 40, 0)
        {
            ItemsCapacity = 0,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            31, 7002, "Create Name", PluginObjectClass.Container, 0x200, 0, 0)
        {
            ItemsCapacity = 24,
            ContainersCapacity = 1,
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { CellId = 0xA9B40021u },
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            32, 7003, "Wand", PluginObjectClass.WandStaffOrb, 0x8000, 0, 50001));
        automation.Properties[31] = Properties(
            strings: new Dictionary<uint, string> { [1] = "Appraised Name" });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate(
            "wobjectgetstringprop[wobjectfindbyid[30],1]==`Insatiable Eater Jaw`").IsTruthy);
        Assert.Equal("", runtime.Evaluate("wobjectgetstringprop[wobjectfindbyid[30],16]").AsString());
        Assert.Equal(7001d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[30],218103808]").AsNumber());
        Assert.Equal(40d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[30],218103810]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[30],218103818]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[30],218103811]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[30],19]").AsNumber());

        Assert.Equal("Appraised Name", runtime.Evaluate(
            "wobjectgetstringprop[wobjectfindbyid[31],1]").AsString());
        Assert.Equal(24d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[31],218103812]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[31],218103813]").AsNumber());
        Assert.Equal((double)unchecked((int)0xA9B40021u), runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[31],218103811]").AsNumber());

        // A wielded object's container is the one wielding it.
        Assert.Equal(50001d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[32],218103810]").AsNumber());
        Assert.Equal(50001d, runtime.Evaluate("wobjectgetintprop[wobjectfindbyid[32],218103818]").AsNumber());
    }

    /// <summary>
    /// Integer property reads are signed 32-bit numbers, the way the
    /// reference answers them, so an id read off an object (its container,
    /// its wielder) compares equal to <c>wobjectgetid</c> of that object,
    /// and a landcell at or above 0x80000000 is negative. Mutation: handing
    /// the description values out unsigned makes "is this item in that
    /// pack?" false for every pack.
    /// </summary>
    [Fact]
    public void ObjectDescriptionIdsAreSignedLikeObjectIds()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x80000100u, 7004, "Pack", PluginObjectClass.Container, 0x200, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            0x80000101u, 7005, "Gem", PluginObjectClass.Gem, 0x800, 0x80000100u, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            0x80000102u, 7006, "Bow", PluginObjectClass.MissileWeapon, 0x100, 0, 0x80000200u));
        automation.WorldObjects.Add(new PluginWorldObject(
            0x80000200u, 7007, "Guard", PluginObjectClass.Npc, 0x1, 0, 0));
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[-2147483391],218103810]"
            + "==wobjectgetid[wobjectfindbyid[-2147483392]]").IsTruthy);
        Assert.Equal(-2147483392d, runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[-2147483391],218103810]").AsNumber());
        Assert.True(runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[-2147483390],218103818]"
            + "==wobjectgetid[wobjectfindbyid[-2147483136]]").IsTruthy);
        Assert.True(runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[-2147483390],218103810]"
            + "==wobjectgetid[wobjectfindbyid[-2147483136]]").IsTruthy);
    }

    /// <summary>
    /// The three vital reads are three DIFFERENT numbers — unbuffed maximum,
    /// current, buffed maximum — and each is floored at 1. Mutation: pointing
    /// base and buffed maximum at the same field makes the first two
    /// assertions equal, and dropping the floor makes the last one 0.
    /// </summary>
    [Fact]
    public void VitalReadsSeparateBaseCurrentAndBuffedMaximumAndFloorAtOne()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(80d, runtime.Evaluate("getcharvital_base[1]").AsNumber());
        Assert.Equal(90d, runtime.Evaluate("getcharvital_current[1]").AsNumber());
        Assert.Equal(100d, runtime.Evaluate("getcharvital_buffedmax[1]").AsNumber());
        Assert.Equal(70d, runtime.Evaluate("getcharvital_base[2]").AsNumber());
        Assert.Equal(60d, runtime.Evaluate("getcharvital_base[3]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_current[9]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_base[9]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharvital_buffedmax[9]").AsNumber());
    }

    /// <summary>
    /// echo takes the reference's second argument, the chat colour (a chat
    /// text type), and prints the text in it; the one-argument form is the
    /// reference's argument-count error. Mutation: refusing the colour fails
    /// the two-argument call.
    /// </summary>
    [Fact]
    public void EchoPrintsInTheColourItIsGiven()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate("echo[\\[RynCMD\\] Clearing All Vars,13]").IsTruthy);
        // Without the colour it is the reference's argument-count error, and
        // nothing is printed.
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("echo[plain]"));

        Assert.Equal(("[RynCMD] Clearing All Vars", 13), Assert.Single(automation.Posted));
    }

    [Fact]
    public void ObjectDiscoveryCountsPropertiesAndNearestMatchUtilityBeltShape()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2d, runtime.Evaluate(
            "listcount[wobjectfindallinventory[]]").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "getitemcountininventorybyname[`Health Elixir`]").AsNumber());
        Assert.Equal(9d, runtime.Evaluate("getfreeitemslots[]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getfreecontainerslots[]").AsNumber());
        Assert.Equal("Drudge", runtime.Evaluate(
            "wobjectgetname[wobjectfindnearestmonster[]]").AsString());
        Assert.Equal(77d, runtime.Evaluate(
            "wobjectgetintprop[wobjectfindbyid[20],25]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx[`(?i)elixir|drudge`]]").AsNumber());
        Assert.Equal((double)PluginObjectClass.Monster, runtime.Evaluate(
            "wobjectgetobjectclass[wobjectfindbyid[20]]").AsNumber());
        Assert.Equal(54321d, runtime.Evaluate(
            "wobjectlastidtime[wobjectfindbyid[20]]").AsNumber());
    }

    /// <summary>
    /// Inventory lookups by name are exact and case-SENSITIVE, and the regex
    /// variant compiles its pattern without IgnoreCase. Mutation: comparing
    /// with OrdinalIgnoreCase, or adding RegexOptions.IgnoreCase, makes the
    /// lower-case probes find the item.
    /// </summary>
    [Fact]
    public void InventoryNameLookupsAreCaseSensitive()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(10d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname[`Health Elixir`]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybyname[`health elixir`]").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx[`^Health`]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybynamerx[`^health`]").AsNumber());
    }

    /// <summary>
    /// The inventory finders match the display name — the material in front
    /// of the bare name — the exact-name finder included, as the reference
    /// does. A material id that only names a group adds no prefix.
    /// Mutation: matching the bare name in the regex finder makes the
    /// `^Silver ` probe answer 0; matching it in the exact finder makes the
    /// `Silver Long Sword` probe answer 0 and the `Long Sword` probe 12;
    /// giving the grouping id 9 a name of its own turns "Shard" into
    /// "Gem Shard".
    /// </summary>
    [Fact]
    public void NamePatternsMatchTheMaterialPrefixedDisplayName()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            12, 102, "Long Sword", PluginObjectClass.MeleeWeapon, 0x1, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            13, 103, "Shard", PluginObjectClass.Gem, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        automation.Properties[12] = Properties(
            ints: new Dictionary<uint, int> { [131] = 63 });
        automation.Properties[13] = Properties(
            ints: new Dictionary<uint, int> { [131] = 9 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Silver Long Sword", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[12]]").AsString());
        Assert.Equal(12d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx[`^Silver `]]").AsNumber());
        Assert.Equal(12d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname[`Silver Long Sword`]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindininventorybyname[`Long Sword`]").AsNumber());
        Assert.Equal("Shard", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[13]]").AsString());
        Assert.Equal(13d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybynamerx[`^Shard$`]]").AsNumber());
    }

    /// <summary>
    /// A name that already carries its material's name gets no second
    /// prefix, as in the reference: a jet named "Jet" stays "Jet" and a
    /// "Silver Ring" of silver stays "Silver Ring", and the finders match
    /// that name. Mutation: always prefixing gives "Jet Jet" and
    /// "Silver Silver Ring" and both exact probes answer 0.
    /// </summary>
    [Fact]
    public void DisplayNameSkipsAMaterialTheNameAlreadyCarries()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            14, 106, "Jet", PluginObjectClass.Gem, 0x2, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            15, 107, "Silver Ring", PluginObjectClass.Jewelry, 0x8, 1, 0)
        {
            IsOwned = true,
        });
        // 27 is Jet, 63 is Silver.
        automation.Properties[14] = Properties(
            ints: new Dictionary<uint, int> { [131] = 27 });
        automation.Properties[15] = Properties(
            ints: new Dictionary<uint, int> { [131] = 63 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Jet", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[14]]").AsString());
        Assert.Equal("Silver Ring", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[15]]").AsString());
        Assert.Equal(14d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname[`Jet`]]").AsNumber());
        Assert.Equal(15d, runtime.Evaluate(
            "wobjectgetid[wobjectfindininventorybyname[`Silver Ring`]]").AsNumber());
    }

    /// <summary>
    /// The inventory counts by name read the display name too, without
    /// regard to case, as the reference does: a stack of silver long swords
    /// counts for `silver long sword` and `^silver `, not for `Long Sword`.
    /// Mutation: counting on the bare name answers 0 for the first two probes
    /// and 3 for the third.
    /// </summary>
    [Fact]
    public void InventoryCountsByNameReadTheDisplayNameInAnyCase()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            12, 102, "Long Sword", PluginObjectClass.MeleeWeapon, 0x1, 1, 0)
        {
            IsOwned = true,
            StackSize = 3,
        });
        automation.Properties[12] = Properties(
            ints: new Dictionary<uint, int> { [131] = 63 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(3d, runtime.Evaluate(
            "getitemcountininventorybyname[`silver long sword`]").AsNumber());
        Assert.Equal(3d, runtime.Evaluate(
            "getitemcountininventorybynamerx[`^silver `]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "getitemcountininventorybyname[`Long Sword`]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "getitemcountininventorybynamerx[`^Long`]").AsNumber());
    }

    /// <summary>
    /// The nearest-by-name-and-class lookup is the one pattern finder that
    /// reads the BARE name, as the reference does: an oak chest answers to
    /// `^Chest$` and not to `^Oak `. Mutation: matching the material-prefixed
    /// display name answers 0 for `^Chest$` and the chest for `^Oak `.
    /// </summary>
    [Fact]
    public void NearestByNameAndObjectClassMatchesTheBareName()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            22, 104, "Chest", PluginObjectClass.Container, 0x200, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 10.2d },
        });
        // 75 is Oak.
        automation.Properties[22] = Properties(
            ints: new Dictionary<uint, int> { [131] = 75 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Oak Chest", runtime.Evaluate(
            "wobjectgetname[wobjectfindbyid[22]]").AsString());
        Assert.Equal(22d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[10,`^Chest$`]]")
            .AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbynameandobjectclass[10,`^Oak `]").AsNumber());
    }

    /// <summary>
    /// The three list-every-match pattern finders read the display name and
    /// match case-sensitively, as the reference does. Mutation: matching the
    /// bare name in the shared registration makes all three `^Silver ` and
    /// `^Oak ` probes answer an empty list, while the two-object `Long Sword`
    /// probe still answers 2; ignoring case makes the lower-case probes
    /// answer 2 and 1.
    /// </summary>
    [Fact]
    public void ListPatternFindersMatchTheDisplayNameCaseSensitively()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            12, 102, "Long Sword", PluginObjectClass.MeleeWeapon, 0x1, 1, 0)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            22, 104, "Long Sword", PluginObjectClass.MeleeWeapon, 0x1, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 10.2d },
        });
        // 63 is Silver, 75 is Oak.
        automation.Properties[12] = Properties(
            ints: new Dictionary<uint, int> { [131] = 63 });
        automation.Properties[22] = Properties(
            ints: new Dictionary<uint, int> { [131] = 75 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx[`Long Sword`]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx[`^Silver `]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx[`^Oak `]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "listcount[wobjectfindallinventorybynamerx[`^Silver `]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "listcount[wobjectfindallinventorybynamerx[`^Oak `]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "listcount[wobjectfindalllandscapebynamerx[`^Oak `]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "listcount[wobjectfindalllandscapebynamerx[`^Silver `]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "listcount[wobjectfindallbynamerx[`long sword`]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "listcount[wobjectfindallinventorybynamerx[`^silver `]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "listcount[wobjectfindalllandscapebynamerx[`^oak `]]").AsNumber());
    }

    /// <summary>
    /// The nearest-by-name-and-class lookup takes the object class FIRST and a
    /// case-sensitive REGEX second. Mutation: the previous argument order plus
    /// literal case-insensitive equality fails every assertion here — the
    /// regex probe outright throws, because argument 0 is a number.
    /// </summary>
    [Fact]
    public void NearestByNameAndObjectClassTakesClassThenCaseSensitiveRegex()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,`^Dru`]]").AsNumber());
        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbynameandobjectclass[5,`Drudge`]]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbynameandobjectclass[5,`^dru`]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbynameandobjectclass[6,`^Dru`]").AsNumber());
    }

    /// <summary>
    /// A position read from the client keeps its height reading: the host
    /// already gives the height in the same 240-metre units as the map
    /// coordinates, so 97.5 m up reads as 0.41Z, `coordinategetz` answers
    /// 97.5/240, and a 3.5 m drop counts as 3.5 m in a 3-D distance.
    /// Mutation: dividing the host's height by 240 once more reads 0.00Z,
    /// answers 97.5/57600 and makes the 3-D distance the flat one.
    /// </summary>
    [Fact]
    public void PositionsKeepTheHostsHeightReading()
    {
        var automation = CreateAutomation();
        automation.Position = automation.Position with { Elevation = 97.5d / 240d };
        automation.WorldObjects.Add(new PluginWorldObject(
            40, 400, "Lower Chest", PluginObjectClass.Container, 0x200, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { Elevation = 94d / 240d },
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(97.5d / 240d, runtime.Evaluate(
            "coordinategetz[getplayercoordinates[]]").AsNumber(), 12);
        Assert.Equal("20.00N, 10.00E, 0.41Z", runtime.Evaluate(
            "coordinatetostring[getplayercoordinates[]]").AsString());
        Assert.Equal(3.5d, runtime.Evaluate(
            "coordinatedistancewithz[getplayercoordinates[],"
            + "wobjectgetphysicscoordinates[wobjectfindbyid[40]]]").AsNumber(), 9);
    }

    /// <summary>
    /// The nearest lookup ranks by the true 3-D distance in metres, the
    /// height difference included at full weight: a chest 1.5 m away on the
    /// same floor is nearer than one 0.8 m across but 3.5 m below. Mutation:
    /// taking the height difference in 240-metre units (a 3.5 m drop counts
    /// as 0.015 m) picks the chest below.
    /// </summary>
    [Fact]
    public void NearestLookupCountsHeightInMetres()
    {
        var automation = CreateAutomation();
        automation.Position = automation.Position with { Elevation = 97.5d / 240d };
        automation.WorldObjects.Add(new PluginWorldObject(
            40, 400, "Chest Below", PluginObjectClass.Container, 0x200, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with
            {
                EastWest = automation.Position.EastWest + (0.8d / 240d),
                Elevation = 94d / 240d,
            },
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            41, 401, "Chest Beside", PluginObjectClass.Container, 0x200, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with
            {
                EastWest = automation.Position.EastWest + (1.5d / 240d),
            },
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(41d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbyobjectclass[10]]").AsNumber());
    }

    /// <summary>
    /// The nearest lookups skip the player's own object and rank by the 3-D
    /// distance. Mutation: dropping the self-exclusion returns the player for
    /// the player class, and ranking on the flat distance picks the object
    /// that is closer on the map but a hundred metres up.
    /// </summary>
    [Fact]
    public void NearestLookupsExcludeSelfAndRankInThreeDimensions()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            21, 200, "Skywards Drudge", PluginObjectClass.Monster, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with
            {
                EastWest = 10.05d,
                Elevation = automation.Position.Elevation + 100d,
            },
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbyobjectclass[24]").AsNumber());
        Assert.Equal(20d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestmonster[]]").AsNumber());
    }

    /// <summary>
    /// The nearest lookups by object class search every object of the class
    /// the client knows, as the reference's class finders do: a wand in the
    /// character's hand is found even with nothing of its class on the
    /// ground. Mutation: restricting the class finder to the ground answers
    /// 0 here.
    /// </summary>
    [Fact]
    public void NearestByObjectClassSearchesEveryKnownObject()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            30, 300, "Wielded Wand", PluginObjectClass.WandStaffOrb, 0x1, 0, 1)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(30d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbyobjectclass[31]]").AsNumber());
    }

    /// <summary>
    /// The nearest lookup by template type searches only objects on the
    /// ground, as the reference does: an item in a pack or in the hand is
    /// never the answer, so a meta asking "is one nearby" reads 0 when the
    /// only copy is carried, and a copy on the ground is found even though a
    /// carried one exists. Mutation: searching every known object answers the
    /// packed lockpick and the wielded wand where the reference answers 0.
    /// </summary>
    [Fact]
    public void NearestByTemplateTypeSearchesOnlyTheGround()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            30, 300, "Wielded Wand", PluginObjectClass.WandStaffOrb, 0x1, 0, 1)
        {
            IsOwned = true,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            31, 301, "Packed Lockpick", PluginObjectClass.Lockpick, 0x1, 11, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbytemplatetype[301]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "wobjectfindnearestbytemplatetype[300]").AsNumber());
        Assert.False(runtime.Evaluate(
            "wobjectfindnearestbytemplatetype[301]!=0").IsTruthy);
        Assert.Equal(200d, runtime.Evaluate(
            "wobjectgettemplatetype[wobjectfindnearestbytemplatetype[200]]").AsNumber());

        automation.WorldObjects.Add(new PluginWorldObject(
            32, 301, "Lockpick", PluginObjectClass.Lockpick, 0x1, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 11d },
        });

        Assert.Equal(32d, runtime.Evaluate(
            "wobjectgetid[wobjectfindnearestbytemplatetype[301]]").AsNumber());
    }

    [Fact]
    public void ActionFunctionsUseSharedSelectionInventoryMagicAndMovementCommands()
    {
        var automation = CreateAutomation();
        var host = new Host(automation);
        using var runtime = new MossTankExpressionRuntime(host);

        runtime.Evaluate("actiontryselect[20]");
        Assert.Equal(20u, host.Selection.SelectedObjectId);
        Assert.True(runtime.Evaluate("actiontryuseitem[10]").IsTruthy);
        Assert.Equal(10u, automation.UsedObject);
        Assert.True(runtime.Evaluate("setmotion[`Forward`,1]").IsTruthy);
        Assert.True(automation.LastIntent.Forward);
        Assert.True(runtime.Evaluate("clearmotion[]").IsTruthy);
        Assert.Equal(1, automation.ClearMovementCount);
    }

    /// <summary>
    /// Selecting from an expression always reports false, whatever the
    /// selection did — a profile branching on the result relies on it.
    /// Mutation: returning the real selection result flips the branch.
    /// </summary>
    [Fact]
    public void SelectingAlwaysReportsFalseEvenWhenItSucceeded()
    {
        var automation = CreateAutomation();
        var host = new Host(automation);
        using var runtime = new MossTankExpressionRuntime(host);

        Assert.False(runtime.Evaluate("actiontryselect[20]").IsTruthy);
        Assert.Equal(20u, host.Selection.SelectedObjectId);
    }

    /// <summary>
    /// The castability questions differ: hunting and buffing each apply their
    /// own skill margin over the spell's difficulty, on top of the spellbook
    /// and component checks. Mutation: answering both from one host gate
    /// makes the two calls agree at every skill level.
    /// </summary>
    [Fact]
    public void CastabilityAppliesTheHuntingAndBuffingSkillMarginsSeparately()
    {
        var automation = CreateAutomation();
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 250);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        // The fake's War Magic is 300 buffed.
        runtime.Policy.SkillMargin = hunting => hunting ? 75 : 25;

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
        Assert.True(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);

        automation.HasComponents = false;
        Assert.False(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);
        automation.HasComponents = true;
        Assert.False(runtime.Evaluate("getcancastspell_buff[2002]").IsTruthy);
    }

    /// <summary>
    /// A spell whose school the host has not reported yet is NOT castable:
    /// the unknown skill reads 0 and fails the difficulty comparison. This is
    /// the state a fresh session is in before the skill table arrives.
    /// Mutation: skipping the comparison when the skill is unknown makes both
    /// castability questions answer true, and the cast answers 0 instead of 2.
    /// </summary>
    [Fact]
    public void CastabilityFailsClosedWhenTheSchoolSkillIsUnknown()
    {
        var automation = CreateAutomation();
        // The fake reports War Magic (34) only; Life Magic (33) is unknown.
        automation.Spells[1001] = Spell(1001, school: 33, difficulty: 1);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
        Assert.False(runtime.Evaluate("getcancastspell_buff[1001]").IsTruthy);
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());

        automation.Skills =
        [
            .. automation.Skills,
            new PluginSkillInfo(33, "Life Magic", PluginSkillTraining.Trained, 300),
        ];
        Assert.True(runtime.Evaluate("getcancastspell_hunt[1001]").IsTruthy);
    }

    /// <summary>
    /// A spell the host reports with no school at all is not castable either,
    /// even when nothing else stands in the way: there is no skill to compare
    /// it against, and no school-less spell a character can actually cast.
    /// The difficulty here is 0, so a comparison against an unknown skill of 0
    /// would PASS — only the closed door answers false. Mutation: skipping the
    /// comparison for a school of 0, or dropping the school test and comparing
    /// anyway, makes both castability questions answer true and the cast
    /// answer 0.
    /// </summary>
    [Fact]
    public void CastabilityIsClosedForASpellWithNoSchool()
    {
        var automation = CreateAutomation();
        automation.Spells[1003] = Spell(1003, school: 0, difficulty: 0);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("getcancastspell_hunt[1003]").IsTruthy);
        Assert.False(runtime.Evaluate("getcancastspell_buff[1003]").IsTruthy);
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1003]").AsNumber());
    }

    /// <summary>
    /// Casting from an expression answers 2 (impossible), 0 (not attempted
    /// yet) or 1 (begun) — never a bare boolean — and neither form casts until
    /// a wand is wielded and the character is in magic mode. Mutation: gating
    /// on the host cast gate alone returns 1 straight away with no weapon and
    /// no mode.
    /// </summary>
    [Fact]
    public void CastingReportsImpossibleNotYetAndBegunAndTakesOneStepFirst()
    {
        var automation = CreateAutomation();
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 10);
        automation.Spells[1002] = Spell(1002, school: 34, difficulty: 10) with
        {
            IsUntargeted = true,
        };
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[9999]").AsNumber());

        // Step one: the wand is not wielded yet.
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(50u, automation.EquippedObject);

        // Step two: wielded, but still in peace mode.
        automation.Equipment =
        [
            automation.Equipment[0] with { EquippedLocation = 0x01000000u },
        ];
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal(PluginCombatMode.Magic, automation.RequestedMode);

        // Ready: the cast begins.
        automation.Mode = PluginCombatMode.Magic;
        Assert.True(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal((1002u, 0u), automation.LastCast);
        Assert.Equal(1d, runtime.Evaluate(
            "actiontrycastbyidontarget[1001,20]").AsNumber());
        Assert.Equal((1001u, 20u), automation.LastCast);
    }

    /// <summary>
    /// The reference's cast rule: `actiontrycastbyid` answers 2 only when the
    /// spell is not known or the hunting skill margin is not met, then casts
    /// with no target and answers 1. It does not ask whether the spell takes
    /// a target (a self-targeted recall is the documented example) nor whether
    /// the components are in the pack. The on-target form asks only the skill
    /// margin. Mutation: requiring an untargeted spell answers 2 for the
    /// recall; checking components answers 2 once they are gone; answering
    /// from the host's send result answers 0 on the last line.
    /// </summary>
    [Fact]
    public void CastByIdCastsAnyKnownSkilledSpellWithNoTarget()
    {
        var automation = CreateAutomation();
        // A self-targeted recall: it needs no target, but it is not an
        // "untargeted" spell either.
        automation.Spells[1004] = new PluginSpellInfo(
            1004, "Recall", 1u, 1, 10, 10, 0f, 34u, string.Empty,
            IsSelfTargeted: true, IsBeneficial: true);
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 10);
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d)
            {
                EquippedLocation = 0x01000000u,
            },
        ];
        automation.Mode = PluginCombatMode.Magic;
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1004]").AsNumber());
        Assert.Equal((1004u, 0u), automation.LastCast);

        automation.HasComponents = false;
        Assert.False(runtime.Evaluate("getcancastspell_hunt[1004]").IsTruthy);
        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1004]").AsNumber());

        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());
        Assert.Equal((1001u, 0u), automation.LastCast);
        Assert.Equal(1d, runtime.Evaluate(
            "actiontrycastbyidontarget[1004,20]").AsNumber());
        Assert.Equal((1004u, 20u), automation.LastCast);

        automation.UnknownSpells.Add(1004);
        Assert.Equal(2d, runtime.Evaluate("actiontrycastbyid[1004]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "actiontrycastbyidontarget[1004,20]").AsNumber());

        automation.CastSent = false;
        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());
    }

    /// <summary>
    /// With the macro's caster bound, both cast forms go to it and not to
    /// the host, and still answer "begun": the reference hands the cast to
    /// its own spell system and answers 1. Mutation: send the cast to the
    /// host as well and the host sees a cast.
    /// </summary>
    [Fact]
    public void CastByIdGoesThroughTheMacrosCasterWhenOneIsBound()
    {
        var automation = CreateAutomation();
        automation.Spells[1001] = Spell(1001, school: 34, difficulty: 10);
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d)
            {
                EquippedLocation = 0x01000000u,
            },
        ];
        automation.Mode = PluginCombatMode.Magic;
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        var handed = new List<(uint Spell, uint? Target)>();
        runtime.Policy.BeginCast = (spellId, target) => handed.Add((spellId, target));

        Assert.Equal(1d, runtime.Evaluate("actiontrycastbyid[1001]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate(
            "actiontrycastbyidontarget[1001,20]").AsNumber());

        Assert.Equal([(1001u, (uint?)null), (1001u, (uint?)20u)], handed);
        Assert.Equal(default, automation.LastCast);
    }

    /// <summary>
    /// A busy character takes NO step towards magic mode: no equip request,
    /// no stance request, and the step answers false. An expression rule ticks
    /// every pass, so without the gate the same two requests are re-issued
    /// while an action is already in flight. Mutation: dropping the busy gate
    /// makes `automation.EquippedObject` 50 on the first evaluation.
    /// </summary>
    [Fact]
    public void MagicModeStepTakesNoStepWhileTheHostIsBusy()
    {
        var automation = CreateAutomation();
        automation.Spells[1002] = Spell(1002, school: 34, difficulty: 10) with
        {
            IsUntargeted = true,
        };
        automation.Equipment =
        [
            new PluginEquipmentItem(
                50, "Wand", 0u, 0x01000000u, 0u, 1u, 0u, 0, 0, 0, 0, 0d),
        ];
        automation.IsBusy = true;
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("getbusystate[]").AsNumber());
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(0u, automation.EquippedObject);
        Assert.Equal(0d, runtime.Evaluate("actiontrycastbyid[1002]").AsNumber());
        Assert.Equal(0u, automation.EquippedObject);
        Assert.Equal((0u, 0u), automation.LastCast);

        automation.IsBusy = false;
        Assert.False(runtime.Evaluate("actiontryequipanywand[]").IsTruthy);
        Assert.Equal(50u, automation.EquippedObject);
    }

    [Fact]
    public void LoginFunctionsUseTheAuthoritativeSortedRosterAndOneShotOwner()
    {
        var automation = CreateAutomation();
        automation.LoginRoster =
        [
            new PluginLoginCharacter(10u, "Beta", 2, false),
            new PluginLoginCharacter(1u, "Expression Tester", 0, false),
            new PluginLoginCharacter(30u, "Mule", 1, false),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("getcharacterindex[``]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate(
            "getcharacterindex[`bet`]").AsNumber());
        Assert.True(runtime.Evaluate("setnextlogin[`1`]").IsTruthy);
        Assert.Equal(30u, automation.NextLoginObjectId);
        Assert.True(runtime.Evaluate("setnextlogin[`bet`]").IsTruthy);
        Assert.Equal(10u, automation.NextLoginObjectId);
        Assert.True(runtime.Evaluate("clearnextlogin[]").IsTruthy);
        Assert.Equal(0u, automation.NextLoginObjectId);
    }

    /// <summary>
    /// The roster arrives in whatever order the account's list is kept in, and
    /// every index the login functions speak in is alphabetical. Mutation:
    /// reading the list in the order it arrived makes the current character
    /// index 0 rather than 1, and one step on from it lands on "Aardvark"
    /// instead of "Zeke".
    /// </summary>
    [Fact]
    public void LoginIndicesAreAlphabeticalWhateverOrderTheRosterArrivesIn()
    {
        var automation = CreateAutomation();
        automation.LoginRoster =
        [
            new PluginLoginCharacter(1u, "Expression Tester", 0, false),
            new PluginLoginCharacter(30u, "Zeke", 1, false),
            new PluginLoginCharacter(10u, "Aardvark", 2, false),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(0d, runtime.Evaluate("getcharacterindex[`aard`]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getcharacterindex[``]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("getcharacterindex[`zek`]").AsNumber());

        Assert.True(runtime.Evaluate("setnextlogin[`1`]").IsTruthy);
        Assert.Equal(30u, automation.NextLoginObjectId);
    }

    /// <summary>
    /// `chatbox` sends and gives its argument back; `chatboxpaste` stages the
    /// text in the chat entry WITHOUT sending it, strips control characters,
    /// and reports false when there is nothing to paste or the player is
    /// already typing. Mutation: pointing chatboxpaste at the same submit
    /// call sends the text and leaves the entry empty.
    /// </summary>
    [Fact]
    public void ChatboxSendsAndChatboxPasteOnlyStagesTheText()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        automation.SubmittedChat.Clear();

        Assert.Equal("/say hi", runtime.Evaluate("chatbox[`/say hi`]").AsString());
        Assert.Equal(["/say hi"], automation.SubmittedChat);

        Assert.True(runtime.Evaluate("chatboxpaste[`/tell Bob, `]").IsTruthy);
        Assert.Equal("/tell Bob, ", automation.ComposedChat);
        Assert.Equal(["/say hi"], automation.SubmittedChat);

        Assert.True(runtime.Evaluate(
            "chatboxpaste[chr[9]+`a`+chr[10]+`b`]").IsTruthy);
        Assert.Equal("ab", automation.ComposedChat);
        Assert.False(runtime.Evaluate("chatboxpaste[chr[9]]").IsTruthy);

        automation.CanCompose = false;
        Assert.False(runtime.Evaluate("chatboxpaste[`/tell Bob, `]").IsTruthy);
    }

    /// <summary>
    /// Both chat verbs take a STRING; a number is a type error, not a number
    /// rendered as text. A status-hud colour, on the other hand, is a 32-bit
    /// pattern and a negative one is legal. Mutation: rendering the chat
    /// argument through the display form makes the first two probes pass
    /// "5" and "6" to chat, and taking the colour as a checked unsigned
    /// number makes the third throw an overflow.
    /// </summary>
    [Fact]
    public void ChatVerbsRequireStringsAndTheStatusColourAcceptsNegatives()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        automation.SubmittedChat.Clear();

        Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate("chatbox[5]"));
        Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate("chatboxpaste[6]"));
        Assert.Empty(automation.SubmittedChat);
        Assert.Equal(string.Empty, automation.ComposedChat);

        Assert.False(runtime.Evaluate("statushudcolored[`k`,`v`,0-1]").IsTruthy);
    }

    /// <summary>
    /// `uisetvisible` treats ANY non-zero number as visible and hands back its
    /// second argument; `uisetlabel` answers 1 and raises an error for a
    /// control that cannot take a label. Mutation: the previous ">= 1" test
    /// hides a control at 0.5, and returning the host's bool loses both the
    /// echoed argument and the error.
    /// </summary>
    [Fact]
    public void UiVisibilityUsesNonZeroTruthAndLabelFailureIsAnError()
    {
        var automation = CreateAutomation();
        var ui = new RecordingUiRegistry();
        using var runtime = new MossTankExpressionRuntime(new Host(automation, ui: ui));

        Assert.Equal(0.5d, runtime.Evaluate(
            "uisetvisible[uigetcontrol[`view`,`control`],0.5]").AsNumber());
        Assert.True(ui.LastVisible);
        Assert.Equal(-1d, runtime.Evaluate(
            "uisetvisible[uigetcontrol[`view`,`control`],0-1]").AsNumber());
        Assert.True(ui.LastVisible);
        Assert.Equal(0d, runtime.Evaluate(
            "uisetvisible[uigetcontrol[`view`,`control`],0]").AsNumber());
        Assert.False(ui.LastVisible);

        Assert.Equal(1d, runtime.Evaluate(
            "uisetlabel[uigetcontrol[`view`,`control`],`Go`]").AsNumber());
        ui.LabelAccepted = false;
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate(
            "uisetlabel[uigetcontrol[`view`,`control`],`Go`]"));
    }

    [Fact]
    public void NetworkClientsReturnUtilityBeltDictionariesAndTagFiltering()
    {
        var automation = CreateAutomation();
        automation.NetworkClients =
        [
            new PluginNetworkClient(
                7u,
                70u,
                "Remote Mule",
                "Coldeve",
                automation.Position,
                ["mules", "trade"],
                90u,
                70u,
                80u,
                100u,
                100u,
                100u,
                90f),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        ExpressionList clients = runtime.Evaluate("netclients[`mules`]").AsList();
        ExpressionDictionary client = Assert.Single(clients.Items).AsDictionary();
        Assert.Equal("Remote Mule", client.Items["Name"].AsString());
        Assert.Equal(70d, client.Items["PlayerId"].AsNumber());
        Assert.Equal(2, client.Items["Tags"].AsList().Items.Count);
        Assert.Empty(runtime.Evaluate("netclients[`combat`]").AsList().Items);
    }

    /// <summary>
    /// <c>netcasts[]</c> shows what the other clients on this computer say
    /// they cast, straight from the host and never through the combat
    /// controller's own cursor, so a user can watch sharing work without a
    /// debugger and without disturbing what the macro has already taken in.
    /// The optional tag narrows it to peers whose record carries the tag,
    /// the way <c>netclients[]</c> does; a peer with no record is kept when
    /// no tag is asked for and dropped when one is.
    /// </summary>
    [Fact]
    public void NetworkCastsReturnWhatPeersReportedWithTagFiltering()
    {
        var automation = CreateAutomation();
        automation.NetworkClients =
        [
            new PluginNetworkClient(
                7u, 70u, "Remote Mule", "Coldeve", automation.Position,
                ["mules", "trade"], 90u, 70u, 80u, 100u, 100u, 100u, 90f),
        ];
        automation.NetworkCasts =
        [
            new PluginPeerCast(3, 7u, 70u, 500u, 2074u, 310, 42.5d, true),
            new PluginPeerCast(4, 8u, 80u, 500u, 2074u, 280, 0d, false),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        ExpressionList casts = runtime.Evaluate("netcasts[]").AsList();
        Assert.Equal(2, casts.Items.Count);
        ExpressionDictionary landed = casts.Items[0].AsDictionary();
        Assert.Equal(3d, landed.Items["Sequence"].AsNumber());
        Assert.Equal(7d, landed.Items["ClientId"].AsNumber());
        Assert.Equal(70d, landed.Items["CasterId"].AsNumber());
        Assert.Equal(500d, landed.Items["TargetId"].AsNumber());
        Assert.Equal(2074d, landed.Items["SpellId"].AsNumber());
        Assert.Equal(310d, landed.Items["EffectiveSkill"].AsNumber());
        Assert.Equal(42.5d, landed.Items["SecondsRemaining"].AsNumber());
        Assert.True(landed.Items["Landed"].IsTruthy);
        Assert.False(casts.Items[1].AsDictionary().Items["Landed"].IsTruthy);

        ExpressionList mules = runtime.Evaluate("netcasts[`mules`]").AsList();
        Assert.Equal(7d, Assert.Single(mules.Items).AsDictionary().Items["ClientId"].AsNumber());
        Assert.Empty(runtime.Evaluate("netcasts[`combat`]").AsList().Items);
    }

    [Fact]
    public void DelayedExecutionUsesTickAndCanBeCancelled()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation()));

        double id = runtime.Evaluate(
            "delayexec[100,`setvar[done,1]`]").AsNumber();
        runtime.OnTick(0.099);
        Assert.Equal(0d, runtime.Evaluate("getvar[`done`]").AsNumber());
        runtime.OnTick(0.001);
        Assert.Equal(1d, runtime.Evaluate("getvar[`done`]").AsNumber());

        double cancelled = runtime.Evaluate(
            "delayexec[1,`setvar[bad,1]`]").AsNumber();
        Assert.True(runtime.Evaluate($"clearexec[{cancelled}]").IsTruthy);
        runtime.OnTick(1d);
        Assert.Equal(0d, runtime.Evaluate("getvar[`bad`]").AsNumber());
        Assert.NotEqual(id, cancelled);
    }

    [Fact]
    public void PersistentAndGlobalVariablesRoundTripThroughPluginStorage()
    {
        var storage = new MemoryStorage();
        var automation = CreateAutomation();
        using (var first = new MossTankExpressionRuntime(new Host(automation, storage)))
        {
            first.Evaluate("setpvar[`count`,42];setgvar[`names`,listcreate[`a`,`b`]]");
        }

        using var second = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));
        Assert.Equal(42d, second.Evaluate("getpvar[`count`]").AsNumber());
        Assert.Equal(2d, second.Evaluate("listcount[getgvar[`names`]]").AsNumber());
        Assert.Contains(
            storage.Text.Keys,
            static key => key.StartsWith("expressions/persistent/", StringComparison.Ordinal));
        Assert.Contains(
            storage.Text.Keys,
            static key => key.StartsWith("expressions/global/", StringComparison.Ordinal));
    }

    /// <summary>
    /// UtilityBelt keeps each character's persistent variables apart. When
    /// the client moves to another character, the last save of the old
    /// character's set goes to the old character's file and the new
    /// character's own saved set is what it reads. Mutation: naming the file
    /// after the character logged in now writes the old (empty) set over the
    /// new character's file, and @kept reads 0.
    /// </summary>
    [Fact]
    public void ACharacterChangeSavesTheOldSetToTheOldCharacterAsUtilityBeltKeepsThemApart()
    {
        var storage = new MemoryStorage();
        var second = CreateAutomation();
        second.Name = "Second";
        using (var earlier = new MossTankExpressionRuntime(new Host(second, storage)))
            earlier.Evaluate("@kept=2");

        var automation = CreateAutomation();
        automation.Name = "First";
        using var runtime = new MossTankExpressionRuntime(new Host(automation, storage));
        automation.Name = "Second";

        Assert.Equal(2d, runtime.Evaluate("@kept").AsNumber());
        runtime.Evaluate("@mine=3");
        automation.Name = "First";
        Assert.False(runtime.Evaluate("testpvar[`kept`]").IsTruthy);
        Assert.False(runtime.Evaluate("testpvar[`mine`]").IsTruthy);
        automation.Name = "Second";
        Assert.Equal(3d, runtime.Evaluate("@mine").AsNumber());
    }

    /// <summary>
    /// UtilityBelt saves a persistent variable as it is set, so a set made
    /// before the expression fails is saved. Mutation: saving only after an
    /// expression completes leaves @early out of storage until some later
    /// expression succeeds, and a second client reads 0.
    /// </summary>
    [Fact]
    public void APersistentSetBeforeAnErrorIsSavedAsUtilityBeltSavesIt()
    {
        var storage = new MemoryStorage();
        using var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));

        Assert.ThrowsAny<Exception>(() => first.Evaluate("@early=4;nosuchfunction[]"));

        using var second = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.Equal(4d, second.Evaluate("@early").AsNumber());
    }

    /// <summary>
    /// A persistent list changed in place is saved at the end of the
    /// expression, as UtilityBelt writes back a changed persistent list
    /// when the run ends. Mutation: saving only after a set or a clear
    /// leaves the added item out of storage.
    /// </summary>
    [Fact]
    public void APersistentListChangedInPlaceIsSavedAsUtilityBeltSavesIt()
    {
        var storage = new MemoryStorage();
        using var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        first.Evaluate("@l=listcreate[1]");
        first.Evaluate("listadd[@l,2]");

        using var second = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.Equal("[1,2]", second.Evaluate("@l").ToDisplayString());
    }

    /// <summary>
    /// The persistent set is saved when something in it may have changed,
    /// not after every expression: an expression that only reads costs no
    /// save. Seen here through a list that cannot be saved in its new form,
    /// which is reported once per save. Mutation: saving after every
    /// expression reports it again for each of the reads.
    /// </summary>
    [Fact]
    public void ThePersistentSetIsSavedOnlyWhenSomethingInItChanged()
    {
        var storage = new MemoryStorage();
        var host = new Host(CreateAutomation(), storage);
        using var runtime = new MossTankExpressionRuntime(host);
        runtime.Evaluate("@l=listcreate[1]");
        runtime.Evaluate("listadd[@l,stopwatchcreate[]]");
        var errors = ((Logger)host.Log).Errors;
        Assert.Single(errors);

        runtime.Evaluate("listcount[@l]");
        runtime.Evaluate("1+1");
        runtime.Evaluate("@l{0}");

        Assert.Single(errors);
    }

    /// <summary>
    /// A number that is not finite is a value like any other, and
    /// UtilityBelt saves it: 0/0 and 1/0 in a persistent or global variable
    /// come back after a relog, and the variables set beside them are saved
    /// too. Mutation: dropping the named-literal number handling refuses the
    /// whole set, so nothing is saved.
    /// </summary>
    [Fact]
    public void NumbersThatAreNotFiniteAreSavedAsUtilityBeltSavesThem()
    {
        var storage = new MemoryStorage();
        using (var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
            first.Evaluate("@avg=0/0;@fast=1/0;&low=-1/0;@after=5");
            first.Evaluate("@later=6");
        }

        using var second = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.True(double.IsNaN(second.Evaluate("@avg").AsNumber()));
        Assert.Equal(double.PositiveInfinity, second.Evaluate("@fast").AsNumber());
        Assert.Equal(double.NegativeInfinity, second.Evaluate("&low").AsNumber());
        Assert.Equal(5d, second.Evaluate("@after").AsNumber());
        Assert.Equal(6d, second.Evaluate("@later").AsNumber());
    }

    /// <summary>
    /// UtilityBelt refuses a stopwatch or a UI control as a persistent
    /// value when it is set, with its own message, and stores nothing; the
    /// variables set afterwards save as usual. Mutation: taking the value
    /// into the persistent set unchecked keeps it readable and makes every
    /// later save fail, so @after never reaches the next session.
    /// </summary>
    [Theory]
    [InlineData("@sw=stopwatchcreate[]")]
    [InlineData("setpvar[`sw`,stopwatchcreate[]]")]
    public void AValueThatCannotBeSavedIsRefusedWhenSetAsUtilityBeltRefusesIt(string source)
    {
        var storage = new MemoryStorage();
        using (var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
            var error = Assert.Throws<ExpressionEvaluationException>(() => first.Evaluate(source));
            Assert.Equal(
                "There is currently no support for serializing Stopwatch types",
                error.Reason);
            Assert.False(first.Evaluate("testpvar[`sw`]").IsTruthy);
            first.Evaluate("@after=7");
        }

        using var second = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.Equal(7d, second.Evaluate("@after").AsNumber());
    }

    /// <summary>
    /// A persistent list changed in place to hold a stopwatch cannot be
    /// saved in its new form; it keeps the form it was last saved in, and
    /// every other persistent variable is still saved. Mutation: failing the
    /// whole save on that one list loses @after.
    /// </summary>
    [Fact]
    public void AListThatCameToHoldAnUnsavableValueKeepsItsSavedFormAndTheRestSave()
    {
        var storage = new MemoryStorage();
        using (var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
            first.Evaluate("@l=listcreate[1]");
            first.Evaluate("listadd[@l,stopwatchcreate[]];@after=8");
        }

        using var second = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.Equal("[1]", second.Evaluate("@l").ToDisplayString());
        Assert.Equal(8d, second.Evaluate("@after").AsNumber());
    }

    /// <summary>
    /// Global variables belong to the server: two clients on different
    /// accounts, both already logged in, see each other's writes on their
    /// next read, and a client on another server does not. Mutation: keying
    /// the store by account again, or loading it once at login, leaves the
    /// second client reading 0.
    /// </summary>
    [Fact]
    public void GlobalVariablesAreSharedLiveByEveryClientOnTheSameServer()
    {
        using var folder = new TemporaryFolder();
        var storage = new DirectoryStorage(folder.Path);
        Automation leaderAutomation = CreateAutomation();
        leaderAutomation.AccountName = "leader-account";
        leaderAutomation.Name = "Leader";
        Automation followerAutomation = CreateAutomation();
        followerAutomation.AccountName = "follower-account";
        followerAutomation.Name = "Follower";
        Automation elsewhereAutomation = CreateAutomation();
        elsewhereAutomation.WorldName = "Frostfell";
        using var leader = new MossTankExpressionRuntime(new Host(leaderAutomation, storage));
        using var follower = new MossTankExpressionRuntime(new Host(followerAutomation, storage));
        using var elsewhere = new MossTankExpressionRuntime(new Host(elsewhereAutomation, storage));

        leader.Evaluate("setgvar[`roster`,listcreate[`Leader`]]");
        Assert.Equal(1d, follower.Evaluate("listcount[getgvar[`roster`]]").AsNumber());
        Assert.False(elsewhere.Evaluate("testgvar[`roster`]").IsTruthy);

        follower.Evaluate("&heartbeat=5");
        Assert.Equal(5d, leader.Evaluate("getgvar[`heartbeat`]").AsNumber());

        follower.Evaluate("cleargvar[`roster`]");
        Assert.False(leader.Evaluate("testgvar[`roster`]").IsTruthy);
        leader.Evaluate("clearallgvars[]");
        Assert.False(follower.Evaluate("testgvar[`heartbeat`]").IsTruthy);
    }

    /// <summary>
    /// Two clients writing their own variables at the same moment keep both
    /// sets, and neither ever fails on a file the other is replacing. The two
    /// runtimes stand for two sessions in one process; the lock they share is
    /// the same one two processes meet on. Mutation: dropping the lock makes
    /// reads and replaces fail while the other writes; one shared file for
    /// the whole set loses the other writer's variables.
    /// </summary>
    [Fact]
    public async Task ConcurrentClientsKeepEachOthersGlobalVariables()
    {
        const int PerClient = 150;
        using var folder = new TemporaryFolder();
        var storage = new DirectoryStorage(folder.Path);
        Automation firstAutomation = CreateAutomation();
        firstAutomation.AccountName = "first-account";
        Automation secondAutomation = CreateAutomation();
        secondAutomation.AccountName = "second-account";
        using var first = new MossTankExpressionRuntime(new Host(firstAutomation, storage));
        using var second = new MossTankExpressionRuntime(new Host(secondAutomation, storage));

        void Run(MossTankExpressionRuntime runtime, string prefix, string other)
        {
            for (int index = 0; index < PerClient; index++)
            {
                runtime.Evaluate($"setgvar[`{prefix}{index}`,{index}]");
                runtime.Evaluate($"getgvar[`{other}{index}`]");
                runtime.Evaluate("testgvar[`shared`]");
                runtime.Evaluate($"setgvar[`shared`,`{prefix}`]");
            }
        }

        Task writerA = Task.Run(() => Run(first, "a", "b"));
        Task writerB = Task.Run(() => Run(second, "b", "a"));
        await Task.WhenAll(writerA, writerB);

        using var reader = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));
        for (int index = 0; index < PerClient; index++)
        {
            Assert.Equal(index, reader.Evaluate($"getgvar[`a{index}`]").AsNumber());
            Assert.Equal(index, reader.Evaluate($"getgvar[`b{index}`]").AsNumber());
        }
    }

    /// <summary>
    /// A list read from a global variable and changed in place is saved when
    /// the expression ends, and the same expression reads its own change
    /// back. Mutation: dropping the write-back leaves the list unchanged for
    /// the next client.
    /// </summary>
    [Fact]
    public void AGlobalListChangedInPlaceIsSavedForTheNextClient()
    {
        using var folder = new TemporaryFolder();
        var storage = new DirectoryStorage(folder.Path);
        using var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Automation otherAutomation = CreateAutomation();
        otherAutomation.AccountName = "other-account";
        using var second = new MossTankExpressionRuntime(new Host(otherAutomation, storage));

        first.Evaluate("setgvar[`roster`,listcreate[`Leader`]]");
        Assert.Equal(
            2d,
            second.Evaluate("listadd[getgvar[`roster`],`Follower`];listcount[getgvar[`roster`]]")
                .AsNumber());

        Assert.Equal("Follower", first.Evaluate("listgetitem[getgvar[`roster`],1]").AsString());
    }

    /// <summary>
    /// The shorthand reads a global the same way getgvar does, so a list or
    /// dictionary changed in place through it is saved too. Mutation: not
    /// recording the read leaves both unchanged.
    /// </summary>
    [Fact]
    public void AGlobalChangedInPlaceThroughTheShorthandIsSaved()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate("setgvar[`roster`,listcreate[`a`]];setgvar[`bag`,dictcreate[]]");

        runtime.Evaluate("listadd[&roster,`b`];dictadditem[&bag,`k`,1]");

        Assert.Equal(2d, runtime.Evaluate("listcount[getgvar[`roster`]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("dictsize[getgvar[`bag`]]").AsNumber());
    }

    /// <summary>
    /// A bare in-place removal from a global list, with no setgvar after it,
    /// is saved: the reference's write-back saves any tracked list whose
    /// items changed, which the shared-colony metas rely on to take a
    /// character off the running list. Mutation: dropping the write-back
    /// keeps the name on the list.
    /// </summary>
    [Fact]
    public void ABareRemovalFromAGlobalListIsSaved()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate("setgvar[`running`,listcreate[`Alpha`,`Beta`]]");

        runtime.Evaluate("listremove[getgvar[`running`],`Alpha`]");

        Assert.Equal(1d, runtime.Evaluate("listcount[getgvar[`running`]]").AsNumber());
        Assert.Equal("Beta", runtime.Evaluate("listgetitem[getgvar[`running`],0]").AsString());
    }

    /// <summary>
    /// A global read and changed in place is written back when the expression
    /// ends, after everything else it did, so a set, a clear or a clear-all
    /// of the same name earlier in the same expression is overwritten, as the
    /// reference's end-of-run write-back does. Mutation: forgetting the read
    /// on a set or a clear keeps the later value.
    /// </summary>
    [Theory]
    [InlineData("listadd[getgvar[`r`],`x`];setgvar[`r`,listcreate[]]")]
    [InlineData("listadd[&r,`x`];&r=listcreate[]")]
    [InlineData("listadd[getgvar[`r`],`x`];cleargvar[`r`]")]
    [InlineData("listadd[&r,`x`];clearallgvars[]")]
    public void TheWriteBackOverwritesASetOrClearInTheSameExpression(string source)
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate("setgvar[`r`,listcreate[`a`]]");

        runtime.Evaluate(source);

        Assert.True(runtime.Evaluate("testgvar[`r`]").IsTruthy);
        Assert.Equal(2d, runtime.Evaluate("listcount[getgvar[`r`]]").AsNumber());
    }

    /// <summary>
    /// What counts is that the list or dictionary was changed, not whether
    /// its contents differ at the end: an add undone by a pop, or clearing a
    /// list that was already empty, is still a change and is still written
    /// back over the clear that followed. Mutation: comparing the contents
    /// before and after leaves the variable cleared.
    /// </summary>
    [Theory]
    [InlineData("listadd[&r,`x`];listpop[&r];cleargvar[`r`]", 1d)]
    [InlineData("listclear[&e];cleargvar[`e`]", 0d)]
    [InlineData("dictadditem[&d,`k`,1];dictremovekey[&d,`k`];cleargvar[`d`]", 0d)]
    public void AnyChangeIsWrittenBackEvenWhenTheContentsEndTheSame(
        string source,
        double count)
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate(
            "setgvar[`r`,listcreate[`a`]];setgvar[`e`,listcreate[]];setgvar[`d`,dictcreate[]]");

        runtime.Evaluate(source);

        string name = source[source.IndexOf("clear", StringComparison.Ordinal)..]
            .Split('`')[1];
        Assert.True(runtime.Evaluate($"testgvar[`{name}`]").IsTruthy);
        Assert.Equal(
            count,
            runtime.Evaluate(name == "d"
                ? "dictsize[getgvar[`d`]]"
                : $"listcount[getgvar[`{name}`]]").AsNumber());
    }

    /// <summary>
    /// Reading a global without changing it, or a call that changes nothing
    /// (removing an item or a key that is not there, clearing a dictionary
    /// that is already empty), writes nothing back, so a set later in the
    /// expression stands. Mutation: writing back every read list or
    /// dictionary, or marking those calls as changes, overwrites the set.
    /// </summary>
    [Theory]
    [InlineData("listcount[getgvar[`r`]];setgvar[`r`,5]", "r")]
    [InlineData("listremove[&r,`absent`];setgvar[`r`,5]", "r")]
    [InlineData("dictremovekey[&d,`absent`];setgvar[`d`,5]", "d")]
    [InlineData("dictclear[&z];setgvar[`z`,5]", "z")]
    public void AGlobalReadButNotChangedIsNotWrittenBack(string source, string name)
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate(
            "setgvar[`r`,listcreate[`a`]];setgvar[`d`,dictcreate[`k`,1]];setgvar[`z`,dictcreate[]]");

        runtime.Evaluate(source);

        Assert.Equal(5d, runtime.Evaluate($"getgvar[`{name}`]").AsNumber());
    }

    /// <summary>
    /// Only the list or dictionary the variable holds is watched. A change
    /// to a list nested inside it is not a change to the variable and is
    /// not saved; a list stored into a global and then changed through
    /// another variable is not saved either. Mutation: comparing the whole
    /// saved value saves the nested change.
    /// </summary>
    [Fact]
    public void ANestedOrUntrackedChangeIsNotSaved()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate("setgvar[`nest`,listcreate[listcreate[1]]]");

        runtime.Evaluate("listadd[listgetitem[&nest,0],2]");
        runtime.Evaluate("setvar[`l`,listcreate[1]];setgvar[`copy`,getvar[`l`]];listadd[getvar[`l`],2]");

        Assert.Equal(1d, runtime.Evaluate("listcount[listgetitem[getgvar[`nest`],0]]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("listcount[getgvar[`copy`]]").AsNumber());
    }

    /// <summary>
    /// An expression that fails part way saves none of its in-place changes,
    /// as the reference's write-back runs only after a run that completed.
    /// Mutation: writing back from the failed run saves the added item.
    /// </summary>
    [Fact]
    public void AFailedExpressionWritesNothingBack()
    {
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), new MemoryStorage()));
        runtime.Evaluate("setgvar[`r`,listcreate[`a`]]");

        Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate("listadd[&r,`x`];nosuchfunction[]"));

        Assert.Equal(1d, runtime.Evaluate("listcount[getgvar[`r`]]").AsNumber());
    }

    /// <summary>
    /// A damaged variable file is the reference's bad database row: reading
    /// it is an error of the expression that reads it, by the shorthand or
    /// by getgvar alike, and never a raw parser error that gets past the
    /// meta's handling of expression errors. The row still exists, so
    /// testgvar and touchgvar see it and cleargvar and setgvar replace it;
    /// the other variables read as before. Mutation: letting the parser's
    /// error through fails the shorthand read; treating the file as absent
    /// makes testgvar answer 0.
    /// </summary>
    [Theory]
    [InlineData("{ not json")]
    [InlineData("   ")]
    [InlineData("null")]
    [InlineData("{\"Name\":\"r\",\"Value\":null}")]
    [InlineData("{\"Name\":\"r\",\"Value\":{\"Kind\":null}}")]
    [InlineData("{\"Name\":\"r\",\"Value\":{\"Kind\":\"list\",\"List\":[null]}}")]
    [InlineData("{\"Name\":\"r\",\"Value\":{\"Kind\":\"worldobject\",\"Number\":-1}}")]
    public void ADamagedGlobalVariableIsAnErrorOfTheExpressionThatReadsIt(string damaged)
    {
        var storage = new MemoryStorage();
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));
        runtime.Evaluate("setgvar[`r`,1]");
        string key = storage.Text.Keys.Single(static key =>
            key.StartsWith("expressions/global/", StringComparison.Ordinal));
        runtime.Evaluate("setgvar[`ok`,2]");
        storage.Text[key] = damaged;

        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("&r"));
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("getgvar[`r`]"));
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("listadd[&r,1]"));
        Assert.Equal(2d, runtime.Evaluate("&ok").AsNumber());
        Assert.True(runtime.Evaluate("testgvar[`r`]").IsTruthy);
        Assert.True(runtime.Evaluate("touchgvar[`r`]").IsTruthy);
        Assert.True(runtime.Evaluate("cleargvar[`r`]").IsTruthy);
        Assert.False(runtime.Evaluate("testgvar[`r`]").IsTruthy);

        storage.Text[key] = damaged;
        Assert.Equal(3d, runtime.Evaluate("setgvar[`r`,3]").AsNumber());
        Assert.Equal(3d, runtime.Evaluate("&r").AsNumber());
    }

    /// <summary>
    /// A storage failure (a file held open by another program, a folder the
    /// client may not write) is an error of the expression that met it,
    /// including the save of a changed list when the expression ends.
    /// Mutation: letting the storage error through fails the shorthand read,
    /// the shorthand set and the end-of-expression save, which no function
    /// call wraps.
    /// </summary>
    [Theory]
    [InlineData("read", "&r")]
    [InlineData("read", "testgvar[`r`]")]
    [InlineData("write", "setgvar[`r`,2]")]
    [InlineData("write", "&r=2")]
    [InlineData("write", "listadd[&l,1]")]
    [InlineData("delete", "cleargvar[`r`]")]
    [InlineData("delete", "clearallgvars[]")]
    [InlineData("list", "clearallgvars[]")]
    public void AStorageFailureIsAnErrorOfTheExpression(string operation, string source)
    {
        foreach (Exception failure in new Exception[]
        {
            new IOException("the file is in use"),
            new UnauthorizedAccessException("access denied"),
        })
        {
            var storage = new FailingStorage();
            using var runtime = new MossTankExpressionRuntime(
                new Host(CreateAutomation(), storage));
            runtime.Evaluate("setgvar[`r`,1];setgvar[`l`,listcreate[]]");
            storage.Fail(operation, failure);

            var error = Assert.Throws<ExpressionEvaluationException>(
                () => runtime.Evaluate(source));
            Assert.Contains(failure.Message, error.Message, StringComparison.Ordinal);
        }
    }

    /// <summary>
    /// The per-account file earlier versions wrote joins the shared set once
    /// and is removed.
    /// </summary>
    [Fact]
    public void TheOldPerAccountGlobalFileJoinsTheSharedSet()
    {
        var storage = new MemoryStorage();
        using (var old = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
        }
        string legacyKey = "expressions/global/"
            + Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(
                System.Text.Encoding.UTF8.GetBytes("Coldeve\nexample-account")).AsSpan(0, 12))
                .ToLowerInvariant()
            + ".json";
        storage.Text[legacyKey] =
            "{\"carried\":{\"Kind\":\"number\",\"Number\":7}}";

        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));

        Assert.Equal(7d, runtime.Evaluate("getgvar[`carried`]").AsNumber());
        Assert.False(storage.Text.ContainsKey(legacyKey));
    }

    [Fact]
    public void FellowshipFunctionsExposeTheAuthoritativeCompleteRoster()
    {
        var automation = CreateAutomation();
        automation.FellowRoster =
        [
            new PluginFellowMember(1, "Expression Tester", 90, 100, 80, 100, 70, 100, 0),
            new PluginFellowMember(22, "Fellow Two", 50, 60, 40, 60, 30, 60, 1),
        ];
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate("getfellowshipstatus[]").IsTruthy);
        Assert.Equal("Test Fellowship", runtime.Evaluate("getfellowshipname[]").AsString());
        Assert.Equal(2d, runtime.Evaluate("getfellowshipcount[]").AsNumber());
        Assert.True(runtime.Evaluate("getfellowshipisleader[]").IsTruthy);
        Assert.True(runtime.Evaluate("getfellowshipcanrecruit[]").IsTruthy);
        Assert.Equal("Fellow Two", runtime.Evaluate("getfellowname[1]").AsString());
        Assert.Equal(22d, runtime.Evaluate("getfellowid[1]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("listcount[getfellowids[]]").AsNumber());
    }

    [Fact]
    public void DerethTimeFunctionsUseTheRuntimeWorldClockProjection()
    {
        var automation = CreateAutomation();
        automation.Time = new PluginWorldTimeSnapshot(
            true,
            123456d,
            142,
            6,
            17,
            10,
            "HarvestGain",
            "Warmtide",
            true,
            0d,
            12.5d);
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(142d, runtime.Evaluate("getgameyear[]").AsNumber());
        Assert.Equal(6d, runtime.Evaluate("getgamemonth[]").AsNumber());
        Assert.Equal("HarvestGain", runtime.Evaluate("getgamemonthname[6]").AsString());
        Assert.Equal(17d, runtime.Evaluate("getgameday[]").AsNumber());
        Assert.Equal("Warmtide", runtime.Evaluate("getgamehourname[10]").AsString());
        Assert.Equal(123456d, runtime.Evaluate("getgameticks[]").AsNumber());
        Assert.True(runtime.Evaluate("getisday[]").IsTruthy);
        Assert.Equal(12.5d, runtime.Evaluate("getminutesuntilnight[]").AsNumber());
    }

    [Fact]
    public void ExperienceMeterAccumulatesCanonicalXpAndLuminanceDeltas()
    {
        var automation = CreateAutomation();
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            int64s: new Dictionary<uint, long> { [1] = 1000, [6] = 10 });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        runtime.OnTick(1d);
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            int64s: new Dictionary<uint, long> { [1] = 1100, [6] = 15 });
        runtime.OnTick(1d);

        Assert.Equal(100d, runtime.Evaluate("xptotal[]").AsNumber());
        Assert.Equal(5d, runtime.Evaluate("lumtotal[]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("xpduration[]").AsNumber());
        Assert.Equal(180000d, runtime.Evaluate("xpavg[]").AsNumber());
        Assert.Contains("100 XP", runtime.Evaluate("xpmeter[]").AsString());
        Assert.True(runtime.Evaluate("xpreset[]").IsTruthy);
        Assert.Equal(0d, runtime.Evaluate("xptotal[]").AsNumber());
    }

    /// <summary>
    /// The refresh MossTank asks for itself (at login, and /ub myquests)
    /// reads the quest lines and keeps them off the screen, as UtilityBelt
    /// does, then says once that the list is in. Other lines pass, and once
    /// the refresh is over a /myquests the player types is shown.
    /// Mutation: a filter that never drops leaves the quest lines on screen;
    /// one that drops without reading loses the flags; one that outlives the
    /// refresh hides the player's own /myquests.
    /// </summary>
    [Fact]
    public void AnOwnQuestRefreshHidesTheQuestLinesItReads()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        Assert.Equal(["/myquests"], automation.SubmittedChat);

        Assert.False(automation.Deliver(
            "killtaskdrudges - 3 solves (0)\"Drudges killed\" 10 0"));
        Assert.True(automation.Deliver("Horan tells you, \"hi\""));
        runtime.OnTick(0d);

        Assert.True(runtime.Evaluate("testquestflag[`killtaskdrudges`]").IsTruthy);
        Assert.Equal(3d, runtime.Evaluate(
            "getquestktprogress[`killtaskdrudges`]").AsNumber());
        Assert.Empty(automation.Posted);

        runtime.OnTick(1.001d);
        Assert.False(runtime.Evaluate("isrefreshingquests[]").IsTruthy);
        // The reference's ordinary line, in its System text class.
        Assert.Equal([("[UB] Quest data updated.", UbChat.GenericChatType)], automation.Posted);

        Assert.True(automation.Deliver(
            "onevisit - 1 solves (1700000000)\"Visited once\" 1 0"));
        runtime.OnTick(0d);
        Assert.True(runtime.Evaluate("testquestflag[`onevisit`]").IsTruthy);
    }

    [Fact]
    public void TheQuestFilterGoesWithTheRuntime()
    {
        var automation = CreateAutomation();
        var runtime = new MossTankExpressionRuntime(new Host(automation));
        Assert.Equal(1, automation.ChatFilterCount);

        runtime.Dispose();

        Assert.Equal(0, automation.ChatFilterCount);
    }

    /// <summary>
    /// UtilityBelt decides a flag's kind by its key first: a kill task's
    /// flag (killtask, killcount, slayerquest, totalgolem...dead, ...kills)
    /// is timed even at one solve of one, so it is ready once its timer has
    /// run out; any other flag at one solve of one is done for good.
    /// Mutation: judging by the counts alone answers not ready for the kill
    /// task.
    /// </summary>
    [Fact]
    public void AKillTaskFlagAtOneSolveOfOneIsTimedAsUtilityBeltTreatsIt()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        automation.ChatMessages.Add(new PluginChatMessage(
            1, 0, 0, string.Empty,
            "slayerquestbanderling - 1 solves (1700000000)\"Banderlings\" 1 60",
            string.Empty));
        automation.ChatMessages.Add(new PluginChatMessage(
            2, 0, 0, string.Empty,
            "onevisit - 1 solves (1700000000)\"Visited once\" 1 60",
            string.Empty));
        runtime.OnTick(0d);

        Assert.True(runtime.Evaluate("getqueststatus[`slayerquestbanderling`]").IsTruthy);
        Assert.True(runtime.Evaluate("getqueststatus[`SlayerQuestBanderling`]").IsTruthy);
        Assert.False(runtime.Evaluate("getqueststatus[`onevisit`]").IsTruthy);
    }

    [Fact]
    public void QuestFunctionsParseTheAuthoritativeMyquestsTranscript()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        Assert.Equal(["/myquests"], automation.SubmittedChat);
        Assert.True(runtime.Evaluate("isrefreshingquests[]").IsTruthy);

        automation.ChatMessages.Add(new PluginChatMessage(
            1, 0, 0, string.Empty,
            "killtaskdrudges - 3 solves (0)\"Drudges killed\" 10 0",
            string.Empty));
        automation.ChatMessages.Add(new PluginChatMessage(
            2, 0, 0, string.Empty,
            "onevisit - 1 solves (1700000000)\"Visited once\" 1 0",
            string.Empty));
        runtime.OnTick(0d);

        Assert.True(runtime.Evaluate("testquestflag[`killtaskdrudges`]").IsTruthy);
        Assert.Equal(3d, runtime.Evaluate(
            "getquestktprogress[`killtaskdrudges`]").AsNumber());
        Assert.Equal(10d, runtime.Evaluate(
            "getquestktrequired[`killtaskdrudges`]").AsNumber());
        Assert.False(runtime.Evaluate("getqueststatus[`onevisit`]").IsTruthy);
        Assert.True(runtime.Evaluate("getqueststatus[`unknown`]").IsTruthy);

        runtime.OnTick(1.001d);
        Assert.False(runtime.Evaluate("isrefreshingquests[]").IsTruthy);
    }

    [Fact]
    public void CorpseFunctionsUseTheCanonicalExternalContainerHistory()
    {
        var automation = CreateAutomation();
        automation.Corpses =
        [
            new PluginLootContainer(30, 300, "Corpse", 2f, false, false, false),
            new PluginLootContainer(31, 301, "Corpse", 3f, true, false, false),
        ];
        // The client knows the corpses: an id it does not know is an error.
        foreach (uint corpse in new uint[] { 30, 31 })
        {
            automation.WorldObjects.Add(new PluginWorldObject(
                corpse, 21, "Corpse", PluginObjectClass.Corpse, 0x200, 1, 0));
        }
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.False(runtime.Evaluate("hascorpsebeenopenedbyme[30]").IsTruthy);
        Assert.True(runtime.Evaluate("hascorpsebeenopenedbyme[31]").IsTruthy);
        Assert.Equal(30d, runtime.Evaluate(
            "listgetitem[getcorpsesunopenedbyme[],0]").AsNumber());
    }

    /// <summary>
    /// UtilityBelt lists an unopened corpse by its signed id, the form
    /// <c>wobjectgetid</c> hands out, and a corpse's id is a dynamic one at
    /// or above 0x80000000, so the list holds the same number the object's
    /// own id reads as. Mutation: listing the ids unsigned makes the
    /// membership test false.
    /// </summary>
    [Fact]
    public void UnopenedCorpsesAreListedBySignedIdAsUtilityBeltListsThem()
    {
        const uint corpse = 0x80000030u;
        var automation = CreateAutomation();
        automation.Corpses = [new PluginLootContainer(corpse, 300, "Corpse", 2f, false, false, false)];
        automation.WorldObjects.Add(new PluginWorldObject(
            corpse, 21, "Corpse", PluginObjectClass.Corpse, 0x200, 1, 0));
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(-2147483600d, runtime.Evaluate(
            "listgetitem[getcorpsesunopenedbyme[],0]").AsNumber());
        Assert.True(runtime.Evaluate(
            "listcontains[getcorpsesunopenedbyme[],wobjectgetid[wobjectfindbyid[-2147483600]]]")
            .IsTruthy);
    }

    [Fact]
    public void ComponentFunctionsUseTheAuthenticDatCatalogProjection()
    {
        var automation = CreateAutomation();
        automation.Components[7] = new PluginSpellComponentInfo(
            7, 101, "Malar Herb", 0.25, 0x13000001, 0.75,
            0x06000010, 4, "Herb", "Malar");
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal("Malar Herb", runtime.Evaluate("componentname[7]").AsString());
        Assert.Equal(0.25d, runtime.Evaluate(
            "dictgetitem[componentdata[7],`BurnRate`]").AsNumber());
        Assert.Equal("Malar", runtime.Evaluate(
            "dictgetitem[componentdata[7],`Word`]").AsString());
    }

    [Fact]
    public void UstFunctionsStageAndSubmitThroughTheCanonicalSalvageCommand()
    {
        var automation = CreateAutomation();
        automation.InventoryItems =
        [
            InventoryItem(40, "Ust"),
            InventoryItem(41, "Salvage One"),
            InventoryItem(42, "Salvage Two"),
        ];
        // The client knows the salvage it stages: an unknown id is an error.
        foreach (uint salvage in new uint[] { 41, 42 })
        {
            automation.WorldObjects.Add(new PluginWorldObject(
                salvage, 20981, "Salvage", PluginObjectClass.Salvage, 0x40000000, 1, 0)
            {
                IsOwned = true,
            });
        }
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.True(runtime.Evaluate("ustopen[]").IsTruthy);
        Assert.Equal(40u, automation.UsedObject);
        Assert.True(runtime.Evaluate("ustadd[41]").IsTruthy);
        Assert.True(runtime.Evaluate("ustadd[42]").IsTruthy);
        Assert.True(runtime.Evaluate("ustsalvage[]").IsTruthy);
        Assert.Equal(40u, automation.LastSalvage.Tool);
        Assert.Equal([41u, 42u], automation.LastSalvage.Items);
        Assert.False(runtime.Evaluate("ustsalvage[]").IsTruthy);
    }

    private static Automation CreateAutomation()
    {
        var automation = new Automation
        {
            Position = new PluginNavigationPosition(
                0x7F7F0001u, 10d, 20d, 3d, 90f, true),
            Attributes =
            [
                new PluginAttributeInfo(0, "Strength", 110) { Base = 100 },
            ],
            Skills =
            [
                new PluginSkillInfo(34, "War Magic", PluginSkillTraining.Trained, 300)
                {
                    Base = 275,
                },
            ],
        };
        automation.WorldObjects.Add(new PluginWorldObject(
            1, 1, "Expression Tester", PluginObjectClass.Player, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position,
            ItemsCapacity = 10,
            ContainersCapacity = 2,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            10, 100, "Health Elixir", PluginObjectClass.Food, 0x20, 1, 0)
        {
            IsOwned = true,
            StackSize = 10,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            11, 101, "Small Pack", PluginObjectClass.Container, 0x200, 1, 0)
        {
            IsOwned = true,
            ItemsCapacity = 8,
        });
        automation.WorldObjects.Add(new PluginWorldObject(
            20, 200, "Drudge", PluginObjectClass.Monster, 0x10, 0, 0)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = automation.Position with { EastWest = 10.1d },
            HasAppraisalData = true,
            LastIdTime = 54321,
        });
        automation.Properties[1] = Properties(
            ints: new Dictionary<uint, int> { [5] = 50, [96] = 100 },
            strings: new Dictionary<uint, string> { [1] = "Expression Tester" });
        automation.Properties[20] = Properties(
            ints: new Dictionary<uint, int> { [25] = 77 });
        return automation;
    }

    private static PluginItemProperties Properties(
        IReadOnlyDictionary<uint, int>? ints = null,
        IReadOnlyDictionary<uint, long>? int64s = null,
        IReadOnlyDictionary<uint, string>? strings = null) => new(
            ints ?? new Dictionary<uint, int>(),
            int64s ?? new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            strings ?? new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());

    private static PluginSpellInfo Spell(uint id, uint school, int difficulty) => new(
        id,
        $"Spell {id}",
        1u,
        1,
        difficulty,
        10,
        30f,
        school,
        string.Empty,
        false,
        true);

    private static PluginInventoryItem InventoryItem(uint id, string name) => new(
        id, 0u, name, 0u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
        1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private sealed class Host(
        Automation automation,
        IPluginStorage? storage = null,
        IUiRegistry? ui = null) : IPluginHost
    {
        public bool HasUi => ui is not null;
        public IPluginLogger Log { get; } = new Logger();
        public IGameState State { get; } = new State();
        public IEvents Events { get; } = new Events();
        public Selection Selection { get; } = new();
        ISelectionService IPluginHost.Selection => Selection;
        public IUiRegistry Ui => ui ?? NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } = storage ?? NoOpPluginStorage.Instance;
        public IAutomationSurface Automation { get; } = automation;
    }

    private sealed class Automation :
        IAutomationSurface,
        ICharacterInfo,
        ISpellCatalog,
        IMagicCommands,
        IPluginChat,
        IWorldObjectAutomation,
        IItemAutomation,
        INavigationAutomation,
        IFellowshipAutomation,
        IWorldTimeAutomation,
        ILootAutomation,
        ILoginAutomation,
        INetworkAutomation,
        IEquipmentAutomation,
        ICombatAutomation
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        ISpellCatalog IAutomationSurface.Spells => this;
        public IMagicCommands Magic => this;
        IEquipmentAutomation IAutomationSurface.Equipment => this;
        public ICombatAutomation Combat => this;
        public IPluginChat Chat => this;
        public IWorldObjectAutomation Objects => this;
        public IItemAutomation Items => this;
        public INavigationAutomation Navigation => this;
        public IFellowshipAutomation Fellowship => this;
        public ILootAutomation Loot => this;
        public IWorldTimeAutomation WorldTime => this;
        public ILoginAutomation Login => this;
        public INetworkAutomation Network => this;
        public bool IsInWorld => true;
        public string Name { get; set; } = "Expression Tester";
        public string WorldName { get; set; } = "Coldeve";
        public string AccountName { get; set; } = "example-account";
        public int CharacterIndex => 2;
        public uint ObjectId => 1;
        public uint CurrentHealth => 90;
        public uint MaxHealth => 100;
        public uint BaseHealth => 80;
        public uint CurrentStamina => 80;
        public uint MaxStamina => 100;
        public uint BaseStamina => 70;
        public uint CurrentMana => 70;
        public uint MaxMana => 100;
        public uint BaseMana => 60;
        public IReadOnlyList<PluginSkillInfo> Skills { get; set; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; set; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];
        public IReadOnlyList<PluginSpellInfo> KnownAttackSpells => [];
        public IReadOnlyList<PluginSpellInfo> KnownCombatSpells => [];
        public List<PluginWorldObject> WorldObjects { get; } = [];
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];
        public Dictionary<uint, PluginSpellComponentInfo> Components { get; } = [];
        public PluginNavigationPosition Position { get; set; }
        public uint UsedObject { get; private set; }
        public (uint SpellId, uint TargetId) LastCast { get; private set; }
        public HashSet<uint> UnknownSpells { get; } = [];
        public bool CastSent { get; set; } = true;
        public PluginMovementIntent LastIntent { get; private set; }
        public int ClearMovementCount { get; private set; }
        public List<PluginChatMessage> ChatMessages { get; } = [];
        public List<string> SubmittedChat { get; } = [];
        public IReadOnlyList<PluginInventoryItem> InventoryItems { get; set; } = [];
        public (uint Tool, IReadOnlyList<uint> Items) LastSalvage { get; private set; }
        public IReadOnlyList<PluginFellowMember> FellowRoster { get; set; } = [];
        public IReadOnlyList<PluginLoginCharacter> LoginRoster { get; set; } = [];
        public uint NextLoginObjectId { get; private set; }
        public IReadOnlyList<PluginNetworkClient> NetworkClients { get; set; } = [];
        public IReadOnlyList<PluginLootContainer> Corpses { get; set; } = [];
        public IReadOnlyList<PluginLootContainer> CaptureCorpses(float maximumDistance) =>
            Corpses.Where(corpse => corpse.Distance <= maximumDistance).ToArray();
        public bool IsInFellowship => FellowRoster.Count != 0;
        string IFellowshipAutomation.Name => "Test Fellowship";
        public uint LeaderObjectId => 1;
        public bool IsOpen => true;
        public bool IsLocked => false;
        public int MemberCount => FellowRoster.Count;
        IReadOnlyList<PluginFellowMember> IFellowshipAutomation.CaptureRoster() =>
            FellowRoster;
        bool ILoginAutomation.IsAvailable => true;
        IReadOnlyList<PluginLoginCharacter> ILoginAutomation.CaptureRoster() =>
            LoginRoster;
        public bool SetNextLogin(uint characterObjectId)
        {
            if (!LoginRoster.Any(character =>
                character.ObjectId == characterObjectId
                && !character.IsPendingDelete))
            {
                return false;
            }
            NextLoginObjectId = characterObjectId;
            return true;
        }
        public bool ClearNextLogin()
        {
            NextLoginObjectId = 0u;
            return true;
        }
        bool INetworkAutomation.IsAvailable => true;
        IReadOnlyList<PluginNetworkClient> INetworkAutomation.CaptureClients() =>
            NetworkClients;
        public IReadOnlyList<PluginPeerCast> NetworkCasts { get; set; } = [];
        IReadOnlyList<PluginPeerCast> INetworkAutomation.CaptureCasts(long afterSequence) =>
            NetworkCasts.Where(cast => cast.Sequence > afterSequence).ToArray();
        public PluginWorldTimeSnapshot Time { get; set; }
        PluginWorldTimeSnapshot IWorldTimeAutomation.Snapshot => Time;

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in Skills)
            {
                if (candidate.SkillId == skillId)
                {
                    skill = candidate;
                    return true;
                }
            }
            skill = default;
            return false;
        }

        public Dictionary<uint, PluginSpellInfo> Spells { get; } = [];
        public bool HasComponents { get; set; } = true;
        public IReadOnlyList<PluginEquipmentItem> Equipment { get; set; } = [];
        public uint EquippedObject { get; private set; }
        public PluginCombatMode Mode { get; set; } = PluginCombatMode.Peace;
        public PluginCombatMode RequestedMode { get; private set; }

        public bool IsBusy { get; set; }

        public bool IsKnown(uint spellId) =>
            !UnknownSpells.Contains(spellId)
            && (spellId == 1001 || Spells.ContainsKey(spellId));
        public bool TryGet(uint spellId, out PluginSpellInfo info) =>
            Spells.TryGetValue(spellId, out info);
        bool IMagicCommands.HasComponents(uint spellId) => HasComponents;

        IReadOnlyList<PluginEquipmentItem> IEquipmentAutomation.CaptureOwnedEquipment() =>
            Equipment;
        PluginEquipmentCommandResult IEquipmentAutomation.Equip(
            uint objectId,
            uint requestedLocation)
        {
            EquippedObject = objectId;
            return new PluginEquipmentCommandResult(
                PluginEquipmentCommandStatus.Started);
        }

        PluginCombatSnapshot ICombatAutomation.Snapshot => new(
            0u, Mode, PluginAttackHeight.Medium, 0f, 0f, false, false, false, false);
        IReadOnlyList<PluginCombatTarget> ICombatAutomation.CaptureHostileTargets(
            float maximumDistance) => [];
        PluginCombatCommandResult ICombatAutomation.EnterDefaultMode() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.EnterMode(PluginCombatMode mode)
        {
            RequestedMode = mode;
            return new PluginCombatCommandResult(
                PluginCombatCommandStatus.ModeChangeSent);
        }
        PluginCombatCommandResult ICombatAutomation.BeginPhysicalAttack(
            uint targetObjectId,
            PluginAttackHeight height,
            float power) => new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.ReleasePhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.AbortPhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        public bool TryGetComponent(uint componentId, out PluginSpellComponentInfo info) =>
            Components.TryGetValue(componentId, out info);
        public bool IsCasting => false;
        public PluginCastGate EvaluateGate(uint spellId) =>
            spellId == 1001 ? PluginCastGate.Ready : PluginCastGate.NotKnown;
        public PluginCastGate EvaluateGate(uint spellId, uint targetObjectId) =>
            EvaluateGate(spellId);
        public bool Cast(uint spellId) => Cast(spellId, 0);
        public bool Cast(uint spellId, uint targetObjectId)
        {
            LastCast = (spellId, targetObjectId);
            return CastSent;
        }

        public IReadOnlyList<PluginWorldObject> CaptureObjects() => WorldObjects;
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginWorldObject candidate in WorldObjects)
            {
                if (candidate.ObjectId == objectId)
                {
                    value = candidate;
                    return true;
                }
            }
            value = default;
            return false;
        }
        public bool TryCaptureProperties(uint objectId, out PluginItemProperties value) =>
            Properties.TryGetValue(objectId, out value);

        public PluginItemCommandResult Use(uint objectId)
        {
            UsedObject = objectId;
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        /// <summary>Every move asked for: item, container, amount, slot.</summary>
        public List<(uint Item, uint Container, uint Amount, int Slot)> Moves { get; } = [];

        /// <summary>For each move in <see cref="Moves"/>, whether it asked to join a stack.</summary>
        public List<bool> MoveJoins { get; } = [];

        public PluginItemCommandResult MoveToContainer(
            uint objectId,
            uint containerId,
            uint amount = 0,
            int placement = 0)
            => MoveToContainer(objectId, containerId, amount, placement, joinStack: false);

        public PluginItemCommandResult MoveToContainer(
            uint objectId,
            uint containerId,
            uint amount,
            int placement,
            bool joinStack)
        {
            Moves.Add((objectId, containerId, amount, placement));
            MoveJoins.Add(joinStack);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => InventoryItems;
        public PluginItemCommandResult Salvage(
            uint toolObjectId,
            IReadOnlyList<uint> itemObjectIds)
        {
            LastSalvage = (toolObjectId, itemObjectIds.ToArray());
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        public PluginNavigationSnapshot Snapshot => new(
            true,
            false,
            ObjectId,
            Position,
            false,
            false);
        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            if (TryGet(objectId, out PluginWorldObject obj) && obj.HasPosition)
            {
                value = new PluginNavigationObject(obj.ObjectId, obj.Name, obj.Position);
                return true;
            }
            value = default;
            return false;
        }
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent)
        {
            LastIntent = intent;
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            ClearMovementCount++;
            return PluginNavigationCommandStatus.Accepted;
        }
        public List<string> SystemMessages { get; } = [];
        public List<(string Text, int LogTextType)> Posted { get; } = [];
        public void PostSystemMessage(string text)
        {
            SystemMessages.Add(text);
            Posted.Add((text, 0));
        }
        public void PostMessage(string text, int logTextType) => Posted.Add((text, logTextType));
        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            ChatMessages.Where(message => message.Sequence > afterSequence).ToArray();
        private readonly List<Func<PluginChatMessage, bool>> _chatFilters = [];
        public int ChatFilterCount => _chatFilters.Count;
        public IDisposable RegisterFilter(Func<PluginChatMessage, bool> suppress)
        {
            _chatFilters.Add(suppress);
            return new FilterRegistration(() => _chatFilters.Remove(suppress));
        }

        /// <summary>
        /// Delivers a server line the way the client does: every filter is
        /// asked first, and a line one of them drops never reaches the
        /// transcript. True when the line was shown.
        /// </summary>
        public bool Deliver(string text)
        {
            var message = new PluginChatMessage(
                (ulong)ChatMessages.Count + 1, 0, 0, string.Empty, text, string.Empty);
            if (_chatFilters.ToArray().Any(filter => filter(message)))
                return false;
            ChatMessages.Add(message);
            return true;
        }

        private sealed class FilterRegistration(Action remove) : IDisposable
        {
            public void Dispose() => remove();
        }
        public bool Submit(string text)
        {
            SubmittedChat.Add(text);
            return true;
        }
        public bool CanCompose { get; set; } = true;
        public string ComposedChat { get; private set; } = string.Empty;
        public bool Compose(string text)
        {
            if (!CanCompose)
                return false;
            ComposedChat = text;
            return true;
        }
    }

    private sealed class RecordingUiRegistry : IUiRegistry
    {
        public bool LabelAccepted { get; set; } = true;
        public bool LastVisible { get; private set; }

        public void AddMarkupPanel(string markupPath, object binding) { }
        public bool ViewExists(string viewName) => true;
        public bool IsViewVisible(string viewName) => true;
        public bool ControlExists(string viewName, string controlName) => true;
        public bool SetControlLabel(string viewName, string controlName, string label) =>
            LabelAccepted;
        public bool SetControlVisible(string viewName, string controlName, bool visible)
        {
            LastVisible = visible;
            return true;
        }
    }

    private sealed class Selection : ISelectionService
    {
        public uint? SelectedObjectId { get; private set; }
        public uint? PreviousObjectId { get; private set; }
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId)
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = objectId;
            return true;
        }
        public bool Clear()
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = null;
            return true;
        }
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = [];
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .ToArray();
    }

    /// <summary>
    /// Storage in memory that can be told to fail one kind of operation, the
    /// way a file held by another program or a read-only folder fails.
    /// </summary>
    private sealed class FailingStorage : IPluginStorage
    {
        private readonly MemoryStorage _inner = new();
        private string? _operation;
        private Exception? _failure;

        public void Fail(string operation, Exception failure)
        {
            _operation = operation;
            _failure = failure;
        }

        public bool IsAvailable => true;
        public string? ReadText(string key)
        {
            Check("read");
            return _inner.ReadText(key);
        }
        public void WriteText(string key, string content)
        {
            Check("write");
            _inner.WriteText(key, content);
        }
        public bool Delete(string key)
        {
            Check("delete");
            return _inner.Delete(key);
        }
        public IReadOnlyList<string> List(string prefix)
        {
            Check("list");
            return _inner.List(prefix);
        }
        private void Check(string operation)
        {
            if (operation == _operation && _failure is not null)
                throw _failure;
        }
    }

    /// <summary>
    /// Storage in a real folder, the way the client keeps a plugin's files:
    /// a write goes to a temporary file that then replaces the key's file,
    /// and keys come back relative to the root.
    /// </summary>
    private sealed class DirectoryStorage(string root) : IPluginStorage
    {
        public bool IsAvailable => true;
        public string? RootPath => root;
        public string? ReadText(string key)
        {
            string path = Resolve(key);
            return File.Exists(path) ? File.ReadAllText(path) : null;
        }
        public IReadOnlyList<string> List(string prefix)
        {
            string directory = prefix.Length == 0 ? root : Resolve(prefix);
            return Directory.Exists(directory)
                ? Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories)
                    .Select(path => Path.GetRelativePath(root, path).Replace('\\', '/'))
                    .ToArray()
                : [];
        }
        public void WriteText(string key, string content)
        {
            string path = Resolve(key);
            string directory = Path.GetDirectoryName(path)!;
            Directory.CreateDirectory(directory);
            string temporary = Path.Combine(
                directory,
                $".{Path.GetFileName(path)}.{Guid.NewGuid():N}.tmp");
            try
            {
                File.WriteAllText(temporary, content);
                File.Move(temporary, path, overwrite: true);
            }
            finally
            {
                if (File.Exists(temporary))
                    File.Delete(temporary);
            }
        }
        public bool Delete(string key)
        {
            string path = Resolve(key);
            if (!File.Exists(path))
                return false;
            File.Delete(path);
            return true;
        }
        private string Resolve(string key) =>
            Path.GetFullPath(Path.Combine(root, key.Replace('/', Path.DirectorySeparatorChar)));
    }

    private sealed class TemporaryFolder : IDisposable
    {
        public string Path { get; } = System.IO.Path.Combine(
            System.IO.Path.GetTempPath(),
            "mosstank-gvars-" + Guid.NewGuid().ToString("N"));

        public TemporaryFolder() => Directory.CreateDirectory(Path);

        public void Dispose()
        {
            try
            {
                Directory.Delete(Path, recursive: true);
            }
            catch (IOException)
            {
            }
        }
    }

    private sealed class Logger : IPluginLogger
    {
        public List<string> Errors { get; } = [];
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) => Errors.Add(message);
    }

    private sealed class State : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class Events : IEvents
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
}
