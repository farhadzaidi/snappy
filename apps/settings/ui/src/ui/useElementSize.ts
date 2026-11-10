import { useLayoutEffect, useState, type RefObject } from "react";

import type { Size } from "../core/model";

/** The element's content size, kept current as it resizes. */
export function useElementSize(element: RefObject<HTMLElement | null>): Size {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });

  useLayoutEffect(() => {
    const target = element.current;
    if (!target) {
      return;
    }
    const measure = () => setSize({ width: target.clientWidth, height: target.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(target);
    return () => observer.disconnect();
  }, [element]);

  return size;
}
