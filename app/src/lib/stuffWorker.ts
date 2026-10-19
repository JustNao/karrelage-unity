/** Runs `solve` off the UI thread: receives {build, info, lots (slot -> lots), targets}, posts {progress} then {done}. */
import { EXO_BASE, FM_STATS, lotStats, solve, type Build, type Info, type Lot } from "./stuff";

self.onmessage = (e: MessageEvent<{ build: Build; info: Info; lots: Record<string, Lot[]>; targets: Record<string, number> }>) => {
  const { build, info, lots, targets } = e.data;
  const post = (m: unknown) => (self as unknown as Worker).postMessage(m);
  const lotsOf = (s: string) => lots[s] ?? [];
  let last = 0;
  const progress = (p: number) => {
    const now = Date.now();
    if (now - last > 80) { last = now; post({ progress: p }); }
  };
  // the build's forgemagie is an expectation, not a hard condition: when no lots can carry it, search again taking it as applied from outside
  let result = solve(build, info, lotsOf, targets, 4_000_000, progress, true);
  if (!result && Object.values(build.fm ?? {}).some((v) => v > 0)) {
    result = solve(build, info, lotsOf, targets, 4_000_000, progress, false);
    if (result) {
      const supplied = new Map<number, number>();
      for (const slot of Object.keys(build.slots)) {
        const uid = result.picks[slot] ?? build.chosen[slot];
        const lot = uid != null ? lotsOf(slot).find((l) => l.uid === uid) : null;
        if (lot) for (const [s, v] of lotStats(lot)) if (s >= EXO_BASE) supplied.set(s, (supplied.get(s) ?? 0) + v);
      }
      result.missingFm = Object.entries(build.fm).flatMap(([code, v]) => {
        const f = FM_STATS[code];
        const miss = f ? v - (supplied.get(EXO_BASE + f.stat) ?? 0) : 0;
        return miss > 0 ? [`+${miss} ${f.label}`] : [];
      });
    }
  }
  post({ done: result });
};
