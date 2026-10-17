namespace Boors.Server.Api.Features.Identity.Models;

public class UserLogin : IdentityUserLogin<Guid>
{
    public User? User { get; set; }
}
