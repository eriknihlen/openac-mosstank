namespace AcDream.Plugins.MossTank;

internal readonly record struct VtankCraftRecipe(
    string FirstItem,
    string SecondItem,
    string ResultItem,
    int ResultCount,
    uint RequiredSkill,
    int Difficulty,
    int Id);

/// <summary>
/// The game database's <c>CraftInteractions</c> table: what two items make
/// when one is used on the other. There is no other source; a database
/// without the table has no recipes, and nothing is crafted.
/// </summary>
internal sealed class VtankCraftDatabase
{
    private readonly Dictionary<string, VtankCraftRecipe[]> _byResult;

    public static VtankCraftDatabase Empty { get; } = new([]);

    public VtankCraftDatabase(IReadOnlyList<VtankCraftRecipe> recipes)
    {
        ArgumentNullException.ThrowIfNull(recipes);
        Recipes = recipes;
        _byResult = recipes
            .GroupBy(static recipe => recipe.ResultItem, StringComparer.OrdinalIgnoreCase)
            .ToDictionary(
                static group => group.Key,
                static group => group.OrderBy(recipe => recipe.Id).ToArray(),
                StringComparer.OrdinalIgnoreCase);
    }

    /// <summary>Every recipe, in the table's own order.</summary>
    public IReadOnlyList<VtankCraftRecipe> Recipes { get; }

    public IReadOnlyList<VtankCraftRecipe> ForResult(string resultName)
    {
        if (string.IsNullOrWhiteSpace(resultName))
            return Array.Empty<VtankCraftRecipe>();
        return _byResult.TryGetValue(
            resultName.Trim(),
            out VtankCraftRecipe[]? recipes)
                ? recipes
                : Array.Empty<VtankCraftRecipe>();
    }
}
