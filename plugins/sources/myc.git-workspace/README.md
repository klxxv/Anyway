# Git Workspace

Uses fixed `git` arguments without a shell. Autosave is opt-in, writes one
`.research-canvas/*.mycproj` file, and commits only that staged project snapshot.
History import returns a review-required GraphPatch; it never mutates the canvas.
Opening a normal folder returns a safe non-repository state; initialization is an
explicit capability-gated action. Empty repositories are supported.

GitHub account access is mediated by the official `gh` browser login and system
credential store. The host can generate one app-named Ed25519 key without
returning private key material to the plugin, list public keys, and explicitly
upload a selected public key through `gh ssh-key add`. Organization SAML SSO
authorization remains a GitHub-hosted user action.

Supported commit-body directives:

```text
canvas-node: block-id|method|Fourier embedding
canvas-edge: block-id|supports|experiment-id
ablation: fourier-off|fourier=false|hiddenDim=64|hiddenLayers=10
```
