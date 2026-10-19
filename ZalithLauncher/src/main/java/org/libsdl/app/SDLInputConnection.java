/*
 * This file is part of SDL3 android-project java code.
 * Licensed under the zlib license: https://www.libsdl.org/license.php
 */

package org.libsdl.app;

import androidx.annotation.Keep;

import android.text.Editable;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.widget.EditText;

/**
 * 输入法文本经编辑器（Editable）与上次转发内容做差量：
 * 删除部分逐字符转发游戏退格，新增部分整体以 SDL_EVENT_TEXT_INPUT 提交。
 * 所有编辑操作先经父类落地到编辑器再差量转发，输入法视图与游戏缓冲区因此保持一致。
 */
@Keep
class SDLInputConnection extends BaseInputConnection
{
    protected EditText mEditText;
    protected String mCommittedText = "";

    SDLInputConnection(View targetView, boolean fullEditor) {
        super(targetView, fullEditor);
        mEditText = new EditText(SDL.getContext());
    }

    @Override
    public Editable getEditable() {
        return mEditText.getEditableText();
    }

    @Override
    public boolean sendKeyEvent(KeyEvent event) {
        //输入法（软键盘）合成的按键事件不经游戏按键转发（会误触游戏绑定）：
        //可打印字符与退格转为编辑器操作，由差量转发产生文本与退格；
        //光标移动会打破“游戏光标位于文本末尾”的差量前提，吞掉；
        //其余非打印按键（含回车）仍作为游戏按键转发
        final int keyCode = event.getKeyCode();
        final boolean softKeyboard = (event.getFlags() & KeyEvent.FLAG_SOFT_KEYBOARD) != 0;
        if (event.getAction() == KeyEvent.ACTION_DOWN) {
            if (softKeyboard && isEditorDeleteKeyCode(keyCode)) {
                if (SdlImeController.isInputAccepted()) {
                    if (keyCode == KeyEvent.KEYCODE_DEL) {
                        deleteSurroundingText(1, 0);
                    } else {
                        deleteSurroundingText(0, 1);
                    }
                }
                return true;
            }
            if (softKeyboard && isCursorKeyCode(keyCode)) {
                return true;
            }
            if (softKeyboard && event.isPrintingKey() && SdlImeController.isInputAccepted()) {
                int unicode = event.getUnicodeChar();
                if (unicode != 0) {
                    commitText(String.valueOf((char) unicode), 1);
                }
                return true;
            }
            SDLActivity.onNativeKeyDown(keyCode);
            return true;
        } else if (event.getAction() == KeyEvent.ACTION_UP) {
            if (softKeyboard && (isEditorDeleteKeyCode(keyCode) || isCursorKeyCode(keyCode) || event.isPrintingKey())) {
                return true;
            }
            SDLActivity.onNativeKeyUp(keyCode);
            return true;
        }
        return true;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        if (!super.commitText(text, newCursorPosition)) {
            return false;
        }
        updateText();
        return true;
    }

    @Override
    public boolean setComposingText(CharSequence text, int newCursorPosition) {
        if (!super.setComposingText(text, newCursorPosition)) {
            return false;
        }
        updateText();
        return true;
    }

    @Override
    public boolean deleteSurroundingText(int beforeLength, int afterLength) {
        // Workaround to capture backspace key. Ref: http://stackoverflow.com/questions>/14560344/android-backspace-in-webview-baseinputconnection
        // and https://bugzilla.libsdl.org/show_bug.cgi?id=2265
        if (beforeLength > 0 && afterLength == 0 && getEditable().length() == 0) {
            //编辑器为空时无处删除（如游戏文本经实体键盘或其他通道输入），直接向游戏转发退格
            while (beforeLength-- > 0) {
                nativeGenerateScancodeForUnichar('\b');
            }
            return true;
        }

        if (!super.deleteSurroundingText(beforeLength, afterLength)) {
            return false;
        }
        updateText();
        return true;
    }

    protected void updateText() {
        final Editable content = getEditable();
        if (content == null) {
            return;
        }

        String text = content.toString();
        int compareLength = Math.min(text.length(), mCommittedText.length());
        int matchLength, offset;

        /* Backspace over characters that are no longer in the string */
        for (matchLength = 0; matchLength < compareLength; ) {
            int codePoint = mCommittedText.codePointAt(matchLength);
            if (codePoint != text.codePointAt(matchLength)) {
                break;
            }
            matchLength += Character.charCount(codePoint);
        }
        /* FIXME: This doesn't handle graphemes, like '🌬️' */
        for (offset = matchLength; offset < mCommittedText.length(); ) {
            int codePoint = mCommittedText.codePointAt(offset);
            nativeGenerateScancodeForUnichar('\b');
            offset += Character.charCount(codePoint);
        }

        if (matchLength < text.length()) {
            String pendingText = text.subSequence(matchLength, text.length()).toString();
            //不为输入法提交的文本合成扫描码按键事件：文本经 nativeCommitText
            //以 SDL_EVENT_TEXT_INPUT 进入游戏，合成按键只会误触游戏绑定
            SDLInputConnection.nativeCommitText(pendingText, 0);
        }
        //先提交文本再处理回车（游戏先收文本再收回车），状态同步不因回车提前跳过
        mCommittedText = text;

        if (matchLength < text.length() && !SDLActivity.dispatchingKeyEvent()) {
            for (offset = matchLength; offset < text.length(); ) {
                int codePoint = text.codePointAt(offset);
                if (codePoint == '\n') {
                    if (SDLActivity.onNativeSoftReturnKey()) {
                        break;
                    }
                }
                offset += Character.charCount(codePoint);
            }
        }
    }

    private static boolean isEditorDeleteKeyCode(int keyCode) {
        return keyCode == KeyEvent.KEYCODE_DEL || keyCode == KeyEvent.KEYCODE_FORWARD_DEL;
    }

    static boolean isCursorKeyCode(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_DPAD_LEFT:
            case KeyEvent.KEYCODE_DPAD_RIGHT:
            case KeyEvent.KEYCODE_DPAD_UP:
            case KeyEvent.KEYCODE_DPAD_DOWN:
            case KeyEvent.KEYCODE_HOME:
            case KeyEvent.KEYCODE_MOVE_END:
            case KeyEvent.KEYCODE_PAGE_UP:
            case KeyEvent.KEYCODE_PAGE_DOWN:
                return true;
            default:
                return false;
        }
    }

    public static native void nativeCommitText(String text, int newCursorPosition);

    public static native void nativeGenerateScancodeForUnichar(char c);
}
