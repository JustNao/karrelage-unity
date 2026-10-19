/** Runs `solve` off the UI thread: receives {build, info, lots (slot -> lots), targets}, posts {progress} then {done}. */
import { solve, type Build, type Info, type Lot } from "./stuff";

self.onmessage = (e: MessageEvent<{ build: Build; info: Info; lots: Record<string, Lot[]>; targets: Record<string, number> }>) => {
  const { build, info, lots, targets } = e.data;
  let last = 0;
  const result = solve(build, info, (s) => lots[s] ?? [], targets, 4_000_000, (p) => {
    const now = Date.now();
    if (now - last > 80) { last = now; (self as unknown as Worker).postMessage({ progress: p }); }
  });
  (self as unknown as Worker).postMessage({ done: result });
};
