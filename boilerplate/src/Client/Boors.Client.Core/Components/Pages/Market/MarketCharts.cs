using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

/// <summary>
/// BitChart configurations for the market pages. Charts are native SVG (no JavaScript) and take their colours from
/// theme tokens, so they follow light/dark mode. Conventions (see docs/development-plan.md §7): one value axis per
/// chart (never dual axes), up/down colours with a sign in tooltips, diverging bars around zero for flows and returns.
/// </summary>
public static class MarketCharts
{
    // Time runs left to right in every culture, as on TSETMC and the Persian charting sites.
    private static BitChartOptions BaseOptions(bool legend = false) => new()
    {
        MaintainAspectRatio = false,
        Plugins = new BitChartPluginOptions { Legend = new BitChartLegendOptions { Display = legend, Position = BitChartPosition.Bottom } },
        Scales =
        {
            // Dates stay horizontal and thin out instead of rotating into the value axis.
            ["x"] = new BitChartScaleOptions { Id = "x", Type = BitChartScaleType.Category, Ticks = new BitChartTickOptions { MaxRotation = 0, AutoSkip = true, MaxTicksLimit = 8 } },
        },
    };

    /// <summary>
    /// Candlesticks: a coloured floating bar from open to close (green up, red down) plus a neutral wick from low to
    /// high, drawn as a cap-less error whisker on an invisible line dataset.
    /// </summary>
    public static BitChartConfig Candles(IReadOnlyList<BarDto> bars, bool adjusted)
    {
        // Point-in-time adjusted OHLC: scale every price of a bar by its adjusted/raw close ratio.
        decimal Scale(BarDto b) => adjusted && b.Close != 0 ? b.AdjustedClose / b.Close : 1m;
        var labels = bars.Select(b => MarketFormat.Date(b.Date)).ToList();
        var bodies = bars.Select(b => (b.Open == 0 ? b.Close : b.Open, b.Close, Scale(b)))
            .Select(x => ((double)(Math.Min(x.Item1, x.Close) * x.Item3), (double)(Math.Max(x.Item1, x.Close) * x.Item3)))
            .Select(x => x.Item1 == x.Item2 ? (x.Item1, x.Item2 + Math.Max(x.Item2 * 0.0005, 0.5)) : x) // keep flat days visible
            .Select(x => ((double Low, double High)?)x)
            .ToList();
        var mids = bars.Select(b => (double?)((double)((b.High + b.Low) / 2 * Scale(b)))).ToList();
        var wicks = bars.Select(b => (BitChartErrorBar?)(BitChartErrorBar)(double)((b.High - b.Low) / 2 * Scale(b))).ToList();
        var colors = bars.Select(b => b.Close >= (b.Open == 0 ? b.Close : b.Open) ? MarketFormat.Up : MarketFormat.Down).ToList();

        var options = BaseOptions();
        options.Scales["y"] = new BitChartScaleOptions { Id = "y", Type = BitChartScaleType.Linear };
        options.Interaction = new BitChartInteractionOptions { Mode = BitChartInteractionMode.Index, Intersect = false };

        return new BitChartConfig
        {
            Type = BitChartType.Bar,
            Options = options,
            Data = new BitChartData
            {
                Labels = labels,
                Datasets =
                {
                    new BitChartDataset { Label = "Open–close", RangeData = bodies, BackgroundColors = colors, Grouped = false, Order = 1 },
                    new BitChartDataset
                    {
                        Label = "Low–high",
                        Type = BitChartType.Line,
                        Data = mids,
                        ErrorData = wicks,
                        ErrorBarCapWidth = 0,
                        ErrorBarWidth = 1,
                        ErrorBarColor = MarketFormat.Neutral,
                        BorderColor = "transparent",
                        BorderWidth = 0,
                        PointRadius = 0,
                        Order = 0,
                    },
                },
            },
        };
    }

    /// <summary>Value traded per day (billion toman), one hue — magnitude, not direction.</summary>
    public static BitChartConfig Value(IReadOnlyList<BarDto> bars) => new()
    {
        Type = BitChartType.Bar,
        Options = WithY(BaseOptions()),
        Data = new BitChartData
        {
            Labels = bars.Select(b => MarketFormat.Date(b.Date)).ToList(),
            Datasets = { new BitChartDataset { Label = "Value (billion toman)", Data = bars.Select(b => (double?)(double)(b.Value / MarketFormat.RialsPerBillionToman)).ToList(), BackgroundColor = MarketFormat.Accent } },
        },
    };

    /// <summary>Individual (حقیقی) net money flow per day, diverging around zero.</summary>
    public static BitChartConfig IndividualFlow(IReadOnlyList<FlowDayDto> flows) => new()
    {
        Type = BitChartType.Bar,
        Options = WithY(BaseOptions()),
        Data = new BitChartData
        {
            Labels = flows.Select(f => MarketFormat.Date(f.Date)).ToList(),
            Datasets =
            {
                new BitChartDataset
                {
                    Label = "Individual net (billion toman)",
                    Data = flows.Select(f => (double?)(double)(f.IndividualNetValue / MarketFormat.RialsPerBillionToman)).ToList(),
                    BackgroundColorFn = ctx => (ctx.Value ?? 0) < 0 ? MarketFormat.Down : MarketFormat.Up,
                },
            },
        },
    };

    /// <summary>Daily breadth as 100% stacked bars: advancers, unchanged, decliners, not traded.</summary>
    public static BitChartConfig Breadth(IReadOnlyList<BreadthDayDto> days, IStringLocalizer localizer)
    {
        var options = BaseOptions(legend: true);
        options.Scales["x"].Stacked = true;
        options.Scales["y"] = new BitChartScaleOptions { Id = "y", Type = BitChartScaleType.Linear, Stacked = true, Stacked100 = true, Ticks = new BitChartTickOptions { Suffix = "%" } };
        var labels = days.Select(d => MarketFormat.Date(d.Date)).ToList();

        BitChartDataset Series(string label, Func<BreadthDayDto, int> value, string color) =>
            new() { Label = label, Data = days.Select(d => (double?)value(d)).ToList(), BackgroundColor = color, BorderColor = color };

        return new BitChartConfig
        {
            Type = BitChartType.Bar,
            Options = options,
            Data = new BitChartData
            {
                Labels = labels,
                Datasets =
                {
                    Series(localizer["Advancers"], d => d.Advancers, MarketFormat.Up),
                    Series(localizer["Unchanged"], d => d.Unchanged, MarketFormat.Neutral),
                    Series(localizer["Decliners"], d => d.Decliners, MarketFormat.Down),
                    Series(localizer["Not traded"], d => d.NotTraded, MarketFormat.Warning),
                },
            },
        };
    }

    /// <summary>Sector returns as horizontal bars diverging around zero.</summary>
    public static BitChartConfig SectorReturns(IReadOnlyList<SectorPerformanceDto> sectors)
    {
        var shown = sectors.Where(s => s.WeightedReturn is not null).OrderByDescending(s => s.WeightedReturn).ToList();
        // BitChart keeps the scale keys by role when the bars lie down: "x" stays the category (index) scale and is
        // drawn vertically, "y" stays the value scale and is drawn horizontally (Chart.js swaps them instead).
        var options = BaseOptions();
        options.IndexAxis = BitChartIndexAxis.Y;
        options.Scales["x"] = new BitChartScaleOptions { Id = "x", Type = BitChartScaleType.Category, Ticks = new BitChartTickOptions { AutoSkip = false } };
        options.Scales["y"] = new BitChartScaleOptions { Id = "y", Type = BitChartScaleType.Linear, Ticks = new BitChartTickOptions { Suffix = "%" } };

        return new BitChartConfig
        {
            Type = BitChartType.Bar,
            Options = options,
            Data = new BitChartData
            {
                Labels = shown.Select(s => s.SectorName ?? s.SectorCode).ToList(),
                Datasets =
                {
                    new BitChartDataset
                    {
                        Label = "Weighted return %",
                        Data = shown.Select(s => (double?)Math.Round(s.WeightedReturn!.Value * 100, 2)).ToList(),
                        BackgroundColorFn = ctx => (ctx.Value ?? 0) < 0 ? MarketFormat.Down : MarketFormat.Up,
                        BorderRadius = 2,
                    },
                },
            },
        };
    }

    /// <summary>A single line (index level, or index in dollars), indexed or raw.</summary>
    public static BitChartConfig Line(IReadOnlyList<(DateOnly Date, double Value)> points, string label)
    {
        var options = WithY(BaseOptions());
        options.Interaction = new BitChartInteractionOptions { Mode = BitChartInteractionMode.Index, Intersect = false };
        return new BitChartConfig
        {
            Type = BitChartType.Line,
            Options = options,
            Data = new BitChartData
            {
                Labels = points.Select(p => MarketFormat.Date(p.Date)).ToList(),
                Datasets = { new BitChartDataset { Label = label, Data = points.Select(p => (double?)p.Value).ToList(), BorderColor = MarketFormat.Accent, BorderWidth = 2, PointRadius = 0 } },
            },
        };
    }

    private static BitChartOptions WithY(BitChartOptions options)
    {
        options.Scales["y"] = new BitChartScaleOptions { Id = "y", Type = BitChartScaleType.Linear };
        return options;
    }
}
