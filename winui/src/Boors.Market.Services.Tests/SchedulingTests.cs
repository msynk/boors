using Boors.Market.Services.Jobs;
using Boors.Market.Text;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;
using Microsoft.Extensions.Time.Testing;

namespace Boors.Market.Services.Tests;

[TestClass]
public sealed class SchedulingTests
{
    private static readonly TimeZoneInfo Iran = IranTime.Zone;

    private static DateTimeOffset IranLocal(int y, int m, int d, int h, int min) => new(y, m, d, h, min, 0, TimeSpan.FromHours(3.5));

    [TestMethod]
    public void EndOfDaySkipsThursdayAndFriday()
    {
        // 2026-10-15 is a Thursday: the next "35 13 * * 6,0-3" run is Saturday 2026-10-17 13:35 Iran time.
        var next = CronExpression.Parse("35 13 * * 6,0-3").GetNextOccurrence(IranLocal(2026, 10, 15, 14, 0), Iran);
        Assert.AreEqual(IranLocal(2026, 10, 17, 13, 35), next);
    }

    [TestMethod]
    public void StepsAndRangesWithinTheDay()
    {
        Assert.AreEqual(IranLocal(2026, 10, 18, 7, 15), CronExpression.Parse("*/15 7-23 * * *").GetNextOccurrence(IranLocal(2026, 10, 18, 7, 1), Iran));
        Assert.AreEqual(IranLocal(2026, 10, 19, 9, 0), CronExpression.Parse("0 9,13,17,21 * * *").GetNextOccurrence(IranLocal(2026, 10, 18, 21, 0), Iran));
        Assert.AreEqual(IranLocal(2026, 10, 18, 8, 30), CronExpression.Parse("*/30 8-18 * * 6,0-3").GetNextOccurrence(IranLocal(2026, 10, 18, 8, 0), Iran));
    }

    [TestMethod]
    public void NextOccurrenceIsStrictlyAfterTheGivenTime()
    {
        var at = IranLocal(2026, 10, 18, 7, 30);
        Assert.AreEqual(IranLocal(2026, 10, 19, 7, 30), CronExpression.Parse("30 7 * * *").GetNextOccurrence(at, Iran));
    }

    [TestMethod]
    public void InvalidExpressionsAreRejected()
    {
        Assert.ThrowsExactly<FormatException>(() => CronExpression.Parse("61 * * * *"));
        Assert.ThrowsExactly<FormatException>(() => CronExpression.Parse("* * *"));
    }

    [TestMethod]
    public void ScheduleIsDescribedWithNextRunsOnlyWhenCollecting()
    {
        var time = new FakeTimeProvider(IranLocal(2026, 10, 18, 13, 0));
        var off = new MarketJobScheduler(new MarketJobQueue(time), time, Options.Create(new MarketDataOptions { Collect = false }), NullLogger<MarketJobScheduler>.Instance);
        Assert.IsTrue(off.Describe().All(s => s.NextRunAt is null));

        var on = new MarketJobScheduler(new MarketJobQueue(time), time, Options.Create(new MarketDataOptions { Collect = true }), NullLogger<MarketJobScheduler>.Instance);
        var eod = on.Describe().Single(s => s.Id == "eod");
        Assert.AreEqual(IranLocal(2026, 10, 18, 13, 35), eod.NextRunAt);
        Assert.HasCount(MarketJobScheduler.Schedule.Count, on.Describe());
    }

    [TestMethod]
    public void QueueDoesNotQueueAWaitingJobTwiceAndListsNewestFirst()
    {
        var queue = new MarketJobQueue(new FakeTimeProvider(DateTimeOffset.UtcNow));
        var first = queue.Enqueue(MarketJobRequest.Collect("eod", "manual"));
        var again = queue.Enqueue(MarketJobRequest.Collect("eod", "schedule"));
        var other = queue.Enqueue(MarketJobRequest.Collect("macro", "manual"));

        Assert.AreEqual(first.Id, again.Id);
        Assert.HasCount(2, queue.Snapshot());
        Assert.AreEqual(other.Id, queue.Snapshot()[0].Id);
    }

    [TestMethod]
    public void CancellingAQueuedJobFinishesIt()
    {
        var queue = new MarketJobQueue(new FakeTimeProvider(DateTimeOffset.UtcNow));
        var job = queue.Enqueue(MarketJobRequest.Collect("codal", "manual"));
        Assert.IsTrue(queue.Cancel(job.Id));
        Assert.AreEqual("cancelled", queue.Find(job.Id)!.State);
        Assert.IsFalse(queue.Cancel(job.Id));
    }
}
