using System.Xml.Linq;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// What a drawn meta view binds to. The host reads each control's text and
/// visibility from its slot every frame and invokes the slot's click, so
/// uisetlabel and uisetvisible show without touching the window, and a
/// button runs exactly the control it was drawn from even when another
/// control shares its name.
/// </summary>
internal sealed partial class MetaViewBinding
{
    /// <summary>Pixels between the window's left and right edges and the view's content.</summary>
    public const int SideMargin = 4;

    /// <summary>Pixels above the view's content, which the window's title occupies.</summary>
    public const int TitleHeight = 24;

    private readonly MetaView _view;
    private readonly Action<MetaViewControl> _hit;
    private readonly Action[] _clicks = new Action[Capacity];

    public MetaViewBinding(MetaView view, Action<MetaViewControl> hit)
    {
        _view = view ?? throw new ArgumentNullException(nameof(view));
        _hit = hit ?? throw new ArgumentNullException(nameof(hit));
        for (int slot = 0; slot < Capacity; slot++)
        {
            int index = slot;
            _clicks[slot] = () => Click(index);
        }
    }

    /// <summary>
    /// The panel markup for a view. The view's content keeps its authored
    /// pixel layout: each fixed layout becomes a group at its offset, each
    /// button a button at its own; the window adds a title strip above and
    /// a margin at the sides. Controls past <see cref="Capacity"/> are not
    /// drawn.
    /// </summary>
    /// <remarks>
    /// Not carried over from VTank: its refusal of a view larger than the
    /// game window less 100 pixels (the plugin does not know the window
    /// size), its window icon, and its opening position; a new window opens
    /// at a fixed spot and the host remembers where the player moves it.
    /// </remarks>
    public static string BuildMarkup(MetaView view)
    {
        ArgumentNullException.ThrowIfNull(view);
        var panel = new XElement(
            "panel",
            new XAttribute("x", 320),
            new XAttribute("y", 120),
            new XAttribute("w", view.Width + (2 * SideMargin)),
            new XAttribute("h", view.Height + TitleHeight + SideMargin),
            new XAttribute("title", WindowTitle(view)));
        if (view.Root is not null)
        {
            // The root control fills the view whatever bounds it declares.
            AddControl(
                panel,
                view.Root,
                SideMargin,
                TitleHeight,
                view.Width,
                view.Height);
        }
        return panel.ToString(SaveOptions.DisableFormatting);
    }

    public static string WindowTitle(MetaView view) => "Meta - " + view.Title;

    private static void AddControl(
        XElement parent,
        MetaViewControl control,
        int x,
        int y,
        int width,
        int height)
    {
        if (control.Index >= Capacity)
            return;

        var element = new XElement(
            control.Kind == MetaViewControlKind.Layout ? "group" : "button");
        if (control.Name.Length != 0)
            element.Add(new XAttribute("name", control.Name));
        element.Add(
            new XAttribute("x", x),
            new XAttribute("y", y),
            new XAttribute("w", width),
            new XAttribute("h", height));
        if (control.Kind == MetaViewControlKind.Button)
        {
            element.Add(
                new XAttribute("text", $"{{Text{control.Index}}}"),
                new XAttribute("onclick", $"{{Click{control.Index}}}"));
        }
        element.Add(new XAttribute("visible", $"{{Visible{control.Index}}}"));
        parent.Add(element);

        foreach (MetaViewControl child in control.Children)
        {
            AddControl(
                element,
                child,
                child.Left,
                child.Top,
                child.Width,
                child.Height);
        }
    }

    private string Text(int slot) =>
        slot < _view.Controls.Count ? _view.Controls[slot].Text : string.Empty;

    private bool Visible(int slot) =>
        slot < _view.Controls.Count && _view.Controls[slot].Visible;

    private void Click(int slot)
    {
        if (slot < _view.Controls.Count
            && _view.Controls[slot].Kind == MetaViewControlKind.Button)
        {
            _hit(_view.Controls[slot]);
        }
    }
}
