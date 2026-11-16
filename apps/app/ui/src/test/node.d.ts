// The Node APIs the unit-test setup and the Playwright config use.
declare module "node:fs" {
  export function readFileSync(path: URL): Uint8Array<ArrayBuffer>;
}

declare const process: { env: Record<string, string | undefined> };
