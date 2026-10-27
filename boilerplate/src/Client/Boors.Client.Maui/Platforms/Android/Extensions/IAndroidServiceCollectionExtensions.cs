// [mirror] per platform DI registrations of the maui project - keep in sync with:
// - src/Client/Boors.Client.Maui/Platforms/iOS/Extensions/IIosServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/MacCatalyst/Extensions/IMacServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/Windows/Extensions/IWindowsServiceCollectionExtensions.cs

using Boors.Client.Maui.Platforms.Android.Services;

namespace Microsoft.Extensions.DependencyInjection;

public static partial class IAndroidServiceCollectionExtensions
{
    extension(IServiceCollection services)
    {
        public IServiceCollection AddClientMauiProjectAndroidServices(IConfiguration configuration)
        {
            // Services being registered here can get injected in Maui/Android.

            services.AddSingleton<IPushNotificationService, AndroidPushNotificationService>();

            return services;
        }
    }
}
