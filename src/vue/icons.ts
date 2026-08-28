/**
 * Shared icon registry for the morphicons-backed dynamic icons.
 *
 * Icons are stroke paths. The node-type glyphs were drawn on a 20×20 grid, so
 * they are re-gridded once at module scope onto morphicons' shared 24×24
 * coordinate space (morphicons only morphs endpoints that live on the same
 * grid — see the README note on `fitIcon`).
 */

import { fitIcon } from "morphicons";
import type { ResearchNodeType } from "../../app/lib/research-types";

/** Raw 20×20 stroke paths (single source of truth for node-type glyphs). */
const RAW_NODE_ICONS: Partial<Record<ResearchNodeType, string>> = {
  question: "M10 17h.01M7.5 7.6a2.7 2.7 0 1 1 4.4 2.1c-1.1.9-1.9 1.3-1.9 3.3M4 3.5h12A1.5 1.5 0 0 1 17.5 5v10A1.5 1.5 0 0 1 16 16.5H4A1.5 1.5 0 0 1 2.5 15V5A1.5 1.5 0 0 1 4 3.5Z",
  concept: "M8 9.5a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm-5.5 6.2a5.5 5.5 0 0 1 11 0M14.5 8a2.5 2.5 0 1 0 0-5M14 10.4a4.6 4.6 0 0 1 3.5 4.5",
  variable: "M3 15.5V8.8m4 6.7V5m4 10.5v-8m4 8V3.5M2 15.5h14.5",
  method: "M8 2.5 9.7 6l3.8.5-2.8 2.6.8 3.9L8 11.2l-3.5 1.8.8-3.9L2.5 6.5 6.3 6 8 2.5Z",
  evidence: "M4 2.5h7l3 3v10H4a1.5 1.5 0 0 1-1.5-1.5V4A1.5 1.5 0 0 1 4 2.5Zm7 0v3h3M5.5 9h5M5.5 12h5",
  paper: "M4 2.5h7l3 3v10H4a1.5 1.5 0 0 1-1.5-1.5V4A1.5 1.5 0 0 1 4 2.5Zm7 0v3h3M5.5 9h5M5.5 12h5",
  dataset: "M3 5c0-1.1 2.2-2 5-2s5 .9 5 2-2.2 2-5 2-5-.9-5-2Zm0 0v4c0 1.1 2.2 2 5 2s5-.9 5-2V5m-10 4v4c0 1.1 2.2 2 5 2s5-.9 5-2V9",
  result: "m3 9 3.2 3.2L14.5 4M3 15.5h12",
};

/** Re-grid a 20×20 path onto the shared 24×24 grid. */
function fit(path: string): string {
  return fitIcon(path, "0 0 20 20", 24);
}

/** Node-type glyphs, re-gridded to 24×24 for morphicons. */
export const nodeTypeIconPaths: Partial<Record<ResearchNodeType, string>> =
  Object.fromEntries(
    Object.entries(RAW_NODE_ICONS).map(([type, path]) => [type, path ? fit(path) : path]),
  ) as Partial<Record<ResearchNodeType, string>>;

/** Fallback outline square (24×24). */
export const defaultNodeIcon = "M4 4h16v16H4z";

/** Morphable chevrons (24×24): right ↔ down rotate 90° via Procrustes. */
export const CHEVRON_RIGHT = "m9 18 6-6-6-6";
export const CHEVRON_DOWN = "m6 9 6 6 6-6";
