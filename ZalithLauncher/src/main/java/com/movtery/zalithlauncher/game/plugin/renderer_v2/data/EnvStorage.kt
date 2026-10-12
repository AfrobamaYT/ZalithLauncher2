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

package com.movtery.zalithlauncher.game.plugin.renderer_v2.data

import com.movtery.zalithlauncher.game.version.installed.VersionConfig

/**
 * 环境变量存储接口，支持全局存储（MMKV）与版本独立存储（VersionConfig）
 */
interface EnvStorage {
    fun getString(key: String, defaultValue: String): String
    fun putString(key: String, value: String)
    fun getBoolean(key: String, defaultValue: Boolean): Boolean
    fun putBoolean(key: String, value: Boolean)
    fun containsKey(key: String): Boolean
}

/**
 * 全局环境变量存储（基于 MMKV）
 */
object GlobalEnvStorage : EnvStorage {
    private val mmkv get() = rendererEnvMMKV()

    override fun getString(key: String, defaultValue: String): String =
        mmkv.getString(key, defaultValue) ?: defaultValue

    override fun putString(key: String, value: String) {
        mmkv.putString(key, value).apply()
    }

    override fun getBoolean(key: String, defaultValue: Boolean): Boolean =
        mmkv.getBoolean(key, defaultValue)

    override fun putBoolean(key: String, value: Boolean) {
        mmkv.putBoolean(key, value).apply()
    }

    override fun containsKey(key: String): Boolean = mmkv.containsKey(key)
}

/**
 * 版本独立环境变量存储（基于 VersionConfig）
 * 读写均只针对该版本的 VersionConfig，不继承全局配置
 */
class VersionEnvStorage(
    private val config: VersionConfig,
    private val onSave: (() -> Unit)? = null
) : EnvStorage {
    override fun getString(key: String, defaultValue: String): String {
        val map = config.rendererEnv
        if (map != null && map.containsKey(key)) {
            return map[key] ?: defaultValue
        }
        return defaultValue
    }

    override fun putString(key: String, value: String) {
        val map = config.rendererEnv ?: mutableMapOf<String, String>().also { config.rendererEnv = it }
        map[key] = value
        onSave?.invoke()
    }

    override fun getBoolean(key: String, defaultValue: Boolean): Boolean {
        val map = config.rendererEnv
        if (map != null && map.containsKey(key)) {
            return map[key]?.toBooleanStrictOrNull() ?: defaultValue
        }
        return defaultValue
    }

    override fun putBoolean(key: String, value: Boolean) {
        val map = config.rendererEnv ?: mutableMapOf<String, String>().also { config.rendererEnv = it }
        map[key] = value.toString()
        onSave?.invoke()
    }

    override fun containsKey(key: String): Boolean {
        return config.rendererEnv?.containsKey(key) == true
    }
}
