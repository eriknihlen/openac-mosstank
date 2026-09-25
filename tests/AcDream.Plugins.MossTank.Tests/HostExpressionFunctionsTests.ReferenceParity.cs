using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class HostExpressionFunctionsTests
{
    /// <summary>
    /// A bool property reads as the number 1 or 0, never as a boolean: the
    /// reference hands every true/false result back as a number, so it
    /// prints and concatenates as "1"/"0". An appraised value is its 0/1; a
    /// known object without that value in its appraisal, or never appraised,
    /// reads 0. Mutation: answering a boolean prints "True" and "False".
    /// </summary>
    [Fact]
    public void ObjectBoolPropertyIsANumber()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            40, 7010, "Ivory Robe", PluginObjectClass.Clothing, 0x4, 1, 0));
        automation.WorldObjects.Add(new PluginWorldObject(
            41, 7011, "Plain Robe", PluginObjectClass.Clothing, 0x4, 1, 0));
        automation.Properties[40] = new PluginItemProperties(
            new Dictionary<uint, int>(),
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool> { [99] = true, [100] = false },
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        ExpressionValue appraisedTrue = runtime.Evaluate("wobjectgetboolprop[wobjectfindbyid[40],99]");
        Assert.Equal(ExpressionValueKind.Number, appraisedTrue.Kind);
        Assert.Equal("1", appraisedTrue.ToDisplayString());
        ExpressionValue appraisedFalse = runtime.Evaluate("wobjectgetboolprop[wobjectfindbyid[40],100]");
        Assert.Equal(ExpressionValueKind.Number, appraisedFalse.Kind);
        Assert.Equal("0", appraisedFalse.ToDisplayString());
        ExpressionValue missing = runtime.Evaluate("wobjectgetboolprop[wobjectfindbyid[40],24]");
        Assert.Equal(ExpressionValueKind.Number, missing.Kind);
        Assert.Equal(0d, missing.AsNumber());
        ExpressionValue unappraised = runtime.Evaluate("wobjectgetboolprop[wobjectfindbyid[41],99]");
        Assert.Equal(ExpressionValueKind.Number, unappraised.Kind);
        Assert.Equal(0d, unappraised.AsNumber());
        Assert.Equal("ivory 1", runtime.Evaluate(
            "`ivory `+wobjectgetboolprop[wobjectfindbyid[40],99]").AsString());
    }

    /// <summary>
    /// Global variable names are case-sensitive, in the per-expression
    /// record of what was read as in the store: the reference keys the
    /// record with an ordinal dictionary and finds rows with a
    /// case-sensitive comparison. So `R` and `r` are two variables, and one
    /// expression changing both lists writes each back to its own name.
    /// Mutation: a case-insensitive record hands the second read the first
    /// list, so both items land in `L` and `l` stays empty; a
    /// case-insensitive store makes `rare` read `Rare`'s 1.
    /// </summary>
    [Fact]
    public void GlobalVariableNamesAreCaseSensitive()
    {
        var storage = new MemoryStorage();
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));

        runtime.Evaluate("setgvar[`Rare`,1]");
        Assert.False(runtime.Evaluate("testgvar[`rare`]").IsTruthy);
        Assert.Equal(0d, runtime.Evaluate("&rare").AsNumber());
        Assert.False(runtime.Evaluate("cleargvar[`RARE`]").IsTruthy);
        runtime.Evaluate("&rare=2");
        Assert.Equal(1d, runtime.Evaluate("getgvar[`Rare`]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("getgvar[`rare`]").AsNumber());

        runtime.Evaluate("setgvar[`L`,listcreate[]];setgvar[`l`,listcreate[]]");
        runtime.Evaluate("listadd[&L,1];listadd[&l,2]");
        Assert.Equal("[1]", runtime.Evaluate("&L").ToDisplayString());
        Assert.Equal("[2]", runtime.Evaluate("&l").ToDisplayString());
    }

    /// <summary>
    /// Without a shared store the global scope is the state's own table,
    /// and it is case-sensitive the same way. Mutation: a case-insensitive
    /// table makes `A` read `a`'s value.
    /// </summary>
    [Fact]
    public void GlobalVariableNamesAreCaseSensitiveWithoutAStore()
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        runtime.Evaluate("setgvar[`a`,1]");
        Assert.Equal(0d, runtime.Evaluate("&A").AsNumber());
        Assert.False(runtime.Evaluate("testgvar[`A`]").IsTruthy);
        Assert.Equal(1d, runtime.Evaluate("&a").AsNumber());
    }

    /// <summary>
    /// A variable saved before names were case-sensitive (one file per name
    /// whatever its case) is still found under the exact name it was saved
    /// with, and only under that name; setting it moves it to its own file.
    /// Mutation: dropping the old file lookup reads the saved 7 as 0.
    /// </summary>
    [Fact]
    public void AGlobalVariableSavedBeforeCaseSensitivityIsFoundByItsExactName()
    {
        var storage = new MemoryStorage();
        using (new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
        {
        }
        string oldKey = "expressions/global/" + Sha("Coldeve") + "/" + Sha("CARRIED") + ".json";
        storage.Text[oldKey] =
            "{\"Name\":\"Carried\",\"Value\":{\"Kind\":\"number\",\"Number\":7}}";
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));

        Assert.Equal(7d, runtime.Evaluate("getgvar[`Carried`]").AsNumber());
        Assert.True(runtime.Evaluate("testgvar[`Carried`]").IsTruthy);
        Assert.False(runtime.Evaluate("testgvar[`carried`]").IsTruthy);
        Assert.Equal(0d, runtime.Evaluate("&CARRIED").AsNumber());

        runtime.Evaluate("setgvar[`Carried`,8]");
        Assert.False(storage.Text.ContainsKey(oldKey));
        Assert.Equal(8d, runtime.Evaluate("&Carried").AsNumber());
        Assert.True(runtime.Evaluate("cleargvar[`Carried`]").IsTruthy);
        Assert.False(runtime.Evaluate("testgvar[`Carried`]").IsTruthy);
    }

    private static string Sha(string text) => Convert.ToHexString(
        System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(text)).AsSpan(0, 12)).ToLowerInvariant();

    /// <summary>
    /// A code string handed to exec, ifthen or a list function runs as a
    /// run of its own, the way the reference compiles and runs it apart
    /// from the expression that holds it: it keeps its own record of the
    /// global variables it read, reads them fresh from the store, and
    /// writes back the lists it changed when it ends. The outer expression
    /// keeps the copy it read, and its own write-back at the end wins.
    /// Mutation: sharing the outer record makes the inner run see and grow
    /// the outer copy, so the count is 1 and then 2, and the list ends
    /// [1,2].
    /// </summary>
    [Fact]
    public void ACodeStringRunsWithItsOwnGlobalVariableRecord()
    {
        var storage = new MemoryStorage();
        using var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage));
        runtime.Evaluate("setgvar[`L`,listcreate[]]");

        Assert.Equal(0d, runtime.Evaluate(
            "listadd[&L,1];exec[`listcount[&L]`]").AsNumber());
        Assert.Equal("[1]", runtime.Evaluate("&L").ToDisplayString());

        Assert.Equal(2d, runtime.Evaluate(
            "listadd[&L,2];ifthen[1,`listadd[&L,3]`];listcount[&L]").AsNumber());
        Assert.Equal("[1,2]", runtime.Evaluate("&L").ToDisplayString());

        runtime.Evaluate("listmap[listcreate[4,5],`listadd[&L,$1]`]");
        Assert.Equal("[1,2,4,5]", runtime.Evaluate("&L").ToDisplayString());
    }

    /// <summary>
    /// An error while a code string runs ends that run only: it is logged,
    /// the run answers 0, what it did before the error stands, and the
    /// expression holding it carries on. A code string that cannot be read
    /// at all is an error of the holding expression. Mutation: letting the
    /// run's error through fails the whole expression.
    /// </summary>
    [Fact]
    public void AnErrorInACodeStringEndsOnlyThatRun()
    {
        var automation = CreateAutomation();
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("exec[`listcount[5]`]+1").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("ifthen[1,`listcount[5]`]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate(
            "setvar[x,1];exec[`setvar[x,2];listcount[5];setvar[x,3]`];getvar[x]").AsNumber());

        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("exec[`1+`]"));
        Assert.Throws<ExpressionEvaluationException>(() =>
            runtime.Evaluate("listmap[listcreate[1],`1+`]"));
    }

    /// <summary>
    /// What each list function makes of a run that failed, as the reference
    /// does: the failed run answers a whole-number 0 that is not a decimal
    /// number, and its truth test counts anything but a decimal number or
    /// a string as true, so listfilter KEEPS the item; listmap puts 0 in
    /// its place; listreduce carries 0 on. Mutation: treating the failure
    /// as false drops the 2 from the filter.
    /// </summary>
    [Fact]
    public void ListFunctionsTakeAFailedRunAsTheReferenceDoes()
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        // listcount of the number 2 fails; of a list it answers the count.
        Assert.Equal("[2]", runtime.Evaluate(
            "listfilter[listcreate[listcreate[],2,listcreate[]],`listcount[$1]`]").ToDisplayString());
        Assert.Equal("[1,0]", runtime.Evaluate(
            "listmap[listcreate[listcreate[7],2],`listcount[$1]`]").ToDisplayString());
        Assert.Equal(2d, runtime.Evaluate(
            "listreduce[listcreate[listcreate[7],2,listcreate[7,8]],`$2+listcount[$1]`]").AsNumber());
    }

    /// <summary>
    /// The iteration variables as the reference leaves them. listmap and
    /// listreduce put $0, $1 and $2 back after every item, and listfilter
    /// only after an item it keeps; putting back an undefined variable
    /// defines it as 0. listsort sets $1 and $2 for every comparison and
    /// puts nothing back. Mutation: restoring after every item, or clearing
    /// what was undefined, fails the $1 and testvar checks.
    /// </summary>
    [Fact]
    public void ListFunctionsLeaveTheIterationVariablesAsTheReferenceDoes()
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        runtime.Evaluate("listmap[listcreate[5],`$1`]");
        Assert.True(runtime.Evaluate("testvar[`0`]").IsTruthy);
        Assert.True(runtime.Evaluate("testvar[`2`]").IsTruthy);
        Assert.Equal(0d, runtime.Evaluate("getvar[`1`]").AsNumber());

        runtime.Evaluate("setvar[`1`,9]");
        runtime.Evaluate("listfilter[listcreate[1,2],`$1==1`]");
        Assert.Equal(2d, runtime.Evaluate("getvar[`1`]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("getvar[`0`]").AsNumber());
        runtime.Evaluate("setvar[`1`,9]");
        runtime.Evaluate("listfilter[listcreate[1,2],`$1==2`]");
        Assert.Equal(9d, runtime.Evaluate("getvar[`1`]").AsNumber());

        runtime.Evaluate("setvar[`1`,9];setvar[`2`,9]");
        runtime.Evaluate("listreduce[listcreate[1,2],`$2+$1`]");
        Assert.Equal(9d, runtime.Evaluate("getvar[`1`]").AsNumber());
        Assert.Equal(9d, runtime.Evaluate("getvar[`2`]").AsNumber());

        runtime.Evaluate("listsort[listcreate[3,1,2],`$1-$2`]");
        Assert.Equal(2d, runtime.Evaluate("getvar[`1`]").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("getvar[`2`]").AsNumber());
    }

    /// <summary>
    /// listsort reads its comparison as the reference does, rounding the
    /// answer to a whole number (so 0.2 is "equal"), and sorts with the
    /// reference's quicksort, which is not stable: items that compare equal
    /// come out in the order it leaves them. Mutation: an insertion sort
    /// keeps [3,1,2] and [12,11,21,22] as they were among equals.
    /// </summary>
    [Fact]
    public void ListSortComparesAndOrdersAsTheReference()
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        Assert.Equal("[2,1,3]", runtime.Evaluate(
            "listsort[listcreate[3,1,2],`($1-$2)/10`]").ToDisplayString());
        Assert.Equal("[11,12,21,22]", runtime.Evaluate(
            "listsort[listcreate[21,12,22,11],`floor[$1/10]-floor[$2/10]`]").ToDisplayString());
        Assert.Equal("[5]", runtime.Evaluate(
            "setvar[`1`,7];listsort[listcreate[5],`$1-$2`]").ToDisplayString());
        Assert.Equal(7d, runtime.Evaluate("getvar[`1`]").AsNumber());
    }

    /// <summary>
    /// A number handed to a function that takes an object is an id, and an
    /// id the client does not know is an error before the function runs,
    /// worded as the reference words it: the function's parameters, which
    /// argument, and the value. A known id works as the object. Mutation:
    /// answering 0 for an unknown id, as a read of nothing, passes none of
    /// the throws.
    /// </summary>
    [Theory]
    [InlineData("wobjectgetname[12345]", "wobjectgetname[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 12345")]
    [InlineData("wobjectgetintprop[-7,5]", "wobjectgetintprop[WorldObject, number] expects argument #1/2 to be a WorldObject but an invalid (number)id was passed instead. Passed value: -7")]
    [InlineData("actiontrymove[10,999]", "actiontrymove[WorldObject, WorldObject, number, number] expects argument #2/4 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    [InlineData("actiontrycastbyidontarget[1,999]", "actiontrycastbyidontarget[number, WorldObject] expects argument #2/2 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    [InlineData("wobjectisvalid[999]", "wobjectisvalid[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    [InlineData("getfreecontainerslots[999]", "getfreecontainerslots[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    [InlineData("hascorpsebeenopenedbyme[999]", "hascorpsebeenopenedbyme[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    [InlineData("wobjectgethealth[0]", "wobjectgethealth[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 0")]
    [InlineData("ustadd[999]", "ustadd[WorldObject] expects argument #1/1 to be a WorldObject but an invalid (number)id was passed instead. Passed value: 999")]
    public void AnUnknownObjectIdIsAnErrorBeforeTheFunctionRuns(string source, string message)
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate(source));
        Assert.Contains(message, error.Message, StringComparison.Ordinal);
        Assert.Equal("Health Elixir", runtime.Evaluate("wobjectgetname[10]").AsString());
    }

    /// <summary>
    /// An object value whose object the client has since lost passes into
    /// the function, and then it is the function's business: one that reads
    /// the object fails (its name, id, properties, heading, health, spells,
    /// a container's slots or count, selecting or using it), one that only
    /// needs the id carries on (asking for its data, dropping it, the corpse
    /// check), and the validity and type checks answer 0. Mutation: reading
    /// a lost object as 0 passes none of the throws; failing on the id-only
    /// functions fails the rest.
    /// </summary>
    [Fact]
    public void ALostObjectFailsOnlyWhereTheFunctionReadsIt()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            50, 7050, "Old Pack", PluginObjectClass.Container, 0x200, 1, 0));
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        runtime.Evaluate("$o=wobjectfindbyid[50]");
        automation.WorldObjects.RemoveAll(static obj => obj.ObjectId == 50);

        foreach (string source in new[]
        {
            "wobjectgetname[$o]", "wobjectgetid[$o]", "wobjectgetobjectclass[$o]",
            "wobjectgettemplatetype[$o]", "wobjectgetintprop[$o,5]",
            "wobjectgetdoubleprop[$o,5]", "wobjectgetboolprop[$o,5]",
            "wobjectgetstringprop[$o,1]", "wobjecthasdata[$o]", "wobjectlastidtime[$o]",
            "wobjectgetisdooropen[$o]", "wobjectgetphysicscoordinates[$o]",
            "getheading[$o]", "getheadingto[$o]", "wobjectgethealth[$o]",
            "wobjectgetmanavalue[$o]", "wobjectgetspellids[$o]",
            "wobjectgetactivespellids[$o]", "getfreeitemslots[$o]",
            "getcontaineritemcount[$o]", "actiontryselect[$o]", "actiontryuseitem[$o]",
        })
        {
            ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
                () => runtime.Evaluate(source));
            Assert.Contains("Object reference not set to an instance of an object", error.Message, StringComparison.Ordinal);
        }

        Assert.Equal(0d, runtime.Evaluate("wobjectisvalid[$o]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("getobjectinternaltype[$o]").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("hascorpsebeenopenedbyme[$o]").AsNumber());
        Assert.Equal(-1d, runtime.Evaluate("getfreecontainerslots[$o]").AsNumber());
        runtime.Evaluate("wobjectrequestdata[$o]");
        runtime.Evaluate("actiontrydrop[$o]");
        Assert.Equal(7d, runtime.Evaluate("getobjectinternaltype[wobjectgetplayer[]]").AsNumber());
    }

    /// <summary>
    /// The reference turns every true/false into the number 1 or 0 the
    /// moment an expression produces it: a literal, a comparison, a
    /// function's answer. So it prints, concatenates, stores and compares
    /// as a number. Mutation: keeping the boolean prints "True"/"False".
    /// </summary>
    [Theory]
    [InlineData("true", "1")]
    [InlineData("false", "0")]
    [InlineData("1==1", "1")]
    [InlineData("1<0", "0")]
    [InlineData("`a`==`A`", "1")]
    [InlineData("istrue[5]", "1")]
    [InlineData("isfalse[5]", "0")]
    [InlineData("testvar[`nothing here`]", "0")]
    [InlineData("tostring[istrue[1]]", "1")]
    [InlineData("`x`+(2>1)", "x1")]
    [InlineData("`x`+testvar[`nothing here`]", "x0")]
    [InlineData("listcreate[1==1,false]", "[1,0]")]
    [InlineData("$b=istrue[1];$b", "1")]
    [InlineData("wobjecthasdata[wobjectgetplayer[]]", "0")]
    public void TrueAndFalseAreTheNumbersOneAndZero(string source, string expected)
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        ExpressionValue result = runtime.Evaluate(source);

        Assert.Equal(expected, result.ToDisplayString());
        if (result.Kind != ExpressionValueKind.String && result.Kind != ExpressionValueKind.List)
            Assert.Equal(ExpressionValueKind.Number, result.Kind);
    }

    /// <summary>
    /// Session ($) and persistent (@) variable names are case-sensitive, as
    /// the reference's are: its session table and its persistent cache are
    /// ordinal dictionaries and its persistent rows are found by a
    /// case-sensitive comparison. Both names survive a save and a reload.
    /// Mutation: a case-insensitive table makes `$A` read `$a`'s 1.
    /// </summary>
    [Fact]
    public void SessionAndPersistentVariableNamesAreCaseSensitive()
    {
        var storage = new MemoryStorage();
        using (var runtime = new MossTankExpressionRuntime(
            new Host(CreateAutomation(), storage)))
        {
            runtime.Evaluate("$a=1;$A=2");
            Assert.Equal(1d, runtime.Evaluate("$a").AsNumber());
            Assert.Equal(2d, runtime.Evaluate("getvar[`A`]").AsNumber());
            Assert.False(runtime.Evaluate("testvar[`B`]").IsTruthy);
            runtime.Evaluate("setvar[`b`,1]");
            Assert.False(runtime.Evaluate("testvar[`B`]").IsTruthy);
            Assert.False(runtime.Evaluate("clearvar[`B`]").IsTruthy);

            runtime.Evaluate("@p=1;@P=2");
            Assert.Equal(1d, runtime.Evaluate("@p").AsNumber());
            Assert.Equal(2d, runtime.Evaluate("getpvar[`P`]").AsNumber());
            Assert.False(runtime.Evaluate("testpvar[`q`]").IsTruthy);
        }

        using var reloaded = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));
        Assert.Equal(1d, reloaded.Evaluate("@p").AsNumber());
        Assert.Equal(2d, reloaded.Evaluate("@P").AsNumber());
    }

    /// <summary>
    /// A persistent variable saved while names ignored case keeps the exact
    /// name it was saved under, and that is the name that reads it now.
    /// Mutation: folding the saved names' case loses the saved spelling.
    /// </summary>
    [Fact]
    public void APersistentVariableSavedBeforeCaseSensitivityIsFoundByItsExactName()
    {
        var storage = new MemoryStorage();
        using (var first = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage)))
            first.Evaluate("@Route=`north`");
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation(), storage));

        Assert.Equal("north", runtime.Evaluate("@Route").AsString());
        Assert.False(runtime.Evaluate("testpvar[`route`]").IsTruthy);
    }

    /// <summary>
    /// A code string's failed run is reported the reference's way: two
    /// UtilityBelt error lines in chat, the first naming the code as the
    /// reference's parser gives it back (its tokens with no spaces between
    /// them, though a space that ends a name is part of it, then
    /// "&lt;EOF&gt;"), the second the reason. An error raised
    /// inside a function is reported by the wrapper's message the
    /// reference's reflection call puts round it; an error it finds itself,
    /// such as an unknown function, by its own message. Mutation: printing
    /// the source as written, or the inner message, fails the lines.
    /// </summary>
    [Fact]
    public void AFailedCodeStringRunIsReportedAsTheReferenceReportsIt()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            60, 7060, "Gone", PluginObjectClass.Misc, 0x80, 1, 0));
        using var runtime = new MossTankExpressionRuntime(new Host(automation));
        runtime.Evaluate("$o=wobjectfindbyid[60]");
        automation.WorldObjects.RemoveAll(static obj => obj.ObjectId == 60);

        runtime.Evaluate("exec[`wobjectgetname[ $o ]`]");
        runtime.Evaluate("exec[`nosuchfunction[1, 2]`]");

        Assert.Equal(
        [
            ("[UB] Error: Error running string expression: wobjectgetname[$o ]<EOF>", UbChat.ErrorChatType),
            ("[UB] Error: Exception has been thrown by the target of an invocation.", UbChat.ErrorChatType),
            ("[UB] Error: Error running string expression: nosuchfunction[1,2]<EOF>", UbChat.ErrorChatType),
            ("[UB] Error: Unknown expression method: nosuchfunction", UbChat.ErrorChatType),
        ], automation.Posted);
    }

    /// <summary>
    /// The reference's object data fills no bool property from what the
    /// client learns when an object appears: its object description gives
    /// numbers, decimals and text only, and nothing ever sets the two
    /// synthetic bool keys (lockable 0x0C000000, inscribable 0x0C000001),
    /// so they read 0 for every object, appraised or not; every other bool
    /// key reads 0 until an appraisal gives it. Mutation: answering
    /// inscribable from the object's description makes the book read 1.
    /// </summary>
    [Theory]
    [InlineData(0x0C000000)]
    [InlineData(0x0C000001)]
    [InlineData(2)]
    [InlineData(91)]
    public void NoBoolPropertyIsAnsweredWithoutAnAppraisal(int key)
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            70, 7070, "Blank Book", PluginObjectClass.Book, 0x2000, 1, 0)
        {
            IsOwned = true,
        });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(0d, runtime.Evaluate($"wobjectgetboolprop[wobjectfindbyid[70],{key}]").AsNumber());
    }

    /// <summary>
    /// A world object reads as text the reference's way: "0x", its id as
    /// eight upper-case hex digits, ": " and its name, looked up when it is
    /// printed; "(Invalid)" in place of the name once the client has lost
    /// it. That is what printing, concatenating, tostring and a list show,
    /// and a saved object reads the same once loaded. Two objects are equal
    /// when their ids are. Mutation: printing the bare id fails every text
    /// check; comparing only the kind makes the player equal the elixir.
    /// </summary>
    [Fact]
    public void AWorldObjectReadsAsItsHexIdAndName()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            0x800008B5u, 105, "Tome", PluginObjectClass.Book, 0x2, 1, 0));
        var storage = new MemoryStorage();
        using var runtime = new MossTankExpressionRuntime(new Host(automation, storage));

        Assert.Equal("0x00000001: Expression Tester",
            runtime.Evaluate("wobjectgetplayer[]").ToDisplayString());
        Assert.Equal("0x800008B5: Tome",
            runtime.Evaluate("tostring[wobjectfindbyid[-2147481419]]").AsString());
        Assert.Equal("is 0x0000000A: Health Elixir",
            runtime.Evaluate("`is `+wobjectfindbyid[10]").AsString());
        Assert.Equal("[0x0000000A: Health Elixir]",
            runtime.Evaluate("listcreate[wobjectfindbyid[10]]").ToDisplayString());
        Assert.Equal(0d, runtime.Evaluate("wobjectgetplayer[]==wobjectfindbyid[10]").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("wobjectfindbyid[10]==wobjectfindbyid[10]").AsNumber());

        runtime.Evaluate("$tome=wobjectfindbyid[-2147481419];setgvar[`tome`,$tome]");
        automation.WorldObjects.RemoveAll(static obj => obj.ObjectId == 0x800008B5u);
        Assert.Equal("0x800008B5 (Invalid)", runtime.Evaluate("tostring[$tome]").AsString());
        Assert.Equal("0x800008B5 (Invalid)", runtime.Evaluate("tostring[&tome]").AsString());
        automation.WorldObjects.Add(new PluginWorldObject(
            0x800008B5u, 105, "Tome", PluginObjectClass.Book, 0x2, 1, 0));
        Assert.Equal("0x800008B5: Tome", runtime.Evaluate("tostring[&tome]").AsString());
    }

    /// <summary>
    /// Function names are case-sensitive, as the reference's are (its
    /// function table is an ordinal dictionary and every name in it is
    /// lower case): a name in any other case is an unknown function, after
    /// its arguments have run. Mutation: a case-insensitive table runs them.
    /// </summary>
    [Theory]
    [InlineData("ISTRUE[1]", "ISTRUE")]
    [InlineData("Wobjectgetplayer[]", "Wobjectgetplayer")]
    [InlineData("getVar[`x`]", "getVar")]
    public void FunctionNamesAreCaseSensitive(string source, string name)
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate(source));
        Assert.Equal($"Unknown expression method: {name}", error.Reason);
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("Setvar[`y`,1]"));
        Assert.Equal(0d, runtime.Evaluate("testvar[`y`]").AsNumber());
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("Istrue[setvar[`z`,1]]"));
        Assert.Equal(1d, runtime.Evaluate("testvar[`z`]").AsNumber());
    }

    /// <summary>
    /// A function the reference has checks its arguments the reference's
    /// way before it runs, and says so in the reference's words: the
    /// function's declared types, which argument, the declared and the
    /// passed type, and the value; or the count it expects and the count it
    /// got (a parameter the reference fills itself, such as the state
    /// getgvar is handed, counts); or the list error the reference hits when
    /// a world object is missing. Mutation: dropping the check leaves
    /// MossTank's own wording.
    /// </summary>
    [Theory]
    [InlineData("listcount[5]", "listcount[List] expects argument #1/1 to be a List but a number was passed instead. Passed value: 5")]
    [InlineData("abs[`a b`]", "abs[number] expects argument #1/1 to be a number but a string was passed instead. Passed value: a b")]
    [InlineData("setvar[5,1]", "setvar[string, Object] expects argument #1/2 to be a string but a number was passed instead. Passed value: 5")]
    [InlineData("dictgetitem[listcreate[1],`k`]", "dictgetitem[Dictionary, string] expects argument #1/2 to be a Dictionary but a List was passed instead. Passed value: [1]")]
    [InlineData("coordinatetostring[stopwatchcreate[]]", "coordinatetostring[Coordinates] expects argument #1/1 to be a Coordinates but a Stopwatch was passed instead. Passed value: 0")]
    [InlineData("wobjectgetintprop[wobjectgetplayer[],`x`]", "wobjectgetintprop[WorldObject, number] expects argument #2/2 to be a number but a string was passed instead. Passed value: x")]
    [InlineData("wobjectgetname[`x`]", "wobjectgetname[WorldObject] expects argument #1/1 to be a WorldObject but a string was passed instead. Passed value: x")]
    [InlineData("listcount[]", "listcount[List] expects 1 arguments. 0 arguments were passed.")]
    [InlineData("abs[1,2]", "abs[number] expects 1 arguments. 2 arguments were passed.")]
    [InlineData("echo[`x`]", "echo[string, number] expects 2 arguments. 1 arguments were passed.")]
    [InlineData("getgvar[`a`,`b`]", "getgvar[string] expects 1 arguments. 3 arguments were passed.")]
    [InlineData("wobjectgetname[]", "Index was out of range. Must be non-negative and less than the size of the collection.\r\nParameter name: index")]
    public void ArgumentErrorsAreWordedAsTheReferenceWordsThem(string source, string message)
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
            () => runtime.Evaluate(source));
        Assert.Equal(message, error.Reason);
    }

    /// <summary>
    /// What the reference's check lets through: a missing optional argument,
    /// any number of items for a function that takes them all, anything for
    /// a parameter that takes anything, a known object's id for a world
    /// object, and an empty name to getcharacterindex, which names the
    /// character itself.
    /// </summary>
    [Fact]
    public void TheReferenceArgumentCheckLetsTheReferenceFormsThrough()
    {
        using var runtime = new MossTankExpressionRuntime(new Host(CreateAutomation()));

        Assert.Equal(1d, runtime.Evaluate("listpop[listcreate[1]]").AsNumber());
        Assert.Equal("[]", runtime.Evaluate("listcreate[]").ToDisplayString());
        Assert.Equal("[1,a]", runtime.Evaluate("listcreate[1,`a`]").ToDisplayString());
        Assert.Equal("Health Elixir", runtime.Evaluate("wobjectgetname[10]").AsString());
        Assert.Equal(2d, runtime.Evaluate("getcharacterindex[``]").AsNumber());
    }

    /// <summary>
    /// spelldata takes a spell id and answers a dictionary of the spell, as
    /// the reference does: numbers for the ids, counts and flag bits, 1/0
    /// for the flag tests (read off the raw flags), text for the name,
    /// description and school name, and a list of the formula's component
    /// ids. Duration is the spell's own for an enchantment, portal summon or
    /// fellowship enchantment and -1 for anything else. An unknown spell is
    /// an empty dictionary. Mutation: the old two-argument form fails the
    /// argument check.
    /// </summary>
    [Fact]
    public void SpellDataIsADictionaryOfTheSpell()
    {
        var automation = CreateAutomation();
        automation.Spells[2001] = Spell(2001, school: 34, difficulty: 275) with
        {
            Description = "Burns.",
            IconId = 0x06001234u,
            RawFlags = 0x4000u | 0x4u | 0x1u,
            SpellType = 1,
            TargetMask = 0x10u,
            FormulaComponentIds = [1u, 2u, 63u],
            CasterEffect = 0x39u,
            TargetEffect = 0x1Fu,
            FormulaVersion = 2u,
            DisplayOrder = 1417,
            ComponentLoss = 0.25f,
        };
        automation.Spells[2002] = Spell(2002, school: 33, difficulty: 10) with { SpellType = 2 };
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        runtime.Evaluate("$d=spelldata[2001]");
        Assert.Equal("Spell 2001", runtime.Evaluate("$d{`Name`}").AsString());
        Assert.Equal("Burns.", runtime.Evaluate("$d{`Description`}").AsString());
        Assert.Equal("War Magic", runtime.Evaluate("$d{`School`}").AsString());
        Assert.Equal(2001d, runtime.Evaluate("$d{`Id`}").AsNumber());
        Assert.Equal(275d, runtime.Evaluate("$d{`Difficulty`}").AsNumber());
        Assert.Equal(10d, runtime.Evaluate("$d{`Mana`}").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("$d{`Family`}").AsNumber());
        Assert.Equal(30d, runtime.Evaluate("$d{`Duration`}").AsNumber());
        Assert.Equal((double)0x06001234u, runtime.Evaluate("$d{`IconId`}").AsNumber());
        Assert.Equal((double)0x4005, runtime.Evaluate("$d{`Flags`}").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("$d{`Type`}").AsNumber());
        Assert.Equal(16d, runtime.Evaluate("$d{`TargetMask`}").AsNumber());
        Assert.Equal("[1,2,63]", runtime.Evaluate("$d{`ComponentIds`}").ToDisplayString());
        Assert.Equal(1d, runtime.Evaluate("$d{`IsOffensive`}").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("$d{`IsIrresistible`}").AsNumber());
        Assert.Equal(1d, runtime.Evaluate("$d{`IsFastWindup`}").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("$d{`IsUntargetted`}").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("$d{`IsDebuff`}").AsNumber());
        Assert.Equal(0d, runtime.Evaluate("$d{`IsFellowship`}").AsNumber());
        Assert.Equal(57d, runtime.Evaluate("$d{`CasterEffect`}").AsNumber());
        Assert.Equal(31d, runtime.Evaluate("$d{`TargetEffect`}").AsNumber());
        Assert.Equal(2d, runtime.Evaluate("$d{`Generation`}").AsNumber());
        Assert.Equal(1417d, runtime.Evaluate("$d{`SortKey`}").AsNumber());
        Assert.Equal(0.25d, runtime.Evaluate("$d{`Speed`}").AsNumber());
        // Every key the reference answers, and no others.
        Assert.Equal(24d, runtime.Evaluate("dictsize[$d]").AsNumber());

        Assert.Equal(-1d, runtime.Evaluate("spelldata[2002]{`Duration`}").AsNumber());
        Assert.Equal("Life Magic", runtime.Evaluate("spelldata[2002]{`School`}").AsString());
        Assert.Equal(0d, runtime.Evaluate("dictsize[spelldata[9999]]").AsNumber());
        Assert.Throws<ExpressionEvaluationException>(() => runtime.Evaluate("spelldata[2001,`name`]"));
    }

    /// <summary>
    /// actiontrysplit as the reference runs it: nothing and 0 while the
    /// character is busy; otherwise it selects the item, moves the new stack
    /// size of it into the destination (the character when none is given)
    /// at the slot given (0 when none), and answers 1 whatever the server
    /// then does. The fifth argument asks the move to join a stack of the
    /// same thing already in the destination; it is off unless given.
    /// Mutation: ignoring the slot, the merge flag, or answering the host's
    /// verdict, fails.
    /// </summary>
    [Fact]
    public void ActionTrySplitTakesTheReferencesSlotAndMerge()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            80, 300, "Pyreals", PluginObjectClass.Money, 0x40, 1, 0) { IsOwned = true });
        automation.WorldObjects.Add(new PluginWorldObject(
            81, 301, "Pack", PluginObjectClass.Container, 0x200, 1, 0) { IsOwned = true });
        var host = new Host(automation);
        using var runtime = new MossTankExpressionRuntime(host);

        Assert.Equal(1d, runtime.Evaluate("actiontrysplit[80,5,81,3]").AsNumber());
        Assert.Equal((80u, 81u, 5u, 3), automation.Moves[^1]);
        Assert.Equal(80u, host.Selection.SelectedObjectId);
        Assert.Equal(1d, runtime.Evaluate("actiontrysplit[80,7]").AsNumber());
        Assert.Equal((80u, 1u, 7u, 0), automation.Moves[^1]);
        Assert.False(automation.MoveJoins[^1]);
        Assert.Equal(1d, runtime.Evaluate("actiontrysplit[80,2,81,0,0]").AsNumber());
        Assert.False(automation.MoveJoins[^1]);

        Assert.Equal(1d, runtime.Evaluate("actiontrysplit[80,2,81,0,1]").AsNumber());
        Assert.Equal((80u, 81u, 2u, 0), automation.Moves[^1]);
        Assert.True(automation.MoveJoins[^1]);

        automation.IsBusy = true;
        int moves = automation.Moves.Count;
        Assert.Equal(0d, runtime.Evaluate("actiontrysplit[80,5,81,3]").AsNumber());
        Assert.Equal(moves, automation.Moves.Count);
    }

    /// <summary>
    /// actiontrymove as the reference runs it: the whole item into the
    /// destination at the slot given (0 when none), joining a stack of the
    /// same thing already there unless the fourth argument is 0 -- the
    /// reference's addToStack defaults to 1. Mutation: dropping the flag, or
    /// defaulting it off, fails.
    /// </summary>
    [Fact]
    public void ActionTryMoveJoinsAStackUnlessToldNotTo()
    {
        var automation = CreateAutomation();
        automation.WorldObjects.Add(new PluginWorldObject(
            80, 300, "Pyreals", PluginObjectClass.Money, 0x40, 1, 0) { IsOwned = true });
        automation.WorldObjects.Add(new PluginWorldObject(
            81, 301, "Pack", PluginObjectClass.Container, 0x200, 1, 0) { IsOwned = true });
        using var runtime = new MossTankExpressionRuntime(new Host(automation));

        Assert.Equal(1d, runtime.Evaluate("actiontrymove[80,81]").AsNumber());
        Assert.Equal((80u, 81u, 0u, 0), automation.Moves[^1]);
        Assert.True(automation.MoveJoins[^1]);

        Assert.Equal(1d, runtime.Evaluate("actiontrymove[80,81,2]").AsNumber());
        Assert.Equal((80u, 81u, 0u, 2), automation.Moves[^1]);
        Assert.True(automation.MoveJoins[^1]);

        Assert.Equal(1d, runtime.Evaluate("actiontrymove[80,81,3,0]").AsNumber());
        Assert.Equal((80u, 81u, 0u, 3), automation.Moves[^1]);
        Assert.False(automation.MoveJoins[^1]);

        Assert.Equal(1d, runtime.Evaluate("actiontrymove[80,81,3,1]").AsNumber());
        Assert.True(automation.MoveJoins[^1]);
    }
}
