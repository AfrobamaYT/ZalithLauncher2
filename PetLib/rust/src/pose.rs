//! 玩家骨架

use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};

use crate::animation::{evaluate, AnimFrame, AnimationId};

/// 攻击动画时长 (20 tick/s 下的 6 tick)
const SWING_SECONDS: f32 = 0.30;

pub const TAU_GAZE: f32 = 0.10;
pub const TAU_YAW: f32 = 0.15;
pub const TAU_ANIM: f32 = 0.25;
pub const TAU_RECENTER: f32 = 0.35;

/// 背对切断阈值：偏航相对摄像机方向超过此角放弃跟随。
const GAZE_OFF_YAW: f32 = 90f32.to_radians();
/// 背对恢复阈值：拖回此角以内才恢复（迟滞）。
const GAZE_ON_YAW: f32 = 75f32.to_radians();
/// 过扭放弃阈值：头-身相对水平扭角超过此角放弃跟随。
const GAZE_TWIST_OFF: f32 = 60f32.to_radians();
/// 过扭恢复阈值（迟滞）。
const GAZE_TWIST_ON: f32 = 50f32.to_radians();

fn wrap_pi(angle: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let a = angle % two_pi;
    if a > std::f32::consts::PI {
        a - two_pi
    } else if a < -std::f32::consts::PI {
        a + two_pi
    } else {
        a
    }
}

/// 相机正面基准偏航。
/// 角色静止时头部以此角度朝向相机。
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
        * Quat::from_rotation_x(-(23f32 / (30f32 * 30f32 + 70f32 * 70f32).sqrt()).atan())
}

#[derive(Clone, Copy)]
struct LimbSnapshot {
    arms: [Quat; 2],
    legs: [Quat; 2],
    head_rot: Quat,
    body_rot: Quat,
    body_pos: Vec3,
    head_pos: f32,
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
    /// 背对切断锁存：越过切断角置位，拖回恢复角以下复位。
    gaze_yaw_cut: bool,
    /// 过扭切断锁存：需求扭角越过放弃角置位，回到恢复角以内复位。
    gaze_twist_cut: bool,
    model_yaw: f32,
    squash_start: Option<f32>,

    // 逐帧输出，render.rs 消费。
    pub arms: [Quat; 2],
    pub legs: [Quat; 2],
    pub head_rot: Quat,
    pub body_rot: Quat,
    pub body_pos: Vec3,
    pub head_pos: f32,
    pub cape: Vec2,
    /// 头部最终局部旋转
    pub head: Quat,
    pub squash: f32,
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
        Self {
            tau_gaze: TAU_GAZE,
            tau_yaw: TAU_YAW,
            tau_anim: TAU_ANIM,
            tau_recenter: TAU_RECENTER,
            head_range: Vec2::new(60f32.to_radians(), 53f32.to_radians()),
            squash_depth: 5.,
            gaze_target: None,
            yaw_target: 0.,
            animation: Some((AnimationId::Idle, 1., true)),
            progress: 0.,
            blend_from: None,
            blend: 1.,
            gaze: Vec2::ZERO,
            gaze_yaw_cut: false,
            gaze_twist_cut: false,
            model_yaw: 0.,
            squash_start: None,
            arms: [Quat::IDENTITY; 2],
            legs: [Quat::IDENTITY; 2],
            head_rot: Quat::IDENTITY,
            body_rot: Quat::IDENTITY,
            body_pos: Vec3::ZERO,
            head_pos: 0.,
            cape: Vec2::ZERO,
            head: Quat::IDENTITY,
            squash: 1.,
            body_yaw: 0.,
            swing_start: None,
            swing_progress: 0.,
            swing_side: 1,
        }
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

    /// 播放动画。
    pub fn play_animation(&mut self, id: AnimationId, speed: f32, head_follow: bool) {
        if self.animation.map_or(true, |(cur, _, _)| cur != id) {
            self.start_blend();
        }
        self.progress = 0.;
        self.animation = Some((id, speed.max(0.01), head_follow));
    }

    /// 停止动画。
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
            head_rot: self.head_rot,
            body_rot: self.body_rot,
            body_pos: self.body_pos,
            head_pos: self.head_pos,
            cape: self.cape,
        }
    }

    /// 播放点按压扁。
    pub fn squash_tap(&mut self, now: f32) {
        self.squash_start = Some(now);
        self.squash = 1.;
    }

    /// 播放攻击摆臂。
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
        // 负间隔非法：平滑系数与动画进度都不允许倒退。
        let dt = dt.max(0.);
        // tick 时序：待机先推进内部时钟再求值；行走先求值再推进。
        let frame = match self.animation {
            None => {
                // resetJoints 语义：皮肤部件归位、cape.x 回默认仰角；
                let mut rest = AnimFrame::rest();
                rest.cape_z = None;
                rest
            }
            Some((AnimationId::Idle, speed, _)) => {
                // 待机在缺帧时最多补 0.05s（NewIdle 的 maxDelta）。
                self.progress += (dt * speed).min(0.05);
                evaluate(AnimationId::Idle, self.progress)
            }
            Some((AnimationId::Walking, speed, _)) => {
                let frame = evaluate(AnimationId::Walking, self.progress);
                self.progress += dt * speed;
                frame
            }
        };

        let arm_targets = frame.arms.map(|e| rotation(e.x, e.y, e.z));
        let leg_targets = frame.legs.map(|e| rotation(e.x, e.y, e.z));
        let head_target = rotation(frame.head_rot.x, frame.head_rot.y, frame.head_rot.z);
        let body_target = rotation(frame.body_rot.x, frame.body_rot.y, frame.body_rot.z);
        // 披风 z：动画不写时保持骨架当前值。
        let cape_target = Vec2::new(frame.cape_x, frame.cape_z.unwrap_or(self.cape.y));

        if let Some(from) = self.blend_from {
            self.blend += (1. - self.blend) * (1. - (-dt / self.tau_anim).exp());
            if self.blend >= 0.999 {
                self.blend = 1.;
                self.blend_from = None;
            }
            let t = self.blend;
            for side in 0..2 {
                self.arms[side] = from.arms[side].slerp(arm_targets[side], t).normalize();
                self.legs[side] = from.legs[side].slerp(leg_targets[side], t).normalize();
            }
            self.head_rot = from.head_rot.slerp(head_target, t);
            self.body_rot = from.body_rot.slerp(body_target, t);
            self.body_pos = from.body_pos.lerp(frame.body_pos, t);
            self.head_pos = Self::lerp_f(from.head_pos, frame.head_pos, t);
            self.cape = from.cape.lerp(cape_target, t);
        } else {
            self.arms = arm_targets;
            self.legs = leg_targets;
            self.head_rot = head_target;
            self.body_rot = body_target;
            self.body_pos = frame.body_pos;
            self.head_pos = frame.head_pos;
            self.cape = cape_target;
        }

        // 整体偏航平滑。
        self.model_yaw += (self.yaw_target - self.model_yaw) * (1. - (-dt / self.tau_yaw).exp());

        // 头部姿态：
        // 追踪中  世界空间解算指向输入点的朝向，受颈部活动范围截断，以 τ_gaze 平滑追随；
        //        背对摄像机或头-身水平扭角超限时放弃跟随，头部回正到身体朝向。
        // 未追踪  与身体朝向对齐（仅动画摆动），以 τ_recenter 平滑回正。
        let parent = Quat::from_rotation_y(self.model_yaw + front_yaw());
        let (head_target, tau) = match self.gaze_target.filter(|_| self.head_follow()) {
            Some(_) => {
                let raw = self.solve_gaze(view);
                // 背对切断（迟滞）：model_yaw 即相对相机正面的偏航。
                let facing = wrap_pi(self.model_yaw).abs();
                if facing > GAZE_OFF_YAW {
                    self.gaze_yaw_cut = true;
                } else if facing < GAZE_ON_YAW {
                    self.gaze_yaw_cut = false;
                }
                // 过扭放弃（迟滞）：头-身相对水平扭角。
                let twist = wrap_pi(raw.x - self.model_yaw).abs();
                if twist > GAZE_TWIST_OFF {
                    self.gaze_twist_cut = true;
                } else if twist < GAZE_TWIST_ON {
                    self.gaze_twist_cut = false;
                }
                if self.gaze_yaw_cut || self.gaze_twist_cut {
                    self.gaze = Vec2::ZERO;
                    (self.head_rot, self.tau_recenter)
                } else {
                    let target = Vec2::new(
                        raw.x.clamp(-self.head_range.x, self.head_range.x),
                        raw.y.clamp(-self.head_range.y, self.head_range.y),
                    );
                    self.gaze += (target - self.gaze) * (1. - (-dt / self.tau_gaze).exp());
                    let head_world = view_quat()
                        * Quat::from_rotation_y(self.gaze.x)
                        * Quat::from_rotation_x(self.gaze.y)
                        * self.head_rot;
                    (parent.inverse() * head_world, self.tau_gaze)
                }
            }
            None => {
                self.gaze = Vec2::ZERO;
                self.gaze_yaw_cut = false;
                self.gaze_twist_cut = false;
                (self.head_rot, self.tau_recenter)
            }
        };
        self.head = self
            .head
            .slerp(head_target, 1. - (-dt / tau).exp())
            .normalize();

        // 压扁脉冲。
        self.squash = 1. - self.squash_pulse(now) * (self.squash_depth / 100.).clamp(0., 0.3);

        // 攻击摆臂。
        if self
            .swing_start
            .is_some_and(|start| now - start >= SWING_SECONDS)
        {
            self.swing_start = None;
            self.start_blend(); // 结束后回到动画目标。
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
            let strike = (eased * pi).sin() * 1.2 + (p * pi).sin() * -(self.gaze.y - 0.7) * 0.75;
            self.arms[side] = rotation(-strike, self.body_yaw * 3., sign * ((p * pi).sin() * 0.4));
        }
    }

    /// 以相机隐含距离解算指向光标的 yaw/pitch（未按颈部活动范围夹取）。
    fn solve_gaze(&self, view: Vec2) -> Vec2 {
        let Some(cursor) = self.gaze_target else {
            return Vec2::ZERO;
        };
        // 眼睛的世界位置：根偏航 ∘ 颈部（头部枢轴，含 bob）+ 相机基准上移。
        let neck = Mat4::from_rotation_y(self.model_yaw + front_yaw())
            * Mat4::from_translation(Vec3::Y * (24. + self.head_pos));
        let view_basis = view_quat();
        let eye = neck.transform_point3(Vec3::ZERO) + view_basis * (Vec3::Y * 3.);
        let p = crate::render::project(eye, view.x, view.y);
        let distance = Vec3::new(30., 23., 70.).length();
        let unit = view.x / 44.;
        let dx = (cursor.x - p.x) / unit;
        let dy = (p.y - cursor.y) / unit;
        let h = dx.atan2(distance);
        let v = (-dy / (dx * dx + dy * dy + distance * distance).sqrt()).asin();
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
    fn gaze_follows_directly_within_twist_limit() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        // 需求角在自然范围内：直接跟随，注视收敛到需求角本身。
        s.set_gaze_target(Some(Vec2::new(920., 262.5)));
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let expected = s.solve_gaze(view);
        assert!((s.gaze.x - expected.x).abs() < 0.001);
    }

    #[test]
    fn gaze_abandons_beyond_twist_limit() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        // 需求扭角远超自然范围：放弃跟随，头部回正到身体朝向。
        s.set_gaze_target(Some(Vec2::new(5000., 262.5)));
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.length() < 0.001, "gaze = {:?}", s.gaze);
        assert!(
            s.head.abs_diff_eq(s.head_rot, 0.1),
            "head must realign with body"
        );
    }

    #[test]
    fn back_facing_cuts_gaze_with_hysteresis() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        s.set_gaze_target(Some(Vec2::new(920., 262.5)));
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let expected = s.solve_gaze(view);
        assert!((s.gaze.x - expected.x).abs() < 0.001);
        // 背对：切断跟随，头部回正。
        s.set_yaw_target(std::f32::consts::PI);
        for i in 600..1200 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.length() < 0.001);
        assert!(
            s.head.abs_diff_eq(s.head_rot, 0.1),
            "head must realign while back-facing"
        );
        // 拖回 82°（处于 75°~90° 迟滞区间）：保持切断。
        s.set_yaw_target(82f32.to_radians());
        for i in 1200..1800 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(
            s.gaze.length() < 0.001,
            "hysteresis must hold within (75°, 90°) from camera"
        );
        // 回到正面：恢复跟随。
        s.set_yaw_target(0.);
        for i in 1800..2400 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let expected = s.solve_gaze(view);
        assert!((s.gaze.x - expected.x).abs() < 0.001);
    }

    #[test]
    fn gaze_rules_mirror_for_rightward_turns() {
        // 上述规则的镜像：身体向右转、光标在屏幕左侧，判定量级必须一致。
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        // 需求角超夹角上限、未过扭：贴在负向夹角上限。
        s.set_gaze_target(Some(Vec2::new(-443., 262.5)));
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let expected = s.solve_gaze(view);
        assert!((s.gaze.x - expected.x).abs() < 0.001);
        // 需求扭角远超自然范围：放弃。
        s.set_gaze_target(Some(Vec2::new(-4500., 262.5)));
        for i in 600..1200 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.length() < 0.001, "gaze = {:?}", s.gaze);
        // 光标回到可跟随位置，继续背对切断的验证。
        s.set_gaze_target(Some(Vec2::new(-443., 262.5)));
        // 背对（负方向）切断。
        s.set_yaw_target(-std::f32::consts::PI);
        for i in 1200..1800 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(s.gaze.length() < 0.001);
        // 拖回 -82°（处于 -90°~-75° 迟滞区间）：保持切断。
        s.set_yaw_target(-82f32.to_radians());
        for i in 1800..2400 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        assert!(
            s.gaze.length() < 0.001,
            "hysteresis must hold within (-90°, -75°) from camera"
        );
        // 回到正面：恢复跟随，看向左侧光标。
        s.set_yaw_target(0.);
        for i in 2400..3000 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        let expected = s.solve_gaze(view);
        assert!((s.gaze.x - expected.x).abs() < 0.001);
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
    fn recentered_head_aligns_with_body() {
        let mut s = Skeleton::new();
        let view = Vec2::new(450., 525.);
        s.set_yaw_target(std::f32::consts::FRAC_PI_2);
        for i in 0..600 {
            s.update(view, i as f32 / 60., 1. / 60.);
        }
        // 无输入时头部与身体朝向对齐（仅存动画摆动），不额外朝相机偏转。
        assert!(
            s.head.abs_diff_eq(s.head_rot, 0.05),
            "head={:?} sway={:?}",
            s.head,
            s.head_rot
        );
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
        let f = evaluate(AnimationId::Walking, 0.08);
        let walking = rotation(f.arms[0].x, f.arms[0].y, f.arms[0].z);
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
