using Boors.Market.Analytics;
using Boors.Market.Domain;
using Boors.Market.Filters;
using Boors.Shared.Features.Market;

namespace Boors.Server.Api.Features.Market;

[ApiVersion(1)]
[ApiController, Route("api/v{v:apiVersion}/[controller]/[action]")]
public partial class MarketController : AppControllerBase, IMarketController
{
    [AutoInject] private MarketAnalytics analytics = default!;
    [AutoInject] private MarketWatchService marketWatch = default!;
    [AutoInject] private Boors.Market.Storage.MarketStore store = default!;

    [HttpGet]
    public Task<MarketOverviewDto?> GetOverview([FromQuery] string? date, CancellationToken cancellationToken) =>
        Task.FromResult(analytics.GetOverview(ParseDate(date))?.Map());

    [HttpGet]
    public Task<List<BreadthDayDto>> GetBreadth([FromQuery] int sessions, CancellationToken cancellationToken) =>
        Task.FromResult(analytics.GetBreadth(Math.Clamp(sessions == 0 ? 20 : sessions, 1, 250)).Select(b => b.Map()).ToList());

    [HttpGet]
    public Task<List<SectorPerformanceDto>> GetSectors([FromQuery] string? date, CancellationToken cancellationToken) =>
        Task.FromResult(analytics.GetSectors(ParseDate(date)).Select(s => s.Map()).ToList());

    [HttpGet("{kind}")]
    public Task<List<MoverDto>> GetMovers(string kind, [FromQuery] int take, [FromQuery] string? date, CancellationToken cancellationToken)
    {
        if (!Enum.TryParse<MoverKind>(kind, ignoreCase: true, out var moverKind))
            throw new BadRequestException($"Unknown movers kind '{kind}'.");

        return Task.FromResult(analytics.GetMovers(moverKind, Take(take), ParseDate(date)).Select(m => m.Map()).ToList());
    }

    [HttpGet("{kind}")]
    public Task<List<ScreenRowDto>> GetScreen(string kind, [FromQuery] int take, CancellationToken cancellationToken)
    {
        if (!Enum.TryParse<ScreenKind>(kind, ignoreCase: true, out var screenKind))
            throw new BadRequestException($"Unknown screen '{kind}'.");

        return Task.FromResult(analytics.Screen(screenKind, Take(take)).Select(r => r.Map()).ToList());
    }

    [HttpGet("{insCode}")]
    public Task<List<SeriesPointDto>> GetIndexHistory(long insCode, [FromQuery] string? from, CancellationToken cancellationToken)
    {
        var start = ParseDate(from) ?? DateOnly.MinValue;
        return Task.FromResult(store.GetIndexBars(insCode).Where(b => b.Date >= start).Select(b => new SeriesPointDto(b.Date, b.Close)).ToList());
    }

    [HttpGet("{series}")]
    public Task<List<SeriesPointDto>> GetMacroHistory(string series, [FromQuery] string? from, CancellationToken cancellationToken)
    {
        var start = ParseDate(from) ?? DateOnly.MinValue;
        return Task.FromResult(store.GetMacroDaily(series).Where(b => b.Date >= start).Select(b => new SeriesPointDto(b.Date, b.Close)).ToList());
    }

    [HttpGet]
    public Task<List<InstrumentSearchResultDto>> Search([FromQuery] string text, [FromQuery] int take, CancellationToken cancellationToken) =>
        Task.FromResult(analytics.Search(text ?? string.Empty, Take(take))
            .Select(i => new InstrumentSearchResultDto(i.InsCode, i.Symbol, i.Name, i.Class.ToRuleKey(), i.ParentInsCode))
            .ToList());

    [HttpPost]
    public Task<MarketWatchResultDto> GetMarketWatch(MarketWatchRequestDto request, CancellationToken cancellationToken)
    {
        try
        {
            var result = marketWatch.Query(new MarketWatchRequest
            {
                Session = ParseDate(request.Date),
                Filter = request.Filter,
                Classes = request.Classes is { Count: > 0 } classes ? classes.ToHashSet() : null,
                TradedOnly = request.TradedOnly,
            }, cancellationToken);

            return Task.FromResult(new MarketWatchResultDto(
                result.Session,
                [.. result.Entries.Select(e => e.Map())],
                result.Filter?.Evaluated ?? result.Entries.Count,
                [.. result.Filter?.Errors ?? []]));
        }
        catch (FilterSyntaxException e)
        {
            throw new BadRequestException($"Filter error: {e.Message}");
        }
    }

    private static int Take(int take) => Math.Clamp(take == 0 ? 20 : take, 1, 500);

    internal static DateOnly? ParseDate(string? date) =>
        string.IsNullOrWhiteSpace(date) ? null
        : DateOnly.TryParseExact(date, "yyyy-MM-dd", CultureInfo.InvariantCulture, DateTimeStyles.None, out var d) ? d
        : throw new BadRequestException($"Invalid date '{date}' (expected yyyy-MM-dd).");
}
