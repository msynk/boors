namespace Boors.Shared.Features.MarketData;

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
