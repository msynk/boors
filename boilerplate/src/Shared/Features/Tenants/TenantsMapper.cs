using Boors.Shared.Features.Tenants.Dtos;

namespace Boors.Shared.Features.Tenants;

[Mapper(UseDeepCloning = true)]
public static partial class TenantsMapper
{
    public static partial void Patch(this TenantDto source, TenantDto destination);
}
