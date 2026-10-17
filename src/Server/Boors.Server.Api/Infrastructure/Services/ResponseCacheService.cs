using Hangfire.Server;
using Microsoft.AspNetCore.OutputCaching;
using Boors.Server.Api.Features.Attachments;
using Boors.Server.Shared.Infrastructure.Services;

namespace Boors.Server.Api.Infrastructure.Services;

/// <summary>
///
/// The default Boors project template includes:
/// 1. `Static` file caching on browsers and CDN edge servers.
/// 2. Caching JSON and dynamic files responses on CDN edge servers and ASP.NET Core's Output Cache by using `AppResponseCache` attribute in controllers like `StatisticsController`, `AttachmentController` and minimal apis.
/// 3. Caching pre-rendered HTML results of Blazor pages on CDN edge servers and ASP.NET Core's Output by using `AppResponseCache` attribute in pages like HomePage.razor
///
/// - Note: Both the output cache and the CDN edge cache are purged by tag, and the tag is the request path alone
///   (See <see cref="AppResponseCachePolicy.CreateCacheTag(string)"/>). So the path passed to <see cref="PurgeCache(string[])"/>
///   must match the request path exactly, while its query string is irrelevant: every variant of the path is purged at once.
/// </summary>
public partial class ResponseCacheService
{
    // Required by AddHttpClient<ResponseCacheService> even when no CDN is configured: the typed client factory only
    // knows how to build this service through a constructor that takes an HttpClient.
    [AutoInject] private HttpClient httpClient = default!;
    [AutoInject] private IOutputCacheStore outputCacheStore = default!;

    /// <summary>For responses tagged by their path, which is the default.</summary>
    public async Task PurgeCache(params string[] relativePaths)
    {
        await PurgeCacheTags([.. relativePaths.Select(AppResponseCachePolicy.CreateCacheTag)]);
    }

    /// <summary>
    /// For responses tagged by an <see cref="AppResponseCacheAttribute.CacheTagTemplate"/>. Both the output cache
    /// entry and the CDN edge entry are stored under the tag (See <see cref="AppResponseCachePolicy"/>).
    /// </summary>
    public async Task PurgeCacheTags(params string[] cacheTags)
    {
        var tags = cacheTags.Distinct().ToArray();

        foreach (var tag in tags)
        {
            await outputCacheStore.EvictByTagAsync(tag, default);
        }
    }

    /// <summary>
    /// The template <see cref="AttachmentController.GetAttachment"/> is tagged by: one tag per attachment rather than
    /// per url, so every kind and every <c>?v=</c> of it is purged together and the tag survives an api version bump.
    /// </summary>
    public const string AttachmentCacheTagTemplate = "Attachment-{attachmentId}";

    /// <summary>Every response of one attachment, product image and profile picture alike.</summary>
    public async Task PurgeAttachmentCache(Guid attachmentId)
    {
        await PurgeCacheTags(AttachmentCacheTagTemplate.Replace("{attachmentId}", attachmentId.ToString()).ToLowerInvariant());
    }


}
