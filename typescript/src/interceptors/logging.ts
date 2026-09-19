import { type Interceptor } from "@connectrpc/connect";
import { ServiceError } from "../core/errors.ts";

/**
 * Logger interface for structured logging.
 * Supports debug, info, warn, and error levels with optional structured fields.
 */
export interface Logger {
  /**
   * Optional debug-level logging (typically omitted in production).
   */
  debug?(msg: string, fields?: Record<string, unknown>): void;

  /**
   * Info-level logging for normal RPC lifecycle events.
   */
  info(msg: string, fields?: Record<string, unknown>): void;

  /**
   * Warn-level logging for unexpected but recoverable conditions.
   */
  warn(msg: string, fields?: Record<string, unknown>): void;

  /**
   * Error-level logging for RPC failures.
   */
  error(msg: string, fields?: Record<string, unknown>): void;
}

/**
 * Default logger implementation that uses console methods.
 * Suitable for development and debugging; replace with a proper logger in production.
 */
export const consoleLogger: Logger = {
  debug: (m, f) => console.debug(m, f ?? {}),
  info: (m, f) => console.info(m, f ?? {}),
  warn: (m, f) => console.warn(m, f ?? {}),
  error: (m, f) => console.error(m, f ?? {}),
};

/**
 * Options for the logging interceptor.
 * Configures which logger to use and what data to log.
 */
export interface LoggingOptions {
  /**
   * Logger instance (default: consoleLogger).
   */
  logger?: Logger;

  /**
   * Whether to include RPC payload in logs (default: false).
   * Reserved for future use; currently unused.
   */
  includePayload?: boolean;
}

/**
 * Creates a logging interceptor that logs RPC lifecycle and errors.
 *
 * Logs RPC start and completion with service/method names, stream type, and latency.
 * Errors are logged with their kind and message. Uses the provided logger or falls back to console.
 *
 * @param options Configuration for the logger instance.
 * @returns An Interceptor that logs RPC activity.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withLogging({ logger: myPinoLogger }),
 *   ],
 * });
 */
export function withLogging(options: LoggingOptions = {}): Interceptor {
  const logger = options.logger ?? consoleLogger;
  return (next) => async (req) => {
    const start = performance.now();
    const fields: Record<string, unknown> = {
      service: req.service.typeName,
      method: req.method.name,
      stream: req.stream,
    };
    logger.debug?.("rpc.start", fields);
    try {
      const res = await next(req);
      logger.info("rpc.ok", {
        ...fields,
        latencyMs: Math.round(performance.now() - start),
      });
      return res;
    } catch (err) {
      const svc = ServiceError.from(err);
      logger.error("rpc.err", {
        ...fields,
        latencyMs: Math.round(performance.now() - start),
        kind: svc.kind,
        message: svc.message,
      });
      throw err;
    }
  };
}
