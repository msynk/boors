using Boors.Shared.Features.Chatbot;
using Boors.Server.Api.Features.Tenants;

namespace Boors.Server.Api.Features.Chatbot;

public class SystemPrompt
    : ITenantAware
{
    public Guid Id { get; set; }

    public PromptKind PromptKind { get; set; }

    [Required]
    public string? Markdown { get; set; }

    public long Version { get; set; }

    [ForeignKey(nameof(TenantId))]
    public Tenant? Tenant { get; set; }

    public Guid TenantId { get; set; }
}
