using Boors.Market.Analytics;
using Boors.Market.Sources;
using Boors.Market.Sources.Tsetmc;
using Boors.Market.Storage;
using Boors.Market.Text;
using Boors.Shared.Features.Market;

namespace Boors.Server.Api.Features.Market;

[ApiVersion(1)]
[ApiController, Route("api/v{v:apiVersion}/[controller]/[action]")]
public partial class InstrumentController : AppControllerBase, IInstrumentController
{
    [AutoInject] private MarketStore store = default!;
    [AutoInject] private MarketAnalytics analytics = default!;
    [AutoInject] private TsetmcClient tsetmc = default!;

    [HttpGet("{insCode}")]
    public Task<InstrumentMetricsDto> GetMetrics(long insCode, CancellationToken cancellationToken) =>
        Task.FromResult((analytics.GetInstrumentMetrics(insCode) ?? throw new ResourceNotFoundException($"Instrument {insCode} is not in the store.")).Map());

    [HttpGet("{insCode}")]
    public Task<List<BarDto>> GetBars(long insCode, [FromQuery] string? from, CancellationToken cancellationToken)
    {
        // Adjust over the full history (point-in-time as of the last bar), then cut to the requested window.
        var bars = store.GetDailyBars(insCode);
        if (bars.Count == 0)
            return Task.FromResult(new List<BarDto>());

        var start = MarketController.ParseDate(from) ?? DateOnly.MinValue;
        var adjusted = PointInTime.AdjustFromBars(bars.Select(b => (b.Bar, b.Status)).ToList(), bars[^1].Bar.Date);
        return Task.FromResult(bars.Zip(adjusted)
            .Where(x => x.First.Bar.Date >= start)
            .Select(x => new BarDto(x.First.Bar.Date, x.First.Bar.Open, x.First.Bar.High, x.First.Bar.Low, x.First.Bar.Close, x.Second.AdjustedClose,
                x.First.Bar.Volume, x.First.Bar.Value, x.First.Bar.TradeCount, x.First.Status?.ToString(), x.Second.Return is { } r ? (double)r : null))
            .ToList());
    }

    [HttpGet("{insCode}")]
    public Task<List<FlowDayDto>> GetFlows(long insCode, [FromQuery] string? from, CancellationToken cancellationToken)
    {
        var start = MarketController.ParseDate(from) ?? DateOnly.MinValue;
        return Task.FromResult(store.GetClientTypes(insCode)
            .Where(f => f.Date >= start)
            .Select(f => new FlowDayDto(f.Date, f.BuyIndividualValue, f.SellIndividualValue, f.BuyInstitutionalValue, f.SellInstitutionalValue,
                f.BuyIndividualCount, f.SellIndividualCount, f.IndividualNetValue, f.PerCapitaBuy, f.PerCapitaSell, f.BuyerPower, f.ValuesEstimated))
            .ToList());
    }

    [HttpGet("{insCode}")]
    public Task<List<CorporateEventDto>> GetEvents(long insCode, CancellationToken cancellationToken)
    {
        var adjustments = new[] { "implied", "published" }
            .SelectMany(source => store.GetPriceAdjustments(insCode, source)
                .Select(a => new CorporateEventDto(a.Date, "adjustment", source, a.Factor, null, null)));
        var shareChanges = store.GetShareChanges(insCode)
            .Select(s => new CorporateEventDto(s.Date, "share_change", null, null, s.SharesBefore, s.SharesAfter));
        return Task.FromResult(adjustments.Concat(shareChanges).OrderBy(e => e.Date).ToList());
    }

    [HttpGet("{insCode}")]
    public async Task<List<ShareholderDto>> GetShareholders(long insCode, CancellationToken cancellationToken)
    {
        var now = IranTime.Now(TimeProvider);
        var today = new DateOnly(now.Year, now.Month, now.Day);
        var holders = store.GetLatestShareholders(insCode);
        if (holders.Count == 0 || holders[0].SnapshotDate < today)
        {
            try
            {
                store.UpsertShareholders(await tsetmc.GetShareholdersAsync(insCode, today, CachePolicy.Network, cancellationToken));
                holders = store.GetLatestShareholders(insCode);
            }
            catch (Exception e) when (e is SourceRequestException or HttpRequestException or Polly.ExecutionRejectedException)
            {
                // Serve the last snapshot we have; the page shows its date.
            }
        }

        return [.. holders.Select(h => new ShareholderDto(h.SnapshotDate, h.HolderName, h.Shares, h.Percent, h.Change, h.ChangeAmount))];
    }

    [HttpGet("{insCode}")]
    public Task<List<CodalLetterDto>> GetLetters(long insCode, [FromQuery] int take, CancellationToken cancellationToken) =>
        Task.FromResult(store.GetCodalLetters(insCode, Math.Clamp(take == 0 ? 50 : take, 1, 500))
            .Select(l => new CodalLetterDto(l.TracingNo, l.Title, l.LetterCode, l.LetterTypeId, l.PublishedAt, l.DisclosureGroup, l.HasExcel, l.ReportUrl, l.ExcelUrl, l.PdfUrl))
            .ToList());

    [HttpGet("{insCode}")]
    public Task<List<SupervisorMessageDto>> GetMessages(long insCode, [FromQuery] int take, CancellationToken cancellationToken) =>
        Task.FromResult(store.GetMessages(insCode)
            .OrderByDescending(m => m.Date).ThenByDescending(m => m.Time)
            .Take(Math.Clamp(take == 0 ? 50 : take, 1, 500))
            .Select(m => new SupervisorMessageDto(m.Id, m.Date, m.Title, m.Body, m.Kind.ToString(), m.Reason.ToString(), m.ReopensWithoutLimit))
            .ToList());
}
