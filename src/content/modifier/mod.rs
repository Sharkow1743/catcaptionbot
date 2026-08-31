pub mod modifiers {
    pub mod adrenaline;
}

use crate::session::UserSession;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};

pub use inventory;
pub use modifiers::adrenaline::*;

// -----------------------------------------------------------------------------
// HookArg Helper
// -----------------------------------------------------------------------------

pub struct HookArg<'a> {
    pub name: &'a str,
    data: *mut (),
    drop_fn: unsafe fn(*mut ()),
    _marker: PhantomData<&'a mut ()>,
}

impl<'a> HookArg<'a> {
    pub fn new<T>(name: &'a str, value: T) -> Self {
        let boxed = Box::new(value);
        let data = Box::into_raw(boxed) as *mut ();
        unsafe fn drop_box<T>(ptr: *mut ()) {
            drop(unsafe { Box::from_raw(ptr as *mut T) });
        }
        Self {
            name,
            data,
            drop_fn: drop_box::<T>,
            _marker: PhantomData,
        }
    }

    pub fn on<T>(mut self, name: &str, f: impl FnOnce(T) -> T) -> Self {
        if self.name == name {
            let val = unsafe { *Box::from_raw(self.data as *mut T) };
            self.data = std::ptr::null_mut();
            self.drop_fn = |_| {};

            let modified = f(val);

            let boxed = Box::new(modified);
            self.data = Box::into_raw(boxed) as *mut ();
            unsafe fn drop_box<T>(ptr: *mut ()) {
                drop(unsafe { Box::from_raw(ptr as *mut T) });
            }
            self.drop_fn = drop_box::<T>;
        }
        self
    }

    pub fn into_value<T>(mut self) -> T {
        let val = unsafe { *Box::from_raw(self.data as *mut T) };
        self.data = std::ptr::null_mut();
        self.drop_fn = |_| {};
        val
    }
}

impl<'a> Drop for HookArg<'a> {
    fn drop(&mut self) {
        if !self.data.is_null() {
            unsafe {
                (self.drop_fn)(self.data);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Trait
// -----------------------------------------------------------------------------

pub trait Modifier: Default + Serialize + DeserializeOwned + Send + Sync + 'static {
    const ID: &'static str;
    const TRIGGERS: &'static [&'static str] = &[];
    const HOOKS: &'static [&'static str] = &[];

    fn is_active(&self) -> bool {
        false
    }

    fn display(&self) -> String {
        String::new()
    }

    fn on_trigger(&mut self, _name: &str, _session: &mut UserSession) {}
    fn apply_hook<'a>(&self, _session: &UserSession, hook: HookArg<'a>) -> HookArg<'a> {
        hook
    }
}

// -----------------------------------------------------------------------------
// Generic Deserialization & Dispatch Trampolines
// -----------------------------------------------------------------------------

pub fn dispatch_trigger<T: Modifier>(
    name: &str,
    json_val: &mut serde_json::Value,
    session: &mut UserSession,
) {
    let mut instance: T = if json_val.is_null() {
        T::default()
    } else {
        serde_json::from_value(json_val.clone()).unwrap_or_default()
    };

    instance.on_trigger(name, session);

    if let Ok(new_json) = serde_json::to_value(&instance) {
        *json_val = new_json;
    }
}

pub fn dispatch_hook<'a, T: Modifier>(
    json_val: &serde_json::Value,
    session: &UserSession,
    hook: HookArg<'a>,
) -> HookArg<'a> {
    let instance: T = if json_val.is_null() {
        T::default()
    } else {
        serde_json::from_value(json_val.clone()).unwrap_or_default()
    };

    instance.apply_hook(session, hook)
}

pub fn dispatch_is_active<T: Modifier>(json_val: &serde_json::Value) -> bool {
    let instance: T = if json_val.is_null() {
        T::default()
    } else {
        serde_json::from_value(json_val.clone()).unwrap_or_default()
    };

    instance.is_active()
}

pub fn dispatch_display<T: Modifier>(json_val: &serde_json::Value) -> String {
    let instance: T = if json_val.is_null() {
        T::default()
    } else {
        serde_json::from_value(json_val.clone()).unwrap_or_default()
    };

    instance.display()
}

// -----------------------------------------------------------------------------
// Registry Definition
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModifierDefinition {
    pub id: &'static str,
    pub triggers: &'static [&'static str],
    pub hooks: &'static [&'static str],
    pub dispatch_trigger: fn(&str, &mut serde_json::Value, &mut UserSession),
    pub dispatch_hook: for<'a> fn(&serde_json::Value, &UserSession, HookArg<'a>) -> HookArg<'a>,
    pub is_active: fn(&serde_json::Value) -> bool,
    pub display: fn(&serde_json::Value) -> String,
}

inventory::collect!(ModifierDefinition);

#[macro_export]
macro_rules! modifier {
    ($Name:ident) => {
        $crate::content::modifier::inventory::submit! {
            $crate::content::modifier::ModifierDefinition {
                id: <$Name as $crate::content::modifier::Modifier>::ID,
                triggers: <$Name as $crate::content::modifier::Modifier>::TRIGGERS,
                hooks: <$Name as $crate::content::modifier::Modifier>::HOOKS,
                dispatch_trigger: $crate::content::modifier::dispatch_trigger::<$Name>,
                dispatch_hook: $crate::content::modifier::dispatch_hook::<$Name>,
                is_active: $crate::content::modifier::dispatch_is_active::<$Name>,
                display: $crate::content::modifier::dispatch_display::<$Name>,
            }
        }
    };
}

#[derive(Clone, Default)]
pub struct ModifierRegistry {
    modifiers: Vec<ModifierDefinition>,
    pub known_triggers: HashSet<&'static str>,
    pub known_hooks: HashSet<&'static str>,
}

impl ModifierRegistry {
    pub fn from_registry() -> Self {
        let mut reg = Self::default();
        for def in inventory::iter::<ModifierDefinition> {
            reg.known_triggers.extend(def.triggers.iter().copied());
            reg.known_hooks.extend(def.hooks.iter().copied());
            reg.modifiers.push(ModifierDefinition {
                id: def.id,
                triggers: def.triggers,
                hooks: def.hooks,
                dispatch_trigger: def.dispatch_trigger,
                dispatch_hook: def.dispatch_hook,
                is_active: def.is_active,
                display: def.display,
            });
        }
        reg
    }

    pub fn global() -> &'static ModifierRegistry {
        static GLOBAL: OnceLock<ModifierRegistry> = OnceLock::new();
        GLOBAL.get_or_init(ModifierRegistry::from_registry)
    }

    pub fn all(&self) -> &[ModifierDefinition] {
        &self.modifiers
    }

    pub fn get(&self, id: &str) -> Option<&ModifierDefinition> {
        self.modifiers.iter().find(|m| m.id == id)
    }
}

pub fn warn_once(kind: &str, name: &str) {
    static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let warned = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    if warned.lock().unwrap().insert(format!("{kind}:{name}")) {
        log::warn!("Fired {kind} \"{name}\" but no registered modifier listens for it");
    }
}

// -----------------------------------------------------------------------------
// ModifierBag Helpers (Binary-safe with HashMap<String, String>)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModifierBag {
    pub entries: HashMap<String, String>,
}

impl ModifierBag {
    pub fn active_displays(&self) -> Vec<String> {
        let registry = ModifierRegistry::global();
        let mut displays = Vec::new();

        for def in registry.all() {
            let json_str = self
                .entries
                .get(def.id)
                .map(|s| s.as_str())
                .unwrap_or("null");
            let json_val: serde_json::Value =
                serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);

            if (def.is_active)(&json_val) {
                let text = (def.display)(&json_val);
                if !text.is_empty() {
                    displays.push(text);
                }
            }
        }

        displays
    }

    pub fn display_active(&self, separator: &str) -> String {
        self.active_displays().join(separator)
    }

    pub fn is_active(&self, id: &str) -> bool {
        let registry = ModifierRegistry::global();
        let Some(def) = registry.get(id) else {
            return false;
        };

        let json_str = self
            .entries
            .get(def.id)
            .map(|s| s.as_str())
            .unwrap_or("null");
        let json_val: serde_json::Value =
            serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);
        (def.is_active)(&json_val)
    }

    pub fn active_ids(&self) -> Vec<&'static str> {
        let registry = ModifierRegistry::global();
        let mut ids = Vec::new();

        for def in registry.all() {
            let json_str = self
                .entries
                .get(def.id)
                .map(|s| s.as_str())
                .unwrap_or("null");
            let json_val: serde_json::Value =
                serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);

            if (def.is_active)(&json_val) {
                ids.push(def.id);
            }
        }

        ids
    }
}
