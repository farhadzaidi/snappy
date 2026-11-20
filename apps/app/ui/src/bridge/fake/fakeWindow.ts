// The shell's window for `pnpm dev` and the tests, which starts hidden as the real one does.

export class FakeWindow {
  /** The page's text when the UI first asked to show the window; null until then. */
  shownText: string | null = null;

  show(): void {
    this.shownText ??= document.body.innerText;
  }
}
