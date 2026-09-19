import { type DescService } from "@bufbuild/protobuf";
import { useCallback, useState } from "react";
import { ServiceError } from "../core/errors.ts";
import {
  type RpcInput,
  type RpcOutput,
  type UnaryMethodNames,
  useRpcClient,
} from "./useRpcQuery.ts";

/**
 * Shape of the form state returned by useRpcForm.
 */
export interface RpcFormState<S extends DescService, K extends UnaryMethodNames<S>> {
  /** Whether the form submission is in flight. */
  isPending: boolean;
  /** Error from the last submission, if any. */
  error: ServiceError | null;
  /** Submit the form with the given input. */
  submit: (input: RpcInput<S, K>) => Promise<RpcOutput<S, K>>;
  /** Reset error state. */
  reset: () => void;
}

/**
 * Hook that wires a unary RPC mutation to a simple form submission flow.
 *
 * Manages pending state, error state, and returns a `submit` function
 * that you can call with form data. No external form library required.
 *
 * @example
 * ```tsx
 * const { submit, isPending, error, reset } = useRpcForm(UsersService, "createUser");
 *
 * async function handleSubmit(formData: FormData) {
 *   const name = formData.get("name") as string;
 *   const user = await submit({ name });
 *   console.log("Created", user.id);
 * }
 * ```
 */
export function useRpcForm<
  S extends DescService,
  K extends UnaryMethodNames<S>,
>(service: S, method: K): RpcFormState<S, K> {
  const client = useRpcClient(service);
  const [isPending, setIsPending] = useState(false);
  const [error, setError] = useState<ServiceError | null>(null);

  const submit = useCallback(
    async (input: RpcInput<S, K>): Promise<RpcOutput<S, K>> => {
      setIsPending(true);
      setError(null);
      try {
        const fn = client[method] as unknown as (
          r: RpcInput<S, K>,
        ) => Promise<RpcOutput<S, K>>;
        const result = await fn(input);
        return result;
      } catch (err) {
        const se = ServiceError.from(err);
        setError(se);
        throw se;
      } finally {
        setIsPending(false);
      }
    },
    [client, method],
  );

  const reset = useCallback(() => {
    setError(null);
  }, []);

  return { isPending, error, submit, reset };
}
