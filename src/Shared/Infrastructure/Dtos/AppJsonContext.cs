using Boors.Shared.Features.Todo;
using Boors.Shared.Features.PushNotification;
using Boors.Shared.Features.Chatbot;
using Boors.Shared.Infrastructure.Dtos.SignalR;
using Boors.Shared.Features.Statistics;
using Boors.Shared.Features.MarketData;
using Boors.Shared.Features.Market;
using Boors.Shared.Features.Diagnostic;
using Boors.Shared.Features.Identity.OAuth.Dtos;
using Boors.Shared.Features.Tenants.Dtos;
using CommunityToolkit.Datasync.Server.Abstractions.Json;

namespace Boors.Shared.Infrastructure.Dtos;

/// <summary>
/// https://devblogs.microsoft.com/dotnet/try-the-new-system-text-json-source-generator/
/// </summary>
[JsonSourceGenerationOptions(

  Converters = [
    typeof(DateTimeConverter),
    typeof(DateTimeOffsetConverter),
    typeof(TimeOnlyConverter)
  ],

  AllowTrailingCommas = true,
  PropertyNameCaseInsensitive = true,
  GenerationMode = JsonSourceGenerationMode.Default,
  PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase

)]


[JsonSerializable(typeof(Dictionary<string, JsonElement>))]
[JsonSerializable(typeof(Dictionary<string, string?>))]
[JsonSerializable(typeof(TimeSpan))]
[JsonSerializable(typeof(string[]))]
[JsonSerializable(typeof(Guid[]))]
[JsonSerializable(typeof(IAsyncEnumerable<string>))] // DiagnosticController.PerformDiagnostic
[JsonSerializable(typeof(GitHubStats))]
[JsonSerializable(typeof(NugetStatsDto))]
[JsonSerializable(typeof(MarketDataHealthDto))]
[JsonSerializable(typeof(MarketOverviewDto))]
[JsonSerializable(typeof(List<BreadthDayDto>))]
[JsonSerializable(typeof(List<SectorPerformanceDto>))]
[JsonSerializable(typeof(List<MoverDto>))]
[JsonSerializable(typeof(List<ScreenRowDto>))]
[JsonSerializable(typeof(List<InstrumentSearchResultDto>))]
[JsonSerializable(typeof(MarketWatchRequestDto))]
[JsonSerializable(typeof(MarketWatchResultDto))]
[JsonSerializable(typeof(InstrumentMetricsDto))]
[JsonSerializable(typeof(List<BarDto>))]
[JsonSerializable(typeof(List<FlowDayDto>))]
[JsonSerializable(typeof(List<CorporateEventDto>))]
[JsonSerializable(typeof(List<ShareholderDto>))]
[JsonSerializable(typeof(List<CodalLetterDto>))]
[JsonSerializable(typeof(List<SupervisorMessageDto>))]
[JsonSerializable(typeof(AppProblemDetails))]
[JsonSerializable(typeof(HealthReportDto))]
[JsonSerializable(typeof(DeploymentConfigurationDto))]
[JsonSerializable(typeof(OAuthAuthorizeRequestDto))]
[JsonSerializable(typeof(OAuthConsentDto))]
[JsonSerializable(typeof(OAuthApprovalDto))]
[JsonSerializable(typeof(OAuthClientDto))]
[JsonSerializable(typeof(List<OAuthClientDto>))]
[JsonSerializable(typeof(RevokeOAuthClientRequestDto))]
[JsonSerializable(typeof(PushNotificationSubscriptionDto))]
[JsonSerializable(typeof(TodoItemDto))]
[JsonSerializable(typeof(PagedResponse<TodoItemDto>))]
[JsonSerializable(typeof(List<TodoItemDto>))]

[JsonSerializable(typeof(TenantDto))]
[JsonSerializable(typeof(List<TenantDto>))]
[JsonSerializable(typeof(PagedResponse<TenantDto>))]
[JsonSerializable(typeof(InviteUserToTenantRequestDto))]
[JsonSerializable(typeof(DiagnosticLogDto[]))]
[JsonSerializable(typeof(StartChatRequest))]
[JsonSerializable(typeof(AiChatMessage))]
[JsonSerializable(typeof(AiChatCard))]
[JsonSerializable(typeof(AssistantTurn))]
[JsonSerializable(typeof(List<SystemPromptDto>))]
[JsonSerializable(typeof(BackgroundJobProgressDto))]
[JsonSerializable(typeof(SynthesizeSpeechRequestDto))]
[JsonSerializable(typeof(TranscribeSpeechResponseDto))]
[JsonSerializable(typeof(StartVoiceCallRequestDto))]
[JsonSerializable(typeof(StartVoiceCallResponseDto))]
public partial class AppJsonContext : JsonSerializerContext
{
}
