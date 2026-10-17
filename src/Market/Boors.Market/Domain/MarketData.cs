namespace Boors.Market.Domain;

/// <summary>One unadjusted daily bar from TSETMC's closing-price history. Prices are rials.</summary>
public sealed record DailyBar
{
    public required long InsCode { get; init; }
    public required DateOnly Date { get; init; }

    /// <summary>First trade price (priceFirst).</summary>
    public decimal Open { get; init; }

    /// <summary>Day high (priceMax).</summary>
    public decimal High { get; init; }

    /// <summary>Day low (priceMin).</summary>
    public decimal Low { get; init; }

    /// <summary>Official closing price (pClosing). VWAP-based on the main markets from 2025-12-22.</summary>
    public decimal Close { get; init; }

    /// <summary>Last trade price (pDrCotVal).</summary>
    public decimal Last { get; init; }

    /// <summary>Reference price of the previous day (priceYesterday); limits are computed from it.</summary>
    public decimal Yesterday { get; init; }

    /// <summary>Number of trades (zTotTran). Zero on halted and market-closed days.</summary>
    public long TradeCount { get; init; }

    /// <summary>Shares traded (qTotTran5J). Use this, not the tape, for daily totals.</summary>
    public long Volume { get; init; }

    /// <summary>Rial value traded (qTotCap).</summary>
    public decimal Value { get; init; }

    public TimeOnly? LastTradeTime { get; init; }
}

/// <summary>Individual (حقیقی, "I") vs institutional (حقوقی, "N") flows for one symbol-day.</summary>
public sealed record ClientTypeDay
{
    public required long InsCode { get; init; }
    public required DateOnly Date { get; init; }

    public long BuyIndividualVolume { get; init; }
    public long BuyInstitutionalVolume { get; init; }
    public decimal BuyIndividualValue { get; init; }
    public decimal BuyInstitutionalValue { get; init; }
    public int BuyIndividualCount { get; init; }
    public int BuyInstitutionalCount { get; init; }

    public long SellIndividualVolume { get; init; }
    public long SellInstitutionalVolume { get; init; }
    public decimal SellIndividualValue { get; init; }
    public decimal SellInstitutionalValue { get; init; }
    public int SellIndividualCount { get; init; }
    public int SellInstitutionalCount { get; init; }

    /// <summary>Individual buy value minus individual sell value (rials).</summary>
    public decimal IndividualNetValue => BuyIndividualValue - SellIndividualValue;

    /// <summary>Average individual buy ticket (سرانه خرید), or null when no individual buyers.</summary>
    public decimal? PerCapitaBuy => BuyIndividualCount > 0 ? BuyIndividualValue / BuyIndividualCount : null;

    /// <summary>Average individual sell ticket (سرانه فروش), or null when no individual sellers.</summary>
    public decimal? PerCapitaSell => SellIndividualCount > 0 ? SellIndividualValue / SellIndividualCount : null;

    /// <summary>Buyer power (قدرت خریدار) = per-capita buy / per-capita sell.</summary>
    public decimal? BuyerPower => PerCapitaBuy is { } b && PerCapitaSell is { } s && s != 0 ? b / s : null;
}

/// <summary>
/// A corporate-action price adjustment (capital increase or dividend). TSETMC reports the adjusted and unadjusted
/// close of the event day; <see cref="Factor"/> multiplies prices before the event to make them comparable.
/// </summary>
public sealed record PriceAdjustment
{
    public required long InsCode { get; init; }
    public required DateOnly Date { get; init; }
    public decimal AdjustedClose { get; init; }
    public decimal UnadjustedClose { get; init; }

    public decimal Factor => UnadjustedClose == 0 ? 1m : AdjustedClose / UnadjustedClose;
}

public sealed record ShareChange
{
    public required long InsCode { get; init; }
    public required DateOnly Date { get; init; }
    public decimal SharesBefore { get; init; }
    public decimal SharesAfter { get; init; }
}

public sealed record IndexBar
{
    public required long InsCode { get; init; }
    public required DateOnly Date { get; init; }
    public decimal Close { get; init; }
    public decimal Low { get; init; }
    public decimal High { get; init; }
}

/// <summary>One order-book level. Demand = bids (MeDem), offer = asks (MeOf).</summary>
public sealed record BestLimitLevel
{
    public int Level { get; init; }
    public decimal BidPrice { get; init; }
    public long BidVolume { get; init; }
    public int BidOrders { get; init; }
    public decimal AskPrice { get; init; }
    public long AskVolume { get; init; }
    public int AskOrders { get; init; }
}

/// <summary>Live state of one symbol from GetClosingPriceInfo.</summary>
public sealed record ClosingPriceSnapshot
{
    public required long InsCode { get; init; }
    public DateOnly? Date { get; init; }
    public TimeOnly? Time { get; init; }
    public decimal Last { get; init; }
    public decimal Close { get; init; }
    public decimal Open { get; init; }
    public decimal Low { get; init; }
    public decimal High { get; init; }
    public decimal Yesterday { get; init; }
    public long TradeCount { get; init; }
    public long Volume { get; init; }
    public decimal Value { get; init; }

    /// <summary>cEtaval, trimmed: "A" = permitted (مجاز), "IS" = halted (ممنوع-متوقف).</summary>
    public string? State { get; init; }

    public string? StateTitle { get; init; }

    public bool IsHalted => State is not null && State != "A";
}

/// <summary>
/// One row of the market watch. Compact wire keys are mapped to names here; the mapping is inferred from value
/// matching against GetClosingPriceInfo and is verified by <c>boors verify mw-keys</c>.
/// </summary>
public sealed record MarketWatchRow
{
    public required long InsCode { get; init; }
    public required string Symbol { get; init; }
    public string? Name { get; init; }
    public string? Isin { get; init; }
    public string? SectorCode { get; init; }

    public decimal Last { get; init; }
    public decimal Close { get; init; }
    public decimal Open { get; init; }
    public decimal Low { get; init; }
    public decimal High { get; init; }
    public decimal Yesterday { get; init; }

    /// <summary>Upper allowed price for the day (pMax).</summary>
    public decimal AllowedHigh { get; init; }

    /// <summary>Lower allowed price for the day (pMin).</summary>
    public decimal AllowedLow { get; init; }

    public long TradeCount { get; init; }
    public long Volume { get; init; }
    public decimal Value { get; init; }
    public long BaseVolume { get; init; }
    public decimal TotalShares { get; init; }
    public decimal? Eps { get; init; }
    public decimal? PriceEarnings { get; init; }
    public int VisitCount { get; init; }
    public TimeOnly? LastTradeTime { get; init; }
    public int HEven { get; init; }
    public IReadOnlyList<BestLimitLevel> BestLimits { get; init; } = [];

    public InstrumentClass Class => InstrumentClassifier.ClassifyByIsin(Isin);
}

public sealed record StaticDataItem(int Id, int Code, string Type, string Name, string? Description);
