namespace Boors.Client.Core.Components.Pages.Market;

/// <summary>
/// Number, date and colour conventions shared by the market pages. Up/down follow the Iranian convention (green up,
/// red down) and always carry a sign and an arrow as well, so meaning never depends on colour alone.
/// </summary>
public static class MarketFormat
{
    /// <summary>همت = 1,000 billion toman = 10^13 rial, the unit Iranian market reports use for value traded.</summary>
    public const decimal RialsPerHemat = 10_000_000_000_000m;

    /// <summary>1 billion toman = 10^10 rial.</summary>
    public const decimal RialsPerBillionToman = 10_000_000_000m;

    public static bool IsRightToLeft => CultureInfo.CurrentUICulture.TextInfo.IsRightToLeft;

    public static BitDir Direction => IsRightToLeft ? BitDir.Rtl : BitDir.Ltr;

    public static string Hemat(decimal? rials) => rials is { } r ? (r / RialsPerHemat).ToString("N2", CultureInfo.CurrentCulture) : "–";

    public static string BillionToman(decimal? rials) => rials is { } r ? (r / RialsPerBillionToman).ToString("N1", CultureInfo.CurrentCulture) : "–";

    public static string MillionToman(decimal? rials) => rials is { } r ? (r / 10_000_000m).ToString("N0", CultureInfo.CurrentCulture) : "–";

    public static string Price(decimal? value) => value is { } v ? v.ToString("N0", CultureInfo.CurrentCulture) : "–";

    public static string Number(long? value) => value is { } v ? v.ToString("N0", CultureInfo.CurrentCulture) : "–";

    public static string Ratio(decimal? value) => value is { } v ? v.ToString("N2", CultureInfo.CurrentCulture) : "–";

    /// <summary>
    /// A percentage (2.5 = 2.5%) with an explicit sign and an arrow: "▲ +2.50%". The whole run is isolated as
    /// left-to-right (U+2066…U+2069) so a Persian page does not reorder it into "2.50%+ ▲".
    /// </summary>
    public static string Change(double? percent) => percent switch
    {
        null => "–",
        > 0.005 => $"⁦▲ +{percent.Value.ToString("N2", CultureInfo.CurrentCulture)}%⁩",
        < -0.005 => $"⁦▼ {percent.Value.ToString("N2", CultureInfo.CurrentCulture)}%⁩",
        _ => $"⁦● {0.0.ToString("N2", CultureInfo.CurrentCulture)}%⁩",
    };

    /// <summary>A fraction (0.25 = 25%) as a signed percentage.</summary>
    public static string FractionChange(double? fraction) => Change(fraction * 100);

    public static BitColor ChangeColor(double? value) => value switch
    {
        > 0.005 => BitColor.Success,
        < -0.005 => BitColor.Error,
        _ => BitColor.SecondaryForeground,
    };

    public static BitColor FlowColor(decimal? value) => value switch
    {
        > 0 => BitColor.Success,
        < 0 => BitColor.Error,
        _ => BitColor.SecondaryForeground,
    };

    /// <summary>
    /// Jalali (yyyy/MM/dd) in a Persian UI — CultureInfoManager gives the fa culture the Persian calendar — and ISO
    /// otherwise.
    /// </summary>
    public static string Date(DateOnly? date) => date switch
    {
        null => "–",
        { } d when CultureInfo.CurrentCulture.Calendar is PersianCalendar => d.ToString("yyyy/MM/dd", CultureInfo.CurrentCulture),
        { } d => d.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture),
    };

    /// <summary>Theme colour tokens for charts (BitChart accepts CSS variables).</summary>
    public static string Up => $"var({BitCss.Var.Color.Success.Main})";

    public static string Down => $"var({BitCss.Var.Color.Error.Main})";

    public static string Neutral => $"var({BitCss.Var.Color.Foreground.Secondary.Main})";

    public static string Warning => $"var({BitCss.Var.Color.Warning.Main})";

    public static string Accent => $"var({BitCss.Var.Color.Primary.Main})";

    public static string ClassName(string instrumentClass, IStringLocalizer localizer) => instrumentClass switch
    {
        "tse_stock" => localizer["TSE share"],
        "ifb_stock" => localizer["IFB share"],
        "ifb_base_yellow" or "ifb_base_orange" or "ifb_base_red" => localizer["Base market"],
        "rights" => localizer["Rights"],
        "etf_equity" => localizer["Equity fund"],
        "etf_fixed_income" => localizer["Fixed-income fund"],
        "etf_real_estate" => localizer["Real-estate fund"],
        "etf_mixed" => localizer["Mixed fund"],
        "fund_leveraged" => localizer["Leveraged fund"],
        "fund_gold_silver" => localizer["Gold fund"],
        "fund_of_funds" => localizer["Fund of funds"],
        "option" => localizer["Option"],
        "debt" => localizer["Bond / sukuk"],
        _ => localizer["Other"],
    };

    public static string DayStatus(string? status, IStringLocalizer localizer) => status switch
    {
        "Traded" => localizer["Traded"],
        "NoTrade" => localizer["No trade"],
        "SymbolHalted" => localizer["Halted"],
        "MarketClosed" => localizer["Market closed"],
        "ReopenAuction" => localizer["Reopening auction"],
        "QueueLockedUp" => localizer["Buy queue all day"],
        "QueueLockedDown" => localizer["Sell queue all day"],
        null => "–",
        _ => status,
    };
}
