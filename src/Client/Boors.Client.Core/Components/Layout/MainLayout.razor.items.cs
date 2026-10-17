namespace Boors.Client.Core.Components.Layout;

public partial class MainLayout
{
    private List<BitNavItem> navPanelItems = [];

    [AutoInject] protected IStringLocalizer<AppStrings> localizer = default!;
    [AutoInject] protected IAuthorizationService authorizationService = default!;

    private async Task SetNavPanelItems(ClaimsPrincipal authUser)
    {
        navPanelItems =
        [
            new()
            {
                Text = localizer[nameof(AppStrings.Home)],
                IconName = BitIconName.Home,
                Url = PageUrls.Home,
            }
        ];

        var tenantIsSelected = await authorizationService.IsAuthorized(authUser!, AuthPolicies.TENANT_SELECTED);


        if (await authorizationService.IsAuthorized(authUser!, AppFeatures.Todo.Todo_Manage_Self))
        {
            navPanelItems.Add(new()
            {
                Text = localizer[nameof(AppStrings.OfflineTodoTitle)],
                IconName = BitIconName.ToDoLogoOutline,
                Url = PageUrls.OfflineTodo,
            });
        }

        navPanelItems.Add(new()
        {
            Text = localizer[nameof(AppStrings.Terms)],
            IconName = BitIconName.EntityExtraction,
            Url = PageUrls.Terms,
        });

        navPanelItems.Add(new()
        {
            Text = localizer[nameof(AppStrings.About)],
            IconName = BitIconName.Info,
            Url = PageUrls.About,
        });

        var (manageRoles, manageUsers) = await (authorizationService.IsAuthorized(authUser!, AppFeatures.Management.Roles_Manage),
            authorizationService.IsAuthorized(authUser!, AppFeatures.Management.Users_Manage));

        var manageOAuthClients = await authorizationService.IsAuthorized(authUser!, AppFeatures.System.OAuthClients_Manage);

        var viewOperations = await authorizationService.IsAuthorized(authUser!, AppFeatures.System.Operations_View);

        var manageAiPrompt = await authorizationService.IsAuthorized(authUser!, AppFeatures.Management.SystemPrompts_Write);

        if (tenantIsSelected is false)
        {
            manageRoles = manageUsers = false;
            manageAiPrompt = false;
        }

        var manageTenantsGlobally = await authorizationService.IsAuthorized(authUser!, AppFeatures.Management.Tenants_Manage_Global);

        // Every flag in this condition has to be able to contribute a child item below, or the group renders empty.
        if (manageRoles || manageUsers || manageOAuthClients || viewOperations
            || manageAiPrompt
            || manageTenantsGlobally
            )
        {
            BitNavItem managementItem = new()
            {
                Text = localizer[nameof(AppStrings.Management)],
                IconName = BitIconName.SettingsSecure,
                ChildItems = []
            };

            navPanelItems.Add(managementItem);

            if (manageRoles)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.UserGroups)],
                    IconName = BitIconName.WorkforceManagement,
                    Url = PageUrls.Roles,
                });
            }

            if (manageUsers)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.Users)],
                    IconName = BitIconName.SecurityGroup,
                    Url = PageUrls.Users,
                });
            }

            if (manageOAuthClients)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.OAuthClients)],
                    IconName = BitIconName.PlugConnected,
                    Url = PageUrls.OAuthClients,
                });
            }

            if (viewOperations)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.Operations)],
                    IconName = BitIconName.ServerProcesses,
                    Url = PageUrls.Operations,
                });
            }

            if (manageAiPrompt)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.SystemPromptsTitle)],
                    IconName = BitIconName.TextDocumentSettings,
                    Url = PageUrls.SystemPrompts,
                });
            }

            if (manageTenantsGlobally)
            {
                managementItem.ChildItems.Add(new()
                {
                    Text = localizer[nameof(AppStrings.ManageAllTenants)],
                    IconName = BitIconName.Org,
                    Url = PageUrls.ManageAllTenants,
                });
            }
        }

        if (authUser.IsAuthenticated())
        {
            navPanelItems.Add(new()
            {
                Text = localizer[nameof(AppStrings.Settings)],
                IconName = BitIconName.Equalizer,
                Url = PageUrls.Settings,
                AdditionalUrls =
                [
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.Profile}",
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.Account}",
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.Tfa}",
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.Sessions}",
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.Privacy}",
                    $"{PageUrls.Settings}/{PageUrls.SettingsSections.UpgradeAccount}",
                ]
            });
        }
    }
}
