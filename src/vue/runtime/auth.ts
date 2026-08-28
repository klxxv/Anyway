import { getPinia } from "../stores/pinia";
import { useRuntimeAuthStore } from "../stores/runtime-auth";

export type ChatGPTUser = {
  displayName: string;
  email: string;
  fullName: string | null;
};

/** Minimal trusted-header reader used by host-provided browser adapters. */
export type AuthHeaders = {
  get(name: string): string | null;
};

export type ChatGPTAuthAdapter = {
  /** Reads identity from a trusted host boundary; returning null is unauthenticated. */
  getUser: () => ChatGPTUser | null | Promise<ChatGPTUser | null>;
  /** Performs navigation using the host's router or browser integration. */
  redirect?: (path: string) => void | Promise<void>;
};

const USER_EMAIL_HEADER = "oai-authenticated-user-email";
const USER_FULL_NAME_HEADER = "oai-authenticated-user-full-name";
const USER_FULL_NAME_ENCODING_HEADER =
  "oai-authenticated-user-full-name-encoding";
const PERCENT_ENCODED_UTF8 = "percent-encoded-utf-8";
const SIGN_IN_PATH = "/signin-with-chatgpt";
const SIGN_OUT_PATH = "/signout-with-chatgpt";
const CALLBACK_PATH = "/callback";

/** Thrown after a redirect adapter returns instead of throwing or navigating. */
export class ChatGPTAuthRedirectError extends Error {
  readonly path: string;

  constructor(path: string) {
    super(`ChatGPT authentication required: ${path}`);
    this.name = "ChatGPTAuthRedirectError";
    this.path = path;
  }
}

/**
 * Injects the trusted identity/navigation bridge used by the Vue host.
 * Returns a restore function so tests and embedded hosts can scope the change.
 */
export function setChatGPTAuthAdapter(
  adapter: ChatGPTAuthAdapter | null,
): () => void {
  return useRuntimeAuthStore(getPinia()).setAdapter(adapter);
}

/** Converts trusted proxy headers to the existing user shape. */
export function chatGPTUserFromHeaders(
  requestHeaders: AuthHeaders,
): ChatGPTUser | null {
  const email = requestHeaders.get(USER_EMAIL_HEADER);
  if (!email) return null;

  const encodedFullName = requestHeaders.get(USER_FULL_NAME_HEADER);
  const fullName =
    encodedFullName &&
    requestHeaders.get(USER_FULL_NAME_ENCODING_HEADER) === PERCENT_ENCODED_UTF8
      ? safeDecodeURIComponent(encodedFullName)
      : null;

  return {
    displayName: fullName ?? email,
    email,
    fullName,
  };
}

/** Reads the current user through the injected trusted adapter. */
export async function getChatGPTUser(): Promise<ChatGPTUser | null> {
  return useRuntimeAuthStore(getPinia()).getUser();
}

/** Requires identity and delegates unauthenticated navigation to the adapter. */
export async function requireChatGPTUser(
  returnTo: string,
): Promise<ChatGPTUser> {
  const user = await getChatGPTUser();
  if (user) return user;

  const path = chatGPTSignInPath(returnTo);
  await useRuntimeAuthStore(getPinia()).redirect(path);
  throw new ChatGPTAuthRedirectError(path);
}

export function chatGPTSignInPath(returnTo: string): string {
  const safeReturnTo = safeRelativeReturnPath(returnTo);
  return `${SIGN_IN_PATH}?return_to=${encodeURIComponent(safeReturnTo)}`;
}

export function chatGPTSignOutPath(returnTo = "/"): string {
  const safeReturnTo = safeRelativeReturnPath(returnTo);
  return `${SIGN_OUT_PATH}?return_to=${encodeURIComponent(safeReturnTo)}`;
}

/** Prevents open redirects and recursive jumps to auth endpoints. */
function safeRelativeReturnPath(value: string): string {
  if (!value.startsWith("/") || value.startsWith("//")) return "/";

  let url: URL;
  try {
    url = new URL(value, "https://app.local");
  } catch {
    return "/";
  }
  if (url.origin !== "https://app.local") return "/";
  if (isReservedAuthPath(url.pathname)) return "/";

  return `${url.pathname}${url.search}${url.hash}`;
}

function isReservedAuthPath(pathname: string): boolean {
  return (
    pathname === SIGN_IN_PATH ||
    pathname === SIGN_OUT_PATH ||
    pathname === CALLBACK_PATH
  );
}

function safeDecodeURIComponent(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}
