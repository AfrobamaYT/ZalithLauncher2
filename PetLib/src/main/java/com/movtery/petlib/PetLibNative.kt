/*
 * Zalith Launcher 2
 * Copyright (C) 2025 MovTery <movtery228@qq.com> and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/gpl-3.0.txt>.
 */

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
    external fun nativePlayInteract(ptr: Long)
    external fun nativeRender(ptr: Long, width: Int, height: Int, now: Float, dt: Float, buffer: ByteBuffer)

    external fun nativeHitTest(ptr: Long, x: Float, y: Float): Boolean
}
