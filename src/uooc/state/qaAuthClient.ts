import { DEFAULT_QA_SERVER_URL } from "./constants";

export { DEFAULT_QA_SERVER_URL };

export function normalizeQaBaseUrl(serverUrl?: string): string {
  const raw = (serverUrl ?? DEFAULT_QA_SERVER_URL).trim();
  return raw.replace(/\/+(?:api(?:\/auth)?)?\/?$/, "") || DEFAULT_QA_SERVER_URL;
}
