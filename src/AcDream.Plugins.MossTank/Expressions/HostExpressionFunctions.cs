using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Expressions;

internal static class HostExpressionFunctions
{
    private static readonly TimeSpan RegexTimeout = TimeSpan.FromMilliseconds(100);
    private static readonly string[] GameMonthNames =
    [
        "Morningthaw", "Solclaim", "Seedsow", "Leafdawning", "Verdantine",
        "Thistledown", "HarvestGain", "Leafcull", "Frostfell", "Snowreap",
        "Coldeve", "Wintersebb",
    ];
    private static readonly string[] GameHourNames =
    [
        "Darktide", "Darktide-and-Half", "Foredawn", "Foredawn-and-Half",
        "Dawnsong", "Dawnsong-and-Half", "Morntide", "Morntide-and-Half",
        "Midsong", "Midsong-and-Half", "Warmtide", "Warmtide-and-Half",
        "Evensong", "Evensong-and-Half", "Gloaming", "Gloaming-and-Half",
    ];

    public static void Register(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        ExpressionHostPolicy? policy = null,
        HeldMotions? heldMotions = null)
    {
        ArgumentNullException.ThrowIfNull(registry);
        ArgumentNullException.ThrowIfNull(host);
        policy ??= new ExpressionHostPolicy();
        heldMotions ??= new HeldMotions(host);
        RegisterCharacter(registry, host);
        RegisterSpells(registry, host, policy);
        RegisterObjects(registry, host);
        RegisterLoot(registry, host);
        RegisterFellowship(registry, host);
        RegisterWorldTime(registry, host);
        RegisterUi(registry, host, policy);
        RegisterActions(registry, host, policy);
        RegisterCombatAndMovement(registry, host, heldMotions);
        RegisterLogin(registry, host);
        RegisterNetwork(registry, host);
    }

    // A view a meta built from view XML answers for itself, drawn or not;
    // any other name is one of the host's own windows. The views are looked
    // up on every call, because the owner wires them after registration.
    private static void RegisterUi(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        ExpressionHostPolicy policy)
    {
        registry.Register("uigetcontrol", 2, 2, (_, args) =>
        {
            string view = args[0].AsString("uigetcontrol");
            string control = args[1].AsString("uigetcontrol");
            IMetaViewControls? views = policy.MetaViews;
            bool exists = views is not null && views.HoldsControls(view)
                ? views.ControlExists(view, control)
                : host.Ui.ControlExists(view, control);
            return exists
                ? ExpressionValue.UiControl(new ExpressionUiControl(view, control))
                : ExpressionValue.Zero;
        }, "uigetcontrol[windowName,controlName]");
        // A control that cannot take a label is an error, not a false. A
        // meta view's control that has gone with its view answers 0.
        registry.Register("uisetlabel", 2, 2, (_, args) =>
        {
            ExpressionUiControl control = args[0].AsUiControl("uisetlabel");
            string label = args[1].AsString("uisetlabel");
            IMetaViewControls? views = policy.MetaViews;
            if (views is not null && views.HoldsControls(control.View))
            {
                return views.SetControlLabel(control.View, control.Control, label) switch
                {
                    MetaViewLabelResult.Set => ExpressionValue.One,
                    MetaViewLabelResult.NotFound => ExpressionValue.Zero,
                    _ => throw new ExpressionEvaluationException(
                        "uisetlabel: that control type takes no label"),
                };
            }
            if (!host.Ui.SetControlLabel(control.View, control.Control, label))
            {
                throw new ExpressionEvaluationException(
                    "uisetlabel: that control type takes no label");
            }
            return ExpressionValue.One;
        }, "uisetlabel[control,label]");
        // Any non-zero number means visible, and the second argument is
        // handed back unchanged.
        registry.Register("uisetvisible", 2, 2, (_, args) =>
        {
            ExpressionUiControl control = args[0].AsUiControl("uisetvisible");
            bool visible = args[1].AsNumber("uisetvisible") != 0d;
            IMetaViewControls? views = policy.MetaViews;
            if (views is not null && views.HoldsControls(control.View))
                views.SetControlVisible(control.View, control.Control, visible);
            else
                host.Ui.SetControlVisible(control.View, control.Control, visible);
            return args[1];
        }, "uisetvisible[control,visible]");
        registry.Register("uiviewexists", 1, 1, (_, args) =>
        {
            string view = args[0].AsString("uiviewexists");
            return ExpressionValue.Boolean(
                policy.MetaViews?.ViewExists(view) == true
                || host.Ui.ViewExists(view));
        }, "uiviewexists[windowName]");
        registry.Register("uiviewvisible", 1, 1, (_, args) =>
        {
            string view = args[0].AsString("uiviewvisible");
            IMetaViewControls? views = policy.MetaViews;
            return ExpressionValue.Boolean(views is not null && views.HoldsControls(view)
                ? views.IsViewVisible(view)
                : host.Ui.IsViewVisible(view));
        }, "uiviewvisible[windowName]");
    }

    private static void RegisterLoot(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        registry.Register("hascorpsebeenopenedbyme", 1, 1, (_, args) =>
        {
            uint objectId = ObjectArgument(
                host.Automation.Objects, args, 0, "hascorpsebeenopenedbyme[WorldObject]");
            return ExpressionValue.Boolean(host.Automation.Loot
                .CaptureCorpses(float.MaxValue)
                .Any(corpse => corpse.ObjectId == objectId && corpse.HasBeenOpened));
        }, "hascorpsebeenopenedbyme[corpse]");
        // Corpse ids are dynamic ones, at or above 0x80000000, and are listed
        // signed, as wobjectgetid hands them out, so they compare equal.
        registry.Register("getcorpsesunopenedbyme", 0, 0, (_, _) =>
            ExpressionValue.List(new ExpressionList(host.Automation.Loot
                .CaptureCorpses(float.MaxValue)
                .Where(static corpse => !corpse.HasBeenOpened)
                .Select(static corpse => ExpressionValue.ObjectIdNumber(corpse.ObjectId)))),
            "getcorpsesunopenedbyme[]");
    }

    private static void RegisterCharacter(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        ICharacterInfo character = host.Automation.Character;
        registry.Register("getworldname", 0, 0, (_, _) =>
            ExpressionValue.String(character.WorldName), "getworldname[]");
        registry.Register("getaccounthash", 0, 0, (_, _) =>
            ExpressionValue.String(LegacyStringHash(character.AccountName)
                .ToString("X", CultureInfo.InvariantCulture)), "getaccounthash[]");
        registry.Register("getcharacterindex", 1, 1, (_, args) =>
        {
            // An empty name is the character's own, as in the reference.
            string name = args[0].AsString("getcharacterindex") is { Length: > 0 } given
                ? given
                : character.Name;
            IReadOnlyList<PluginLoginCharacter> roster =
                LoginRoster.Capture(host.Automation.Login);
            // Before the account's list has arrived there is no alphabetical
            // order to place anyone in, so the client's own answer stands.
            return ExpressionValue.Number(roster.Count == 0
                ? character.CharacterIndex
                : LoginRoster.IndexOfName(roster, name));
        }, "getcharacterindex[name]");
        registry.Register("getplayercoordinates", 0, 0, (_, _) =>
        {
            PluginNavigationSnapshot snapshot = host.Automation.Navigation.Snapshot;
            return snapshot.IsAvailable
                ? Coordinates(snapshot.Position)
                : ExpressionValue.Zero;
        }, "getplayercoordinates[]");
        // The landblock keeps its high word in place (0xAABB0000), the form
        // profiles compare against; it is not shifted down to 0xAABB.
        registry.Register("getplayerlandblock", 0, 0, (_, _) =>
            ExpressionValue.Number(
                host.Automation.Navigation.Snapshot.Position.CellId & 0xFFFF0000u),
            "getplayerlandblock[]");
        registry.Register("getplayerlandcell", 0, 0, (_, _) =>
            ExpressionValue.Number(
                host.Automation.Navigation.Snapshot.Position.CellId),
            "getplayerlandcell[]");
        registry.Register("isportaling", 0, 0, (_, _) =>
            ExpressionValue.Boolean(
                !character.IsInWorld
                || host.Automation.Navigation.Snapshot.IsPortalSpace),
            "isportaling[]");
        registry.Register("getcharburden", 0, 0, (_, _) =>
        {
            if (!TryPlayerProperties(host, out PluginItemProperties properties))
                return ExpressionValue.Zero;
            double burden = Get(properties.Ints, 5u);
            double capacity = Get(properties.Ints, 96u);
            return ExpressionValue.Number(capacity > 0d
                ? Math.Round(burden * 100d / capacity)
                : 0d);
        }, "getcharburden[]");
        registry.Register("getcharintprop", 1, 1, (_, args) =>
            ExpressionValue.Number(PlayerProperty(host, args[0], PropertyKind.Int)),
            "getcharintprop[property]");
        registry.Register("getchardoubleprop", 1, 1, (_, args) =>
            ExpressionValue.Number(PlayerProperty(host, args[0], PropertyKind.Double)),
            "getchardoubleprop[property]");
        registry.Register("getcharquadprop", 1, 1, (_, args) =>
            ExpressionValue.Number(PlayerProperty(host, args[0], PropertyKind.Int64)),
            "getcharquadprop[property]");
        registry.Register("getcharboolprop", 1, 1, (_, args) =>
            ExpressionValue.Boolean(PlayerProperty(host, args[0], PropertyKind.Bool) != 0d),
            "getcharboolprop[property]");
        registry.Register("getcharstringprop", 1, 1, (_, args) =>
        {
            if (!TryPlayerProperties(host, out PluginItemProperties properties))
                return ExpressionValue.Zero;
            uint key = ToUInt(args[0], "getcharstringprop");
            return properties.Strings.TryGetValue(key, out string? value)
                ? ExpressionValue.String(value)
                : ExpressionValue.Zero;
        }, "getcharstringprop[property]");

        registry.Register("getcharattribute_base", 1, 1, (_, args) =>
            ExpressionValue.Number(Attribute(character, args[0], buffed: false)),
            "getcharattribute_base[attributeId]");
        registry.Register("getcharattribute_buffed", 1, 1, (_, args) =>
            ExpressionValue.Number(Attribute(character, args[0], buffed: true)),
            "getcharattribute_buffed[attributeId]");
        registry.Register("getcharskill_base", 1, 1, (_, args) =>
            ExpressionValue.Number(Skill(character, args[0], SkillRead.Base)),
            "getcharskill_base[skillId]");
        registry.Register("getcharskill_buffed", 1, 1, (_, args) =>
            ExpressionValue.Number(Skill(character, args[0], SkillRead.Buffed)),
            "getcharskill_buffed[skillId]");
        registry.Register("getcharskill_traininglevel", 1, 1, (_, args) =>
            ExpressionValue.Number(Skill(character, args[0], SkillRead.Training)),
            "getcharskill_traininglevel[skillId]");
        // Three separate reads of the same vital: the unbuffed maximum, the
        // live value, and the buffed maximum. A profile compares the first
        // against the third to decide whether a vital buff is still needed,
        // so they must not collapse onto one field. Each is floored at 1.
        registry.Register("getcharvital_base", 1, 1, (_, args) =>
            ExpressionValue.Number(Vital(character, args[0], VitalRead.Base)),
            "getcharvital_base[vitalId]");
        registry.Register("getcharvital_buffedmax", 1, 1, (_, args) =>
            ExpressionValue.Number(Vital(character, args[0], VitalRead.Maximum)),
            "getcharvital_buffedmax[vitalId]");
        registry.Register("getcharvital_current", 1, 1, (_, args) =>
            ExpressionValue.Number(Vital(character, args[0], VitalRead.Current)),
            "getcharvital_current[vitalId]");
        registry.Register("vitae", 0, 0, (_, _) =>
        {
            if (!TryPlayerProperties(host, out PluginItemProperties properties))
                return ExpressionValue.Number(100d);
            // PropertyFloat.Vitae (129) is the multiplier 0..1.
            double value = Get(properties.Floats, 129u, 1d);
            return ExpressionValue.Number(value * 100d);
        }, "vitae[]");
    }

    private static void RegisterLogin(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        registry.Register("setnextlogin", 1, 1, (_, args) =>
        {
            ILoginAutomation login = host.Automation.Login;
            IReadOnlyList<PluginLoginCharacter> roster = LoginRoster.Capture(login);
            if (!login.IsAvailable || roster.Count == 0)
                return ExpressionValue.Zero;

            // An index written in an expression always counts on from the
            // current character and always wraps, because what a macro asks
            // for is "the next one round", not a fixed position.
            return LoginRoster.TryResolve(
                roster,
                args[0].ToDisplayString(),
                host.Automation.Character.Name,
                relative: true,
                looping: true,
                out int index,
                out LoginRosterFailure _)
                ? ExpressionValue.Boolean(login.SetNextLogin(roster[index].ObjectId))
                : ExpressionValue.Zero;
        }, "setnextlogin[nameOrRelativeIndex]");
        registry.Register("clearnextlogin", 0, 0, (_, _) =>
            ExpressionValue.Boolean(host.Automation.Login.ClearNextLogin()),
            "clearnextlogin[]");
    }

    private static void RegisterNetwork(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        registry.Register("netclients", 0, 1, (_, args) =>
        {
            string? tag = args.Count == 0
                ? null
                : args[0].AsString("netclients");
            var clients = new ExpressionList();
            foreach (PluginNetworkClient client in
                host.Automation.Network.CaptureClients())
            {
                if (!string.IsNullOrEmpty(tag)
                    && !client.Tags.Any(candidate => candidate.Equals(
                        tag,
                        StringComparison.OrdinalIgnoreCase)))
                {
                    continue;
                }

                var tags = new ExpressionList();
                foreach (string candidate in client.Tags)
                    tags.Items.Add(ExpressionValue.String(candidate));
                var data = new ExpressionDictionary();
                data.Items["ClientId"] = ExpressionValue.Number(client.ClientId);
                data.Items["PlayerId"] = ExpressionValue.ObjectIdNumber(client.PlayerId);
                data.Items["Position"] = Coordinates(client.Position);
                data.Items["Name"] = ExpressionValue.String(client.Name);
                data.Items["Tags"] = ExpressionValue.List(tags);
                data.Items["WorldName"] = ExpressionValue.String(client.WorldName);
                data.Items["CurrentHealth"] =
                    ExpressionValue.Number(client.CurrentHealth);
                data.Items["CurrentMana"] =
                    ExpressionValue.Number(client.CurrentMana);
                data.Items["CurrentStamina"] =
                    ExpressionValue.Number(client.CurrentStamina);
                data.Items["MaxHealth"] = ExpressionValue.Number(client.MaxHealth);
                data.Items["MaxMana"] = ExpressionValue.Number(client.MaxMana);
                data.Items["MaxStamina"] =
                    ExpressionValue.Number(client.MaxStamina);
                data.Items["Heading"] = ExpressionValue.Number(client.Heading);
                clients.Items.Add(ExpressionValue.Dictionary(data));
            }
            return ExpressionValue.List(clients);
        }, "netclients[tag?]");

        // What the other clients say they cast, read straight from the host
        // and never through the macro's own cursor: reads do not consume, so
        // looking here disturbs nothing the macro has already taken in. The
        // tag narrows it to peers whose record carries the tag, as above.
        registry.Register("netcasts", 0, 1, (_, args) =>
        {
            string? tag = args.Count == 0
                ? null
                : args[0].AsString("netcasts");
            HashSet<uint>? tagged = null;
            if (!string.IsNullOrEmpty(tag))
            {
                tagged = [];
                foreach (PluginNetworkClient client in
                    host.Automation.Network.CaptureClients())
                {
                    if (client.Tags.Any(candidate => candidate.Equals(
                            tag,
                            StringComparison.OrdinalIgnoreCase)))
                    {
                        tagged.Add(client.ClientId);
                    }
                }
            }

            var casts = new ExpressionList();
            foreach (PluginPeerCast cast in host.Automation.Network.CaptureCasts(0))
            {
                if (tagged is not null && !tagged.Contains(cast.ClientId))
                    continue;
                var data = new ExpressionDictionary();
                data.Items["Sequence"] = ExpressionValue.Number(cast.Sequence);
                data.Items["ClientId"] = ExpressionValue.Number(cast.ClientId);
                data.Items["CasterId"] = ExpressionValue.ObjectIdNumber(cast.CasterObjectId);
                data.Items["TargetId"] = ExpressionValue.ObjectIdNumber(cast.TargetObjectId);
                data.Items["SpellId"] = ExpressionValue.Number(cast.SpellId);
                data.Items["EffectiveSkill"] =
                    ExpressionValue.Number(cast.EffectiveSkill);
                data.Items["SecondsRemaining"] =
                    ExpressionValue.Number(cast.SecondsRemaining);
                data.Items["Landed"] = ExpressionValue.Boolean(cast.Landed);
                casts.Items.Add(ExpressionValue.Dictionary(data));
            }
            return ExpressionValue.List(casts);
        }, "netcasts[tag?]");
    }

    private static void RegisterSpells(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        ExpressionHostPolicy policy)
    {
        ISpellCatalog spells = host.Automation.Spells;
        ICharacterInfo character = host.Automation.Character;
        registry.Register("componentname", 1, 1, (_, args) =>
            spells.TryGetComponent(
                ToUInt(args[0], "componentname"),
                out PluginSpellComponentInfo component)
                    ? ExpressionValue.String(component.Name)
                    : ExpressionValue.Zero,
            "componentname[componentid]");
        registry.Register("componentdata", 1, 1, (_, args) =>
        {
            if (!spells.TryGetComponent(
                    ToUInt(args[0], "componentdata"),
                    out PluginSpellComponentInfo component))
            {
                return ExpressionValue.Dictionary(new ExpressionDictionary());
            }
            var data = new ExpressionDictionary();
            data.Items["BurnRate"] = ExpressionValue.Number(component.BurnRate);
            data.Items["GestureId"] = ExpressionValue.Number(component.GestureId);
            data.Items["GestureSpeed"] = ExpressionValue.Number(component.GestureSpeed);
            data.Items["IconId"] = ExpressionValue.Number(component.IconId);
            data.Items["Id"] = ExpressionValue.Number(component.ComponentId);
            data.Items["Name"] = ExpressionValue.String(component.Name);
            data.Items["SortKey"] = ExpressionValue.Number(component.SortKey);
            data.Items["Type"] = ExpressionValue.String(component.Type);
            data.Items["Word"] = ExpressionValue.String(component.Word);
            return ExpressionValue.Dictionary(data);
        }, "componentdata[componentid]");
        registry.Register("getisspellknown", 1, 1, (_, args) =>
            ExpressionValue.Boolean(spells.IsKnown(ToUInt(args[0], "getisspellknown"))),
            "getisspellknown[spellId]");
        registry.Register("getknownspells", 0, 0, (_, _) =>
        {
            IEnumerable<uint> ids = spells.KnownSelfBuffs
                .Concat(spells.KnownCombatSpells)
                .Select(static spell => spell.SpellId)
                .Distinct()
                .Order();
            return NumberList(ids);
        }, "getknownspells[]");
        registry.Register("spellname", 1, 1, (_, args) =>
            spells.TryGet(ToUInt(args[0], "spellname"), out PluginSpellInfo spell)
                ? ExpressionValue.String(spell.Name)
                : ExpressionValue.Zero, "spellname[spellId]");
        // The reference reads the id as a whole number and answers an empty
        // dictionary for a spell it does not have.
        registry.Register("spelldata", 1, 1, (_, args) =>
            spells.TryGet(
                unchecked((uint)(int)args[0].AsNumber("spelldata")),
                out PluginSpellInfo spell)
                    ? SpellData(spell)
                    : ExpressionValue.Dictionary(new ExpressionDictionary()),
            "spelldata[spellId]");
        registry.Register("getspellexpiration", 1, 1, (_, args) =>
            ExpressionValue.Number(SpellExpiration(
                character.ActiveEnchantments,
                ToUInt(args[0], "getspellexpiration"))),
            "getspellexpiration[spellId]");
        registry.Register("getspellexpirationbyname", 1, 1, (_, args) =>
        {
            string name = args[0].AsString("getspellexpirationbyname");
            PluginSpellInfo? match = spells.KnownSelfBuffs
                .Concat(spells.KnownCombatSpells)
                .FirstOrDefault(spell => spell.Name.Equals(
                    name,
                    StringComparison.OrdinalIgnoreCase));
            return ExpressionValue.Number(match is { SpellId: > 0u } spell
                ? SpellExpiration(character.ActiveEnchantments, spell.SpellId)
                : 0d);
        }, "getspellexpirationbyname[name]");
        registry.Register("getcooldownexpiration", 1, 1, (_, args) =>
            ExpressionValue.Number(spells.GetCooldownRemaining(
                ToUInt(args[0], "getcooldownexpiration"))),
            "getcooldownexpiration[cooldownId]");
        // Two questions, two answers: the buffing and hunting margins over a
        // spell's difficulty are separate profile settings.
        registry.Register("getcancastspell_buff", 1, 1, (_, args) =>
            ExpressionValue.Boolean(CanCastNow(
                host,
                policy,
                ToUInt(args[0], "getcancastspell_buff"),
                hunting: false)),
            "getcancastspell_buff[spellId]");
        registry.Register("getcancastspell_hunt", 1, 1, (_, args) =>
            ExpressionValue.Boolean(CanCastNow(
                host,
                policy,
                ToUInt(args[0], "getcancastspell_hunt"),
                hunting: true)),
            "getcancastspell_hunt[spellId]");
    }

    private static void RegisterObjects(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        IWorldObjectAutomation objects = host.Automation.Objects;
        registry.Register("wobjectfindbyid", 1, 1, (context, args) =>
        {
            uint id = ExpressionValue.SignedObjectId(args[0].AsNumber("wobjectfindbyid"));
            return objects.TryGet(id, out _)
                ? ObjectValue(host, id)
                : ExpressionValue.Zero;
        }, "wobjectfindbyid[id]");
        registry.Register("wobjectgetplayer", 0, 0, (_, _) =>
            ObjectValue(host, host.Automation.Character.ObjectId),
            "wobjectgetplayer[]");
        registry.Register("wobjectgetselection", 0, 0, (_, _) =>
            host.Selection.SelectedObjectId is uint id
                ? ObjectValue(host, id)
                : ExpressionValue.Zero, "wobjectgetselection[]");
        registry.Register("wobjectgetopencontainer", 0, 0, (_, _) =>
            objects.OpenContainerObjectId != 0u
                ? ObjectValue(host, objects.OpenContainerObjectId)
                : ExpressionValue.Zero, "wobjectgetopencontainer[]");
        // Every function below that takes an object reads its argument the
        // reference's way (ObjectArgument): an unknown id is an error before
        // the function runs. Those that read the object itself (ObjectRead)
        // also fail on an object the client has lost since; those that only
        // need its id carry on.
        //
        // Ids come out signed, the form profiles store and hand back.
        registry.Register("wobjectgetid", 1, 1, (_, args) =>
            ExpressionValue.ObjectIdNumber(
                ObjectRead(objects, args, 0, "wobjectgetid[WorldObject]").ObjectId),
            "wobjectgetid[object]");
        // The name a profile reads off an object is its DISPLAY name: the
        // material in front of the bare name.
        registry.Register("wobjectgetname", 1, 1, (_, args) =>
            ExpressionValue.String(DisplayName(
                objects,
                ObjectRead(objects, args, 0, "wobjectgetname[WorldObject]"))),
            "wobjectgetname[object]");
        registry.Register("wobjectgetobjectclass", 1, 1, (_, args) =>
            ExpressionValue.Number((int)ObjectRead(
                objects, args, 0, "wobjectgetobjectclass[WorldObject]").ObjectClass),
            "wobjectgetobjectclass[object]");
        registry.Register("wobjectgettemplatetype", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectRead(
                objects, args, 0, "wobjectgettemplatetype[WorldObject]").WeenieClassId),
            "wobjectgettemplatetype[object]");
        registry.Register("wobjectgetinternaltype", 1, 1, (_, args) =>
            TryObject(objects, args[0], "wobjectgetinternaltype", out PluginWorldObject obj)
                ? ExpressionValue.Number(obj.ItemType)
                : ExpressionValue.Zero, "wobjectgetinternaltype[object]");
        registry.Register("wobjecthasdata", 1, 1, (_, args) =>
            ExpressionValue.Boolean(ObjectRead(
                objects, args, 0, "wobjecthasdata[WorldObject]").HasAppraisalData),
            "wobjecthasdata[object]");
        registry.Register("wobjectlastidtime", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectRead(
                objects, args, 0, "wobjectlastidtime[WorldObject]").LastIdTime),
            "wobjectlastidtime[object]");
        // A lost object is simply not valid; an unknown id is still an error.
        registry.Register("wobjectisvalid", 1, 1, (_, args) =>
            ExpressionValue.Boolean(
                objects.TryGet(
                    ObjectArgument(objects, args, 0, "wobjectisvalid[WorldObject]"),
                    out PluginWorldObject obj)
                && obj.HasPosition),
            "wobjectisvalid[object]");
        registry.Register("wobjectrequestdata", 1, 1, (_, args) =>
            ExpressionValue.Boolean(objects.Identify(
                ObjectArgument(objects, args, 0, "wobjectrequestdata[WorldObject]")).Accepted),
            "wobjectrequestdata[object]");
        registry.Register("wobjectgetisdooropen", 1, 1, (_, args) =>
            ExpressionValue.Boolean(ObjectRead(
                objects, args, 0, "wobjectgetisdooropen[WorldObject]").IsDoorOpen),
            "wobjectgetisdooropen[object]");
        registry.Register("wobjectgetphysicscoordinates", 1, 1, (_, args) =>
        {
            PluginWorldObject obj = ObjectRead(
                objects, args, 0, "wobjectgetphysicscoordinates[WorldObject]");
            return obj.HasPosition ? Coordinates(obj.Position) : ExpressionValue.Zero;
        }, "wobjectgetphysicscoordinates[object]");
        registry.Register("getheading", 1, 1, (_, args) =>
        {
            PluginWorldObject obj = ObjectRead(objects, args, 0, "getheading[WorldObject]");
            return obj.HasPosition
                ? ExpressionValue.Number(NormalizeHeading(obj.Position.HeadingDegrees))
                : ExpressionValue.Zero;
        }, "getheading[object]");
        registry.Register("getheadingto", 1, 1, (_, args) =>
        {
            PluginWorldObject obj = ObjectRead(objects, args, 0, "getheadingto[WorldObject]");
            PluginNavigationSnapshot player = host.Automation.Navigation.Snapshot;
            return player.IsAvailable && obj.HasPosition
                ? ExpressionValue.Number(HeadingTo(player.Position, obj.Position))
                : ExpressionValue.Zero;
        }, "getheadingto[object]");

        RegisterObjectProperty(registry, host, "wobjectgetintprop", PropertyKind.Int);
        RegisterObjectProperty(registry, host, "wobjectgetdoubleprop", PropertyKind.Double);
        RegisterObjectProperty(registry, host, "wobjectgetboolprop", PropertyKind.Bool);
        RegisterObjectProperty(registry, host, "wobjectgetstringprop", PropertyKind.String);
        registry.Register("wobjectgetspellids", 1, 1, (_, args) =>
            NumberList(ObjectRead(
                objects, args, 0, "wobjectgetspellids[WorldObject]").SpellIds),
            "wobjectgetspellids[object]");
        registry.Register("wobjectgetactivespellids", 1, 1, (_, args) =>
            NumberList(ObjectRead(
                objects, args, 0, "wobjectgetactivespellids[WorldObject]").ActiveSpellIds),
            "wobjectgetactivespellids[object]");

        RegisterObjectFinders(registry, host);
        RegisterInventoryCounts(registry, host);
        RegisterObjectVitals(registry, host);
    }

    private static void RegisterObjectFinders(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        registry.Register("wobjectfindall", 0, 0, (_, _) =>
            ObjectList(host, ObjectCapture.For(host).Objects()), "wobjectfindall[]");
        RegisterFinder(registry, host, "wobjectfindallbyobjectclass", ObjectSet.All,
            (obj, arg) => (int)obj.ObjectClass == arg.AsInt32());
        RegisterFinder(registry, host, "wobjectfindallbytemplatetype", ObjectSet.All,
            (obj, arg) => obj.WeenieClassId == ToUInt(arg, "template type"));
        RegisterRegexFinder(registry, host, "wobjectfindallbynamerx", ObjectSet.All);
        RegisterFinder(registry, host, "wobjectfindallinventorybyobjectclass", ObjectSet.Inventory,
            (obj, arg) => (int)obj.ObjectClass == arg.AsInt32());
        RegisterFinder(registry, host, "wobjectfindallinventorybytemplatetype", ObjectSet.Inventory,
            (obj, arg) => obj.WeenieClassId == ToUInt(arg, "template type"));
        RegisterRegexFinder(registry, host, "wobjectfindallinventorybynamerx", ObjectSet.Inventory);
        RegisterFinder(registry, host, "wobjectfindalllandscapebyobjectclass", ObjectSet.Landscape,
            (obj, arg) => (int)obj.ObjectClass == arg.AsInt32());
        RegisterFinder(registry, host, "wobjectfindalllandscapebytemplatetype", ObjectSet.Landscape,
            (obj, arg) => obj.WeenieClassId == ToUInt(arg, "template type"));
        RegisterRegexFinder(registry, host, "wobjectfindalllandscapebynamerx", ObjectSet.Landscape);
        registry.Register("wobjectfindallinventory", 0, 0, (_, _) => ObjectList(host,
            FilterSet(ObjectCapture.For(host).Objects(), ObjectSet.Inventory)),
            "wobjectfindallinventory[]");
        registry.Register("wobjectfindalllandscape", 0, 0, (_, _) => ObjectList(host,
            FilterSet(ObjectCapture.For(host).Objects(), ObjectSet.Landscape)),
            "wobjectfindalllandscape[]");
        registry.Register("wobjectfindallbycontainer", 1, 1, (_, args) =>
        {
            uint container = args[0].AsObjectId("wobjectfindallbycontainer");
            return ObjectList(host, ObjectCapture.For(host).Objects().Where(
                obj => obj.ContainerObjectId == container));
        }, "wobjectfindallbycontainer[container]");
        // Exact match on the DISPLAY name, case included: a profile naming
        // "Silver Long Sword" finds the silver sword, and "Health Elixir" must
        // not pick up "health elixir".
        registry.Register("wobjectfindininventorybyname", 1, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            string name = args[0].AsString("wobjectfindininventorybyname");
            return FirstObject(host, ObjectSet.Inventory, obj =>
                DisplayName(objects, obj).Equals(name, StringComparison.Ordinal));
        }, "wobjectfindininventorybyname[name]");
        // The regex is matched against the DISPLAY name, so `^Silver ` finds
        // a silver sword.
        registry.Register("wobjectfindininventorybynamerx", 1, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            Regex regex = CreateCaseSensitiveRegex(
                args[0].AsString("wobjectfindininventorybynamerx"));
            return FirstObject(host, ObjectSet.Inventory, obj =>
                regex.IsMatch(DisplayName(objects, obj)));
        }, "wobjectfindininventorybynamerx[pattern]");
        registry.Register("wobjectfindininventorybytemplatetype", 1, 1, (_, args) =>
            FirstObject(host, ObjectSet.Inventory, obj =>
                obj.WeenieClassId == ToUInt(args[0], "template type")),
            "wobjectfindininventorybytemplatetype[templateType]");

        // The class finders search every object the client knows; the
        // template-type finder searches only the ground.
        RegisterNearest(registry, host, "wobjectfindnearestbyobjectclass", ObjectSet.All,
            (obj, args) => (int)obj.ObjectClass == args[0].AsInt32());
        RegisterNearest(registry, host, "wobjectfindnearestbytemplatetype", ObjectSet.Landscape,
            (obj, args) => obj.WeenieClassId == ToUInt(args[0], "template type"));
        // Object class first, then a case-sensitive REGEX over the BARE name —
        // unlike the list finders, which read the material-prefixed one.
        registry.Register("wobjectfindnearestbynameandobjectclass", 2, 2, (_, args) =>
        {
            int objectClass = args[0].AsInt32("wobjectfindnearestbynameandobjectclass");
            Regex regex = CreateCaseSensitiveRegex(
                args[1].AsString("wobjectfindnearestbynameandobjectclass"));
            return Nearest(host, ObjectSet.All, obj =>
                (int)obj.ObjectClass == objectClass
                && regex.IsMatch(obj.Name));
        }, "wobjectfindnearestbynameandobjectclass[objectClass,namePattern]");
        RegisterNearest(registry, host, "wobjectfindnearestdoor", ObjectSet.All,
            (obj, _) => obj.ObjectClass == PluginObjectClass.Door, argumentCount: 0);
        // Every monster, blacklisted or not: the reference asks nothing of
        // the combat pass.
        RegisterNearest(registry, host, "wobjectfindnearestmonster", ObjectSet.All,
            (obj, _) => obj.ObjectClass == PluginObjectClass.Monster, argumentCount: 0);
    }

    private static void RegisterInventoryCounts(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        // Both counts read the DISPLAY name and ignore case.
        registry.Register("getitemcountininventorybyname", 1, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            string name = args[0].AsString("getitemcountininventorybyname");
            return ExpressionValue.Number(ObjectCapture.For(host).Objects()
                .Where(obj => obj.IsOwned && DisplayName(objects, obj).Equals(
                    name,
                    StringComparison.OrdinalIgnoreCase))
                .Sum(static obj => Math.Max(1, obj.StackSize)));
        }, "getitemcountininventorybyname[name]");
        registry.Register("getitemcountininventorybynamerx", 1, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            Regex regex = CreateRegex(args[0].AsString("getitemcountininventorybynamerx"));
            return ExpressionValue.Number(ObjectCapture.For(host).Objects()
                .Where(obj => obj.IsOwned && regex.IsMatch(DisplayName(objects, obj)))
                .Sum(static obj => Math.Max(1, obj.StackSize)));
        }, "getitemcountininventorybynamerx[pattern]");
        registry.Register("getinventorycountbytemplatetype", 1, 1, (_, args) =>
        {
            uint template = ToUInt(args[0], "getinventorycountbytemplatetype");
            return ExpressionValue.Number(ObjectCapture.For(host).Objects()
                .Where(obj => obj.IsOwned && obj.WeenieClassId == template)
                .Sum(static obj => Math.Max(1, obj.StackSize)));
        }, "getinventorycountbytemplatetype[templateType]");
        registry.Register("getcontaineritemcount", 0, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            const string parameters = "getcontaineritemcount[WorldObject]";
            uint container = args.Count == 0
                ? host.Automation.Character.ObjectId
                : ObjectArgument(objects, args, 0, parameters);
            // Any container but the character's own is read, so a lost one
            // fails.
            if (container != host.Automation.Character.ObjectId)
                KnownObject(objects, container, parameters);
            if (!objects.TryGet(container, out PluginWorldObject obj)
                || obj.ObjectClass is not (PluginObjectClass.Container or PluginObjectClass.Player))
            {
                return ExpressionValue.Number(-1d);
            }
            return ExpressionValue.Number(ObjectCapture.For(host).Objects().Count(
                item => item.ContainerObjectId == container));
        }, "getcontaineritemcount[container?]");
        registry.Register("getfreeitemslots", 0, 1, (_, args) =>
            ExpressionValue.Number(FreeSlots(host, args, containers: false)),
            "getfreeitemslots[container?]");
        registry.Register("getfreecontainerslots", 0, 1, (_, args) =>
            ExpressionValue.Number(FreeSlots(host, args, containers: true)),
            "getfreecontainerslots[container?]");
    }

    private static void RegisterObjectVitals(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        registry.Register("wobjectgethealth", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectVital(
                host, args, "wobjectgethealth[WorldObject]", VitalObjectRead.Fraction)),
            "wobjectgethealth[object]");
        registry.Register("wobjectgethealthvalue", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectVital(
                host, args, "wobjectgethealthvalue[WorldObject]", VitalObjectRead.Health)),
            "wobjectgethealthvalue[object]");
        registry.Register("wobjectgetstaminavalue", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectVital(
                host, args, "wobjectgetstaminavalue[WorldObject]", VitalObjectRead.Stamina)),
            "wobjectgetstaminavalue[object]");
        registry.Register("wobjectgetmanavalue", 1, 1, (_, args) =>
            ExpressionValue.Number(ObjectVital(
                host, args, "wobjectgetmanavalue[WorldObject]", VitalObjectRead.Mana)),
            "wobjectgetmanavalue[object]");
    }

    private static void RegisterActions(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        ExpressionHostPolicy policy)
    {
        // The reference's echo takes the chat colour (a text type) as its
        // second argument, prints in it and answers true.
        registry.Register("echo", 2, 2, (_, args) =>
        {
            host.Automation.Chat.PostMessage(
                args[0].ToDisplayString(),
                Convert.ToInt32(args[1].AsNumber("echo")));
            return ExpressionValue.Boolean(true);
        }, "echo[text,color]");
        // `chatbox` sends and hands the argument straight back. Both chat
        // verbs take a STRING and nothing else.
        registry.Register("chatbox", 1, 1, (_, args) =>
        {
            string text = args[0].AsString("chatbox");
            if (text.Length != 0)
                host.Automation.Chat.Submit(text);
            return args[0];
        }, "chatbox[text]");
        // `chatboxpaste` only STAGES the text in the chat entry, control
        // characters removed, for the player to finish and send themselves.
        registry.Register("chatboxpaste", 1, 1, (_, args) =>
        {
            string text = StripControlCharacters(args[0].AsString("chatboxpaste"));
            return ExpressionValue.Boolean(
                text.Length != 0 && host.Automation.Chat.Compose(text));
        }, "chatboxpaste[text]");
        // The selection is attempted and the answer is always false: there is
        // no success path, and a profile branches on that.
        registry.Register("actiontryselect", 1, 1, (_, args) =>
        {
            host.Selection.Select(ObjectRead(
                host.Automation.Objects, args, 0, "actiontryselect[WorldObject]").ObjectId);
            return ExpressionValue.Zero;
        }, "actiontryselect[object]");
        registry.Register("actiontryuseitem", 1, 1, (_, args) => ExpressionValue.Boolean(
            host.Automation.Items.Use(ObjectRead(
                host.Automation.Objects, args, 0, "actiontryuseitem[WorldObject]").ObjectId).Accepted),
            "actiontryuseitem[object]");
        registry.Register("actiontryapplyitem", 2, 2, (_, args) => ExpressionValue.Boolean(
            host.Automation.Items.Apply(
                ObjectPair(host, args, "actiontryapplyitem[WorldObject, WorldObject]", out uint target),
                target).Accepted),
            "actiontryapplyitem[source,target]");
        registry.Register("actiontrygiveitem", 2, 2, (_, args) => ExpressionValue.Boolean(
            host.Automation.Items.Give(
                ObjectPair(host, args, "actiontrygiveitem[WorldObject, WorldObject]", out uint target),
                target,
                0u).Accepted),
            "actiontrygiveitem[item,target]");
        registry.Register("actiontrydrop", 1, 1, (_, args) => ExpressionValue.Boolean(
            host.Automation.Items.Drop(
                ObjectArgument(host.Automation.Objects, args, 0, "actiontrydrop[WorldObject]"),
                0u).Accepted),
            "actiontrydrop[item]");
        // The reference joins a stack of the same thing already in the
        // destination unless addToStack is given as 0.
        registry.Register("actiontrymove", 2, 4, (_, args) => ExpressionValue.Boolean(
            host.Automation.Items.MoveToContainer(
                ObjectArgument(host.Automation.Objects, args, 0, ActionTryMoveParameters),
                ObjectArgument(host.Automation.Objects, args, 1, ActionTryMoveParameters),
                0u,
                args.Count >= 3 ? args[2].AsInt32("actiontrymove") : 0,
                joinStack: args.Count < 4
                    || args[3].AsNumber("actiontrymove") != 0d).Accepted),
            "actiontrymove[item,destination,slot?,addToStack?]");
        // As the reference runs it: nothing while the character is busy;
        // otherwise select the item, then move the new stack size of it into
        // the destination (the character when none is given) at the slot
        // given, joining a stack of the same thing already there when the
        // merge flag is set, and answer 1 whatever the server then does.
        registry.Register("actiontrysplit", 2, 5, (_, args) =>
        {
            const string parameters =
                "actiontrysplit[WorldObject, number, WorldObject, number, number]";
            uint item = ObjectArgument(host.Automation.Objects, args, 0, parameters);
            uint destination = args.Count >= 3
                ? ObjectArgument(host.Automation.Objects, args, 2, parameters)
                : host.Automation.Character.ObjectId;
            int slot = args.Count >= 4 ? (int)args[3].AsNumber("actiontrysplit") : 0;
            bool merge = args.Count >= 5 && args[4].AsNumber("actiontrysplit") != 0d;
            if (IsBusy(host))
                return ExpressionValue.Zero;
            host.Selection.Select(item);
            host.Automation.Items.MoveToContainer(
                item,
                destination,
                unchecked((uint)(int)args[1].AsNumber("actiontrysplit")),
                slot,
                joinStack: merge);
            return ExpressionValue.One;
        }, "actiontrysplit[item,newStackSize,destination?,slot?,merge?]");
        // 2 impossible, 0 not attempted yet, 1 begun.
        registry.Register("actiontrycastbyid", 1, 1, (_, args) => CastResult(
            host,
            policy,
            ToUInt(args[0], "actiontrycastbyid"),
            target: null), "actiontrycastbyid[spellId]");
        registry.Register("actiontrycastbyidontarget", 2, 2, (_, args) => CastResult(
            host,
            policy,
            ToUInt(args[0], "actiontrycastbyidontarget"),
            ObjectArgument(
                host.Automation.Objects, args, 1, "actiontrycastbyidontarget[number, WorldObject]")),
            "actiontrycastbyidontarget[spellId,target]");
        // One step towards being able to cast, and true only once there is
        // nothing left to do.
        registry.Register("actiontryequipanywand", 0, 0, (_, _) =>
            ExpressionValue.Boolean(MagicModeStep(host)),
            "actiontryequipanywand[]");
    }

    private static void RegisterFellowship(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        IFellowshipAutomation fellowship = host.Automation.Fellowship;
        registry.Register("getfellowshipstatus", 0, 0, (_, _) =>
            ExpressionValue.Boolean(fellowship.IsInFellowship),
            "getfellowshipstatus[]");
        registry.Register("getfellowshipname", 0, 0, (_, _) =>
            ExpressionValue.String(fellowship.Name), "getfellowshipname[]");
        registry.Register("getfellowshipcount", 0, 0, (_, _) =>
            ExpressionValue.Number(fellowship.MemberCount), "getfellowshipcount[]");
        registry.Register("getfellowshipleaderid", 0, 0, (_, _) =>
            ExpressionValue.ObjectIdNumber(fellowship.LeaderObjectId),
            "getfellowshipleaderid[]");
        registry.Register("getfellowid", 1, 1, (_, args) =>
        {
            IReadOnlyList<PluginFellowMember> roster = fellowship.CaptureRoster();
            int index = args[0].AsInt32("getfellowid");
            return (uint)index < (uint)roster.Count
                ? ExpressionValue.ObjectIdNumber(roster[index].ObjectId)
                : ExpressionValue.Zero;
        }, "getfellowid[index]");
        registry.Register("getfellowname", 1, 1, (_, args) =>
        {
            IReadOnlyList<PluginFellowMember> roster = fellowship.CaptureRoster();
            int index = args[0].AsInt32("getfellowname");
            return (uint)index < (uint)roster.Count
                ? ExpressionValue.String(roster[index].Name)
                : ExpressionValue.String(string.Empty);
        }, "getfellowname[index]");
        registry.Register("getfellowshiplocked", 0, 0, (_, _) =>
            ExpressionValue.Boolean(fellowship.IsLocked), "getfellowshiplocked[]");
        registry.Register("getfellowshipisleader", 0, 0, (_, _) =>
            ExpressionValue.Boolean(
                fellowship.IsInFellowship
                && fellowship.LeaderObjectId == host.Automation.Character.ObjectId),
            "getfellowshipisleader[]");
        registry.Register("getfellowshipisopen", 0, 0, (_, _) =>
            ExpressionValue.Boolean(fellowship.IsOpen), "getfellowshipisopen[]");
        registry.Register("getfellowshipisfull", 0, 0, (_, _) =>
            ExpressionValue.Boolean(
                fellowship.IsInFellowship && fellowship.MemberCount == 9),
            "getfellowshipisfull[]");
        registry.Register("getfellowshipcanrecruit", 0, 0, (_, _) =>
            ExpressionValue.Boolean(
                fellowship.IsInFellowship
                && (fellowship.IsOpen
                    || fellowship.LeaderObjectId == host.Automation.Character.ObjectId)
                && fellowship.MemberCount < 9),
            "getfellowshipcanrecruit[]");
        registry.Register("getfellownames", 0, 0, (_, _) => ExpressionValue.List(
            new ExpressionList(fellowship.CaptureRoster().Select(
                static member => ExpressionValue.String(member.Name)))),
            "getfellownames[]");
        registry.Register("getfellowids", 0, 0, (_, _) => ExpressionValue.List(
            new ExpressionList(fellowship.CaptureRoster().Select(
                static member => ExpressionValue.ObjectIdNumber(member.ObjectId)))),
            "getfellowids[]");
    }

    private static void RegisterWorldTime(
        ExpressionFunctionRegistry registry,
        IPluginHost host)
    {
        PluginWorldTimeSnapshot Snapshot() => host.Automation.WorldTime.Snapshot;
        registry.Register("getgameyear", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().Year), "getgameyear[]");
        registry.Register("getgamemonth", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().Month), "getgamemonth[]");
        registry.Register("getgamemonthname", 1, 1, (_, args) =>
        {
            int index = args[0].AsInt32("getgamemonthname");
            return (uint)index < (uint)GameMonthNames.Length
                ? ExpressionValue.String(GameMonthNames[index])
                : ExpressionValue.String(string.Empty);
        }, "getgamemonthname[monthIndex]");
        registry.Register("getgameday", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().Day), "getgameday[]");
        registry.Register("getgamehour", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().Hour), "getgamehour[]");
        registry.Register("getgamehourname", 1, 1, (_, args) =>
        {
            int index = args[0].AsInt32("getgamehourname");
            return (uint)index < (uint)GameHourNames.Length
                ? ExpressionValue.String(GameHourNames[index])
                : ExpressionValue.String(string.Empty);
        }, "getgamehourname[hourIndex]");
        registry.Register("getminutesuntilday", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().MinutesUntilDay),
            "getminutesuntilday[]");
        registry.Register("getminutesuntilnight", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().MinutesUntilNight),
            "getminutesuntilnight[]");
        registry.Register("getgameticks", 0, 0, (_, _) =>
            ExpressionValue.Number(Snapshot().GameTicks), "getgameticks[]");
        registry.Register("getisday", 0, 0, (_, _) =>
            ExpressionValue.Boolean(Snapshot().IsDay), "getisday[]");
        registry.Register("getisnight", 0, 0, (_, _) =>
            ExpressionValue.Boolean(!Snapshot().IsDay), "getisnight[]");
    }

    private static void RegisterCombatAndMovement(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        HeldMotions heldMotions)
    {
        registry.Register("getcombatstate", 0, 0, (_, _) => ExpressionValue.String(
            host.Automation.Combat.Snapshot.Mode.ToString()), "getcombatstate[]");
        registry.Register("setcombatstate", 1, 1, (_, args) =>
        {
            if (!Enum.TryParse(
                args[0].AsString("setcombatstate"),
                ignoreCase: true,
                out PluginCombatMode mode)
                || mode == PluginCombatMode.Unknown)
            {
                return ExpressionValue.Zero;
            }
            return ExpressionValue.Boolean(
                host.Automation.Combat.EnterMode(mode).Accepted);
        }, "setcombatstate[state]");
        registry.Register("getbusystate", 0, 0, (_, _) =>
            ExpressionValue.Number(IsBusy(host) ? 1d : 0d), "getbusystate[]");
        registry.Register("getequippedweapontype", 0, 0, (_, _) =>
        {
            foreach (PluginEquipmentItem item in host.Automation.Equipment
                .CaptureOwnedEquipment().Where(static item => item.IsEquipped))
            {
                if ((item.EquippedLocation & 0x00400000u) != 0u)
                    return ExpressionValue.String("Missile");
                if ((item.EquippedLocation & 0x01000000u) != 0u)
                    return ExpressionValue.String("Wand");
                if ((item.EquippedLocation & 0x00100000u) != 0u)
                    return ExpressionValue.String("Melee");
            }
            return ExpressionValue.String("None");
        }, "getequippedweapontype[]");
        registry.Register("setmotion", 2, 2, (_, args) =>
        {
            string motion = args[0].AsString("setmotion");
            bool enabled = args[1].AsNumber("setmotion") != 0d;
            if (!HeldMotions.TryParse(motion, out HeldMotion held))
            {
                throw new ExpressionEvaluationException(
                    $"Invalid motion '{motion}'. Valid values are: {HeldMotions.ValidNames}");
            }
            return ExpressionValue.Boolean(
                heldMotions.Set(held, enabled) == PluginNavigationCommandStatus.Accepted);
        }, "setmotion[motion,state]");
        registry.Register("getmotion", 1, 1, (_, args) =>
        {
            string motion = args[0].AsString("getmotion");
            PluginNavigationSnapshot snapshot = host.Automation.Navigation.Snapshot;
            return ExpressionValue.Number(snapshot.IsMoving
                && motion is not null ? 2d : 0d);
        }, "getmotion[motion]");
        registry.Register("clearmotion", 0, 0, (_, _) => ExpressionValue.Boolean(
            heldMotions.Clear() == PluginNavigationCommandStatus.Accepted), "clearmotion[]");
    }

    private static void RegisterObjectProperty(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        string name,
        PropertyKind kind)
    {
        registry.Register(name, 2, 2, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            string parameters = $"{name}[WorldObject, number]";
            uint objectId = ObjectArgument(objects, args, 0, parameters);
            uint key = ToUInt(args[1], name);
            PluginWorldObject obj = KnownObject(objects, objectId, parameters);
            bool appraised = objects.TryCaptureProperties(
                objectId,
                out PluginItemProperties properties);
            // An appraised value wins; otherwise what the client learned
            // when the object appeared; otherwise the kind's default.
            return kind switch
            {
                PropertyKind.Int => ExpressionValue.Number(
                    appraised && properties.Ints.TryGetValue(key, out int number)
                        ? number
                        : TryObjectDescriptionInt(obj, key, out int described)
                            ? described
                            : 0d),
                PropertyKind.Double => ExpressionValue.Number(
                    appraised ? Get(properties.Floats, key) : 0d),
                // The reference answers a bool property as the number 1 or
                // 0, so it prints and concatenates as a number.
                PropertyKind.Bool => ExpressionValue.Number(
                    appraised && Get(properties.Bools, key) ? 1d : 0d),
                PropertyKind.String => ExpressionValue.String(
                    appraised && properties.Strings.TryGetValue(key, out string? text)
                        ? text
                        : key == NameStringKey
                            ? obj.Name
                            : string.Empty),
                _ => ExpressionValue.Zero,
            };
        }, $"{name}[object,property]");
    }

    /// <summary>The string key of an object's name.</summary>
    private const uint NameStringKey = 1u;

    /// <summary>
    /// The integer keys an object answers from its description, the data the
    /// client has as soon as the object appears. They sit in their own key
    /// range, above the appraisal properties.
    /// </summary>
    private const uint ClassIdKey = 0x0D000000u;
    private const uint ContainerKey = 0x0D000002u;
    private const uint LandcellKey = 0x0D000003u;
    private const uint ItemSlotsKey = 0x0D000004u;
    private const uint PackSlotsKey = 0x0D000005u;
    private const uint WielderKey = 0x0D00000Au;

    /// <summary>
    /// One integer an object answers from its description. A wielded object
    /// is "contained" by whoever wields it; the landcell is only there for an
    /// object with a position. Like every integer property the value is a
    /// signed 32-bit number, the reference's form: an id or a cell at or
    /// above 0x80000000 is negative, the same number <c>wobjectgetid</c>
    /// gives for that object.
    /// </summary>
    private static bool TryObjectDescriptionInt(
        in PluginWorldObject obj,
        uint key,
        out int value)
    {
        switch (key)
        {
            case ClassIdKey:
                value = unchecked((int)obj.WeenieClassId);
                return true;
            case ContainerKey:
                value = unchecked((int)(obj.ContainerObjectId != 0u
                    ? obj.ContainerObjectId
                    : obj.WielderObjectId));
                return true;
            case LandcellKey when obj.HasPosition:
                value = unchecked((int)obj.Position.CellId);
                return true;
            case ItemSlotsKey:
                value = unchecked((int)obj.ItemsCapacity);
                return true;
            case PackSlotsKey:
                value = unchecked((int)obj.ContainersCapacity);
                return true;
            case WielderKey:
                value = unchecked((int)obj.WielderObjectId);
                return true;
            default:
                value = 0;
                return false;
        }
    }

    private static void RegisterFinder(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        string name,
        ObjectSet set,
        Func<PluginWorldObject, ExpressionValue, bool> predicate)
    {
        registry.Register(name, 1, 1, (_, args) => ObjectList(host, FilterSet(
            ObjectCapture.For(host).Objects(),
            set).Where(obj => predicate(obj, args[0]))), $"{name}[value]");
    }

    /// <summary>
    /// The shared body of the three list-every-match pattern finders: a
    /// case-sensitive regex over the DISPLAY name, like the single-match
    /// inventory finders.
    /// </summary>
    private static void RegisterRegexFinder(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        string name,
        ObjectSet set)
    {
        registry.Register(name, 1, 1, (_, args) =>
        {
            IWorldObjectAutomation objects = host.Automation.Objects;
            Regex regex = CreateCaseSensitiveRegex(args[0].AsString(name));
            return ObjectList(host, FilterSet(ObjectCapture.For(host).Objects(), set)
                .Where(obj => regex.IsMatch(DisplayName(objects, obj))));
        }, $"{name}[pattern]");
    }

    private static void RegisterNearest(
        ExpressionFunctionRegistry registry,
        IPluginHost host,
        string name,
        ObjectSet set,
        Func<PluginWorldObject, IReadOnlyList<ExpressionValue>, bool> predicate,
        int argumentCount = 1)
    {
        registry.Register(
            name,
            argumentCount,
            argumentCount,
            (_, args) => Nearest(host, set, obj => predicate(obj, args)),
            $"{name}[...]");
    }

    /// <summary>
    /// The nearest matching object in <paramref name="set"/> to the player,
    /// measured in three dimensions and never the player's own object, or 0
    /// when nothing matches. In the whole set an object in a pack or in the
    /// character's hand is a candidate too; its position is not known, so it
    /// sorts last but is still the answer when nothing else matched. The
    /// ground set holds only positioned objects. Ties break on the object id,
    /// which the pool's own enumeration order does not promise.
    /// </summary>
    private static ExpressionValue Nearest(
        IPluginHost host,
        ObjectSet set,
        Func<PluginWorldObject, bool> predicate)
    {
        PluginNavigationSnapshot player = host.Automation.Navigation.Snapshot;
        if (!player.IsAvailable)
            return ExpressionValue.Zero;
        uint self = host.Automation.Character.ObjectId;
        PluginWorldObject? nearest = FilterSet(ObjectCapture.For(host).Objects(), set)
            .Where(obj => obj.ObjectId != self && predicate(obj))
            .OrderBy(obj => obj.HasPosition
                ? DistanceMeters(player.Position, obj.Position)
                : double.MaxValue)
            .ThenBy(static obj => obj.ObjectId)
            .Cast<PluginWorldObject?>()
            .FirstOrDefault();
        return nearest is { } found
            ? ObjectValue(host, found.ObjectId)
            : ExpressionValue.Zero;
    }

    /// <summary>
    /// Straight-line distance in metres, elevation included. The heights
    /// come in 240-metre units, like the map coordinates.
    /// </summary>
    private static double DistanceMeters(
        in PluginNavigationPosition from,
        in PluginNavigationPosition to)
    {
        double flat = from.HorizontalDistanceMeters(to);
        double elevation = (from.Elevation - to.Elevation) * 240d;
        return Math.Sqrt((flat * flat) + (elevation * elevation));
    }

    private static ExpressionValue FirstObject(
        IPluginHost host,
        ObjectSet set,
        Func<PluginWorldObject, bool> predicate)
    {
        PluginWorldObject? found = FilterSet(
                ObjectCapture.For(host).Objects(), set)
            .Where(predicate)
            .OrderBy(static obj => obj.ObjectId)
            .Cast<PluginWorldObject?>()
            .FirstOrDefault();
        return found is { } value
            ? ObjectValue(host, value.ObjectId)
            : ExpressionValue.Zero;
    }

    private static IEnumerable<PluginWorldObject> FilterSet(
        IEnumerable<PluginWorldObject> objects,
        ObjectSet set) => set switch
    {
        ObjectSet.Inventory => objects.Where(static obj => obj.IsOwned),
        ObjectSet.Landscape => objects.Where(static obj => obj.IsLandscape),
        _ => objects,
    };

    private static ExpressionValue ObjectList(
        IPluginHost host,
        IEnumerable<PluginWorldObject> objects)
    {
        Func<uint, string?> names = ObjectNames(host);
        return ExpressionValue.List(new ExpressionList(objects
            .OrderBy(static obj => obj.ObjectId)
            .Select(obj => ExpressionValue.WorldObject(obj.ObjectId, names))));
    }

    private static readonly System.Runtime.CompilerServices.ConditionalWeakTable<
        IPluginHost,
        Func<uint, string?>> Names = new();

    /// <summary>
    /// The name the client knows an object by now, or null once it has
    /// lost it: what a world object prints with. The bare name, as the
    /// reference prints it, without a material in front.
    /// </summary>
    internal static Func<uint, string?> ObjectNames(IPluginHost host) =>
        Names.GetValue(host, static owner => id =>
            owner.Automation.Objects.TryGet(id, out PluginWorldObject obj) ? obj.Name : null);

    /// <summary>A world object value that prints with its current name.</summary>
    internal static ExpressionValue ObjectValue(IPluginHost host, uint id) =>
        ExpressionValue.WorldObject(id, ObjectNames(host));

    private static ExpressionValue NumberList(IEnumerable<uint> values) =>
        ExpressionValue.List(new ExpressionList(values.Select(
            static value => ExpressionValue.Number(value))));

    private static bool TryObject(
        IWorldObjectAutomation objects,
        in ExpressionValue value,
        string operation,
        out PluginWorldObject obj) => objects.TryGet(
        value.AsObjectId(operation),
        out obj);

    /// <summary>
    /// An object argument, read the way the reference reads it before the
    /// function runs. A number is an object id, and an id the client does
    /// not know is an error worded as the reference words it, naming the
    /// function's parameters (<paramref name="parameters"/>, e.g.
    /// "wobjectgetintprop[WorldObject, number]"), the argument and the
    /// value. A world object passes as it is, even one the client has lost
    /// since; whether that matters is up to the function (see
    /// <see cref="KnownObject"/>).
    /// </summary>
    internal static uint ObjectArgument(
        IWorldObjectAutomation objects,
        IReadOnlyList<ExpressionValue> args,
        int index,
        string parameters)
    {
        ExpressionValue value = args[index];
        if (value.Kind != ExpressionValueKind.Number)
            return value.AsObjectId(parameters);
        uint id = ExpressionValue.SignedObjectId(value.AsNumber());
        if (objects.TryGet(id, out _))
            return id;
        int count = parameters.Count(static character => character == ',') + 1;
        throw new ExpressionEvaluationException(
            $"{parameters} expects argument #{index + 1}/{count} to be a WorldObject "
            + $"but an invalid (number)id was passed instead. Passed value: {value.ToDisplayString()}")
        {
            IsArgumentError = true,
        };
    }

    /// <summary>
    /// The object behind an object argument, for a function that reads the
    /// object itself and not just its id. An object the client no longer
    /// knows fails there, as reading a lost object does in the reference.
    /// </summary>
    internal static PluginWorldObject KnownObject(
        IWorldObjectAutomation objects,
        uint id,
        string parameters)
    {
        if (objects.TryGet(id, out PluginWorldObject obj))
            return obj;
        throw new ExpressionEvaluationException(
            $"{parameters} failed: Object reference not set to an instance of an object.");
    }

    private const string ActionTryMoveParameters =
        "actiontrymove[WorldObject, WorldObject, number, number]";

    /// <summary>
    /// The two objects of an apply or a give. Both arguments are read the
    /// reference's way; the objects themselves are read only when the
    /// character is not busy, because the reference answers a busy
    /// character before it looks at them.
    /// </summary>
    private static uint ObjectPair(
        IPluginHost host,
        IReadOnlyList<ExpressionValue> args,
        string parameters,
        out uint second)
    {
        IWorldObjectAutomation objects = host.Automation.Objects;
        uint first = ObjectArgument(objects, args, 0, parameters);
        second = ObjectArgument(objects, args, 1, parameters);
        if (!IsBusy(host))
        {
            KnownObject(objects, first, parameters);
            KnownObject(objects, second, parameters);
        }
        return first;
    }

    /// <summary>
    /// <see cref="ObjectArgument"/> then <see cref="KnownObject"/>: the
    /// object a function reads.
    /// </summary>
    private static PluginWorldObject ObjectRead(
        IWorldObjectAutomation objects,
        IReadOnlyList<ExpressionValue> args,
        int index,
        string parameters) =>
        KnownObject(objects, ObjectArgument(objects, args, index, parameters), parameters);

    private static bool TryPlayerProperties(
        IPluginHost host,
        out PluginItemProperties properties) =>
        host.Automation.Objects.TryCaptureProperties(
            host.Automation.Character.ObjectId,
            out properties);

    private static double PlayerProperty(
        IPluginHost host,
        in ExpressionValue keyValue,
        PropertyKind kind)
    {
        if (!TryPlayerProperties(host, out PluginItemProperties properties))
            return 0d;
        uint key = ToUInt(keyValue, "character property");
        return kind switch
        {
            PropertyKind.Int => Get(properties.Ints, key),
            PropertyKind.Int64 => Get(properties.Int64s, key),
            PropertyKind.Double => Get(properties.Floats, key),
            PropertyKind.Bool => Get(properties.Bools, key) ? 1d : 0d,
            _ => 0d,
        };
    }

    private static double Attribute(
        ICharacterInfo character,
        in ExpressionValue id,
        bool buffed)
    {
        int kind = id.AsInt32("character attribute") - 1;
        foreach (PluginAttributeInfo attribute in character.Attributes)
        {
            if (attribute.Kind == kind)
                return buffed ? attribute.Current : attribute.Base;
        }
        return 0d;
    }

    private static double Skill(
        ICharacterInfo character,
        in ExpressionValue id,
        SkillRead read)
    {
        if (!character.TryGetSkill(ToUInt(id, "character skill"), out PluginSkillInfo skill))
            return 0d;
        return read switch
        {
            SkillRead.Base => skill.Base,
            SkillRead.Buffed => skill.Current,
            SkillRead.Training => skill.Training switch
            {
                PluginSkillTraining.Untrained => 1d,
                PluginSkillTraining.Trained => 2d,
                PluginSkillTraining.Specialized => 3d,
                _ => 0d,
            },
            _ => 0d,
        };
    }

    private static double Vital(
        ICharacterInfo character,
        in ExpressionValue id,
        VitalRead read)
    {
        (uint current, uint maximum, uint baseMaximum) =
            id.AsInt32("character vital") switch
            {
                1 => (character.CurrentHealth, character.MaxHealth, character.BaseHealth),
                2 => (character.CurrentStamina, character.MaxStamina, character.BaseStamina),
                3 => (character.CurrentMana, character.MaxMana, character.BaseMana),
                _ => (0u, 0u, 0u),
            };
        double value = read switch
        {
            VitalRead.Current => current,
            VitalRead.Base => baseMaximum,
            _ => maximum,
        };
        return value < 1d ? 1d : value;
    }

    private static double ObjectVital(
        IPluginHost host,
        IReadOnlyList<ExpressionValue> args,
        string parameters,
        VitalObjectRead read)
    {
        uint id = ObjectRead(host.Automation.Objects, args, 0, parameters).ObjectId;
        ICharacterInfo character = host.Automation.Character;
        if (id == character.ObjectId)
        {
            return read switch
            {
                VitalObjectRead.Fraction => character.MaxHealth == 0u
                    ? -1d : character.CurrentHealth / (double)character.MaxHealth,
                VitalObjectRead.Health => character.CurrentHealth,
                VitalObjectRead.Stamina => character.CurrentStamina,
                VitalObjectRead.Mana => character.CurrentMana,
                _ => -1d,
            };
        }
        if (read is VitalObjectRead.Fraction or VitalObjectRead.Health)
        {
            foreach (PluginCombatTarget target in host.Automation.Combat
                .CaptureHostileTargets(float.MaxValue))
            {
                if (target.ObjectId != id || !target.IsHealthKnown)
                    continue;
                return read == VitalObjectRead.Fraction
                    ? target.HealthFraction
                    : target.MaximumHealth > 0
                        ? target.HealthFraction * target.MaximumHealth
                        : -1d;
            }
        }
        return -1d;
    }

    private static double FreeSlots(
        IPluginHost host,
        IReadOnlyList<ExpressionValue> args,
        bool containers)
    {
        IWorldObjectAutomation objects = host.Automation.Objects;
        string parameters = containers
            ? "getfreecontainerslots[WorldObject]"
            : "getfreeitemslots[WorldObject]";
        uint containerId = args.Count == 0
            ? host.Automation.Character.ObjectId
            : ObjectArgument(objects, args, 0, parameters);
        // The item-slot count reads any container but the character's own,
        // so a lost one fails; the container-slot count reads the id only,
        // and a lost container has none.
        if (!containers && containerId != host.Automation.Character.ObjectId)
            KnownObject(objects, containerId, parameters);
        if (!host.Automation.Objects.TryGet(containerId, out PluginWorldObject container)
            || container.ObjectClass is not (PluginObjectClass.Container or PluginObjectClass.Player))
        {
            return -1d;
        }
        IReadOnlyList<PluginWorldObject> all = ObjectCapture.For(host).Objects();
        int used = all.Count(item => item.ContainerObjectId == containerId
            && (item.ObjectClass == PluginObjectClass.Container) == containers);
        int capacity = containers
            ? container.ContainersCapacity
            : container.ItemsCapacity;
        return Math.Max(0, capacity - used);
    }

    /// <summary>
    /// Castability as a profile asks it: the spell is in the book, its
    /// components are to hand, and the buffed school skill clears the spell's
    /// difficulty plus the profile's margin. A school the host HAS named but
    /// whose skill it has not reported yet reads as skill 0 and FAILS the
    /// comparison, which is the same answer a character who has never trained
    /// that school gets. A spell carrying no school at all names no skill to
    /// compare against, so it is not castable either.
    /// </summary>
    private static bool CanCastNow(
        IPluginHost host,
        ExpressionHostPolicy policy,
        uint spellId,
        bool hunting)
    {
        return host.Automation.Spells.IsKnown(spellId)
            && host.Automation.Magic.HasComponents(spellId)
            && HasSkillFor(host, policy, spellId, hunting);
    }

    /// <summary>
    /// Whether the character's current skill in the spell's school reaches
    /// the spell's difficulty plus the hunting or buffing margin. A spell the
    /// catalogue does not describe, or one with no school, fails.
    /// </summary>
    private static bool HasSkillFor(
        IPluginHost host,
        ExpressionHostPolicy policy,
        uint spellId,
        bool hunting)
    {
        if (!host.Automation.Spells.TryGet(spellId, out PluginSpellInfo spell))
            return false;
        if (spell.School == 0u)
            return false;
        long skill = host.Automation.Character.TryGetSkill(
            spell.School,
            out PluginSkillInfo info)
                ? info.Current
                : 0L;
        return skill >= spell.Difficulty + policy.Margin(hunting);
    }

    /// <summary>The host has an action of its own in flight.</summary>
    private static bool IsBusy(IPluginHost host) =>
        host.Automation.Items.IsBusy
        || host.Automation.Equipment.IsBusy
        || host.Automation.Magic.IsCasting;

    /// <summary>
    /// One step towards being able to cast: wield a wand, then take the magic
    /// stance. True only when both are already done. A busy character takes no
    /// step at all — an expression-driven rule ticks every pass, and without
    /// this the equip and stance requests would be re-issued mid-action.
    /// </summary>
    private static bool MagicModeStep(IPluginHost host)
    {
        if (IsBusy(host))
            return false;
        IEquipmentAutomation equipment = host.Automation.Equipment;
        PluginEquipmentItem? wand = equipment
            .CaptureOwnedEquipment()
            .Cast<PluginEquipmentItem?>()
            .FirstOrDefault(static item =>
                (item!.Value.ValidLocations & 0x01000000u) != 0u
                || (item.Value.EquippedLocation & 0x01000000u) != 0u);
        if (wand is not { } item)
            return false;
        if (!item.IsEquipped)
        {
            equipment.Equip(item.ObjectId);
            return false;
        }
        if (host.Automation.Combat.Snapshot.Mode != PluginCombatMode.Magic)
        {
            host.Automation.Combat.EnterMode(PluginCombatMode.Magic);
            return false;
        }
        return true;
    }

    private static ExpressionValue CastResult(
        IPluginHost host,
        ExpressionHostPolicy policy,
        uint spellId,
        uint? target)
    {
        const double Impossible = 2d;
        const double NotAttempted = 0d;
        const double Begun = 1d;

        // The no-target form also needs the spell known; neither form looks
        // at components or at whether the spell takes a target. With no
        // target the spell goes out at no one: an untargeted or self-targeted
        // spell (a recall, a self buff) needs none.
        if (target is null && !host.Automation.Spells.IsKnown(spellId))
            return ExpressionValue.Number(Impossible);
        if (!HasSkillFor(host, policy, spellId, hunting: true))
            return ExpressionValue.Number(Impossible);
        if (!MagicModeStep(host))
            return ExpressionValue.Number(NotAttempted);

        // The target is read only now, as the reference reads it, so a lost
        // target fails here and not before.
        if (target is uint targetId)
        {
            KnownObject(
                host.Automation.Objects,
                targetId,
                "actiontrycastbyidontarget[number, WorldObject]");
        }

        // Once handed to the caster the attempt counts as begun; whether it
        // lands is the caster's business.
        if (policy.BeginCast is { } beginCast)
            beginCast(spellId, target);
        else if (target is uint objectId)
            host.Automation.Magic.Cast(spellId, objectId);
        else
            host.Automation.Magic.Cast(spellId);
        return ExpressionValue.Number(Begun);
    }

    /// <summary>
    /// A spell as the reference's spelldata describes it, key for key in its
    /// order: numbers for ids, amounts and the raw flag bits, 1 or 0 for each
    /// flag test (read off the raw flags with the reference's bits), text for
    /// the name, description and school, and the formula's component ids as
    /// a list. Duration is the spell's own for an enchantment (type 1), a
    /// portal summon (7) or a fellowship enchantment (12), and -1 for any
    /// other type.
    /// </summary>
    /// <remarks>
    /// CasterEffect, TargetEffect, Generation, SortKey and Speed are the
    /// spell table's caster and target effects, formula version, display
    /// order and component loss.
    /// </remarks>
    private static ExpressionValue SpellData(in PluginSpellInfo spell)
    {
        uint flags = spell.RawFlags;
        double Flag(uint bit) => (flags & bit) != 0u ? 1d : 0d;
        var components = new ExpressionList(spell.FormulaComponentIds.Select(
            static id => ExpressionValue.Number(unchecked((int)id))));
        var data = new ExpressionDictionary();
        data.Items["CasterEffect"] = ExpressionValue.Number(spell.CasterEffect);
        data.Items["ComponentIds"] = ExpressionValue.List(components);
        data.Items["Description"] = ExpressionValue.String(spell.Description);
        data.Items["Difficulty"] = ExpressionValue.Number(spell.Difficulty);
        data.Items["Duration"] = ExpressionValue.Number(
            spell.SpellType is 1 or 7 or 12 ? spell.DurationSeconds : -1d);
        data.Items["Family"] = ExpressionValue.Number(unchecked((int)spell.Family));
        data.Items["Flags"] = ExpressionValue.Number(unchecked((int)flags));
        // The reference's generation, sort key and speed are the spell
        // table's formula version, display order and component loss.
        data.Items["Generation"] = ExpressionValue.Number(spell.FormulaVersion);
        data.Items["IconId"] = ExpressionValue.Number(unchecked((int)spell.IconId));
        data.Items["Id"] = ExpressionValue.Number(unchecked((int)spell.SpellId));
        data.Items["IsDebuff"] = ExpressionValue.Number(Flag(0x10u));
        data.Items["IsFastWindup"] = ExpressionValue.Number(Flag(0x4000u));
        data.Items["IsFellowship"] = ExpressionValue.Number(Flag(0x2000u));
        data.Items["IsIrresistible"] = ExpressionValue.Number(Flag(0x4u));
        data.Items["IsOffensive"] = ExpressionValue.Number(Flag(0x1u));
        data.Items["IsUntargetted"] = ExpressionValue.Number(Flag(0x8u));
        data.Items["Mana"] = ExpressionValue.Number(spell.ManaCost);
        data.Items["Name"] = ExpressionValue.String(spell.Name);
        data.Items["School"] = ExpressionValue.String(SchoolName(spell.School));
        data.Items["SortKey"] = ExpressionValue.Number(spell.DisplayOrder);
        data.Items["Speed"] = ExpressionValue.Number(spell.ComponentLoss);
        data.Items["TargetEffect"] = ExpressionValue.Number(spell.TargetEffect);
        data.Items["TargetMask"] = ExpressionValue.Number(unchecked((int)spell.TargetMask));
        data.Items["Type"] = ExpressionValue.Number(spell.SpellType);
        return ExpressionValue.Dictionary(data);
    }

    /// <summary>
    /// A magic school's name as the reference spells it, from the skill the
    /// spell is cast with. A spell with no school the reference knows fails,
    /// as reading the name of a school it has no entry for does there.
    /// </summary>
    private static string SchoolName(uint skill) => skill switch
    {
        34u => "War Magic",
        33u => "Life Magic",
        32u => "Item Enchantment",
        31u => "Creature Enchantment",
        43u => "Void Magic",
        _ => throw new NullReferenceException(),
    };

    private static double SpellExpiration(
        IReadOnlyList<PluginActiveEnchantment> enchantments,
        uint spellId)
    {
        foreach (PluginActiveEnchantment enchantment in enchantments)
        {
            if (enchantment.SpellId == spellId)
                return enchantment.SecondsRemaining;
        }
        return 0d;
    }

    /// <summary>
    /// A client position as an expression coordinate. The host gives the
    /// height in the same 240-metre units the coordinate keeps, so it goes
    /// across unchanged.
    /// </summary>
    private static ExpressionValue Coordinates(in PluginNavigationPosition position) =>
        ExpressionValue.Coordinates(new ExpressionCoordinates(
            position.EastWest,
            position.NorthSouth,
            position.Elevation));

    private static double HeadingTo(
        in PluginNavigationPosition from,
        in PluginNavigationPosition to)
    {
        double east = to.EastWest - from.EastWest;
        double north = to.NorthSouth - from.NorthSouth;
        return NormalizeHeading(Math.Atan2(east, north) * 180d / Math.PI);
    }

    private static double NormalizeHeading(double heading)
    {
        double result = heading % 360d;
        return result < 0d ? result + 360d : result;
    }

    private static Regex CreateRegex(string pattern) => new(
        pattern,
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
        RegexTimeout);

    /// <summary>
    /// The name a profile means when it writes a name pattern: the object's
    /// material in front of its bare name, as in "Silver Long Sword". An
    /// object with no material, one whose material has no name of its own,
    /// or one whose name already carries the material's name (a jet named
    /// "Jet") keeps the bare name. Both parts are trimmed.
    /// </summary>
    /// <remarks>
    /// This runs once per candidate inside the finders' predicates, so it
    /// reads the one property it needs rather than capturing the object's
    /// whole property bundle.
    /// </remarks>
    private static string DisplayName(
        IWorldObjectAutomation objects,
        in PluginWorldObject obj)
    {
        const uint MaterialTypeProperty = 131u;
        if (!objects.TryGetIntProperty(
                obj.ObjectId,
                MaterialTypeProperty,
                out int material)
            || MaterialNames.Name(material) is not { } prefix
            || obj.Name.Contains(prefix, StringComparison.Ordinal))
        {
            return obj.Name.Trim();
        }
        return prefix + " " + obj.Name.Trim();
    }

    /// <summary>
    /// The regex flavour the name-matching finders use: no IgnoreCase,
    /// so a pattern means exactly what it says. The timeout is a runaway
    /// guard, not a matching rule.
    /// </summary>
    private static Regex CreateCaseSensitiveRegex(string pattern) => new(
        pattern,
        RegexOptions.CultureInvariant,
        RegexTimeout);

    private static string StripControlCharacters(string text)
    {
        if (text.Length == 0 || !text.Any(char.IsControl))
            return text;
        return string.Concat(text.Where(static value => !char.IsControl(value)));
    }

    private static uint ToUInt(in ExpressionValue value, string operation) =>
        checked((uint)value.AsNumber(operation));

    private static double Get(IReadOnlyDictionary<uint, int> values, uint key) =>
        values.TryGetValue(key, out int value) ? value : 0d;

    private static double Get(IReadOnlyDictionary<uint, long> values, uint key) =>
        values.TryGetValue(key, out long value) ? value : 0d;

    private static double Get(
        IReadOnlyDictionary<uint, double> values,
        uint key,
        double fallback = 0d) =>
        values.TryGetValue(key, out double value) ? value : fallback;

    private static bool Get(IReadOnlyDictionary<uint, bool> values, uint key) =>
        values.TryGetValue(key, out bool value) && value;

    /// <summary>Legacy .NET Framework ordinal string hash, the one the reference macro language uses.</summary>
    private static int LegacyStringHash(string value)
    {
        unchecked
        {
            int hash1 = 5381;
            int hash2 = hash1;
            for (int index = 0; index < value.Length; index += 2)
            {
                hash1 = ((hash1 << 5) + hash1) ^ value[index];
                if (index == value.Length - 1)
                    break;
                hash2 = ((hash2 << 5) + hash2) ^ value[index + 1];
            }
            return hash1 + hash2 * 1566083941;
        }
    }

    private enum PropertyKind { Int, Int64, Double, Bool, String }
    private enum SkillRead { Base, Buffed, Training }
    private enum VitalRead { Current, Base, Maximum }
    private enum VitalObjectRead { Fraction, Health, Stamina, Mana }
    private enum ObjectSet { All, Inventory, Landscape }
}
