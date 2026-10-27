using Boors.Market.Analytics;
using Boors.Market.Services.Contracts;

namespace Boors.Market.Services.Local;

/// <summary>Boors.Market read models → API DTOs. Plain code: the shapes match one to one (the boilerplate used Mapperly).</summary>
public static class DtoMapping
{
    public static MarketOverviewDto ToDto(this MarketOverview s) => new(
        s.Session,
        [.. s.Indices.Select(i => new IndexMoveDto(i.InsCode, i.Name, i.Value, i.ChangePercent))],
        s.EquityValue,
        s.FundValue,
        s.EquityTrades,
        s.Advancers,
        s.Decliners,
        s.Unchanged,
        s.NotTraded,
        s.Halted,
        s.BuyQueues,
        s.BuyQueueValue,
        s.SellQueues,
        s.SellQueueValue,
        s.IndividualNetFlow,
        s.IndividualPerCapitaBuy,
        s.IndividualPerCapitaSell,
        s.FlowValuesEstimated,
        s.Usd?.ToDto(),
        s.Coin?.ToDto(),
        s.TedpixUsdChangePercent);

    public static MacroQuoteDto ToDto(this MacroQuote s) => new(s.Series, s.Date, s.Value, s.ChangePercent);

    public static BreadthDayDto ToDto(this BreadthDay s) =>
        new(s.Date, s.Advancers, s.Decliners, s.Unchanged, s.NotTraded, s.Halted, s.LockedUp, s.LockedDown, s.EquityValue);

    public static SectorPerformanceDto ToDto(this SectorPerformance s) =>
        new(s.SectorCode, s.SectorName, s.Instruments, s.Traded, s.Value, s.WeightedReturn, s.MedianReturn, s.IndividualNetFlow, s.Advancers, s.Decliners);

    public static MoverDto ToDto(this Mover s) =>
        new(s.InsCode, s.Symbol, s.Name, s.InstrumentClass, s.DayStatus, s.Close, s.ChangePercent, s.Value, s.IndividualNetFlow);

    public static ScreenRowDto ToDto(this ScreenRow s) =>
        new(s.InsCode, s.Symbol, s.Name, s.InstrumentClass, s.SectorCode, s.Score, s.Observations);

    public static MarketWatchEntryDto ToDto(this MarketWatchEntry s) => new(
        s.InsCode, s.Symbol, s.Name, s.InstrumentClass, s.SectorCode, s.DayStatus, s.Close, s.ChangePercent, s.Last, s.LastChangePercent,
        s.Volume, s.Value, s.Trades, s.AllowedLow, s.AllowedHigh, s.Queue, s.QueueValue, s.PriceEarnings, s.MarketCap, s.IndividualNetValue,
        s.PerCapitaBuy, s.PerCapitaSell, s.BuyerPower, s.IndividualBuyShare);

    public static InstrumentMetricsDto ToDto(this InstrumentMetrics s) => new(
        s.InsCode, s.Symbol, s.Name, s.InstrumentClass, s.SectorCode, s.SectorName, s.AsOf, s.DayStatus, s.Close, s.ChangePercent, s.MarketCap,
        s.High52Week, s.Low52Week, s.FromHigh52WeekPercent, s.Returns.ToDictionary(), s.UsdReturn1Year, s.AverageValue20, s.IndividualNetFlow20,
        s.PerCapitaBuy, s.PerCapitaSell, s.BuyerPower, s.BuyerPowerPercentile);
}
