import { Code, ConnectError } from "@connectrpc/connect";

/**
 * Categorization of errors that services can produce. Maps to gRPC codes and HTTP status codes.
 */
export type ServiceErrorKind =
  | "InvalidArgument"
  | "NotFound"
  | "AlreadyExists"
  | "PermissionDenied"
  | "Unauthenticated"
  | "Internal"
  | "Unavailable"
  | "Unimplemented"
  | "DeadlineExceeded"
  | "Network"
  | "Serialization"
  | "Configuration";

const KIND_TO_CODE: Record<ServiceErrorKind, Code> = {
  InvalidArgument: Code.InvalidArgument,
  NotFound: Code.NotFound,
  AlreadyExists: Code.AlreadyExists,
  PermissionDenied: Code.PermissionDenied,
  Unauthenticated: Code.Unauthenticated,
  Internal: Code.Internal,
  Unavailable: Code.Unavailable,
  Unimplemented: Code.Unimplemented,
  DeadlineExceeded: Code.DeadlineExceeded,
  Network: Code.Unavailable,
  Serialization: Code.Internal,
  Configuration: Code.Internal,
};

const KIND_TO_HTTP: Record<ServiceErrorKind, number> = {
  InvalidArgument: 400,
  NotFound: 404,
  AlreadyExists: 409,
  PermissionDenied: 403,
  Unauthenticated: 401,
  Internal: 500,
  Unavailable: 503,
  Unimplemented: 501,
  DeadlineExceeded: 504,
  Network: 503,
  Serialization: 500,
  Configuration: 500,
};

/**
 * Represents an error originating from a service call. Stores the error kind,
 * provides access to gRPC and HTTP equivalents, and indicates if the error is retryable.
 */
export class ServiceError extends Error {
  /** Categorical kind that drives retry and transport behavior. */
  readonly kind: ServiceErrorKind;
  /** Wrapped underlying cause (network error, ConnectError, etc.). */
  override readonly cause?: unknown;

  /**
   * Creates a new ServiceError.
   * @param kind The error category (InvalidArgument, NotFound, etc.)
   * @param message A human-readable error message
   * @param cause The underlying error (if any)
   */
  constructor(kind: ServiceErrorKind, message: string, cause?: unknown) {
    super(message);
    this.name = "ServiceError";
    this.kind = kind;
    this.cause = cause;
  }

  /**
   * Gets the gRPC Code equivalent for this error kind.
   */
  get code(): Code {
    return KIND_TO_CODE[this.kind];
  }

  /**
   * Gets the HTTP status code equivalent for this error kind.
   */
  get httpStatus(): number {
    return KIND_TO_HTTP[this.kind];
  }

  /**
   * Determines if this error is safe to retry.
   */
  get retryable(): boolean {
    return (
      this.kind === "Unavailable" ||
      this.kind === "Network" ||
      this.kind === "DeadlineExceeded"
    );
  }

  /**
   * Converts a ConnectError to a ServiceError.
   * @param err The ConnectError to convert
   * @returns A ServiceError with the error kind mapped from the Connect code
   */
  static fromConnect(err: ConnectError): ServiceError {
    return new ServiceError(codeToKind(err.code), err.rawMessage, err);
  }

  /**
   * Converts an unknown error to a ServiceError. Handles ConnectError, AbortError, network errors, and other Error types.
   * @param err The error to convert
   * @returns A ServiceError (or the input if already a ServiceError)
   */
  static from(err: unknown): ServiceError {
    if (err instanceof ServiceError) return err;
    if (err instanceof ConnectError) return ServiceError.fromConnect(err);
    if (err instanceof Error) {
      if (err.name === "AbortError") {
        return new ServiceError("DeadlineExceeded", err.message, err);
      }
      if (err.name === "TypeError" && /fetch|network/i.test(err.message)) {
        return new ServiceError("Network", err.message, err);
      }
      return new ServiceError("Internal", err.message, err);
    }
    return new ServiceError("Internal", String(err), err);
  }
}

/**
 * Maps a gRPC Code to a ServiceErrorKind.
 */
function codeToKind(code: Code): ServiceErrorKind {
  switch (code) {
    case Code.InvalidArgument:
      return "InvalidArgument";
    case Code.NotFound:
      return "NotFound";
    case Code.AlreadyExists:
      return "AlreadyExists";
    case Code.PermissionDenied:
      return "PermissionDenied";
    case Code.Unauthenticated:
      return "Unauthenticated";
    case Code.Unavailable:
      return "Unavailable";
    case Code.Unimplemented:
      return "Unimplemented";
    case Code.DeadlineExceeded:
      return "DeadlineExceeded";
    default:
      return "Internal";
  }
}

/**
 * Type guard that checks if an error is a ServiceError.
 * @param e The error to check
 * @returns True if e is a ServiceError instance
 */
export const isServiceError = (e: unknown): e is ServiceError =>
  e instanceof ServiceError;

/**
 * Options for the isRetryable check.
 */
export interface IsRetryableOptions {
  /**
   * When true, Unauthenticated errors are also considered retryable.
   * Useful when a token refresh hook is in place (default: false).
   */
  retryUnauthenticated?: boolean;
}

/**
 * Checks if an error (of any type) is safe to retry. Converts to ServiceError if needed.
 * @param e The error to check
 * @param options Optional flags; set `retryUnauthenticated` to also retry Unauthenticated errors
 * @returns True if the error kind is retryable (Unavailable, Network, or DeadlineExceeded)
 */
export const isRetryable = (e: unknown, options: IsRetryableOptions = {}): boolean => {
  const err = ServiceError.from(e);
  if (err.retryable) return true;
  return options.retryUnauthenticated === true && err.kind === "Unauthenticated";
};
