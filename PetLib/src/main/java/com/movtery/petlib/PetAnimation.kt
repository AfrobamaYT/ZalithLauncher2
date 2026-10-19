package com.movtery.petlib

/** 循环动画预设 */
enum class PetAnimation(internal val id: Int) {
    /** 待机动画 */
    Idle(0),
    /** 行走动画 */
    Walking(1),
}