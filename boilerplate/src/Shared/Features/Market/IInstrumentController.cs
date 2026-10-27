namespace Boors.Shared.Features.Market;

/// <summary>Per-instrument data for the symbol workspace. <c>insCode</c> is TSETMC's instrument code.</summary>
[Route("api/v1/[controller]/[action]/"), AuthorizedApi]
public interface IInstrumentController : IAppController
{
    [HttpGet("{insCode}")]
    Task<InstrumentMetricsDto> GetMetrics(long insCode, CancellationToken cancellationToken);

    /// <summary>Daily bars from <paramref name="from"/> (yyyy-MM-dd, default: everything) with point-in-time adjusted closes.</summary>
    [HttpGet("{insCode}{?from}")]
    Task<List<BarDto>> GetBars(long insCode, string? from, CancellationToken cancellationToken) => default!;

    [HttpGet("{insCode}{?from}")]
    Task<List<FlowDayDto>> GetFlows(long insCode, string? from, CancellationToken cancellationToken) => default!;

    [HttpGet("{insCode}")]
    Task<List<CorporateEventDto>> GetEvents(long insCode, CancellationToken cancellationToken) => default!;

    /// <summary>Latest major shareholders; refreshed from TSETMC at most once a day.</summary>
    [HttpGet("{insCode}")]
    Task<List<ShareholderDto>> GetShareholders(long insCode, CancellationToken cancellationToken) => default!;

    [HttpGet("{insCode}{?take}")]
    Task<List<CodalLetterDto>> GetLetters(long insCode, int take, CancellationToken cancellationToken) => default!;

    [HttpGet("{insCode}{?take}")]
    Task<List<SupervisorMessageDto>> GetMessages(long insCode, int take, CancellationToken cancellationToken) => default!;
}
