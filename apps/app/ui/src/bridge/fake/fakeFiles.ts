// In-memory files for `pnpm dev` and the tests, standing in for the file pickers and the
// shell's reading and writing of the chosen file.

import type { SettingsFiles } from "../bridge";
import { EXPORT_FILE_NAME, UNREADABLE_FILE, UNWRITABLE_FILE } from "../bridge";

/** Where both fake pickers point. */
export const FAKE_SETTINGS_PATH = `/Users/snappy/${EXPORT_FILE_NAME}`;

export class FakeFiles implements SettingsFiles {
  private readonly contents = new Map<string, string>();
  /** Paths that can't be read or written, with the shell's message for each. */
  private readonly failures = new Map<string, string>();

  chooseImportPath(): Promise<string | null> {
    return Promise.resolve(FAKE_SETTINGS_PATH);
  }

  chooseExportPath(): Promise<string | null> {
    return Promise.resolve(FAKE_SETTINGS_PATH);
  }

  read(path: string): Promise<string> {
    const failure = this.failures.get(path);
    const text = this.contents.get(path);
    if (failure !== undefined) {
      return Promise.reject(failure);
    }
    return text === undefined ? Promise.reject(UNREADABLE_FILE) : Promise.resolve(text);
  }

  write(path: string, contents: string): Promise<void> {
    if (this.failures.has(path)) {
      return Promise.reject(UNWRITABLE_FILE);
    }
    this.contents.set(path, contents);
    return Promise.resolve();
  }

  /** The text of the file at `path`, or undefined when there's none. */
  textOf(path = FAKE_SETTINGS_PATH): string | undefined {
    return this.contents.get(path);
  }

  /** Put a file at `path`, as if the user had one there. */
  put(contents: string, path = FAKE_SETTINGS_PATH): void {
    this.contents.set(path, contents);
  }

  /** Make reading and writing `path` fail, reading with `message`, as the shell would. */
  fail(message: string, path = FAKE_SETTINGS_PATH): void {
    this.failures.set(path, message);
  }
}
