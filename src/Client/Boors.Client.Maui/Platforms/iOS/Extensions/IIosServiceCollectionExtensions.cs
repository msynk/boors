// [mirror] per platform DI registrations of the maui project - keep in sync with:
// - src/Client/Boors.Client.Maui/Platforms/Android/Extensions/IAndroidServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/MacCatalyst/Extensions/IMacServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/Windows/Extensions/IWindowsServiceCollectionExtensions.cs

using Boors.Client.Maui.Platforms.iOS.Services;

namespace Microsoft.Extensions.DependencyInjection;

public static partial class IIosServiceCollectionExtensions
{
    extension(IServiceCollection services)
    {
        public IServiceCollection AddClientMauiProjectIosServices(IConfiguration configuration)
        {
            // Services registered in this class can be injected in iOS.

            services.AddSingleton<IPushNotificationService, iOSPushNotificationService>();

            return services;
        }
    }
}
