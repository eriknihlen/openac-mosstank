namespace AcDream.Plugins.MossTank.Tinkering;

/// <summary>
/// One item and the bags of salvage queued against it, in the order they will
/// be applied. A job is finished when the queue empties or an attempt fails.
/// </summary>
internal sealed class TinkerJob
{
    /// <summary>The item being tinkered.</summary>
    public uint ItemObjectId { get; init; }

    /// <summary>
    /// The bags still to apply, soonest first. A bag is taken off this list
    /// the moment it is handed to the server, not when it is answered for.
    /// </summary>
    public List<uint> SalvageToApply { get; init; } = [];

    /// <summary>The bags already handed over, in the order they went.</summary>
    public List<uint> SalvageApplied { get; } = [];
}

/// <summary>
/// One line of the Tinker or Imbue list: what the plan is, and once the
/// server has answered, whether it came off.
/// </summary>
internal sealed class TinkerListRow
{
    /// <summary>Which attempt on the item this is, counting from one.</summary>
    public int Number { get; init; }

    /// <summary>The item the salvage goes on.</summary>
    public uint ItemObjectId { get; init; }

    /// <summary>The item's name as the list shows it.</summary>
    public string ItemName { get; init; } = string.Empty;

    /// <summary>The bag of salvage this line spends.</summary>
    public uint SalvageObjectId { get; init; }

    /// <summary>The bag's name and workmanship, as the list shows them.</summary>
    public string SalvageName { get; init; } = string.Empty;

    /// <summary>The odds, already formatted as a percentage.</summary>
    public string SuccessText { get; init; } = string.Empty;

    /// <summary>
    /// The odds themselves, kept beside the text so a test or a command can
    /// read the number rather than parse it back.
    /// </summary>
    public double SuccessChance { get; init; }

    /// <summary>
    /// Null while the attempt has not been made, then true or false. The list
    /// colours the row from this.
    /// </summary>
    public bool? Succeeded { get; set; }
}
