using Boors.Market.Services.Contracts;

namespace Boors.Market.Services;

/// <summary>
/// Everything the workstation reads and the operations it can trigger. The same contract is served over HTTP by
/// Boors.Node (routes in the comments) and consumed in-process by the desktop (<see cref="Local.LocalMarketApi"/>) or
/// remotely (<see cref="Remote.RemoteMarketApi"/>). Dates are yyyy-MM-dd strings; null means the latest session.
/// Implementations never block the caller's thread: store and analytics work runs on the thread pool.
/// </summary>
public interface IMarketApi
{
    /// <summary>GET api/v1/Market/GetOverview?date=</summary>
    Task<MarketOverviewDto?> GetOverviewAsync(string? date, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetBreadth?sessions= (default 20, 1–250)</summary>
    Task<List<BreadthDayDto>> GetBreadthAsync(int sessions, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetSectors?date=</summary>
    Task<List<SectorPerformanceDto>> GetSectorsAsync(string? date, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetMovers/{kind}?take=&amp;date= — kind: gainers, losers, value, inflow, outflow.</summary>
    Task<List<MoverDto>> GetMoversAsync(string kind, int take, string? date, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetScreen/{kind}?take= — kind: Momentum6To1, High52WeekProximity, IndividualFlowIntensity20, BuyerPower5.</summary>
    Task<List<ScreenRowDto>> GetScreenAsync(string kind, int take, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetIndexHistory/{insCode}?from=</summary>
    Task<List<SeriesPointDto>> GetIndexHistoryAsync(long insCode, string? from, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/GetMacroHistory/{series}?from= — price_dollar_rl, sekee, geram18, ons, price_eur.</summary>
    Task<List<SeriesPointDto>> GetMacroHistoryAsync(string series, string? from, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Market/Search?text=&amp;take=</summary>
    Task<List<InstrumentSearchResultDto>> SearchAsync(string text, int take, CancellationToken cancellationToken);

    /// <summary>POST api/v1/Market/GetMarketWatch — a filter syntax error is a <see cref="MarketApiException"/> with status 400.</summary>
    Task<MarketWatchResultDto> GetMarketWatchAsync(MarketWatchRequestDto request, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetMetrics/{insCode} — 404 when the instrument is unknown.</summary>
    Task<InstrumentMetricsDto> GetMetricsAsync(long insCode, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetBars/{insCode}?from=</summary>
    Task<List<BarDto>> GetBarsAsync(long insCode, string? from, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetFlows/{insCode}?from=</summary>
    Task<List<FlowDayDto>> GetFlowsAsync(long insCode, string? from, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetEvents/{insCode}</summary>
    Task<List<CorporateEventDto>> GetEventsAsync(long insCode, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetShareholders/{insCode} — refreshed from TSETMC at most once a day (unless offline).</summary>
    Task<List<ShareholderDto>> GetShareholdersAsync(long insCode, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetLetters/{insCode}?take=</summary>
    Task<List<CodalLetterDto>> GetLettersAsync(long insCode, int take, CancellationToken cancellationToken);

    /// <summary>GET api/v1/Instrument/GetMessages/{insCode}?take=</summary>
    Task<List<SupervisorMessageDto>> GetMessagesAsync(long insCode, int take, CancellationToken cancellationToken);

    /// <summary>GET api/v1/MarketData/GetHealth</summary>
    Task<MarketDataHealthDto> GetHealthAsync(CancellationToken cancellationToken);

    /// <summary>POST api/v1/MarketData/RunJob/{job} — queues a job; returns its id.</summary>
    Task<string> RunJobAsync(string job, CancellationToken cancellationToken);

    /// <summary>GET api/v1/MarketData/GetJobs</summary>
    Task<JobQueueDto> GetJobsAsync(CancellationToken cancellationToken);

    /// <summary>POST api/v1/Sync/Pull — queues "download another node's bundle, import, reload"; returns the job id.</summary>
    Task<string> PullFromNodeAsync(SyncPullRequestDto request, CancellationToken cancellationToken);
}
