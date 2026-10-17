using Boors.Market.Analytics;
using Boors.Shared.Features.Market;

namespace Boors.Server.Api.Features.Market;

/// <summary>
/// Boors.Market read models → shared DTOs. More info at src/Server/Boors.Server.Api/Features/Mappers.md
/// </summary>
[Mapper]
public static partial class MarketMapper
{
    public static partial MarketOverviewDto Map(this MarketOverview source);

    public static partial BreadthDayDto Map(this BreadthDay source);

    public static partial SectorPerformanceDto Map(this SectorPerformance source);

    public static partial MoverDto Map(this Mover source);

    public static partial ScreenRowDto Map(this ScreenRow source);

    public static partial MarketWatchEntryDto Map(this MarketWatchEntry source);

    public static partial InstrumentMetricsDto Map(this InstrumentMetrics source);
}
