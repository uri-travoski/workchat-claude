//! Android glue: JNI entry points used by the Kotlin side (`app.workchat.Native`) and the bridge that turns a
//! `Notice` into a system notification (`Native.notify`). A push wake-up or the keep-alive service can run the engine
//! without the activity (no webview): `wake` fetches what is new and notifies, `start_engine` keeps it connected.

use crate::engine::Engine;
use crate::notify::Notice;
use jni::objects::{GlobalRef, JClass, JString, JValue};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM};
use std::sync::{Arc, OnceLock};

static HOST: OnceLock<(JavaVM, GlobalRef)> = OnceLock::new();
static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();
static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn global_engine() -> Option<Arc<Engine>> {
    ENGINE.get().cloned()
}

pub fn set_global_engine(e: Arc<Engine>) {
    let _ = ENGINE.set(e);
}

/// Posts a notification through Kotlin. Safe to call from any thread.
pub fn post_notice(n: &Notice) {
    let Some((vm, class)) = HOST.get() else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let json = serde_json::to_string(n).unwrap_or_default();
    let Ok(s) = env.new_string(json) else { return };
    // SAFETY: `class` is a global reference to the `Native` class.
    let cls = unsafe { JClass::from_raw(class.as_obj().as_raw()) };
    let _ = env.call_static_method(&cls, "notify", "(Ljava/lang/String;)V", &[JValue::Object(&s)]);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime"))
}

fn ensure_engine(dir: &str, tz_min: i32) -> Option<Arc<Engine>> {
    let e = match ENGINE.get() {
        Some(e) => e.clone(),
        None => {
            let e = Engine::new(std::path::PathBuf::from(dir), Arc::new(|_| {}), runtime().handle().clone()).ok()?;
            let _ = ENGINE.set(e.clone());
            ENGINE.get().cloned().unwrap_or(e)
        }
    };
    e.set_ui_state(false, None, Some(tz_min));
    e.set_notifier(Some(Arc::new(|n| post_notice(&n))));
    Some(e)
}

#[no_mangle]
pub extern "system" fn Java_app_workchat_Native_nativeInit(env: JNIEnv, class: JClass) {
    if HOST.get().is_none() {
        if let (Ok(vm), Ok(c)) = (env.get_java_vm(), env.new_global_ref(&class)) {
            let _ = HOST.set((vm, c));
        }
    }
}

/// A push wake-up: sync once and notify. Returns the number of new events.
#[no_mangle]
pub extern "system" fn Java_app_workchat_Native_wake(mut env: JNIEnv, _class: JClass, dir: JString, tz_min: jint) -> jint {
    let dir: String = env.get_string(&dir).map(Into::into).unwrap_or_default();
    let Some(e) = ensure_engine(&dir, tz_min) else { return -1 };
    let r = runtime().block_on(async { tokio::time::timeout(std::time::Duration::from_secs(25), e.wake_once()).await });
    match r {
        Ok(Ok(n)) => n as jint,
        _ => -1,
    }
}

/// Keep-alive service: run the full connection loops (WebSocket) without the activity.
#[no_mangle]
pub extern "system" fn Java_app_workchat_Native_startEngine(mut env: JNIEnv, _class: JClass, dir: JString, tz_min: jint) {
    let dir: String = env.get_string(&dir).map(Into::into).unwrap_or_default();
    if let Some(e) = ensure_engine(&dir, tz_min) {
        if !e.is_started() {
            runtime().block_on(async { e.start() });
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_app_workchat_Native_stopEngine(_env: JNIEnv, _class: JClass) {
    if let Some(e) = ENGINE.get() {
        // The activity (if any) owns the loops while it is open: only a headless engine is stopped.
        if e.headless() {
            e.stop();
        }
    }
}

/// A new push endpoint or token from the platform (UnifiedPush `onNewEndpoint`, FCM `onNewToken`): tell the server.
#[no_mangle]
pub extern "system" fn Java_app_workchat_Native_endpoint(mut env: JNIEnv, _class: JClass, dir: JString, tz_min: jint, kind: JString, target: JString) {
    let dir: String = env.get_string(&dir).map(Into::into).unwrap_or_default();
    let kind: String = env.get_string(&kind).map(Into::into).unwrap_or_default();
    let target: String = env.get_string(&target).map(Into::into).unwrap_or_default();
    let Some(e) = ensure_engine(&dir, tz_min) else { return };
    runtime().block_on(async {
        if e.load_api().is_ok() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(20), e.register_push(&kind, &target)).await;
        }
    });
}
