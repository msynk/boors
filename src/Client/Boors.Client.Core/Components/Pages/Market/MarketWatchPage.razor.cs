using Boors.Shared.Features.Market;

namespace Boors.Client.Core.Components.Pages.Market;

public partial class MarketWatchPage
{
    /// <summary>Instrument class groups offered as toggle buttons; keys are rules.yaml class keys.</summary>
    private static readonly Dictionary<string, string[]?> ClassGroups = new()
    {
        ["shares"] = ["tse_stock", "ifb_stock", "ifb_base_yellow", "ifb_base_orange", "ifb_base_red"],
        ["funds"] = ["etf_equity", "etf_fixed_income", "etf_real_estate", "etf_mixed", "fund_leveraged", "fund_gold_silver", "fund_of_funds"],
        ["rights"] = ["rights"],
        ["all"] = null,
    };

    [AutoInject] private IMarketController marketController = default!;

    private bool isLoading;
    private bool tradedOnly = true;
    private string? group = "shares";
    private string? filter;
    private string? errorMessage;
    private MarketWatchResultDto? result;
    private List<MarketWatchEntryDto> entries = [];
    private List<BitButtonGroupItem> groups = [];
    private List<BitDropdownItem<string>> presets = [];

    protected override async Task OnInitAsync()
    {
        await base.OnInitAsync();

        groups =
        [
            new() { Key = "shares", Text = Localizer["Shares"] },
            new() { Key = "funds", Text = Localizer["Funds"] },
            new() { Key = "rights", Text = Localizer["Rights"] },
            new() { Key = "all", Text = Localizer["All"] },
        ];

        // Common TSETMC-style filters; the text goes into the filter box so it can be edited before applying.
        presets =
        [
            new() { Text = Localizer["Closed more than 3% up"], Value = "(pc)>(py)*1.03" },
            new() { Text = Localizer["Buy queue at the upper limit"], Value = "(pd1)==(tmax) && (qd1)>0" },
            new() { Text = Localizer["Sell queue at the lower limit"], Value = "(po1)==(tmin) && (qo1)>0" },
            new() { Text = Localizer["Buyer power above 2"], Value = "(ct).Buy_CountI>0 && (ct).Sell_CountI>0 && ((ct).Buy_I_Volume/(ct).Buy_CountI)/((ct).Sell_I_Volume/(ct).Sell_CountI)>2" },
            new() { Text = Localizer["Individuals bought more than they sold"], Value = "(ct).Buy_I_Volume>(ct).Sell_I_Volume*1.2" },
            new() { Text = Localizer["Volume above 3× base volume"], Value = "(bvol)>0 && (tvol)>(bvol)*3" },
            new() { Text = Localizer["Last price above close"], Value = "(pl)>(pc) && (pl)>(py)" },
            new() { Text = Localizer["P/E between 0 and 6"], Value = "(pe)>0 && (pe)<6" },
            new() { Text = Localizer["New 20-session closing high"], Value = "true==function(){if([ih].length<19)return false;for(var i=0;i<19;i++){if([ih][i].PClosing>=(pc))return false;}return true;}()" },
        ];

        await Load();
    }

    private async Task ApplyPreset(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
            return;
        filter = value;
        await Load();
    }

    private async Task ClearFilter()
    {
        filter = null;
        await Load();
    }

    private async Task Load()
    {
        isLoading = true;
        errorMessage = null;
        StateHasChanged();
        try
        {
            var classes = group is not null && ClassGroups.TryGetValue(group, out var keys) && keys is not null ? keys.ToList() : null;
            result = await marketController.GetMarketWatch(new MarketWatchRequestDto(null, string.IsNullOrWhiteSpace(filter) ? null : filter, classes, tradedOnly), CurrentCancellationToken);
            entries = result.Entries;
        }
        catch (KnownException exception)
        {
            // A syntax error in the filter is a user mistake, not a crash: show it next to the filter box.
            errorMessage = exception.Message;
            entries = [];
        }
        finally
        {
            isLoading = false;
        }
    }

    private void OpenSymbol(long insCode) => NavigationManager.NavigateTo($"{PageUrls.Symbol}/{insCode}");
}
