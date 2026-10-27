using Microsoft.Extensions.Logging;

namespace Boors.Market.Services.Jobs;

/// <summary>
/// Turns the collectors' own log lines ("Backfill: 150/1161 instruments ...") into the running job's progress text,
/// so the operations UI shows live progress without the collectors knowing about the queue.
/// </summary>
public sealed class JobProgressLoggerProvider(MarketJobQueue queue) : ILoggerProvider
{
    private static readonly string[] Categories = ["Boors.Market.Ingestion", "Boors.Market.Services.Sync"];

    public ILogger CreateLogger(string categoryName) =>
        Categories.Any(c => categoryName.StartsWith(c, StringComparison.Ordinal)) ? new ProgressLogger(queue) : NullProgressLogger.Instance;

    public void Dispose()
    {
    }

    private sealed class ProgressLogger(MarketJobQueue queue) : ILogger
    {
        public IDisposable? BeginScope<TState>(TState state) where TState : notnull => null;

        public bool IsEnabled(LogLevel logLevel) => logLevel >= LogLevel.Information;

        public void Log<TState>(LogLevel logLevel, EventId eventId, TState state, Exception? exception, Func<TState, Exception?, string> formatter)
        {
            if (IsEnabled(logLevel))
                queue.ReportProgress(formatter(state, exception));
        }
    }

    private sealed class NullProgressLogger : ILogger
    {
        public static readonly NullProgressLogger Instance = new();

        public IDisposable? BeginScope<TState>(TState state) where TState : notnull => null;

        public bool IsEnabled(LogLevel logLevel) => false;

        public void Log<TState>(LogLevel logLevel, EventId eventId, TState state, Exception? exception, Func<TState, Exception?, string> formatter)
        {
        }
    }
}
