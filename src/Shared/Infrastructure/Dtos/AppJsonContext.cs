using Boors.Shared.Features.Todo;
using Boors.Shared.Features.PushNotification;
using Boors.Shared.Features.Chatbot;
using Boors.Shared.Infrastructure.Dtos.SignalR;
using Boors.Shared.Features.Statistics;
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
