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
            label: "Прототип",
            description: "Превратить идею механики в небольшой реализуемый план прототипа.",
        },
        GameWorkflowSpec {
            id: "generate_spritesheet",
            label: "Спрайт-лист",
            description: "Спланировать спрайт-лист персонажа/объекта с кадрами и промптами.",
        },
        GameWorkflowSpec {
            id: "generate_ui_sounds",
            label: "UI-звуки",
            description: "Спланировать небольшой набор UI/game-звуков.",
        },
        GameWorkflowSpec {
            id: "create_item_icons",
            label: "Иконки",
            description: "Спланировать целостный набор иконок предметов.",
        },
        GameWorkflowSpec {
            id: "build_vertical_slice",
            label: "Вертикальный срез",
            description: "Спланировать играбельный срез с системами, ассетами, QA и шагами сборки.",
        },
        GameWorkflowSpec {
            id: "run_playtest_checklist",
            label: "Плейтест",
            description: "Создать практичный playtest-чеклист и таблицу фиксации проблем.",
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
        .unwrap_or("Сценарий")
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
        "Бриф пока не указан. Замените этот раздел идеей геймплея или приложения.".to_string()
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
            "# Прототип механики: {title}\n\n## Бриф\n{brief}\n\n## Обещание игроку\n- Что игрок должен почувствовать за первые 30 секунд?\n\n## Основной цикл\n1. Наблюдение\n2. Решение\n3. Действие\n4. Обратная связь\n5. Награда или сброс\n\n## Минимальный scope прототипа\n- Одна сцена или экран.\n- Одно управляемое действие.\n- Одно условие провала/успеха.\n- Placeholder-арт допустим.\n\n## Задачи реализации\n- Определить модель ввода и состояния.\n- Собрать минимальное playable-взаимодействие.\n- Добавить debug UI или логи для настройки.\n- Запустить project check/test/build.\n\n## Критерии приёмки\n- Механику можно пройти end-to-end меньше чем за две минуты.\n- Обратная связь видима или слышна.\n- У прототипа есть одна очевидная переменная для настройки.\n\n## Рекомендуемые инструменты агента\n- `project_command` с `check`, `test` или `run`.\n- `generate_image_asset` для placeholder-визуалов.\n- `generate_audio_asset` для звуков обратной связи.\n"
        ),
        GameWorkflowKind::GenerateSpritesheet => format!(
            "# План спрайт-листа: {title}\n\n## Бриф\n{brief}\n\n## Определение листа\n- Сущность:\n- Стиль:\n- Сетка: 4 колонки x 4 ряда\n- Целевой размер кадра:\n- Прозрачный фон: да\n\n## Ряды анимации\n1. Idle\n2. Ходьба или движение\n3. Действие\n4. Удар/смерть/альтернатива\n\n## Стартовый промпт\nСоздай чистый игровой спрайт-лист для {title}. Сохрани консистентного персонажа/объект, изолированные позы, читаемый силуэт и строгую сетку.\n\n## Рекомендуемые инструменты агента\n- `generate_spritesheet_asset` с columns/rows.\n- `export_asset` после подтверждения.\n- `attach_asset` перед последующими вариациями.\n"
        ),
        GameWorkflowKind::GenerateUiSounds => format!(
            "# Набор UI-звуков: {title}\n\n## Бриф\n{brief}\n\n## Список звуков\n- Подтверждение/выбор\n- Назад/отмена\n- Hover/focus\n- Ошибка/блокировка\n- Награда/успех\n\n## Гайд по стилю\n- Целевая длительность: 0.1s-1.0s.\n- Держать транзиенты чистыми и читаемыми для игры.\n- Избегать мелодии, если продукту не нужен мотив.\n\n## Именование\n- `ui_confirm.wav`\n- `ui_back.wav`\n- `ui_hover.wav`\n- `ui_error.wav`\n- `ui_success.wav`\n\n## Рекомендуемые инструменты агента\n- `generate_audio_asset` для каждого звука.\n- `export_asset` в папку ассетов игры/приложения.\n"
        ),
        GameWorkflowKind::CreateItemIcons => format!(
            "# Набор иконок предметов: {title}\n\n## Бриф\n{brief}\n\n## Набор иконок\n- Обычный предмет:\n- Редкий предмет:\n- Расходник:\n- Ключевой предмет:\n- Улучшение:\n\n## Визуальные правила\n- Одинаковый угол камеры и свет.\n- Сильный силуэт на 32px и 64px.\n- Фон прозрачный или простой.\n- Единая система акцентов редкости.\n\n## Стартовый промпт\nСоздай целостный набор игровых иконок предметов для {title}; читаемые в малом размере, единый стиль, объект по центру, без текста.\n\n## Рекомендуемые инструменты агента\n- `generate_image_asset` для отдельных иконок.\n- `vary_image_asset` для консистентности набора.\n- `upscale_asset` для финального экспорта.\n"
        ),
        GameWorkflowKind::BuildVerticalSlice => format!(
            "# Вертикальный срез: {title}\n\n## Бриф\n{brief}\n\n## Цель среза\n- Какой законченный игровой опыт он должен доказать?\n\n## Обязательные системы\n- Ввод/управление\n- Основное взаимодействие\n- Прогрессия или счёт\n- UI-обратная связь\n- Сохранение, сброс или повтор\n\n## Набор ассетов\n- Один ключевой объект\n- Одно окружение или экран\n- UI-иконки\n- 3-5 звуков\n- Опциональный короткий трейлер или preview-клип\n\n## Чеклист сборки\n- Проект запускается локально.\n- Основной игровой цикл играбелен.\n- Ассеты интегрированы.\n- Базовая QA-проверка завершена.\n- Известные проблемы зафиксированы.\n\n## Рекомендуемые инструменты агента\n- `project_command` для run/build/test.\n- `generate_image_asset`, `generate_spritesheet_asset`, `generate_audio_asset`.\n- `run_game_workflow` с `run_playtest_checklist`.\n"
        ),
        GameWorkflowKind::RunPlaytestChecklist => format!(
            "# Playtest-чеклист: {title}\n\n## Бриф\n{brief}\n\n## Настройка сессии\n- Build/version:\n- Тестовое устройство:\n- Профиль тестера:\n- Длительность сессии:\n\n## Наблюдение\n- Первое действие без инструкции:\n- Первая точка непонимания:\n- Первый момент удовольствия:\n- Состояние completion/fail:\n\n## Чеклист\n- Запускается чисто.\n- Управление можно обнаружить.\n- Обратная связь понятна.\n- Нет блокирующих багов в core loop.\n- Производительность ощущается приемлемой.\n- Игрок понимает, что делать дальше.\n\n## Фиксация проблем\n| Серьёзность | Область | Что произошло | Ожидалось | Шаги воспроизведения |\n| --- | --- | --- | --- | --- |\n|  |  |  |  |  |\n\n## Follow-up\n- Главный фикс:\n- Быстрый polish-win:\n- Следующий вопрос теста:\n"
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
            .contains("Вертикальный срез"));
    }
}
