using Bunit;
using Microsoft.AspNetCore.Components.Authorization;
using Boors.Client.Core.Components.Pages.Market;
using Boors.Shared.Features.Market;
using Boors.Shared.Features.MarketData;

namespace Boors.Tests.Features.Market;

/// <summary>
/// Renders the market pages in-memory with bUnit against faked <see cref="IMarketController"/>,
/// <see cref="IInstrumentController"/> and <see cref="IMarketDataController"/>, so they need no market-data store and
/// make no HTTP calls. They check that each page binds its data (KPIs, grids, charts, feeds) and handles the empty and
/// error paths; they do not prove pixel layout, which needs a real browser.
/// </summary>
// Each test builds its own server; building several at once races on the in-memory Hangfire storage, hence no
// parallelism within the class and the same retry the template's own bUnit tests use.
[TestClass, TestCategory("UITest"), DoNotParallelize, Retry(2)]
public class MarketPagesBunitTests
{
    private static readonly DateOnly Session = new(2026, 10, 14);
    private const long Foolad = 46348559193224090;

    private readonly IMarketController market = A.Fake<IMarketController>();
    private readonly IInstrumentController instrument = A.Fake<IInstrumentController>();
    private readonly IMarketDataController marketData = A.Fake<IMarketDataController>();

    [TestMethod]
    public async Task Overview_Should_ShowKpisChartsAndMovers()
    {
        A.CallTo(() => market.GetOverview(A<string?>._, A<CancellationToken>._)).Returns(Overview());
        A.CallTo(() => market.GetBreadth(A<int>._, A<CancellationToken>._)).Returns(
            [new BreadthDayDto(Session.AddDays(-1), 200, 300, 40, 60, 5, 10, 20, 50_000_000_000_000m), new BreadthDayDto(Session, 310, 190, 40, 60, 5, 30, 4, 60_000_000_000_000m)]);
        A.CallTo(() => market.GetSectors(A<string?>._, A<CancellationToken>._)).Returns(
            [new SectorPerformanceDto("27", "Basic metals", 30, 28, 9_000_000_000_000m, 0.012, 0.008, 400_000_000_000m, 20, 8)]);
        A.CallTo(() => market.GetIndexHistory(A<long>._, A<string?>._, A<CancellationToken>._)).Returns(
            [new SeriesPointDto(Session.AddDays(-1), 8_400_000m), new SeriesPointDto(Session, 8_265_157m)]);
        A.CallTo(() => market.GetMacroHistory(A<string>._!, A<string?>._, A<CancellationToken>._)).Returns(
            [new SeriesPointDto(Session.AddDays(-1), 1_000_000m), new SeriesPointDto(Session, 1_010_000m)]);
        A.CallTo(() => market.GetMovers(A<string>._!, A<int>._, A<string?>._, A<CancellationToken>._)).Returns(
            [new MoverDto(Foolad, "فولاد", "Mobarakeh Steel", "tse_stock", "Traded", 5_120m, 2.4, 3_000_000_000_000m, -120_000_000_000m)]);

        await using var server = new AppTestServer();
        await using var ctx = CreateContext(server);

        var cut = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<MarketOverviewPage>());

        await cut.WaitForAssertionAsync(() =>
        {
            Assert.Contains("8,265,157", cut.Markup, "The TEDPIX tile shows the index level.");
            Assert.Contains("▼ -1.91%", cut.Markup, "Index change carries a sign and an arrow, not colour alone.");
            Assert.Contains("فولاد", cut.Markup, "The movers grid lists the symbol.");
            Assert.IsNotEmpty(cut.FindAll("svg"), "Charts render as SVG.");
        }, timeout: TimeSpan.FromSeconds(10));

        // The movers grid opens on the "value" list.
        A.CallTo(() => market.GetMovers("value", 15, null, A<CancellationToken>._)).MustHaveHappened();
    }

    [TestMethod]
    public async Task Overview_Should_ExplainWhenThereIsNoData()
    {
        A.CallTo(() => market.GetOverview(A<string?>._, A<CancellationToken>._)).Returns(Task.FromResult<MarketOverviewDto?>(null));

        await using var server = new AppTestServer();
        await using var ctx = CreateContext(server);

        var cut = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<MarketOverviewPage>());

        await cut.WaitForAssertionAsync(() => Assert.Contains("No market data yet", cut.Markup), timeout: TimeSpan.FromSeconds(10));
        A.CallTo(() => market.GetMovers(A<string>._!, A<int>._, A<string?>._, A<CancellationToken>._)).MustNotHaveHappened();
    }

    [TestMethod]
    public async Task MarketWatch_Should_ListRows_And_ShowFilterErrors()
    {
        A.CallTo(() => market.GetMarketWatch(A<MarketWatchRequestDto>._, A<CancellationToken>._)).Returns(
            new MarketWatchResultDto(Session, [WatchEntry()], 412, ["[ih][59] is undefined for 3 new listings"]));

        await using var server = new AppTestServer();
        await using var ctx = CreateContext(server);

        var cut = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<MarketWatchPage>());

        await cut.WaitForAssertionAsync(() =>
        {
            Assert.Contains("فولاد", cut.Markup);
            Assert.Contains("[ih][59] is undefined", cut.Markup, "Per-instrument filter errors are listed, not swallowed.");
        }, timeout: TimeSpan.FromSeconds(10));

        // The first load asks for traded shares only.
        A.CallTo(() => market.GetMarketWatch(A<MarketWatchRequestDto>.That.Matches(r => r.TradedOnly && r.Classes!.Contains("tse_stock") && r.Filter == null), A<CancellationToken>._))
            .MustHaveHappenedOnceExactly();

        // A syntax error from the server is shown next to the filter box instead of crashing the page.
        A.CallTo(() => market.GetMarketWatch(A<MarketWatchRequestDto>._, A<CancellationToken>._)).Throws(new BadRequestException("Filter error: Unexpected token ')'"));
        cut.Find("textarea").Change("(pc)>)");
        cut.FindAll("button").First(b => b.TextContent.Contains("Apply")).Click();

        await cut.WaitForAssertionAsync(() => Assert.Contains("Unexpected token", cut.Markup), timeout: TimeSpan.FromSeconds(10));
    }

    [TestMethod]
    public async Task Symbol_Should_ShowMetricsChartsAndFeeds()
    {
        A.CallTo(() => instrument.GetMetrics(Foolad, A<CancellationToken>._)).Returns(new InstrumentMetricsDto(
            Foolad, "فولاد", "Mobarakeh Steel", "tse_stock", "27", "Basic metals", Session, "Traded", 5_120m, 2.4, 1_500_000_000_000_000m,
            6_000m, 2_800m, -14.7, new() { ["1w"] = 0.02, ["1m"] = -0.05, ["3m"] = 0.1, ["6m"] = 0.3, ["1y"] = 1.14 }, -0.118,
            2_500_000_000_000m, -900_000_000_000m, 120_000_000m, 80_000_000m, 1.5m, 0.82));
        A.CallTo(() => instrument.GetBars(Foolad, A<string?>._, A<CancellationToken>._)).Returns(
        [
            new BarDto(Session.AddDays(-1), 5_000m, 5_100m, 4_950m, 5_000m, 5_000m, 1_000_000, 5_000_000_000m, 900, "Traded", 0.01),
            new BarDto(Session, 5_010m, 5_200m, 5_000m, 5_120m, 5_120m, 1_200_000, 6_100_000_000m, 1_100, "Traded", 0.024),
        ]);
        A.CallTo(() => instrument.GetFlows(Foolad, A<string?>._, A<CancellationToken>._)).Returns(
            [new FlowDayDto(Session, 4_000_000_000m, 4_500_000_000m, 2_100_000_000m, 1_600_000_000m, 300, 250, -500_000_000m, 13_333_333m, 18_000_000m, 0.74m, false)]);
        A.CallTo(() => instrument.GetLetters(Foolad, A<int>._, A<CancellationToken>._)).Returns(
            [new CodalLetterDto(1, "Monthly activity report, Mehr 1405", "ن-30", 58, new DateTimeOffset(2026, 10, 12, 9, 0, 0, TimeSpan.FromHours(3.5)), null, true, "https://codal.ir/Reports/Decision.aspx?LetterSerial=x", "https://excel.codal.ir/x", null)]);
        A.CallTo(() => instrument.GetMessages(Foolad, A<int>._, A<CancellationToken>._)).Returns(
            [new SupervisorMessageDto(9, Session, "Reopening of فولاد", "Trading resumes after the general meeting.", "Reopen", "GeneralMeeting", true)]);

        await using var server = new AppTestServer();
        await using var ctx = CreateContext(server);

        var cut = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<SymbolPage>(page => page.Add(x => x.InsCode, Foolad)));

        await cut.WaitForAssertionAsync(() =>
        {
            Assert.Contains("Mobarakeh Steel", cut.Markup);
            Assert.Contains("▲ +114.00%", cut.Markup, "The 1-year rial return is shown as a signed percentage.");
            Assert.Contains("▼ -11.80%", cut.Markup, "The 1-year dollar return is shown next to it.");
            Assert.IsGreaterThanOrEqualTo(3, cut.FindAll("svg").Count, "Price, value and flow charts render.");
        }, timeout: TimeSpan.FromSeconds(10));

        // Feeds live in pivot tabs that mount on selection.
        cut.FindAll("[role=tab]").First(tab => tab.TextContent.Contains("Codal disclosures")).Click();

        await cut.WaitForAssertionAsync(() => Assert.Contains("Monthly activity report", cut.Markup), timeout: TimeSpan.FromSeconds(10));
        Assert.Contains(a => a.GetAttribute("href") == "https://codal.ir/Reports/Decision.aspx?LetterSerial=x" && a.GetAttribute("rel")?.Contains("noopener") is true,
            cut.FindAll("a"), "Codal letters link out with rel=noopener.");
    }

    [TestMethod]
    public async Task Screens_And_Health_Should_Render()
    {
        A.CallTo(() => market.GetScreen(A<string>._!, A<int>._, A<CancellationToken>._)).Returns(
            [new ScreenRowDto(Foolad, "فولاد", "Mobarakeh Steel", "tse_stock", "27", 0.42, 120)]);
        A.CallTo(() => marketData.GetHealth(A<CancellationToken>._)).Returns(new MarketDataHealthDto(
            DateTimeOffset.UtcNow, "iran-1", Session, false,
            [new DatasetHealthDto("daily_bar", 812_345, Session, Session, "Ok"), new DatasetHealthDto("codal_letter", 4_210, Session.AddDays(-3), Session, "Stale")],
            [new CollectionRunDto("eod", "iran-1", DateTimeOffset.UtcNow.AddMinutes(-5), DateTimeOffset.UtcNow.AddMinutes(-4), "Succeeded", 820, 0, null)],
            null, []));

        await using var server = new AppTestServer();
        await using var ctx = CreateContext(server);

        var screens = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<MarketScreensPage>());
        await screens.WaitForAssertionAsync(() => Assert.Contains("▲ +42.00%", screens.Markup, "Momentum is a fraction shown as a signed percentage."), timeout: TimeSpan.FromSeconds(10));
        A.CallTo(() => market.GetScreen("Momentum6To1", 50, A<CancellationToken>._)).MustHaveHappenedOnceExactly();

        var health = ctx.Render<CascadingAuthenticationState>(p => p.AddChildContent<MarketHealthPage>());
        await health.WaitForAssertionAsync(() =>
        {
            Assert.Contains("daily_bar", health.Markup);
            Assert.Contains("Stale", health.Markup);
            Assert.Contains("Needs attention", health.Markup);
        }, timeout: TimeSpan.FromSeconds(10));
    }

    private BunitContext CreateContext(AppTestServer server)
    {
        // Only the service container is needed: the controllers are faked, so the server is built but never started.
        server.Build(services =>
        {
            services.RemoveAll<IMarketController>();
            services.RemoveAll<IInstrumentController>();
            services.RemoveAll<IMarketDataController>();
            services.AddScoped(_ => market);
            services.AddScoped(_ => instrument);
            services.AddScoped(_ => marketData);
        });

        return server.CreateBunitContext();
    }

    private static MarketOverviewDto Overview() => new(
        Session,
        [new IndexMoveDto(32097828799138957, "TEDPIX", 8_265_157m, -1.91)],
        EquityValue: 55_000_000_000_000m,
        FundValue: 120_000_000_000_000m,
        EquityTrades: 900_000,
        Advancers: 310,
        Decliners: 190,
        Unchanged: 40,
        NotTraded: 60,
        Halted: 5,
        BuyQueues: 30,
        BuyQueueValue: 4_000_000_000_000m,
        SellQueues: 4,
        SellQueueValue: 300_000_000_000m,
        IndividualNetFlow: -1_200_000_000_000m,
        IndividualPerCapitaBuy: 150_000_000m,
        IndividualPerCapitaSell: 110_000_000m,
        FlowValuesEstimated: true,
        Usd: new MacroQuoteDto("price_dollar_rl", Session, 1_010_000m, 1.0),
        Coin: null,
        TedpixUsdChangePercent: -3.37);

    private static MarketWatchEntryDto WatchEntry() => new(
        Foolad, "فولاد", "Mobarakeh Steel", "tse_stock", "27", "Traded", 5_120m, 2.4, 5_150m, 3.0, 1_200_000, 6_100_000_000m, 1_100,
        4_850m, 5_150m, "buy", 900_000_000m, 7.1m, 1_500_000_000_000_000m, -500_000_000m, 13_333_333m, 18_000_000m, 0.74m, 0.6);
}
