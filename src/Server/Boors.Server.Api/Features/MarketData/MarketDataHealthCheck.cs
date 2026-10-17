using Boors.Market.Ingestion;
using Microsoft.Extensions.Diagnostics.HealthChecks;

namespace Boors.Server.Api.Features.MarketData;

/// <summary>Degraded when a market dataset is stale or a collection job failed; never Unhealthy, since the app still serves.</summary>
public partial class MarketDataHealthCheck : IHealthCheck
{
    [AutoInject] private DataHealthService healthService = default!;

    public Task<HealthCheckResult> CheckHealthAsync(HealthCheckContext context, CancellationToken cancellationToken = default)
    {
        var report = healthService.GetReport();
        if (report.Healthy)
            return Task.FromResult(HealthCheckResult.Healthy($"Market data current to {report.ExpectedSession:yyyy-MM-dd}."));

        var problems = report.Datasets.Where(d => d.Status is DatasetStatus.Stale or DatasetStatus.Empty).Select(d => $"{d.Dataset}: {d.Status}")
            .Concat(report.LatestRuns.Where(r => r.Status is "failed" or "partial").Select(r => $"job {r.Job}: {r.Status}"))
            .Concat(report.Sources.Where(s => s.Verdict is not ("ok" or "reachable")).Select(s => $"source {s.Name}: {s.Verdict}"));
        return Task.FromResult(HealthCheckResult.Degraded(string.Join("; ", problems)));
    }
}
