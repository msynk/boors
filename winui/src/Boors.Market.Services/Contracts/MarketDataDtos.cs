namespace Boors.Market.Services.Contracts;

// Operations read models: data health (same shape as the boilerplate's MarketDataHealthDto), the job queue and node sync.

/// <summary>Freshness of the market-data store, the last run of every collection job and the last reachability probe.</summary>
public record MarketDataHealthDto(
    DateTimeOffset GeneratedAt,
    string Node,
    DateOnly ExpectedSession,
    bool Healthy,
    IReadOnlyList<DatasetHealthDto> Datasets,
    IReadOnlyList<CollectionRunDto> Runs,
    DateTimeOffset? LastProbeAt,
    IReadOnlyList<SourceReachabilityDto> Sources);

/// <summary><paramref name="Status"/> is Ok, Stale, Empty or Info.</summary>
public record DatasetHealthDto(string Dataset, long Rows, DateOnly? LatestDate, DateOnly? ExpectedDate, string Status);

public record CollectionRunDto(string Job, string Node, DateTimeOffset StartedAt, DateTimeOffset? FinishedAt, string Status, int Items, int Failures, string? Message);

public record SourceReachabilityDto(string Name, string Verdict, int? Status, string? Error);

/// <summary>
/// One entry of the single-writer job queue. <paramref name="State"/> is queued, running, succeeded, failed or cancelled;
/// <paramref name="Progress"/> is the latest progress line the job logged; <paramref name="Trigger"/> is manual, schedule or sync.
/// </summary>
public record JobInfoDto(
    string Id,
    string Job,
    string State,
    string Trigger,
    DateTimeOffset EnqueuedAt,
    DateTimeOffset? StartedAt,
    DateTimeOffset? FinishedAt,
    int Items,
    int Failures,
    string? Message,
    string? Progress);

/// <summary>The queue: jobs that can be run by name, whether a scheduler is active, and recent/queued entries (newest first).</summary>
public record JobQueueDto(IReadOnlyList<string> Jobs, bool Scheduled, IReadOnlyList<ScheduledJobDto> Schedule, IReadOnlyList<JobInfoDto> Entries);

/// <summary>A recurring job in Iran time; <paramref name="NextRunAt"/> is null when scheduling is off.</summary>
public record ScheduledJobDto(string Id, string Job, string Cron, DateTimeOffset? NextRunAt);

/// <summary>Pull another node's raw-cache bundle: entries fetched on or after <paramref name="Since"/> (yyyy-MM-dd, default everything).</summary>
public record SyncPullRequestDto(string NodeUrl, string? Since, string? OperatorToken = null);
