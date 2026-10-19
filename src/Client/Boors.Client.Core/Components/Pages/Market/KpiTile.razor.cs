namespace Boors.Client.Core.Components.Pages.Market;

public partial class KpiTile
{
    [Parameter, EditorRequired] public string Caption { get; set; } = default!;

    [Parameter, EditorRequired] public string Value { get; set; } = default!;

    /// <summary>Colours the value itself, for tiles whose headline number is a change (green up, red down).</summary>
    [Parameter] public BitColor? ValueColor { get; set; }

    [Parameter] public string? ChangeText { get; set; }

    [Parameter] public BitColor ChangeColor { get; set; } = BitColor.SecondaryForeground;

    [Parameter] public string? Note { get; set; }
}
