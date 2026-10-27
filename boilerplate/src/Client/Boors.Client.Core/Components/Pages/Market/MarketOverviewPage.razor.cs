using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

public partial class MarketOverviewPage
{
    private const long Tedpix = 32097828799138957;

    [AutoInject] private IMarketController marketController = default!;

    private bool isLoading = true;
    private MarketOverviewDto? overview;
    private List<SectorPerformanceDto> sectors = [];
    private List<MoverDto> movers = [];
    private string moverKind = "value";
    private BitChartConfig? breadthChart;
    private BitChartConfig? sectorChart;
    private List<SeriesPointDto> index = [];
    private BitChartConfig? indexChart;
    private List<SeriesPointDto> dollar = [];
    private string? denomination = "rial";
    private List<BitButtonGroupItem> denominations = [];

    protected override async Task OnInitAsync()
    {
        await base.OnInitAsync();

        denominations =
        [
            new() { Key = "rial", Text = Localizer["In rial"] },
            new() { Key = "usd", Text = Localizer["In free-market dollars"] },
        ];

        try
        {
            overview = await marketController.GetOverview(null, CurrentCancellationToken);
            if (overview is null)
                return;

            var from = overview.Session.AddYears(-1).ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
            var breadthTask = marketController.GetBreadth(20, CurrentCancellationToken);
            var sectorsTask = marketController.GetSectors(null, CurrentCancellationToken);
            var indexTask = marketController.GetIndexHistory(Tedpix, from, CurrentCancellationToken);
            var dollarTask = marketController.GetMacroHistory("price_dollar_rl", from, CurrentCancellationToken);

            breadthChart = MarketCharts.Breadth(await breadthTask, Localizer);
            sectors = await sectorsTask;
            sectorChart = MarketCharts.SectorReturns(sectors);
            index = await indexTask;
            dollar = await dollarTask;
            BuildIndexChart();
            await LoadMovers(moverKind);
        }
        finally
        {
            isLoading = false;
        }
    }

    private async Task LoadMovers(string kind)
    {
        moverKind = kind;
        movers = await marketController.GetMovers(kind, 15, null, CurrentCancellationToken);
    }

    /// <summary>
    /// TEDPIX in rial, or divided by the free-market dollar of the same (or the latest earlier) day and indexed to
    /// 100 at the start — one axis either way, never a dual-axis chart.
    /// </summary>
    private void BuildIndexChart() => indexChart = IndexChart();

    private BitChartConfig IndexChart()
    {
        var points = index;
        if (denomination != "usd" || dollar.Count == 0)
            return MarketCharts.Line([.. points.Select(p => (p.Date, (double)p.Value))], Localizer["TEDPIX"]);

        var inDollars = new List<(DateOnly, double)>();
        var j = 0;
        decimal? usd = null;
        foreach (var point in points)
        {
            while (j < dollar.Count && dollar[j].Date <= point.Date)
                usd = dollar[j++].Value;
            if (usd is > 0)
                inDollars.Add((point.Date, (double)(point.Value / usd.Value)));
        }

        var first = inDollars.Count > 0 ? inDollars[0].Item2 : 1;
        return MarketCharts.Line([.. inDollars.Select(p => (p.Item1, p.Item2 / first * 100))], Localizer["TEDPIX in dollars (start = 100)"]);
    }

    private string? QueueValue(decimal? rials) =>
        rials is null ? null : $"{MarketFormat.Hemat(rials)} {Localizer["hemat"]}";

    private void OpenSymbol(long insCode) => NavigationManager.NavigateTo($"{PageUrls.Symbol}/{insCode}");
}
