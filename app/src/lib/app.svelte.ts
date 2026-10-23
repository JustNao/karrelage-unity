import { getStatus, on, type Status } from "./api";

function persisted(key: string, fallback: boolean) {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

/** App-wide reactive state shared by the shell and modules. */
class AppState {
  status = $state<Status>({ ready: false, log: [] });
  dev = $state(persisted("karrelage.dev", false));

  setDev(v: boolean) {
    this.dev = v;
    try {
      localStorage.setItem("karrelage.dev", v ? "1" : "0");
    } catch {}
  }

  /** Capture state the user left (restored at the next start, see App.svelte). */
  capture = $state(persisted("karrelage.capture", false));

  setCapture(v: boolean) {
    this.capture = v;
    try {
      localStorage.setItem("karrelage.capture", v ? "1" : "0");
    } catch {}
  }

  /** Open tab (module id); null = the first one shown. */
  tab = $state<string | null>(null);
  /** A lot to show in the HDV tab (a notification was clicked): the tab opens it, then clears this. */
  hdvFocus = $state<{ gid: number; lot: string } | null>(null);

  /** Open the HDV tab on a lot of an item. */
  showLot(gid: number, lot: string) {
    this.hdvFocus = { gid, lot };
    this.tab = "auction";
  }

  private poll: ReturnType<typeof setInterval> | undefined;

  /** The status is pushed (`app:status`) at startup and when capture is switched; the connections to Dofus.exe are found
   *  by the capture threads without an event, so they are polled, only while capture is on and the window is visible. */
  private apply(s: Status) {
    this.status = s;
    if (s.on && !this.poll)
      this.poll = setInterval(() => !document.hidden && getStatus().then((x) => this.apply(x)).catch(() => {}), 3000);
    else if (!s.on && this.poll) (clearInterval(this.poll), (this.poll = undefined));
  }

  async init() {
    await on<Status>("app:status", (s) => this.apply(s));
    this.apply(await getStatus());
  }
}

export const app = new AppState();
