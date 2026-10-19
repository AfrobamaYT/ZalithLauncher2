package com.movtery.petlib

import android.view.InputDevice
import android.view.MotionEvent
import java.util.concurrent.atomic.AtomicReference

/**
 * Activity 事件泵，把真实鼠标的位置喂给当前注册的玩家模型预览
 */
object PetGazeRouter {
    private const val IDLE_RECENTER_NANOS: Long = 10_000_000_000L

    interface Target {
        /** 光标移动，坐标为窗口坐标 */
        fun onCursor(x: Float, y: Float)
        /** 超时未再见鼠标，应清空注视 */
        fun onGazeExpired()
    }

    private val target = AtomicReference<Target?>(null)
    private var lastMouseNanos = 0L

    fun register(newTarget: Target) {
        target.set(newTarget)
    }

    fun unregister(oldTarget: Target) {
        target.compareAndSet(oldTarget, null)
    }

    fun onGenericMotionEvent(event: MotionEvent) {
        if (!event.isMouse()) return
        when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_ENTER,
            MotionEvent.ACTION_HOVER_MOVE,
            MotionEvent.ACTION_SCROLL,
            MotionEvent.ACTION_MOVE -> {
                lastMouseNanos = System.nanoTime()
                target.get()?.onCursor(event.x, event.y)
            }
        }
    }

    fun onTouchEvent(event: MotionEvent) {
        if (event.isMouse()) {
            lastMouseNanos = System.nanoTime()
            return
        }
        // 仅在非鼠标事件到来时检查空闲时长，触发一次后归零
        if (lastMouseNanos != 0L && System.nanoTime() - lastMouseNanos >= IDLE_RECENTER_NANOS) {
            lastMouseNanos = 0L
            target.get()?.onGazeExpired()
        }
    }

    private fun MotionEvent.isMouse(): Boolean {
        for (i in 0 until pointerCount) {
            if (getToolType(i) == MotionEvent.TOOL_TYPE_MOUSE) return true
        }
        return source and InputDevice.SOURCE_MOUSE == InputDevice.SOURCE_MOUSE ||
            source and InputDevice.SOURCE_TOUCHPAD == InputDevice.SOURCE_TOUCHPAD
    }
}
