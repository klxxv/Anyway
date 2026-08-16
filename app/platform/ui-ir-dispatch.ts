/**
 * Action dispatcher adapter: maps UI IR action requests onto the unified
 * Host SDK. Lives in app/platform because it depends only on the transport
 * facing SDK and the data-only UI IR contract, never on Vue.
 */
import type { HostSdk } from "./host-sdk";
import {
  type UiIrActionDispatcher,
  type UiIrActionRequest,
  type UiIrPermissionPolicy,
} from "../plugins/ui-ir";

export type UiIrActionDisallowedCode =
  | "action-policy-missing"
  | "action-not-allowed"
  | "capability-not-allowed"
  | "action-capability-not-allowed"
  | "plugin-mismatch";

/** Typed rejection for an action the permission policy does not allow. */
export class UiIrActionDisallowedError extends Error {
  readonly name = "UiIrActionDisallowedError";

  constructor(
    readonly code: UiIrActionDisallowedCode,
    readonly actionId: string,
    readonly capability: string,
    message: string,
  ) {
    super(message);
  }
}

function toSet(
  value: ReadonlySet<string> | readonly string[] | undefined,
): ReadonlySet<string> | undefined {
  if (value === undefined) return undefined;
  return value instanceof Set ? value : new Set(value);
}

function assertAllowed(
  request: UiIrActionRequest,
  pluginId: string,
  policy: UiIrPermissionPolicy,
): void {
  if (request.pluginId !== pluginId) {
    throw new UiIrActionDisallowedError(
      "plugin-mismatch",
      request.actionId,
      request.capability,
      `UI_IR_ACTION_DISALLOWED: plugin '${request.pluginId}' does not match dispatcher plugin '${pluginId}'`,
    );
  }
  const requireAllowlist = policy.requireActionAllowlist !== false;
  const actions = toSet(policy.allowedActions);
  const capabilities = toSet(policy.allowedCapabilities);
  if (requireAllowlist && (!actions || !capabilities)) {
    throw new UiIrActionDisallowedError(
      "action-policy-missing",
      request.actionId,
      request.capability,
      `UI_IR_ACTION_DISALLOWED: action allowlist is missing for '${request.actionId}'`,
    );
  }
  if (actions && !actions.has(request.actionId)) {
    throw new UiIrActionDisallowedError(
      "action-not-allowed",
      request.actionId,
      request.capability,
      `UI_IR_ACTION_DISALLOWED: action '${request.actionId}' is not allowed for plugin '${request.pluginId}'`,
    );
  }
  if (capabilities && !capabilities.has(request.capability)) {
    throw new UiIrActionDisallowedError(
      "capability-not-allowed",
      request.actionId,
      request.capability,
      `UI_IR_ACTION_DISALLOWED: capability '${request.capability}' is not allowed for action '${request.actionId}'`,
    );
  }
  if (policy.allowedActionCapabilities) {
    const allowedCapabilities = toSet(policy.allowedActionCapabilities.get(request.actionId));
    if (!allowedCapabilities || !allowedCapabilities.has(request.capability)) {
      throw new UiIrActionDisallowedError(
        "action-capability-not-allowed",
        request.actionId,
        request.capability,
        `UI_IR_ACTION_DISALLOWED: capability '${request.capability}' is not paired with action '${request.actionId}'`,
      );
    }
  }
}

/**
 * Builds a {@link UiIrActionDispatcher} that validates the request against
 * the optional policy and forwards it through the Host SDK. The Host SDK
 * call uses the capability as the operation name and wraps the action in a
 * `{ actionId, parameters }` value payload.
 */
export function createUiIrActionDispatcher(
  hostSdk: HostSdk,
  pluginId: string,
  policy?: UiIrPermissionPolicy,
): UiIrActionDispatcher {
  return async (request: UiIrActionRequest): Promise<unknown> => {
    if (policy) assertAllowed(request, pluginId, policy);
    return hostSdk.call(request.capability, {
      actionId: request.actionId,
      parameters: request.parameters,
    });
  };
}
