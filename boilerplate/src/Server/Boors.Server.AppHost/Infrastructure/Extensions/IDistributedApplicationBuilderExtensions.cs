using Aspire.Hosting.Azure;
using Aspire.Hosting.RustFs;

namespace Aspire.Hosting;

public static class IDistributedApplicationBuilderExtensions
{
    extension(IDistributedApplicationBuilder builder)
    {
        /// <summary>
        /// Adds a Keycloak identity server. In run mode the development realm of the <c>./Infrastructure/Realms</c>
        /// folder is imported into it; that realm seeds accounts with well-known passwords, so it must never reach a
        /// published application model.
        /// https://aspire.dev/integrations/security/keycloak/
        /// </summary>
        public IResourceBuilder<KeycloakResource> AddKeycloak()
        {
            // No explicit host port: every other container here lets Aspire allocate one, and a fixed port cannot be
            // held by two app hosts at once - which `aspire run` plus `dotnet test` on one machine already is.
            var keycloak = builder.AddKeycloak("keycloak")
                .WithDataVolume();

            if (builder.ExecutionContext.IsRunMode)
            {
                keycloak.WithRealmImport("./Infrastructure/Realms");
            }

            return keycloak;
        }



        /// <summary>
        /// Adds a PostgreSQL Server instance with pgAdmin and a database named <c>postgresdb</c>.
        /// Uses pgvector (pg18) image which supports embedded vector search.
        /// </summary>
        public IResourceBuilder<AzurePostgresFlexibleServerDatabaseResource> AddPostgreSQL()
        {
            return builder.AddAzurePostgresFlexibleServer("postgresserver")
                .RunAsContainer(postgresDatabase => // Remove this RunAsContainer and related configuration to use actual Azure PostgreSQL instance
                {
                    postgresDatabase.WithPgAdmin()
                        .WithV18DataVolume()
                        .WithOptimizedSetup()
                        .WithImage("pgvector/pgvector", "pg18") // pgvector supports embedded vector search.
                        .WithOtlpExporter();
                })
                .AddDatabase("postgresdb");
        }




        /// <summary>
        /// Adds RustFS, a lightweight S3-compatible object storage server, and exposes it as the <c>s3</c> connection
        /// string that <c>Program.Services.cs</c> builds its FluentStorage store out of.
        /// <see cref="RustFsResource"/> is a plain container resource with no connection string of its own, so it is
        /// assembled here from the container's primary endpoint and its credentials.
        /// https://github.com/konnta0/Aspire.Extensions
        /// </summary>
        public IResourceBuilder<IResourceWithConnectionString> AddS3Storage()
        {
            var accessKey = builder.AddParameter("rustfs-accessKey", new GenerateParameterDefault { MinLength = 22 }, secret: true);
            var secretKey = builder.AddParameter("rustfs-secretKey", new GenerateParameterDefault { MinLength = 22 }, secret: true);

            var rustFs = builder.AddRustFs("rustfs", accessKey, secretKey)
                .WithDataVolume();

            var endpoint = rustFs.GetEndpoint(RustFsResource.PrimaryEndpointName);

            return builder.AddConnectionString("s3", ReferenceExpression.Create(
                $"Endpoint={endpoint.Property(EndpointProperty.Url)};AccessKey={accessKey.Resource};SecretKey={secretKey.Resource};"));
        }


        /// <summary>
        /// Projects' launchSettings bind <c>http://*:port</c> so a direct <c>dotnet run</c> is reachable over the LAN
        /// (e.g. from Android/iOS devices), but Aspire can't give a container a reachable address for a wildcard host,
        /// so that endpoint only makes the ingress container fail to start. Drops every wildcard endpoint from the
        /// model - run mode only, and launchSettings is untouched, so a direct run still binds every interface.
        /// </summary>
        public IDistributedApplicationBuilder RemoveWildcardEndpoints()
        {
            if (builder.ExecutionContext.IsRunMode is false)
                return builder;

            foreach (var project in builder.Resources.OfType<ProjectResource>().ToArray())
            {
                foreach (var wildcard in project.Annotations.OfType<EndpointAnnotation>().Where(endpoint => endpoint.TargetHost is "*").ToArray())
                    project.Annotations.Remove(wildcard);
            }

            return builder;
        }

        /// <summary>
        /// Gives every container of the application model a persistent lifetime, so that they are created once and are
        /// then reused by every subsequent run, instead of being re-created and booted up from scratch each and every time.
        /// </summary>
        /// <remarks>
        /// Call it right before <see cref="IDistributedApplicationBuilder.Build"/>, so all the resources are already added
        /// while the application model is still mutable.
        /// </remarks>
        public IDistributedApplicationBuilder UsePersistentContainers()
        {
            foreach (var container in builder.Resources.OfType<ContainerResource>().ToArray())
            {
                builder.CreateResourceBuilder(container)
                    .WithEnvironment(context =>
                    {
                        // Aspire injects its own OTLP endpoint (https://aspire.dev.internal:<port>) into the containers,
                        // and that port is allocated again on every run. Since the environment variables are part of the
                        // container's lifecycle key, leaving them in place makes Aspire re-create every container on each
                        // run ("Found existing Container, but calculated lifecycle key doesn't match"), which defeats the
                        // whole purpose. Dropping them costs us the containers' telemetry in the Aspire dashboard only.
                        foreach (var otelVariable in context.EnvironmentVariables.Keys.Where(key => key.StartsWith("OTEL_")).ToArray())
                        {
                            context.EnvironmentVariables.Remove(otelVariable);
                        }
                    })
                    .WithLifetime(ContainerLifetime.Persistent);
            }

            return builder;
        }
    }
}
