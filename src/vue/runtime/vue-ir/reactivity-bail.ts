/**
 * Vue reactivity integration for the UI IR contract.
 *
 * Without this augmentation, running the recursive UI IR document types
 * through Vue's `UnwrapRef` (e.g. when a plugin manifest that declares
 * `uiIr` contributions flows into a `reactive()` store) expands the node
 * union so deeply that TypeScript reports TS2589 "excessively deep and
 * possibly infinite" instantiation. `RefUnwrapBailTypes` is Vue's public,
 * documented extension point for exactly this case: the IR document is
 * immutable declarative data and is treated as raw by the unwrap.
 *
 * Type-only; there is no runtime effect and the IR contract itself stays
 * free of any Vue import.
 */
import type { UiIrDocument } from "../../../../app/plugins/ui-ir";

declare module "@vue/reactivity" {
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  interface RefUnwrapBailTypes {
    readonly uiIrDocument: UiIrDocument;
  }
}

export {};
