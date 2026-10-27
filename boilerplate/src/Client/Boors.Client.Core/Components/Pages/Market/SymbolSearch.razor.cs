using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

public partial class SymbolSearch
{
    [AutoInject] private IMarketController marketController = default!;

    private string? text;
    private List<string> suggestions = [];
    private readonly Dictionary<string, long> insCodes = new(StringComparer.Ordinal);

    private async Task LoadSuggestions(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            suggestions = [];
            return;
        }

        var results = await marketController.Search(value.Trim(), 10, CurrentCancellationToken);
        insCodes.Clear();
        suggestions = [];
        foreach (var result in results)
        {
            var label = string.IsNullOrWhiteSpace(result.Name) ? result.Symbol : $"{result.Symbol} · {result.Name}";
            if (insCodes.TryAdd(label, result.ParentInsCode ?? result.InsCode))
                suggestions.Add(label);
        }
    }

    private async Task Open(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
            return;

        // A picked suggestion maps straight to its instrument; free text opens the best match.
        if (!insCodes.TryGetValue(value, out var insCode))
        {
            var best = (await marketController.Search(value.Trim(), 1, CurrentCancellationToken)).FirstOrDefault();
            if (best is null)
                return;
            insCode = best.ParentInsCode ?? best.InsCode;
        }

        NavigationManager.NavigateTo($"{PageUrls.Symbol}/{insCode}");
    }
}
