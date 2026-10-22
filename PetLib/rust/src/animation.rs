use glam::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationId {
    /// 待机
    Idle,
    /// 行走
    Walking,
}

/// 单帧动画姿态
/// 所有欧拉角为绝对值，弧度 (x, y, z)，XYZ 序
#[derive(Clone, Copy)]
pub struct AnimFrame {
    /// 躯干方块欧拉旋转。
    pub body_rot: Vec3,
    /// 头部欧拉旋转。
    pub head_rot: Vec3,
    /// 双臂欧拉旋转 [左, 右]。
    pub arms: [Vec3; 2],
    /// 双腿欧拉旋转 [左, 右]。
    pub legs: [Vec3; 2],
    /// 躯干方块位置增量。
    pub body_pos: Vec3,
    /// 头部位置增量（仅 y）。
    pub head_pos: f32,
    /// 披风欧拉 x。
    pub cape_x: f32,
    /// 披风欧拉 z。
    /// None 表示不触碰骨架当前值。
    pub cape_z: Option<f32>,
}

impl AnimFrame {
    /// resetJoints 等价的静止姿态
    pub fn rest() -> Self {
        Self {
            cape_x: std::f32::consts::PI * 0.06,
            cape_z: Some(0.),
            ..Self::default()
        }
    }

    /// 静止姿态且披风 z 不写
    pub fn rest_with_open_cape() -> Self {
        let mut rest = Self::rest();
        rest.cape_z = None;
        rest
    }
}

impl Default for AnimFrame {
    fn default() -> Self {
        Self {
            body_rot: Vec3::ZERO,
            head_rot: Vec3::ZERO,
            arms: [Vec3::ZERO; 2],
            legs: [Vec3::ZERO; 2],
            body_pos: Vec3::ZERO,
            head_pos: 0.,
            cape_x: 0.,
            cape_z: Some(0.),
        }
    }
}

/// t 为累计动画时间（秒，已含速度缩放）。
pub(crate) fn evaluate_walking(t: f32) -> AnimFrame {
    walking(t * 8.)
}

/// 行走动画，移植 skinview3d 的 WalkingAnimation
/// https://github.com/bs-community/skinview3d/blob/9b8d22d/src/animation.ts#L153
fn walking(t: f32) -> AnimFrame {
    const ARM_Z: f32 = std::f32::consts::PI * 0.02;
    AnimFrame {
        body_rot: Vec3::ZERO,
        head_rot: Vec3::new((t / 5.).sin() * 0.1, (t / 4.).sin() * 0.2, 0.),
        arms: [
            Vec3::new(
                (t + std::f32::consts::PI).sin() * 0.5,
                0.,
                t.cos() * 0.03 + ARM_Z,
            ),
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
        body_pos: Vec3::ZERO,
        head_pos: 0.,
        cape_x: (t / 1.5).sin() * 0.06 + std::f32::consts::PI * 0.06,
        cape_z: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_is_periodic() {
        let period = std::f32::consts::TAU / 8.;
        let a = evaluate_walking(13.7);
        let b = evaluate_walking(13.7 + period);
        assert!(a.arms[0].abs_diff_eq(b.arms[0], 0.0001));
        assert!(a.legs[1].abs_diff_eq(b.legs[1], 0.0001));
    }

    #[test]
    fn walking_arms_oppose_legs() {
        // t = π/2 时左腿前摆最大，左臂应后摆（反相）。
        let f = evaluate_walking(std::f32::consts::FRAC_PI_2 / 8.);
        assert!(f.legs[0].x > 0.4);
        assert!(f.arms[0].x < -0.4);
    }

    #[test]
    fn walking_uses_absolute_z_without_rest_splay() {
        // 行走时手臂 z 分量是绝对值（约 ±0.063 弧度内），不含站姿外张。
        for i in 0..240 {
            let f = evaluate_walking(i as f32 / 60.);
            assert!(f.arms[0].z.abs() < 0.1, "{}", f.arms[0].z);
            assert!(f.arms[1].z.abs() < 0.1);
        }
    }
}
