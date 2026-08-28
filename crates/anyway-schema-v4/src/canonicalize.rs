//! Concept / unit / expression canonicalization (handoff-spec.md §70–§74).
//!
//! The canonicalizer maps textual variants to canonical concept ids, converts
//! raw values to canonical units, and normalizes expressions to a stable form.
//! It is deterministic and never destroys the raw source: the raw phrase,
//! value, and expression always remain recoverable (V3-15, V3-16).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// How a raw phrase was mapped to a canonical concept id.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MappingType {
    Exact,
    Alias,
    NewChild,
    Unresolved,
}

/// The canonicalization record for one raw concept phrase (handoff-spec.md §71).
///
/// The raw phrase must remain recoverable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CanonicalizationRecord {
    pub raw_concept: String,
    pub canonical_concept_id: String,
    pub mapping_type: MappingType,
    pub confidence: f64,
}

/// A deterministic concept canonicalizer with a PINN seed ontology (§69) and an
/// alias map.
#[derive(Clone, Debug)]
pub struct ConceptCanonicalizer {
    /// Canonical concept ids that are recognized verbatim.
    exact: std::collections::HashSet<String>,
    /// Normalized raw phrase -> canonical concept id.
    aliases: HashMap<String, String>,
    /// Normalized raw phrase -> alias confidence.
    confidence: HashMap<String, f64>,
}

impl ConceptCanonicalizer {
    /// The PINN MVP seed ontology plus a small alias map.
    pub fn pinn_seed() -> Self {
        let exact: std::collections::HashSet<String> = [
            "problem",
            "physics",
            "physics.pde",
            "physics.parameter",
            "residual.enabled",
            "residual.strong_form.enabled",
            "residual.weak_form.enabled",
            "residual.adaptive.enabled",
            "residual.expression",
            "residual.sample_count",
            "representation.fourier.enabled",
            "representation.fourier.sigma",
            "representation.fourier.dimension",
            "representation.siren.enabled",
            "representation.wavelet.enabled",
            "architecture.depth",
            "architecture.width",
            "sampling.random.enabled",
            "sampling.lhs.enabled",
            "sampling.adaptive.enabled",
            "sampling.sample_count",
            "loss.dynamic.enabled",
            "loss.residual_weight",
            "loss.boundary_weight",
            "loss.initial_weight",
            "loss.weight_expression",
            "optimizer.adam.enabled",
            "optimizer.lbfgs.enabled",
            "optimizer.learning_rate",
            "training.epochs",
            "training.batch_size",
            "result.l2_error",
            "result.relative_l2_error",
            "result.residual_error",
            "result.training_time",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let mut aliases = HashMap::new();
        let mut confidence = HashMap::new();
        for (raw, canonical) in [
            ("fourier feature encoding", "representation.fourier.enabled"),
            ("random fourier feature encoding", "representation.fourier.enabled"),
            ("random fourier features", "representation.fourier.enabled"),
            ("fourier features", "representation.fourier.enabled"),
            ("siren", "representation.siren.enabled"),
            ("siren activation", "representation.siren.enabled"),
            ("wavelet encoding", "representation.wavelet.enabled"),
            ("wavelet representation", "representation.wavelet.enabled"),
            ("adaptive residual", "residual.adaptive.enabled"),
            ("adaptive residual formulation", "residual.adaptive.enabled"),
            ("strong form residual", "residual.strong_form.enabled"),
            ("strong-form residual", "residual.strong_form.enabled"),
            ("weak form residual", "residual.weak_form.enabled"),
            ("dynamic loss weighting", "loss.dynamic.enabled"),
            ("dynamic reweighting", "loss.dynamic.enabled"),
            ("adaptive loss weighting", "loss.dynamic.enabled"),
            ("gradient-based balancing", "loss.dynamic.enabled"),
            ("adam optimizer", "optimizer.adam.enabled"),
            ("adam", "optimizer.adam.enabled"),
            ("lbfgs", "optimizer.lbfgs.enabled"),
            ("l-bfgs", "optimizer.lbfgs.enabled"),
            ("relative l2 error", "result.relative_l2_error"),
            ("relative l2", "result.relative_l2_error"),
            ("fourier bandwidth sigma", "representation.fourier.sigma"),
            ("fourier sigma", "representation.fourier.sigma"),
        ] {
            aliases.insert(normalize_phrase(raw), canonical.to_string());
            confidence.insert(normalize_phrase(raw), 0.95);
        }

        Self {
            exact,
            aliases,
            confidence,
        }
    }

    /// Map a raw phrase to a canonical concept id.
    ///
    /// - an exact canonical id maps with [`MappingType::Exact`] and confidence 1.0;
    /// - a known alias maps with [`MappingType::Alias`] and the alias confidence;
    /// - anything else maps to a slugified candidate with
    ///   [`MappingType::Unresolved`] and confidence 0.0.
    pub fn canonicalize(&self, raw: &str) -> CanonicalizationRecord {
        let phrase = normalize_phrase(raw);
        if self.exact.contains(&phrase) {
            return CanonicalizationRecord {
                raw_concept: raw.to_string(),
                canonical_concept_id: phrase,
                mapping_type: MappingType::Exact,
                confidence: 1.0,
            };
        }
        if let Some(canonical) = self.aliases.get(&phrase) {
            return CanonicalizationRecord {
                raw_concept: raw.to_string(),
                canonical_concept_id: canonical.clone(),
                mapping_type: MappingType::Alias,
                confidence: self.confidence.get(&phrase).copied().unwrap_or(0.95),
            };
        }
        CanonicalizationRecord {
            raw_concept: raw.to_string(),
            canonical_concept_id: slugify(&phrase),
            mapping_type: MappingType::Unresolved,
            confidence: 0.0,
        }
    }
}

impl Default for ConceptCanonicalizer {
    fn default() -> Self {
        Self::pinn_seed()
    }
}

/// The result of canonicalizing a raw number plus its unit (handoff-spec.md §72).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnitCanonicalization {
    pub value_raw: f64,
    pub unit_raw: Option<String>,
    pub value_canonical: f64,
    pub unit_canonical: Option<String>,
}

/// A tiny, deterministic SI-prefix table: `(prefix, factor, canonical_unit)`.
const SI_PREFIXES: &[(&str, f64)] = &[
    ("p", 1e-12),
    ("n", 1e-9),
    ("u", 1e-6),
    ("m", 1e-3),
    ("c", 1e-2),
    ("k", 1e3),
    ("M", 1e6),
    ("G", 1e9),
];

/// Convert a raw number to canonical SI units when the unit is recognized.
///
/// Recognized base units: `Pa`, `m`, `s`, `g`, `K`, `Hz`, `N`, `J`, `W`.
/// Unknown units are passed through unchanged so raw information is preserved.
pub fn canonicalize_number(value_raw: f64, unit_raw: Option<&str>) -> UnitCanonicalization {
    let Some(unit) = unit_raw.map(str::trim).filter(|u| !u.is_empty()) else {
        return UnitCanonicalization {
            value_raw,
            unit_raw: None,
            value_canonical: value_raw,
            unit_canonical: None,
        };
    };

    // Try a prefixed base unit first, then a bare base unit.
    for (base, aliases) in [
        ("Pa", &["Pa", "pa", "pascal"] as &[&str]),
        ("m", &["m", "meter", "metre"]),
        ("s", &["s", "sec", "second"]),
        ("g", &["g", "gram"]),
        ("K", &["K", "kelvin"]),
        ("Hz", &["Hz", "hz", "hertz"]),
        ("N", &["N", "newton"]),
        ("J", &["J", "joule"]),
        ("W", &["W", "watt"]),
    ] {
        if aliases.contains(&unit) {
            return UnitCanonicalization {
                value_raw,
                unit_raw: Some(unit.to_string()),
                value_canonical: value_raw,
                unit_canonical: Some(base.to_string()),
            };
        }
        for (prefix, factor) in SI_PREFIXES {
            let prefixed = format!("{prefix}{base}");
            if unit == prefixed || unit == prefixed.to_lowercase() {
                return UnitCanonicalization {
                    value_raw,
                    unit_raw: Some(unit.to_string()),
                    value_canonical: value_raw * factor,
                    unit_canonical: Some(base.to_string()),
                };
            }
        }
    }

    // Unknown unit: preserve raw and canonical identically.
    UnitCanonicalization {
        value_raw,
        unit_raw: Some(unit.to_string()),
        value_canonical: value_raw,
        unit_canonical: Some(unit.to_string()),
    }
}

/// The result of normalizing one expression (handoff-spec.md §73, §74).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExpressionNormalization {
    pub raw: String,
    pub normalized: String,
    pub raw_hash: String,
    pub normalized_hash: String,
}

/// Normalize an expression to a stable form and compute both hashes.
///
/// The normalization is deterministic: Unicode NFC, whitespace collapse, and a
/// small LaTeX-symbol folding. It does not perform symbolic equivalence
/// (that is a later, explicitly versioned pass).
pub fn normalize_expression(raw: &str) -> ExpressionNormalization {
    let normalized = normalize_expression_text(raw);
    ExpressionNormalization {
        raw: raw.to_string(),
        normalized: normalized.clone(),
        raw_hash: sha256_hex(raw),
        normalized_hash: sha256_hex(&normalized),
    }
}

fn normalize_expression_text(raw: &str) -> String {
    let nfc: String = raw.nfc().collect();
    let folded = nfc
        .replace("\\partial", "D")
        .replace("\\nabla", "Grad")
        .replace("\\cdot", "·")
        .replace("\\times", "×")
        .replace("\\frac", "/")
        .replace("\\left", "")
        .replace("\\right", "");
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_string()
}

fn normalize_phrase(raw: &str) -> String {
    let nfc: String = raw.nfc().collect();
    nfc.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn slugify(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_concept_id_is_exact() {
        let canonicalizer = ConceptCanonicalizer::pinn_seed();
        let record = canonicalizer.canonicalize("representation.fourier.enabled");
        assert_eq!(record.mapping_type, MappingType::Exact);
        assert_eq!(record.confidence, 1.0);
    }

    #[test]
    fn alias_maps_to_canonical_concept() {
        let canonicalizer = ConceptCanonicalizer::pinn_seed();
        let record = canonicalizer.canonicalize("random Fourier feature encoding");
        assert_eq!(record.mapping_type, MappingType::Alias);
        assert_eq!(record.canonical_concept_id, "representation.fourier.enabled");
        assert_eq!(record.raw_concept, "random Fourier feature encoding");
    }

    #[test]
    fn unknown_phrase_stays_unresolved_but_recoverable() {
        let canonicalizer = ConceptCanonicalizer::pinn_seed();
        let record = canonicalizer.canonicalize("neural tangent kernel adaptive weighting");
        assert_eq!(record.mapping_type, MappingType::Unresolved);
        assert_eq!(record.raw_concept, "neural tangent kernel adaptive weighting");
    }

    #[test]
    fn megapascal_converts_to_pascal() {
        let canonical = canonicalize_number(80.0, Some("MPa"));
        assert_eq!(canonical.value_canonical, 80_000_000.0);
        assert_eq!(canonical.unit_canonical.as_deref(), Some("Pa"));
        assert_eq!(canonical.value_raw, 80.0);
        assert_eq!(canonical.unit_raw.as_deref(), Some("MPa"));
    }

    #[test]
    fn bare_unit_is_identity() {
        let canonical = canonicalize_number(3.0, Some("Pa"));
        assert_eq!(canonical.value_canonical, 3.0);
        assert_eq!(canonical.unit_canonical.as_deref(), Some("Pa"));
    }

    #[test]
    fn unknown_unit_preserves_raw() {
        let canonical = canonicalize_number(5.0, Some("flops"));
        assert_eq!(canonical.value_canonical, 5.0);
        assert_eq!(canonical.unit_canonical.as_deref(), Some("flops"));
    }

    #[test]
    fn expression_normalization_is_deterministic_and_hashed() {
        let a = normalize_expression("u_t + u\\cdot u_x - \\nu u_{xx}");
        let b = normalize_expression("u_t + u\\cdot u_x - \\nu u_{xx}");
        assert_eq!(a.normalized, b.normalized);
        assert_eq!(a.normalized_hash, b.normalized_hash);
        assert!(a.normalized.contains("·"));
        assert_eq!(a.raw_hash.len(), 64);
    }

    #[test]
    fn expression_normalization_collapses_whitespace() {
        let a = normalize_expression("u_t   +  u_x");
        assert_eq!(a.normalized, "u_t + u_x");
    }
}
