package com.movtery.petlib

import java.nio.ByteBuffer

internal object PetLibNative {
    init {
        System.loadLibrary("petlib")
    }

    external fun nativeCreate(skin: ByteArray, arm: Int): Long
    external fun nativeDestroy(ptr: Long)

    external fun nativeSetSkin(ptr: Long, skin: ByteArray, arm: Int): String?
    external fun nativeSetCape(ptr: Long, cape: ByteArray?): String?

    external fun nativeSetLighting(ptr: Long, azimuth: Float, elevation: Float, brightness: Float, softness: Float)

    external fun nativeSetSuperSample(ptr: Long, value: Float)
    external fun nativeSetOuterLayer(ptr: Long, enabled: Boolean)
    external fun nativeSetTau(ptr: Long, gaze: Float, yaw: Float, anim: Float, recenter: Float)
    external fun nativeSetHeadRange(ptr: Long, x: Float, y: Float)
    external fun nativeSetSquashDepth(ptr: Long, depth: Float)
    external fun nativeSetGaze(ptr: Long, x: Float, y: Float, hasTarget: Boolean)
    external fun nativeSetYaw(ptr: Long, yaw: Float)
    external fun nativeGetYaw(ptr: Long): Float
    external fun nativePlayAnimation(ptr: Long, id: Int, speed: Float, headFollow: Boolean)
    external fun nativeStopAnimation(ptr: Long)
    external fun nativeSquashTap(ptr: Long, now: Float)
    external fun nativePlaySwing(ptr: Long, now: Float)
    external fun nativeRender(ptr: Long, width: Int, height: Int, now: Float, dt: Float, buffer: ByteBuffer)

    external fun nativeHitTest(ptr: Long, x: Float, y: Float): Boolean
}
