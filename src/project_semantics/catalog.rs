use super::types::{SemanticTagDefinition, SemanticTagGroup};
use crate::game_task_builder::game_task_catalog_ref;
use std::collections::BTreeMap;

pub fn semantic_catalog() -> Vec<SemanticTagDefinition> {
    let mut definitions = Vec::new();
    for domain in &game_task_catalog_ref().domains {
        definitions.push(definition(
            &format!("domain.{}", domain.id),
            &domain.label,
            SemanticTagGroup::Domain,
            "Сфера разработки из конструктора игровых задач",
        ));
        for direction in &domain.directions {
            definitions.push(definition(
                &format!("system.{}", direction.id),
                &direction.label,
                SemanticTagGroup::System,
                "Игровая или производственная подсистема проекта",
            ));
        }
    }

    for (id, label, group, description) in fixed_definitions() {
        definitions.push(definition(id, label, group, description));
    }
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    definitions.dedup_by(|left, right| left.id == right.id);
    definitions
}

pub fn semantic_catalog_map() -> BTreeMap<String, SemanticTagDefinition> {
    semantic_catalog()
        .into_iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

fn definition(
    id: &str,
    label: &str,
    group: SemanticTagGroup,
    description: &str,
) -> SemanticTagDefinition {
    SemanticTagDefinition {
        id: id.to_string(),
        label: label.to_string(),
        group,
        description: description.to_string(),
    }
}

fn fixed_definitions() -> Vec<(&'static str, &'static str, SemanticTagGroup, &'static str)> {
    use SemanticTagGroup as G;
    vec![
        (
            "entity.blueprint",
            "Blueprint",
            G::Entity,
            "Blueprint-класс или Blueprint-ассет",
        ),
        (
            "entity.character",
            "Персонаж",
            G::Entity,
            "Игровой Character или Pawn",
        ),
        (
            "entity.controller",
            "Контроллер",
            G::Entity,
            "PlayerController или AIController",
        ),
        (
            "entity.widget",
            "Widget",
            G::Entity,
            "UMG/Common UI элемент",
        ),
        (
            "entity.input",
            "Ввод",
            G::Entity,
            "Input Action или Mapping Context",
        ),
        (
            "entity.animation",
            "Анимация",
            G::Entity,
            "Анимационный ассет или граф",
        ),
        (
            "entity.skeleton",
            "Skeleton",
            G::Entity,
            "Skeleton персонажа",
        ),
        (
            "entity.skeletal_mesh",
            "Skeletal Mesh",
            G::Entity,
            "Скелетная модель",
        ),
        (
            "entity.static_mesh",
            "Static Mesh",
            G::Entity,
            "Статическая модель",
        ),
        (
            "entity.weapon",
            "Оружие",
            G::Entity,
            "Оружие или его игровая конфигурация",
        ),
        ("entity.level", "Уровень", G::Entity, "Карта или часть мира"),
        (
            "entity.audio",
            "Аудио",
            G::Entity,
            "Звук, MetaSound или микс",
        ),
        (
            "entity.material",
            "Материал",
            G::Entity,
            "Материал, шейдер или текстура",
        ),
        (
            "entity.vfx",
            "VFX",
            G::Entity,
            "Niagara или визуальный эффект",
        ),
        (
            "entity.data",
            "Данные",
            G::Entity,
            "Data Asset или конфигурация",
        ),
        (
            "role.player.primary",
            "Главный персонаж",
            G::Role,
            "Основной управляемый персонаж",
        ),
        (
            "role.player.controller",
            "Контроллер игрока",
            G::Role,
            "Контроллер локального игрока",
        ),
        (
            "role.npc.base",
            "Базовый NPC",
            G::Role,
            "Базовый класс неигровых персонажей",
        ),
        (
            "role.enemy.base",
            "Базовый противник",
            G::Role,
            "Базовый класс противников",
        ),
        ("role.companion", "Компаньон", G::Role, "Персонаж-компаньон"),
        (
            "role.hud.root",
            "Основной HUD",
            G::Role,
            "Корневой интерфейс игрового HUD",
        ),
        (
            "role.crosshair",
            "Прицел",
            G::Role,
            "Виджет или логика прицела",
        ),
        (
            "role.menu.root",
            "Главное меню",
            G::Role,
            "Корневой экран меню",
        ),
        (
            "role.camera.player",
            "Камера игрока",
            G::Role,
            "Камера управляемого персонажа",
        ),
        (
            "capability.aim",
            "Прицеливание",
            G::Capability,
            "Участвует в механике прицеливания",
        ),
        (
            "capability.jump",
            "Прыжок",
            G::Capability,
            "Участвует в механике прыжка",
        ),
        (
            "capability.move",
            "Перемещение",
            G::Capability,
            "Участвует в перемещении",
        ),
        (
            "capability.interact",
            "Взаимодействие",
            G::Capability,
            "Обрабатывает взаимодействие с миром",
        ),
        (
            "capability.display_health",
            "Показывает здоровье",
            G::Capability,
            "Отображает состояние здоровья",
        ),
        (
            "capability.display_ammo",
            "Показывает боезапас",
            G::Capability,
            "Отображает боезапас",
        ),
        (
            "capability.provide_hud_state",
            "Источник HUD",
            G::Capability,
            "Передаёт состояние в игровой интерфейс",
        ),
        (
            "importance.primary",
            "Основной",
            G::Importance,
            "Главный объект своей системы",
        ),
        (
            "importance.shared",
            "Общий",
            G::Importance,
            "Переиспользуемая база или общий объект",
        ),
        (
            "importance.supporting",
            "Вспомогательный",
            G::Importance,
            "Поддерживающий объект системы",
        ),
        (
            "state.production",
            "Рабочий",
            G::State,
            "Используется в рабочем контуре игры",
        ),
        (
            "state.experimental",
            "Эксперимент",
            G::State,
            "Экспериментальная реализация",
        ),
        (
            "state.deprecated",
            "Устаревший",
            G::State,
            "Не должен использоваться в новой работе",
        ),
        (
            "scope.runtime",
            "Runtime",
            G::Scope,
            "Используется во время игры",
        ),
        (
            "scope.editor",
            "Editor",
            G::Scope,
            "Используется только в редакторе",
        ),
        ("scope.test", "Test", G::Scope, "Тестовый объект"),
    ]
}
