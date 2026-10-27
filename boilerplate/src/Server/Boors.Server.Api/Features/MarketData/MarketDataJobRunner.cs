using Boors.Market;
using Boors.Market.Ingestion;
using Boors.Market.Sources;
using Hangfire;
using Microsoft.Extensions.Options;

namespace Boors.Server.Api.Features.MarketData;

/// <summary>
/// Hangfire entry points for the market-data collection jobs (see Boors.Market's <see cref="MarketCollector"/>).
/// The store is one DuckDB file per node, so jobs run one at a time per process; each node runs its own collectors
/// and the raw caches are merged between nodes with <c>boors cache export/import</c>.
/// </summary>
public partial class MarketDataJobRunner
{
    public const string RecurringJobPrefix = "MarketData.";

    public static readonly IReadOnlyList<string> Jobs = ["instruments", "indices", "backfill", "eod", "funds", "macro", "codal", "messages", "halts"];

    // One writer per process. Jobs queue behind each other instead of failing on DuckDB write conflicts.
    private static readonly SemaphoreSlim Gate = new(1, 1);

    [AutoInject] private MarketCollector collector = default!;
    [AutoInject] private IOptions<MarketDataOptions> options = default!;
    [AutoInject] private ILogger<MarketDataJobRunner> logger = default!;

    [AutomaticRetry(Attempts = 2)]
    public Task Run(string job, CancellationToken cancellationToken) => job switch
    {
        "instruments" => Gated(() => collector.SyncInstrumentsAsync(details: true, options.Value.MaxInstrumentDetailsPerRun, CachePolicy.Network, cancellationToken), cancellationToken),
        "indices" => Gated(() => collector.SyncIndicesAsync(history: true, CachePolicy.Network, cancellationToken), cancellationToken),
        "backfill" => Gated(() => collector.BackfillHistoryAsync(options.Value.MaxBackfillPerRun, force: false, withMessages: false, scope: null, CachePolicy.Network, cancellationToken), cancellationToken),
        "eod" => Gated(() => collector.EndOfDayAsync(force: false, CachePolicy.Network, cancellationToken), cancellationToken),
        "funds" => Gated(() => collector.SyncFundsAsync(CachePolicy.Network, cancellationToken), cancellationToken),
        "macro" => Gated(() => collector.SyncMacroAsync(null, CachePolicy.Network, cancellationToken), cancellationToken),
        "codal" => Gated(() => collector.SyncCodalAsync(maxPages: 3, CachePolicy.Network, cancellationToken), cancellationToken),
        "messages" => Gated(() => collector.SyncMessagesAsync(top: 200, CachePolicy.Network, cancellationToken), cancellationToken),
        "halts" => Gated(() => collector.ResolveHaltsAsync(options.Value.MaxHaltQueriesPerRun, scope: null, CachePolicy.Network, cancellationToken), cancellationToken),
        _ => throw new ArgumentOutOfRangeException(nameof(job), job, $"Unknown market-data job. Jobs: {string.Join(", ", Jobs)}."),
    };

    private async Task Gated(Func<Task<CollectionResult>> work, CancellationToken cancellationToken)
    {
        await Gate.WaitAsync(cancellationToken);
        try
        {
            var result = await work();
            logger.LogInformation("Market-data job {Job}: {Status}, {Items} items, {Failures} failures. {Message}", result.Job, result.Status, result.Items, result.Failures, result.Message);

            // A failed job (source down, circuit open) is retried by Hangfire; partial results are kept as they are.
            if (result.Status == "failed")
                throw new InvalidOperationException($"Market-data job {result.Job} failed: {result.Message}");
        }
        finally
        {
            Gate.Release();
        }
    }
}
