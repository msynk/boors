namespace Boors.Shared.Features.MarketData;

[Route("api/v1/[controller]/[action]/")]
public interface IMarketDataController : IAppController
{
    [HttpGet, AuthorizedApi]
    Task<MarketDataHealthDto> GetHealth(CancellationToken cancellationToken);

    /// <summary>Queues a collection job now (instruments, indices, backfill, eod, funds, macro, codal, messages, halts). Returns the background job id.</summary>
    [HttpPost("{job}"), AuthorizedApi, NoRetryPolicy]
    Task<string> RunJob(string job, CancellationToken cancellationToken);
}
