# Anyway Schema v3
## LLM Extraction Schema + Canvas Compiler IR
### Handoff Implementation Specification

**Schema version:** `v3`  
**Target:** Anyway MVP  
**Primary domain for validation:** PINN research literature  
**Status:** implementation handoff

---

# 1. Purpose

Schema v3 defines the contract between:

\[
\boxed{
Scientific\ Source
\rightarrow
LLM\ Extractor
\rightarrow
ExtractionV3
\rightarrow
Canvas\ Compiler
\rightarrow
CanvasIRV3
}
\]

The architecture deliberately separates two responsibilities.

### LLM Extractor

The LLM answers:

> What is explicitly present in the source?

It extracts:

- variables;
- numerical values;
- mathematical expressions;
- baseline/proposed states;
- context;
- assumptions;
- experiments;
- reported results;
- candidate relations;
- exact provenance.

### Canvas Compiler

The compiler answers:

> What computational structure follows from these extracted facts?

It constructs:

- canonical variables;
- interventions;
- Blocks;
- Operators;
- Chains;
- Fibers;
- Bundles;
- identifiability states;
- consistency checks;
- hashes;
- graph indexes.

The central boundary is:

\[
\boxed{
LLM\ extracts\ semantics;
\quad
Compiler\ constructs\ computation.
}
\]

---

# 2. Two root schemas

Schema v3 contains two independent versioned root objects.

```text
anyway.extract.v3
anyway.canvas-ir.v3
```

They are the only stable cross-module contracts required by the MVP.

Internal databases, model providers, retrieval systems, graph engines, and storage implementations may change without changing these contracts.

---

# 3. ExtractionV3 root

```json
{
  "schema_version": "anyway.extract.v3",
  "document": {},
  "evidence": [],
  "variables": [],
  "contexts": [],
  "axiom_sets": [],
  "experiments": [],
  "operator_candidates": [],
  "abstraction_candidates": []
}
```

Required fields:

```text
schema_version
document
evidence
variables
contexts
axiom_sets
experiments
operator_candidates
abstraction_candidates
```

Arrays may be empty.

The LLM must never omit a required root field.

---

# 4. ID conventions

Every object receives a locally unique stable ID.

Recommended prefixes:

```text
doc_
ev_
var_
ctx_
ax_
exp_
state_
opc_
qcan_

block_
op_
chain_
fiber_
bundle_
check_
idn_
```

Example:

```text
doc_01H...
ev_01H...
var_01H...
```

ULID or UUIDv7 is recommended.

IDs are identity keys only.

Scientific equivalence is determined separately through canonical representations and semantic hashes.

---

# 5. Document object

```json
{
  "document_id": "doc_001",
  "title": "Physics-informed neural networks ...",
  "authors": [
    "Author A",
    "Author B"
  ],
  "year": 2025,
  "doi": "10.xxxx/xxxx",
  "arxiv_id": "2501.00001",
  "url": "https://...",
  "source_type": "paper"
}
```

Minimum required:

```text
document_id
source_type
```

Recommended `source_type`:

```text
paper
preprint
repository
dataset
supplement
benchmark
experiment
report
other
```

These values describe provenance and are not scientific variable types.

---

# 6. Evidence object

Evidence is immutable provenance.

```json
{
  "id": "ev_001",
  "document_id": "doc_001",

  "location": {
    "page": 6,
    "section": "4.2 Ablation Study",
    "paragraph": 3,
    "table": null,
    "figure": null,
    "equation": null
  },

  "text_span": "We introduce Fourier features with sigma = 10.",

  "verification": {
    "status": "supported",
    "confidence": 0.98
  }
}
```

---

# 6.1 Evidence status

Allowed:

```text
supported
ambiguous
unsupported
```

Meaning:

### supported

The source directly supports the extracted fact.

### ambiguous

Relevant information exists, but interpretation is uncertain.

### unsupported

The extraction candidate cannot be justified from the source.

Only `supported` evidence enters the default computational graph.

`ambiguous` information may remain in the extraction record.

`unsupported` information must never become canonical scientific state.

---

# 6.2 Evidence immutability

After ingestion:

```text
document_id
location
text_span
```

must be immutable.

Verification metadata may change through re-review.

Never rewrite the evidence span to match the canonical concept.

---

# 7. Scientific primitive types

Schema v3 uses exactly three scientific primitive value types:

\[
\boxed{
Bool
\cup
Number
\cup
Expression
}
\]

Allowed JSON values:

```text
bool
number
expression
```

No first-class:

```text
enum
string category
array type
distribution type
method type
```

Complex scientific concepts are represented compositionally.

---

# 8. Variable object

Canonical extraction envelope:

```json
{
  "id": "var_001",

  "concept_id": "representation.fourier.enabled",

  "value_type": "bool",

  "observed": true,

  "value": true,

  "unit_raw": null,

  "expression_raw": null,

  "evidence_refs": [
    "ev_001"
  ]
}
```

Required:

```text
id
concept_id
value_type
observed
value
unit_raw
expression_raw
evidence_refs
```

---

# 9. Bool variable

```json
{
  "id": "var_fourier_enabled",
  "concept_id": "representation.fourier.enabled",
  "value_type": "bool",
  "observed": true,
  "value": true,
  "unit_raw": null,
  "expression_raw": null,
  "evidence_refs": ["ev_001"]
}
```

Bool represents presence/absence of a mechanism or structural choice.

Examples:

```text
residual.enabled
residual.strong_form.enabled
residual.weak_form.enabled

representation.fourier.enabled
representation.siren.enabled

loss.dynamic.enabled

optimizer.adam.enabled
optimizer.lbfgs.enabled

normalization.rmsnorm.enabled
normalization.layernorm.enabled

position.rope.enabled
```

---

# 10. Number variable

```json
{
  "id": "var_sigma",
  "concept_id": "representation.fourier.sigma",
  "value_type": "number",
  "observed": true,
  "value": 10.0,
  "unit_raw": null,
  "expression_raw": null,
  "evidence_refs": ["ev_002"]
}
```

Examples:

```text
training.learning_rate = 0.001
representation.fourier.sigma = 10
representation.fourier.dimension = 128
physics.viscosity = 0.01
sampling.sample_count = 10000
result.relative_l2_error = 0.021
```

---

# 11. Expression variable

```json
{
  "id": "var_residual_expression",
  "concept_id": "residual.expression",
  "value_type": "expression",
  "observed": true,
  "value": null,
  "unit_raw": null,
  "expression_raw": "u_t + u*u_x - nu*u_xx",
  "evidence_refs": ["ev_003"]
}
```

Expression represents mathematical or algorithmic structure.

Examples:

\[
u_t+uu_x-\nu u_{xx}
\]

\[
\phi(x)=
[\sin(Bx),\cos(Bx)]
\]

\[
\lambda_r(t)
=
\frac{\|\nabla_\theta L_b\|}
{\|\nabla_\theta L_r\|+\epsilon}
\]

Compiler may later produce:

```json
{
  "ast": {
    "op": "add",
    "args": []
  }
}
```

The LLM does not need to generate a fully validated AST.

Raw expression preservation has priority.

---

# 12. Unknown semantics

`Unknown` is an observation state.

It is not a fourth primitive type.

Example:

```json
{
  "id": "var_sigma_unknown",
  "concept_id": "representation.fourier.sigma",
  "value_type": "number",
  "observed": false,
  "value": null,
  "unit_raw": null,
  "expression_raw": null,
  "evidence_refs": ["ev_010"]
}
```

Interpretation:

> The relevant source was inspected, but the value could not be determined.

Critical distinction:

```text
observed=true, value=false
```

means explicit absence.

```text
observed=false, value=null
```

means unknown.

Therefore:

\[
\boxed{
false\neq unknown.
}
\]

---

# 13. Variable invariants

Compiler validation must enforce:

```text
VAR-001 value_type ∈ {bool, number, expression}

VAR-002 observed = false
        => value = null

VAR-003 observed = false
        => expression_raw = null

VAR-004 observed = true
        => evidence_refs.length >= 1

VAR-005 value_type = bool AND observed = true
        => value ∈ {true,false}

VAR-006 value_type = number AND observed = true
        => value is finite numeric

VAR-007 value_type = expression AND observed = true
        => expression_raw != null

VAR-008 value_type = expression
        => value = null
```

---

# 14. Concept IDs

Concept IDs use hierarchical dot notation.

Examples:

```text
physics.pde.burgers.enabled
physics.viscosity

residual.enabled
residual.strong_form.enabled
residual.adaptive.enabled
residual.expression

representation.fourier.enabled
representation.fourier.sigma
representation.siren.enabled

loss.dynamic.enabled
loss.residual_weight
loss.weight_expression

sampling.random.enabled
sampling.adaptive.enabled

optimizer.adam.enabled
optimizer.learning_rate

result.relative_l2_error
```

Concept IDs describe semantic identity.

They are not variable IDs.

Many variables from many papers may share one `concept_id`.

---

# 15. Categorical concepts

Categorical scientific choices are decomposed into Bool features.

Example:

```text
Normalization = RMSNorm
```

becomes:

```text
normalization.rmsnorm.enabled = true
normalization.layernorm.enabled = false
```

Another model:

```text
normalization.rmsnorm.enabled = false
normalization.layernorm.enabled = true
```

A compiler-side constraint may express:

\[
RMSNorm+LayerNorm\le1.
\]

The constraint belongs to the graph logic.

It does not require an Enum type.

---

# 16. Hierarchical feature composition

Complex techniques use Bool roots with Number or Expression children.

Example:

```text
representation.fourier.enabled = true
representation.fourier.sigma = 10
representation.fourier.dimension = 128
representation.fourier.trainable.enabled = false
```

Dynamic weighting:

```text
loss.dynamic.enabled = true
loss.weight_expression = Expression(...)
loss.update_interval = 100
```

RoPE:

```text
position.rope.enabled = true
position.rope.base = 500000
position.rope.scaling.enabled = true
position.rope.scaling.factor = 4
```

This yields:

\[
\boxed{
Complex\ Method
=
Bool\ topology
+
Number\ parameters
+
Expression\ relations.
}
\]

---

# 17. Context object

Context defines conditions under which a scientific relation is interpreted.

```json
{
  "id": "ctx_001",

  "variable_refs": [
    "var_pde_burgers",
    "var_viscosity",
    "var_architecture_depth",
    "var_optimizer_adam"
  ],

  "evidence_refs": [
    "ev_020",
    "ev_021"
  ]
}
```

Context references variables.

It does not duplicate variable values.

Typical context concepts:

```text
PDE
dataset
problem domain
boundary conditions
initial conditions
architecture
optimizer
evaluation protocol
hardware
training budget
random seed policy
physical constants
```

Which fields are considered conditioning variables depends on the downstream comparison.

---

# 18. Context identity

Compiler creates a canonical context signature:

\[
C=
\{
(concept_i,canonicalValue_i)
\}.
\]

Recommended:

```text
sort by concept_id
normalize values
remove provenance
hash canonical serialization
```

This becomes:

```text
context_semantic_hash
```

Two experiments can share semantic context even when they come from different papers.

---

# 19. AxiomSet object

AxiomSet describes assumptions under which inference is valid.

```json
{
  "id": "ax_001",

  "constraint_refs": [
    "var_pde_expression",
    "var_boundary_periodic",
    "var_viscosity_positive"
  ],

  "evidence_refs": [
    "ev_030",
    "ev_031"
  ]
}
```

Examples:

```text
governing PDE
boundary assumptions
conservation assumptions
model assumptions
evaluation assumptions
mathematical premises
```

Relations must conceptually be interpreted as:

\[
\boxed{
P(Y|X,C,A)
}
\]

where:

\[
C=Context
\]

and:

\[
A=AxiomSet.
\]

---

# 20. Experiment object

An Experiment groups comparable states.

```json
{
  "id": "exp_001",

  "context_ref": "ctx_001",

  "axiom_set_ref": "ax_001",

  "states": [],

  "comparisons": [],

  "evidence_refs": []
}
```

The LLM extracts experimental states.

The compiler derives interventions.

---

# 21. State object

```json
{
  "id": "state_baseline",

  "role": "baseline",

  "variable_refs": [
    "var_residual_strong_true",
    "var_fourier_false",
    "var_dynamic_false"
  ],

  "result_refs": [
    "var_result_l2_011"
  ],

  "evidence_refs": [
    "ev_040"
  ]
}
```

Allowed initial roles:

```text
baseline
proposed
ablation
control
variant
reference
```

Role is document structure metadata.

It is not part of scientific variable type algebra.

---

# 22. Proposed state

```json
{
  "id": "state_proposed",

  "role": "proposed",

  "variable_refs": [
    "var_residual_adaptive_true",
    "var_fourier_true",
    "var_fourier_sigma_10",
    "var_dynamic_true"
  ],

  "result_refs": [
    "var_result_l2_0021"
  ],

  "evidence_refs": [
    "ev_041"
  ]
}
```

---

# 23. State comparison

```json
{
  "from_state": "state_baseline",
  "to_state": "state_proposed",
  "evidence_refs": [
    "ev_042"
  ]
}
```

The LLM only needs to indicate that the source compares these states.

It should not determine individual causal effects.

---

# 24. State construction rule

A State is a sparse configuration.

A missing concept does not automatically receive `false`.

For example, if a paper specifies:

```text
Fourier = true
sigma = 10
```

and says nothing about adaptive sampling, then:

```text
sampling.adaptive.enabled
```

remains unknown unless evidence establishes it.

---

# 25. Compiler StateDiff

The compiler compares two states:

\[
X_0
\rightarrow
X_1.
\]

For Bool:

\[
\Delta b
\in
\{-1,0,+1\}.
\]

For Number:

\[
\Delta x=x_1-x_0.
\]

For Expression:

\[
\Delta e=(e_0,e_1).
\]

Unknown values do not produce confirmed intervention dimensions.

---

# 26. Intervention construction

Given:

```text
baseline:
    residual.strong_form = true
    representation.fourier.enabled = false
    loss.dynamic.enabled = false

proposed:
    residual.strong_form = false
    residual.adaptive.enabled = true
    representation.fourier.enabled = true
    loss.dynamic.enabled = true
```

Compiler generates one joint intervention:

\[
I=
\{
\Delta Residual,
\Delta Fourier,
\Delta DynamicLoss
\}.
\]

Compiled structure:

```json
{
  "id": "op_I_001",
  "operator": "I",

  "input_refs": [
    "block_state_baseline"
  ],

  "output_refs": [
    "block_state_proposed"
  ],

  "payload": {
    "changes": [
      {
        "concept_id": "residual.strong_form.enabled",
        "before": true,
        "after": false
      },
      {
        "concept_id": "residual.adaptive.enabled",
        "before": false,
        "after": true
      },
      {
        "concept_id": "representation.fourier.enabled",
        "before": false,
        "after": true
      },
      {
        "concept_id": "loss.dynamic.enabled",
        "before": false,
        "after": true
      }
    ]
  }
}
```

---

# 27. Joint intervention invariant

When:

\[
|changes|>1,
\]

the compiler must preserve one joint intervention.

It must not immediately construct:

\[
Residual\rightarrow Y
\]

\[
Fourier\rightarrow Y
\]

\[
DynamicLoss\rightarrow Y.
\]

Current evidence directly supports:

\[
\boxed{
P(
Y
|
do(R,F,W),
C,A
).
}
\]

Independent effects require additional controls.

---

# 28. Operator candidate root

LLM may identify explicit relations from the source.

```json
{
  "id": "opc_001",

  "operator": "T",

  "input_refs": [
    "var_input_x"
  ],

  "output_refs": [
    "var_fourier_representation"
  ],

  "payload": {
    "expression_ref": "var_fourier_expression"
  },

  "context_ref": "ctx_001",

  "axiom_set_ref": "ax_001",

  "evidence_refs": [
    "ev_050"
  ],

  "verification": {
    "status": "supported",
    "confidence": 0.96
  }
}
```

Allowed operators:

\[
\boxed{
O=\{T,K,I,M,Q\}.
}
\]

---

# 29. Operator T — Transform

Definition:

\[
\boxed{
Y=T(X).
}
\]

Used for deterministic scientific relations.

Examples:

\[
x
\rightarrow
Fourier(x)
\]

\[
u
\rightarrow
u_x
\]

\[
Residual
\rightarrow
ResidualLoss.
\]

Schema:

```json
{
  "operator": "T",
  "input_refs": ["..."],
  "output_refs": ["..."],
  "payload": {
    "expression_ref": "var_expr|null"
  }
}
```

Compiler should validate source/target compatibility where possible.

---

# 30. Operator K — Kernel

Definition:

\[
\boxed{
K(Y|X,C,A)
}
\]

represents conditional, empirical, or stochastic dependence.

Extraction candidate:

```json
{
  "operator": "K",

  "input_refs": [
    "var_fourier_enabled",
    "var_residual_adaptive"
  ],

  "output_refs": [
    "var_result_l2"
  ],

  "payload": {
    "relation_mode": "reported"
  },

  "context_ref": "ctx_001",
  "axiom_set_ref": "ax_001"
}
```

Allowed initial `relation_mode`:

```text
reported
empirical
estimated
conditional
```

The LLM may say the paper reports a relation.

The LLM does not estimate the cross-paper probability kernel.

---

# 31. Operator I — Intervention

Definition:

\[
\boxed{
I:X_0\rightarrow X_1.
}
\]

Most \(I\) operators should be generated by StateDiff.

The LLM may provide an explicit candidate when the source says:

```text
we replace ...
we remove ...
we introduce ...
we increase ...
we modify ...
```

Compiler must still verify against extracted states.

---

# 32. Operator M — Marginalization / aggregation

Definition:

\[
\boxed{
M:
X_{fine}
\rightarrow
X_{coarse}.
}
\]

Examples:

```text
average over seeds
aggregate trials
integrate latent variable
merge subcategories
marginalize nuisance variable
```

Schema:

```json
{
  "operator": "M",

  "input_refs": [
    "result_seed_1",
    "result_seed_2",
    "result_seed_3"
  ],

  "output_refs": [
    "result_mean"
  ],

  "payload": {
    "expression_ref": "expr_mean"
  }
}
```

`mean`, `sum`, `integration`, etc. are represented through payload/Expression.

The primary operator remains `M`.

---

# 33. Operator Q — Quotient / abstraction

Definition:

\[
\boxed{
Q:X\rightarrow X/{\sim}.
}
\]

Example candidate:

\[
\{
Fourier,
SIREN,
Wavelet
\}
\xrightarrow Q
SpectralEnrichment.
\]

LLM schema:

```json
{
  "id": "qcan_001",

  "input_concept_ids": [
    "representation.fourier.enabled",
    "representation.siren.enabled",
    "representation.wavelet.enabled"
  ],

  "proposed_concept_id": "representation.spectral_enrichment.enabled",

  "rationale_evidence_refs": [
    "ev_060"
  ],

  "status": "candidate"
}
```

Allowed LLM status:

```text
candidate
```

The LLM never outputs:

```text
validated
accepted
proven
```

---

# 34. Q compiler validation

Compiler/inference layer evaluates candidate Q using:

\[
Compression(Q)
\]

\[
PredictionLoss(Q)
\]

\[
CommutationError(Q)
\]

\[
ConflictIncrease(Q).
\]

Candidate acceptance criterion may later be:

\[
Compression>\tau_c
\]

\[
PredictionLoss<\epsilon_p
\]

\[
CommutationError<\epsilon_m
\]

\[
ConflictIncrease<\epsilon_c.
\]

MVP may store these measurements before enforcing automatic promotion.

---

# 35. ExtractionV3 complete example

```json
{
  "schema_version": "anyway.extract.v3",

  "document": {
    "document_id": "doc_pinn_001",
    "title": "Example PINN Paper",
    "authors": ["A", "B"],
    "year": 2025,
    "doi": null,
    "arxiv_id": null,
    "url": null,
    "source_type": "paper"
  },

  "evidence": [
    {
      "id": "ev_001",
      "document_id": "doc_pinn_001",
      "location": {
        "page": 5,
        "section": "Method",
        "paragraph": 2,
        "table": null,
        "figure": null,
        "equation": null
      },
      "text_span": "We introduce Fourier features with sigma = 10.",
      "verification": {
        "status": "supported",
        "confidence": 0.98
      }
    }
  ],

  "variables": [
    {
      "id": "var_001",
      "concept_id": "representation.fourier.enabled",
      "value_type": "bool",
      "observed": true,
      "value": true,
      "unit_raw": null,
      "expression_raw": null,
      "evidence_refs": ["ev_001"]
    },

    {
      "id": "var_002",
      "concept_id": "representation.fourier.sigma",
      "value_type": "number",
      "observed": true,
      "value": 10,
      "unit_raw": null,
      "expression_raw": null,
      "evidence_refs": ["ev_001"]
    }
  ],

  "contexts": [],

  "axiom_sets": [],

  "experiments": [],

  "operator_candidates": [],

  "abstraction_candidates": []
}
```

---

# 36. CanvasIRV3 root

Compiler produces:

```json
{
  "schema_version": "anyway.canvas-ir.v3",

  "blocks": [],

  "operators": [],

  "chains": [],

  "fibers": [],

  "bundles": [],

  "identifiability": [],

  "consistency_checks": [],

  "provenance_index": {}
}
```

This representation is computational.

LLM output is evidential.

---

# 37. Block object

```json
{
  "id": "block_001",

  "block_type": "variable",

  "concept_id": "representation.fourier.enabled",

  "variable_ref": "var_001",

  "member_refs": [],

  "context_ref": "ctx_001",

  "axiom_set_ref": "ax_001",

  "semantic_hash": "sha256:...",

  "instance_hash": "sha256:..."
}
```

Allowed `block_type` for MVP:

```text
variable
state
result
concept
axiom
```

These are graph object types.

They are separate from scientific value types.

---

# 38. Block semantics

### variable

A Bool, Number, or Expression.

### state

A complete/sparse experimental configuration.

### result

A target measurement or outcome.

### concept

A higher-level conceptual object.

### axiom

A premise or constraint object.

---

# 39. Semantic hash

Semantic hash ignores source provenance.

For a variable:

\[
h_s=
H(
conceptID,
canonicalValue,
canonicalContext,
axiomSignature
).
\]

Purpose:

```text
cross-paper matching
deduplication
fiber grouping
nearest experiment retrieval
```

Two papers reporting the same configuration may share:

```text
semantic_hash
```

---

# 40. Instance hash

Instance hash includes provenance.

\[
h_i=
H(
semanticHash,
documentID,
evidenceRefs
).
\]

Purpose:

```text
audit
traceability
community sharing
replication counting
source independence
```

Therefore:

```text
same experiment semantics
=> same semantic_hash possible

different papers
=> different instance_hash
```

---

# 41. Compiled Operator object

```json
{
  "id": "op_001",

  "operator": "I",

  "input_refs": [
    "block_state_A"
  ],

  "output_refs": [
    "block_state_B"
  ],

  "payload": {},

  "context_ref": "ctx_001",

  "axiom_set_ref": "ax_001",

  "evidence_refs": [
    "ev_001",
    "ev_002"
  ],

  "semantic_hash": "sha256:...",

  "instance_hash": "sha256:..."
}
```

---

# 42. Operator type invariant

Every compiled operator must have a valid domain and codomain.

Formally:

\[
o:X\rightarrow Y.
\]

Compiler rejects operators whose referenced inputs or outputs cannot be resolved.

Validation:

```text
OP-001 operator ∈ {T,K,I,M,Q}

OP-002 input_refs.length >= 1

OP-003 output_refs.length >= 1

OP-004 every ref resolves

OP-005 context_ref resolves when required

OP-006 axiom_set_ref resolves when required
```

---

# 43. Chain object

```json
{
  "id": "chain_001",

  "block_path": [
    "block_A",
    "block_B",
    "block_C"
  ],

  "operator_path": [
    "op_001",
    "op_002"
  ],

  "context_ref": "ctx_001",

  "axiom_set_ref": "ax_001",

  "source_experiment_refs": [
    "exp_001"
  ],

  "semantic_hash": "sha256:...",

  "instance_hash": "sha256:..."
}
```

A Chain is an ordered computational path:

\[
B_0
\xrightarrow{o_1}
B_1
\xrightarrow{o_2}
\dots
\xrightarrow{o_n}
B_n.
\]

---

# 44. Chain invariants

```text
CHAIN-001
block_path.length = operator_path.length + 1

CHAIN-002
operator[i].input includes block_path[i]

CHAIN-003
operator[i].output includes block_path[i+1]

CHAIN-004
all refs resolve

CHAIN-005
chain context is compatible with member operator contexts

CHAIN-006
chain axiom set is compatible with member operators
```

---

# 45. Chain hash

Semantic chain hash:

\[
h_\gamma^s=
H(
h_{B_0}^s,
h_{o_1}^s,
h_{B_1}^s,
\dots,
h_{B_n}^s
).
\]

Instance chain hash:

\[
h_\gamma^i=
H(
h_\gamma^s,
sourceExperiments,
evidence
).
\]

---

# 46. Fiber object

```json
{
  "id": "fiber_001",

  "conditioning": [
    {
      "concept_id": "physics.helmholtz.enabled",
      "semantic_value_hash": "..."
    },

    {
      "concept_id": "optimizer.adam.enabled",
      "semantic_value_hash": "..."
    }
  ],

  "varying_concepts": [
    "representation.fourier.enabled",
    "representation.fourier.sigma"
  ],

  "chain_refs": [
    "chain_001",
    "chain_002",
    "chain_003"
  ],

  "semantic_hash": "sha256:..."
}
```

Definition:

\[
\boxed{
F_x=
\{\gamma:
conditioning(\gamma)=x
\}.
}
\]

Fiber contains Chains.

Fiber does not contain bare Variables as primary members.

---

# 47. Fiber conditioning

Example:

```text
PDE = Helmholtz
Residual = strong-form
Optimizer = Adam
Architecture = fixed
```

These variables form:

\[
x.
\]

Fourier changes across experiments.

Therefore:

```text
conditioning:
    PDE
    Residual
    Optimizer
    Architecture

varying:
    Fourier.enabled
    Fourier.sigma
```

This creates a research fiber for Fourier effects.

---

# 48. Fiber membership

Two chains may join the same fiber when:

1. target outcome is compatible;
2. conditioning concepts match after canonicalization;
3. axiom sets are compatible;
4. differences are contained within allowed varying dimensions.

Fiber grouping is therefore query-dependent.

A chain can belong to multiple fibers under different analytical projections.

Example:

The same experiment may participate in:

```text
fiber: Fourier variation
fiber: residual variation
fiber: optimizer-controlled studies
```

Fiber membership should be represented many-to-many.

---

# 49. Bundle object

```json
{
  "id": "bundle_001",

  "target_concepts": [
    "result.relative_l2_error"
  ],

  "fiber_refs": [
    "fiber_001",
    "fiber_002",
    "fiber_003"
  ],

  "varying_dimensions": [
    "representation.fourier.sigma"
  ],

  "semantic_hash": "sha256:..."
}
```

Bundle groups fibers across context variation.

Conceptually:

\[
\boxed{
\mathcal B
=
\bigsqcup_xF_x.
}
\]

---

# 50. Identifiability object

Compiler-generated:

```json
{
  "id": "idn_001",

  "target_ref": "block_result_l2",

  "intervention_ref": "op_I_001",

  "joint_effect": {
    "status": "identifiable"
  },

  "component_effects": [
    {
      "concept_id": "residual.adaptive.enabled",
      "status": "unresolved"
    },

    {
      "concept_id": "representation.fourier.enabled",
      "status": "unresolved"
    }
  ],

  "interactions": [
    {
      "concept_refs": [
        "residual.adaptive.enabled",
        "representation.fourier.enabled"
      ],
      "status": "unresolved"
    }
  ],

  "missing_controls": [
    {
      "configuration": {
        "residual.adaptive.enabled": true,
        "representation.fourier.enabled": false
      }
    }
  ]
}
```

Allowed effect status:

```text
identifiable
partially_identifiable
unresolved
confounded
insufficient_evidence
```

The compiler owns this status.

---

# 51. Two-variable identifiability

For:

\[
R,F\in\{0,1\}
\]

desired experiment matrix:

\[
Y_{00},
Y_{10},
Y_{01},
Y_{11}.
\]

Main residual effect:

\[
\Delta_R
=
Y_{10}-Y_{00}.
\]

Main Fourier effect:

\[
\Delta_F
=
Y_{01}-Y_{00}.
\]

Interaction:

\[
\boxed{
\Delta_{RF}
=
Y_{11}-Y_{10}-Y_{01}+Y_{00}.
}
\]

With only:

\[
Y_{00},Y_{11},
\]

only the joint change is observed.

The system must report:

```text
joint effect: identifiable
R effect: unresolved
F effect: unresolved
R×F interaction: unresolved
```

---

# 52. Historical neighbor matcher

For new state:

\[
X.
\]

Compiler searches historical graph for sparse neighboring states.

Boolean distance:

\[
d_B(X,Y)
=
\sum_i
1[x_i\neq y_i].
\]

Numerical distance:

\[
d_N(X,Y)
=
\sum_j
w_j
|\tilde x_j-\tilde y_j|.
\]

Expression matching priority:

```text
1 exact canonical expression
2 AST structural equivalence
3 symbolic equivalence
4 semantic candidate similarity
```

Semantic similarity should only generate candidates.

---

# 53. Missing control generation

For a joint intervention:

\[
(R_0,F_0)
\rightarrow
(R_1,F_1),
\]

compiler should search:

\[
(R_1,F_0)
\]

and:

\[
(R_0,F_1).
\]

For three variables:

\[
R,F,W,
\]

compiler may search the factorial neighborhood:

```text
R1 F0 W0
R0 F1 W0
R0 F0 W1
R1 F1 W0
R1 F0 W1
R0 F1 W1
```

The missing states become explicit graph requirements.

---

# 54. ConsistencyCheck object

```json
{
  "id": "check_001",

  "check_type": "path",

  "input_refs": [
    "chain_001",
    "chain_002"
  ],

  "metric": "relative_difference",

  "value": 0.002,

  "threshold": 0.01,

  "status": "pass",

  "details": {}
}
```

Allowed:

```text
path
representation
branch
abstraction
conflict
```

---

# 55. Path consistency

Given:

\[
\gamma_1:A\rightarrow C
\]

and:

\[
\gamma_2:A\rightarrow C,
\]

compute:

\[
E_{path}
=
d(
R_{\gamma_1},
R_{\gamma_2}
).
\]

For numerical differentiable paths:

\[
E_{path}
=
\left|
\frac{\partial C}{\partial A}\bigg|_{\gamma_1}
-
\frac{\partial C}{\partial A}\bigg|_{\gamma_2}
\right|.
\]

For kernels:

\[
E_{path}
=
D(
K_{\gamma_1},
K_{\gamma_2}
).
\]

---

# 56. Representation consistency

For equivalent representation transformation:

\[
g_X:X\rightarrow X'
\]

and:

\[
g_Y:Y\rightarrow Y',
\]

check:

\[
\boxed{
f\circ g_X
\simeq
g_Y\circ f.
}
\]

Examples:

```text
unit conversion
coordinate transformation
equivalent mathematical expression
canonical variable rename
```

---

# 57. Branch consistency

Original:

\[
A\rightarrow B\rightarrow C.
\]

Refined:

\[
A
\rightarrow
\{b_1,b_2,b_3\}
\rightarrow
C.
\]

Compute:

\[
K_{fine}(C|A)
=
\sum_i
K(C|A,b_i)
K(b_i|A).
\]

Compare:

\[
K_{coarse}(C|A).
\]

Define:

\[
\boxed{
E_{branch}
=
D(
K_{fine},
K_{coarse}
).
}
\]

---

# 58. Abstraction consistency

For:

\[
Q_X:X\rightarrow\bar X
\]

\[
Q_Y:Y\rightarrow\bar Y,
\]

and lower-level relation:

\[
f:X\rightarrow Y,
\]

require:

\[
\boxed{
Q_Y\circ f
\simeq
\bar f\circ Q_X.
}
\]

Measure:

\[
E_Q
=
d(
Q_Yf,
\bar fQ_X
).
\]

---

# 59. Conflict consistency

Conflicting evidence is partitioned by:

\[
(Context,AxiomSet).
\]

Compiler outputs one of:

```text
contextual_divergence
axiomatic_divergence
internal_conflict
insufficient_resolution
```

The system should preserve opposing evidence sets separately.

---

# 60. Conflict schema

```json
{
  "id": "check_conflict_001",

  "check_type": "conflict",

  "input_refs": [
    "chain_positive",
    "chain_negative"
  ],

  "status": "flag",

  "details": {
    "classification": "contextual_divergence",
    "context_difference_refs": [
      "var_temperature"
    ],
    "axiom_difference_refs": []
  }
}
```

---

# 61. Contradiction handling rule

Evaluation order:

\[
\boxed{
Conflict
\rightarrow
ContextComparison
\rightarrow
AxiomComparison
\rightarrow
InternalConflict.
}
\]

Never perform:

\[
0.99\ support
+
0.99\ contradiction
\rightarrow
0.5.
\]

Conflicting high-certainty systems may encode different contexts or axioms.

---

# 62. Provenance index

CanvasIR keeps reverse lookup:

```json
{
  "provenance_index": {
    "ev_001": [
      "block_001",
      "op_001",
      "chain_001"
    ]
  }
}
```

This supports:

```text
click graph node → source
source → all graph objects derived from it
retraction
recompile
audit
```

---

# 63. Retraction behavior

When evidence becomes invalid or a paper is withdrawn:

1. mark Evidence verification state;
2. identify derived instances through provenance index;
3. remove/recompute affected operator instances;
4. recompute Chains;
5. recompute Fibers;
6. recompute effects and consistency checks.

Semantic graph objects should be recomputed from remaining evidence.

Do not mutate historical provenance silently.

---

# 64. Compiler pipeline

Reference implementation sequence:

```text
ExtractionV3
      │
      ▼
Schema Validator
      │
      ▼
Reference Resolver
      │
      ▼
Evidence Gate
      │
      ▼
Concept Canonicalizer
      │
      ▼
Unit Canonicalizer
      │
      ▼
Expression Parser
      │
      ▼
State Builder
      │
      ▼
State Diff
      │
      ▼
Joint Intervention Builder
      │
      ▼
Block Compiler
      │
      ▼
Operator Compiler
      │
      ▼
Hash Builder
      │
      ▼
Historical Matcher
      │
      ▼
Identifiability Engine
      │
      ▼
Chain Builder
      │
      ▼
Fiber Grouper
      │
      ▼
Bundle Builder
      │
      ▼
Consistency Engine
      │
      ▼
CanvasIRV3
```

---

# 65. LLM extraction pass 1

Pass 1 extracts candidate facts.

Input:

```text
paper sections
tables
captions
equations
metadata
```

Output:

```text
ExtractionV3 candidates
```

The extractor should favor recall while retaining evidence.

---

# 66. LLM verification pass 2

Input:

```text
candidate extraction
+
exact evidence span
+
local surrounding context
```

Output:

```text
supported
ambiguous
unsupported
```

The verifier should specifically check:

```text
Is the value explicitly present?
Does the evidence refer to the baseline or proposed model?
Does "without" indicate explicit false?
Is this configuration reported or inferred?
Does the reported result correspond to this state?
```

Only verified entries enter the default compiler path.

---

# 67. LLM prohibited behavior

The extractor must never:

```text
invent missing hyperparameters
map absence of mention to false
split a joint intervention into independent effects
invent probability values
declare causal identifiability
validate an abstraction
resolve scientific contradictions
construct a fiber directly
construct a bundle directly
declare two expressions mathematically equivalent without verification
replace provenance text with a summary
```

---

# 68. Compiler prohibited behavior

The compiler must never:

```text
overwrite raw source values
discard evidence links
convert unknown to false
create independent effect from joint intervention without controls
merge incompatible axiom branches
merge incompatible contexts silently
accept Q solely from semantic similarity
treat graph direction alone as causal proof
infer Markov independence solely from visual graph structure
```

---

# 69. PINN canonical concept seed

MVP ontology seed:

```text
problem.*

physics.*
physics.pde.*
physics.parameter.*

residual.*
residual.strong_form.enabled
residual.weak_form.enabled
residual.adaptive.enabled
residual.expression
residual.sample_count

representation.*
representation.fourier.enabled
representation.fourier.sigma
representation.fourier.dimension
representation.siren.enabled
representation.wavelet.enabled

architecture.*
architecture.depth
architecture.width

sampling.*
sampling.random.enabled
sampling.lhs.enabled
sampling.adaptive.enabled
sampling.sample_count

loss.*
loss.dynamic.enabled
loss.residual_weight
loss.boundary_weight
loss.initial_weight
loss.weight_expression

optimizer.*
optimizer.adam.enabled
optimizer.lbfgs.enabled
optimizer.learning_rate

training.*
training.epochs
training.batch_size

result.*
result.l2_error
result.relative_l2_error
result.residual_error
result.training_time
```

This seed is extensible.

New concepts do not require new primitive types.

---

# 70. Concept creation policy

When the extractor encounters an unknown concept:

```text
Neural Tangent Kernel adaptive weighting
```

it may propose:

```text
loss.ntk_weighting.enabled
```

Canonicalizer decides whether to:

```text
reuse existing concept
create new child concept
map to alias
flag unresolved
```

Concept extension should remain separate from value type extension.

---

# 71. Canonicalization record

Recommended internal structure:

```json
{
  "raw_concept": "random Fourier feature encoding",
  "canonical_concept_id": "representation.fourier.enabled",
  "mapping_type": "alias",
  "confidence": 0.97
}
```

Raw phrase must remain recoverable.

---

# 72. Number normalization

Compiler stores:

```json
{
  "value_raw": 80,
  "unit_raw": "MPa",

  "value_canonical": 80000000,
  "unit_canonical": "Pa"
}
```

Scientific comparisons use canonical values.

Evidence display uses raw values.

Hash policy should explicitly choose canonical representation.

---

# 73. Expression normalization

Expression processing stages:

```text
raw source expression
→ token parse
→ AST candidate
→ variable canonicalization
→ normalized AST
→ symbolic equivalence candidate
```

Example:

\[
u_t+u u_x-\nu u_{xx}
\]

and:

\[
\partial_tu+
u\partial_xu-
\nu\partial_{xx}u
\]

may become equivalent normalized ASTs.

Semantic equivalence must remain explicitly versioned.

---

# 74. Expression hash

Recommended:

```text
raw_expression_hash
normalized_expression_hash
```

This mirrors semantic/instance identity.

Equivalent formatting should share normalized hash where parser confidence is sufficient.

---

# 75. Markov assumptions

Conditional independence must be explicit.

Optional compiled metadata:

```json
{
  "conditional_independence": {
    "target": "Y",
    "independent_of": ["X"],
    "given": ["Z", "C"],
    "evidence_refs": []
  }
}
```

Meaning:

\[
Y\perp X\mid Z,C.
\]

The compiler should not derive this from DAG shape alone.

---

# 76. Kernel estimation

For experiments matching:

\[
X,C,A,
\]

empirical kernel:

\[
\hat\mu_{X,C,A}
=
\frac{
\sum_iw_i\delta_{Y_i}
}{
\sum_iw_i
}.
\]

Initial MVP may use:

\[
w_i=1.
\]

Later weight dimensions:

```text
replication
measurement uncertainty
sample size
source independence
benchmark compatibility
evaluation comparability
```

Weights must remain decomposable.

Avoid a single opaque global credibility score.

---

# 77. Bayesian inference

Bayesian inference belongs downstream of extraction.

For hypothesis \(H\):

\[
P(H|E)
\propto
P(E|H)P(H).
\]

Graph evidence must preserve enough structure to construct likelihoods.

LLM should never directly create posterior probabilities unless the source explicitly reports them as source data.

---

# 78. Local effect calculation

When sufficient neighboring configurations exist:

\[
Y=f(X).
\]

Local approximation:

\[
\Delta Y
\approx
\nabla f^\top\Delta X
+
\frac12
\Delta X^\top H_f\Delta X.
\]

Bool variables:

\[
\frac{\partial Y}{\partial b}
\]

is implemented using finite difference.

Number variables may use continuous derivative estimation.

Interaction:

\[
\frac{\partial^2Y}
{\partial X_i\partial X_j}
\]

is estimated only when coverage permits.

---

# 79. Normalization policy

Three data layers must coexist.

### raw

Exactly source-reported data.

### canonical

Normalized units, concepts, expressions.

### comparison

Derived representation for graph computation.

Never overwrite:

```text
raw
```

with:

```text
canonical
```

or:

```text
comparison
```

---

# 80. Fiber distribution

Given chains:

\[
\gamma_1,\ldots,\gamma_N
\in F_x,
\]

the empirical fiber measure can be represented as:

\[
\mu_x^{(N)}
=
\frac{
\sum_iw_i\delta_{\gamma_i}
}{
\sum_iw_i
}.
\]

The MVP storage only needs to preserve membership and weights.

Full limit computation can remain downstream.

---

# 81. Fiber convergence target

Long-term invariant:

\[
\boxed{
\mu_x^{(N)}
\Rightarrow
\mu_x.
}
\]

Schema v3 must retain enough identity/context information to calculate this later.

No explicit infinite-limit representation is required by MVP.

---

# 82. Bundle transport

Potential post-MVP structure:

\[
T_{x\rightarrow x'}:
F_x\rightarrow F_{x'}.
\]

Schema v3 Bundle should retain:

```text
fiber identities
varying dimensions
target concepts
```

so future transport estimation does not require data migration.

---

# 83. Abstraction hierarchy

Potential hierarchy:

\[
RawEvidence
\rightarrow
Variables
\rightarrow
Interventions
\rightarrow
Chains
\rightarrow
Fibers
\rightarrow
Bundles
\rightarrow
Mechanisms
\rightarrow
Theories.
\]

Schema v3 directly supports up to:

```text
Bundle
+
Q candidate
```

Higher-level Sections/Theories may be added later.

---

# 84. API validation behavior

Recommended compile endpoint:

```text
POST /compiler/v3/compile
```

Input:

```text
ExtractionV3
```

Output on success:

```json
{
  "ok": true,
  "ir": {},
  "warnings": []
}
```

On validation failure:

```json
{
  "ok": false,
  "errors": [
    {
      "code": "VAR-004",
      "path": "$.variables[3]",
      "message": "Observed variable requires evidence reference."
    }
  ]
}
```

---

# 85. Error code families

Recommended:

```text
DOC-*   document
EV-*    evidence
VAR-*   variable
CTX-*   context
AX-*    axiom
EXP-*   experiment
STATE-* state
OP-*    operator
CHAIN-* chain
FIB-*   fiber
BUN-*   bundle
IDN-*   identifiability
Q-*     abstraction
HASH-*  hashing
REF-*   references
```

---

# 86. Critical validation errors

Examples:

```text
VAR-001 invalid primitive type
VAR-004 observed variable missing evidence

REF-001 unresolved reference

EXP-002 state has duplicate conflicting concept values

OP-001 invalid operator class

CHAIN-001 invalid block/operator path length

FIB-001 fiber member is not a chain

IDN-001 component effect marked identifiable without required controls

Q-001 LLM abstraction marked validated

HASH-001 noncanonical serialization
```

---

# 87. Warnings

Warnings should not necessarily stop compilation.

Examples:

```text
AMBIGUOUS_UNIT
UNKNOWN_BASELINE_VALUE
INCOMPLETE_CONTEXT
EXPRESSION_PARSE_FAILED
CONTROL_CONFIGURATION_MISSING
AXIOM_SET_PARTIAL
POSSIBLE_DUPLICATE_EXPERIMENT
SEMANTIC_MAPPING_LOW_CONFIDENCE
```

---

# 88. Compiler deterministic requirements

Given identical:

```text
ExtractionV3
+
ontology version
+
compiler version
```

Compiler should produce identical:

```text
canonical values
semantic hashes
intervention sets
IR topology
```

This is required for community reproducibility.

---

# 89. Version metadata

Recommended CanvasIR root metadata:

```json
{
  "schema_version": "anyway.canvas-ir.v3",

  "compiler": {
    "version": "0.3.0",
    "ontology_version": "pinn-0.1.0",
    "expression_parser_version": "0.1.0",
    "hash_algorithm": "sha256"
  }
}
```

---

# 90. Schema migration principle

Within v3:

```text
new optional fields allowed
new concept IDs allowed
new warning codes allowed
```

Breaking changes require v4 when they alter:

```text
primitive scientific types
operator basis
root contract
reference semantics
hash semantics
unknown semantics
joint intervention semantics
```

---

# 91. Test fixture 1 — explicit Bool

Source:

```text
We do not use Fourier feature encoding.
```

Expected:

```json
{
  "concept_id": "representation.fourier.enabled",
  "value_type": "bool",
  "observed": true,
  "value": false
}
```

---

# 92. Test fixture 2 — unknown Bool

Source contains no information about Fourier features.

Expected:

```text
no inferred false
```

Either omit the variable or produce:

```json
{
  "concept_id": "representation.fourier.enabled",
  "value_type": "bool",
  "observed": false,
  "value": null
}
```

according to extraction mode.

---

# 93. Test fixture 3 — Number

Source:

```text
The Fourier bandwidth sigma is fixed at 10.
```

Expected:

```text
representation.fourier.sigma
number
10
observed=true
```

---

# 94. Test fixture 4 — Expression

Source:

\[
r=u_t+uu_x-\nu u_{xx}.
\]

Expected:

```text
residual.expression
expression
raw formula preserved
```

---

# 95. Test fixture 5 — joint intervention

Baseline:

```text
strong residual
no Fourier
static weighting
```

Proposed:

```text
adaptive residual
Fourier sigma=10
dynamic weighting
```

Expected compiled operator:

\[
I=
\{
\Delta R,
\Delta F,
\Delta W
\}.
\]

Expected identifiability:

```text
joint effect identifiable
R unresolved
F unresolved
W unresolved
interactions unresolved
```

---

# 96. Test fixture 6 — missing control

Historical database contains:

```text
R0 F0
R1 F1
R1 F0
```

Expected:

```text
R main effect may be estimated under compatible context
F effect remains unresolved without R0 F1
R×F interaction unresolved
```

---

# 97. Test fixture 7 — branch refinement

Original:

\[
A\rightarrow B\rightarrow C.
\]

New ontology splits:

\[
B\rightarrow b_1,b_2,b_3.
\]

Expected compiler behavior:

```text
preserve old coarse node
create M relation from refined representation
schedule branch consistency check
```

---

# 98. Test fixture 8 — contradiction with context change

Paper A:

\[
C
\]

under:

```text
temperature = 300 K
```

Paper B:

\[
\neg C
\]

under:

```text
temperature = 500 K
```

Expected:

```text
contextual_divergence
```

No internal contradiction.

---

# 99. Test fixture 9 — axiom divergence

Two theories use different governing assumptions.

Expected:

```text
axiomatic_divergence
```

Both Chains remain valid inside their respective AxiomSets.

---

# 100. Test fixture 10 — Q candidate

Papers describe:

```text
Fourier Features
SIREN
Wavelet Encoding
```

LLM proposes:

```text
Spectral Enrichment
```

Expected:

```text
Q candidate stored
status=candidate
```

Compiler does not promote automatically until validation criteria are satisfied.

---

# 101. Minimal JSON-like Extraction schema

For implementation reference:

```json
{
  "schema_version": "anyway.extract.v3",

  "document": {
    "document_id": "string",
    "source_type": "string"
  },

  "evidence": [
    {
      "id": "string",
      "document_id": "string",
      "location": {},
      "text_span": "string",
      "verification": {
        "status": "supported|ambiguous|unsupported",
        "confidence": 0.0
      }
    }
  ],

  "variables": [
    {
      "id": "string",
      "concept_id": "string",
      "value_type": "bool|number|expression",
      "observed": true,
      "value": null,
      "unit_raw": null,
      "expression_raw": null,
      "evidence_refs": []
    }
  ],

  "contexts": [],

  "axiom_sets": [],

  "experiments": [],

  "operator_candidates": [],

  "abstraction_candidates": []
}
```

---

# 102. Minimal CanvasIR schema

```json
{
  "schema_version": "anyway.canvas-ir.v3",

  "blocks": [],

  "operators": [],

  "chains": [],

  "fibers": [],

  "bundles": [],

  "identifiability": [],

  "consistency_checks": [],

  "provenance_index": {}
}
```

---

# 103. Core formal model

Scientific values:

\[
\boxed{
V=
Bool\cup Number\cup Expression
}
\]

Operators:

\[
\boxed{
O=
\{T,K,I,M,Q\}.
}
\]

Finite chain:

\[
\boxed{
\gamma=
o_n\circ\cdots\circ o_1,
\qquad
o_i\in O.
}
\]

Fiber:

\[
\boxed{
F_x=
\{\gamma:
conditioning(\gamma)=x
\}.
}
\]

Bundle:

\[
\boxed{
\mathcal B=
\bigsqcup_xF_x.
}
\]

---

# 104. Five graph consistency requirements

### Composition

\[
g\circ f
\]

must remain valid when domains match.

### Representation invariance

\[
f\circ g_X
\simeq
g_Y\circ f.
\]

### Path coherence

\[
R_{\gamma_1}
\simeq
R_{\gamma_2}.
\]

### Projective consistency

\[
M_*\mu_{fine}
\simeq
\mu_{coarse}.
\]

### Abstraction naturality

\[
Q_Yf
\simeq
\bar fQ_X.
\]

These checks belong to CanvasIR/runtime, not LLM extraction.

---

# 105. Final ownership boundary

## LLM owns

```text
source interpretation
evidence extraction
Bool extraction
Number extraction
Expression extraction
context candidates
axiom candidates
state identification
baseline/proposed identification
result extraction
explicit relation candidates
Q proposals
```

## Compiler owns

```text
schema validity
references
canonical concepts
canonical units
expression normalization
state diff
joint interventions
Blocks
Operators
Chains
Fibers
Bundles
hashing
historical graph matching
identifiability
effect decomposition
interaction calculation
path consistency
branch consistency
conflict classification
Q validation
```

---

# 106. MVP hard invariants

The implementation must preserve the following invariants:

```text
V3-01
Scientific primitive type ∈ {bool, number, expression}.

V3-02
false and unknown are distinct.

V3-03
Every observed fact has provenance.

V3-04
Every compiled operator belongs to {T,K,I,M,Q}.

V3-05
Every compiled operator has valid typed input/output references.

V3-06
A multi-variable state diff generates one joint intervention.

V3-07
Joint intervention evidence does not automatically generate independent effects.

V3-08
LLM-generated Q always begins as candidate.

V3-09
Chain length satisfies:
blocks = operators + 1.

V3-10
A Fiber is a collection of Chains under shared conditioning.

V3-11
A semantic scientific object may have many provenance instances.

V3-12
Context participates in scientific relation identity.

V3-13
AxiomSet participates in scientific relation identity.

V3-14
Raw evidence is immutable.

V3-15
Canonicalization never destroys raw extraction.

V3-16
Normalization never overwrites raw values.

V3-17
Identifiability is computed by the graph engine.

V3-18
Contradiction classification occurs after context and axiom comparison.

V3-19
Graph topology alone does not imply causality.

V3-20
Schema/compiler output must be deterministic under fixed versions.
```

---

# 107. MVP acceptance target

A successful implementation should accept a PINN paper stating:

> We replace the standard strong residual with an adaptive residual, add Fourier features with \(\sigma=10\), dynamically reweight the residual and boundary losses, and reduce relative L2 error from 0.11 to 0.021.

And automatically produce:

```text
Evidence
    ↓
Variables
    ↓
Baseline State
    ↓
Proposed State
    ↓
Joint Intervention
    {
      Residual change,
      Fourier change,
      Dynamic loss change
    }
    ↓
Outcome
    relative_l2_error:
    0.11 → 0.021
    ↓
Historical Neighbor Search
    ↓
Identifiability
    ↓
Chains
    ↓
Fibers
    ↓
Cross-paper graph
```

The directly justified relationship is:

\[
\boxed{
P(
Y
|
do(R_1,F_1,W_1),
C,A
)
}
\]

Independent relations:

\[
R\rightarrow Y,
\qquad
F\rightarrow Y,
\qquad
W\rightarrow Y
\]

are promoted only when historical experiment coverage makes those effects identifiable.

---

# 108. Implementation priority

Recommended coding order:

```text
1. ExtractionV3 data models
2. JSON Schema validator
3. evidence/reference validation
4. concept canonicalization
5. State model
6. StateDiff
7. joint Intervention compiler
8. Canvas Block/Operator IR
9. semantic/instance hashing
10. historical experiment matcher
11. identifiability engine
12. Chain builder
13. Fiber grouping
14. Bundle grouping
15. consistency checks
16. Q candidate validation
```

The first usable MVP milestone is reached after step 11.

At that point Anyway can already transform scientific papers into structured multi-variable experimental evidence and determine which cross-paper effects are computable.

---

# 109. Architectural contract

The complete v3 handoff can be reduced to two equations:

\[
\boxed{
ExtractionV3
=
Evidence
+
Variables
+
Context
+
Axioms
+
ExperimentalStates
+
OperatorCandidates
}
\]

and:

\[
\boxed{
CanvasIRV3
=
Blocks
+
Operators
+
Chains
+
Fibers
+
Bundles
+
Identifiability
+
Consistency.
}
\]

With:

\[
\boxed{
Variables=
Bool\cup Number\cup Expression
}
\]

and:

\[
\boxed{
Operators=
\{T,K,I,M,Q\}.
}
\]

The implementation should keep these two contracts stable through the entire MVP.