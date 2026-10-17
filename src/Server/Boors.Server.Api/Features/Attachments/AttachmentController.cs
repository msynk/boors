using ImageMagick;
using FluentStorage.Storage;
using System.Diagnostics.Metrics;
using Boors.Server.Api.Features.Identity;
using Boors.Shared.Features.Attachments;
using Boors.Server.Shared.Infrastructure.Services;

namespace Boors.Server.Api.Features.Attachments;

[ApiController]
[ApiVersion(1)]
[Route("api/v{v:apiVersion}/[controller]/[action]")]
public partial class AttachmentController : AppControllerBase, IAttachmentController
{
    [AutoInject] private IStore blobStorage = default!;
    [AutoInject] private UserManager<User> userManager = default!;

    [AutoInject] private IServiceProvider serviceProvider = default!;
    [AutoInject] private ILogger<AttachmentController> logger = default!;

    [AutoInject] private IHubContext<AppHub> appHubContext = default!;

    [AutoInject] private ResponseCacheService responseCacheService = default!;

    [AutoInject] private IConfiguration configuration = default!;

    /// <summary>The largest upload every endpoint here accepts; the Dev MCP reports this value rather than a copy of it.</summary>
    public const int MaxUploadSizeBytes = 11 * 1024 * 1024;

    // For open telemetry metrics
    private static readonly Histogram<double> updateResizeDurationHistogram = Meter.Current.CreateHistogram<double>("attachment.resize_duration", "ms", "Elapsed time to resize and persist an uploaded image");

    [HttpPost]
    [RequestSizeLimit(MaxUploadSizeBytes)]
    public async Task<IActionResult> UploadUserProfilePicture(IFormFile? file, CancellationToken cancellationToken)
    {
        return await UploadAttachment(
             User.GetUserId(),
             [AttachmentKind.UserProfileImageSmall, AttachmentKind.UserProfileImageOriginal],
             file,
             cancellationToken);
    }


    /// <summary>
    /// Takes an image the user attached to an AI chat message and answers with the id it was stored under, which is
    /// all the client needs to build the attachment url it puts in the message.
    /// <para>
    /// The id is minted here rather than accepted from the client: an attachment id is the whole address of a blob
    /// (See <see cref="GetFilePath(Guid, AttachmentKind)"/>), so a caller-chosen one would let a user overwrite somebody else's attachment -
    /// their profile picture included - by naming its id.
    /// </para>
    /// </summary>
    [HttpPost]
    [RequestSizeLimit(MaxUploadSizeBytes)]
    public async Task<IActionResult> UploadAiChatImage(IFormFile? file, CancellationToken cancellationToken)
    {
        var attachmentId = Guid.CreateSequentialGuid();

        var result = await UploadAttachment(attachmentId, [AttachmentKind.AiChatImage], file, cancellationToken);

        return result is OkObjectResult ? Ok(attachmentId.ToString()) : result;
    }

    [AllowAnonymous]
    [HttpGet("{attachmentId}/{kind}")]
    [AppResponseCache(MaxAge = 3600 * 24 * 7, UserAgnostic = true, SkipOutputCache = true, CacheTagTemplate = ResponseCacheService.AttachmentCacheTagTemplate)]
    public async Task<IActionResult> GetAttachment(Guid attachmentId, AttachmentKind kind, CancellationToken cancellationToken = default)
    {
        // If the backend is hosted behind a CDN (which is recommended for production), the GetAttachment method's returned stream will be cached on CDN edge servers.
        // Alternatively, you can generate URLs that allow clients to download files directly from the file storage, further reducing the load on the backend.
        // If security is a concern, you can generate short-lived signed URLs for the file storage. These signed URLs can be validated either at the CDN edge or on the file storage server, ensuring that only authorized users can access the files.

        var filePath = GetFilePath(attachmentId, kind);

        if (await blobStorage.ObjectExists(filePath, cancellationToken) is false)
            throw new ResourceNotFoundException().WithData("Reason", "The attachment does not exist.");

        var mimeType = kind switch
        {
            AttachmentKind.AiChatImage => "image/webp",
            AttachmentKind.UserProfileImageSmall => "image/webp",
            // Only the *Original kinds keep the uploaded format, so only they pay for the lookup - the kinds every
            // rendered page asks for are answered above without touching the database.
            _ => await DbContext.Attachments.Where(att => att.Id == attachmentId && att.Kind == kind)
                                            .Select(att => att.ContentType)
                                            .FirstOrDefaultAsync(cancellationToken) ?? "application/octet-stream"
        };

        return File(await blobStorage.OpenRead(filePath, cancellationToken), mimeType, enableRangeProcessing: true);
    }

    [HttpDelete]
    public async Task DeleteUserProfilePicture(CancellationToken cancellationToken)
    {
        await DeleteAttachment(User.GetUserId(), [AttachmentKind.UserProfileImageSmall, AttachmentKind.UserProfileImageOriginal], cancellationToken);
    }


    private async Task PublishUserProfileUpdated(User user, CancellationToken cancellationToken)
    {
        // Notify other sessions of the user that user's info has been updated, so they'll update their UI.
        var currentUserSessionId = User.GetSessionId();
        var userSessionIdsExceptCurrentUserSessionId = await DbContext.UserSessions
            .Where(us => us.UserId == user.Id && us.Id != currentUserSessionId && us.SignalRConnectionId != null)
            .Select(us => us.SignalRConnectionId!)
            .ToArrayAsync(cancellationToken);
        await appHubContext.Clients.Clients(userSessionIdsExceptCurrentUserSessionId).Publish(SharedAppMessages.PROFILE_UPDATED, user.Map(), cancellationToken);
    }

    private async Task DeleteAttachment(Guid attachmentId, AttachmentKind[] kinds, CancellationToken cancellationToken)
    {
        var attachments = await DbContext.Attachments.Where(p => p.Id == attachmentId && kinds.Contains(p.Kind)).ToArrayAsync(cancellationToken);

        foreach (var attachment in attachments)
        {
            var filePath = attachment.Path;

            if (await blobStorage.ObjectExists(filePath, cancellationToken) is false)
                throw new ResourceNotFoundException(Localizer[nameof(AppStrings.ImageCouldNotBeFound)]);

            await blobStorage.DeleteSingleObject(filePath, cancellationToken);


            if (attachment.Kind is AttachmentKind.UserProfileImageOriginal)
            {
                var user = await userManager.FindByIdAsync(User.GetUserId().ToString());
                user!.HasProfilePicture = false;

                var result = await userManager.UpdateAsync(user);
                if (!result.Succeeded)
                    throw new ResourceValidationException(result.Errors.Select(err => new LocalizedString(err.Code, err.Description)).ToArray());

                await PublishUserProfileUpdated(user, cancellationToken);
            }

            DbContext.Attachments.Remove(attachment);
            await DbContext.SaveChangesAsync(cancellationToken);
        }

        // After the loop: every kind shares one tag, and by now no blob is left for a racing read to re-cache.
        await responseCacheService.PurgeAttachmentCache(attachmentId);
    }

    private async Task<IActionResult> UploadAttachment(Guid attachmentId, AttachmentKind[] kinds, IFormFile? file, CancellationToken cancellationToken)
    {
        if (file is null)
            throw new BadRequestException().WithData("Reason", "No file provided.");

        string? altText = null; // AI-generated alt text, when the analysis agent is configured.

        var preparedUploads = new List<(Attachment Attachment, byte[] Bytes)>();

        foreach (var kind in kinds)
        {
            var attachment = new Attachment
            {
                Id = attachmentId,
                Kind = kind,
                Path = GetFilePath(attachmentId, kind),
                CreatedOn = TimeProvider.GetUtcNow(),
            };

            // ShrinkOnly makes the size a ceiling instead of a floor: the picture is only scaled down to it, and one
            // that is already smaller is stored exactly as it arrived rather than rejected or blown up.
            (bool NeedsResize, uint Width, uint Height, bool ShrinkOnly) imageResizeContext = kind switch
            {
                AttachmentKind.UserProfileImageSmall => (true, 256, 256, false),
                // Whatever the user had on screen when they attached it - a crop, a screenshot, a phone photo - so
                // there is no size below which it is not worth showing the model.
                AttachmentKind.AiChatImage => (true, 512, 512, true),
                _ => (false, 0, 0, false)
            };

            Stopwatch stopwatch = Stopwatch.StartNew();

            // Every kind is decoded, including the ones that are not resized: stripping the metadata means re-encoding,
            // so the *Original kinds keep the uploaded FORMAT rather than the uploaded bytes.
            // Process-wide ImageMagick ResourceLimits are configured at startup (Program.Services.cs), so what a
            // decode of an untrusted upload can cost is bounded, and anything Magick.NET cannot decode throws here.
            // OpenReadStream hands out a NEW stream per call and MagickImage does not take ownership of it.
            using var sourceStream = file.OpenReadStream();
            MagickImage? decodedImage;
            try
            {
                decodedImage = new(sourceStream);
            }
            catch (MagickException)
            {
                // An undecodable upload is bad input, not a server fault - a 400 like the endpoint's other rejections,
                // instead of a 500 with a Critical log per attempt. Only the decode is caught: a failure in the resize
                // or encode below would be a server problem and stays loud.
                return BadRequest(Localizer[nameof(AppStrings.UnsupportedImageFormat)].ToString());
            }
            using MagickImage sourceImage = decodedImage;

            if (imageResizeContext.NeedsResize)
            {
                if (imageResizeContext.ShrinkOnly is false &&
                    (sourceImage.Width < imageResizeContext.Width || sourceImage.Height < imageResizeContext.Height))
                    return BadRequest(Localizer[nameof(AppStrings.ImageTooSmall), imageResizeContext.Width, imageResizeContext.Height, sourceImage.Width, sourceImage.Height].ToString());

                sourceImage.Resize(new MagickGeometry(imageResizeContext.Width, imageResizeContext.Height) { Greater = imageResizeContext.ShrinkOnly });
            }

            // Drops EXIF - GPS coordinates, capture time, camera serial - along with XMP, IPTC, comments and the ICC
            // profile. A phone photo carries all of it, and the WebP re-encode below does NOT drop it on its own.
            sourceImage.Strip();

            var storedBytes = imageResizeContext.NeedsResize
                ? sourceImage.ToByteArray(MagickFormat.WebP)
                : sourceImage.ToByteArray();

            // The format these bytes really are, which for the *Original kinds is the only record of it.
            attachment.ContentType = imageResizeContext.NeedsResize
                ? "image/webp"
                : MagickFormatInfo.Create(sourceImage.Format)?.MimeType;

            updateResizeDurationHistogram.Record(stopwatch.Elapsed.TotalMilliseconds, new KeyValuePair<string, object?>("kind", kind.ToString()));


            preparedUploads.Add((attachment, storedBytes));
        }

        //  ---------------------------------------------------------------------------------------------
        //  PHASE 2 - everything validated; now mutate.
        //
        //  A re-upload updates the rows in place instead of deleting and re-inserting them: Attachment's key is
        //  composite - { Id, Kind } (AttachmentConfiguration) - and EF cannot track a second instance with a key it
        //  already tracks, even when the tracked one is marked Deleted. Only ContentType can actually differ,
        //  since GetFilePath is a pure function of those same two values.
        //
        //  For the same reason there is no stale blob to clean up: the new key IS the old key, so the writes
        //  below overwrite in place.
        //  ---------------------------------------------------------------------------------------------
        var existingAttachments = await DbContext.Attachments
            .Where(att => att.Id == attachmentId)
            .ToDictionaryAsync(att => att.Kind, cancellationToken);

        foreach (var (attachment, _) in preparedUploads)
        {
            if (existingAttachments.TryGetValue(attachment.Kind, out var existingAttachment))
            {
                existingAttachment.ContentType = attachment.ContentType;
            }
            else
            {
                await DbContext.Attachments.AddAsync(attachment, cancellationToken);
            }
        }

        await DbContext.SaveChangesAsync(cancellationToken);

        var wroteAnyBlob = false;

        try
        {
            foreach (var (attachment, storedBytes) in preparedUploads)
            {
                // SetBytes would leave the content type to be guessed from the key, which not every kind names.
                using MemoryStream storedStream = new(storedBytes);
                await blobStorage.SetObject(attachment.Path, storedStream, attachment.ContentType, cancellationToken: cancellationToken);
                wroteAnyBlob = true;
            }
        }
        finally
        {
            // The replacement is stored under the very key the old bytes were (See GetFilePath), so nothing else
            // invalidates the copies already on the edge and in browsers. In the finally because a second kind that
            // fails still leaves the first one replaced, and the edge still holding what it replaced.
            if (wroteAnyBlob)
            {
                await responseCacheService.PurgeAttachmentCache(attachmentId);
            }
        }


        if (kinds.Contains(AttachmentKind.UserProfileImageSmall))
        {
            var user = await userManager.FindByIdAsync(User.GetUserId().ToString());
            user!.HasProfilePicture = true;

            var result = await userManager.UpdateAsync(user);
            if (!result.Succeeded)
                throw new ResourceValidationException(result.Errors.Select(err => new LocalizedString(err.Code, err.Description)).ToArray());

            await PublishUserProfileUpdated(user, cancellationToken);
        }

        return Ok(altText);
    }

    /// <summary>
    /// A pure function of { Id, Kind }: no part of the key comes from the uploaded file, so a png -> jpg re-upload
    /// lands on the very key it replaces and leaves no blob behind that no row names. The *Original kinds therefore
    /// carry no extension, and <c>Attachment.ContentType</c> is what records the format they were stored in.
    /// <br/>
    /// Environment variables are expanded over the CONFIGURED directory prefix only, never over anything the client
    /// influenced - a file name ending ".%TEMP%" used to expand a server environment value into the storage key.
    /// </summary>
    private string GetFilePath(Guid attachmentId, AttachmentKind kind) => GetFilePath(AppSettings, attachmentId, kind);

    /// <inheritdoc cref="GetFilePath(Guid, AttachmentKind)"/>
    /// <remarks>
    /// Static so that whoever needs a blob can work out where it is without asking this controller or the database
    /// for it - <c>AppChatbot</c> reads an attached image straight off storage this way.
    /// </remarks>
    public static string GetFilePath(ServerApiSettings appSettings, Guid attachmentId, AttachmentKind kind)
    {
        var directory = kind switch
        {
            AttachmentKind.AiChatImage => appSettings.AiChatImagesDir,
            AttachmentKind.UserProfileImageSmall or AttachmentKind.UserProfileImageOriginal => appSettings.UserProfileImagesDir,
            _ => throw new NotImplementedException()
        };

        directory = Environment.ExpandEnvironmentVariables(directory);

        return kind switch
        {
            AttachmentKind.AiChatImage => $"{directory}{attachmentId}_{kind}.webp",
            AttachmentKind.UserProfileImageSmall => $"{directory}{attachmentId}_{kind}.webp",
            _ => $"{directory}{attachmentId}_{kind}" // The *Original kinds; their format lives in Attachment.ContentType.
        };
    }

    public record AIImageReviewResponse(bool IsCar, double Confidence, string? Alt, string? Reasoning);
}
