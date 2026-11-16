import { useEffect, useRef, type RefObject } from "react";

interface RecordingInput {
  /** Every key pressed while recording, whichever element has focus. */
  onKey: (event: KeyboardEvent) => void;
  /** A press outside the recorder, or the window losing focus. */
  onLeave: () => void;
}

/**
 * While `recording`, keys go to the recorder from the whole window rather than from its
 * focused button: WebKit doesn't focus a button that is clicked, so a recorder that waited
 * for its own key events would never see any. Nothing else in the window sees those keys.
 */
export function useRecordingInput(
  recording: boolean,
  root: RefObject<HTMLElement | null>,
  input: RecordingInput,
): void {
  const latest = useRef(input);
  latest.current = input;

  useEffect(() => {
    if (!recording) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      latest.current.onKey(event);
    };
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) {
        latest.current.onLeave();
      }
    };
    const onBlur = () => latest.current.onLeave();
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [recording, root]);
}
