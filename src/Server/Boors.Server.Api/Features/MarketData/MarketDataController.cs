using Boors.Market.Ingestion;
using Boors.Shared.Features.MarketData;
using Hangfire;

namespace Boors.Server.Api.Features.MarketData;

[ApiVersion(1)]
[ApiController, Route("api/v{v:apiVersion}/[controller]/[action]")]
[Authorize(Policy = AppFeatures.System.Operations_View)]
public partial class MarketDataController : AppControllerBase, IMarketDataController
{
    [AutoInject] private DataHealthService healthService = default!;
    [AutoInject] private IBackgroundJobClient backgroundJobClient = default!;

    [HttpGet]
    public Task<MarketDataHealthDto> GetHealth(CancellationToken cancellationToken)
    {
        var report = healthService.GetReport();
        return Task.FromResult(new MarketDataHealthDto(
            report.GeneratedAt,
            report.Node,
            report.ExpectedSession,
            report.Healthy,
            [.. report.Datasets.Select(d => new DatasetHealthDto(d.Dataset, d.Rows, d.LatestDate, d.ExpectedDate, d.Status.ToString()))],
            [.. report.LatestRuns.Select(r => new CollectionRunDto(r.Job, r.Node, r.StartedAt, r.FinishedAt, r.Status, r.Items, r.Failures, r.Message))],
            report.LastProbeAt,
            [.. report.Sources.Select(s => new SourceReachabilityDto(s.Name, s.Verdict, s.Status, s.Error))]));
    }

    [HttpPost("{job}")]
    public Task<string> RunJob(string job, CancellationToken cancellationToken)
    {
        if (MarketDataJobRunner.Jobs.Contains(job) is false)
            throw new BadRequestException($"Unknown market-data job '{job}'.");

        return Task.FromResult(backgroundJobClient.Enqueue<MarketDataJobRunner>(runner => runner.Run(job, CancellationToken.None)));
    }
}
