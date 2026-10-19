package com.movtery.petlib

/**
 * 玩家臂型
 */
enum class ArmModel(internal val code: Int) {
    Classic(0),
    Slim(1);

    internal companion object {
        /**
         * @param value 为 null 时表示自动检测类型
         */
        fun codeOf(value: ArmModel?): Int = value?.code ?: -1
    }
}