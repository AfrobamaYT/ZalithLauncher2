//! PetLib 渲染核心：皮肤玩家模型的软件光栅化预览。
//!
//! 移植自 Minecraft-Desktop-Pet（MIT），按 Android 启动器的皮肤预览场景裁剪：
//! 无配置持久化、无窗口/输入平台层，姿态与动画由宿主经 API 驱动。
//! 单线程约定：所有 `&mut self` 接口只允许宿主的渲染线程调用。

pub mod animation;
pub mod api;
pub mod lighting;
pub mod pose;
pub mod render;
pub mod skin;
