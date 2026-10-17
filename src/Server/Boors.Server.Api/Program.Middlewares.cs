
using Hangfire.Storage;
using Scalar.AspNetCore;
using Microsoft.IdentityModel.Tokens;
using Boors.Server.Api.Features.Identity.OAuth;
using Boors.Server.Api.Features.Identity.OAuth.Services;
using Boors.Server.Api.Features.Identity;
using Boors.Server.Api.Features.Attachments;
using Boors.Server.Api.Features.PushNotification;

namespace Boors.Server.Api;

public static partial class Program
{
    /// <summary>
    /// https://learn.microsoft.com/en-us/aspnet/core/fundamentals/middleware/?view=aspnetcore-9.0#middleware-order
    /// </summary>
    private static void ConfigureMiddlewares(this WebApplication app)
    {
        var configuration = app.Configuration;
        var env = app.Environment;

        ServerApiSettings settings = new();
        configuration.Bind(settings);

        app.UseAppForwardedHeaders();

        app.UseLocalization();

        app.UseExceptionHandler();

        if (env.IsDevelopment() is false)
        {
            app.UseHttpsRedirection();
            app.UseResponseCompression();

            app.UseSecurityHeaders();
        }

        if (env.IsDevelopment())
        {
            app.UseDirectoryBrowser();
        }

        app.UseStaticFiles();

        app.UseCors();

        app.UseMiddleware<ForceUpdateMiddleware>();

        app.UseAuthentication();
        app.UseRateLimiter(); // After UseAuthentication, so rate limit partitions can use HttpContext.User.
        app.UseAuthorization();

        app.UseOutputCache();

        app.UseAntiforgery();

        app.MapAppHealthChecks(OAuthEndpoints.AuthorizationFor(OAuthResources.HealthzPath));

        app.MapOpenApi().CacheOutput("AppResponseCachePolicy");
        app.MapScalarApiReference().CacheOutput("AppResponseCachePolicy");
        app.MapGet("/", () => Results.Redirect("/scalar")).ExcludeFromDescription();
        app.MapGet("/swagger", () => Results.Redirect("/scalar")).ExcludeFromDescription();

        app.UseHangfireDashboard(options: new()
        {
            DarkModeEnabled = true,
            Authorization = [new HangfireDashboardAuthorizationFilter()]
        });

        app.ScheduleAppRecurringJobs();

        app.MapGet("/api/minimal-api-sample/{routeParameter}", [AppResponseCache(MaxAge = 3600 * 24)] (string routeParameter, [FromQuery] string queryStringParameter) => new
        {
            RouteParameter = routeParameter,
            QueryStringParameter = queryStringParameter
        }).WithTags("Test").CacheOutput("AppResponseCachePolicy").ExcludeFromDescription();

        app.MapHub<Infrastructure.SignalR.AppHub>("/app-hub", options => options.AllowStatefulReconnects = true);

        // Chatbot tools. Isolated from /dev-mcp. Served under the api version the controllers carry as well.
        foreach (var path in new[] { OAuthResources.McpPath, $"{OAuthResources.McpPath}/v1" })
            app.MapMcp(path).RequireAuthorization(OAuthEndpoints.AuthorizationFor(OAuthResources.McpPath));

        // The feature AND two factor, for the app's own bearer scheme or a token issued for this resource; every
        // requirement is read off OAuthResources (OAuthEndpoints.AuthorizationFor).
        app.MapMcp(OAuthResources.DevMcpPath).RequireAuthorization(OAuthEndpoints.AuthorizationFor(OAuthResources.DevMcpPath));

        app.MapOpenIdConfiguration();

        app.MapOAuthEndpoints();

        app.MapControllers()
           .RequireAuthorization()
           .CacheOutput("AppResponseCachePolicy");
    }


    /// <summary>
    /// Recurring hangfire jobs. AddOrUpdate is idempotent and keyed by job id, so re-running it on every start (and
    /// on every replica) simply re-applies the current schedule.
    /// </summary>
    public static WebApplication ScheduleAppRecurringJobs(this WebApplication app)
    {
        var recurringJobManager = app.Services.GetRequiredService<IRecurringJobManager>();

        List<string> scheduled = [];

        recurringJobManager.AddOrUpdate<UserSessionsRetentionJobRunner>(UserSessionsRetentionJobRunner.RecurringJobId,
                                                                       runner => runner.EnforceRetention(CancellationToken.None),
                                                                       Cron.Daily);
        scheduled.Add(UserSessionsRetentionJobRunner.RecurringJobId);

        recurringJobManager.AddOrUpdate<UnconfirmedUsersRetentionJobRunner>(UnconfirmedUsersRetentionJobRunner.RecurringJobId,
                                                                           runner => runner.EnforceRetention(CancellationToken.None),
                                                                           Cron.Daily);
        scheduled.Add(UnconfirmedUsersRetentionJobRunner.RecurringJobId);

        recurringJobManager.AddOrUpdate<OAuthRetentionJobRunner>(OAuthRetentionJobRunner.RecurringJobId,
                                                                runner => runner.EnforceRetention(CancellationToken.None),
                                                                Cron.Daily);
        scheduled.Add(OAuthRetentionJobRunner.RecurringJobId);

        recurringJobManager.AddOrUpdate<PushSubscriptionsRetentionJobRunner>(PushSubscriptionsRetentionJobRunner.RecurringJobId,
                                                                            runner => runner.EnforceRetention(CancellationToken.None),
                                                                            Cron.Daily);
        scheduled.Add(PushSubscriptionsRetentionJobRunner.RecurringJobId);

        recurringJobManager.AddOrUpdate<AiChatImagesRetentionJobRunner>(AiChatImagesRetentionJobRunner.RecurringJobId,
                                                                       runner => runner.EnforceRetention(CancellationToken.None),
                                                                       Cron.Hourly);
        scheduled.Add(AiChatImagesRetentionJobRunner.RecurringJobId);

        app.ScheduleMarketDataJobs(recurringJobManager, scheduled);

        app.RemoveUnscheduledRecurringJobs(scheduled);

        return app;
    }

    /// <summary>
    /// Market-data collection, in Iran time. Trading days are Saturday-Wednesday (cron 6,0-3); equities close with the
    /// closing-price stage at 13:00 and non-equity ETFs at 18:00 (from 18 Mehr 1405), so end of day runs twice.
    /// Scheduled only when MarketData:Collect is true; otherwise the jobs are removed as unscheduled.
    /// </summary>
    private static void ScheduleMarketDataJobs(this WebApplication app, IRecurringJobManager recurringJobManager, List<string> scheduled)
    {
        var options = app.Services.GetRequiredService<IOptions<Boors.Market.MarketDataOptions>>().Value;
        if (options.Collect is false)
            return;

        var iran = new RecurringJobOptions { TimeZone = Boors.Market.Text.IranTime.Zone };
        void Schedule(string job, string cron, string? suffix = null)
        {
            var id = $"{Features.MarketData.MarketDataJobRunner.RecurringJobPrefix}{job}{suffix}";
            recurringJobManager.AddOrUpdate<Features.MarketData.MarketDataJobRunner>(id, runner => runner.Run(job, CancellationToken.None), cron, iran);
            scheduled.Add(id);
        }

        Schedule("instruments", "30 7 * * *");
        Schedule("eod", "35 13 * * 6,0-3");
        Schedule("eod", "35 18 * * 6,0-3", ".etf");
        Schedule("messages", "*/30 8-18 * * 6,0-3");
        Schedule("codal", "*/15 7-23 * * *");
        Schedule("macro", "0 9,13,17,21 * * *");
        Schedule("funds", "0 20 * * *");
        Schedule("indices", "15 1 * * *");
        Schedule("backfill", "0 2 * * *");
        Schedule("halts", "0 3 * * *");
    }

    /// <summary>
    /// A schedule outlives the class it names. Every job id here is nameof(TheRunner), and AddOrUpdate only ever adds,
    /// so renaming a runner - or switching off the feature that registered it - leaves the old id in storage, where
    /// Hangfire retries it until it gives up and reports a job it can no longer load.
    /// </summary>
    private static void RemoveUnscheduledRecurringJobs(this WebApplication app, IReadOnlyCollection<string> scheduled)
    {
        try
        {
            var recurringJobManager = app.Services.GetRequiredService<IRecurringJobManager>();
            using var connection = app.Services.GetRequiredService<JobStorage>().GetConnection();

            foreach (var orphan in connection.GetRecurringJobs().Select(job => job.Id).Except(scheduled).ToArray())
            {
                recurringJobManager.RemoveIfExists(orphan);
                app.Logger.LogWarning("Removed recurring job {RecurringJobId}: it is no longer registered by ScheduleAppRecurringJobs.", orphan);
            }
        }
        catch (Exception exception)
        {
            // Tidying old schedules must never be what stops the app from starting.
            app.Logger.LogError(exception, "Could not prune unregistered recurring jobs.");
        }
    }

    /// <summary>
    /// This allows other backends to retrieve the OpenID Connect configuration and the public key for validating JWT tokens issued by this server.
    /// Checkout AppCertificate.md for more information.
    /// </summary>
    public static WebApplication MapOpenIdConfiguration(this WebApplication app)
    {
        var jwks = AppCertificateService.GetPublicSecurityKeys(app.Configuration)
            .Select(publicKey =>
            {
                var jwk = JsonWebKeyConverter.ConvertFromRSASecurityKey(publicKey);
                jwk.Use = "sig";
                jwk.Alg = SecurityAlgorithms.RsaSha256;
                return jwk;
            })
            .ToArray();

        app.MapGet("/.well-known/openid-configuration", (HttpRequest request) =>
        {
            var baseUrl = request.GetBaseUrl();
            return new
            {
                // Must be derived exactly as AppJwtSecureDataFormat derives it when minting, or a consumer that trusts
                // this document rejects every token it was published to validate.
                issuer = request.GetIssuer(),
                jwks_uri = new Uri(baseUrl, ".well-known/jwks"),
                id_token_signing_alg_values_supported = new[] { SecurityAlgorithms.RsaSha256 },
            };
        });

        app.MapGet("/.well-known/jwks", () =>
        {
            return new
            {
                keys = jwks
            };
        });

        return app;
    }
}
