/**
 * Sound pool shared by every automation rule (Automation tab): a rule picks one by id and plays it at its own volume.
 * Files live in app/public/sounds (served by the app itself, no CSP change). To add one: drop the file there and add a row.
 */
export interface Sound { id: string; name: string; src: string }

export const SOUNDS: Sound[] = [{ id: "loot-coin", name: "Pièces (petit butin)", src: "/sounds/LootCoinSmall.ogg" }];
export const DEFAULT_SOUND = SOUNDS[0].id;

const cache = new Map<string, HTMLAudioElement>();

/** Plays a sound of the pool at `volume` (0-1); an unknown id or a blocked playback is ignored. */
export function playSound(id: string, volume: number) {
  const s = SOUNDS.find((x) => x.id === id);
  if (!s) return;
  let a = cache.get(s.id);
  if (!a) cache.set(s.id, (a = new Audio(s.src)));
  // a fresh element per play, so quick repeats overlap instead of cutting each other
  const p = a.cloneNode() as HTMLAudioElement;
  p.volume = Math.min(1, Math.max(0, volume));
  void p.play().catch(() => {});
}
