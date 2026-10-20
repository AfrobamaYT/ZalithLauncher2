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

package com.movtery.zalithlauncher.ui.components

import android.content.Context
import com.movtery.petlib.ArmModel
import com.movtery.petlib.PetController

/**
 * 设置皮肤，若皮肤数据传 null，则在 assets 中加载默认皮肤
 * @param skin PNG 皮肤数据
 * @param arm 手臂模型类别，为 null 则自动检测类型
 * @param onError 失败时在渲染线程的回调
 */
fun PetController.setSkinDefaultable(
    context: Context,
    skin: ByteArray?,
    arm: ArmModel?,
    onError: ((String) -> Unit)? = null
) {
    val nonNullSkin = skin ?: context.loadDefaultSkin(arm)
    setSkin(nonNullSkin, arm, onError)
}

private fun Context.loadDefaultSkin(arm: ArmModel?): ByteArray = if (arm == ArmModel.Slim) {
    assets.open("alex.png")
} else {
    assets.open("steve.png")
}.use { it.readBytes() }