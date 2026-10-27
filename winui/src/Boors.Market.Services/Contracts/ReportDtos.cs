namespace Boors.Market.Services.Contracts;

// Read models of the Reports page (Boors.Market.Analytics.MarketReports). Returns, shares and scores of 0–1 are
// fractions (0.25 = 25%); the reliability score is 0–100; values are rials. Periods: 1m, 3m, 6m, ytd, 1y, 3y, 5y, and
// custom (a window given by its start date). A window ends at its base date (AsOf); when that is earlier than the
// latest session (Latest), the SinceThen fields show what followed.

public record PerformanceRowDto(
    long InsCode,
    string Symbol,
    string? Name,
    string InstrumentClass,
    string? SectorCode,
    string? SectorName,
    decimal? Close,
    double Return,
    double? AnnualizedReturn,
    double? UsdReturn,
    double? ExcessReturn,
    double? Volatility,
    double? RiskAdjusted,
    double MaxDrawdown,
    double? PositiveMonths,
    double? UsdWinRate,
    double? FromHigh52Week,
    decimal AverageValue20,
    int PricedSessions,
    double Coverage,
    double? BuyQueueShare,
    double? Score,
    double? SinceThen);

public record PerformanceReportDto(
    DateOnly? AsOf,
    string Period,
    DateOnly? From,
    int Sessions,
    double? TedpixReturn,
    double? MedianReturn,
    double? UsdReturn,
    double? CoinReturn,
    int Universe,
    int ShortHistory,
    int Illiquid,
    List<PerformanceRowDto> Rows,
    DateOnly? Latest,
    double? TedpixSinceThen,
    double? UsdSinceThen);

public record SectorReturnDto(
    string SectorCode,
    string? SectorName,
    int Members,
    double? MedianReturn,
    double? AverageReturn,
    double? MedianUsdReturn,
    double PositiveShare,
    double? BeatTedpixShare,
    long? BestInsCode,
    string? BestSymbol,
    double? BestReturn,
    decimal Value20,
    double? MedianSinceThen);

public record SectorReturnsReportDto(
    DateOnly? AsOf,
    string Period,
    DateOnly? From,
    double? TedpixReturn,
    double? UsdReturn,
    List<SectorReturnDto> Sectors,
    DateOnly? Latest,
    double? TedpixSinceThen);

public record SignalMonthDto(DateOnly Date, double Top, double Universe, double Bottom, double InformationCoefficient, double? Turnover);

public record SignalPickDto(long InsCode, string Symbol, string? Name, string? SectorCode, double Value);

/// <summary><paramref name="Verdict"/>: works, eaten-by-costs, reverses, no-edge or insufficient.</summary>
public record SignalVerdictDto(
    string Signal,
    int Months,
    double? TopReturn,
    double? UniverseReturn,
    double? BottomReturn,
    double? Excess,
    double? HitRate,
    double? TStat,
    double? InformationCoefficient,
    double? Turnover,
    double? NetExcess,
    double? AnnualNetExcess,
    string Verdict,
    List<SignalMonthDto> History,
    List<SignalPickDto> Picks);

public record SignalTrackRecordDto(DateOnly? AsOf, DateOnly? From, int Years, double RoundTripCost, List<SignalVerdictDto> Signals);

/// <summary>Twelve monthly returns per series, index 0 = Farvardin; null where a month has no data.</summary>
public record SeasonalityYearDto(int Year, List<double?> Tedpix, List<double?> TedpixUsd, List<double?> EqualWeight, List<double?> Usd);

public record SeasonalityReportDto(DateOnly? AsOf, int? CurrentYear, int? CurrentMonth, List<SeasonalityYearDto> Years);
