namespace Boors.Shared.Features.Todo;

[DtoResourceType(typeof(AppStrings))]
public partial class TodoItemDto
    : BaseDtoTableData
{

    [Required(ErrorMessage = nameof(AppStrings.RequiredAttribute_ValidationError))]
    [Display(Name = nameof(AppStrings.Title))]
    public string? Title { get; set; }

    public bool IsDone { get; set; }

    [JsonIgnore, NotMapped]
    public bool IsInEditMode { get; set; }
}
