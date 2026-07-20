using UnrealBuildTool;

public class SampleGameEditorTarget : TargetRules
{
    public SampleGameEditorTarget(TargetInfo Target) : base(Target)
    {
        Type = TargetType.Editor;
        ExtraModuleNames.Add("SampleGame");
    }
}
