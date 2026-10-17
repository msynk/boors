using Boors.Market;
using Boors.Market.Cli;
using Boors.Market.Sources;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

var cli = new CliArgs(args);

if (cli.Positional.Count == 0 || cli.Has("help"))
{
    Console.WriteLine("""
        boors — Boors market-data developer CLI

        Usage: boors <command> [arguments] [options]

        Commands:
          probe                                  Check reachability of every data source from this node.
          fetch instruments [--details] [--limit N]
                                                 Instrument master from the market watch; --details adds GetInstrumentInfo per instrument.
          fetch history <symbol|insCode>         Daily history, flows, adjustments and share changes, with day-status classification.
          fetch marketwatch                      Market-watch snapshot with queue counts.
          fetch index [insCode]                  Index history (default TEDPIX) and detected market closures.
          fetch static                           TSETMC static data (sector and paper-type codes).
          fetch codal-categories                 Codal letter taxonomy (categories and LetterType ids).
          fetch codal-letters <symbol> [--type id] [--page n]
                                                 Codal letters for a symbol.
          verify mw-keys [--count N]             Check the inferred market-watch compact keys against GetClosingPriceInfo/GetInstrumentInfo.

        Options:
          --data <dir>     Data root for the raw cache and outputs (default: .boors-data)
          --node <name>    Collector node name recorded on every raw entry (default: machine name)
          --cache          Use cached responses when present (resume a long fetch)
          --offline        Use cached responses only; never touch the network
          --verbose        Log every request
        """);
    return 0;
}

var builder = Host.CreateApplicationBuilder(new HostApplicationBuilderSettings { Args = [] });
builder.Logging.ClearProviders();
builder.Logging.AddSimpleConsole(o => o.SingleLine = true);
builder.Logging.SetMinimumLevel(cli.Has("verbose") ? LogLevel.Debug : LogLevel.Warning);
builder.Logging.AddFilter("System.Net.Http", cli.Has("verbose") ? LogLevel.Information : LogLevel.Warning);

builder.Services.AddBoorsMarketData(options =>
{
    options.DataRoot = cli.Get("data") ?? Environment.GetEnvironmentVariable("BOORS_DATA") ?? ".boors-data";
    options.Node = cli.Get("node") ?? Environment.GetEnvironmentVariable("BOORS_NODE") ?? Environment.MachineName;
});
builder.Services.AddHttpClient(ProbeCommand.HttpClientName)
    .ConfigurePrimaryHttpMessageHandler(ProbeCommand.CreateHandler);
builder.Services.AddTransient<ProbeCommand>();
builder.Services.AddTransient<FetchCommands>();
builder.Services.AddTransient<VerifyCommands>();

using var host = builder.Build();
using var cancellation = new CancellationTokenSource();
Console.CancelKeyPress += (_, e) =>
{
    e.Cancel = true;
    cancellation.Cancel();
};

var policy = cli.Has("offline") ? CachePolicy.CacheOnly : cli.Has("cache") ? CachePolicy.PreferCache : CachePolicy.Network;
var services = host.Services;
var token = cancellation.Token;

try
{
    return (cli.At(0), cli.At(1)) switch
    {
        ("probe", _) => await services.GetRequiredService<ProbeCommand>().RunAsync(token),
        ("fetch", "instruments") => await services.GetRequiredService<FetchCommands>().InstrumentsAsync(cli, policy, token),
        ("fetch", "history") => await services.GetRequiredService<FetchCommands>().HistoryAsync(cli, policy, token),
        ("fetch", "marketwatch") => await services.GetRequiredService<FetchCommands>().MarketWatchAsync(policy, token),
        ("fetch", "index") => await services.GetRequiredService<FetchCommands>().IndexAsync(cli, policy, token),
        ("fetch", "static") => await services.GetRequiredService<FetchCommands>().StaticDataAsync(policy, token),
        ("fetch", "codal-categories") => await services.GetRequiredService<FetchCommands>().CodalCategoriesAsync(policy, token),
        ("fetch", "codal-letters") => await services.GetRequiredService<FetchCommands>().CodalLettersAsync(cli, policy, token),
        ("verify", "mw-keys") => await services.GetRequiredService<VerifyCommands>().MarketWatchKeysAsync(cli, policy, token),
        _ => Unknown(),
    };
}
catch (OperationCanceledException) when (token.IsCancellationRequested)
{
    Console.Error.WriteLine("Cancelled.");
    return 130;
}
catch (Exception e) when (e is SourceRequestException or NotCachedException or HttpRequestException or Polly.ExecutionRejectedException)
{
    Console.Error.WriteLine($"error: {e.Message}");
    return 2;
}

int Unknown()
{
    Console.Error.WriteLine($"Unknown command '{string.Join(' ', cli.Positional)}'. Run 'boors --help'.");
    return 1;
}
