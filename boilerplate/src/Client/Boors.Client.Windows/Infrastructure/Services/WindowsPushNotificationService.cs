// [mirror] not implemented push notification service of the windows targets - keep in sync with:
// - src/Client/Boors.Client.Maui/Platforms/Windows/Services/WindowsPushNotificationService.cs

using Boors.Shared.Features.PushNotification;

namespace Boors.Client.Windows.Infrastructure.Services;

public partial class WindowsPushNotificationService : PushNotificationServiceBase
{
    public override Task<PushNotificationSubscriptionDto?> GetSubscription(CancellationToken cancellationToken) =>
        throw new NotImplementedException();

    public override Task RequestPermission(CancellationToken cancellationToken) =>
        throw new NotImplementedException();
}
