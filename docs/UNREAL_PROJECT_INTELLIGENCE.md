# Unreal Project Intelligence

Stage 39 extends the shared Project Map instead of creating a separate Unreal database. A refresh scans Unreal descriptors and source structure, imports editor-authored Asset Registry data, and merges the result with manual graph relations.

## What Is Scanned

- `.uproject` and `.uplugin` descriptors, including declared module types.
- `*.Build.cs` public, private, and dynamically loaded dependencies.
- `*.Target.cs` target type and `ExtraModuleNames`.
- `Config/**/*.ini` sections and keys.
- C++, header, and C# files under project/plugin `Source` directories.
- `.umap` and `.uasset` paths under project/plugin `Content` as a safe fallback.

`Binaries`, `DerivedDataCache`, `Intermediate`, `Saved`, dependency caches, and build output are excluded from the structural scan. Binary `.uasset`, `.umap`, and `AssetRegistry.bin` files are never parsed as text.

## Asset Registry Export

The preferred source is:

```text
assets/generated/leetcode/unreal/asset_registry.json
```

Inside Unreal Editor 5.8, run `scripts/unreal/export_asset_registry.py` through the Python console. The script uses `AssetRegistryHelpers.get_asset_registry()`, waits for discovery to complete, exports project assets under `/Game`, reads semantic tags through `AssetData.get_tag_value`, and asks the registry for package dependencies. The same JSON can be selected manually with `Project Map -> Import Asset Registry`.

The importer accepts exports up to 96 MB and reconstructs a missing UE 5.8 `object_path` as `<package_name>.<asset_name>`. Engine and plugin dependencies remain visible as external graph context. Missing `/Game` targets are reported as project diagnostics but do not reduce source readiness after a complete Asset Registry pass.

Supported records are intentionally tolerant of common Unreal/export naming styles:

```json
{
  "assets": [
    {
      "object_path": "/Game/Blueprints/BP_Player.BP_Player",
      "package_name": "/Game/Blueprints/BP_Player",
      "asset_name": "BP_Player",
      "asset_class": "Blueprint",
      "tags": {},
      "dependencies": [
        { "target": "/Game/Data/DA_Player", "type": "hard" }
      ]
    }
  ],
  "dependencies": [
    { "from": "/Game/Maps/L_Main", "to": "/Game/Blueprints/BP_Player", "type": "hard" }
  ]
}
```

The importer classifies maps, Blueprints, Data Assets, materials, Niagara systems, and animation assets. Unknown classes remain `unreal_asset` nodes instead of being discarded.

## Incremental Refresh

`project_graph_snapshot` with `refresh=true` and both Project Map refresh buttons use an incremental merge:

- unchanged nodes and edges keep their original `updated_at`;
- scanner-owned nodes are replaced when their source data changes;
- manual `ui:project_map` relations survive refresh while both endpoints exist;
- the current node selection is persisted in `project_graph_selection.json` and restored with the project.

## Agent And MCP Context

Selecting a Project Map node makes it active task context. The context contains the exact serialized node, up to 40 incident edges, and neighbouring nodes.

The desktop map keeps the parsed graph plus incoming, outgoing and structural `contains/declares` indexes in a workspace-scoped cache. It exposes four coordinated views: aggregate subsystem overview, a paged local hierarchy browser, a focused incoming/outgoing dependency explorer, and a depth-limited causal impact flow. The hierarchy browser renders only the parent, active container, twelve children and an eight-node next-level preview; double-click drills into a container while breadcrumbs, parent, root and history actions keep the route reversible. Impact traversal interprets relation semantics instead of copying the dependency view: dependency-style edges are followed in reverse while producer/configurator edges are followed forward. Only the selected visible subgraph is laid out and rendered. Every visible edge is directed and its hover detail includes the localized relation label, stable technical kind, source and confidence. Pan, cursor-anchored wheel zoom, paging and selection therefore do not trigger JSON parsing, structural index reconstruction or a project rescan.

- The block is included in the next agent prompt and in the current user turn.
- `mcp_call` accepts an optional `context_node_id`; otherwise it uses the persisted selection.
- The resolved context is attached to the MCP `tools/call` request as namespaced protocol metadata at `_meta["com.leetcode/projectNodeContext"]`.
- Tool arguments are not modified, so strict Unreal MCP schemas continue to work.

## Verification

```powershell
.\.cargo\bin\cargo.exe fmt --all
.\.cargo\bin\cargo.exe check
.\.cargo\bin\cargo.exe test
```

The Unreal fixture under `tests/fixtures/unreal/SampleGame` covers descriptors, modules, targets, config, source, all required asset classes, dependency edges, incremental refresh, selection persistence, and MCP metadata.

Official references:

- https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/AssetRegistryHelpers?application_version=5.8
- https://dev.epicgames.com/documentation/en-us/unreal-engine/asset-registry-in-unreal-engine
- https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/AssetRegistry/IAssetRegistry/GetDependencies
