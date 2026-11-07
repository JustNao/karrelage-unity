//! Global keyboard shortcuts (Paramètres « Raccourcis »): the UI sends every binding by name, they are registered with
//! Windows (they work while the game has the focus, and the game does not get them) and `shortcuts:pressed` {name} is sent
//! when one is pressed, `shortcuts:released` {name} when its key is let go (shortcuts held down: `held` {name} tells whether it
//! still is). `shortcuts:status` {name, ok}: false = another program already holds that combination.
//! Passive shortcuts (`passive`: the class help's Eliotrope portal and Exil keys, the game's own spell keys): watched by polling, the
//! game still gets them; `shortcuts:pressed` {name, passive: true} when pressed while the game has the focus.
//! Mouse buttons (middle, « Souris 4 / 5 »: `sys::MOUSE_BUTTONS`) can't be registered: such shortcuts are polled too, pressed
//! while the game or this app has the focus (the click still reaches them), and act like the others (pressed / released / held).
use crate::kit::unknown;
use crate::sys::{Hotkeys, MOUSE_BUTTONS};
use crate::{Ctx, Interest, Module};
use anyhow::Result;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub struct Shortcuts {
    /// Started at the first `set` (no thread for a session that binds nothing, nor in tests).
    keys: OnceLock<Option<Hotkeys>>,
    /// Shortcut names: the index is the hotkey's number.
    names: Arc<Mutex<Vec<String>>>,
    /// Shortcuts whose key is down.
    held: Arc<Mutex<HashSet<String>>>,
    /// Passive shortcuts: names and (mods, vk), in the same order.
    passive_names: Arc<Mutex<Vec<String>>>,
    passive: Arc<Mutex<Vec<(u32, u32)>>>,
    /// The passive shortcuts' polling thread runs.
    watching: Arc<AtomicBool>,
    /// Shortcuts on a mouse button: name -> (mods, vk); the polled list and its names, in the same order; its thread runs.
    mouse_binds: Mutex<BTreeMap<String, (u32, u32)>>,
    mouse_names: Arc<Mutex<Vec<String>>>,
    mouse: Arc<Mutex<Vec<(u32, u32)>>>,
    mouse_watching: Arc<AtomicBool>,
}

impl Shortcuts {
    pub fn new() -> Self {
        Self::default()
    }

    /// The mouse button shortcuts polled (thread started when there are some, it ends by itself once there are none).
    fn watch_mouse(&self, ctx: &Ctx) {
        let binds = self.mouse_binds.lock().clone();
        // names first: the thread reads the bindings, then their names
        *self.mouse_names.lock() = binds.keys().cloned().collect();
        *self.mouse.lock() = binds.values().copied().collect();
        if binds.is_empty() || self.mouse_watching.swap(true, Ordering::Relaxed) {
            return;
        }
        let (emit, names, held) = (ctx.emitter(), self.mouse_names.clone(), self.held.clone());
        crate::sys::watch_bindings(self.mouse.clone(), self.mouse_watching.clone(), crate::sys::game_or_own_focused, move |i, down| {
            let Some(name) = names.lock().get(i).cloned() else { return };
            if down {
                held.lock().insert(name.clone());
                emit("shortcuts:pressed", json!({"name": name}));
            } else {
                held.lock().remove(&name);
                emit("shortcuts:released", json!({"name": name}));
            }
        });
    }
}

impl Module for Shortcuts {
    fn id(&self) -> &'static str {
        "shortcuts"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[])
    }

    fn needs_decoded(&self) -> bool {
        false
    }

    /// `set` {binds: {name: {mods, vk} | null}}: (re)binds each named shortcut (`mods` = Alt 1 | Ctrl 2 | Shift 4 | Win 8,
    /// `vk` = Windows virtual key code; null or vk 0 = unbound). `held` {name}: whether its key is down. `passive` {binds}: the
    /// passive shortcuts, all of them (same format; none = no polling).
    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "set" => {
                let keys = self.keys.get_or_init(|| {
                    let (emit, emit2) = (ctx.emitter(), ctx.emitter());
                    let (names, names2, held) = (self.names.clone(), self.names.clone(), self.held.clone());
                    Hotkeys::start(
                        move |n, down| {
                            let Some(name) = names.lock().get(n as usize).cloned() else { return };
                            if down {
                                held.lock().insert(name.clone());
                                emit("shortcuts:pressed", json!({"name": name}));
                            } else {
                                held.lock().remove(&name);
                                emit("shortcuts:released", json!({"name": name}));
                            }
                        },
                        move |n, ok| {
                            if let Some(name) = names2.lock().get(n as usize).cloned() {
                                emit2("shortcuts:status", json!({"name": name, "ok": ok}));
                            }
                        },
                    )
                });
                let Some(keys) = keys else { return Ok(Value::Null) };
                for (name, b) in args["binds"].as_object().into_iter().flatten() {
                    let n = {
                        let mut names = self.names.lock();
                        names.iter().position(|x| x == name).unwrap_or_else(|| {
                            names.push(name.clone());
                            names.len() - 1
                        })
                    };
                    let (mods, vk) = (b["mods"].as_u64().unwrap_or(0) as u32, b["vk"].as_u64().unwrap_or(0) as u32);
                    let mut mouse = self.mouse_binds.lock();
                    if MOUSE_BUTTONS.contains(&vk) {
                        mouse.insert(name.clone(), (mods, vk));
                        keys.set(n as i32, 0, 0); // (frees a key it held, tells it is fine)
                    } else {
                        mouse.remove(name);
                        keys.set(n as i32, mods, vk);
                    }
                }
                self.watch_mouse(ctx);
                Ok(Value::Null)
            }
            "passive" => {
                let binds: Vec<(String, u32, u32)> = args["binds"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(n, b)| (n.clone(), b["mods"].as_u64().unwrap_or(0) as u32, b["vk"].as_u64().unwrap_or(0) as u32))
                    .filter(|(_, _, vk)| *vk != 0)
                    .collect();
                // names first: the thread reads the bindings, then their names
                *self.passive_names.lock() = binds.iter().map(|(n, ..)| n.clone()).collect();
                *self.passive.lock() = binds.iter().map(|(_, m, vk)| (*m, *vk)).collect();
                if !binds.is_empty() && !self.watching.swap(true, Ordering::Relaxed) {
                    let (emit, names) = (ctx.emitter(), self.passive_names.clone());
                    crate::sys::watch_bindings(self.passive.clone(), self.watching.clone(), crate::sys::game_focused, move |i, down| {
                        if let Some(name) = names.lock().get(i).cloned().filter(|_| down) {
                            emit("shortcuts:pressed", json!({"name": name, "passive": true}));
                        }
                    });
                }
                Ok(Value::Null)
            }
            "held" => Ok(json!(self.held.lock().contains(args["name"].as_str().unwrap_or("")))),
            _ => Err(unknown("shortcuts", method)),
        }
    }
}
