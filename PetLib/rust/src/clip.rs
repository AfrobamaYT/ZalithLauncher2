//! Modrinth 关键帧剪辑的采样与待机调度

use glam::{Mat3, Quat, Vec2, Vec3};

use crate::clip_data::{Clip, Joint, CLIP_FPS, CLIP_IDLE, CLIP_SUBS};

/// 随机子动画触发间隔（秒）
pub(crate) const SUB_INTERVAL: f32 = 8.0;
/// 点击反应剪辑的播放时长（截取自 0.83 秒的完整数据）
pub(crate) const INTERACT_DURATION: f32 = 0.5;

/// 采样剪辑得到的骨架关节目标
#[derive(Clone, Copy)]
pub(crate) struct ClipPose {
    pub body_rot: Quat,
    pub head_rot: Quat,
    /// [左, 右]
    pub arms: [Quat; 2],
    /// [左, 右]
    pub legs: [Quat; 2],
    pub body_pos: Vec3,
    /// 披风欧拉 (x, z)
    pub cape: Vec2,
}

/// 采样剪辑在时间 `t`（秒）的姿态
pub(crate) fn sample(clip: &Clip, t: f32) -> ClipPose {
    let mut pose = ClipPose {
        body_rot: Quat::IDENTITY,
        head_rot: Quat::IDENTITY,
        arms: [Quat::IDENTITY; 2],
        legs: [Quat::IDENTITY; 2],
        body_pos: Vec3::ZERO,
        cape: Vec2::ZERO,
    };
    for channel in clip.channels {
        match channel.joint {
            Joint::Body => {
                pose.body_rot = sample_rotation(channel.rotation, t);
                pose.body_pos = sample_translation(channel.translation, t);
            }
            Joint::Head => pose.head_rot = sample_rotation(channel.rotation, t),
            Joint::ArmLeft => pose.arms[0] = sample_rotation(channel.rotation, t),
            Joint::ArmRight => pose.arms[1] = sample_rotation(channel.rotation, t),
            Joint::LegLeft => pose.legs[0] = sample_rotation(channel.rotation, t),
            Joint::LegRight => pose.legs[1] = sample_rotation(channel.rotation, t),
            Joint::Cape => pose.cape = cape_euler(sample_rotation(channel.rotation, t)),
        }
    }
    pose
}

fn sample_rotation(keys: &[[f32; 4]], t: f32) -> Quat {
    let frame = (t * CLIP_FPS).max(0.);
    let last = keys.len() - 1;
    let quat = |k: &[f32; 4]| Quat::from_xyzw(k[0], k[1], k[2], k[3]);
    if last == 0 {
        return quat(&keys[0]);
    }
    let index = (frame as usize).min(last - 1);
    let frac = (frame - index as f32).clamp(0., 1.);
    quat(&keys[index]).slerp(quat(&keys[index + 1]), frac)
}

fn sample_translation(keys: &[[f32; 3]], t: f32) -> Vec3 {
    let frame = (t * CLIP_FPS).max(0.);
    let last = keys.len() - 1;
    if last == 0 {
        return Vec3::new(keys[0][0], keys[0][1], keys[0][2]);
    }
    let index = (frame as usize).min(last - 1);
    let frac = (frame - index as f32).clamp(0., 1.);
    let a = Vec3::new(keys[index][0], keys[index][1], keys[index][2]);
    let b = Vec3::new(keys[index + 1][0], keys[index + 1][1], keys[index + 1][2]);
    a.lerp(b, frac)
}

/// 把披风四元数分解为渲染端欧拉 (x, z)：目标旋转 = rotation(x, π, z)，
/// 即 Rx(x)·Ry(π)·Rz(z)，要求目标不含 y 分量（披风剪辑满足）。
fn cape_euler(q: Quat) -> Vec2 {
    // glam Mat3 的 x/y/z_axis 即矩阵列
    let m = Mat3::from_quat(q);
    let x = m.z_axis.y.atan2(-m.z_axis.z);
    let z = m.y_axis.x.atan2(-m.x_axis.x);
    Vec2::new(x, z)
}

fn clip_duration(clip: &Clip) -> f32 {
    let keys = clip
        .channels
        .iter()
        .map(|c| c.rotation.len().max(c.translation.len()))
        .max()
        .unwrap_or(1);
    (keys.max(1) - 1) as f32 / CLIP_FPS
}

fn next_rand(rng: &mut u64) -> u64 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *rng = x;
    x
}

/// 待机调度：基础 idle 循环，每 8 秒随机播一个子动画（不连续重复），
/// 播完淡回基础循环并重新计时。
pub(crate) struct IdleClips {
    base_time: f32,
    /// 当前播放的子动画（序号，已播时间）
    sub: Option<(usize, f32)>,
    /// 距下次随机子动画的秒数
    pub(crate) countdown: f32,
    last_sub: Option<usize>,
    rng: u64,
}

/// 一次调度推进的结果
pub(crate) struct IdleTick {
    pub clip: &'static Clip,
    pub time: f32,
    /// 本次推进是否切换了剪辑源（调用方应启动交叉过渡）
    pub switched: bool,
}

impl IdleClips {
    pub(crate) fn new() -> Self {
        // 测试下用固定种子：姿态断言不随随机子动画的选择而波动
        #[cfg(test)]
        return Self::seeded(0x2545F4914F6CDD1D);
        #[cfg(not(test))]
        {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(1);
            Self::seeded(seed)
        }
    }

    pub(crate) fn seeded(seed: u64) -> Self {
        Self {
            base_time: 0.,
            sub: None,
            countdown: SUB_INTERVAL,
            last_sub: None,
            rng: seed | 1,
        }
    }

    /// 是否有子动画正在播放
    pub(crate) fn sub_active(&self) -> bool {
        self.sub.is_some()
    }

    /// 回到基础循环起点并重新计时
    pub(crate) fn reset(&mut self) {
        self.base_time = 0.;
        self.sub = None;
        self.countdown = SUB_INTERVAL;
        self.last_sub = None;
    }

    /// 点击反应播完后的回归：丢弃未完的子动画，重置基础时间并重新计时
    pub(crate) fn rebase(&mut self) {
        self.base_time = 0.;
        self.sub = None;
        self.countdown = SUB_INTERVAL;
    }

    pub(crate) fn tick(&mut self, dt: f32, speed: f32) -> IdleTick {
        if let Some((index, time)) = self.sub {
            let time = time + (dt * speed).min(0.05);
            if time >= clip_duration(&CLIP_SUBS[index]) {
                self.rebase();
                let mut tick = self.base_tick();
                tick.switched = true;
                return tick;
            }
            self.sub = Some((index, time));
            return IdleTick {
                clip: &CLIP_SUBS[index],
                time,
                switched: false,
            };
        }
        // 就地回绕：无限累计会让采样相位在数日量级后损失 f32 精度
        self.base_time =
            (self.base_time + (dt * speed).min(0.05)) % clip_duration(&CLIP_IDLE);
        self.countdown -= dt.min(0.05);
        if self.countdown <= 0. && !CLIP_SUBS.is_empty() {
            let index = self.pick_sub();
            self.last_sub = Some(index);
            self.sub = Some((index, 0.));
            return IdleTick {
                clip: &CLIP_SUBS[index],
                time: 0.,
                switched: true,
            };
        }
        self.base_tick()
    }

    /// 基础循环采样：时间已就地回绕，取模仅作冗余保护
    fn base_tick(&self) -> IdleTick {
        IdleTick {
            clip: &CLIP_IDLE,
            time: self.base_time % clip_duration(&CLIP_IDLE),
            switched: false,
        }
    }

    /// 随机挑一个子动画，排除上一次播放的
    fn pick_sub(&mut self) -> usize {
        let count = CLIP_SUBS.len();
        let excluded = self.last_sub;
        let candidates: Vec<usize> = (0..count).filter(|&i| Some(i) != excluded).collect();
        let pool: Vec<usize> = if candidates.is_empty() {
            (0..count).collect()
        } else {
            candidates
        };
        pool[next_rand(&mut self.rng) as usize % pool.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip_data::{CLIP_IDLE, CLIP_INTERACT, CLIP_SUBS};
    use crate::pose::rotation;

    fn all_clips() -> Vec<&'static Clip> {
        let mut clips = vec![&CLIP_IDLE, &CLIP_INTERACT];
        clips.extend(CLIP_SUBS.iter());
        clips
    }

    #[test]
    fn idle_sample_starts_near_rest() {
        let pose = sample(&CLIP_IDLE, 0.);
        let angle = |q: Quat| 2. * q.w.acos().abs();
        assert!(angle(pose.body_rot) < 0.02);
        assert!(angle(pose.head_rot) < 0.02);
        for arm in pose.arms {
            assert!(angle(arm) < 0.05, "arm at rest must hang straight");
        }
        for leg in pose.legs {
            assert!(angle(leg) < 0.001);
        }
        // 躯干下沉换算到像素后落在 [−0.65, −0.3]
        assert!((-0.66..=-0.3).contains(&pose.body_pos.y));
        assert!(pose.body_pos.x.abs() < 0.05);
    }

    #[test]
    fn cape_euler_roundtrips_every_key() {
        for clip in all_clips() {
            for channel in clip.channels {
                if channel.joint != Joint::Cape {
                    continue;
                }
                for key in channel.rotation {
                    let q = Quat::from_xyzw(key[0], key[1], key[2], key[3]);
                    let e = cape_euler(q);
                    let restored = rotation(e.x, std::f32::consts::PI, e.y);
                    // 四元数双覆盖：按旋转角比较而非分量
                    assert!(
                        restored.angle_between(q) < 0.01,
                        "cape euler roundtrip failed for {key:?}: {e:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn cape_rest_matches_skeleton_convention() {
        // 披风静止欧拉必须落在骨架基准 (0.06π, 0) 附近，
        // 这是整套坐标系换算（180° 翻转 + 模型朝向）正确性的锚点。
        let pose = sample(&CLIP_IDLE, 0.);
        let base = std::f32::consts::PI * 0.06;
        assert!(
            (pose.cape.x - base).abs() < 0.15,
            "cape x = {}",
            pose.cape.x
        );
        assert!(pose.cape.y.abs() < 0.1, "cape z = {}", pose.cape.y);
    }

    #[test]
    fn clip_samples_stay_finite() {
        for clip in all_clips() {
            let duration = clip_duration(clip);
            let steps = (duration / 0.033).ceil() as usize;
            for i in 0..=steps {
                let pose = sample(clip, duration * i as f32 / steps.max(1) as f32);
                for q in [pose.body_rot, pose.head_rot] {
                    assert!(q.is_finite());
                    assert!((q.length() - 1.).abs() < 0.001);
                }
                assert!(pose.body_pos.is_finite());
            }
        }
    }

    #[test]
    fn idle_loops_seamlessly() {
        let duration = clip_duration(&CLIP_IDLE);
        let a = sample(&CLIP_IDLE, 0.);
        let b = sample(&CLIP_IDLE, duration);
        assert!(a.body_rot.abs_diff_eq(b.body_rot, 0.001));
        assert!(a.head_rot.abs_diff_eq(b.head_rot, 0.001));
        assert!(a.arms[0].abs_diff_eq(b.arms[0], 0.001));
        assert!(a.cape.abs_diff_eq(b.cape, 0.001));
        assert!(a.body_pos.abs_diff_eq(b.body_pos, 0.001));
    }

    #[test]
    fn scheduler_fires_sub_after_interval_and_returns() {
        let mut clips = IdleClips::seeded(0x2545F4914F6CDD1D);
        let dt = 1. / 60.;
        let sub_index = |clip: &Clip| {
            CLIP_SUBS
                .iter()
                .position(|c| std::ptr::eq(c.channels, clip.channels))
        };
        // 8 秒后触发子动画（允许 ±3 帧误差）
        let mut fired_at = None;
        for i in 0..600 {
            let tick = clips.tick(dt, 1.);
            if tick.switched {
                fired_at = Some(i);
                assert!(sub_index(tick.clip).is_some(), "must switch to a sub clip");
                assert_eq!(tick.time, 0.);
                break;
            }
        }
        let fired_at = fired_at.expect("sub must fire within 10s");
        assert!((fired_at as f32 * dt - SUB_INTERVAL).abs() < 0.1);
        // 子动画 3 秒后淡回基础循环
        let mut returned = false;
        for _ in 0..200 {
            if clips.tick(dt, 1.).switched {
                returned = true;
                break;
            }
        }
        assert!(returned, "sub must return to base after ~3s");
        // 回归后一个完整间隔内不再触发
        for _ in 0..470 {
            assert!(!clips.tick(dt, 1.).switched);
        }
    }

    #[test]
    fn scheduler_never_repeats_last_sub() {
        let mut clips = IdleClips::seeded(0x9E3779B97F4A7C15);
        let dt = 0.05;
        let mut last = None;
        let mut fired = 0;
        while fired < 12 {
            let tick = clips.tick(dt, 1.);
            // 子动画触发与播完淡回都会置 switched，只统计触发
            let Some(index) = CLIP_SUBS
                .iter()
                .position(|c| std::ptr::eq(c.channels, tick.clip.channels))
            else {
                continue;
            };
            if !tick.switched {
                continue;
            }
            fired += 1;
            if let Some(prev) = last {
                assert_ne!(prev, index, "sub must not repeat consecutively");
            }
            last = Some(index);
        }
    }

    #[test]
    fn reset_restarts_countdown() {
        let mut clips = IdleClips::seeded(7);
        let dt = 1. / 60.;
        for _ in 0..500 {
            clips.tick(dt, 1.);
        }
        clips.reset();
        for _ in 0..470 {
            assert!(!clips.tick(dt, 1.).switched);
        }
    }

    #[test]
    fn base_idle_time_wraps_for_looping() {
        let mut clips = IdleClips::seeded(11);
        let dt = 0.05;
        let duration = clip_duration(&CLIP_IDLE);
        // 覆盖多轮循环与子动画穿插：基础采样时间必须始终落在循环长度内
        for _ in 0..400 {
            let tick = clips.tick(dt, 1.);
            if std::ptr::eq(tick.clip.channels, CLIP_IDLE.channels) {
                assert!(
                    (0.0..=duration).contains(&tick.time),
                    "base time {} must wrap within {duration}",
                    tick.time
                );
            }
        }
    }

    #[test]
    fn scheduler_runs_indefinitely() {
        // 20 虚拟分钟：子动画持续触发（不因时间累计而停摆），
        // 基础循环相位持续回绕（不冻结在末帧）。
        let mut clips = IdleClips::seeded(0x51ED270B);
        let dt = 1. / 60.;
        let mut sub_fires = 0;
        let mut phase_wraps = 0;
        let mut last_idle_time = 0f32;
        for _ in 0..(60 * 60 * 20) {
            let tick = clips.tick(dt, 1.);
            if std::ptr::eq(tick.clip.channels, CLIP_IDLE.channels) {
                if tick.time < last_idle_time {
                    phase_wraps += 1;
                }
                last_idle_time = tick.time;
            } else if tick.switched && tick.time == 0. {
                sub_fires += 1;
            }
        }
        assert!(sub_fires > 100, "subs must keep firing: {sub_fires}");
        assert!(phase_wraps > 300, "idle must keep looping: {phase_wraps}");
    }
}