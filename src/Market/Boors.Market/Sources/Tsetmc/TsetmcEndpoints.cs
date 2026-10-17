using Boors.Market.RawCache;

namespace Boors.Market.Sources.Tsetmc;

/// <summary>Parameters of the bulk market-watch call; hEven/RefID enable incremental polling.</summary>
public sealed record MarketWatchQuery
{
    /// <summary>PaperType codes from GetStaticData: 1 shares, 2 IFB base market, 3 housing facilities, 4 rights, 5 debt, 6 options, 7 futures, 8 funds, 9 commodity exchange.</summary>
    public IReadOnlyList<int> PaperTypes { get; init; } = [1, 2, 3, 4, 5, 6, 7, 8, 9];

    public int HEven { get; init; }
    public long RefId { get; init; }
    public bool WithBestLimits { get; init; } = true;
    public bool ShowTraded { get; init; }
}

/// <summary>Builds <see cref="RawRequest"/>s for cdn.tsetmc.com. Endpoint names are the API paths, so cache folders read like the API.</summary>
public sealed class TsetmcEndpoints(string baseUrl = TsetmcEndpoints.DefaultBaseUrl)
{
    public const string DefaultBaseUrl = "https://cdn.tsetmc.com/api/";
    public const string Source = "tsetmc";

    /// <summary>TEDPIX (شاخص کل).</summary>
    public const long TedpixInsCode = 32097828799138957;

    private readonly string baseUrl = baseUrl.EndsWith('/') ? baseUrl : baseUrl + "/";

    public RawRequest InstrumentInfo(long insCode) => ForInstrument("Instrument/GetInstrumentInfo", insCode);

    public RawRequest InstrumentIdentity(long insCode) => ForInstrument("Instrument/GetInstrumentIdentity", insCode);

    public RawRequest InstrumentShareChange(long insCode) => ForInstrument("Instrument/GetInstrumentShareChange", insCode);

    public RawRequest InstrumentSearch(string text) =>
        new(Source, "Instrument/GetInstrumentSearch", $"{baseUrl}Instrument/GetInstrumentSearch/{Uri.EscapeDataString(text)}", $"text={text}");

    public RawRequest ClosingPriceInfo(long insCode) => ForInstrument("ClosingPrice/GetClosingPriceInfo", insCode);

    /// <summary>Full daily history (n = 0 means all rows).</summary>
    public RawRequest ClosingPriceDailyList(long insCode, int count = 0) =>
        new(Source, "ClosingPrice/GetClosingPriceDailyList", $"{baseUrl}ClosingPrice/GetClosingPriceDailyList/{Inv(insCode)}/{Inv(count)}", $"insCode={Inv(insCode)}&n={Inv(count)}");

    public RawRequest PriceAdjustList(long insCode) => ForInstrument("ClosingPrice/GetPriceAdjustList", insCode);

    public RawRequest ClientTypeHistory(long insCode) => ForInstrument("ClientType/GetClientTypeHistory", insCode);

    public RawRequest BestLimits(long insCode) => ForInstrument("BestLimits", insCode);

    public RawRequest IndexHistory(long indexInsCode) => ForInstrument("Index/GetIndexB2History", indexInsCode);

    public RawRequest StaticData() => new(Source, "StaticData/GetStaticData", $"{baseUrl}StaticData/GetStaticData", string.Empty);

    public RawRequest MarketWatch(MarketWatchQuery query)
    {
        var parameters = new List<(string Key, string Value)>
        {
            ("market", "0"),
            ("industrialGroup", string.Empty),
        };
        for (var i = 0; i < query.PaperTypes.Count; i++)
            parameters.Add(($"paperTypes[{i}]", Inv(query.PaperTypes[i])));
        parameters.Add(("showTraded", query.ShowTraded ? "true" : "false"));
        parameters.Add(("withBestLimits", query.WithBestLimits ? "true" : "false"));
        parameters.Add(("hEven", Inv(query.HEven)));
        parameters.Add(("RefID", Inv(query.RefId)));

        var queryString = string.Join('&', parameters.Select(p => $"{Uri.EscapeDataString(p.Key)}={Uri.EscapeDataString(p.Value)}"));
        var paramsKey = string.Join('&', parameters.Select(p => $"{p.Key}={p.Value}"));
        return new(Source, "ClosingPrice/GetMarketWatch", $"{baseUrl}ClosingPrice/GetMarketWatch?{queryString}", paramsKey);
    }

    private RawRequest ForInstrument(string endpoint, long insCode) =>
        new(Source, endpoint, $"{baseUrl}{endpoint}/{Inv(insCode)}", $"insCode={Inv(insCode)}");

    private static string Inv(long value) => value.ToString(CultureInfo.InvariantCulture);
}
