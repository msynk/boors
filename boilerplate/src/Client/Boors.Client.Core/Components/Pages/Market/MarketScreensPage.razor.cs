using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

/// <summary>Single-factor equity rankings (Boors.Market ScreenKind); the key is the enum name the API parses.</summary>
public partial class MarketScreensPage
{
    [AutoInject] private IMarketController marketController = default!;

    private bool isLoading;
    private string kind = "Momentum6To1";
    private List<ScreenRowDto> rows = [];
    private List<(string Key, string Title, string Description)> screens = [];

    protected override async Task OnInitAsync()
    {
        await base.OnInitAsync();

        screens =
        [
            ("Momentum6To1", Localizer["Momentum"], Localizer["Return from 6 months ago to 1 month ago. The latest month is skipped because very recent winners tend to reverse."]),
            ("High52WeekProximity", Localizer["Near 52-week high"], Localizer["Last adjusted close divided by the 52-week closing high. 100% means the stock closed at its high."]),
            ("IndividualFlowIntensity20", Localizer["Individual inflow"], Localizer["Individual (retail) net buying over 20 sessions as a share of the value traded in those sessions."]),
            ("BuyerPower5", Localizer["Buyer power"], Localizer["Average per-capita individual buy divided by per-capita individual sell over the last 5 sessions."]),
        ];

        await Load(kind);
    }

    private async Task Load(string key)
    {
        kind = key;
        isLoading = true;
        try
        {
            rows = await marketController.GetScreen(key, 50, CurrentCancellationToken);
        }
        finally
        {
            isLoading = false;
        }
    }

    // Momentum and flow intensity are fractions, proximity is a ratio (1 = at the high), buyer power is a plain ratio.
    private string FormatScore(double score) => kind switch
    {
        "Momentum6To1" or "IndividualFlowIntensity20" => MarketFormat.FractionChange(score),
        "High52WeekProximity" => (score * 100).ToString("N1", CultureInfo.CurrentCulture) + "%",
        _ => score.ToString("N2", CultureInfo.CurrentCulture),
    };

    private void OpenSymbol(long insCode) => NavigationManager.NavigateTo($"{PageUrls.Symbol}/{insCode}");
}
