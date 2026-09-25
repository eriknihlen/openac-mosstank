using System.Globalization;
using System.Xml;
using System.Xml.Linq;

namespace AcDream.Plugins.MossTank;

internal enum MetaViewControlKind
{
    Layout,
    Button,
}

/// <summary>
/// One control of a meta view: a fixed layout that places its children at
/// pixel offsets, or a button that runs an expression and/or changes the
/// meta state when it is hit. Text and visibility are the parts a meta can
/// change after the view is created.
/// </summary>
internal sealed class MetaViewControl
{
    public MetaViewControlKind Kind { get; init; }
    public string Name { get; init; } = string.Empty;
    public int Left { get; init; }
    public int Top { get; init; }
    public int Width { get; init; }
    public int Height { get; init; }
    public string ActionExpression { get; init; } = string.Empty;
    public string SetState { get; init; } = string.Empty;
    public List<MetaViewControl> Children { get; } = [];

    /// <summary>
    /// Position in <see cref="MetaView.Controls"/>; the drawn window binds
    /// each control's text, visibility and click by this number.
    /// </summary>
    public int Index { get; set; }

    public string Text { get; set; } = string.Empty;
    public bool Visible { get; set; } = true;
}

/// <summary>
/// A meta view parsed from the view XML a CreateView action carries:
/// <c>&lt;view title width height&gt;</c> whose first child is the root
/// control, <c>&lt;control type="layout"&gt;</c> holding
/// <c>&lt;control type="button" name left top width height text actionexpr
/// setstate&gt;</c>. It is the view's whole state, drawn or not: a host
/// without a window keeps the same controls and answers the same queries.
/// </summary>
internal sealed class MetaView
{
    private readonly Dictionary<string, MetaViewControl> _byName =
        new(StringComparer.Ordinal);
    private readonly List<MetaViewControl> _controls = [];

    private MetaView(string title, int width, int height)
    {
        Title = title;
        Width = width;
        Height = height;
    }

    public string Title { get; }
    public int Width { get; }
    public int Height { get; }

    /// <summary>The first child of the view element, or null when it is not a known control.</summary>
    public MetaViewControl? Root { get; private set; }

    /// <summary>Every parsed control in document order, children before their layout's next sibling.</summary>
    public IReadOnlyList<MetaViewControl> Controls => _controls;

    /// <summary>
    /// The control a name refers to. Names need not be unique; when two
    /// controls share one, the one parsed last answers, and a layout is
    /// parsed after its own children.
    /// </summary>
    public MetaViewControl? Find(string name) =>
        _byName.GetValueOrDefault(name);

    /// <summary>
    /// Parses view XML. Malformed XML or a view element with no child node
    /// gives null and a reason; unknown elements and control types are
    /// skipped with everything beneath them.
    /// </summary>
    public static MetaView? Parse(string xml, out string error)
    {
        XElement documentElement;
        try
        {
            documentElement = XDocument.Parse(xml).Root
                ?? throw new XmlException("The view has no root element.");
        }
        catch (XmlException exception)
        {
            error = "Malformed XML in Meta View. " + exception.Message;
            return null;
        }

        XNode? first = documentElement.FirstNode;
        if (first is null)
        {
            error = "The view element has no control.";
            return null;
        }

        var view = new MetaView(
            AttributeString(documentElement, "title"),
            AttributeInt(documentElement, "width"),
            AttributeInt(documentElement, "height"));
        view.Root = view.ParseControl(first);
        error = string.Empty;
        return view;
    }

    private MetaViewControl? ParseControl(XNode node)
    {
        if (node is not XElement element
            || !element.Name.LocalName.Equals("control", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        MetaViewControlKind kind;
        switch (AttributeString(element, "type").ToUpperInvariant())
        {
            case "BUTTON":
                kind = MetaViewControlKind.Button;
                break;
            case "LAYOUT":
                kind = MetaViewControlKind.Layout;
                break;
            default:
                return null;
        }

        var control = new MetaViewControl
        {
            Kind = kind,
            Name = AttributeString(element, "name"),
            Left = AttributeInt(element, "left"),
            Top = AttributeInt(element, "top"),
            Width = AttributeInt(element, "width"),
            Height = AttributeInt(element, "height"),
            ActionExpression = kind == MetaViewControlKind.Button
                ? AttributeString(element, "actionexpr")
                : string.Empty,
            SetState = kind == MetaViewControlKind.Button
                ? AttributeString(element, "setstate")
                : string.Empty,
            Text = kind == MetaViewControlKind.Button
                ? AttributeString(element, "text")
                : string.Empty,
        };
        control.Index = _controls.Count;
        _controls.Add(control);

        if (kind == MetaViewControlKind.Layout)
        {
            foreach (XNode child in element.Nodes())
            {
                MetaViewControl? parsed = ParseControl(child);
                if (parsed is not null)
                    control.Children.Add(parsed);
            }
        }

        if (control.Name.Length != 0)
            _byName[control.Name] = control;
        return control;
    }

    private static string AttributeString(XElement element, string name) =>
        (string?)element.Attribute(name) ?? string.Empty;

    private static int AttributeInt(XElement element, string name) =>
        int.TryParse(
            (string?)element.Attribute(name),
            NumberStyles.Integer,
            CultureInfo.InvariantCulture,
            out int value)
            ? value
            : 0;
}
