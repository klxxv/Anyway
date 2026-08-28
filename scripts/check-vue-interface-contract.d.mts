export type PiniaStoreRequirement = {
  readonly file: string;
  readonly name: string;
  readonly id: string;
};

export type PiniaFacadeRequirement = {
  readonly file: string;
  readonly exportName: string;
  readonly storeName: string;
  readonly preserveRefs: readonly string[];
  readonly requireStoreToRefs: boolean;
};

export type PiniaConsumerRequirement = {
  readonly file: string;
  readonly storeName: string;
  readonly requireStoreToRefs: boolean;
};

export const REQUIRED_PINIA_STORES: readonly PiniaStoreRequirement[];
export const REQUIRED_PINIA_FACADES: readonly PiniaFacadeRequirement[];
export const REQUIRED_PINIA_CONSUMERS: readonly PiniaConsumerRequirement[];

export function collectContractViolations(root?: string): string[];
export function formatContractReport(violations: readonly string[]): string;
