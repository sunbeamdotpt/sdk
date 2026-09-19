import { ServiceError } from "../core/errors.ts";
import { authSelectors } from "../state/auth.ts";

/**
 * HTTP methods supported by the REST client.
 */
export type RestMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

/**
 * Options for a single REST request.
 */
export interface RestRequestOptions {
  /** Request body (serialized to JSON automatically). */
  body?: unknown;
  /** Additional headers merged with the client's default headers. */
  headers?: Record<string, string>;
  /** Abort signal for cancellation. */
  signal?: AbortSignal;
}

/**
 * Configuration for {@link createRestClient}.
 */
export interface RestClientConfig {
  /** Base URL prepended to every request path. */
  baseUrl: string;
  /** Default headers sent with every request. */
  defaultHeaders?: Record<string, string>;
  /** Custom fetch implementation (defaults to `globalThis.fetch`). */
  fetch?: typeof globalThis.fetch;
  /** Request timeout in milliseconds (default: 30000). */
  timeoutMs?: number;
}

/**
 * A typed REST client returned by {@link createRestClient}.
 */
export interface RestClient {
  /** Perform a GET request. */
  get<T>(path: string, options?: RestRequestOptions): Promise<T>;
  /** Perform a POST request. */
  post<T>(path: string, options?: RestRequestOptions): Promise<T>;
  /** Perform a PUT request. */
  put<T>(path: string, options?: RestRequestOptions): Promise<T>;
  /** Perform a PATCH request. */
  patch<T>(path: string, options?: RestRequestOptions): Promise<T>;
  /** Perform a DELETE request. */
  del<T>(path: string, options?: RestRequestOptions): Promise<T>;
  /** Perform a request with an arbitrary HTTP method. */
  request<T>(method: RestMethod, path: string, options?: RestRequestOptions): Promise<T>;
}

/**
 * Creates a REST client with base URL, default headers, automatic JSON
 * serialization, auth token injection, and {@link ServiceError} normalization.
 *
 * @example
 * ```ts
 * const api = createRestClient({ baseUrl: "https://api.example.com" });
 * const user = await api.get<User>("/users/123");
 * await api.post("/users", { body: { name: "Alice" } });
 * ```
 */
export function createRestClient(config: RestClientConfig): RestClient {
  const {
    baseUrl,
    defaultHeaders = {},
    fetch: fetchImpl = globalThis.fetch,
    timeoutMs = 30_000,
  } = config;

  async function request<T>(
    method: RestMethod,
    path: string,
    options: RestRequestOptions = {},
  ): Promise<T> {
    const url = `${baseUrl.replace(/\/$/, "")}/${path.replace(/^\//, "")}`;

    const token = authSelectors.token();
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json",
      ...defaultHeaders,
      ...options.headers,
    };
    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
    if (options.signal) {
      options.signal.addEventListener("abort", () => controller.abort());
    }

    try {
      const response = await fetchImpl(url, {
        method,
        headers,
        body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const kind = httpStatusToKind(response.status);
        const text = await response.text().catch(() => "Unknown error");
        throw new ServiceError(kind, text);
      }

      const contentType = response.headers.get("content-type") ?? "";
      if (contentType.includes("application/json")) {
        return (await response.json()) as T;
      }
      return (await response.text()) as unknown as T;
    } catch (err) {
      clearTimeout(timeoutId);
      if (err instanceof ServiceError) throw err;
      throw ServiceError.from(err);
    }
  }

  return {
    get: <T>(path: string, opts?: RestRequestOptions) => request<T>("GET", path, opts),
    post: <T>(path: string, opts?: RestRequestOptions) => request<T>("POST", path, opts),
    put: <T>(path: string, opts?: RestRequestOptions) => request<T>("PUT", path, opts),
    patch: <T>(path: string, opts?: RestRequestOptions) => request<T>("PATCH", path, opts),
    del: <T>(path: string, opts?: RestRequestOptions) => request<T>("DELETE", path, opts),
    request,
  };
}

function httpStatusToKind(status: number): ServiceError["kind"] {
  switch (status) {
    case 400:
      return "InvalidArgument";
    case 401:
      return "Unauthenticated";
    case 403:
      return "PermissionDenied";
    case 404:
      return "NotFound";
    case 409:
      return "AlreadyExists";
    case 429:
      return "Unavailable";
    case 500:
      return "Internal";
    case 503:
      return "Unavailable";
    default:
      return status >= 500 ? "Internal" : "InvalidArgument";
  }
}
