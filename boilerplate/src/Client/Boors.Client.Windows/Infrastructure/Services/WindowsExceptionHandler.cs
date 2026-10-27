// [mirror] blazor hybrid exception handling - keep in sync with:
// - src/Client/Boors.Client.Maui/Infrastructure/Services/MauiExceptionHandler.cs

namespace Boors.Client.Windows.Infrastructure.Services;

public partial class WindowsExceptionHandler : ClientExceptionHandlerBase
{
    protected override void Handle(Exception exception, ExceptionDisplayKind displayKind, Dictionary<string, object?> parameters)
    {
        exception = UnWrapException(exception);

        if (IgnoreException(exception))
            return;

        base.Handle(exception, displayKind, parameters);
    }
}
