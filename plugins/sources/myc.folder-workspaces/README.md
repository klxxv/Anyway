# Folder Workspaces

Declares `project.folder`. The host owns a bounded, lazy Explorer tree:

- `scan_project_folder` returns compatible project summaries for the existing
  project-open flow.
- `list_folder_entries` returns one directory level at a time for the selected
  root. The plugin receives no filesystem handle and cannot request a path
  outside that root.
- The host skips symbolic links, limits each response to 1,000 entries, and
  rejects paths that leave the selected root.

`workspace-plugin.json` declares `config.tree.lazy: true` so future Workspace
plugins can opt into the same UI contract without implementing a filesystem
walker.

The Folder UI is intentionally Explorer-like: folders expand on demand, files
are visible as leaf nodes, and the existing project cards remain available for
opening `.mycproj` files.
