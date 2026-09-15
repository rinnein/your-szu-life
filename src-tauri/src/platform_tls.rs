#[cfg(target_os = "android")]
pub(crate) fn initialize_platform_verifier() -> Result<(), String> {
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::Mutex,
    };

    static INITIALIZED: Mutex<bool> = Mutex::new(false);
    let mut initialized = INITIALIZED
        .lock()
        .map_err(|_| "Android TLS 初始化锁不可用".to_owned())?;
    if *initialized {
        return Ok(());
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let context = ndk_context::android_context();
        let vm = unsafe { jni022::JavaVM::from_raw(context.vm().cast()) };
        vm.attach_current_thread(|environment| {
            let context_object = unsafe {
                jni022::objects::JObject::from_raw(environment, context.context().cast())
            };
            rustls_platform_verifier::android::init_with_env(environment, context_object)
        })
        .map_err(|error: jni022::errors::Error| error.to_string())
    }))
    .unwrap_or_else(|_| Err("Android TLS 验证器初始化异常".to_owned()));
    match result {
        Ok(()) => {
            *initialized = true;
            Ok(())
        }
        Err(error) => Err(error),
    }
}
