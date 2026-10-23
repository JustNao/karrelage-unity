/**
 * The cheapest-combination search of the Stuff tab, run in a web worker (lib/stuffWorker.ts) so the UI stays responsive:
 * progress, cancellation, and the result (`"none"` = no combination meets the minimums). Changing the build sets `result`
 * back to null.
 */
import type { SolveResult } from "../../lib/stuff";

export class Solver {
  result = $state<SolveResult | null | "none">(null);
  solving = $state(false);
  progress = $state(0);
  cancelled = $state(false);
  private worker: Worker | null = null;

  /** `input`: what lib/stuffWorker.ts takes; `ondone` gets a combination found. */
  run(input: unknown, ondone: (r: SolveResult) => void, onerror: (msg: string) => void) {
    this.solving = true;
    this.cancelled = false;
    this.progress = 0;
    this.result = null;
    const w = (this.worker = new Worker(new URL("../../lib/stuffWorker.ts", import.meta.url), { type: "module" }));
    w.onmessage = (e: MessageEvent<{ progress?: number; done?: SolveResult | null }>) => {
      if (e.data.progress != null) this.progress = e.data.progress;
      else if ("done" in e.data) {
        this.result = e.data.done ?? "none";
        if (this.result !== "none") ondone(this.result);
        this.stop();
      }
    };
    w.onerror = (e) => {
      onerror(e.message || "erreur du calcul");
      this.stop();
    };
    w.postMessage(input);
  }

  stop() {
    this.worker?.terminate();
    this.worker = null;
    this.solving = false;
  }

  cancel() {
    this.cancelled = true;
    this.stop();
  }
}
