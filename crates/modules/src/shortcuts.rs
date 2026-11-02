//! Global keyboard shortcuts (Paramètres « Raccourcis »): the UI sends every binding by name, they are registered with
//! Windows (they work while the game has the focus, and the game does not get them) and `shortcuts:pressed` {name} is sent
//! when one is pressed. `shortcuts:status` {name, ok}: false = another program already holds that combination.
use crate::kit::unknown;
use crate::sys::Hotkeys;
use crate::{Ctx, Interest, Module};
use anyhow::Result;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub struct Shortcuts {
    /// Started at the first `set` (no thread for a session that binds nothing, nor in tests).
    keys: OnceLock<Option<Hotkeys>>,
    /// Shortcut names: the index is the hotkey's number.
    names: Arc<Mutex<Vec<String>>>,
}

impl Shortcuts {
    pub fn new() -> Self {
        Self::default()
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
    /// `vk` = Windows virtual key code; null or vk 0 = unbound).
    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "set" => {
                let keys = self.keys.get_or_init(|| {
                    let (emit, emit2) = (ctx.emitter(), ctx.emitter());
                    let (names, names2) = (self.names.clone(), self.names.clone());
                    let name = move |names: &Mutex<Vec<String>>, n: i32| names.lock().get(n as usize).cloned();
                    Hotkeys::start(
                        move |n| {
                            if let Some(name) = name(&names, n) {
                                emit("shortcuts:pressed", json!({"name": name}));
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
                    keys.set(n as i32, b["mods"].as_u64().unwrap_or(0) as u32, b["vk"].as_u64().unwrap_or(0) as u32);
                }
                Ok(Value::Null)
            }
            _ => Err(unknown("shortcuts", method)),
        }
    }
}
