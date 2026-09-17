import { lazy, Suspense, type ComponentType } from "react";

/** Adapter for OmniRoute's existing Next dynamic imports inside ArtCraft/Vite. */
export default function dynamic<T extends object>(
  load: () => Promise<{ default: ComponentType<T> }>,
  _options?: unknown
) {
  const LazyComponent = lazy(load);
  return function DynamicComponent(props: T) {
    return (
      <Suspense fallback={null}>
        <LazyComponent {...props} />
      </Suspense>
    );
  };
}
