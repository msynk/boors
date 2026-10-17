namespace Boors.Shared.Features.Chatbot;

/// <summary>The cards the server names (See <see cref="AiChatCard.ComponentType"/>); each is a component in AppAiChatPanelCards.</summary>
public static class AiChatCardComponents
{
    private const string Namespace = "Boors.Client.Core.Components.Layout.AppAiChatPanelCards.";

    public const string HumanFollowUp = Namespace + "HumanFollowUpCard";

    public const string UserApproval = Namespace + "UserApprovalCard";

}
