namespace Boors.Shared.Features.Attachments;

public enum AttachmentKind
{
    /// <summary>
    /// Resized to fit within 256*256px, preserving the aspect ratio.
    /// </summary>
    UserProfileImageSmall = 0,
    UserProfileImageOriginal = 1,
    /// <summary>
    /// An image the user attached to a message in the AI chat panel.
    /// </summary>
    AiChatImage = 4
}
