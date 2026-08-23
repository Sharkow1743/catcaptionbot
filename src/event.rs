use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};

pub use inventory;

pub type BoxFuture<'a, Output> = Pin<Box<dyn Future<Output = Output> + Send + 'a>>;

pub type StaticHandlerFn = fn(Arc<dyn Any + Send + Sync>) -> BoxFuture<'static, ()>;
pub type StaticMutHandlerFn = for<'a> fn(&'a mut (dyn Any + Send)) -> BoxFuture<'a, ()>;
pub type DynHandler =
    Box<dyn Fn(Arc<dyn Any + Send + Sync>) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

pub struct EventHandlerRegistration {
    pub name: &'static str,
    pub handler: StaticHandlerFn,
}

pub struct MutEventHandlerRegistration {
    pub name: &'static str,
    pub handler: StaticMutHandlerFn,
}

inventory::collect!(EventHandlerRegistration);
inventory::collect!(MutEventHandlerRegistration);

// --- Macros ---

/// Listen to an event anywhere in your project.
///
/// # Examples:
/// ```rust
/// // 1. Zero arguments
/// on_event!("server_start", || { ... });
///
/// // 2. Mutable reference (in-place modification)
/// on_event!("pre_roll", |session: &mut UserSession| { ... });
///
/// // 3. Multiple arguments (no struct required!)
/// on_event!("reward", |user_id: u64, prize: String, amount: u32| { ... });
///
/// // 4. Single argument
/// on_event!("user_left", |user_id: u64| { ... });
/// ```
#[macro_export]
macro_rules! on_event {
    // 1. Zero arguments
    ($name:literal, || $body:block) => {
        const _: () = {
            async fn __handler() $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    handler: |any_data| {
                        Box::pin(async move {
                            if let Some(()) = any_data.downcast_ref::<()>() {
                                __handler().await;
                            }
                        })
                    },
                }
            }
        };
    };

    // 2. Single mutable reference: |session: &mut UserSession|
    ($name:literal, |$arg:ident: &mut $arg_ty:ty| $body:block) => {
        const _: () = {
            async fn __mut_handler($arg: &mut $arg_ty) $body

            $crate::event::inventory::submit! {
                $crate::event::MutEventHandlerRegistration {
                    name: $name,
                    handler: |any_data| {
                        Box::pin(async move {
                            if let Some(val) = any_data.downcast_mut::<$arg_ty>() {
                                __mut_handler(val).await;
                            }
                        })
                    },
                }
            }
        };
    };

    // 3. Multiple arguments: |arg1: Type1, arg2: Type2, ...|
    ($name:literal, |$arg1:ident: $arg1_ty:ty, $arg2:ident: $arg2_ty:ty $(, $rest_arg:ident: $rest_ty:ty)* $(,)?| $body:block) => {
        const _: () = {
            async fn __handler($arg1: $arg1_ty, $arg2: $arg2_ty $(, $rest_arg: $rest_ty)*) $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    handler: |any_data| {
                        Box::pin(async move {
                            if let Some(($arg1, $arg2 $(, $rest_arg)*)) = any_data.downcast_ref::<($arg1_ty, $arg2_ty $(, $rest_ty)*)>() {
                                __handler($arg1.clone(), $arg2.clone() $(, $rest_arg.clone())*).await;
                            }
                        })
                    },
                }
            }
        };
    };

    // 4. Single immutable argument: |arg: Type|
    ($name:literal, |$arg:ident: $arg_ty:ty| $body:block) => {
        const _: () = {
            async fn __handler($arg: $arg_ty) $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    handler: |any_data| {
                        Box::pin(async move {
                            if let Some(val) = any_data.downcast_ref::<$arg_ty>() {
                                __handler(val.clone()).await;
                            }
                        })
                    },
                }
            }
        };
    };
}

/// Emits an event from anywhere.
///
/// # Examples:
/// ```rust
/// // 1. Zero arguments
/// emit!("server_start").await;
///
/// // 2. Mutate in-place
/// emit!("pre_roll", &mut session).await;
///
/// // 3. Multiple arguments directly
/// emit!("reward", user_id, "Legendary Card".to_string(), 500).await;
/// ```
#[macro_export]
macro_rules! emit {
    // Zero arguments
    ($name:expr $(,)?) => {
        $crate::event::emit_raw($name, ())
    };

    // Single mutable argument
    ($name:expr, &mut $val:expr $(,)?) => {
        $crate::event::emit_mut_raw($name, &mut $val)
    };

    // Multiple arguments (2 or more) -> packs into tuple automatically
    ($name:expr, $arg1:expr, $arg2:expr, $($rest:expr),* $(,)?) => {
        $crate::event::emit_raw($name, ($arg1, $arg2, $($rest),*))
    };

    // Single argument
    ($name:expr, $single:expr $(,)?) => {
        $crate::event::emit_raw($name, $single)
    };
}

// --- Event Manager & Raw Dispatchers ---

pub struct EventManager {
    static_handlers: HashMap<&'static str, Vec<StaticHandlerFn>>,
    static_mut_handlers: HashMap<&'static str, Vec<StaticMutHandlerFn>>,
    dynamic_handlers: RwLock<HashMap<String, Vec<DynHandler>>>,
}

impl EventManager {
    pub fn from_registry() -> Self {
        let mut mgr = Self {
            static_handlers: HashMap::new(),
            static_mut_handlers: HashMap::new(),
            dynamic_handlers: RwLock::new(HashMap::new()),
        };

        for reg in inventory::iter::<EventHandlerRegistration> {
            mgr.static_handlers
                .entry(reg.name)
                .or_default()
                .push(reg.handler);
        }

        for reg in inventory::iter::<MutEventHandlerRegistration> {
            mgr.static_mut_handlers
                .entry(reg.name)
                .or_default()
                .push(reg.handler);
        }

        mgr
    }

    pub fn global() -> &'static EventManager {
        static GLOBAL: OnceLock<EventManager> = OnceLock::new();
        GLOBAL.get_or_init(EventManager::from_registry)
    }
}

// Internal functions called by the `emit!` macro
pub async fn emit_raw<T: Send + Sync + 'static>(name: &str, data: T) {
    let shared: Arc<dyn Any + Send + Sync> = Arc::new(data);
    let mgr = EventManager::global();

    if let Some(handlers) = mgr.static_handlers.get(name) {
        for handler in handlers {
            handler(Arc::clone(&shared)).await;
        }
    }

    let dynamic_futures: Vec<_> = {
        let lock = mgr.dynamic_handlers.read().unwrap();
        lock.get(name)
            .map(|handlers| {
                handlers
                    .iter()
                    .map(|h| h(Arc::clone(&shared)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    for fut in dynamic_futures {
        fut.await;
    }
}

pub async fn emit_mut_raw<T: Send + 'static>(name: &str, data: &mut T) {
    let mgr = EventManager::global();
    if let Some(handlers) = mgr.static_mut_handlers.get(name) {
        for handler in handlers {
            let any_ref: &mut (dyn Any + Send) = data;
            handler(any_ref).await;
        }
    }
}
