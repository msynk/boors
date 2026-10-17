using Microsoft.Extensions.Hosting;

var builder = DistributedApplication.CreateBuilder(args);

// Check out appsettings.Development.json for credentials/passwords settings.


var postgresDatabase = builder.AddPostgreSQL();

var s3Storage = builder.AddS3Storage();

var keycloak = builder.AddKeycloak();

var serverWebProject = builder.AddProject("serverweb", "../Boors.Server.Web/Boors.Server.Web.csproj")
    .WithExternalHttpEndpoints();

// Adding health checks endpoints to applications in non-development environments has security implications.
// See https://aka.ms/dotnet/aspire/healthchecks for details before enabling these endpoints in non-development environments.
if (builder.Environment.IsDevelopment())
{
    serverWebProject.WithHttpHealthCheck("/alive");
}


serverWebProject.WithReference(postgresDatabase).WaitFor(postgresDatabase);
serverWebProject.WithReference(s3Storage);
serverWebProject.WithReference(keycloak);


if (builder.ExecutionContext.IsRunMode) // The following project is only added for testing purposes.
{
    // Blazor WebAssembly Standalone project.
    builder.AddProject("clientwebwasm", "../../Client/Boors.Client.Web/Boors.Client.Web.csproj")
        .WithExplicitStart();

    var mailpit = builder.AddMailPit("smtp") // For testing purposes only, in production, you would use a real SMTP server.
        .WithOtlpExporter()
        .WithDataVolume("mailpit");

    serverWebProject.WithReference(mailpit);

    if (OperatingSystem.IsWindows())
    {
        // Blazor Hybrid Windows project.
        builder.AddProject("clientwindows", "../../Client/Boors.Client.Windows/Boors.Client.Windows.csproj")
            .WithExplicitStart();
    }

    // By default every container is created from scratch on each run and is destroyed as soon as the app host stops.
    // UsePersistentContainers keeps them alive and reuses them instead, which makes starting the project
    // (F5 / `aspire start`) and running the automated tests considerably faster, at the cost of the memory they keep
    // consuming while you're not debugging (stop them from Docker Desktop whenever you need it back).
    // Inside a Dev Container / GitHub Codespaces it is always on: the containers run in its docker-in-docker, so they
    // never outlive the dev container itself. To have it on your own machine as well, remove the `if` below and keep the `builder.UsePersistentContainers();`.
    // Check out the `.docs/20- .NET Aspire.md` file for more details.

    var inDevContainer = Environment.GetEnvironmentVariable("REMOTE_CONTAINERS") is "true" || Environment.GetEnvironmentVariable("CODESPACES") is "true";
    if (inDevContainer)
    {
        builder.UsePersistentContainers();
    }


    builder.RemoveWildcardEndpoints();
}

await builder
    .Build()
    .RunAsync();
