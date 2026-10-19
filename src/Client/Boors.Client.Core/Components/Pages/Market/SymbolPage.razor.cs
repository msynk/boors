using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

/// <summary>One instrument: price, flow and value charts, the key metrics, and its events, holders and disclosures.</summary>
public partial class SymbolPage
{
    [Parameter] public long InsCode { get; set; }

    [AutoInject] private IInstrumentController instrumentController = default!;

    private bool isLoading = true;
    private bool adjusted = true;
    private string? range = "1y";
    private long loadedInsCode;
    private InstrumentMetricsDto? metrics;
    private List<BarDto> bars = [];
    private List<FlowDayDto> flows = [];
    private BitChartConfig? candleChart;
    private BitChartConfig? valueChart;
    private BitChartConfig? flowChart;
    private List<CorporateEventDto> events = [];
    private List<ShareholderDto> shareholders = [];
    private List<CodalLetterDto> letters = [];
    private List<SupervisorMessageDto> messages = [];
    private List<BitButtonGroupItem> ranges = [];
    private List<(string Key, string Label)> returnWindows = [];

    protected override async Task OnInitAsync()
    {
        await base.OnInitAsync();

        ranges =
        [
            new() { Key = "1m", Text = Localizer["1M"] },
            new() { Key = "3m", Text = Localizer["3M"] },
            new() { Key = "6m", Text = Localizer["6M"] },
            new() { Key = "1y", Text = Localizer["1Y"] },
            new() { Key = "3y", Text = Localizer["3Y"] },
            new() { Key = "5y", Text = Localizer["5Y"] },
        ];
        returnWindows =
        [
            ("1w", Localizer["1 week"]),
            ("1m", Localizer["1 month"]),
            ("3m", Localizer["3 months"]),
            ("6m", Localizer["6 months"]),
            ("1y", Localizer["1 year"]),
        ];
    }

    protected override async Task OnParamsSetAsync()
    {
        await base.OnParamsSetAsync();

        // The search box navigates between symbols on the same page instance, so reload whenever the code changes.
        if (InsCode == loadedInsCode)
            return;

        loadedInsCode = InsCode;
        isLoading = true;
        try
        {
            var metricsTask = instrumentController.GetMetrics(InsCode, CurrentCancellationToken);
            var eventsTask = instrumentController.GetEvents(InsCode, CurrentCancellationToken);
            var holdersTask = instrumentController.GetShareholders(InsCode, CurrentCancellationToken);
            var lettersTask = instrumentController.GetLetters(InsCode, 30, CurrentCancellationToken);
            var messagesTask = instrumentController.GetMessages(InsCode, 30, CurrentCancellationToken);

            metrics = await metricsTask;
            await LoadSeries();
            events = await eventsTask;
            shareholders = await holdersTask;
            letters = await lettersTask;
            messages = await messagesTask;
        }
        finally
        {
            isLoading = false;
        }
    }

    private async Task LoadSeries()
    {
        var end = metrics?.AsOf ?? DateOnly.FromDateTime(TimeProvider.GetLocalNow().Date);
        var start = range switch
        {
            "1m" => end.AddMonths(-1),
            "3m" => end.AddMonths(-3),
            "6m" => end.AddMonths(-6),
            "3y" => end.AddYears(-3),
            "5y" => end.AddYears(-5),
            _ => end.AddYears(-1),
        };
        var from = start.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);

        var barsTask = instrumentController.GetBars(InsCode, from, CurrentCancellationToken);
        var flowsTask = instrumentController.GetFlows(InsCode, from, CurrentCancellationToken);

        // Charts show sessions that traded; halted and closed days have no candle to draw.
        bars = [.. (await barsTask).Where(b => b.Trades > 0)];
        flows = await flowsTask;
        BuildCandles();
        valueChart = MarketCharts.Value(bars);
        flowChart = MarketCharts.IndividualFlow(flows);
    }

    private void BuildCandles() => candleChart = MarketCharts.Candles(bars, adjusted);
}
