namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// The sort the reference's listsort runs its comparison under: the depth-
/// limited quicksort of the runtime it is hosted in, with a heapsort once
/// the depth runs out. It is not stable, and it calls the comparison in a
/// fixed order, so a profile sees the same order of equal items and the
/// same last pair in $1 and $2 as it does there. A list of fewer than two
/// items is not compared at all.
/// </summary>
internal static class ReferenceSort
{
    private const int DepthLimit = 32;

    public static void Sort(ExpressionValue[] keys, Comparison<ExpressionValue> comparison)
    {
        ArgumentNullException.ThrowIfNull(keys);
        ArgumentNullException.ThrowIfNull(comparison);
        if (keys.Length < 2)
            return;
        try
        {
            DepthLimitedQuickSort(keys, 0, keys.Length - 1, comparison, DepthLimit);
        }
        catch (IndexOutOfRangeException)
        {
            // A comparison that contradicts itself can walk the partition
            // off the end of the list.
            throw new ArgumentException(
                "Unable to sort because the comparison returns inconsistent results. "
                + "Either a value does not compare equal to itself, or one value "
                + "repeatedly compared to another value yields different results.");
        }
        catch (Exception error) when (error is not (OperationCanceledException
            or ExpressionEvaluationException or ArgumentException))
        {
            throw new InvalidOperationException(
                "Failed to compare two elements in the array.",
                error);
        }
    }

    private static void DepthLimitedQuickSort(
        ExpressionValue[] keys,
        int left,
        int right,
        Comparison<ExpressionValue> comparison,
        int depthLimit)
    {
        do
        {
            if (depthLimit == 0)
            {
                Heapsort(keys, left, right, comparison);
                return;
            }

            int i = left;
            int j = right;
            // Order the low, middle and high items first; the middle one is
            // the pivot.
            int middle = i + ((j - i) >> 1);
            SwapIfGreater(keys, comparison, i, middle);
            SwapIfGreater(keys, comparison, i, j);
            SwapIfGreater(keys, comparison, middle, j);

            ExpressionValue pivot = keys[middle];
            do
            {
                while (comparison(keys[i], pivot) < 0)
                    i++;
                while (comparison(pivot, keys[j]) < 0)
                    j--;
                if (i > j)
                    break;
                if (i < j)
                    (keys[i], keys[j]) = (keys[j], keys[i]);
                i++;
                j--;
            }
            while (i <= j);

            // The smaller side is sorted by recursion, the larger by the
            // next pass of this loop; both see the reduced depth.
            depthLimit--;
            if (j - left <= right - i)
            {
                if (left < j)
                    DepthLimitedQuickSort(keys, left, j, comparison, depthLimit);
                left = i;
            }
            else
            {
                if (i < right)
                    DepthLimitedQuickSort(keys, i, right, comparison, depthLimit);
                right = j;
            }
        }
        while (left < right);
    }

    private static void SwapIfGreater(
        ExpressionValue[] keys,
        Comparison<ExpressionValue> comparison,
        int a,
        int b)
    {
        if (a != b && comparison(keys[a], keys[b]) > 0)
            (keys[a], keys[b]) = (keys[b], keys[a]);
    }

    private static void Heapsort(
        ExpressionValue[] keys,
        int lo,
        int hi,
        Comparison<ExpressionValue> comparison)
    {
        int n = hi - lo + 1;
        for (int i = n / 2; i >= 1; i--)
            DownHeap(keys, i, n, lo, comparison);
        for (int i = n; i > 1; i--)
        {
            Swap(keys, lo, lo + i - 1);
            DownHeap(keys, 1, i - 1, lo, comparison);
        }
    }

    private static void DownHeap(
        ExpressionValue[] keys,
        int i,
        int n,
        int lo,
        Comparison<ExpressionValue> comparison)
    {
        ExpressionValue d = keys[lo + i - 1];
        while (i <= n / 2)
        {
            int child = 2 * i;
            if (child < n && comparison(keys[lo + child - 1], keys[lo + child]) < 0)
                child++;
            if (!(comparison(d, keys[lo + child - 1]) < 0))
                break;
            keys[lo + i - 1] = keys[lo + child - 1];
            i = child;
        }
        keys[lo + i - 1] = d;
    }

    private static void Swap(ExpressionValue[] keys, int i, int j)
    {
        if (i != j)
            (keys[i], keys[j]) = (keys[j], keys[i]);
    }
}
