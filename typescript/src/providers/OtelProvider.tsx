import { type ReactNode, useEffect } from "react";
import { setupOtel, type OtelSetupOptions } from "../otel/setup.ts";

/**
 * Props for the OtelProvider component.
 * Extends OtelSetupOptions with children to render.
 * @property children - React nodes to render within the provider.
 */
export interface OtelProviderProps extends OtelSetupOptions {
  /** React subtree that should run with OTel initialised. */
  children: ReactNode;
}

/**
 * Provider component that initializes OpenTelemetry tracing on mount.
 * Should be placed high in the component tree to ensure all descendant components are instrumented.
 *
 * @param props - Component props containing OpenTelemetry configuration and children.
 * @returns JSX element rendering children without additional wrapper elements.
 *
 * @example
 * ```tsx
 * <OtelProvider
 *   serviceName="my-app"
 *   otlpUrl="https://otel-collector.example.com"
 *   sampleRatio={0.1}
 * >
 *   <App />
 * </OtelProvider>
 * ```
 */
export function OtelProvider({ children, ...options }: OtelProviderProps): ReactNode {
  useEffect(() => {
    setupOtel(options);
  }, [options.serviceName, options.otlpUrl, options.sampleRatio]);
  return <>{children}</>;
}
