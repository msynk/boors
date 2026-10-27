using Boors.Shared.Features.PushNotification;

namespace Boors.Server.Api.Features.PushNotification;

/// <summary>
/// More info at src/Server/Boors.Server.Api/Features/Mappers.md
/// </summary>
[Mapper]
public static partial class PushNotificationMapper
{
    public static partial void Patch(this PushNotificationSubscriptionDto source, PushNotificationSubscription destination);
}
