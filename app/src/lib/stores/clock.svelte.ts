/**
 * A clock ticking every minute, for "il y a 5min" texts: reading `minute.now` (unix seconds) in a template re-renders it
 * each minute. The timer only runs while something reads it.
 *
 *   {ago(item.updated, minute.now)}
 */
import { createSubscriber } from "svelte/reactivity";

class Clock {
  private sub: () => void;
  constructor(ms: number) {
    this.sub = createSubscriber((update) => {
      const id = setInterval(update, ms);
      return () => clearInterval(id);
    });
  }
  get now() {
    this.sub();
    return Date.now() / 1000;
  }
}

export const minute = new Clock(60_000);
