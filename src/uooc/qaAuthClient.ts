import { createAuthClient } from "better-auth/client";
import { deviceAuthorizationClient } from "better-auth/client/plugins";
import { apiKeyClient } from "@better-auth/api-key/client";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_QA_SERVER_URL } from "./constants";

export { DEFAULT_QA_SERVER_URL };

export function normalizeQaBaseUrl(serverUrl?: string): string {
  const raw = (serverUrl ?? DEFAULT_QA_SERVER_URL).trim();
  return raw.replace(/\/+(?:api(?:\/auth)?)?\/?$/, "") || DEFAULT_QA_SERVER_URL;
}

type QaHttpResponse = {
  status: number;
  statusText: string;
  headers: Record<string, string>;
  body: string;
};

export async function tauriFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url =
    typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
  const method =
    init?.method ||
    (typeof input === "object" && "method" in input && typeof input.method === "string"
      ? input.method
      : "GET");

  const headersObj: Record<string, string> = {};
  if (init?.headers) {
    if (init.headers instanceof Headers) {
      init.headers.forEach((val, key) => {
        headersObj[key] = val;
      });
    } else if (Array.isArray(init.headers)) {
      init.headers.forEach(([k, v]) => {
        headersObj[k] = v;
      });
    } else {
      Object.assign(headersObj, init.headers);
    }
  }

  let bodyStr: string | undefined = undefined;
  if (init?.body) {
    bodyStr = typeof init.body === "string" ? init.body : JSON.stringify(init.body);
  }

  try {
    const res = await invoke<QaHttpResponse>("uooc_qa_http_request", {
      request: {
        url,
        method,
        headers: headersObj,
        body: bodyStr,
      },
    });

    return new Response(res.body, {
      status: res.status,
      statusText: res.statusText,
      headers: new Headers(res.headers),
    });
  } catch {
    // If invoke fails (e.g. In unit tests without Tauri backend), fallback to browser fetch
    return fetch(input, init);
  }
}

export function getQaAuthClient(baseURL?: string, bearerToken?: string) {
  const url = normalizeQaBaseUrl(baseURL);
  return createAuthClient({
    baseURL: url,
    plugins: [deviceAuthorizationClient(), apiKeyClient()],
    fetchOptions: {
      customFetchImpl: tauriFetch,
      auth: bearerToken
        ? {
            type: "Bearer",
            token: () => bearerToken,
          }
        : undefined,
    },
  });
}
