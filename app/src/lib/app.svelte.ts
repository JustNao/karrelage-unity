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
  dev = $state(persisted("loutil.dev", false));

  setDev(v: boolean) {
    this.dev = v;
    try {
      localStorage.setItem("loutil.dev", v ? "1" : "0");
    } catch {}
  }

  async init() {
    this.status = await getStatus();
    await on<Status>("app:status", (s) => (this.status = s));
    setInterval(async () => (this.status = await getStatus()), 3000);
  }
}

export const app = new AppState();
