// [mirror] per platform DI registrations of the maui project - keep in sync with:
// - src/Client/Boors.Client.Maui/Platforms/Android/Extensions/IAndroidServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/iOS/Extensions/IIosServiceCollectionExtensions.cs
// - src/Client/Boors.Client.Maui/Platforms/MacCatalyst/Extensions/IMacServiceCollectionExtensions.cs

using Boors.Client.Maui.Platforms.Windows.Services;

namespace Microsoft.Extensions.DependencyInjection;

public static partial class IWindowsServiceCollectionExtensions
{
    extension(IServiceCollection services)
    {
        public IServiceCollection AddClientMauiProjectWindowsServices(IConfiguration configuration)
        {
            // Services being registered here can get injected in Maui/windows.

            services.AddSingleton<IPushNotificationService, WindowsPushNotificationService>();

            return services;
        }
    }
}
