//! 玩家骨架：动画叠加、注视叠加、整体偏航、压扁脉冲与摆臂（保留特性，暂无输入入口）。
//!
//! 所有平滑均为帧率无关的指数平滑（alpha = 1 - exp(-dt/τ)）。
//! 宿主负责输入策略（鼠标判定、10 秒回正等），本层只接收策略结果：
//! 注视目标坐标与整体偏航目标角。

use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};

use crate::animation::{evaluate, AnimationId, AnimFrame};

const SWING_SECONDS: f32 = 0.30; // 原版攻击动画：20 tick/s 下的 6 tick。
pub const TAU_GAZE: f32 = 0.10;
pub const TAU_YAW: f32 = 0.15;
pub const TAU_ANIM: f32 = 0.25;
pub const TAU_RECENTER: f32 = 0.35;

/// 相机正面基准偏航。角色静止时头部以此角度朝向相机。
pub fn front_yaw() -> f32 {
    30f32.atan2(70.)
}

pub fn rotation(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_euler(EulerRot::XYZ, x, y, z)
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// 相机基准四元数：把角色正面基准对齐到投影相机的屏幕平面。
pub fn view_quat() -> Quat {
    Quat::from_rotation_y(front_yaw())
        * Quat::from_rotation_x(-(23f32 / (30f32 * 30. + 70f32 * 70.).sqrt()).atan())
}

#[derive(Clone, Copy)]
struct LimbSnapshot {
    arms: [Quat; 2],
    legs: [Quat; 2],
    torso: Quat,
    head_sway: Quat,
    bob: f32,
    cape: Vec2,
}

pub struct Skeleton {
    pub tau_gaze: f32,
    pub tau_yaw: f32,
    pub tau_anim: f32,
    pub tau_recenter: f32,
    /// 注视夹角限制（弧度，水平 / 垂直）。
    pub head_range: Vec2,
    /// 压扁深度百分比（0..=30）。
    pub squash_depth: f32,

    gaze_target: Option<Vec2>,
    yaw_target: f32,

    animation: Option<(AnimationId, f32, bool)>,
    progress: f32,
    blend_from: Option<LimbSnapshot>,
    blend: f32,

    /// 平滑后的注视增量（弧度，水平 / 垂直）。
    gaze: Vec2,
    model_yaw: f32,
    squash_start: Option<f32>,

    // 逐帧输出，render.rs 消费。
    pub arms: [Quat; 2],
    pub legs: [Quat; 2],
    pub torso: Quat,
    pub head_sway: Quat,
    pub bob: f32,
    pub cape: Vec2,
    /// 头部最终局部旋转 = 父链逆 ∘ 相机基准 ∘ 注视增量 ∘ 动画摆动。
    pub head: Quat,
    pub squash: f32,
    /// 摆臂扭转角（保留特性）。
    pub body_yaw: f32,

    swing_start: Option<f32>,
    pub swing_progress: f32,
    swing_side: usize,
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton {
    pub fn new() -> Self {
        let arms = [Self::rest_arm(0), Self::rest_arm(1)];
        Self {
            tau_gaze: TAU_GAZE,
            tau_yaw: TAU_YAW,
            tau_anim: TAU_ANIM,
            tau_recenter: TAU_RECENTER,
            head_range: Vec2::new(19.5f32.to_radians(), 53f32.to_radians()),
            squash_depth: 5.,
            gaze_target: None,
            yaw_target: 0.,
            animation: Some((AnimationId::Idle, 1., true)),
            progress: 0.,
            blend_from: None,
            blend: 1.,
            gaze: Vec2::ZERO,
            model_yaw: 0.,
            squash_start: None,
            arms,
            legs: [Quat::IDENTITY; 2],
            torso: Quat::IDENTITY,
            head_sway: Quat::IDENTITY,
            bob: 0.,
            cape: Vec2::ZERO,
            head: view_quat(),
            squash: 1.,
            body_yaw: 0.,
            swing_start: None,
            swing_progress: 0.,
            swing_side: 1,
        }
    }

    fn rest_arm(side: usize) -> Quat {
        let sign = if side == 0 { 1. } else { -1. };
        rotation(-0.035, 0., sign * 0.105)
    }

    pub fn set_gaze_target(&mut self, target: Option<Vec2>) {
        self.gaze_target = target;
    }

    pub fn gaze_target(&self) -> Option<Vec2> {
        self.gaze_target
    }

    pub fn set_yaw_target(&mut self, yaw: f32) {
        self.yaw_target = yaw;
    }

    pub fn model_yaw(&self) -> f32 {
        self.model_yaw
    }

    pub fn head_follow(&self) -> bool {
        self.animation.is_some_and(|(_, _, follow)| follow)
    }

    /// 播放动画。切换动画（含从静止）时从当前肢体姿态交叉过渡。
    pub fn play_animation(&mut self, id: AnimationId, speed: f32, head_follow: bool) {
        if self.animation.is_none_or(|(cur, _, _)| cur != id) {
            self.start_blend();
        }
        self.progress = 0.;
        self.animation = Some((id, speed.max(0.01), head_follow));
    }

    /// 停止动画：肢体回到静止姿态，头部注视按回正常数衰减。
    pub fn stop_animation(&mut self) {
        if self.animation.is_some() {
            self.start_blend();
        }
        self.animation = None;
        self.progress = 0.;
    }

    fn start_blend(&mut self) {
        self.blend_from = Some(self.snapshot());
        self.blend = 0.;
    }

    fn snapshot(&self) -> LimbSnapshot {
        LimbSnapshot {
            arms: self.arms,
            legs: self.legs,
            torso: self.torso,
            head_sway: self.head_sway,
            bob: self.bob,
            cape: self.cape,
        }
    }

    /// 点按压扁。新的点按打断进行中的脉冲，从满高重启。
    pub fn squash_tap(&mut self, now: f32) {
        self.squash_start = Some(now);
        self.squash = 1.;
    }

    /// 攻击摆臂（保留特性，宿主暂不接线）。
    pub fn click(&mut self, now: f32) {
        self.swing_start = Some(now);
        self.swing_progress = 0.;
        self.body_yaw = 0.;
    }

    fn squash_pulse(&self, now: f32) -> f32 {
        let Some(start) = self.squash_start else {
            return 0.;
        };
        let t = (now - start).max(0.);
        if t < 0.08 {
            smooth(t / 0.08)
        } else {
            1. - smooth((t - 0.08) / 0.20)
        }
    }

    fn lerp_f(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    /// 逐帧推进。`view` 为输出视图尺寸（像素），注视目标坐标与之同空间。
    pub fn update(&mut self, view: Vec2, now: f32, dt: f32) {
        if let Some((id, speed, _)) = self.animation {
            let step = dt * speed;
            // 与 skinview3d 的 NewIdle 一致：待机在缺帧时最多补 0.05s。
            self.progress += if id == AnimationId::Idle {
                step.min(0.05)
            } else {
                step
            };
        }
        let frame = self
            .animation
            .map_or(AnimFrame::default(), |(id, _, _)| evaluate(id, self.progress));

        if let Some(from) = self.blend_from {
            self.blend += (1. - self.blend) * (1. - (-dt / self.tau_anim).exp());
            if self.blend >= 0.999 {
                self.blend = 1.;
                self.blend_from = None;
            }
            let t = self.blend;
            for side in 0..2 {
                self.arms[side] = from.arms[side]
                    .slerp(Self::target_arm(side, &frame), t)
                    .normalize();
                self.legs[side] = from.legs[side]
                    .slerp(rotation(frame.legs[side].x, 0., 0.), t)
                    .normalize();
            }
            self.torso = from.torso.slerp(rotation(frame.torso.x, frame.torso.y, frame.torso.z), t);
            self.head_sway =
                from.head_sway.slerp(rotation(frame.head_sway.x, frame.head_sway.y, frame.head_sway.z), t);
            self.bob = Self::lerp_f(from.bob, frame.bob, t);
            self.cape = from.cape.lerp(frame.cape, t);
        } else {
            for side in 0..2 {
                self.arms[side] = Self::target_arm(side, &frame);
                self.legs[side] = rotation(frame.legs[side].x, 0., 0.);
            }
            self.torso = rotation(frame.torso.x, frame.torso.y, frame.torso.z);
            self.head_sway = rotation(frame.head_sway.x, frame.head_sway.y, frame.head_sway.z);
            self.bob = frame.bob;
            self.cape = frame.cape;
        }

        // 整体偏航平滑。
        self.model_yaw += (self.yaw_target - self.model_yaw) * (1. - (-dt / self.tau_yaw).exp());

        // 注视增量：有跟踪目标时朝目标追随，否则按回正常数衰减到 0。
        let tracking = self.gaze_target.is_some() && self.head_follow();
        let (target, tau) = if tracking {
            (self.solve_gaze(view), self.tau_gaze)
        } else {
            (Vec2::ZERO, self.tau_recenter)
        };
        self.gaze += (target - self.gaze) * (1. - (-dt / tau).exp());

        let head_world =
            view_quat() * Quat::from_rotation_y(self.gaze.x) * Quat::from_rotation_x(self.gaze.y)
                * self.head_sway;
        let parent = Quat::from_rotation_y(self.model_yaw) * self.torso;
        self.head = parent.inverse() * head_world;

        // 压扁脉冲。
        self.squash = 1. - self.squash_pulse(now) * (self.squash_depth / 100.).clamp(0., 0.3);

        // 摆臂（保留特性）。
        if self.swing_start.is_some_and(|start| now - start >= SWING_SECONDS) {
            self.swing_start = None;
            // 摆臂结束后从当前姿态平滑回到动画目标。
            self.start_blend();
        }
        self.swing_progress = self
            .swing_start
            .map(|start| ((now - start) / SWING_SECONDS).clamp(0., 1.))
            .unwrap_or(0.);
        self.body_yaw = if self.swing_start.is_some() {
            -(self.swing_progress.sqrt() * std::f32::consts::TAU).sin()
                * 0.2
                * if self.swing_side == 0 { -1. } else { 1. }
        } else {
            0.
        };
        if self.swing_start.is_some() {
            let side = self.swing_side;
            let sign = if side == 0 { -1. } else { 1. };
            let p = self.swing_progress;
            let pi = std::f32::consts::PI;
            // 原版 HumanoidModel 攻击曲线：四次缓动、躯干扭转与侧向翻滚。
            let eased = 1. - (1. - p).powi(4);
            let strike =
                (eased * pi).sin() * 1.2 + (p * pi).sin() * -(self.gaze.y - 0.7) * 0.75;
            self.arms[side] = rotation(
                -0.035 - strike,
                self.body_yaw * 3.,
                sign * (-0.105 + (p * pi).sin() * 0.4),
            );
        }
    }

    fn target_arm(side: usize, frame: &AnimFrame) -> Quat {
        let sign = if side == 0 { 1. } else { -1. };
        rotation(
            -0.035 + frame.arms[side].x,
            0.,
            sign * 0.105 + frame.arms[side].z,
        )
    }

    /// 以相机隐含距离解算指向光标的 yaw/pitch，并按颈部活动范围截断。
    fn solve_gaze(&self, view: Vec2) -> Vec2 {
        let Some(cursor) = self.gaze_target else {
            return Vec2::ZERO;
        };
        // 眼睛的世界位置：根偏航 ∘ 髋部 ∘ bob ∘ 躯干 ∘ 颈部（Y*12）+ 相机基准上移。
        let neck = Mat4::from_rotation_y(self.model_yaw)
            * Mat4::from_translation(Vec3::Y * (12. + self.bob))
            * Mat4::from_quat(self.torso)
            * Mat4::from_translation(Vec3::Y * 12.);
        let view_basis = view_quat();
        let eye = neck.transform_point3(Vec3::ZERO) + view_basis * (Vec3::Y * 3.);
        let p = crate::render::project(eye, view.x, view.y);
        let distance = Vec3::new(30., 23., 70.).length();
        let unit = view.x / 44.;
        let dx = (cursor.x - p.x) / unit;
        let dy = (p.y - cursor.y) / unit;
        let h = dx
            .atan2(distance)
            .clamp(-self.head_range.x, self.head_range.x);
        let v = (-dy / (dx * dx + dy * dy + distance * distance).sqrt())
            .asin()
            .clamp(-self.head_range.y, self.head_range.y);
        Vec2::new(h, v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaze_recenters_after_target_removed() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        s.set_gaze_target(Some(Vec2::new(400., 100.)));
        for i in 0..120 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.x > 0.01);
        s.set_gaze_target(None);
        for i in 120..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.length() < 0.001, "gaze = {:?}", s.gaze);
    }

    #[test]
    fn gaze_respects_head_range() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        // 光标远超水平活动范围，增量必须被截断。
        s.set_gaze_target(Some(Vec2::new(5000., 525.)));
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!((s.gaze.x - s.head_range.x).abs() < 0.001);
    }

    #[test]
    fn model_yaw_approaches_target() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        s.set_yaw_target(std::f32::consts::FRAC_PI_2);
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!((s.model_yaw - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    }

    #[test]
    fn animation_switch_blends_from_current_pose() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        for i in 0..60 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let before = s.arms[0];
        s.play_animation(AnimationId::Walking, 0.8, true);
        s.update(view, 1.01, 1. / 60.);
        // 切换瞬间不应直接跳到行走目标。
        let walking = Skeleton::target_arm(0, &evaluate(AnimationId::Walking, 0.08));
        assert!(before.abs_diff_eq(s.arms[0], 0.4));
        assert!(!s.arms[0].abs_diff_eq(walking, 0.0001));
        for i in 2..180 {
            s.update(view, 1. + i as f32 / 60., 1. / 60.);
        }
        assert!(s.blend_from.is_none());
    }

    #[test]
    fn squash_taps_restart_from_full_height() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        s.squash_tap(0.);
        for i in 0..6 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let mid = s.squash;
        assert!(mid < 1.);
        // 连续点按：重启后脉冲回到满高。
        s.squash_tap(0.1);
        s.update(view, 0.11, 1. / 60.);
        assert!(s.squash > mid);
    }
}
