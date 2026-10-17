using CommunityToolkit.Datasync.Server.EntityFrameworkCore;

namespace Boors.Server.Api.Features.Todo;

public partial class TodoItem
    : RepositoryControlledEntityTableData
{

    [Required]
    public string? Title { get; set; }
    public bool IsDone { get; set; }

    [ForeignKey(nameof(UserId))]
    public User? User { get; set; }
    public Guid UserId { get; set; }
}
