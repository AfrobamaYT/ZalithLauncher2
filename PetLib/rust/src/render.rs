//! 软件光栅化：盒体部件 → 投影 → 三角形覆盖 → z-buffer 逐像素混合，
//! 末端经盒式降采样输出预乘 RGBA（Android Bitmap 原生格式）。

use glam::{Mat4, Vec2, Vec3};

use crate::lighting::Lighting;
use crate::pose::{self, Skeleton};
use crate::skin::{ArmModel, Skin, Texture};

/// 投影：弱透视（z 仅用于深度排序），模型竖直居中于视图。
pub fn project(v: Vec3, width: f32, height: f32) -> Vec3 {
    let forward = Vec3::new(30., 23., 70.).normalize();
    let right = Vec3::Y.cross(forward).normalize();
    let up = forward.cross(right);
    let v = v - Vec3::new(0., 15., 0.);
    let scale = width / 44.;
    Vec3::new(
        width / 2. + v.dot(right) * scale,
        height / 2. - v.dot(up) * scale,
        -v.dot(forward),
    )
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tex {
    Skin,
    Cape,
}

#[derive(Clone)]
pub(crate) struct BoxPart {
    size: Vec3,
    uv: Vec2,
    transform: Mat4,
    inflate: f32,
    mirror: bool,
    tex: Tex,
}

pub struct SceneParams<'a> {
    pub skin: &'a Skin,
    pub cape: Option<&'a Texture>,
    pub outer_layer: bool,
}

pub(crate) fn parts(pose: &Skeleton, scene: &SceneParams) -> Vec<BoxPart> {
    let skin = scene.skin;
    let aw = if skin.arm == ArmModel::Slim && !skin.legacy {
        3.
    } else {
        4.
    };
    let hip = Mat4::from_translation(Vec3::Y * 12.);
    let torso_m = hip
        * Mat4::from_translation(Vec3::Y * pose.bob)
        * Mat4::from_quat(pose.torso)
        * Mat4::from_rotation_y(pose.body_yaw);
    // 摆臂扭转只带动躯干与手臂；头部在扭转中保持注视目标。
    let torso_steady = hip * Mat4::from_translation(Vec3::Y * pose.bob) * Mat4::from_quat(pose.torso);
    let mut v: Vec<BoxPart> = Vec::with_capacity(13);
    {
        let mut part = |transform: Mat4,
                        size: Vec3,
                        u: f32,
                        w: f32,
                        outer: Vec2,
                        mirror: bool,
                        hat: bool| {
            v.push(BoxPart {
                size,
                uv: Vec2::new(u, w),
                transform,
                inflate: 0.,
                mirror,
                tex: Tex::Skin,
            });
            if scene.outer_layer && (!skin.legacy || hat) {
                v.push(BoxPart {
                    size,
                    uv: outer,
                    transform,
                    inflate: if hat { 1. } else { 0.5 },
                    mirror,
                    tex: Tex::Skin,
                });
            }
        };
        part(
            torso_m * Mat4::from_translation(Vec3::new(0., 6., 0.)),
            Vec3::new(8., 12., 4.),
            16.,
            16.,
            Vec2::new(16., 32.),
            false,
            false,
        );
        part(
            torso_steady * Mat4::from_translation(Vec3::Y * 12.)
                * Mat4::from_quat(pose.head)
                * Mat4::from_translation(Vec3::Y * 4.),
            Vec3::splat(8.),
            0.,
            0.,
            Vec2::new(32., 0.),
            false,
            true,
        );
        for side in 0..2 {
            let left = side == 0;
            let sign = if left { 1. } else { -1. };
            let tr = torso_m
                * Mat4::from_translation(Vec3::new(sign * (4. + aw / 2.), 10., 0.))
                * Mat4::from_quat(pose.arms[side])
                * Mat4::from_translation(Vec3::new(0., -4., 0.));
            let (u, w, outer) = if left && !skin.legacy {
                (32., 48., Vec2::new(48., 48.))
            } else {
                (40., 16., Vec2::new(40., 32.))
            };
            part(
                tr,
                Vec3::new(aw, 12., 4.),
                u,
                w,
                outer,
                left && skin.legacy,
                false,
            );
        }
        for side in 0..2 {
            let left = side == 0;
            let sign = if left { 1. } else { -1. };
            let tr = hip
                * Mat4::from_translation(Vec3::new(sign * 2., 0., 0.))
                * Mat4::from_quat(pose.legs[side])
                * Mat4::from_translation(Vec3::new(0., -6., 0.));
            let (u, w, outer) = if left && !skin.legacy {
                (16., 48., Vec2::new(0., 48.))
            } else {
                (0., 16., Vec2::new(0., 32.))
            };
            part(
                tr,
                Vec3::new(4., 12., 4.),
                u,
                w,
                outer,
                left && skin.legacy,
                false,
            );
        }
    }
    // 披风：背挂平面盒，顶部为摆动枢轴。uv (0,0) 展开条带的前脸即 (1,1)，
    // 与原版披风纹理布局一致。
    if scene.cape.is_some() {
        v.push(BoxPart {
            size: Vec3::new(10., 16., 1.),
            uv: Vec2::ZERO,
            transform: torso_steady
                * Mat4::from_translation(Vec3::new(0., 11., -2.75))
                * Mat4::from_quat(pose::rotation(pose.cape.x, 0., pose.cape.y))
                * Mat4::from_translation(Vec3::new(0., -8., 0.)),
            inflate: 0.,
            mirror: false,
            tex: Tex::Cape,
        });
    }
    // 压扁以脚底为锚，双脚下沉为零，头顶随脉冲下压。
    let anchor = Mat4::from_translation(Vec3::Y * -12.)
        * Mat4::from_scale(Vec3::new(1., pose.squash, 1.))
        * Mat4::from_translation(Vec3::Y * 12.);
    for part in &mut v {
        part.transform = anchor * part.transform;
    }
    let global = Mat4::from_rotation_y(pose.model_yaw());
    for part in &mut v {
        part.transform = global * part.transform;
    }
    // 半透明外层最后绘制，并按相机空间深度排序（对齐 three.js 行为）。
    v.sort_by(|a, b| {
        (a.inflate > 0.).cmp(&(b.inflate > 0.)).then_with(|| {
            if a.inflate > 0. {
                let az = project(a.transform.transform_point3(Vec3::ZERO), 1., 1.).z;
                let bz = project(b.transform.transform_point3(Vec3::ZERO), 1., 1.).z;
                bz.total_cmp(&az)
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    v
}

pub struct Frame {
    pub width: usize,
    pub height: usize,
    /// 预乘 BGRA。
    pub pixels: Vec<u8>,
    depth: Vec<f32>,
    pub bounds: [i32; 4],
    resample: Option<Resample>,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            width: w,
            height: h,
            pixels: vec![0; w * h * 4],
            depth: vec![f32::INFINITY; w * h],
            bounds: [0; 4],
            resample: None,
        }
    }

    /// 组装场景并绘制一帧。
    pub fn draw_scene(&mut self, pose: &Skeleton, scene: &SceneParams, light: &Lighting) {
        let parts = parts(pose, scene);
        self.draw(&parts, scene.skin, scene.cape, light);
    }

    fn draw(&mut self, parts: &[BoxPart], skin: &Skin, cape: Option<&Texture>, light: &Lighting) {
        self.pixels.fill(0);
        self.depth.fill(f32::INFINITY);
        self.bounds = [self.width as i32, self.height as i32, 0, 0];
        for part in parts {
            let tex = match part.tex {
                Tex::Skin => &skin.texture,
                Tex::Cape => cape.expect("cape part without cape texture"),
            };
            self.draw_box(part, tex, light);
        }
    }

    fn draw_box(&mut self, b: &BoxPart, tex: &Texture, light: &Lighting) {
        let Vec3 { x: w, y: h, z: d } = b.size;
        let u = b.uv.x;
        let v = b.uv.y;
        let regions = [
            // +X 为角色左侧。展开条带顺序为 角色-右 / 前 / 角色-左 / 后。
            (u + d + w, v + d, d, h),
            (u, v + d, d, h),
            (u + d, v, w, d),
            (u + d + w, v, w, d),
            (u + d, v + d, w, h),
            (u + 2. * d + w, v + d, w, h),
        ];
        let sz = b.size + Vec3::splat(b.inflate);
        let planes = [
            (2, 1, 0, -1., -1., sz.z, sz.y, sz.x),
            (2, 1, 0, 1., -1., sz.z, sz.y, -sz.x),
            (0, 2, 1, 1., 1., sz.x, sz.z, sz.y),
            (0, 2, 1, 1., -1., sz.x, sz.z, -sz.y),
            (0, 1, 2, 1., -1., sz.x, sz.y, sz.z),
            (0, 1, 2, -1., -1., sz.x, sz.y, -sz.z),
        ];
        let normals = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        for (f, &(uax, vax, wax, ud, vd, ww, hh, dd)) in planes.iter().enumerate() {
            let mut lit = normals[f];
            if b.mirror {
                lit.x = -lit.x;
            }
            let shade = light.shade(b.transform.transform_vector3(lit).normalize());
            let lut = light.lut(shade);
            let mut pos = [Vec3::ZERO; 4];
            let mut uv = [Vec2::ZERO; 4];
            let (x, y, tw, th) = regions[f];
            for iy in 0..2 {
                for ix in 0..2 {
                    let i = iy * 2 + ix;
                    let mut a = Vec3::ZERO;
                    a[uax] = (ix as f32 * ww - ww / 2.) * ud;
                    a[vax] = (iy as f32 * hh - hh / 2.) * vd;
                    a[wax] = dd / 2.;
                    if b.mirror {
                        a.x = -a.x;
                    }
                    pos[i] = project(
                        b.transform.transform_point3(a),
                        self.width as f32,
                        self.height as f32,
                    );
                    uv[i] = Vec2::new(
                        x + ix as f32 * tw,
                        y + if f == 3 {
                            (1 - iy) as f32 * th
                        } else {
                            iy as f32 * th
                        },
                    );
                }
            }
            for ids in [[0, 2, 1], [2, 3, 1]] {
                let area = edge(pos[ids[0]], pos[ids[1]], pos[ids[2]]);
                if b.inflate == 0. && ((area >= 0.) != b.mirror) {
                    continue;
                }
                self.triangle(
                    [pos[ids[0]], pos[ids[1]], pos[ids[2]]],
                    [uv[ids[0]], uv[ids[1]], uv[ids[2]]],
                    tex,
                    lut,
                );
            }
        }
    }

    fn triangle(&mut self, p: [Vec3; 3], uv: [Vec2; 3], tex: &Texture, lut: &[[u8; 256]; 3]) {
        let area = edge(p[0], p[1], p[2]);
        if area.abs() < 0.0001 {
            return;
        }
        let minx = p
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.) as usize;
        let maxx = p
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.width as f32) as usize;
        let miny = p
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.) as usize;
        let maxy = p
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.height as f32) as usize;
        let texscale = tex.image.width() as f32 / 64.;
        let inv_area = area.recip();
        let ax = -(p[2].y - p[1].y) * inv_area;
        let ay = (p[2].x - p[1].x) * inv_area;
        let cx = -(p[1].y - p[0].y) * inv_area;
        let cy = (p[1].x - p[0].x) * inv_area;
        let origin = Vec3::new(minx as f32 + 0.5, miny as f32 + 0.5, 0.);
        let a0 = edge(p[1], p[2], origin) * inv_area;
        let c0 = edge(p[0], p[1], origin) * inv_area;
        let za = p[0].z - p[1].z;
        let zc = p[2].z - p[1].z;
        let t = uv.map(|t| t * texscale);
        let ta = t[0] - t[1];
        let tc = t[2] - t[1];
        // 定点、半开覆盖：共享边恰有一个归属者，外层斜边不重复混合。
        let points = p.map(|v| {
            (
                (v.x as f64 * 256.).round() as i64,
                (v.y as f64 * 256.).round() as i64,
            )
        });
        let edges = [(0, 1), (1, 2), (2, 0)].map(|(i, j)| {
            let (s, e) = if area > 0. {
                (points[i], points[j])
            } else {
                (points[j], points[i])
            };
            let dx = e.0 - s.0;
            let dy = e.1 - s.1;
            let bias = if dy < 0 || (dy == 0 && dx > 0) { 0 } else { -1 };
            (
                dx,
                dy,
                dx * (miny as i64 * 256 + 128 - s.1) - dy * (minx as i64 * 256 + 128 - s.0) + bias,
            )
        });
        for y in miny..maxy {
            let row_a = a0 + (y - miny) as f32 * ay;
            let row_c = c0 + (y - miny) as f32 * cy;
            let coverage = edges.map(|(dx, _, start)| start + dx * (y - miny) as i64 * 256);
            for x in minx..maxx {
                if (0..3).any(|e| coverage[e] - edges[e].1 * (x - minx) as i64 * 256 < 0) {
                    continue;
                }
                let a = row_a + (x - minx) as f32 * ax;
                let c = row_c + (x - minx) as f32 * cx;
                let z = p[1].z + za * a + zc * c;
                let i = y * self.width + x;
                if z >= self.depth[i] {
                    continue;
                }
                let tx = t[1].x + ta.x * a + tc.x * c;
                let ty = t[1].y + ta.y * a + tc.y * c;
                let xx = (tx as u32).min(tex.image.width() - 1);
                let yy = (ty as u32).min(tex.image.height() - 1);
                let ti = (yy * tex.image.width() + xx) as usize;
                let base = &tex.colors[ti];
                let color = [
                    lut[0][base[0] as usize],
                    lut[1][base[1] as usize],
                    lut[2][base[2] as usize],
                ];
                let alpha = tex.image.get_pixel(xx, yy)[3];
                if alpha < 16 {
                    continue;
                }
                let off = i * 4;
                if alpha == 255 {
                    self.pixels[off..off + 4].copy_from_slice(&[color[2], color[1], color[0], 255]);
                } else {
                    let aa = alpha as u32;
                    let inv = 255 - aa;
                    let channels = [color[2], color[1], color[0]];
                    for ch in 0..3 {
                        self.pixels[off + ch] =
                            ((channels[ch] as u32 * aa + self.pixels[off + ch] as u32 * inv + 127)
                                / 255) as u8;
                    }
                    self.pixels[off + 3] =
                        (aa + (self.pixels[off + 3] as u32 * inv + 127) / 255) as u8;
                }
                self.depth[i] = z;
                self.bounds[0] = self.bounds[0].min(x as i32);
                self.bounds[1] = self.bounds[1].min(y as i32);
                self.bounds[2] = self.bounds[2].max(x as i32 + 1);
                self.bounds[3] = self.bounds[3].max(y as i32 + 1);
            }
        }
    }

    /// 降采样/缩放并输出预乘 RGBA 到调用方缓冲。
    /// 帧大于目标时盒式平均（预乘空间直接平均即抗锯齿），
    /// 帧小于目标时最近邻放大。
    pub fn present_into(&mut self, out: &mut [u8], w: usize, h: usize) {
        debug_assert_eq!(out.len(), w * h * 4);
        if self.width == w && self.height == h {
            for (src, dst) in self.pixels.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
                dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
            }
        } else if self.width > w && self.height > h {
            if self.resample.as_ref().map(|r| r.source) != Some((self.width, self.height)) {
                self.resample = Some(Resample {
                    source: (self.width, self.height),
                    columns: box_filter(self.width, w),
                    rows: box_filter(self.height, h),
                });
            }
            let resample = self.resample.as_ref().unwrap();
            for (y, row) in resample.rows.iter().enumerate() {
                for (x, col) in resample.columns.iter().enumerate() {
                    let mut acc = [0u32; 4];
                    let mut total = 0u32;
                    for (dy, &wy) in row.factors.iter().enumerate() {
                        let src =
                            self.pixels[(row.start + dy) * self.width * 4 + col.start * 4..].as_ptr();
                        for (dx, &wx) in col.factors.iter().enumerate() {
                            let weight = wx * wy;
                            total += weight;
                            let px = unsafe { src.add(dx * 4) };
                            for ch in 0..4 {
                                acc[ch] += unsafe { *px.add(ch) } as u32 * weight;
                            }
                        }
                    }
                    let out_off = (y * w + x) * 4;
                    out[out_off..out_off + 4].copy_from_slice(&[
                        ((acc[2] + total / 2) / total) as u8,
                        ((acc[1] + total / 2) / total) as u8,
                        ((acc[0] + total / 2) / total) as u8,
                        ((acc[3] + total / 2) / total) as u8,
                    ]);
                }
            }
        } else {
            let mut last_sy = usize::MAX;
            for y in 0..h {
                let sy = y * self.height / h;
                let dst_off = y * w * 4;
                if sy == last_sy {
                    out.copy_within(dst_off - w * 4..dst_off, dst_off);
                } else {
                    last_sy = sy;
                    for x in 0..w {
                        let sx = x * self.width / w;
                        let src = (sy * self.width + sx) * 4;
                        out[dst_off + x * 4..dst_off + x * 4 + 4]
                            .copy_from_slice(&[
                                self.pixels[src + 2],
                                self.pixels[src + 1],
                                self.pixels[src],
                                self.pixels[src + 3],
                            ]);
                    }
                }
            }
        }
    }
}

fn edge(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

struct Weights {
    start: usize,
    factors: Vec<u32>,
}

/// 定点（256/像素）盒式滤波权重表：每个目标像素的源覆盖恰好铺满一次。
fn box_filter(src: usize, dst: usize) -> Vec<Weights> {
    (0..dst)
        .map(|i| {
            let left = i as u64 * src as u64 * 256 / dst as u64;
            let right = (i as u64 + 1) * src as u64 * 256 / dst as u64;
            let start = (left / 256) as usize;
            let end = right.div_ceil(256) as usize;
            let factors = (start..end)
                .map(|j| (((j as u64 + 1) * 256).min(right) - (j as u64 * 256).max(left)) as u32)
                .collect();
            Weights { start, factors }
        })
        .collect()
}

struct Resample {
    source: (usize, usize),
    columns: Vec<Weights>,
    rows: Vec<Weights>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_filter_tiles_the_source_exactly_once() {
        for (src, dst) in [(900usize, 360usize), (630, 420), (540, 270), (3, 7)] {
            let filter = box_filter(src, dst);
            assert_eq!(filter.len(), dst);
            let mut covered = vec![0u64; src];
            for entry in &filter {
                assert!(entry.start < src);
                for (j, &factor) in entry.factors.iter().enumerate() {
                    assert!(factor > 0);
                    covered[entry.start + j] += factor as u64;
                }
            }
            assert!(covered.iter().all(|&c| c == 256), "{src}->{dst}: {covered:?}");
        }
    }

    #[test]
    fn downsample_averages_premultiplied_colors() {
        let mut frame = Frame::new(4, 4);
        // 两行不透明红（预乘 BGRA），两行半透明红。
        for y in 0..4 {
            for x in 0..4 {
                let off = (y * 4 + x) * 4;
                let v = if y < 2 { [0, 0, 255, 255] } else { [0, 0, 128, 128] };
                frame.pixels[off..off + 4].copy_from_slice(&v);
            }
        }
        let mut out = vec![0u8; 2 * 2 * 4];
        frame.present_into(&mut out, 2, 2);
        // 上行 = 不透明红；下行 = 一半透明 → 预乘减半。输出为 RGBA。
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out[4..8], &[255, 0, 0, 255]);
        assert_eq!(&out[8..12], &[128, 0, 0, 128]);
    }
}
