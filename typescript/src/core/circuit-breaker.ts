import { ServiceError } from "./errors.ts";

/**
 * State of a circuit breaker: closed (normal), open (failing), or half-open (testing recovery).
 */
export type CircuitState = "closed" | "open" | "half-open";

/**
 * Configuration for circuit breaker behavior.
 */
export interface CircuitBreakerConfig {
  /** Number of consecutive failures before circuit opens */
  failureThreshold: number;
  /** Time in milliseconds before attempting recovery from open state */
  resetMs: number;
  /** Number of allowed attempts while in half-open state */
  halfOpenMaxAttempts?: number;
  /** Clock function for testing (defaults to Date.now) */
  now?: () => number;
}

/**
 * Default circuit breaker configuration: opens after 5 failures, resets after 30 seconds.
 */
export const defaultCircuitConfig: CircuitBreakerConfig = {
  failureThreshold: 5,
  resetMs: 30_000,
  halfOpenMaxAttempts: 1,
};

/**
 * Circuit breaker for detecting and responding to upstream failures.
 * Transitions between closed (normal), open (failing), and half-open (testing recovery) states.
 */
export class CircuitBreaker {
  private state: CircuitState = "closed";
  private failures = 0;
  private openedAt = 0;
  private halfOpenAttempts = 0;
  private readonly config: Required<CircuitBreakerConfig>;

  /**
   * Creates a new circuit breaker.
   * @param config Circuit breaker configuration; uses defaultCircuitConfig if not provided
   */
  constructor(config: CircuitBreakerConfig = defaultCircuitConfig) {
    this.config = {
      failureThreshold: config.failureThreshold,
      resetMs: config.resetMs,
      halfOpenMaxAttempts: config.halfOpenMaxAttempts ?? 1,
      now: config.now ?? Date.now,
    };
  }

  /**
   * Gets the current state, automatically transitioning from open to half-open after resetMs.
   */
  get currentState(): CircuitState {
    if (
      this.state === "open" &&
      this.config.now() - this.openedAt >= this.config.resetMs
    ) {
      this.state = "half-open";
      this.halfOpenAttempts = 0;
    }
    return this.state;
  }

  /**
   * Executes an operation, recording success or failure and updating circuit state.
   * Throws if the circuit is open or has exhausted half-open attempts.
   * @param op The async operation to run
   * @returns The result of the operation
   * @throws ServiceError if the circuit is open or half-open quota is exhausted
   * @example
   * ```ts
   * const breaker = new CircuitBreaker(defaultCircuitConfig);
   * try {
   *   const result = await breaker.run(() => fetchData());
   * } catch (err) {
   *   if (err.kind === "Unavailable") {
   *     // circuit breaker opened
   *   }
   * }
   * ```
   */
  async run<T>(op: () => Promise<T>): Promise<T> {
    const state = this.currentState;
    if (state === "open") {
      throw new ServiceError(
        "Unavailable",
        "circuit breaker open: upstream marked unhealthy",
      );
    }
    if (
      state === "half-open" &&
      this.halfOpenAttempts >= this.config.halfOpenMaxAttempts
    ) {
      throw new ServiceError(
        "Unavailable",
        "circuit breaker half-open: probe in flight",
      );
    }
    if (state === "half-open") this.halfOpenAttempts++;
    try {
      const result = await op();
      this.onSuccess();
      return result;
    } catch (err) {
      this.onFailure();
      throw err;
    }
  }

  /**
   * Records a successful operation, closing the circuit and resetting the failure count.
   */
  private onSuccess(): void {
    this.failures = 0;
    this.state = "closed";
    this.halfOpenAttempts = 0;
  }

  /**
   * Records a failed operation, incrementing failure count and potentially opening the circuit.
   */
  private onFailure(): void {
    this.failures++;
    if (this.failures >= this.config.failureThreshold) {
      this.state = "open";
      this.openedAt = this.config.now();
    }
  }
}
