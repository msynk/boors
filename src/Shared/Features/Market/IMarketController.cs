namespace Boors.Shared.Features.Market;

/// <summary>Market-wide read models for the workstation. Dates are yyyy-MM-dd; omitted means the latest session.</summary>
[Route("api/v1/[controller]/[action]/"), AuthorizedApi]
public interface IMarketController : IAppController
{
    [HttpGet("{?date}")]
    Task<MarketOverviewDto?> GetOverview(string? date, CancellationToken cancellationToken);

    [HttpGet("{?sessions}")]
    Task<List<BreadthDayDto>> GetBreadth(int sessions, CancellationToken cancellationToken) => default!;

    [HttpGet("{?date}")]
    Task<List<SectorPerformanceDto>> GetSectors(string? date, CancellationToken cancellationToken) => default!;

    /// <summary><paramref name="kind"/>: gainers, losers, value, inflow, outflow.</summary>
    [HttpGet("{kind}{?take,date}")]
    Task<List<MoverDto>> GetMovers(string kind, int take, string? date, CancellationToken cancellationToken) => default!;

    /// <summary><paramref name="kind"/>: Momentum6To1, High52WeekProximity, IndividualFlowIntensity20, BuyerPower5.</summary>
    [HttpGet("{kind}{?take}")]
    Task<List<ScreenRowDto>> GetScreen(string kind, int take, CancellationToken cancellationToken) => default!;

    [HttpGet("{?text,take}")]
    Task<List<InstrumentSearchResultDto>> Search(string text, int take, CancellationToken cancellationToken) => default!;

    /// <summary>The market watch for a session, optionally filtered with a TSETMC filter expression.</summary>
    [HttpPost]
    Task<MarketWatchResultDto> GetMarketWatch(MarketWatchRequestDto request, CancellationToken cancellationToken);
}
