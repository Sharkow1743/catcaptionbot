use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::OnceLock;

pub use inventory;

#[derive(Clone, Copy)]
pub struct SendPtr(pub *mut ());
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

// Signature marker types
pub struct RefMut<T: ?Sized + 'static>(PhantomData<T>);
pub struct Ref<T: ?Sized + 'static>(PhantomData<T>);
pub struct Val<T: 'static>(PhantomData<T>);

// Argument wrapper types
pub struct MutArg<'a, T: ?Sized>(pub &'a mut T);
pub struct RefArg<'a, T: ?Sized>(pub &'a T);
pub struct ValArg<T>(pub T);

pub trait EventArg {
    type Key: 'static;
    fn to_raw_ptr(&mut self) -> *mut ();
}

impl<'a, T: 'static> EventArg for MutArg<'a, T> {
    type Key = RefMut<T>;
    fn to_raw_ptr(&mut self) -> *mut () {
        self.0 as *mut T as *mut ()
    }
}

impl<'a, T: 'static> EventArg for RefArg<'a, T> {
    type Key = Ref<T>;
    fn to_raw_ptr(&mut self) -> *mut () {
        self.0 as *const T as *mut ()
    }
}

impl<T: 'static> EventArg for ValArg<T> {
    type Key = Val<T>;
    fn to_raw_ptr(&mut self) -> *mut () {
        &mut self.0 as *mut T as *mut ()
    }
}

// Tuple packing trait
pub trait EventTuple {
    type Signature: 'static;
    fn into_raw_ptr(&mut self) -> *mut ();
}

impl EventTuple for () {
    type Signature = ();
    fn into_raw_ptr(&mut self) -> *mut () {
        std::ptr::null_mut()
    }
}

impl<A: EventArg> EventTuple for (A,) {
    type Signature = (A::Key,);
    fn into_raw_ptr(&mut self) -> *mut () {
        self.0.to_raw_ptr()
    }
}

impl<A: EventArg, B: EventArg> EventTuple for (A, B) {
    type Signature = (A::Key, B::Key);
    fn into_raw_ptr(&mut self) -> *mut () {
        self as *mut _ as *mut ()
    }
}

impl<A: EventArg, B: EventArg, C: EventArg> EventTuple for (A, B, C) {
    type Signature = (A::Key, B::Key, C::Key);
    fn into_raw_ptr(&mut self) -> *mut () {
        self as *mut _ as *mut ()
    }
}

impl<A: EventArg, B: EventArg, C: EventArg, D: EventArg> EventTuple for (A, B, C, D) {
    type Signature = (A::Key, B::Key, C::Key, D::Key);
    fn into_raw_ptr(&mut self) -> *mut () {
        self as *mut _ as *mut ()
    }
}

// --- Handler Registrations (Pure Sync Function Pointers) ---

pub struct EventHandlerRegistration {
    pub name: &'static str,
    pub signature_id: TypeId,
    pub call: fn(SendPtr, SendPtr),
}

pub struct CollectHandlerRegistration {
    pub name: &'static str,
    pub signature_id: TypeId,
    pub call: fn(SendPtr, SendPtr),
}

inventory::collect!(EventHandlerRegistration);
inventory::collect!(CollectHandlerRegistration);

pub struct EventManager {
    handlers: HashMap<&'static str, Vec<EventHandlerRegistration>>,
    collect_handlers: HashMap<&'static str, Vec<CollectHandlerRegistration>>,
}

impl EventManager {
    pub fn from_registry() -> Self {
        let mut mgr = Self {
            handlers: HashMap::new(),
            collect_handlers: HashMap::new(),
        };

        for reg in inventory::iter::<EventHandlerRegistration> {
            mgr.handlers
                .entry(reg.name)
                .or_default()
                .push(EventHandlerRegistration {
                    name: reg.name,
                    signature_id: reg.signature_id,
                    call: reg.call,
                });
        }

        for reg in inventory::iter::<CollectHandlerRegistration> {
            mgr.collect_handlers
                .entry(reg.name)
                .or_default()
                .push(CollectHandlerRegistration {
                    name: reg.name,
                    signature_id: reg.signature_id,
                    call: reg.call,
                });
        }

        mgr
    }

    pub fn global() -> &'static EventManager {
        static GLOBAL: OnceLock<EventManager> = OnceLock::new();
        GLOBAL.get_or_init(EventManager::from_registry)
    }
}

// --- Universal Sync Dispatchers ---

pub fn emit_event<Args: EventTuple, Ret: 'static>(
    name: &str,
    mut initial_ret: Ret,
    mut args: Args,
) -> Ret {
    let sig = TypeId::of::<(Args::Signature, Ret)>();
    let args_ptr = SendPtr(args.into_raw_ptr());
    let ret_ptr = SendPtr(&mut initial_ret as *mut Ret as *mut ());

    let mgr = EventManager::global();
    if let Some(handlers) = mgr.handlers.get(name) {
        for handler in handlers {
            if handler.signature_id == sig {
                (handler.call)(args_ptr, ret_ptr);
            }
        }
    }

    initial_ret
}

pub fn emit_collect_event<Item: 'static, Args: EventTuple>(
    name: &str,
    mut args: Args,
) -> Vec<Item> {
    let sig = TypeId::of::<(Args::Signature, Item)>();
    let mut collected: Vec<Item> = Vec::new();
    let args_ptr = SendPtr(args.into_raw_ptr());
    let vec_ptr = SendPtr(&mut collected as *mut Vec<Item> as *mut ());

    let mgr = EventManager::global();
    if let Some(handlers) = mgr.collect_handlers.get(name) {
        for handler in handlers {
            if handler.signature_id == sig {
                (handler.call)(args_ptr, vec_ptr);
            }
        }
    }

    collected
}

pub trait IntoCollectorItem<T> {
    fn into_collector_item(self) -> Option<T>;
}

impl<T> IntoCollectorItem<T> for Option<T> {
    fn into_collector_item(self) -> Option<T> {
        self
    }
}

impl<T> IntoCollectorItem<T> for T {
    fn into_collector_item(self) -> Option<T> {
        Some(self)
    }
}

// --- Macros ---

#[macro_export]
#[doc(hidden)]
macro_rules! __wrap_arg {
    (&mut $e:expr) => {
        $crate::event::MutArg(&mut $e)
    };
    (&$e:expr) => {
        $crate::event::RefArg(&$e)
    };
    ($e:expr) => {
        $crate::event::ValArg($e)
    };
}

#[macro_export]
macro_rules! on_event {
    // 0 args void
    ($name:literal, || $body:block) => {
        const _: () = {
            fn __handler() $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<((), ())>(),
                    call: |_args_ptr, _ret_ptr| {
                        __handler();
                    },
                }
            }
        };
    };

    // 1 arg void: |a: &mut A|
    ($name:literal, |$arg:ident: &mut $arg_ty:ty| $body:block) => {
        const _: () = {
            fn __handler($arg: &mut $arg_ty) $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<(( $crate::event::RefMut<$arg_ty>, ), ())>(),
                    call: |args_ptr, _ret_ptr| {
                        let val: &mut $arg_ty = unsafe { &mut *(args_ptr.0 as *mut $arg_ty) };
                        __handler(val);
                    },
                }
            }
        };
    };

    // 2 args void: |a: &mut A, b: &mut B|
    ($name:literal, |$a:ident: &mut $a_ty:ty, $b:ident: &mut $b_ty:ty| $body:block) => {
        const _: () = {
            fn __handler($a: &mut $a_ty, $b: &mut $b_ty) $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<(( $crate::event::RefMut<$a_ty>, $crate::event::RefMut<$b_ty> ), ())>(),
                    call: |args_ptr, _ret_ptr| {
                        let tuple: &mut ($crate::event::MutArg<$a_ty>, $crate::event::MutArg<$b_ty>) =
                            unsafe { &mut *(args_ptr.0 as *mut _) };
                        __handler(tuple.0.0, tuple.1.0);
                    },
                }
            }
        };
    };

    // 3 args void: |a: &mut A, b: &mut B, c: C|
    ($name:literal, |$a:ident: &mut $a_ty:ty, $b:ident: &mut $b_ty:ty, $c:ident: $c_ty:ty| $body:block) => {
        const _: () = {
            fn __handler($a: &mut $a_ty, $b: &mut $b_ty, $c: $c_ty) $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<(( $crate::event::RefMut<$a_ty>, $crate::event::RefMut<$b_ty>, $crate::event::Val<$c_ty> ), ())>(),
                    call: |args_ptr, _ret_ptr| {
                        let tuple: &mut ($crate::event::MutArg<$a_ty>, $crate::event::MutArg<$b_ty>, $crate::event::ValArg<$c_ty>) =
                            unsafe { &mut *(args_ptr.0 as *mut _) };
                        __handler(tuple.0.0, tuple.1.0, tuple.2.0.clone());
                    },
                }
            }
        };
    };

    // PIPE: 2 args (Acc, &Arg) -> Ret
    ($name:literal, |$acc:ident: $acc_ty:ty, $arg:ident: &$arg_ty:ty| -> $ret_ty:ty $body:block) => {
        const _: () = {
            fn __handler($acc: $acc_ty, $arg: &$arg_ty) -> $ret_ty $body

            $crate::event::inventory::submit! {
                $crate::event::EventHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<(( $crate::event::Val<$acc_ty>, $crate::event::Ref<$arg_ty> ), $ret_ty)>(),
                    call: |args_ptr, ret_ptr| {
                        let args: &mut ($crate::event::ValArg<$acc_ty>, $crate::event::RefArg<$arg_ty>) =
                            unsafe { &mut *(args_ptr.0 as *mut _) };
                        let ret: &mut $ret_ty = unsafe { &mut *(ret_ptr.0 as *mut $ret_ty) };
                        *ret = __handler(ret.clone(), args.1.0);
                    },
                }
            }
        };
    };
}

#[macro_export]
macro_rules! on_collect {
    ($name:literal, |$arg:ident: &$arg_ty:ty| -> $ret_ty:ty $body:block) => {
        const _: () = {
            fn __handler($arg: &$arg_ty) -> $ret_ty $body

            $crate::event::inventory::submit! {
                $crate::event::CollectHandlerRegistration {
                    name: $name,
                    signature_id: std::any::TypeId::of::<(( $crate::event::Ref<$arg_ty>, ), $ret_ty)>(),
                    call: |args_ptr, vec_ptr| {
                        use $crate::event::IntoCollectorItem;
                        let vec_ref: &mut Vec<$ret_ty> = unsafe { &mut *(vec_ptr.0 as *mut Vec<$ret_ty>) };
                        let arg_ref: &$arg_ty = unsafe { &*(args_ptr.0 as *const $arg_ty) };
                        if let Some(item) = __handler(arg_ref).into_collector_item() {
                            vec_ref.push(item);
                        }
                    },
                }
            }
        };
    };
}

/// Synchronous emission macro (No `.await` required)
#[macro_export]
macro_rules! emit {
    ($name:expr) => {
        $crate::event::emit_event($name, (), ())
    };
    ($name:expr, &mut $a:expr) => {
        $crate::event::emit_event($name, (), ($crate::__wrap_arg!(&mut $a),))
    };
    ($name:expr, &mut $a:expr, &mut $b:expr) => {
        $crate::event::emit_event(
            $name,
            (),
            ($crate::__wrap_arg!(&mut $a), $crate::__wrap_arg!(&mut $b)),
        )
    };
    ($name:expr, &mut $a:expr, &mut $b:expr, $c:expr) => {
        $crate::event::emit_event(
            $name,
            (),
            (
                $crate::__wrap_arg!(&mut $a),
                $crate::__wrap_arg!(&mut $b),
                $crate::__wrap_arg!($c),
            ),
        )
    };
    ($name:expr, $initial:expr, &$arg:expr) => {
        $crate::event::emit_event(
            $name,
            $initial,
            ($crate::__wrap_arg!($initial), $crate::__wrap_arg!(&$arg)),
        )
    };
}

/// Synchronous collector macro (No `.await` required)
#[macro_export]
macro_rules! emit_collect {
    ($name:expr, &$arg:expr) => {
        $crate::event::emit_collect_event($name, ($crate::__wrap_arg!(&$arg),))
    };
    ($name:expr, &$a:expr, &$b:expr) => {
        $crate::event::emit_collect_event(
            $name,
            ($crate::__wrap_arg!(&$a), $crate::__wrap_arg!(&$b)),
        )
    };
}
