using System.Globalization;
using System.Text.RegularExpressions;

namespace AcDream.Plugins.MossTank.Expressions;

internal static class CoreExpressionFunctions
{
    // The reference's coordinate pattern, searched for anywhere in the text:
    // up to three digits (and up to three decimals) with N or S, optional
    // commas or spaces, the same with an optional E or W, then an optional
    // height ending in Z. The height's "." is any character, as it is there.
    private static readonly Regex CoordinatePattern = new(
        @"(?<NSval>[0-9]{1,3}(?:\.[0-9]{1,3})?)(?<NSchr>(?:[ns]))(?:[,\s]+)?"
        + @"(?<EWval>[0-9]{1,3}(?:\.[0-9]{1,3})?)(?<EWchr>(?:[ew]))?"
        + @"(,?\s*(?<Zval>\-?\d+.?\d+)z)?",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
        TimeSpan.FromMilliseconds(100));

    /// <param name="random">The random source; the shared one when null.</param>
    /// <param name="isKnownObject">
    /// Whether the client still knows an object, for the one core function
    /// that tells a lost object apart; null takes every object as known.
    /// </param>
    /// <param name="writeToChat">
    /// Prints a line the reference writes to chat itself (a date format it
    /// cannot use, say); null prints nothing.
    /// </param>
    public static ExpressionFunctionRegistry CreateDefault(
        Random? random = null,
        Func<uint, bool>? isKnownObject = null,
        Action<string>? writeToChat = null)
    {
        var registry = new ExpressionFunctionRegistry();
        Register(registry, random, isKnownObject, writeToChat);
        return registry;
    }

    public static void Register(
        ExpressionFunctionRegistry registry,
        Random? random = null,
        Func<uint, bool>? isKnownObject = null,
        Action<string>? writeToChat = null)
    {
        ArgumentNullException.ThrowIfNull(registry);
        RegisterVariables(registry, ExpressionVariableScope.Session, string.Empty);
        RegisterVariables(registry, ExpressionVariableScope.Persistent, "p");
        RegisterVariables(registry, ExpressionVariableScope.Global, "g");
        RegisterConversionsAndMath(registry, random ?? Random.Shared, isKnownObject);
        RegisterLists(registry);
        RegisterDictionaries(registry);
        RegisterCoordinates(registry);
        RegisterTime(registry, writeToChat);
    }

    private static void RegisterVariables(
        ExpressionFunctionRegistry registry,
        ExpressionVariableScope scope,
        string infix)
    {
        string get = "get" + infix + "var";
        string set = "set" + infix + "var";
        string test = "test" + infix + "var";
        string touch = "touch" + infix + "var";
        string clear = "clear" + infix + "var";
        string clearAll = "clearall" + infix + "vars";

        registry.Register(get, 1, 1, (context, args) =>
            context.State.Get(scope, args[0].AsString(get)), $"{get}[name]");
        registry.Register(set, 2, 2, (context, args) =>
            context.State.Set(scope, args[0].AsString(set), args[1]),
            $"{set}[name,value]");
        registry.Register(test, 1, 1, (context, args) =>
            ExpressionValue.Boolean(context.State.Contains(
                scope,
                args[0].AsString(test))), $"{test}[name]");
        registry.Register(touch, 1, 1, (context, args) =>
        {
            string name = args[0].AsString(touch);
            bool existed = context.State.Contains(scope, name);
            if (!existed)
                context.State.Set(scope, name, ExpressionValue.Zero);
            return ExpressionValue.Boolean(existed);
        }, $"{touch}[name]");
        registry.Register(clear, 1, 1, (context, args) =>
            ExpressionValue.Boolean(context.State.Clear(
                scope,
                args[0].AsString(clear))), $"{clear}[name]");
        registry.Register(clearAll, 0, 0, (context, _) =>
        {
            context.State.Clear(scope);
            return ExpressionValue.One;
        }, $"{clearAll}[]");
    }

    private static void RegisterConversionsAndMath(
        ExpressionFunctionRegistry registry,
        Random random,
        Func<uint, bool>? isKnownObject)
    {
        RegisterUnaryMath(registry, "abs", Math.Abs);
        RegisterUnaryMath(registry, "acos", Math.Acos);
        RegisterUnaryMath(registry, "asin", Math.Asin);
        RegisterUnaryMath(registry, "atan", Math.Atan);
        RegisterUnaryMath(registry, "ceiling", Math.Ceiling);
        RegisterUnaryMath(registry, "cos", Math.Cos);
        RegisterUnaryMath(registry, "cosh", Math.Cosh);
        RegisterUnaryMath(registry, "floor", Math.Floor);
        RegisterUnaryMath(registry, "round", Math.Round);
        RegisterUnaryMath(registry, "sin", Math.Sin);
        RegisterUnaryMath(registry, "sinh", Math.Sinh);
        RegisterUnaryMath(registry, "sqrt", Math.Sqrt);
        RegisterUnaryMath(registry, "tan", Math.Tan);
        RegisterUnaryMath(registry, "tanh", Math.Tanh);
        registry.Register("atan2", 2, 2, (_, args) => ExpressionValue.Number(
            Math.Atan2(args[0].AsNumber("atan2"), args[1].AsNumber("atan2"))),
            "atan2[y,x]");
        registry.Register("chr", 1, 1, (_, args) => ExpressionValue.String(
            char.ConvertFromUtf32(checked((int)args[0].AsNumber("chr")))),
            "chr[codepoint]");
        registry.Register("ord", 1, 1, (_, args) =>
        {
            string value = args[0].AsString("ord");
            if (value.Length == 0)
                throw new ExpressionEvaluationException("ord expects a non-empty string");
            return ExpressionValue.Number(char.ConvertToUtf32(value, 0));
        }, "ord[text]");
        // Group separators are accepted, and an unparsable string is 0 rather
        // than an error. The culture is pinned so a profile reads the same on
        // every machine.
        registry.Register("cnumber", 1, 1, (_, args) =>
            double.TryParse(
                args[0].AsString("cnumber"),
                NumberStyles.Float | NumberStyles.AllowThousands,
                CultureInfo.InvariantCulture,
                out double result)
                    ? ExpressionValue.Number(result)
                    : ExpressionValue.Zero,
            "cnumber[text]");
        // Type introspection over ANY value: it answers with the expression
        // token's own type tag, not with a game item type.
        // An object the client has lost answers 0, as nothing does.
        registry.Register("getobjectinternaltype", 1, 1, (_, args) =>
            ExpressionValue.Number(
                args[0].Kind == ExpressionValueKind.WorldObject
                && isKnownObject?.Invoke(args[0].AsObjectId()) == false
                    ? 0d
                    : InternalTypeTag(args[0].Kind)),
            "getobjectinternaltype[value]");
        registry.Register("cstr", 1, 1, (_, args) => ExpressionValue.String(
            args[0].AsNumber("cstr").ToString("G15", CultureInfo.InvariantCulture)),
            "cstr[number]");
        registry.Register("cstrf", 2, 2, (_, args) =>
        {
            double number = args[0].AsNumber("cstrf");
            string format = args[1].AsString("cstrf");
            return ExpressionValue.String(
                IsStandardHexFormat(format)
                    ? WrapToUInt32(number).ToString(format, CultureInfo.InvariantCulture)
                    : number.ToString(format, CultureInfo.InvariantCulture));
        }, "cstrf[number,format]");
        registry.Register("hexstr", 1, 1, (_, args) => ExpressionValue.String(
            $"0x{checked((int)args[0].AsNumber("hexstr")):X}"),
            "hexstr[number]");
        registry.Register("strlen", 1, 1, (_, args) => ExpressionValue.Number(
            args[0].AsString("strlen").Length), "strlen[text]");
        registry.Register("tostring", 1, 1, (_, args) =>
            ExpressionValue.String(args[0].ToDisplayString()), "tostring[value]");
        registry.Register("istrue", 1, 1, (_, args) =>
            ExpressionValue.Boolean(args[0].IsTruthy), "istrue[value]");
        registry.Register("isfalse", 1, 1, (_, args) =>
            ExpressionValue.Boolean(!args[0].IsTruthy), "isfalse[value]");
        registry.Register("iif", 3, 3, (_, args) =>
            args[0].IsTruthy ? args[1] : args[2], "iif[test,trueValue,falseValue]");
        registry.Register("ifthen", 2, 3, (context, args) =>
        {
            string? source = args[0].IsTruthy
                ? args[1].AsString("ifthen")
                : args.Count == 3
                    ? args[2].AsString("ifthen")
                    : null;
            return source is null
                ? ExpressionValue.Zero
                : context.RunSeparately(context.Compile(source), out _);
        }, "ifthen[test,trueExpression,falseExpression?]");
        registry.Register("randint", 2, 2, (_, args) =>
        {
            int minimum = checked((int)args[0].AsNumber("randint"));
            int maximum = checked((int)args[1].AsNumber("randint"));
            return ExpressionValue.Number(random.Next(minimum, maximum));
        }, "randint[min,maxExclusive]");
        registry.Register("getregexmatch", 2, 2, (_, args) =>
        {
            var regex = new Regex(
                args[1].AsString("getregexmatch"),
                RegexOptions.CultureInvariant,
                TimeSpan.FromMilliseconds(100));
            Match match = regex.Match(args[0].AsString("getregexmatch"));
            return match.Success
                ? ExpressionValue.String(match.Value)
                : ExpressionValue.Zero;
        }, "getregexmatch[text,pattern]");
    }

    /// <summary>
    /// The type tags an expression exposes: 0 none, 1 number, 3 string,
    /// 7 object. Booleans ride a number, so they report 1.
    /// </summary>
    private static double InternalTypeTag(ExpressionValueKind kind) => kind switch
    {
        ExpressionValueKind.Number or ExpressionValueKind.Boolean => 1d,
        ExpressionValueKind.String => 3d,
        _ => 7d,
    };

    private static void RegisterUnaryMath(
        ExpressionFunctionRegistry registry,
        string name,
        Func<double, double> operation) =>
        registry.Register(name, 1, 1, (_, args) => ExpressionValue.Number(
            operation(args[0].AsNumber(name))), $"{name}[number]");

    private static void RegisterLists(ExpressionFunctionRegistry registry)
    {
        registry.Register("listcreate", 0, int.MaxValue, (_, args) =>
            ExpressionValue.List(new ExpressionList(args)), "listcreate[items...]");
        registry.Register("listadd", 2, 2, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listadd");
            GuardNoCycle(list, args[1], "listadd");
            list.Items.Add(args[1]);
            list.MarkChanged();
            return args[0];
        }, "listadd[list,item]");
        registry.Register("listinsert", 3, 3, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listinsert");
            GuardNoCycle(list, args[1], "listinsert");
            int index = ToTruncatedInt(args[2], "listinsert");
            if ((uint)index > (uint)list.Items.Count)
                throw BadIndex("insert", index, list.Items.Count, allowEnd: true);
            list.Items.Insert(index, args[1]);
            list.MarkChanged();
            return args[0];
        }, "listinsert[list,item,index]");
        registry.Register("listremove", 2, 2, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listremove");
            if (list.Items.Remove(args[1]))
                list.MarkChanged();
            return args[0];
        }, "listremove[list,item]");
        registry.Register("listremoveat", 2, 2, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listremoveat");
            int index = RequireListIndex(list, args[1], "listremoveat");
            list.Items.RemoveAt(index);
            list.MarkChanged();
            return args[0];
        }, "listremoveat[list,index]");
        registry.Register("listgetitem", 2, 2, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listgetitem");
            return list.Items[RequireListIndex(list, args[1], "listgetitem")];
        }, "listgetitem[list,index]");
        registry.Register("listcontains", 2, 2, (_, args) =>
            ExpressionValue.Boolean(args[0].AsList("listcontains").Items.Contains(args[1])),
            "listcontains[list,item]");
        registry.Register("listindexof", 2, 2, (_, args) => ExpressionValue.Number(
            args[0].AsList("listindexof").Items.IndexOf(args[1])),
            "listindexof[list,item]");
        registry.Register("listlastindexof", 2, 2, (_, args) =>
            ExpressionValue.Number(args[0].AsList("listlastindexof")
                .Items.LastIndexOf(args[1])), "listlastindexof[list,item]");
        registry.Register("listcopy", 1, 1, (_, args) => ExpressionValue.List(
            new ExpressionList(args[0].AsList("listcopy").Items)), "listcopy[list]");
        registry.Register("listreverse", 1, 1, (_, args) =>
        {
            var values = args[0].AsList("listreverse").Items.ToArray();
            Array.Reverse(values);
            return ExpressionValue.List(new ExpressionList(values));
        }, "listreverse[list]");
        registry.Register("listpop", 1, 2, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listpop");
            int index = args.Count == 1 || args[1].AsNumber("listpop") == -1d
                ? list.Items.Count - 1
                : RequireListIndex(list, args[1], "listpop");
            if (index < 0)
                throw BadIndex("pop", index, list.Items.Count, allowEnd: false);
            ExpressionValue result = list.Items[index];
            list.Items.RemoveAt(index);
            list.MarkChanged();
            return result;
        }, "listpop[list,index?]");
        registry.Register("listcount", 1, 1, (_, args) => ExpressionValue.Number(
            args[0].AsList("listcount").Items.Count), "listcount[list]");
        registry.Register("listclear", 1, 1, (_, args) =>
        {
            ExpressionList list = args[0].AsList("listclear");
            list.Items.Clear();
            list.MarkChanged();
            return args[0];
        }, "listclear[list]");
        // The list functions run their code string once per item (per
        // comparison for listsort), each time as a run of its own; see
        // ExpressionEvaluationContext.RunSeparately. $0 is the item's index
        // and $1 the item; what they held before is saved first and put
        // back as each function of the reference puts it back.
        registry.Register("listfilter", 2, 2, (context, args) =>
        {
            ExpressionList source = args[0].AsList("listfilter");
            string code = args[1].AsString("listfilter");
            ExpressionProgram program = context.Compile(code);
            var result = new ExpressionList();
            ExpressionValue[] saved = SaveIterationVariables(context.State);
            for (int index = 0; index < source.Items.Count; index++)
            {
                ExpressionValue item = source.Items[index];
                SetIteration(context.State, index, item);
                ExpressionValue answer = context.RunSeparately(program, out bool failed);
                // A failed run answers a whole-number 0, which the
                // reference's truth test counts as true (only a decimal
                // number or a string can be false), so the item is kept.
                // The variables go back only after an item that is kept.
                if (failed || answer.IsTruthy)
                {
                    result.Items.Add(item);
                    RestoreIterationVariables(context.State, saved);
                }
            }
            return ExpressionValue.List(result);
        }, "listfilter[list,expression]");
        registry.Register("listmap", 2, 2, (context, args) =>
        {
            ExpressionList source = args[0].AsList("listmap");
            string code = args[1].AsString("listmap");
            ExpressionProgram program = context.Compile(code);
            var result = new ExpressionList();
            ExpressionValue[] saved = SaveIterationVariables(context.State);
            for (int index = 0; index < source.Items.Count; index++)
            {
                SetIteration(context.State, index, source.Items[index]);
                result.Items.Add(context.RunSeparately(program, out _));
                RestoreIterationVariables(context.State, saved);
            }
            return ExpressionValue.List(result);
        }, "listmap[list,expression]");
        registry.Register("listreduce", 2, 2, (context, args) =>
        {
            ExpressionList source = args[0].AsList("listreduce");
            string code = args[1].AsString("listreduce");
            ExpressionProgram program = context.Compile(code);
            ExpressionValue result = ExpressionValue.Zero;
            ExpressionValue[] saved = SaveIterationVariables(context.State);
            for (int index = 0; index < source.Items.Count; index++)
            {
                SetIteration(context.State, index, source.Items[index], result);
                result = context.RunSeparately(program, out _);
                RestoreIterationVariables(context.State, saved);
            }
            return result;
        }, "listreduce[list,expression]");
        // $1 and $2 are the two items being compared and stay set to the
        // last pair afterwards. The answer is rounded to a whole number, so
        // anything between -0.5 and 0.5 means "equal".
        registry.Register("listsort", 1, 2, (context, args) =>
        {
            var result = new ExpressionList(args[0].AsList("listsort").Items);
            if (args.Count == 1 || args[1].AsString("listsort").Length == 0)
            {
                ExpressionValue[] sorted = result.Items.ToArray();
                ReferenceSort.Sort(sorted, DefaultComparison);
                return ExpressionValue.List(new ExpressionList(sorted));
            }

            string code = args[1].AsString("listsort");
            ExpressionProgram program = context.Compile(code);
            ExpressionValue[] items = result.Items.ToArray();
            ReferenceSort.Sort(items, (left, right) =>
            {
                context.State.Set(ExpressionVariableScope.Session, "1", left);
                context.State.Set(ExpressionVariableScope.Session, "2", right);
                return ComparisonAnswer(context.RunSeparately(program, out _));
            });
            result.Items.Clear();
            result.Items.AddRange(items);
            return ExpressionValue.List(result);
        }, "listsort[list,expression?]");
        // The reference counts up from the start to the end, both included;
        // a start past the end gives an empty list, not a count down.
        registry.Register("listfromrange", 2, 2, (_, args) =>
        {
            int start = ToTruncatedInt(args[0], "listfromrange");
            int end = ToTruncatedInt(args[1], "listfromrange");
            var result = new ExpressionList();
            if (start > end)
                return ExpressionValue.List(result);
            if ((long)end - start + 1 > 100_000)
            {
                throw new ExpressionEvaluationException(
                    "listfromrange is limited to 100000 entries");
            }
            for (long value = start; value <= end; value++)
                result.Items.Add(ExpressionValue.Number(value));
            return ExpressionValue.List(result);
        }, "listfromrange[start,end]");
    }

    private static void RegisterDictionaries(ExpressionFunctionRegistry registry)
    {
        registry.Register("dictcreate", 0, int.MaxValue, (_, args) =>
        {
            if ((args.Count & 1) != 0)
                throw new ExpressionEvaluationException(
                    "dictcreate expects key/value pairs");
            var dictionary = new ExpressionDictionary();
            for (int index = 0; index < args.Count; index += 2)
            {
                string key = args[index].AsString("dictcreate key");
                if (!dictionary.Items.TryAdd(key, args[index + 1]))
                {
                    throw new ExpressionEvaluationException(
                        $"dictcreate received duplicate key '{key}'");
                }
            }
            return ExpressionValue.Dictionary(dictionary);
        }, "dictcreate[key,value,...]");
        registry.Register("dictgetitem", 2, 2, (_, args) =>
        {
            ExpressionDictionary dictionary = args[0].AsDictionary("dictgetitem");
            string key = args[1].AsString("dictgetitem key");
            if (!dictionary.Items.TryGetValue(key, out ExpressionValue value))
                throw new ExpressionEvaluationException($"Dictionary key '{key}' was not found");
            return value;
        }, "dictgetitem[dictionary,key]");
        registry.Register("dictadditem", 3, 3, (_, args) =>
        {
            ExpressionDictionary dictionary = args[0].AsDictionary("dictadditem");
            string key = args[1].AsString("dictadditem key");
            GuardNoCycle(dictionary, args[2], "dictadditem");
            bool replaced = dictionary.Items.ContainsKey(key);
            dictionary.Items[key] = args[2];
            dictionary.MarkChanged();
            return ExpressionValue.Boolean(replaced);
        }, "dictadditem[dictionary,key,value]");
        registry.Register("dicthaskey", 2, 2, (_, args) => ExpressionValue.Boolean(
            args[0].AsDictionary("dicthaskey").Items.ContainsKey(
                args[1].AsString("dicthaskey key"))), "dicthaskey[dictionary,key]");
        registry.Register("dictremovekey", 2, 2, (_, args) =>
        {
            ExpressionDictionary dictionary = args[0].AsDictionary("dictremovekey");
            bool removed = dictionary.Items.Remove(args[1].AsString("dictremovekey key"));
            if (removed)
                dictionary.MarkChanged();
            return ExpressionValue.Boolean(removed);
        }, "dictremovekey[dictionary,key]");
        registry.Register("dictkeys", 1, 1, (_, args) => ExpressionValue.List(
            new ExpressionList(args[0].AsDictionary("dictkeys").Items.Keys.Select(
                ExpressionValue.String))), "dictkeys[dictionary]");
        registry.Register("dictvalues", 1, 1, (_, args) => ExpressionValue.List(
            new ExpressionList(args[0].AsDictionary("dictvalues").Items.Values)),
            "dictvalues[dictionary]");
        registry.Register("dictsize", 1, 1, (_, args) => ExpressionValue.Number(
            args[0].AsDictionary("dictsize").Items.Count), "dictsize[dictionary]");
        registry.Register("dictclear", 1, 1, (_, args) =>
        {
            // The reference removes the keys one by one, so an empty
            // dictionary sees no removal and is not changed.
            ExpressionDictionary dictionary = args[0].AsDictionary("dictclear");
            if (dictionary.Items.Count != 0)
            {
                dictionary.Items.Clear();
                dictionary.MarkChanged();
            }
            return args[0];
        }, "dictclear[dictionary]");
        registry.Register("dictcopy", 1, 1, (_, args) =>
        {
            var result = new ExpressionDictionary();
            foreach ((string key, ExpressionValue value) in
                args[0].AsDictionary("dictcopy").Items)
            {
                result.Items[key] = value;
            }
            return ExpressionValue.Dictionary(result);
        }, "dictcopy[dictionary]");
    }

    private static void RegisterCoordinates(ExpressionFunctionRegistry registry)
    {
        // Text with no coordinate in it reads as 0N, 0E rather than failing.
        // The height is taken as a raw height, so its reading (a raw height
        // over 240) is the written number over 240.
        registry.Register("coordinateparse", 1, 1, (_, args) =>
        {
            string source = args[0].AsString("coordinateparse");
            Match match = CoordinatePattern.Match(source);
            if (!match.Success)
                return ExpressionValue.Coordinates(new ExpressionCoordinates(0d, 0d));
            NumberFormatInfo format = CultureInfo.InvariantCulture.NumberFormat;
            double northSouth = double.Parse(match.Groups["NSval"].Value, format);
            if (!match.Groups["NSchr"].Value.Equals("n", StringComparison.OrdinalIgnoreCase))
                northSouth = -northSouth;
            double eastWest = double.Parse(match.Groups["EWval"].Value, format);
            string eastWestLetter = match.Groups["EWchr"].Value;
            if (eastWestLetter.Length != 0
                && !eastWestLetter.Equals("e", StringComparison.OrdinalIgnoreCase))
            {
                eastWest = -eastWest;
            }
            string height = match.Groups["Zval"].Value;
            double elevation = height.Length != 0
                ? double.Parse(height, format) / 240d
                : 0d;
            return ExpressionValue.Coordinates(new ExpressionCoordinates(
                eastWest,
                northSouth,
                elevation));
        }, "coordinateparse[text]");
        registry.Register("coordinategetns", 1, 1, (_, args) =>
            ExpressionValue.Number(args[0].AsCoordinates("coordinategetns").NorthSouth),
            "coordinategetns[coordinates]");
        registry.Register("coordinategetwe", 1, 1, (_, args) =>
            ExpressionValue.Number(args[0].AsCoordinates("coordinategetwe").EastWest),
            "coordinategetwe[coordinates]");
        registry.Register("coordinategetz", 1, 1, (_, args) =>
            ExpressionValue.Number(args[0].AsCoordinates("coordinategetz").Elevation),
            "coordinategetz[coordinates]");
        registry.Register("coordinatetostring", 1, 1, (_, args) =>
            ExpressionValue.String(args[0].AsCoordinates("coordinatetostring").ToString()),
            "coordinatetostring[coordinates]");
        registry.Register("coordinatedistanceflat", 2, 2, (_, args) =>
            ExpressionValue.Number(CoordinateDistance(args[0], args[1], includeElevation: false)),
            "coordinatedistanceflat[first,second]");
        registry.Register("coordinatedistancewithz", 2, 2, (_, args) =>
            ExpressionValue.Number(CoordinateDistance(args[0], args[1], includeElevation: true)),
            "coordinatedistancewithz[first,second]");
    }

    private static void RegisterTime(
        ExpressionFunctionRegistry registry,
        Action<string>? writeToChat)
    {
        // A format the date cannot be written in is not an error: the
        // reference prints why to chat and answers the empty string.
        string DateText(DateTime time, string format)
        {
            try
            {
                return time.ToString(format, CultureInfo.InvariantCulture);
            }
            catch (FormatException error)
            {
                writeToChat?.Invoke(error.Message);
                return string.Empty;
            }
        }
        registry.Register("getdatetimelocal", 0, 1, (_, args) => ExpressionValue.String(
            DateText(
                DateTime.Now,
                args.Count == 0 ? "hh:mm:ss tt" : args[0].AsString("getdatetimelocal"))),
            "getdatetimelocal[format?]");
        registry.Register("getdatetimeutc", 0, 1, (_, args) => ExpressionValue.String(
            DateText(
                DateTime.UtcNow,
                args.Count == 0 ? "hh:mm:ss tt" : args[0].AsString("getdatetimeutc"))),
            "getdatetimeutc[format?]");
        registry.Register("getunixtime", 0, 0, (_, _) => ExpressionValue.Number(
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000d), "getunixtime[]");
        registry.Register("stopwatchcreate", 0, 0, (_, _) =>
            ExpressionValue.Stopwatch(new ExpressionStopwatch()), "stopwatchcreate[]");
        registry.Register("stopwatchstart", 1, 1, (_, args) =>
        {
            args[0].AsStopwatch("stopwatchstart").Start();
            return args[0];
        }, "stopwatchstart[stopwatch]");
        registry.Register("stopwatchstop", 1, 1, (_, args) =>
        {
            args[0].AsStopwatch("stopwatchstop").Stop();
            return args[0];
        }, "stopwatchstop[stopwatch]");
        registry.Register("stopwatchelapsedseconds", 1, 1, (_, args) =>
            ExpressionValue.Number(args[0].AsStopwatch(
                "stopwatchelapsedseconds").ElapsedSeconds),
            "stopwatchelapsedseconds[stopwatch]");
    }

    private static double CoordinateDistance(
        in ExpressionValue first,
        in ExpressionValue second,
        bool includeElevation)
    {
        ExpressionCoordinates left = first.AsCoordinates("coordinate distance");
        ExpressionCoordinates right = second.AsCoordinates("coordinate distance");
        double eastWest = (left.EastWest - right.EastWest) * 240d;
        double northSouth = (left.NorthSouth - right.NorthSouth) * 240d;
        double elevation = includeElevation
            ? (left.Elevation - right.Elevation) * 240d
            : 0d;
        return Math.Sqrt(
            eastWest * eastWest
            + northSouth * northSouth
            + elevation * elevation);
    }

    private static int RequireListIndex(
        ExpressionList list,
        in ExpressionValue value,
        string operation)
    {
        int index = ToTruncatedInt(value, operation);
        if ((uint)index >= (uint)list.Items.Count)
            throw BadIndex(operation, index, list.Items.Count, allowEnd: false);
        return index;
    }

    /// <summary>
    /// A whole-string hex specifier — "X", "x", "X4" and so on, nothing else.
    /// `cstrf` renders through an unsigned integer for those and through the
    /// number itself for every other format, so a custom format that merely
    /// contains a literal x ("0.0 x") keeps its fractional digits.
    /// </summary>
    private static bool IsStandardHexFormat(string format)
    {
        if (format.Length == 0 || format[0] is not ('X' or 'x'))
            return false;
        for (int index = 1; index < format.Length; index++)
        {
            if (!char.IsAsciiDigit(format[index]))
                return false;
        }
        return true;
    }

    /// <summary>
    /// The number as the 32 bits a hex format shows, the way the reference
    /// converts it: truncated toward zero through a 64-bit integer and cut to
    /// the low 32 bits, so a signed object id such as -2147481419 shows as
    /// 800008B5 and -1 as FFFFFFFF. A value with no 64-bit integer form
    /// (NaN, or 2^63 and beyond either way) comes out as 0.
    /// </summary>
    private static uint WrapToUInt32(double number) =>
        number is >= -9223372036854775808d and < 9223372036854775808d
            ? unchecked((uint)(long)number)
            : 0u;

    private static int ToTruncatedInt(in ExpressionValue value, string operation) =>
        checked((int)value.AsNumber(operation));

    private static ExpressionEvaluationException BadIndex(
        string operation,
        int index,
        int count,
        bool allowEnd) => new(
        $"Unable to {operation} index {index}; valid range is 0.."
        + (allowEnd ? count : count - 1));

    private static void SetIteration(
        ExpressionState state,
        int index,
        in ExpressionValue item,
        ExpressionValue? accumulator = null)
    {
        state.Set(ExpressionVariableScope.Session, "0", ExpressionValue.Number(index));
        state.Set(ExpressionVariableScope.Session, "1", item);
        if (accumulator is { } value)
            state.Set(ExpressionVariableScope.Session, "2", value);
    }

    private static readonly string[] IterationVariables = ["0", "1", "2"];

    /// <summary>
    /// $0, $1 and $2 as they are before a list function starts; an
    /// undefined one reads 0.
    /// </summary>
    private static ExpressionValue[] SaveIterationVariables(ExpressionState state) =>
        IterationVariables
            .Select(name => state.Get(ExpressionVariableScope.Session, name))
            .ToArray();

    /// <summary>
    /// Sets $0, $1 and $2 back to what <see cref="SaveIterationVariables"/>
    /// read. One that was undefined is set to 0, not cleared, as the
    /// reference sets it.
    /// </summary>
    private static void RestoreIterationVariables(
        ExpressionState state,
        ExpressionValue[] saved)
    {
        for (int index = 0; index < IterationVariables.Length; index++)
            state.Set(ExpressionVariableScope.Session, IterationVariables[index], saved[index]);
    }

    /// <summary>
    /// A listsort comparison's answer as a whole number, converted the way
    /// the reference converts it: a number rounds to the nearest whole
    /// number (a half to the even one) and must fit in 32 bits, a string
    /// must be a whole number written out, and anything else cannot be
    /// converted.
    /// </summary>
    private static int ComparisonAnswer(in ExpressionValue answer) => answer.Kind switch
    {
        ExpressionValueKind.Number or ExpressionValueKind.Boolean =>
            Convert.ToInt32(answer.AsNumber(), CultureInfo.InvariantCulture),
        ExpressionValueKind.String => int.Parse(
            answer.AsString(),
            NumberStyles.Integer,
            CultureInfo.InvariantCulture),
        _ => throw new InvalidCastException(
            $"A {answer.Kind} cannot be read as a comparison result."),
    };

    private static void GuardNoCycle(object destination, in ExpressionValue value, string operation)
    {
        if (ContainsReference(value, destination, new HashSet<object>(
                ReferenceEqualityComparer.Instance)))
        {
            throw new ExpressionEvaluationException(
                $"{operation} cannot create a cyclic collection");
        }
    }

    private static bool ContainsReference(
        in ExpressionValue value,
        object destination,
        HashSet<object> visited)
    {
        if (value.Kind == ExpressionValueKind.List)
        {
            ExpressionList list = value.AsList();
            if (ReferenceEquals(list, destination))
                return true;
            return visited.Add(list)
                && list.Items.Any(item => ContainsReference(item, destination, visited));
        }
        if (value.Kind == ExpressionValueKind.Dictionary)
        {
            ExpressionDictionary dictionary = value.AsDictionary();
            if (ReferenceEquals(dictionary, destination))
                return true;
            return visited.Add(dictionary)
                && dictionary.Items.Values.Any(item =>
                    ContainsReference(item, destination, visited));
        }
        return false;
    }

    /// <summary>
    /// listsort's order without an expression, as the reference's runtime
    /// orders two values with its default comparer: numbers by value,
    /// strings by the culture's word order (case counts, lower case first),
    /// a value as equal to itself. Anything else (a number against a string,
    /// or two lists) cannot be compared, and the sort fails.
    /// </summary>
    private static int DefaultComparison(ExpressionValue left, ExpressionValue right)
    {
        if (left.Kind == ExpressionValueKind.String && right.Kind == ExpressionValueKind.String)
            return StringComparer.InvariantCulture.Compare(left.AsString(), right.AsString());
        if (left.Kind == right.Kind
            && left.Kind is ExpressionValueKind.Number or ExpressionValueKind.Boolean)
        {
            return left.AsNumber().CompareTo(right.AsNumber());
        }
        // A list, dictionary, stopwatch, control or coordinates is equal to
        // itself only; a world object is made afresh each time it is read,
        // so two are never the same one.
        if (left.Kind is ExpressionValueKind.List or ExpressionValueKind.Dictionary
                or ExpressionValueKind.Stopwatch or ExpressionValueKind.UiControl
                or ExpressionValueKind.Coordinates
            && left.Equals(right))
        {
            return 0;
        }
        throw new ArgumentException(ComparableKind(left) is { } kind
            ? $"Object must be of type {kind}."
            : ComparableKind(right) is { } other
                ? $"Object must be of type {other}."
                : "At least one object must implement IComparable.");
    }

    /// <summary>The runtime type name of a value that can be compared at all.</summary>
    private static string? ComparableKind(in ExpressionValue value) => value.Kind switch
    {
        ExpressionValueKind.Number => "Double",
        ExpressionValueKind.Boolean => "Boolean",
        ExpressionValueKind.String => "String",
        _ => null,
    };
}
