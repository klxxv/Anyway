# Research Canvas plugins

The directory follows a module-oriented layout similar to Go modules:

```text
plugins/
  packages/                       distributable *.myc archives
  installed/                      verified expanded modules
  sdk/python/research_canvas.py   reference lifecycle contract
  sources/<plugin-id>/            source tree for one module
```

A `.myc` file is a ZIP archive with two root files:

```text
plugin.yml    version, developer, description, capabilities, permissions
theme.json    the entry declared by a ThemePlugin
```

Build a package:

```bash
python scripts/build_myc_plugin.py \
  plugins/sources/researchcanvas.onedarkpro \
  plugins/packages/researchcanvas.onedarkpro@1.0.0.myc
```

In development, the Tauri client scans `plugins/packages`, extracts verified
packages into `plugins/installed/<id>@<version>`, and loads their manifests.
Dropping a `.myc` file onto the Plugin Store invokes the same installer.

The current MVP executes no third-party code. A theme plugin is declarative and
may only register semantic color tokens. Future executable plugins must receive
the narrow capability context shown in the Python SDK and must not import the
application store directly.

