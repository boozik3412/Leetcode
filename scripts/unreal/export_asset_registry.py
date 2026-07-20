"""Export Unreal Asset Registry data for Leetcode Project Map.

Run inside Unreal Editor 5.8, for example from the Python console:
    py "<project>/scripts/unreal/export_asset_registry.py"
"""

import json
import os

import unreal


SEMANTIC_TAGS = (
    "Skeleton",
    "TargetSkeleton",
    "PreviewSkeletalMesh",
    "SkeletalMesh",
    "PhysicsAsset",
    "ParentClass",
    "NativeParentClass",
    "GeneratedClass",
    "InputAction",
    "InputMappingContext",
)


def text(value):
    return "" if value is None else str(value)


def property_value(value, name, default=None):
    try:
        return value.get_editor_property(name)
    except Exception:
        return getattr(value, name, default)


def object_path(asset):
    package_name = text(property_value(asset, "package_name"))
    asset_name = text(property_value(asset, "asset_name"))
    return "{}.{}".format(package_name, asset_name) if package_name and asset_name else package_name


def class_path(asset):
    value = property_value(asset, "asset_class_path")
    if value:
        name = text(property_value(value, "asset_name"))
        if name:
            return name
    return text(property_value(asset, "asset_class"))


def tags(asset):
    result = {}
    for key in SEMANTIC_TAGS:
        try:
            value = asset.get_tag_value(key)
        except Exception:
            value = None
        value = text(value)
        if value:
            result[key] = value
    return result


registry = unreal.AssetRegistryHelpers.get_asset_registry()
registry.wait_for_completion()
dependency_options = unreal.AssetRegistryDependencyOptions()
for name in (
    "include_hard_package_references",
    "include_soft_package_references",
    "include_hard_management_references",
    "include_soft_management_references",
):
    try:
        dependency_options.set_editor_property(name, True)
    except Exception:
        pass

project_assets = registry.get_assets_by_path("/Game", True, True) or []
if not project_assets:
    # Some custom UE builds expose a reduced get_assets_by_path binding.
    project_assets = [
        asset
        for asset in (registry.get_all_assets(True) or [])
        if text(property_value(asset, "package_name")).startswith("/Game/")
    ]
assets = []
dependencies = []
for asset in project_assets:
    package_name = text(property_value(asset, "package_name"))
    path = object_path(asset)
    assets.append(
        {
            "object_path": path,
            "package_name": package_name,
            "asset_name": text(property_value(asset, "asset_name")),
            "asset_class": class_path(asset),
            "tags": tags(asset),
        }
    )
    for dependency in registry.get_dependencies(package_name, dependency_options) or []:
        dependencies.append(
            {
                "from": package_name,
                "to": text(dependency),
                "type": "package",
            }
        )

output_path = os.path.join(
    unreal.Paths.project_dir(),
    "assets",
    "generated",
    "leetcode",
    "unreal",
    "asset_registry.json",
)
os.makedirs(os.path.dirname(output_path), exist_ok=True)
with open(output_path, "w", encoding="utf-8") as output:
    json.dump(
        {
            "schema_version": 1,
            "assets": assets,
            "dependencies": dependencies,
        },
        output,
        ensure_ascii=False,
        separators=(",", ":"),
    )

unreal.log(
    "Leetcode Asset Registry export: {} assets, {} dependencies -> {}".format(
        len(assets), len(dependencies), output_path
    )
)
