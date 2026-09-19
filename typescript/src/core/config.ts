/**
 * JWT authentication configuration for Connect services.
 */
export interface AuthConfig {
  /** JWKS URL for verifying JWT signatures */
  jwksUrl?: string;
  /** JWT issuer identifier */
  issuer?: string;
  /** Expected JWT audience claim */
  audience?: string;
  /** OAuth client ID for implicit flow */
  clientId?: string;
  /** Path to redirect to when authentication is required */
  loginPath?: string;
}

/**
 * OpenTelemetry observability configuration.
 */
export interface ObservabilityConfig {
  /** OTLP collector HTTP endpoint */
  otlpUrl?: string;
  /** Service name for traces and metrics */
  serviceName: string;
  /** Service version string */
  serviceVersion?: string;
  /** Deployment environment (development, staging, production) */
  environment?: string;
  /** Trace sampling ratio (0.0–1.0); 1.0 samples all traces */
  sampleRatio?: number;
}

/**
 * HTTP transport configuration for Connect services.
 */
export interface TransportConfig {
  /** Base URL for service requests (e.g., https://api.example.com) */
  baseUrl: string;
  /** Whether to use binary Connect protocol instead of JSON */
  useBinaryFormat?: boolean;
  /** Default request timeout in milliseconds */
  defaultTimeoutMs?: number;
  /** Fetch credentials mode (same-origin, include, omit) */
  credentials?: RequestCredentials;
}

/**
 * Retry and circuit breaker limits.
 */
export interface LimitsConfig {
  /** Maximum number of retry attempts */
  maxRetries?: number;
  /** Initial backoff delay in milliseconds */
  retryBaseMs?: number;
  /** Maximum backoff delay in milliseconds */
  retryMaxMs?: number;
  /** Number of failures before circuit breaker opens */
  circuitFailureThreshold?: number;
  /** Time in milliseconds before circuit breaker tries recovery */
  circuitResetMs?: number;
}

/**
 * Complete service configuration combining transport, auth, observability, and limits.
 */
export interface ServiceConfig {
  /** Name of this service */
  serviceName: string;
  /** Version of this service */
  serviceVersion?: string;
  /** Deployment environment */
  environment?: string;
  /** HTTP transport settings */
  transport: TransportConfig;
  /** Authentication settings */
  auth?: AuthConfig;
  /** Observability settings */
  observability?: ObservabilityConfig;
  /** Retry and circuit breaker settings */
  limits?: LimitsConfig;
}

type EnvSource = Record<string, string | undefined>;

const importMetaEnv = (): EnvSource => {
  try {
    return (import.meta as unknown as { env?: EnvSource }).env ?? {};
  } catch {
    return {};
  }
};

const processEnv = (): EnvSource => {
  const proc = (globalThis as unknown as { process?: { env?: EnvSource } }).process;
  if (proc?.env) return proc.env;
  return {};
};

const read = (env: EnvSource, key: string): string | undefined =>
  env[`VITE_${key}`] ?? env[key];

const readNumber = (env: EnvSource, key: string): number | undefined => {
  const raw = read(env, key);
  if (raw === undefined) return undefined;
  const n = Number(raw);
  return Number.isFinite(n) ? n : undefined;
};

/**
 * Options for loadConfig.
 */
export interface LoadOptions {
  /** Environment variables to read from; defaults to import.meta.env and process.env */
  env?: EnvSource;
  /** Default configuration values to use if environment variables are not set */
  defaults?: Partial<ServiceConfig>;
}

/**
 * Loads service configuration from environment variables and defaults.
 * Reads from `VITE_*` prefixed variables first (Vite runtime), then unprefixed variables.
 * All environment keys are optional; missing keys use sensible defaults.
 * @param options Environment source and default overrides
 * @returns A complete ServiceConfig with all required fields populated
 * @example
 * ```ts
 * const config = loadConfig({
 *   env: process.env,
 *   defaults: { serviceName: "my-app", transport: { baseUrl: "/api" } }
 * });
 * ```
 */
export function loadConfig(options: LoadOptions = {}): ServiceConfig {
  const env = options.env ?? { ...processEnv(), ...importMetaEnv() };
  const d = options.defaults ?? {};

  const serviceName =
    read(env, "SUNBEAM_SERVICE_NAME") ?? d.serviceName ?? "sunbeam-app";
  const baseUrl =
    read(env, "SUNBEAM_API_BASE_URL") ?? d.transport?.baseUrl ?? "/api";

  return {
    serviceName,
    serviceVersion:
      read(env, "SUNBEAM_SERVICE_VERSION") ?? d.serviceVersion,
    environment:
      read(env, "SUNBEAM_ENVIRONMENT") ?? d.environment ?? "development",
    transport: {
      baseUrl,
      useBinaryFormat:
        read(env, "SUNBEAM_USE_BINARY")?.toLowerCase() === "true" ||
        d.transport?.useBinaryFormat,
      defaultTimeoutMs:
        readNumber(env, "SUNBEAM_TIMEOUT_MS") ??
        d.transport?.defaultTimeoutMs ??
        30_000,
      credentials: d.transport?.credentials ?? "same-origin",
    },
    auth: {
      jwksUrl: read(env, "SUNBEAM_JWKS_URL") ?? d.auth?.jwksUrl,
      issuer: read(env, "SUNBEAM_ISSUER") ?? d.auth?.issuer,
      audience: read(env, "SUNBEAM_AUDIENCE") ?? d.auth?.audience,
      clientId: read(env, "SUNBEAM_CLIENT_ID") ?? d.auth?.clientId,
      loginPath: d.auth?.loginPath ?? "/login",
    },
    observability: {
      serviceName,
      serviceVersion: d.observability?.serviceVersion,
      environment:
        read(env, "SUNBEAM_ENVIRONMENT") ?? d.observability?.environment,
      otlpUrl: read(env, "SUNBEAM_OTLP_URL") ?? d.observability?.otlpUrl,
      sampleRatio:
        readNumber(env, "SUNBEAM_TRACE_SAMPLE_RATIO") ??
        d.observability?.sampleRatio ??
        1.0,
    },
    limits: {
      maxRetries:
        readNumber(env, "SUNBEAM_MAX_RETRIES") ?? d.limits?.maxRetries ?? 3,
      retryBaseMs:
        readNumber(env, "SUNBEAM_RETRY_BASE_MS") ??
        d.limits?.retryBaseMs ??
        100,
      retryMaxMs:
        readNumber(env, "SUNBEAM_RETRY_MAX_MS") ??
        d.limits?.retryMaxMs ??
        5_000,
      circuitFailureThreshold:
        readNumber(env, "SUNBEAM_CIRCUIT_THRESHOLD") ??
        d.limits?.circuitFailureThreshold ??
        5,
      circuitResetMs:
        readNumber(env, "SUNBEAM_CIRCUIT_RESET_MS") ??
        d.limits?.circuitResetMs ??
        30_000,
    },
  };
}
