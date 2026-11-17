// The Node APIs the unit-test setup, the Playwright configs, and the screenshots use.
declare module "node:fs" {
  export function mkdirSync(path: URL, options: { recursive: true }): void;
  export function readFileSync(path: URL): Uint8Array<ArrayBuffer>;
  export function writeFileSync(path: URL, data: Uint8Array): void;
}

declare const process: { env: Record<string, string | undefined> };
