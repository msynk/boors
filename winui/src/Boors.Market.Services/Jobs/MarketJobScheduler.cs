using Boors.Market.Services.Contracts;
using Boors.Market.Text;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Boors.Market.Services.Jobs;

/// <summary>
/// The boilerplate's Hangfire schedule, in Iran time (Asia/Tehran). Trading days are Saturday–Wednesday (cron 6,0-3);
/// equities close at 13:00 and non-equity ETFs at 18:00, so end of day runs twice. Active only when
/// <see cref="MarketDataOptions.Collect"/> is true; jobs land in the same single-writer <see cref="MarketJobQueue"/>.
/// </summary>
public sealed class MarketJobScheduler(
    MarketJobQueue queue,
    TimeProvider timeProvider,
    IOptions<MarketDataOptions> options,
    ILogger<MarketJobScheduler> logger) : BackgroundService
{
    public static readonly IReadOnlyList<(string Id, string Job, string Cron)> Schedule =
    [
        ("instruments", "instruments", "30 7 * * *"),
        ("eod", "eod", "35 13 * * 6,0-3"),
        ("eod.etf", "eod", "35 18 * * 6,0-3"),
        ("messages", "messages", "*/30 8-18 * * 6,0-3"),
        ("codal", "codal", "*/15 7-23 * * *"),
        ("macro", "macro", "0 9,13,17,21 * * *"),
        ("funds", "funds", "0 20 * * *"),
        ("indices", "indices", "15 1 * * *"),
        ("backfill", "backfill", "0 2 * * *"),
        ("halts", "halts", "0 3 * * *"),
    ];

    private static readonly IReadOnlyList<(string Id, string Job, CronExpression Cron)> Parsed =
        [.. Schedule.Select(s => (s.Id, s.Job, CronExpression.Parse(s.Cron)))];

    public bool Enabled => options.Value.Collect;

    /// <summary>The schedule with the next run of each entry (null when scheduling is off).</summary>
    public IReadOnlyList<ScheduledJobDto> Describe()
    {
        var now = timeProvider.GetUtcNow();
        return [.. Schedule.Zip(Parsed).Select(x => new ScheduledJobDto(
            x.First.Id, x.First.Job, x.First.Cron, Enabled ? x.Second.Cron.GetNextOccurrence(now, IranTime.Zone) : null))];
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!Enabled)
        {
            logger.LogInformation("Market-data collection is off (MarketData:Collect=false); no jobs are scheduled.");
            return;
        }

        logger.LogInformation("Scheduling {Count} market-data jobs in {Zone}", Parsed.Count, IranTime.Zone.Id);
        while (!stoppingToken.IsCancellationRequested)
        {
            var now = timeProvider.GetUtcNow();
            var due = Parsed
                .Select(p => (p.Id, p.Job, At: p.Cron.GetNextOccurrence(now, IranTime.Zone)))
                .Where(p => p.At is not null)
                .OrderBy(p => p.At)
                .ToList();
            if (due.Count == 0)
                return;

            var next = due[0].At!.Value;
            var delay = next - now;
            if (delay > TimeSpan.Zero)
                await Task.Delay(delay, timeProvider, stoppingToken);

            foreach (var job in due.Where(d => d.At == next))
            {
                logger.LogInformation("Scheduled job {ScheduleId} is due; queuing {Job}", job.Id, job.Job);
                queue.Enqueue(MarketJobRequest.Collect(job.Job, "schedule"));
            }
        }
    }
}

/// <summary>
/// A five-field cron expression (minute hour day-of-month month day-of-week) with <c>*</c>, <c>*/n</c>, ranges,
/// stepped ranges and lists; day-of-week 0 or 7 is Sunday. Evaluated in a given time zone's wall-clock time.
/// </summary>
public sealed class CronExpression
{
    private readonly bool[] minutes;
    private readonly bool[] hours;
    private readonly bool[] days;
    private readonly bool[] months;
    private readonly bool[] weekdays;
    private readonly bool anyDay;
    private readonly bool anyWeekday;

    private CronExpression(bool[] minutes, bool[] hours, bool[] days, bool[] months, bool[] weekdays, bool anyDay, bool anyWeekday)
    {
        (this.minutes, this.hours, this.days, this.months, this.weekdays, this.anyDay, this.anyWeekday) = (minutes, hours, days, months, weekdays, anyDay, anyWeekday);
    }

    public static CronExpression Parse(string expression)
    {
        var fields = expression.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (fields.Length != 5)
            throw new FormatException($"Cron '{expression}' must have 5 fields.");

        var weekdays = Field(fields[4], 0, 7);
        weekdays[0] |= weekdays[7];
        return new CronExpression(Field(fields[0], 0, 59), Field(fields[1], 0, 23), Field(fields[2], 1, 31), Field(fields[3], 1, 12), weekdays[..7],
            fields[2] == "*", fields[4] == "*");
    }

    /// <summary>The first matching minute strictly after <paramref name="after"/>, within the next 366 days.</summary>
    public DateTimeOffset? GetNextOccurrence(DateTimeOffset after, TimeZoneInfo zone)
    {
        var local = TimeZoneInfo.ConvertTime(after, zone);
        var start = new DateTime(local.Year, local.Month, local.Day, local.Hour, local.Minute, 0, DateTimeKind.Unspecified).AddMinutes(1);
        for (var day = start.Date; day < start.Date.AddDays(366); day = day.AddDays(1))
        {
            if (!months[day.Month] || !DayMatches(day))
                continue;

            for (var h = day == start.Date ? start.Hour : 0; h < 24; h++)
            {
                if (!hours[h])
                    continue;
                for (var m = day == start.Date && h == start.Hour ? start.Minute : 0; m < 60; m++)
                {
                    if (minutes[m])
                    {
                        var wallClock = day.AddHours(h).AddMinutes(m);
                        if (zone.IsInvalidTime(wallClock))
                            continue;
                        return new DateTimeOffset(wallClock, zone.GetUtcOffset(wallClock));
                    }
                }
            }
        }

        return null;
    }

    // Standard cron: when both day fields are restricted, either may match.
    private bool DayMatches(DateTime day) =>
        anyDay && anyWeekday ? true
        : anyDay ? weekdays[(int)day.DayOfWeek]
        : anyWeekday ? days[day.Day]
        : days[day.Day] || weekdays[(int)day.DayOfWeek];

    private static bool[] Field(string field, int min, int max)
    {
        var set = new bool[max + 1];
        foreach (var part in field.Split(','))
        {
            var (range, step) = part.Split('/') is [var r, var s] ? (r, int.Parse(s, CultureInfo.InvariantCulture)) : (part, 1);
            var (from, to) = range == "*" ? (min, max)
                : range.Split('-') is [var a, var b] ? (int.Parse(a, CultureInfo.InvariantCulture), int.Parse(b, CultureInfo.InvariantCulture))
                : (int.Parse(range, CultureInfo.InvariantCulture), int.Parse(range, CultureInfo.InvariantCulture));
            if (from < min || to > max || from > to || step < 1)
                throw new FormatException($"Cron field '{field}' is out of range {min}-{max}.");
            for (var v = from; v <= to; v += step)
                set[v] = true;
        }

        return set;
    }
}
