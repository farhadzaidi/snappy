import { core, type Limits } from "./snappyCore";

let limits: Limits | undefined;

/** The most zones, workspaces, and undo steps `snappy-core` allows, which never change. */
export function coreLimits(): Limits {
  limits ??= core.limits();
  return limits;
}
