import { useEffect, useRef, useState } from "react";
import { type DescService } from "@bufbuild/protobuf";
import { createClient, type Transport } from "@connectrpc/connect";
import { useTransport } from "../providers/transport-context.tsx";
import { authStore } from "../state/auth.ts";

// ─── Ring-buffer helpers ─────────────────────────────────────────────────────

const DEFAULT_BUFFER_SIZE = 1024;

/**
 * A bounded ring buffer of string IDs for deduplication.
 * When capacity is exceeded the oldest entry is dropped.
 */
class IdRing {
  private readonly cap: number;
  private readonly set: Set<string> = new Set();
  private readonly ring: string[] = [];

  constructor(cap: number) {
    this.cap = cap;
  }

  has(id: string): boolean {
    return this.set.has(id);
  }

  add(id: string): void {
    if (this.set.has(id)) return;
    if (this.ring.length >= this.cap) {
      const oldest = this.ring.shift()!;
      this.set.delete(oldest);
    }
    this.ring.push(id);
    this.set.add(id);
  }
}

// ─── Public types ─────────────────────────────────────────────────────────────

/**
 * Lifecycle status of a server-streaming RPC connection.
 * - `idle`: hook is mounted but `enabled` is false; no connection has been attempted.
 * - `connecting`: first attempt, no prior error.
 * - `open`: stream is active and receiving events.
 * - `reconnecting`: stream lost; a reconnect attempt is in progress.
 * - `closed-by-logout`: connection was aborted because the user logged out or the session expired.
 * - `error`: a non-recoverable error occurred (should not happen under normal backoff).
 */
export type RpcStreamStatus =
  | "idle"
  | "connecting"
  | "open"
  | "reconnecting"
  | "closed-by-logout"
  | "error";

/**
 * Shape of a server-streaming RPC method descriptor as required by useRpcStream.
 * Matches the shape produced by protobuf-es for server-streaming methods.
 *
 * The phantom `_input` / `_output` fields are never set at runtime; they exist
 * solely to bind the type parameters so that TypeScript can propagate the
 * request and event types through {@link UseRpcStreamOptions}.
 */
export interface StreamMethodDescriptor<TInput extends object, TOutput extends object> {
  readonly name: string;
  readonly localName?: string;
  readonly kind: "server_streaming";
  readonly methodKind: "server_streaming";
  readonly input: { readonly typeName: string; readonly _type?: TInput };
  readonly output: { readonly typeName: string; readonly _type?: TOutput };
  /** Back-reference to the owning service, used to build the client. */
  readonly parent: DescService;
}

/**
 * Options for {@link useRpcStream}.
 */
export interface UseRpcStreamOptions<TInput extends object, TEvent extends object> {
  /**
   * The Connect server-streaming RPC method descriptor.
   */
  method: StreamMethodDescriptor<TInput, TEvent>;
  /**
   * Initial request object. The hook will shallow-clone this on each connect/reconnect
   * and inject the `resume_from` field before opening the stream.
   * The original object is never mutated.
   */
  request: TInput & { resume_from?: bigint | number };
  /**
   * Extracts the monotonic sequence number from an event. Used to track the resume token.
   */
  getSeq: (event: TEvent) => bigint | number;
  /**
   * Extracts the stable event ID from an event. Used for per-event deduplication.
   */
  getEventId: (event: TEvent) => string;
  /**
   * Maximum number of events to keep in the `events` array.
   * Oldest events are dropped when the array would exceed this size.
   * Defaults to 1024.
   */
  bufferSize?: number;
  /**
   * When false the stream will not be opened. Useful when the request shape
   * is not yet ready. Defaults to true.
   */
  enabled?: boolean;
  /**
   * Optional transport override. If omitted the hook uses the transport from
   * the nearest `<TransportProvider>` in the React tree.
   */
  transport?: Transport;
  /**
   * Optional headers to send with every request (and every reconnect).
   * Use this to pass e.g. `x-sunbeam-object-id` for keto_dispatch.
   */
  headers?: Record<string, string>;
}

/**
 * Return value of {@link useRpcStream}.
 */
export interface UseRpcStreamResult<TEvent extends object> {
  /** Events in arrival order, bounded by `bufferSize`. */
  events: TEvent[];
  /** The seq of the last event received. `0n` until the first event arrives. */
  lastSeq: bigint | number;
  /** Current lifecycle status of the stream. */
  status: RpcStreamStatus;
  /** The most recent error, or null if none. */
  error: Error | null;
}

// ─── Backoff ─────────────────────────────────────────────────────────────────

const MIN_BACKOFF_MS = 250;
const MAX_BACKOFF_MS = 8_000;

function backoffMs(attempt: number): number {
  return Math.min(MAX_BACKOFF_MS, MIN_BACKOFF_MS * Math.pow(2, attempt));
}

// ─── Hook ────────────────────────────────────────────────────────────────────

/**
 * React hook that wraps a Connect server-streaming RPC with:
 * - Auto-reconnect using the last `nats_seq` as the resume token (exponential backoff, cap 8 s).
 * - Per-event deduplication via a bounded in-memory ID ring (default 1024 entries).
 * - Automatic abort on logout / session expiry.
 * - A bounded `events` array that drops the oldest when `bufferSize` is exceeded.
 *
 * @example
 * ```tsx
 * const { events, status, lastSeq } = useRpcStream({
 *   method: BoardService.method.subscribeBoard,
 *   request: { boardId: "board-123", resumeFrom: 0n },
 *   getSeq: (e) => e.natsSeq,
 *   getEventId: (e) => e.eventId,
 * });
 * ```
 */
export function useRpcStream<TInput extends object, TEvent extends object>(
  opts: UseRpcStreamOptions<TInput, TEvent>,
): UseRpcStreamResult<TEvent> {
  const ctxTransport = useTransport();
  const effectiveTransport = opts.transport ?? ctxTransport;

  // ── Stable refs for values that must not trigger re-renders ──────────────
  // All mutable state that drives reconnect logic lives in refs so that
  // the main effect only runs once on mount (and on enabled/transport changes).
  const optsRef = useRef(opts);
  optsRef.current = opts;

  const transportRef = useRef<Transport>(effectiveTransport);
  transportRef.current = effectiveTransport;

  const lastSeqRef = useRef<bigint | number>(0n);
  const attemptRef = useRef<number>(0);
  const seenIdsRef = useRef<IdRing>(new IdRing(opts.bufferSize ?? DEFAULT_BUFFER_SIZE));
  const abortRef = useRef<AbortController | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const logoutAbortedRef = useRef<boolean>(false);

  // ── React state (drives re-renders) ──────────────────────────────────────
  const [events, setEvents] = useState<TEvent[]>([]);
  const [status, setStatus] = useState<RpcStreamStatus>(
    opts.enabled !== false ? "connecting" : "idle",
  );
  const [error, setError] = useState<Error | null>(null);
  const [lastSeq, setLastSeq] = useState<bigint | number>(0n);

  // ── Main effect ───────────────────────────────────────────────────────────
  useEffect(() => {
    if (optsRef.current.enabled === false) {
      setStatus("idle");
      return;
    }

    // Reset on each mount / enabled toggle.
    logoutAbortedRef.current = false;
    attemptRef.current = 0;
    seenIdsRef.current = new IdRing(
      optsRef.current.bufferSize ?? DEFAULT_BUFFER_SIZE,
    );

    let mounted = true;

    function scheduleReconnect(): void {
      if (!mounted || logoutAbortedRef.current) return;
      const delay = backoffMs(attemptRef.current);
      attemptRef.current += 1;
      timerRef.current = setTimeout(() => {
        if (mounted && !logoutAbortedRef.current) {
          void openStream();
        }
      }, delay);
    }

    async function openStream(): Promise<void> {
      if (!mounted || logoutAbortedRef.current) return;

      const abort = new AbortController();
      abortRef.current = abort;

      setStatus(attemptRef.current === 0 ? "connecting" : "reconnecting");
      setError(null);

      const currentOpts = optsRef.current;
      const currentTransport = transportRef.current;

      try {
        const client = createClient(
          currentOpts.method.parent,
          currentTransport,
        );

        // Derive the client key from localName if present, else camelCase name.
        const localName =
          currentOpts.method.localName ??
          currentOpts.method.name.charAt(0).toLowerCase() +
            currentOpts.method.name.slice(1);

        const callable = (client as unknown as Record<string, unknown>)[
          localName
        ] as (
          req: TInput & { resume_from: bigint | number },
          opts: { signal: AbortSignal },
        ) => AsyncIterable<TEvent>;

        // Shallow-clone the request and inject the resume token.
        const req = {
          ...currentOpts.request,
          resume_from: lastSeqRef.current,
        } as TInput & { resume_from: bigint | number };

        const callOpts: { signal: AbortSignal; headers?: Record<string, string> } = {
          signal: abort.signal,
        };
        if (currentOpts.headers) {
          callOpts.headers = currentOpts.headers;
        }
        const iterable = callable(req, callOpts);

        if (!mounted || abort.signal.aborted) return;

        setStatus("open");
        // Reset attempt counter on a successful open.
        attemptRef.current = 0;

        const bufferSize = currentOpts.bufferSize ?? DEFAULT_BUFFER_SIZE;

        for await (const event of iterable) {
          if (!mounted || abort.signal.aborted) break;

          const id = currentOpts.getEventId(event);
          if (seenIdsRef.current.has(id)) continue;
          seenIdsRef.current.add(id);

          const seq = currentOpts.getSeq(event);
          // Only advance the resume token for live events (nats_seq > 0).
          if (seq > 0) {
            lastSeqRef.current = seq;
            setLastSeq(seq);
          }

          setEvents((prev) => {
            const next = [...prev, event];
            return next.length > bufferSize
              ? next.slice(next.length - bufferSize)
              : next;
          });
        }

        // Stream ended gracefully — schedule a reconnect.
        if (mounted && !abort.signal.aborted && !logoutAbortedRef.current) {
          scheduleReconnect();
        }
      } catch (err) {
        if (!mounted || abort.signal.aborted || logoutAbortedRef.current) {
          return;
        }

        const e = err instanceof Error ? err : new Error(String(err));
        setError(e);
        setStatus("reconnecting");
        scheduleReconnect();
      }
    }

    void openStream();

    return () => {
      mounted = false;
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      abortRef.current?.abort();
    };
    // Intentionally narrow deps: we only want to re-run this effect when
    // `enabled` or the effective transport reference changes. Everything else is
    // accessed via stable refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opts.enabled, effectiveTransport]);

  // ── Auth-store subscription ───────────────────────────────────────────────
  useEffect(() => {
    const unsub = authStore.status.onChange(({ value }) => {
      if (value === "anonymous" || value === "expired") {
        logoutAbortedRef.current = true;
        if (timerRef.current !== null) {
          clearTimeout(timerRef.current);
          timerRef.current = null;
        }
        abortRef.current?.abort();
        setStatus("closed-by-logout");
      }
    });
    return () => {
      unsub();
    };
  }, []);

  return { events, lastSeq, status, error };
}
