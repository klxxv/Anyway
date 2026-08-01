# Git Workspace

Uses fixed `git` arguments without a shell. Autosave is opt-in, writes one
`.research-canvas/*.mycproj` file, and commits only that staged project snapshot.
History import returns a review-required GraphPatch; it never mutates the canvas.

Supported commit-body directives:

```text
canvas-node: block-id|method|Fourier embedding
canvas-edge: block-id|supports|experiment-id
ablation: fourier-off|fourier=false|hiddenDim=64|hiddenLayers=10
```
