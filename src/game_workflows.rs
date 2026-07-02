use crate::workspace::Workspace;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameWorkflowKind {
    PrototypeMechanic,
    GenerateSpritesheet,
    GenerateUiSounds,
    CreateItemIcons,
    BuildVerticalSlice,
    RunPlaytestChecklist,
}

#[derive(Clone, Copy, Debug)]
pub struct GameWorkflowSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

#[derive(Clone, Debug)]
pub struct GameWorkflowRequest {
    pub workflow: GameWorkflowKind,
    pub title: String,
    pub brief: String,
}

#[derive(Clone, Debug)]
pub struct GameWorkflowResult {
    pub path: String,
    pub summary: Value,
}

pub fn workflow_specs() -> &'static [GameWorkflowSpec] {
    &[
        GameWorkflowSpec {
            id: "prototype_mechanic",
            label: "Prototype",
            description: "Turn a mechanic idea into a small buildable prototype plan.",
        },
        GameWorkflowSpec {
            id: "generate_spritesheet",
            label: "Spritesheet",
            description: "Plan a character/object spritesheet with frames and prompts.",
        },
        GameWorkflowSpec {
            id: "generate_ui_sounds",
            label: "UI Sounds",
            description: "Plan a small UI/game sound pack.",
        },
        GameWorkflowSpec {
            id: "create_item_icons",
            label: "Item Icons",
            description: "Plan a cohesive item icon set.",
        },
        GameWorkflowSpec {
            id: "build_vertical_slice",
            label: "Vertical Slice",
            description: "Plan a playable slice with systems, assets, QA, and build steps.",
        },
        GameWorkflowSpec {
            id: "run_playtest_checklist",
            label: "Playtest",
            description: "Create a practical playtest checklist and issue capture sheet.",
        },
    ]
}

pub fn parse_workflow_kind(value: &str) -> Option<GameWorkflowKind> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "prototype_mechanic" | "prototype" | "mechanic" => {
            Some(GameWorkflowKind::PrototypeMechanic)
        }
        "generate_spritesheet" | "spritesheet" | "sprite_sheet" => {
            Some(GameWorkflowKind::GenerateSpritesheet)
        }
        "generate_ui_sounds" | "ui_sounds" | "sounds" | "audio" => {
            Some(GameWorkflowKind::GenerateUiSounds)
        }
        "create_item_icons" | "item_icons" | "icons" => Some(GameWorkflowKind::CreateItemIcons),
        "build_vertical_slice" | "vertical_slice" | "slice" => {
            Some(GameWorkflowKind::BuildVerticalSlice)
        }
        "run_playtest_checklist" | "playtest" | "checklist" => {
            Some(GameWorkflowKind::RunPlaytestChecklist)
        }
        _ => None,
    }
}

pub fn workflow_id(kind: GameWorkflowKind) -> &'static str {
    match kind {
        GameWorkflowKind::PrototypeMechanic => "prototype_mechanic",
        GameWorkflowKind::GenerateSpritesheet => "generate_spritesheet",
        GameWorkflowKind::GenerateUiSounds => "generate_ui_sounds",
        GameWorkflowKind::CreateItemIcons => "create_item_icons",
        GameWorkflowKind::BuildVerticalSlice => "build_vertical_slice",
        GameWorkflowKind::RunPlaytestChecklist => "run_playtest_checklist",
    }
}

pub fn workflow_label(kind: GameWorkflowKind) -> &'static str {
    workflow_specs()
        .iter()
        .find(|spec| spec.id == workflow_id(kind))
        .map(|spec| spec.label)
        .unwrap_or("Workflow")
}

pub fn run_game_workflow(
    workspace: &Workspace,
    request: GameWorkflowRequest,
) -> anyhow::Result<GameWorkflowResult> {
    let title = if request.title.trim().is_empty() {
        workflow_label(request.workflow).to_string()
    } else {
        request.title.trim().to_string()
    };
    let brief = if request.brief.trim().is_empty() {
        "No brief supplied yet. Replace this section with the gameplay or app idea.".to_string()
    } else {
        request.brief.trim().to_string()
    };
    let path = format!(
        "docs/game-workflows/{}-{}.md",
        workflow_id(request.workflow),
        slugify(&title)
    );
    let body = render_workflow(request.workflow, &title, &brief);
    workspace.write_text(&path, &body)?;

    Ok(GameWorkflowResult {
        path,
        summary: json!({
            "workflow": workflow_id(request.workflow),
            "title": title,
            "recommended_next_tools": recommended_tools(request.workflow),
        }),
    })
}

fn render_workflow(kind: GameWorkflowKind, title: &str, brief: &str) -> String {
    match kind {
        GameWorkflowKind::PrototypeMechanic => format!(
            "# Prototype Mechanic: {title}\n\n## Brief\n{brief}\n\n## Player Promise\n- What should the player feel in the first 30 seconds?\n\n## Core Loop\n1. Observe\n2. Decide\n3. Act\n4. Feedback\n5. Reward or reset\n\n## Minimal Prototype Scope\n- One scene or screen.\n- One controllable action.\n- One fail/success condition.\n- Placeholder art is acceptable.\n\n## Implementation Tasks\n- Define input and state model.\n- Build the smallest playable interaction.\n- Add debug UI or logs for tuning.\n- Run project check/test/build.\n\n## Acceptance Criteria\n- The mechanic can be played end to end in under two minutes.\n- Feedback is visible or audible.\n- The prototype has one obvious tuning variable.\n\n## Recommended Agent Tools\n- `project_command` with `check`, `test`, or `run`.\n- `generate_image_asset` for placeholder visuals.\n- `generate_audio_asset` for feedback sounds.\n"
        ),
        GameWorkflowKind::GenerateSpritesheet => format!(
            "# Spritesheet Plan: {title}\n\n## Brief\n{brief}\n\n## Sheet Definition\n- Entity:\n- Style:\n- Grid: 4 columns x 4 rows\n- Frame size target:\n- Transparent background: yes\n\n## Animation Rows\n1. Idle\n2. Walk or move\n3. Action\n4. Hit/death/alternate\n\n## Prompt Seed\nCreate a clean game spritesheet for {title}. Keep a consistent character/object, isolated poses, readable silhouette, and a strict grid.\n\n## Recommended Agent Tools\n- `generate_spritesheet_asset` with columns/rows.\n- `export_asset` after approval.\n- `attach_asset` before follow-up variations.\n"
        ),
        GameWorkflowKind::GenerateUiSounds => format!(
            "# UI Sound Pack: {title}\n\n## Brief\n{brief}\n\n## Sound List\n- Confirm/select\n- Back/cancel\n- Hover/focus\n- Error/blocked\n- Reward/success\n\n## Style Guide\n- Duration target: 0.1s to 1.0s.\n- Keep transients clean and game-readable.\n- Avoid melody unless the product needs a motif.\n\n## Naming\n- `ui_confirm.wav`\n- `ui_back.wav`\n- `ui_hover.wav`\n- `ui_error.wav`\n- `ui_success.wav`\n\n## Recommended Agent Tools\n- `generate_audio_asset` for each sound.\n- `export_asset` into the game/app asset folder.\n"
        ),
        GameWorkflowKind::CreateItemIcons => format!(
            "# Item Icon Set: {title}\n\n## Brief\n{brief}\n\n## Icon Set\n- Common item:\n- Rare item:\n- Consumable:\n- Key item:\n- Upgrade:\n\n## Visual Rules\n- Same camera angle and lighting.\n- Strong silhouette at 32px and 64px.\n- Background should be transparent or simple.\n- Use a consistent rarity accent system.\n\n## Prompt Seed\nCreate a cohesive set of game item icons for {title}; readable at small sizes, consistent style, centered object, no text.\n\n## Recommended Agent Tools\n- `generate_image_asset` for individual icons.\n- `vary_image_asset` for set consistency.\n- `upscale_asset` for final export.\n"
        ),
        GameWorkflowKind::BuildVerticalSlice => format!(
            "# Vertical Slice Plan: {title}\n\n## Brief\n{brief}\n\n## Slice Goal\n- What complete experience should this prove?\n\n## Must-Have Systems\n- Input/control\n- Core interaction\n- Progression or scoring\n- UI feedback\n- Save/reset or replay\n\n## Asset Pack\n- One hero/key object\n- One environment or screen\n- UI icons\n- 3-5 sounds\n- Optional short trailer/preview clip\n\n## Build Checklist\n- Project runs locally.\n- Main loop playable.\n- Assets integrated.\n- Basic QA pass complete.\n- Known issues captured.\n\n## Recommended Agent Tools\n- `project_command` run/build/test.\n- `generate_image_asset`, `generate_spritesheet_asset`, `generate_audio_asset`.\n- `run_game_workflow` with `run_playtest_checklist`.\n"
        ),
        GameWorkflowKind::RunPlaytestChecklist => format!(
            "# Playtest Checklist: {title}\n\n## Brief\n{brief}\n\n## Session Setup\n- Build/version:\n- Test device:\n- Tester profile:\n- Session length:\n\n## Observe\n- First action without instruction:\n- First confusion point:\n- First moment of delight:\n- Completion/fail state:\n\n## Checklist\n- Launches cleanly.\n- Controls are discoverable.\n- Feedback is understandable.\n- No blocking bugs in core loop.\n- Performance feels acceptable.\n- Player knows what to do next.\n\n## Issue Capture\n| Severity | Area | What happened | Expected | Repro steps |\n| --- | --- | --- | --- | --- |\n|  |  |  |  |  |\n\n## Follow-Up\n- Top fix:\n- Fast polish win:\n- Next test question:\n"
        ),
    }
}

fn recommended_tools(kind: GameWorkflowKind) -> Vec<&'static str> {
    match kind {
        GameWorkflowKind::PrototypeMechanic => {
            vec![
                "project_command",
                "generate_image_asset",
                "generate_audio_asset",
            ]
        }
        GameWorkflowKind::GenerateSpritesheet => {
            vec!["generate_spritesheet_asset", "export_asset", "attach_asset"]
        }
        GameWorkflowKind::GenerateUiSounds => vec!["generate_audio_asset", "export_asset"],
        GameWorkflowKind::CreateItemIcons => {
            vec!["generate_image_asset", "vary_image_asset", "upscale_asset"]
        }
        GameWorkflowKind::BuildVerticalSlice => vec![
            "project_command",
            "generate_image_asset",
            "generate_spritesheet_asset",
            "generate_audio_asset",
        ],
        GameWorkflowKind::RunPlaytestChecklist => vec!["project_command", "open_project_preview"],
    }
}

fn slugify(text: &str) -> String {
    let mut slug = text
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug = slug.trim_matches('-').chars().take(48).collect();
    if slug.is_empty() {
        "workflow".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workflow_aliases() {
        assert_eq!(
            parse_workflow_kind("prototype"),
            Some(GameWorkflowKind::PrototypeMechanic)
        );
        assert_eq!(
            parse_workflow_kind("ui_sounds"),
            Some(GameWorkflowKind::GenerateUiSounds)
        );
    }

    #[test]
    fn writes_workflow_file() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let result = run_game_workflow(
            &workspace,
            GameWorkflowRequest {
                workflow: GameWorkflowKind::BuildVerticalSlice,
                title: "Arena Dash".to_string(),
                brief: "Tiny action prototype".to_string(),
            },
        )
        .unwrap();

        assert!(result.path.starts_with("docs/game-workflows/"));
        assert!(workspace
            .read_text(&result.path, 20_000)
            .unwrap()
            .contains("Vertical Slice"));
    }
}
