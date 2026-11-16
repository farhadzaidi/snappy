import { beforeEach, describe, expect, it, vi } from "vitest";

import { FakeDaemon } from "../bridge/fake/fakeDaemon";
import { FakeFiles } from "../bridge/fake/fakeFiles";
import { DaemonClient } from "../state/daemonClient";
import { EXPORTED_MESSAGE, exportSettings, importSettings } from "./settingsBackup";

describe("settings backup", () => {
  let daemon: FakeDaemon;
  let files: FakeFiles;
  let client: DaemonClient;

  beforeEach(async () => {
    daemon = new FakeDaemon("macos");
    files = new FakeFiles();
    client = new DaemonClient(daemon);
    client.connect();
    await vi.waitFor(() => expect(client.state.get()).not.toBeNull());
  });

  it("export writes the daemon's text to the chosen file, then says so", async () => {
    await exportSettings(files, client);
    expect(daemon.commands).toEqual([{ kind: "exportSettings" }]);
    expect(JSON.parse(files.textOf() ?? "")).toEqual(client.state.get()?.settings);
    expect(client.toast.get()).toMatchObject({ message: EXPORTED_MESSAGE, isError: false });
  });

  it("a file that can't be written is said, not exported", async () => {
    files.fail("Couldn't read that file.");
    await exportSettings(files, client);
    expect(files.textOf()).toBeUndefined();
    expect(client.toast.get()).toMatchObject({ message: "Couldn't save to that file.", isError: true });
  });

  it("import sends the chosen file's text to the daemon", async () => {
    await exportSettings(files, client);
    await importSettings(files, client);
    expect(daemon.commands.at(-1)).toEqual({ kind: "importSettings", contents: files.textOf() });
  });

  it("a file that can't be read is said, and nothing is sent", async () => {
    await importSettings(files, client);
    expect(daemon.commands).toEqual([]);
    expect(client.toast.get()).toMatchObject({ message: "Couldn't read that file.", isError: true });
  });
});
