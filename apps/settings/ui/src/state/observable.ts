import { useSyncExternalStore } from "react";

/** A value that notifies subscribers when it is replaced. */
export class Observable<T> {
  private value: T;
  private readonly listeners = new Set<() => void>();

  constructor(initial: T) {
    this.value = initial;
  }

  readonly get = (): T => this.value;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  set(next: T): void {
    if (next === this.value) {
      return;
    }
    this.value = next;
    this.listeners.forEach((listener) => listener());
  }

  update(change: (current: T) => T): void {
    this.set(change(this.value));
  }
}

export function useObservable<T>(observable: Observable<T>): T {
  return useSyncExternalStore(observable.subscribe, observable.get);
}
