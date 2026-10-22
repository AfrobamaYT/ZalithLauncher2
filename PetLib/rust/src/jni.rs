//! Android JNI 桥，`com.movtery.petlib.PetLibNative` 的原生实现。

use std::panic::{catch_unwind, AssertUnwindSafe};

use jni::objects::{JByteArray, JByteBuffer, JClass, JString};
use jni::sys::{jboolean, jfloat, jint, jlong};
use jni::JNIEnv;

use crate::animation::AnimationId;
use crate::api::{LightingParams, PetModel};
use crate::skin::ArmModel;

/// 臂型编码
/// -1 自动检测，0 经典，1 纤细
fn arm_from(code: jint) -> Option<ArmModel> {
    match code {
        0 => Some(ArmModel::Classic),
        1 => Some(ArmModel::Slim),
        _ => None,
    }
}

/// 动画编码
/// 0 待机，1 行走
fn animation_from(code: jint) -> Option<AnimationId> {
    match code {
        0 => Some(AnimationId::Idle),
        1 => Some(AnimationId::Walking),
        _ => None,
    }
}

/// # Safety
/// `ptr` 必须是此前 `nativeCreate` 返回且尚未销毁的句柄，或为 0。
unsafe fn model_from(ptr: jlong) -> Option<&'static mut PetModel> {
    if ptr == 0 {
        None
    } else {
        Some(&mut *(ptr as *mut PetModel))
    }
}

fn read_bytes(env: &mut JNIEnv, array: &JByteArray) -> Option<Vec<u8>> {
    if array.is_null() {
        return None;
    }
    let len = env.get_array_length(array).ok()? as usize;
    let mut buf = vec![0u8; len];
    // SAFETY: jbyte 与 u8 尺寸对齐，长度取自数组本身。
    let as_jbyte =
        unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut jni::sys::jbyte, len) };
    env.get_byte_array_region(array, 0, as_jbyte).ok()?;
    Some(buf)
}

/// panic 兜底：渲染线程的恐慌不允许穿越 JNI 边界，失败时返回类型默认值。
fn guard<R: Default>(f: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_default()
}

/// 以 `IllegalArgumentException` 抛出消息；抛出失败时静默。
fn throw(env: &mut JNIEnv, message: &str) {
    let _ = env.throw_new("java/lang/IllegalArgumentException", message);
}

fn error_or_null<'local>(env: &mut JNIEnv<'local>, error: Option<String>) -> JString<'local> {
    match error {
        Some(message) => env
            .new_string(message.as_str())
            .unwrap_or_else(|_| env.new_string("Failed to apply.").unwrap()),
        None => JString::default(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeCreate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    skin: JByteArray,
    arm: jint,
) -> jlong {
    guard(|| {
        let Some(bytes) = read_bytes(&mut env, &skin) else {
            throw(&mut env, "Skin data is required.");
            return 0;
        };
        match PetModel::new(&bytes, arm_from(arm)) {
            Ok(model) => Box::into_raw(Box::new(model)) as jlong,
            Err(e) => {
                throw(&mut env, &e);
                0
            }
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    guard(|| {
        // SAFETY: 宿主约定句柄只销毁一次。
        if ptr != 0 {
            drop(unsafe { Box::from_raw(ptr as *mut PetModel) });
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetSkin<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    ptr: jlong,
    bytes: JByteArray,
    arm: jint,
) -> JString<'local> {
    let error = guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        let Some(model) = (unsafe { model_from(ptr) }) else {
            return Some("Invalid model handle.".to_string());
        };
        let Some(data) = read_bytes(&mut env, &bytes) else {
            return Some("Skin data is required.".to_string());
        };
        model.set_skin(&data, arm_from(arm)).err()
    });
    error_or_null(&mut env, error)
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetCape<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    ptr: jlong,
    bytes: JByteArray,
) -> JString<'local> {
    let error = guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        let Some(model) = (unsafe { model_from(ptr) }) else {
            return Some("Invalid model handle.".to_string());
        };
        let data = read_bytes(&mut env, &bytes);
        model.set_cape(data.as_deref()).err()
    });
    error_or_null(&mut env, error)
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetLighting(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    azimuth: jfloat,
    elevation: jfloat,
    brightness: jfloat,
    softness: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_lighting(LightingParams {
                azimuth,
                elevation,
                brightness,
                softness,
            });
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetSuperSample(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    value: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_super_sample(value);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetOuterLayer(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    enabled: jboolean,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_outer_layer(enabled != 0);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetTau(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    gaze: jfloat,
    yaw: jfloat,
    anim: jfloat,
    recenter: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_tau(gaze, yaw, anim, recenter);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetHeadRange(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_head_range(x, y);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetSquashDepth(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    depth: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_squash_depth(depth);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetGaze(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    has_target: jboolean,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_gaze(if has_target != 0 { Some((x, y)) } else { None });
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSetYaw(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    yaw: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.set_model_yaw(yaw);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeGetYaw(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jfloat {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        unsafe { model_from(ptr) }.map_or(0., |model| model.model_yaw())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativePlayAnimation(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    id: jint,
    speed: jfloat,
    head_follow: jboolean,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let (Some(model), Some(id)) = (unsafe { model_from(ptr) }, animation_from(id)) {
            model.play_animation(id, speed, head_follow != 0);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeStopAnimation(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.stop_animation();
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeSquashTap(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    now: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.squash_tap(now);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativePlaySwing(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    now: jfloat,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.play_swing(now);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativePlayInteract(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        if let Some(model) = unsafe { model_from(ptr) } {
            model.play_interact();
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeRender<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    ptr: jlong,
    width: jint,
    height: jint,
    now: jfloat,
    dt: jfloat,
    buffer: JByteBuffer,
) {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        let Some(model) = (unsafe { model_from(ptr) }) else {
            return;
        };
        // SAFETY: 指针与容量来自同一个 direct buffer，生命周期由宿主保证
        // 覆盖本次调用；写入仅发生在 render 内部。
        let Ok(address) = env.get_direct_buffer_address(&buffer) else {
            throw(&mut env, "A direct ByteBuffer is required.");
            return;
        };
        let Ok(capacity) = env.get_direct_buffer_capacity(&buffer) else {
            throw(&mut env, "A direct ByteBuffer is required.");
            return;
        };
        // SAFETY: address 有效且可写 capacity 字节。
        let out = unsafe { std::slice::from_raw_parts_mut(address, capacity as usize) };
        if let Err(e) = model.render(width as usize, height as usize, now, dt, out) {
            throw(&mut env, &e);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_com_movtery_petlib_PetLibNative_nativeHitTest(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
) -> jboolean {
    guard(|| {
        // SAFETY: 句柄由宿主渲染线程独占使用。
        unsafe { model_from(ptr) }.is_some_and(|model| model.hit_test(x, y)) as jboolean
    })
}
