"""Import a validated Leetcode 3D asset into Unreal Engine 5.8.

Run through UnrealEditor-Cmd with LEETCODE_3D_IMPORT_MANIFEST pointing to the
JSON manifest created by the Rust application. Unreal routes supported GLB,
glTF and USD sources through Interchange; FBX follows the project feature flag.
"""

import json
import os
import traceback

import unreal


def load_manifest():
    path = os.environ.get("LEETCODE_3D_IMPORT_MANIFEST", "").strip()
    if not path:
        raise RuntimeError("LEETCODE_3D_IMPORT_MANIFEST is not set")
    with open(path, "r", encoding="utf-8") as stream:
        return path, json.load(stream)


def fbx_options(manifest):
    source = manifest["source_file"].lower()
    if not source.endswith(".fbx"):
        return None

    asset_type = manifest.get("asset_type", "static_mesh")
    options = unreal.FbxImportUI()
    options.set_editor_property("automated_import_should_detect_type", False)
    options.set_editor_property("import_materials", True)
    options.set_editor_property("import_textures", True)
    options.set_editor_property("import_animations", asset_type == "animation")
    options.set_editor_property("import_as_skeletal", asset_type != "static_mesh")

    if asset_type == "static_mesh":
        options.set_editor_property("mesh_type_to_import", unreal.FBXImportType.FBXIT_STATIC_MESH)
        static_data = options.get_editor_property("static_mesh_import_data")
        static_data.set_editor_property("import_mesh_lods", manifest.get("import_lods", True))
        static_data.set_editor_property("combine_meshes", False)
        static_data.set_editor_property("generate_lightmap_u_vs", True)
    else:
        options.set_editor_property("mesh_type_to_import", unreal.FBXImportType.FBXIT_SKELETAL_MESH)
        skeletal_data = options.get_editor_property("skeletal_mesh_import_data")
        skeletal_data.set_editor_property("import_mesh_lo_ds", manifest.get("import_lods", True))
        skeleton_path = manifest.get("skeleton_path")
        if skeleton_path:
            skeleton = unreal.load_asset(skeleton_path)
            if not skeleton:
                raise RuntimeError("Skeleton was not found: " + skeleton_path)
            options.set_editor_property("skeleton", skeleton)
    return options


def import_asset(manifest):
    task = unreal.AssetImportTask()
    task.set_editor_property("filename", manifest["source_file"])
    task.set_editor_property("destination_path", manifest["destination_path"])
    task.set_editor_property("automated", True)
    task.set_editor_property("save", True)
    task.set_editor_property("replace_existing", manifest.get("replace_existing", True))
    task.set_editor_property("replace_existing_settings", manifest.get("replace_existing", True))

    options = fbx_options(manifest)
    if options:
        task.set_editor_property("options", options)

    # AssetTools uses the project's configured Interchange pipeline for
    # supported formats. This also preserves reimport source metadata.
    unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks([task])
    imported = list(task.get_editor_property("imported_object_paths"))
    if not imported:
        raise RuntimeError("Unreal did not return an imported object path")
    return imported


def configure_static_mesh(asset, manifest, warnings):
    if not isinstance(asset, unreal.StaticMesh):
        return

    if manifest.get("enable_nanite", False):
        try:
            settings = asset.get_editor_property("nanite_settings")
            settings.set_editor_property("enabled", True)
            asset.set_editor_property("nanite_settings", settings)
        except Exception as error:
            warnings.append("Nanite was not configured: " + str(error))

    collision = manifest.get("collision", "auto")
    if collision == "complex":
        try:
            body_setup = asset.get_editor_property("body_setup")
            body_setup.set_editor_property(
                "collision_trace_flag", unreal.CollisionTraceFlag.CTF_USE_COMPLEX_AS_SIMPLE
            )
        except Exception as error:
            warnings.append("Complex collision was not configured: " + str(error))
    elif collision in ("auto", "simple"):
        try:
            subsystem = unreal.get_editor_subsystem(unreal.StaticMeshEditorSubsystem)
            subsystem.add_simple_collisions(
                asset, unreal.ScriptingCollisionShapeType.NDOP18
            )
        except Exception as error:
            warnings.append("Simple collision generation was skipped: " + str(error))


def save_and_report(manifest, imported):
    warnings = []
    classes = []
    for object_path in imported:
        asset = unreal.load_asset(object_path)
        if not asset:
            warnings.append("Could not load imported object: " + object_path)
            continue
        classes.append(asset.get_class().get_name())
        configure_static_mesh(asset, manifest, warnings)
        unreal.EditorAssetLibrary.save_loaded_asset(asset, only_if_is_dirty=False)

    result = {
        "ok": True,
        "manifest_id": manifest["id"],
        "source_file": manifest["source_file"],
        "destination_path": manifest["destination_path"],
        "asset_type": manifest["asset_type"],
        "imported_object_paths": imported,
        "classes": classes,
        "warnings": warnings,
    }
    with open(manifest["result_path"], "w", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
    unreal.log("LEETCODE_3D_IMPORT_RESULT=" + json.dumps(result, ensure_ascii=False))


def write_failure(manifest, error):
    result = {
        "ok": False,
        "manifest_id": manifest.get("id"),
        "error": str(error),
        "traceback": traceback.format_exc(),
    }
    result_path = manifest.get("result_path")
    if result_path:
        with open(result_path, "w", encoding="utf-8") as stream:
            json.dump(result, stream, ensure_ascii=False, indent=2)
    unreal.log_error("LEETCODE_3D_IMPORT_RESULT=" + json.dumps(result, ensure_ascii=False))


def main():
    manifest = {}
    try:
        _, manifest = load_manifest()
        imported = import_asset(manifest)
        save_and_report(manifest, imported)
    except Exception as error:
        write_failure(manifest, error)
        raise


main()
