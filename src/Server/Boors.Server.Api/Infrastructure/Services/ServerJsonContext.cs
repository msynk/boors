using Fido2NetLib;
using Boors.Shared.Features.Statistics;
using Boors.Server.Api.Features.Identity.OAuth.Services;

namespace Boors.Server.Api.Infrastructure.Services;

/// <summary>
/// https://devblogs.microsoft.com/dotnet/try-the-new-system-text-json-source-generator/
/// </summary>
[JsonSourceGenerationOptions(
  AllowTrailingCommas = true,
  PropertyNameCaseInsensitive = true,
  GenerationMode = JsonSourceGenerationMode.Default,
  PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase
)]
[JsonSerializable(typeof(NugetStatsDto))]
[JsonSerializable(typeof(AuthenticatorResponse))]
[JsonSerializable(typeof(AuthenticatorAssertionRawResponse))]
[JsonSerializable(typeof(ClientIdMetadataDocument))]
public partial class ServerJsonContext : JsonSerializerContext
{
}
