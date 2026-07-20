"""Apply a validated Leetcode gameplay manifest inside Unreal Editor 5.8.

The Rust side owns validation and approvals. This script intentionally accepts a
small declarative operation set and never evaluates Python supplied by a model.
"""

import json
import os
import traceback
from pathlib import Path

import unreal


MANIFEST_ENV = "LEETCODE_GAMEPLAY_MANIFEST"
SAFE_PROPERTIES = {"actor_label", "hidden", "can_be_damaged", "tags", "folder_path"}


def log(message):
    unreal.log(f"[LeetcodeGameplay] {message}")


def load_manifest():
    path = os.environ.get(MANIFEST_ENV, "").strip()
    if not path:
        raise RuntimeError(f"{MANIFEST_ENV} is not set")
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def write_result(manifest, payload):
    result_path = Path(manifest["result_path"])
    result_path.parent.mkdir(parents=True, exist_ok=True)
    with result_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)


def vector(values, default):
    values = values or default
    return unreal.Vector(float(values[0]), float(values[1]), float(values[2]))


def rotator(values):
    values = values or [0.0, 0.0, 0.0]
    return unreal.Rotator(float(values[0]), float(values[1]), float(values[2]))


def find_actor(actor_subsystem, label):
    for actor in actor_subsystem.get_all_level_actors():
        if actor.get_actor_label() == label:
            return actor
    raise RuntimeError(f"Actor not found: {label}")


def load_class(path):
    value = unreal.load_class(None, path)
    if value is None:
        raise RuntimeError(f"Class not found: {path}")
    return value


def load_asset(path):
    value = unreal.load_asset(path)
    if value is None:
        raise RuntimeError(f"Asset not found: {path}")
    return value


def spawn_actor(actor_subsystem, operation):
    location = vector(operation.get("location"), [0.0, 0.0, 0.0])
    rotation = rotator(operation.get("rotation"))
    class_path = operation.get("class_path")
    asset_path = operation.get("asset_path")
    if class_path:
        actor = actor_subsystem.spawn_actor_from_class(load_class(class_path), location, rotation)
    elif asset_path:
        actor = actor_subsystem.spawn_actor_from_object(load_asset(asset_path), location, rotation)
    else:
        raise RuntimeError("spawn_actor requires class_path or asset_path")
    if actor is None:
        raise RuntimeError("Unreal failed to spawn actor")
    if operation.get("actor_label"):
        actor.set_actor_label(operation["actor_label"], mark_dirty=True)
    if operation.get("scale"):
        actor.set_actor_scale3d(vector(operation["scale"], [1.0, 1.0, 1.0]))

    # A static mesh can be assigned declaratively without exposing arbitrary code.
    if class_path and asset_path and hasattr(actor, "static_mesh_component"):
        mesh = load_asset(asset_path)
        actor.static_mesh_component.set_static_mesh(mesh)
    return actor


def set_safe_property(actor, operation):
    name = operation.get("property")
    if name not in SAFE_PROPERTIES:
        raise RuntimeError(f"Property is not allowed: {name}")
    value = operation.get("value")
    if name == "actor_label":
        actor.set_actor_label(str(value), mark_dirty=True)
    elif name == "hidden":
        actor.set_actor_hidden_in_game(bool(value))
    elif name == "tags":
        actor.set_editor_property("tags", [unreal.Name(str(item)) for item in (value or [])])
    elif name == "folder_path":
        actor.set_folder_path(str(value))
    else:
        actor.set_editor_property(name, value)


def add_actor_component(actor, operation):
    component_class = load_class(operation.get("class_path"))
    component_name = operation.get("component_name")
    if not component_name:
        raise RuntimeError("add_actor_component requires component_name")

    subsystem = unreal.get_engine_subsystem(unreal.SubobjectDataSubsystem)
    library = unreal.SubobjectDataBlueprintFunctionLibrary
    handles = subsystem.k2_gather_subobject_data_for_instance(actor)
    if not handles:
        raise RuntimeError(f"Failed to inspect actor components: {actor.get_actor_label()}")

    actor_handle = handles[0]
    params = unreal.AddNewSubobjectParams(
        parent_handle=actor_handle,
        new_class=component_class,
        blueprint_context=None,
        conform_transform_to_parent=True,
    )
    component_handle, failure = subsystem.add_new_subobject(params)
    if not library.is_handle_valid(component_handle):
        raise RuntimeError(f"Failed to create component: {failure}")
    if not subsystem.rename_subobject(component_handle, unreal.Text(component_name)):
        subsystem.delete_subobject(actor_handle, component_handle, None)
        raise RuntimeError(f"Failed to name component: {component_name}")

    component_data = library.get_data(component_handle)
    component = library.get_associated_object(component_data)
    if component is None:
        raise RuntimeError(f"Failed to resolve component: {component_name}")

    if isinstance(component, unreal.SceneComponent):
        root_handle = next(
            (
                handle
                for handle in handles
                if library.is_root_component(library.get_data(handle))
            ),
            None,
        )
        if root_handle is not None:
            if not subsystem.attach_subobject(root_handle, component_handle):
                raise RuntimeError(f"Failed to attach component: {component_name}")
        elif not subsystem.make_new_scene_root(actor_handle, component_handle, None):
            raise RuntimeError(f"Failed to create scene root: {component_name}")
        if operation.get("location"):
            component.set_relative_location(
                vector(operation["location"], [0.0, 0.0, 0.0]), False, False
            )
        if operation.get("rotation"):
            component.set_relative_rotation(rotator(operation["rotation"]), False, False)
        if operation.get("scale"):
            component.set_relative_scale3d(
                vector(operation["scale"], [1.0, 1.0, 1.0])
            )
    actor.modify()
    return component


def create_data_asset(operation):
    package_path = operation.get("package_path")
    class_path = operation.get("class_path")
    if not package_path or not class_path:
        raise RuntimeError("create_data_asset requires package_path and class_path")
    package_folder, asset_name = package_path.rsplit("/", 1)
    asset_class = load_class(class_path)
    tools = unreal.AssetToolsHelpers.get_asset_tools()
    existing = unreal.load_asset(package_path)
    if existing is not None:
        return existing
    factory = unreal.DataAssetFactory()
    factory.set_editor_property("data_asset_class", asset_class)
    created = tools.create_asset(asset_name, package_folder, asset_class, factory)
    if created is None:
        raise RuntimeError(f"Failed to create Data Asset: {package_path}")
    return created


def apply_manifest(manifest):
    actor_subsystem = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
    level_subsystem = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
    map_path = manifest["map_path"]
    if manifest.get("create_map"):
        loaded = level_subsystem.new_level(map_path)
    else:
        loaded = level_subsystem.load_level(map_path)
    if not loaded:
        raise RuntimeError(f"Failed to load/create level: {map_path}")

    completed = []
    affected = []
    for index, operation in enumerate(manifest.get("operations", [])):
        kind = operation["operation"]
        if kind == "load_level":
            if not level_subsystem.load_level(map_path):
                raise RuntimeError(f"Failed to load level: {map_path}")
        elif kind == "create_level":
            if not level_subsystem.new_level(map_path):
                raise RuntimeError(f"Failed to create level: {map_path}")
        elif kind == "spawn_actor":
            actor = spawn_actor(actor_subsystem, operation)
            affected.append(actor.get_path_name())
        elif kind == "add_actor_component":
            actor = find_actor(actor_subsystem, operation["actor_label"])
            component = add_actor_component(actor, operation)
            affected.extend([actor.get_path_name(), component.get_path_name()])
        elif kind == "delete_actor":
            actor = find_actor(actor_subsystem, operation["actor_label"])
            affected.append(actor.get_path_name())
            if not actor_subsystem.destroy_actor(actor):
                raise RuntimeError(f"Failed to delete actor: {operation['actor_label']}")
        elif kind == "set_actor_transform":
            actor = find_actor(actor_subsystem, operation["actor_label"])
            if operation.get("location"):
                actor.set_actor_location(vector(operation["location"], [0.0, 0.0, 0.0]), False, False)
            if operation.get("rotation"):
                actor.set_actor_rotation(rotator(operation["rotation"]), False)
            if operation.get("scale"):
                actor.set_actor_scale3d(vector(operation["scale"], [1.0, 1.0, 1.0]))
            affected.append(actor.get_path_name())
        elif kind == "set_actor_property":
            actor = find_actor(actor_subsystem, operation["actor_label"])
            set_safe_property(actor, operation)
            affected.append(actor.get_path_name())
        elif kind == "create_data_asset":
            asset = create_data_asset(operation)
            affected.append(asset.get_path_name())
        elif kind == "save_level":
            if not level_subsystem.save_current_level():
                raise RuntimeError("Failed to save current level")
        else:
            raise RuntimeError(f"Unsupported gameplay operation: {kind}")
        completed.append({"index": index, "operation": kind})

    if manifest.get("save_level", True) and not level_subsystem.save_current_level():
        raise RuntimeError("Failed to save current level")
    unreal.EditorAssetLibrary.save_loaded_assets(
        [asset for path in affected if (asset := unreal.load_asset(path)) is not None],
        only_if_is_dirty=True,
    )
    return {
        "ok": True,
        "manifest_id": manifest["id"],
        "map_path": map_path,
        "completed": completed,
        "affected_objects": sorted(set(affected)),
    }


def main():
    manifest = load_manifest()
    try:
        result = apply_manifest(manifest)
        write_result(manifest, result)
        log(f"completed {manifest['id']}")
    except Exception as error:
        result = {
            "ok": False,
            "manifest_id": manifest.get("id"),
            "error": str(error),
            "traceback": traceback.format_exc(),
        }
        write_result(manifest, result)
        unreal.log_error(f"[LeetcodeGameplay] {error}")
        raise


if __name__ == "__main__":
    main()
