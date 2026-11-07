// The one Node API the unit-test setup uses.
declare module "node:fs" {
  export function readFileSync(path: URL): Uint8Array<ArrayBuffer>;
}
