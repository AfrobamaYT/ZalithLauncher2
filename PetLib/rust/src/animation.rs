//! 循环动画预设的逐帧求值。
//!
//! 全部为正弦周期函数，天然循环。角度为相对于静止姿态的欧拉偏移（弧度），
//! 由骨架层叠加到部件的静止姿态上。左臂/左腿为 side 0（+X）。

use glam::{Vec2, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationId {
    /// 呼吸待机（转写自启动器原 skinview3d 集成的自定义 NewIdle）。
    Idle,
    /// 行走（转写自 skinview3d 的 WalkingAnimation）。
    Walking,
}

/// 单帧动画偏移。
#[derive(Clone, Copy, Default)]
pub struct AnimFrame {
    /// 躯干（相对髋部）欧拉偏移。
    pub torso: Vec3,
    /// 头部摆动欧拉偏移。
    pub head_sway: Vec3,
    /// 双臂欧拉偏移 [左, 右]。
    pub arms: [Vec3; 2],
    /// 双腿欧拉偏移 [左, 右]。
    pub legs: [Vec3; 2],
    /// 上半身（躯干组）竖直位移，模型像素单位。
    pub bob: f32,
    /// 披风欧拉偏移 (x, z)。
    pub cape: Vec2,
}

/// t 为累计动画时间（秒，已含速度缩放）。
pub fn evaluate(id: AnimationId, t: f32) -> AnimFrame {
    match id {
        AnimationId::Idle => idle(t),
        AnimationId::Walking => walking(t * 8.),
    }
}

/// 待机动画，参考 Modrinth 启动器的呼吸待机动画。
fn idle(t: f32) -> AnimFrame {
    let wave1 = (t * std::f32::consts::TAU * 0.3).sin();
    let wave2 = (t * std::f32::consts::TAU * 0.25).sin() * 0.3;
    let normalized = ((wave1 + wave2 + 1.3) / 2.6).clamp(0., 1.);
    let arm_x = (t * 2.2).sin() * 0.05;
    // 原实现：rightArm.z = armZ（负值），leftArm.z = -armZ，双臂对称外张。
    let arm_z = ((t * 1.8).sin() + 1.) / 2. * -0.05;
    AnimFrame {
        torso: Vec3::new((t * 2.2).sin() * 0.02, 0., (t * 1.5).sin() * 0.01),
        head_sway: Vec3::new(
            (t * 2.2).sin() * 0.04,
            (t * 0.6).sin() * 0.1,
            (t * 1.5).sin() * 0.01,
        ),
        arms: [Vec3::new(arm_x, 0., -arm_z), Vec3::new(arm_x, 0., arm_z)],
        legs: [Vec3::ZERO; 2],
        bob: (t * 2.2).sin() * -0.1,
        cape: Vec2::new(
            0.15 + normalized * 0.15,
            0.03 + ((t * std::f32::consts::TAU * 0.35).sin() + 1.) / 2. * 0.03,
        ),
    }
}

/// WalkingAnimation：t = progress * 8。
fn walking(t: f32) -> AnimFrame {
    const ARM_Z: f32 = std::f32::consts::PI * 0.02;
    AnimFrame {
        torso: Vec3::ZERO,
        head_sway: Vec3::new((t / 5.).sin() * 0.1, (t / 4.).sin() * 0.2, 0.),
        arms: [
            Vec3::new((t + std::f32::consts::PI).sin() * 0.5, 0., t.cos() * 0.03 + ARM_Z),
            Vec3::new(
                t.sin() * 0.5,
                0.,
                (t + std::f32::consts::PI).cos() * 0.03 - ARM_Z,
            ),
        ],
        legs: [
            Vec3::new(t.sin() * 0.5, 0., 0.),
            Vec3::new((t + std::f32::consts::PI).sin() * 0.5, 0., 0.),
        ],
        bob: 0.,
        cape: Vec2::new((t / 1.5).sin() * 0.06 + std::f32::consts::PI * 0.06, 0.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_is_periodic() {
        let period = std::f32::consts::TAU / 8.;
        let a = evaluate(AnimationId::Walking, 13.7);
        let b = evaluate(AnimationId::Walking, 13.7 + period);
        assert!(a.arms[0].abs_diff_eq(b.arms[0], 0.0001));
        assert!(a.legs[1].abs_diff_eq(b.legs[1], 0.0001));
    }

    #[test]
    fn idle_offsets_stay_small() {
        for i in 0..600 {
            let f = evaluate(AnimationId::Idle, i as f32 / 60.);
            assert!(f.head_sway.x.abs() <= 0.05);
            assert!(f.head_sway.y.abs() <= 0.11);
            assert!(f.bob.abs() <= 0.11);
            assert!(f.cape.x >= 0.15 - 0.001 && f.cape.x <= 0.31);
        }
    }

    #[test]
    fn walking_arms_oppose_legs() {
        // t = π/2 时左腿前摆最大，左臂应后摆（反相）。
        let f = evaluate(AnimationId::Walking, std::f32::consts::FRAC_PI_2 / 8.);
        assert!(f.legs[0].x > 0.4);
        assert!(f.arms[0].x < -0.4);
    }
}
