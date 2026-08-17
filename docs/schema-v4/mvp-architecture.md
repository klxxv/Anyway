# Anyway MVP Architecture
## Scientific Extraction & Internal Logic Graph Computation

**Status:** MVP handoff architecture  
**Primary goal:** automatically transform scientific papers and historical public research records into a typed, provenance-preserving computational graph that supports cross-paper comparison, multi-variable intervention analysis, conditional probability updates, consistency checks, and later abstraction discovery.

---

# 1. System objective

Anyway is a scientific computation layer over research literature.

The input is unstructured scientific evidence:

\[
\text{papers}+\text{repositories}+\text{datasets}+\text{experiments}
\]

The output is a computable research graph:

\[
\boxed{
\text{Evidence}
\rightarrow
\text{Variables}
\rightarrow
\text{Operators}
\rightarrow
\text{Chains}
\rightarrow
\text{Fibers}
\rightarrow
\text{Bundles}
}
\]

The MVP should answer questions such as:

- Which experimental variables changed between two PINN papers?
- Did a paper modify residual formulation, Fourier representation, loss weighting, sampling, or several simultaneously?
- Which result can currently be attributed to a single variable?
- Which result is only identifiable as a joint intervention?
- Which historical experiments provide missing controls?
- Do two different computation paths give a consistent effect on the same result?
- Does refining an intermediate concept preserve the relation between upstream and downstream variables?
- Are conflicting conclusions caused by context differences, axiom differences, or genuine internal inconsistency?
- Can several concrete techniques eventually be abstracted into one higher-level mechanism?

The MVP focuses on **extraction + canonicalization + graph construction + finite graph computation**.

Large-scale automatic theory discovery is a later layer.

---

# 2. Core design principle

Anyway should not reduce a paper to:

\[
Paper\rightarrow Claim\rightarrow confidence.
\]

The computational object is:

\[
\boxed{
Paper
\rightarrow
Configuration
\rightarrow
Intervention
\rightarrow
Mechanism
\rightarrow
Outcome
}
\]

A PINN paper, for example, should be represented through variables such as:

\[
Residual,
Fourier,
LossWeighting,
Sampling,
Architecture,
Optimizer,
PDE,
Result.
\]

A paper modifying both residual formulation and Fourier features should produce:

\[
do(R=R_1,F=F_1)
\]

rather than two independently asserted causal edges.

This distinction is fundamental.

---

# 3. Mathematical object

The core object is a typed computational category:

\[
\boxed{
\mathsf{Anyway}
=
\langle T,K,I,M,Q\rangle/\mathcal R
}
\]

where:

\[
\Sigma=\{T,K,I,M,Q\}
\]

is the finite operator basis, and \(\mathcal R\) contains the consistency axioms.

The five operator classes are:

\[
\boxed{
T=\text{Transform}
}
\]

\[
\boxed{
K=\text{Conditional / stochastic kernel}
}
\]

\[
\boxed{
I=\text{Intervention}
}
\]

\[
\boxed{
M=\text{Marginalization / aggregation}
}
\]

\[
\boxed{
Q=\text{Quotient / abstraction}
}
\]

Axiom information and evidence provenance remain separate layers:

\[
A=\text{constraint / axiom layer}
\]

\[
E=\text{evidence / provenance layer}.
\]

Therefore Axiom and Evidence are not primary computational operators.

---

# 4. Fundamental graph objects

## 4.1 Block

A Block is the smallest addressable scientific object.

Examples:

```text
Fourier.enabled
Fourier.sigma
Residual.form
Residual.expression
Loss.dynamic
L2_error
PDE.type
```

Formally:

\[
B=(id,concept,value,context,provenance).
\]

Blocks can represent variables, expressions, results, experimental conditions, or higher-level concepts.

---

# 4.2 Chain

A Chain is a finite composition of typed relations:

\[
\gamma:
B_0
\xrightarrow{o_1}
B_1
\xrightarrow{o_2}
\cdots
\xrightarrow{o_n}
B_n
\]

with

\[
o_i\in\{T,K,I,M,Q\}.
\]

Its computation is:

\[
\boxed{
R_\gamma
=
o_n\circ\cdots\circ o_1.
}
\]

A Chain must preserve provenance.

Example:

\[
Fourier
\xrightarrow{I}
Representation
\xrightarrow{T}
HighFrequencyCapacity
\xrightarrow{K}
Error.
\]

---

# 4.3 Fiber

A Fiber is a set of Chains under the same relevant context.

Let:

\[
\pi:\Gamma\rightarrow\mathcal X
\]

map a chain to its context.

Then:

\[
\boxed{
F_x
=
\pi^{-1}(x)
=
\{\gamma:\pi(\gamma)=x\}.
}
\]

Example:

```text
PDE = Helmholtz
Residual = strong_form
Optimizer = Adam
Architecture = 8×128
```

while Fourier configuration varies.

All compatible experiments under this fixed context belong to the same fiber.

The empirical distribution over chains is:

\[
\mu_x^{(N)}
=
\sum_iw_i\delta_{\gamma_i}.
\]

As research evidence increases, the desired limit is:

\[
\boxed{
\mu_x^{(N)}
\Rightarrow
\mu_x.
}
\]

---

# 4.4 Bundle

The complete research object is the collection of fibers across contexts:

\[
\boxed{
\mathcal B
=
\bigsqcup_{x\in\mathcal X}F_x.
}
\]

Changes in experimental configuration move computation between fibers.

For example:

\[
F_{\sigma=1}
\rightarrow
F_{\sigma=10}.
\]

---

# 4.5 Section

A Section represents a coherent theory or explanation across contexts:

\[
s:\mathcal X\rightarrow\mathcal B
\]

such that

\[
\pi\circ s=id.
\]

Sections are primarily post-MVP functionality, but the data model should permit them.

---

# 5. Variable type system

The MVP uses only three primitive scientific value types:

\[
\boxed{
V=
Bool
\;|\;
Number
\;|\;
Expression
}
\]

This is deliberately minimal.

---

## 5.1 Bool

Bool expresses whether a mechanism, component, method, feature, or constraint is present.

Examples:

```text
Residual.enabled = true
Fourier.enabled = true
DynamicLoss.enabled = false
RMSNorm.enabled = true
LayerNorm.enabled = false
RoPE.enabled = true
```

Categorical systems should be decomposed into Boolean concepts.

For example:

```text
RMSNorm = true
LayerNorm = false
```

instead of:

```text
NormType = "RMSNorm"
```

This representation directly supports interventions.

---

# 5.2 Number

Number stores scalar scientific parameters.

Examples:

```text
Fourier.sigma = 10
Fourier.dimension = 128
LearningRate = 0.001
RoPE.base = 500000
Residual.sample_count = 10000
```

Raw units must remain attached as metadata:

```text
value_raw
unit_raw
value_canonical
unit_canonical
```

The mathematical primitive remains Number.

---

# 5.3 Expression

Expression represents relationships that cannot be reduced to a scalar or Boolean configuration.

Examples:

\[
r=u_t+uu_x-\nu u_{xx}
\]

\[
\lambda_r(t)
=
f(
\|\nabla_\theta L_r\|,
\|\nabla_\theta L_b\|
)
\]

\[
\phi(x)
=
[\sin(Bx),\cos(Bx)].
\]

Expressions should eventually use an AST representation.

Example:

```text
Add
├── Dt(u)
├── Mul(u, Dx(u))
└── Mul(-nu, Dxx(u))
```

The raw LaTeX/string representation should also be retained.

---

# 5.4 Unknown state

Unknown is an observation state, not a fourth scientific value type.

Every variable therefore has:

\[
observed\in\{0,1\}.
\]

Example:

```text
RoPE.enabled = false
observed = true
```

means the source explicitly reports NoPE.

```text
RoPE.enabled = null
observed = false
```

means the source does not contain enough information.

The system must never map absence of text to `false`.

---

# 6. Variable schema

Minimal representation:

```json
{
  "id": "var_xxx",
  "concept_id": "fourier.enabled",
  "type": "bool",
  "value": true,
  "observed": true,
  "context_id": "ctx_xxx",
  "source_evidence": ["ev_xxx"]
}
```

Number:

```json
{
  "id": "var_xxx",
  "concept_id": "fourier.sigma",
  "type": "number",
  "value": 10,
  "unit_raw": null,
  "observed": true,
  "source_evidence": ["ev_xxx"]
}
```

Expression:

```json
{
  "id": "var_xxx",
  "concept_id": "residual.expression",
  "type": "expression",
  "value_raw": "u_t + u*u_x - nu*u_xx",
  "ast": {},
  "observed": true,
  "source_evidence": ["ev_xxx"]
}
```

---

# 7. Five edge operators

## 7.1 Transform — \(T\)

A deterministic relation:

\[
\boxed{
y=T(x).
}
\]

Examples:

\[
u\rightarrow u_x
\]

\[
x\rightarrow Fourier(x)
\]

\[
Residual\rightarrow ResidualLoss.
\]

Schema:

```json
{
  "operator": "T",
  "source": ["var_a"],
  "target": ["var_b"],
  "expression": "..."
}
```

---

# 7.2 Kernel — \(K\)

A conditional or stochastic relationship:

\[
\boxed{
K(Y|X,C)
=
P(Y|X,C).
}
\]

Examples:

\[
P(Error|Fourier,Residual,PDE)
\]

\[
P(Convergence|DynamicWeighting,Architecture).
\]

The kernel may initially be empirical rather than explicitly probabilistic.

Schema:

```json
{
  "operator": "K",
  "source": ["var_a", "var_b"],
  "target": ["result_y"],
  "context": ["ctx_xxx"],
  "distribution": null,
  "estimate": {},
  "evidence": []
}
```

---

# 7.3 Intervention — \(I\)

An explicit change in system configuration:

\[
\boxed{
I:
X_0\rightarrow X_1.
}
\]

Examples:

```text
Fourier.enabled: false → true
Residual.strong: true → false
Residual.weak: false → true
RoPE.base: 10000 → 500000
```

Multiple changes are represented as one intervention set:

\[
\boxed{
I=
\{
X_1:a\rightarrow b,
X_2:c\rightarrow d,
\dots
\}.
}
\]

This prevents false causal decomposition.

---

# 7.4 Marginalization / aggregation — \(M\)

M removes, aggregates, or coarse-grains variables:

\[
\boxed{
M_*\mu_{\text{fine}}
=
\mu_{\text{coarse}}.
}
\]

Examples:

- average over random seeds;
- marginalize optimizer;
- merge \(b_1,b_2,b_3\) back to \(B\);
- integrate over an unobserved variable.

Discrete form:

\[
P(C|A)
=
\sum_b
P(C|A,b)P(b|A).
\]

---

# 7.5 Quotient / abstraction — \(Q\)

Q creates a higher-level concept from a lower-level equivalence class:

\[
\boxed{
Q:X\rightarrow X/{\sim}.
}
\]

Example candidate:

\[
\{
FourierFeatures,
SIREN,
WaveletEncoding
\}
\xrightarrow{Q}
SpectralEnrichment.
\]

Q must not be accepted solely because an LLM reports semantic similarity.

The graph system validates Q using invariance and predictive consistency.

---

# 8. Evidence layer

Every extracted scientific object must maintain direct provenance.

Evidence record:

```json
{
  "id": "ev_xxx",
  "document_id": "paper_xxx",
  "page": 6,
  "section": "4.2",
  "text_span": "...",
  "figure": null,
  "table": null,
  "extractor_confidence": 0.96
}
```

Evidence should point to variables and operators:

\[
Evidence
\rightarrow
Variable
\]

and

\[
Evidence
\rightarrow
Operator.
\]

Evidence never substitutes for a scientific relation.

---

# 9. Axiom / constraint layer

Scientific conclusions are conditional on:

\[
\boxed{
P(C|Context,AxiomSet).
}
\]

An AxiomSet can contain:

- governing PDE;
- physical conservation laws;
- mathematical assumptions;
- boundary assumptions;
- modelling assumptions;
- evaluation protocol.

Example:

```text
AxiomSet:
    PDE = Burgers
    viscosity > 0
    boundary = periodic
```

This layer becomes essential when conflicting systems are individually coherent under different assumptions.

---

# 10. LLM extraction pipeline

The LLM should perform semantic extraction, not final mathematical judgment.

Pipeline:

\[
\boxed{
Document
\rightarrow
RawExtraction
\rightarrow
Canonicalization
\rightarrow
ExperimentDiff
\rightarrow
GraphInsertion
}
\]

---

# 10.1 Stage A — document segmentation

Split paper into semantic regions:

```text
title
abstract
method
architecture
loss
training
experiments
ablation
results
limitations
appendix
```

Tables and captions must remain addressable.

---

# 10.2 Stage B — raw extraction

The LLM extracts only explicitly supported information.

Required output:

```json
{
  "variables": [],
  "expressions": [],
  "reported_interventions": [],
  "outcomes": [],
  "baselines": [],
  "contexts": [],
  "axioms": [],
  "evidence_spans": []
}
```

Every field must include evidence.

No evidence → no canonical variable.

---

# 10.3 Stage C — canonicalization

The canonicalizer maps textual variants to canonical concepts.

Examples:

```text
adaptive loss weighting
dynamic loss weighting
gradient-based balancing
```

may map to related concepts under:

```text
loss.dynamic
loss.gradient_control
```

Canonicalization can use the LLM, embedding retrieval, and deterministic aliases.

The original text remains preserved.

---

# 10.4 Stage D — baseline detection

This is one of the most important extraction stages.

The system must identify:

\[
X_{baseline}
\]

and

\[
X_{proposed}.
\]

The difference becomes:

\[
\boxed{
\Delta X
=
X_{proposed}-X_{baseline}.
}
\]

For Boolean variables:

\[
\Delta b\in\{-1,0,+1\}.
\]

For Numbers:

\[
\Delta x=x_1-x_0.
\]

For Expressions:

\[
\Delta e=(e_0,e_1).
\]

---

# 10.5 Stage E — intervention construction

Suppose a paper changes residual and Fourier features simultaneously.

The extractor creates:

\[
I=
\{
Residual:R_0\rightarrow R_1,
Fourier:F_0\rightarrow F_1
\}.
\]

The outcome is attached to the joint intervention:

\[
\boxed{
(R_1,F_1,C)
\xrightarrow{K}
Y.
}
\]

The system does not create independent relations:

\[
R\rightarrow Y
\]

and

\[
F\rightarrow Y
\]

until identifiability permits them.

---

# 11. PINN MVP canonical configuration

For the first domain, use PINN literature.

Recommended top-level concepts:

```text
problem
physics
residual
representation
architecture
sampling
loss
optimizer
training
result
```

Example decomposition:

```text
residual.enabled
residual.strong_form
residual.weak_form
residual.expression
residual.sample_count

fourier.enabled
fourier.sigma
fourier.dimension
siren.enabled

loss.dynamic
loss.residual_weight
loss.boundary_weight
loss.weight_expression

sampling.random
sampling.lhs
sampling.adaptive
sampling.sample_count

optimizer.adam
optimizer.lbfgs
optimizer.learning_rate

result.l2_error
result.relative_error
result.residual_error
result.training_time
```

All remain Bool, Number, or Expression.

---

# 12. Example extraction

Paper A:

```text
Strong-form residual
No Fourier features
Static equal loss weighting
L2 error = 0.10
```

Canonical state:

\[
X_A=(R_0,F_0,W_0).
\]

Paper B:

```text
Adaptive residual
Fourier features sigma=10
Static weighting
L2 error = 0.03
```

Canonical state:

\[
X_B=(R_1,F_1,W_0).
\]

The graph stores:

\[
I_{AB}
=
\{
R_0\rightarrow R_1,
F_0\rightarrow F_1
\}.
\]

Current evidence supports:

\[
\boxed{
(R_1,F_1)
\rightarrow
\Delta Y=-0.07.
}
\]

It does not yet support separate:

\[
R_1\rightarrow\Delta Y_R
\]

or

\[
F_1\rightarrow\Delta Y_F.
\]

---

# 13. Historical graph matching

After inserting an experiment, search the existing graph for neighboring configurations.

For:

\[
(R_0,F_0)
\rightarrow
(R_1,F_1),
\]

search:

\[
(R_1,F_0)
\]

and

\[
(R_0,F_1).
\]

If all four configurations exist:

\[
(R_0,F_0),
(R_1,F_0),
(R_0,F_1),
(R_1,F_1),
\]

the system can estimate interaction.

---

# 14. Multi-variable intervention computation

Define:

\[
Y_{00}=Y(R_0,F_0)
\]

\[
Y_{10}=Y(R_1,F_0)
\]

\[
Y_{01}=Y(R_0,F_1)
\]

\[
Y_{11}=Y(R_1,F_1).
\]

Residual effect:

\[
\Delta_R
=
Y_{10}-Y_{00}.
\]

Fourier effect:

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
Y_{11}
-
Y_{10}
-
Y_{01}
+
Y_{00}.
}
\]

Total:

\[
\Delta Y
=
\Delta_R+\Delta_F+\Delta_{RF}.
\]

Until sufficient controls exist:

```text
joint_effect = identifiable
residual_effect = unresolved
fourier_effect = unresolved
interaction = unresolved
```

Identifiability is therefore a first-class graph property.

---

# 15. Local response model

With sufficient experimental coverage, estimate:

\[
Y=f(X).
\]

For mixed Boolean/numerical configuration \(X\):

\[
\Delta Y
\approx
\nabla f^\top\Delta X
+
\frac12
\Delta X^\top H_f\Delta X.
\]

First derivatives represent local effects.

Second derivatives represent interactions:

\[
\frac{\partial^2Y}
{\partial R\partial F}.
\]

For Boolean variables, use finite discrete differences.

For Number variables, use continuous derivatives where justified.

---

# 16. Normalization

Anyway should preserve three different representations.

## Raw

Exactly what the source reports:

```text
learning_rate = 1e-3
pressure = 80 MPa
Fourier sigma = 10
```

## Canonical

Units and concept identifiers normalized.

## Comparison representation

Only generated when graph computation needs comparable values.

For dimensionless local sensitivity:

\[
E_{ij}
=
\frac{\partial Y}{\partial X_i}
\frac{X_i}{Y}.
\]

Loss contributions may use:

\[
G_i(t)
=
\lambda_i(t)
\|\nabla_\theta L_i(t)\|.
\]

Normalized effective contribution:

\[
\bar G_i(t)
=
\frac{G_i(t)}
{\sum_jG_j(t)}.
\]

Raw values must never be overwritten by normalized values.

---

# 17. Five consistency axioms

The internal graph engine should enforce five basic axioms.

---

## Axiom 1 — Composition closure

For:

\[
f:X\rightarrow Y
\]

and:

\[
g:Y\rightarrow Z
\]

there exists:

\[
g\circ f:X\rightarrow Z.
\]

Composition is associative:

\[
(h\circ g)\circ f
=
h\circ(g\circ f).
\]

Every object has identity:

\[
id_X.
\]

This makes finite Chains computable.

---

# 17.1 Axiom 2 — Representation invariance

Equivalent representations should preserve scientific meaning.

For allowed transformation:

\[
g_X\in Aut(X),
\]

require:

\[
\boxed{
f\circ g_X
\simeq
g_Y\circ f.
}
\]

Examples:

- units;
- variable renaming;
- equivalent coordinate systems;
- mathematically equivalent expressions.

The invertible part of Anyway forms a groupoid.

---

# 17.2 Axiom 3 — Path coherence

For two valid paths:

\[
\gamma_1,\gamma_2:X\rightarrow Y,
\]

equivalent scientific computation should satisfy:

\[
\boxed{
\gamma_1\simeq\gamma_2.
}
\]

For differentiable effects:

\[
\boxed{
D_{\gamma_1}
\simeq
D_{\gamma_2}.
}
\]

Locally:

\[
[\nabla_i,\nabla_j]
\rightarrow0.
\]

For two independent interventions:

\[
I_iI_j
\simeq
I_jI_i.
\]

The MVP should calculate a path error:

\[
\boxed{
E_{path}
=
d(
R_{\gamma_1},
R_{\gamma_2}
).
}
\]

---

# 17.3 Axiom 4 — Projective consistency

If an intermediate concept is refined:

\[
B
\rightarrow
\{b_1,\ldots,b_k\},
\]

marginalizing the refined representation should recover the coarse relation:

\[
\boxed{
M_*\mu_{fine}
\simeq
\mu_{coarse}.
}
\]

For upstream \(A\) and downstream \(C\):

\[
P(C|A)
=
\sum_i
P(C|A,b_i)P(b_i|A).
\]

This implements branch stability.

---

# 17.4 Axiom 5 — Abstraction naturality

For abstraction:

\[
Q_X:X\rightarrow\bar X
\]

and:

\[
Q_Y:Y\rightarrow\bar Y,
\]

a valid higher-level relation \(\bar f\) must satisfy:

\[
\boxed{
Q_Y\circ f
\simeq
\bar f\circ Q_X.
}
\]

Diagram:

```text
X --------f--------> Y
|                     |
Qx                    Qy
|                     |
v                     v
X̄ -------f̄--------> Ȳ
```

The error:

\[
E_Q
=
d(
Q_Yf,
\bar fQ_X
)
\]

must remain below threshold.

---

# 18. Contradiction handling

Contradictions must never be collapsed directly into a scalar probability.

The evaluation order is:

\[
\boxed{
Conflict
\rightarrow
ContextCheck
\rightarrow
AxiomCheck
\rightarrow
InternalConsistencyCheck.
}
\]

---

# 18.1 Context branch

Example:

\[
P(C|A,Z=0)\rightarrow1
\]

while:

\[
P(\neg C|A,Z=1)\rightarrow1.
\]

The correct operation is fiber refinement:

\[
F_A
\rightarrow
F_{A,Z=0}
\sqcup
F_{A,Z=1}.
\]

---

# 18.2 Axiom branch

Two internally coherent systems may use different assumptions:

\[
P(C|x,a_1)\rightarrow1
\]

and:

\[
P(\neg C|x,a_2)\rightarrow1.
\]

Store two branches.

Do not average them into:

\[
P(C)=0.5.
\]

---

# 18.3 Internal conflict

Only when:

\[
Context_1=Context_2
\]

and:

\[
AxiomSet_1=AxiomSet_2
\]

should opposing evidence be treated as direct internal conflict.

Positive and negative evidence remain independently addressable.

---

# 19. Abstraction generation

LLM may propose abstraction candidates.

Example:

\[
\{
Fourier,
SIREN,
Wavelet
\}
\rightarrow
SpectralEnrichment.
\]

The graph engine validates the candidate.

An abstraction \(Q\) should satisfy approximately:

\[
Compression(Q)>\tau_c
\]

\[
PredictionLoss(Q)<\epsilon_p
\]

\[
CommutationError(Q)<\epsilon_m
\]

\[
ConflictIncrease(Q)<\epsilon_c.
\]

Information-theoretically:

\[
I(Y;X|Q(X))
<\epsilon.
\]

Meaning:

> after knowing the abstract concept, lower-level identity contributes little additional information to the target observable.

Q is therefore:

\[
\boxed{
Abstraction
=
InvariantExtraction
+
RelationPreservation.
}
\]

LLM proposes Q.

The graph system approves Q.

---

# 20. Hash and provenance structure

A Block should be content-addressable.

Example:

\[
h_B
=
H(
concept,
value,
context,
source
).
\]

An operator:

\[
h_O
=
H(
operator,
sources,
targets,
parameters,
evidence
).
\]

A Chain:

\[
\boxed{
h_\gamma
=
H(
h_{B_0},
h_{O_1},
h_{B_1},
\dots,
h_{O_n},
h_{B_n}
).
}
\]

A Fiber contains compatible chain hashes.

This provides:

- reproducibility;
- deduplication;
- version tracking;
- distributed sharing;
- evidence traceability.

The MVP does not require blockchain consensus.

Content-addressable hash chains are sufficient.

---

# 21. Internal module architecture

Recommended modules:

```text
┌─────────────────────────────────────┐
│             Data Sources            │
│ papers / arXiv / repos / datasets   │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│             Ingestor                │
│ PDF / HTML / metadata / sections    │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│           LLM Extractor             │
│ variable / expression / baseline    │
│ intervention / result / evidence    │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│           Canonicalizer             │
│ concept IDs / aliases / units       │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│        Experiment Comparator        │
│ baseline diff / intervention set    │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│             Graph Store             │
│ Blocks / Edges / Chains / Fibers    │
│ Evidence / Context / AxiomSets      │
└───────────┬─────────────────────────┘
            │
      ┌─────┴──────────────────────┐
      │                            │
      ▼                            ▼
┌───────────────┐          ┌─────────────────┐
│ Graph Matcher │          │ Operator Engine │
│ nearest       │          │ T K I M Q       │
│ experiments   │          │ composition     │
└───────┬───────┘          └────────┬────────┘
        │                           │
        └─────────────┬─────────────┘
                      ▼
             ┌─────────────────┐
             │ Inference Engine│
             │ effects         │
             │ interactions    │
             │ Bayes / kernels │
             └────────┬────────┘
                      ▼
             ┌─────────────────┐
             │Consistency Engine│
             │ path / branch    │
             │ contradiction    │
             │ abstraction      │
             └─────────────────┘
```

---

# 22. Recommended MVP storage

Use a relational store first.

Recommended logical tables:

```text
documents
evidence
concepts
variables
contexts
axiom_sets
experiments
interventions
operators
operator_inputs
operator_outputs
chains
chain_members
fibers
fiber_members
results
abstraction_candidates
consistency_checks
```

PostgreSQL + JSONB is sufficient for the first implementation.

A dedicated graph database can be added when traversal volume justifies it.

The graph is a logical model; physical storage does not need to be a graph DB.

---

# 23. LLM extraction contract

The extractor must obey:

1. Extract only information supported by evidence.
2. Preserve exact text span.
3. Distinguish explicit false from unknown.
4. Never infer a single-variable causal effect from a multi-variable experiment.
5. Separate baseline and proposed configuration.
6. Return raw values before canonicalization.
7. Preserve mathematical expressions.
8. Report extraction uncertainty.
9. Prefer sparse output.
10. Never invent missing hyperparameters.

Minimal extraction result:

```json
{
  "baseline": {},
  "proposed": {},
  "changes": [],
  "results": [],
  "context": {},
  "axioms": [],
  "evidence": []
}
```

---

# 24. Two-pass extraction

For stability, use two logical passes.

## Pass 1 — extractor

Produces candidate structure.

## Pass 2 — verifier

Receives:

```text
candidate
+
source spans
```

and returns:

```text
supported
unsupported
ambiguous
missing
```

Only supported elements enter the canonical graph.

The verifier should not rewrite the scientific interpretation unless evidence demands it.

---

# 25. Historical retrieval

After canonical graph insertion:

\[
X_{new}
\]

search for neighboring configurations:

\[
d(X_{new},X_i).
\]

Distance should initially use sparse intervention distance.

For Bool:

\[
d_b(X,Y)
=
\sum_i
1[x_i\neq y_i].
\]

For Numbers:

\[
d_n
=
\sum_j
w_j
|\tilde x_j-\tilde y_j|.
\]

For Expressions:

use:

1. exact normalized AST match;
2. symbolic equivalence where available;
3. semantic similarity only as fallback candidate retrieval.

The matcher should specifically search for missing factorial controls.

---

# 26. Kernel estimation

For a configuration \(X\), context \(C\), and outcome \(Y\):

\[
K(Y|X,C).
\]

Initial MVP estimates can use weighted empirical observations:

\[
\hat\mu_{X,C}
=
\frac{
\sum_iw_i\delta_{Y_i}
}{
\sum_iw_i
}.
\]

Weights can later include:

```text
measurement uncertainty
sample size
replication
source independence
evaluation comparability
```

The first MVP should avoid complicated global confidence formulas.

Preserve the underlying evidence.

---

# 27. Bayesian update

For a hypothesis \(H\):

\[
P(H|E)
\propto
P(E|H)P(H).
\]

Bayesian inference should operate over identified hypotheses, rather than every graph edge.

A graph edge should therefore preserve enough evidence to construct likelihoods later.

The MVP can initially expose Bayesian update as an optional inference module.

---

# 28. Markov structure

When a relationship satisfies a conditional independence structure:

\[
P(Y|X,Z,C)
=
P(Y|Z,C),
\]

the graph may encode a Markov simplification.

A full DAG factorization is:

\[
P(V_1,\ldots,V_n)
=
\prod_iP(V_i|Pa(V_i)).
\]

Markov assumptions must be explicitly stored.

They should not be inferred merely from graph topology.

---

# 29. Path consistency computation

Suppose two chains connect:

\[
A\rightarrow C.
\]

Path one:

\[
A\rightarrow B\rightarrow C.
\]

Path two:

\[
A\rightarrow D\rightarrow C.
\]

Compute:

\[
R_{\gamma_1}
=
R_{BC}\circ R_{AB}
\]

and:

\[
R_{\gamma_2}
=
R_{DC}\circ R_{AD}.
\]

Then:

\[
\boxed{
E_{path}
=
d(
R_{\gamma_1},
R_{\gamma_2}
).
}
\]

For numerical differentiable relations:

\[
E_{path}
=
\left|
\frac{\partial C}{\partial A}\bigg|_{\gamma_1}
-
\frac{\partial C}{\partial A}\bigg|_{\gamma_2}
\right|.
\]

For probability kernels:

\[
E_{path}
=
D(
K_{\gamma_1},
K_{\gamma_2}
)
\]

using an appropriate distribution distance.

---

# 30. Branch stability computation

Suppose:

\[
A\rightarrow B\rightarrow C
\]

and later:

\[
B
\rightarrow
\{b_1,b_2,b_3\}.
\]

Compute:

\[
K_{fine}(C|A)
=
\sum_i
K(C|A,b_i)
K(b_i|A).
\]

Compare against:

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

Stable branch refinement requires:

\[
E_{branch}<\epsilon.
\]

---

# 31. Contradiction computation

For opposing conclusions:

\[
C
\]

and:

\[
\neg C,
\]

first partition by:

\[
(Context,AxiomSet).
\]

For every identical partition, retain separate evidence sets:

```text
supporting_evidence
opposing_evidence
```

The system reports:

```text
contextual_divergence
axiomatic_divergence
internal_conflict
insufficient_resolution
```

The system should not automatically collapse contradiction into \(P=0.5\).

---

# 32. Extraction and graph computation boundaries

## LLM responsibilities

```text
scientific entity recognition
method identification
baseline identification
variable extraction
formula extraction
intervention detection
result extraction
context extraction
axiom candidate extraction
semantic canonicalization candidate
abstraction candidate proposal
```

## Deterministic / statistical responsibilities

```text
schema validation
unit normalization
bool consistency
hashing
graph insertion
operator type checks
multi-variable intervention construction
identifiability
interaction calculation
path consistency
marginalization
Bayesian updates
branch consistency
abstraction validation
```

The LLM is the semantic parser.

The graph engine is the mathematical authority.

---

# 33. MVP implementation order

## Phase 1 — extraction

Implement:

```text
PDF/text ingestion
section splitting
evidence spans
Bool / Number / Expression extraction
baseline detection
result extraction
context extraction
```

---

## Phase 2 — canonical experiment

Produce:

\[
Experiment
=
(
Context,
Baseline,
Proposed,
InterventionSet,
Outcome,
Evidence
).
\]

---

## Phase 3 — graph storage

Implement:

```text
Block
Operator
Chain
Evidence
Context
Experiment
Fiber
```

Operators initially:

```text
T
K
I
M
```

Support Q schema, while keeping automatic Q validation experimental.

---

## Phase 4 — historical matching

Given an intervention:

\[
(R_0,F_0)
\rightarrow
(R_1,F_1),
\]

search existing graph for:

\[
(R_1,F_0)
\]

and:

\[
(R_0,F_1).
\]

---

## Phase 5 — identifiability

Return:

```text
joint effect
single variable effect
interaction effect
unidentified components
required missing controls
```

---

## Phase 6 — consistency engine

Implement:

```text
path consistency
branch consistency
context conflict detection
axiom conflict detection
```

---

## Phase 7 — abstraction candidate

Allow:

```text
LLM proposal
→ graph validation
→ accepted/rejected/pending
```

---

# 34. MVP API surface

Recommended conceptual API:

```text
POST /documents
POST /extract
POST /canonicalize
POST /experiments
GET  /experiments/{id}
GET  /experiments/{id}/neighbors
GET  /experiments/{id}/interventions

POST /graph/query
POST /graph/path
POST /graph/effect
POST /graph/interaction
POST /graph/marginalize

POST /consistency/path
POST /consistency/branch
POST /consistency/conflict

POST /abstraction/propose
POST /abstraction/validate
```

---

# 35. Example end-to-end PINN workflow

Input paper states:

```text
We replace the standard strong-form residual with an adaptive residual,
introduce random Fourier features with sigma 10,
and dynamically reweight residual and boundary losses.
Relative L2 error decreases from 0.11 to 0.021.
```

Extractor produces:

```text
Residual.strong_form: true → false
Residual.adaptive: false → true

Fourier.enabled: false → true
Fourier.sigma: unknown → 10

Loss.dynamic: false → true

Result.relative_l2:
0.11 → 0.021
```

Intervention:

\[
I=
\{
\Delta Residual,
\Delta Fourier,
\Delta DynamicWeight
\}.
\]

Outcome:

\[
\Delta Y=-0.089.
\]

Current graph conclusion:

\[
\boxed{
P(
Y
|
do(R_1,F_1,W_1),
C
)
}
\]

The graph does not assign three independent effects.

Historical matcher searches:

```text
R1 F0 W0
R0 F1 W0
R0 F0 W1
R1 F1 W0
R1 F0 W1
R0 F1 W1
```

As evidence fills the intervention cube, Anyway estimates:

\[
\Delta_R,
\Delta_F,
\Delta_W,
\Delta_{RF},
\Delta_{RW},
\Delta_{FW},
\Delta_{RFW}.
\]

This is the correct computational interpretation of cross-paper evidence.

---

# 36. Core abstraction hierarchy

The long-term hierarchy is:

\[
\boxed{
RawEvidence
\rightarrow
Variable
\rightarrow
Intervention
\rightarrow
Chain
\rightarrow
Fiber
\rightarrow
Bundle
\rightarrow
Mechanism
\rightarrow
Theory
}
\]

Mechanisms and theories are generated through validated Q operators.

The MVP stops primarily at Bundle + candidate Q.

---

# 37. Non-goals for MVP

Do not attempt yet:

```text
fully autonomous causal discovery
global scientific truth probability
automatic proof of every paper
universal ontology
automatic creation of unrestricted new operator types
global graphon / infinite-network limit calculation
automatic theory acceptance
blockchain consensus
```

The architecture should preserve enough structure to support these later.

---

# 38. MVP success criteria

The MVP is successful when it can take a PINN literature corpus and reliably perform the following pipeline:

\[
\boxed{
Paper
\rightarrow
Configuration
\rightarrow
Intervention
\rightarrow
Outcome
\rightarrow
CrossPaperGraph
}
\]

and satisfy these concrete tests:

1. Extract whether residual, Fourier representation, dynamic weighting, sampling and major architecture mechanisms are used.
2. Extract relevant numerical parameters.
3. Preserve residual and weighting expressions.
4. Distinguish `false` from `unknown`.
5. Identify baseline and proposed method.
6. Detect multi-variable intervention.
7. Avoid attributing a joint intervention to individual variables without controls.
8. Retrieve historical neighboring configurations.
9. Compute main effects when identifiable.
10. Compute interaction effects when identifiable.
11. Compare multiple computational paths.
12. Test coarse/fine branch consistency.
13. Separate context conflicts from axiom conflicts.
14. Preserve complete evidence provenance.
15. Produce abstraction candidates while requiring graph validation before promotion.

---

# 39. Minimal formal specification

The minimum Anyway object can be summarized as:

\[
\boxed{
\mathfrak A
=
(
V,
O,
C,
E,
F,
\mathcal A
)
}
\]

where:

\[
V
=
Bool\cup Number\cup Expression
\]

is the variable space;

\[
O
=
\{T,K,I,M,Q\}
\]

is the operator basis;

\[
C
\]

is the set of Chains;

\[
E
\]

is provenance evidence;

\[
F
\]

is the set of context-conditioned Fibers;

\[
\mathcal A
\]

is the axiom/context structure.

Every finite computation is a typed composition:

\[
\boxed{
\gamma
=
o_n\circ\cdots\circ o_1,
\qquad
o_i\in O.
}
\]

The graph is considered internally coherent when the relevant operations satisfy:

\[
\boxed{
\begin{aligned}
&\text{Composition} &&
g\circ f\in\mathsf{Anyway}
\\
&\text{Representation invariance} &&
fg_X\simeq g_Yf
\\
&\text{Path coherence} &&
R_{\gamma_1}\simeq R_{\gamma_2}
\\
&\text{Projective consistency} &&
M_*\mu_{fine}\simeq\mu_{coarse}
\\
&\text{Abstraction naturality} &&
Q_Yf\simeq\bar fQ_X.
\end{aligned}
}
\]

The central engineering rule is:

\[
\boxed{
\textbf{LLM extracts semantics; the graph engine decides computation.}
}
\]

The central mathematical rule is:

\[
\boxed{
\textbf{A scientific relation is valid only inside an explicit context, intervention structure, and evidence path.}
}
\]

The central MVP representation is therefore:

\[
\boxed{
\textbf{3 primitive variable types + 5 operators + provenance + context + consistency tests.}
}
\]