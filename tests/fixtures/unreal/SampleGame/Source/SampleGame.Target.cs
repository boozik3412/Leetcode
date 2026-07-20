using UnrealBuildTool;

public class SampleGameTarget : TargetRules
{
    public SampleGameTarget(TargetInfo Target) : base(Target)
    {
        Type = TargetType.Game;
        ExtraModuleNames.Add("SampleGame");
    }
}
