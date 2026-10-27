using Boors.Shared.Features.MarketData;

namespace Boors.Client.Core.Components.Pages.Market;

/// <summary>Freshness of every dataset, the last collection runs, source reachability, and buttons to queue a job now.</summary>
public partial class MarketHealthPage
{
    // Mirrors MarketDataJobRunner.Jobs on the server; an unknown name is rejected there with a 400.
    private static readonly string[] jobs = ["instruments", "indices", "eod", "backfill", "funds", "macro", "codal", "messages", "halts"];

    [AutoInject] private IMarketDataController marketDataController = default!;

    private bool isLoading;
    private string? runningJob;
    private string? queuedMessage;
    private MarketDataHealthDto? health;

    protected override async Task OnInitAsync()
    {
        await base.OnInitAsync();
        await Load();
    }

    private async Task Load()
    {
        isLoading = true;
        try
        {
            health = await marketDataController.GetHealth(CurrentCancellationToken);
        }
        finally
        {
            isLoading = false;
        }
    }

    private async Task Run(string job)
    {
        runningJob = job;
        try
        {
            var id = await marketDataController.RunJob(job, CurrentCancellationToken);
            queuedMessage = $"{Localizer["Queued"]} {job} (#{id})";
        }
        finally
        {
            runningJob = null;
        }
    }

    private static BitColor StatusColor(string? status) => status?.ToLowerInvariant() switch
    {
        "ok" or "succeeded" or "reachable" => BitColor.Success,
        "stale" or "partial" or "degraded" or "running" => BitColor.Warning,
        "empty" or "failed" or "blocked" or "unreachable" => BitColor.Error,
        _ => BitColor.Info,
    };
}
