using System.ComponentModel;

namespace Boors.Shared;

// The [Description] attribute defines a short, human/AI friendly summary for a page.
// Only pages that carry a [Description] are exposed by PageUrls.GetPages() (see PageUrls.Pages.cs).
// Those are published PUBLICLY and WITHOUT authentication at GET /llms.txt
// (Server.Web/Infrastructure/Extensions/WebApplicationExtensions.cs) and, when signalR is enabled, passed to the
// chatbot's GetAppPages tool.
// Do not describe a page here unless the description is safe for an anonymous reader.
public static partial class PageUrls
{
    [Description("Home page.")]
    public const string Home = "/";

    public const string NotFound = "/not-found";

    [Description("Legal terms, conditions and end-user license agreement.")]
    public const string Terms = "/terms";

    [Description("Privacy policy.")]
    public const string PrivacyPolicy = "/privacy-policy";

    [Description("User settings hub (profile, account, two-factor authentication and sessions). Requires sign-in.")]
    public const string Settings = "/settings";

    [Description("Information about the application.")]
    public const string About = "/about";

    [Description("A simple personal offline to-do list. Requires sign-in.")]
    public const string OfflineTodo = "/offline-todo";

    [Description("Manage the AI assistant's system prompts. Requires sign-in.")]
    public const string SystemPrompts = "/system-prompts";

    [Description("Manage the tenants you own. Requires sign-in.")]
    public const string ManageMyTenants = "/manage-my-tenants";

    [Description("Administer all tenants. Requires sign-in.")]
    public const string ManageAllTenants = "/manage-all-tenants";

    [Description("Manage user groups and roles. Requires sign-in.")]
    public const string Roles = "/user-groups";

    [Description("Manage users. Requires sign-in.")]
    public const string Users = "/users";

    [Description("Review and revoke the external applications authorized over OAuth. Requires sign-in.")]
    public const string OAuthClients = "/oauth-clients";

    [Description("Run this deployment: the status of every dependency it relies on, and its background jobs. Requires sign-in.")]
    public const string Operations = "/operations";

    /// <summary>
    /// Where <c>/oauth/authorize</c> sends the browser; no [Description] because only that redirect reaches it. An app
    /// link on both platforms: whoever installed the app is usually signed in there, and even a cold hybrid start beats
    /// booting Blazor WebAssembly in an Android browser (ConsentPage.LeaveFor gets back out). Deliberately not under
    /// /oauth/, which belongs to the server, so the service worker, the AASA and the Android filter need no carve-out.
    /// </summary>
    public const string OAuthConsent = "/oauth-consent";

    public const string WebInteropApp = "/web-interop-app.html";

    [Description("Tehran Stock Exchange and Fara Bourse at a glance: indices, value traded, breadth, queues, money flow and sectors for the latest session. Requires sign-in.")]
    public const string MarketOverview = "/market";

    [Description("The market watch: every instrument of the session, filterable with TSETMC-compatible filter expressions. Requires sign-in.")]
    public const string MarketWatch = "/market/watch";

    [Description("Rankings of shares by momentum, 52-week-high proximity, individual money flow and buyer power. Requires sign-in.")]
    public const string MarketScreens = "/market/screens";

    [Description("Freshness of the market data, collection jobs and source reachability. Requires sign-in.")]
    public const string MarketHealth = "/market/health";

    /// <summary>The symbol workspace; the page route appends <c>/{insCode:long}</c>.</summary>
    public const string Symbol = "/symbol";
}
