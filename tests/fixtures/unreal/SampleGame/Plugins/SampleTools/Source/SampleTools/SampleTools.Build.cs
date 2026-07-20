using UnrealBuildTool;

public class SampleTools : ModuleRules
{
    public SampleTools(ReadOnlyTargetRules Target) : base(Target)
    {
        PrivateDependencyModuleNames.AddRange(new[] { "Core", "UnrealEd" });
    }
}
