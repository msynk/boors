using Boors.Desktop.Infrastructure;
using Boors.Market.Services;
using Boors.Market.Services.Contracts;
using Boors.Market.Services.Jobs;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.Extensions.Logging;

namespace Boors.Desktop.ViewModels;

/// <summary>
/// Data health and operations: dataset freshness, last runs, source reachability, the job queue with live progress,
/// buttons that queue each job and the "sync from node" form. Local mode listens to the in-process queue; remote mode
/// polls the node while the page is open.
/// </summary>
public sealed partial class HealthViewModel : PageViewModel
{
    private readonly MarketJobQueue queue;
    private readonly SettingsService settings;
    private readonly UiDispatcher dispatcher;
    private int refreshQueued;

    public HealthViewModel(IMarketApi api, MarketJobQueue queue, SettingsService settings, UiDispatcher dispatcher, ILogger<HealthViewModel> logger) : base(api, logger)
    {
        this.queue = queue;
        this.settings = settings;
        this.dispatcher = dispatcher;
        NodeUrl = settings.Current.NodeUrl;
        queue.Changed += (_, _) => OnQueueChanged();
    }

    [ObservableProperty]
    public partial MarketDataHealthDto? Health { get; set; }

    [ObservableProperty]
    public partial IReadOnlyList<JobInfoDto> Jobs { get; set; } = [];

    [ObservableProperty]
    public partial JobInfoDto? Running { get; set; }

    [ObservableProperty]
    public partial string? Notice { get; set; }

    [ObservableProperty]
    public partial string NodeUrl { get; set; }

    [ObservableProperty]
    public partial string? SyncToken { get; set; }

    [ObservableProperty]
    public partial DateTimeOffset? SyncSince { get; set; }

    [ObservableProperty]
    public partial string StatusText { get; set; } = string.Empty;

    [ObservableProperty]
    public partial bool Healthy { get; set; }

    /// <summary>The job buttons (MarketJobs.Names, in the boilerplate's order, plus reload).</summary>
    public static IReadOnlyList<Choice> JobChoices => [.. MarketJobs.Names.Select(j => new Choice(j, JobName(j)))];

    public bool IsLocal => settings.Current.Mode != "Remote";

    public Task LoadAsync() => RunAsync(async ct =>
    {
        var healthTask = Api.GetHealthAsync(ct);
        var jobsTask = Api.GetJobsAsync(ct);
        Health = await healthTask;
        SetJobs((await jobsTask).Entries);
        HasData = true;
        Refresh();
    }, "Data health");

    public async Task RunJobAsync(string job)
    {
        try
        {
            var id = await Api.RunJobAsync(job, CancellationToken.None);
            Notice = Loc.F("Queued {0} (#{1})", JobName(job), id);
            await RefreshJobsAsync();
        }
        catch (Exception e)
        {
            Notice = Describe(e);
        }
    }

    public async Task PullAsync()
    {
        try
        {
            var since = SyncSince is { } s ? DateOnly.FromDateTime(s.Date).ToString("yyyy-MM-dd", CultureInfo.InvariantCulture) : null;
            var id = await Api.PullFromNodeAsync(new SyncPullRequestDto(NodeUrl.Trim(), since, string.IsNullOrWhiteSpace(SyncToken) ? null : SyncToken.Trim()), CancellationToken.None);
            Notice = Loc.F("Queued sync from {0} (#{1})", NodeUrl.Trim(), id);
            await RefreshJobsAsync();
        }
        catch (Exception e)
        {
            Notice = Describe(e);
        }
    }

    public bool Cancel(string id) => IsLocal && queue.Cancel(id);

    public async Task RefreshJobsAsync()
    {
        try
        {
            SetJobs((await Api.GetJobsAsync(CancellationToken.None)).Entries);
        }
        catch (Exception e)
        {
            Logger.LogInformation("Job list unavailable: {Message}", e.Message);
        }
    }

    public override void Refresh()
    {
        if (Health is not { } h)
            return;
        Healthy = h.Healthy;
        StatusText = $"{Loc.T("Expected session")} {Fmt.Date(h.ExpectedSession)} · {Loc.T("Node")} {h.Node} · {Fmt.DateTime(h.GeneratedAt)}";
    }

    private void SetJobs(IReadOnlyList<JobInfoDto> entries)
    {
        Jobs = entries;
        Running = entries.FirstOrDefault(e => e.State == "running");
    }

    // Queue events arrive on pool threads, many per second during a backfill: coalesce to one UI refresh at a time.
    private void OnQueueChanged()
    {
        if (!IsLocal || Interlocked.Exchange(ref refreshQueued, 1) == 1)
            return;
        dispatcher.Post(() =>
        {
            Interlocked.Exchange(ref refreshQueued, 0);
            var before = Running?.State;
            SetJobs(queue.Snapshot());
            if (before == "running" && Running is null)
                _ = LoadAsync(); // a job finished: datasets and runs changed
        });
    }

    public static string JobName(string job) => job switch
    {
        "instruments" => Loc.T("Instruments"),
        "indices" => Loc.T("Indices"),
        "backfill" => Loc.T("History backfill"),
        "eod" => Loc.T("End of day"),
        "funds" => Loc.T("Funds"),
        "macro" => Loc.T("Dollar and gold"),
        "codal" => Loc.T("Codal letters"),
        "messages" => Loc.T("Supervisor messages"),
        "halts" => Loc.T("Halts"),
        "reload" => Loc.T("Reload from cache"),
        "sync" => Loc.T("Sync from node"),
        _ => job,
    };

    public static string StateName(string state) => state switch
    {
        "queued" => Loc.T("Queued"),
        "running" => Loc.T("Running"),
        "succeeded" => Loc.T("Succeeded"),
        "failed" => Loc.T("Failed"),
        "cancelled" => Loc.T("Cancelled"),
        "ok" => Loc.T("OK"),
        "partial" => Loc.T("Partial"),
        "skipped" => Loc.T("Skipped"),
        "Ok" => Loc.T("OK"),
        "Stale" => Loc.T("Stale"),
        "Empty" => Loc.T("Empty"),
        "Info" => Loc.T("Info"),
        "reachable" => Loc.T("Reachable"),
        "blocked" => Loc.T("Blocked"),
        "unreachable" => Loc.T("Unreachable"),
        _ => state,
    };

    /// <summary>Status colour class: reserved for health, always shown with an icon and a label.</summary>
    public static StatusKind StatusOf(string? status) => status?.ToLowerInvariant() switch
    {
        "ok" or "succeeded" or "reachable" => StatusKind.Good,
        "stale" or "partial" or "degraded" or "running" or "queued" => StatusKind.Warning,
        "empty" or "failed" or "blocked" or "unreachable" or "cancelled" => StatusKind.Critical,
        _ => StatusKind.Info,
    };
}

public enum StatusKind
{
    Info,
    Good,
    Warning,
    Critical,
}
