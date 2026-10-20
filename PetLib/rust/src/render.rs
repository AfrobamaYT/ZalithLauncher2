//! 软件光栅化：盒体部件 → 投影 → 三角形覆盖 → z-buffer 逐像素混合，
//! 末端经盒式降采样输出预乘 RGBA（Android Bitmap 原生格式）。

use glam::{Mat4, Vec2, Vec3};
use std::sync::OnceLock;

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
    pub size: Vec3,
    pub uv: Vec2,
    pub transform: Mat4,
    pub inflate: f32,
    pub mirror: bool,
    pub tex: Tex,
}

pub struct SceneParams<'a> {
    pub skin: &'a Skin,
    pub cape: Option<&'a Texture>,
    pub outer_layer: bool,
}

pub(crate) fn parts(pose: &Skeleton, scene: &SceneParams) -> Vec<BoxPart> {
    let skin = scene.skin;
    let slim = skin.arm == ArmModel::Slim && !skin.legacy;
    let aw = if slim { 3. } else { 4. };
    // 各部件互为兄弟节点：躯干方块的旋转与位置增量不带动臂/腿/披风。
    // 枢轴与几何偏移以模型像素为单位。
    let mut v: Vec<BoxPart> = Vec::with_capacity(13);
    {
        let mut part =
            |transform: Mat4, size: Vec3, u: f32, w: f32, outer: Vec2, mirror: bool, hat: bool| {
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
        // 躯干：部件原点即盒心（skin 0,-6,0 → 模型 0,18,0），旋转绕盒心。
        part(
            Mat4::from_translation(Vec3::new(0., 18., 0.) + pose.body_pos)
                * Mat4::from_quat(pose.body_rot)
                * Mat4::from_rotation_y(pose.body_yaw),
            Vec3::new(8., 12., 4.),
            16.,
            16.,
            Vec2::new(16., 32.),
            false,
            false,
        );
        // 头部：枢轴在颈部（模型 0,24,0），盒心上移 4。
        part(
            Mat4::from_translation(Vec3::new(0., 24. + pose.head_pos, 0.))
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
            // 臂：部件原点 (∓5,22,0)（肩点，盒顶下方 2），枢轴组内偏移
            // (∓1,−4,0)（纤细 ∓0.5），盒心挂在枢轴上。
            let pivot_x = if slim { 0.5 } else { 1. };
            let tr = Mat4::from_translation(Vec3::new(sign * 5., 22., 0.))
                * Mat4::from_quat(pose.arms[side])
                * Mat4::from_translation(Vec3::new(sign * pivot_x, -4., 0.));
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
            // 腿：部件原点 (∓1.9,12,−0.1)（髋点），枢轴组内偏移 (0,−6,0)。
            let tr = Mat4::from_translation(Vec3::new(sign * 1.9, 12., -0.1))
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
    // 披风：部件原点 (0,24,−2)，mesh 偏移 (0,−8,+0.5)；rotation.y 恒为 π
    if scene.cape.is_some() {
        v.push(BoxPart {
            size: Vec3::new(10., 16., 1.),
            uv: Vec2::ZERO,
            transform: Mat4::from_translation(Vec3::new(0., 24., -2.))
                * Mat4::from_quat(pose::rotation(
                    pose.cape.x,
                    std::f32::consts::PI,
                    pose.cape.y,
                ))
                * Mat4::from_translation(Vec3::new(0., -8., 0.5)),
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
        // 只清上一帧写入过的矩形：每帧写入都落在当帧 bounds 内，
        // 逐帧接力清除即覆盖全部历史写入，区域外像素从未被触碰、保持为零。
        let left = self.bounds[0].clamp(0, self.width as i32) as usize;
        let right = self.bounds[2].clamp(0, self.width as i32) as usize;
        let top = self.bounds[1].clamp(0, self.height as i32) as usize;
        let bottom = self.bounds[3].clamp(0, self.height as i32) as usize;
        if left < right && top < bottom {
            for y in top..bottom {
                let row = y * self.width;
                self.pixels[(row + left) * 4..(row + right) * 4].fill(0);
                self.depth[row + left..row + right].fill(f32::INFINITY);
            }
        }
        let empty = [self.width as i32, self.height as i32, 0, 0];
        self.bounds = empty;

        let threads = if parts.is_empty() {
            1
        } else {
            raster_threads(self.width, self.height)
        };
        if threads <= 1 {
            let mut band = Band {
                pixels: &mut self.pixels[..],
                depth: &mut self.depth[..],
                width: self.width,
                height: self.height,
                start_row: 0,
                end_row: self.height,
                bounds: empty,
            };
            for part in parts {
                let tex = match part.tex {
                    Tex::Skin => &skin.texture,
                    Tex::Cape => cape.expect("cape part without cape texture"),
                };
                draw_box(&mut band, part, tex, light);
            }
            self.bounds = band.bounds;
            return;
        }

        let width = self.width;
        let height = self.height;
        let band_count = threads * 4;
        let rows_per_band = height.div_ceil(band_count);
        let mut groups: Vec<Vec<Band>> = (0..threads).map(|_| Vec::new()).collect();
        let mut rest_pixels = self.pixels.as_mut_slice();
        let mut rest_depth = self.depth.as_mut_slice();
        let mut start_row = 0;
        let mut index = 0;
        while start_row < height {
            let end_row = (start_row + rows_per_band).min(height);
            let (band_pixels, tail_pixels) = rest_pixels.split_at_mut((end_row - start_row) * width * 4);
            let (band_depth, tail_depth) = rest_depth.split_at_mut((end_row - start_row) * width);
            rest_pixels = tail_pixels;
            rest_depth = tail_depth;
            groups[index % threads].push(Band {
                pixels: band_pixels,
                depth: band_depth,
                width,
                height,
                start_row,
                end_row,
                bounds: empty,
            });
            index += 1;
            start_row = end_row;
        }
        self.bounds = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(groups.len());
            for group in groups {
                handles.push(scope.spawn(move || {
                    let mut acc = empty;
                    for mut band in group {
                        for part in parts {
                            let tex = match part.tex {
                                Tex::Skin => &skin.texture,
                                Tex::Cape => cape.expect("cape part without cape texture"),
                            };
                            draw_box(&mut band, part, tex, light);
                        }
                        acc = merge_bounds(acc, band.bounds);
                    }
                    acc
                }));
            }
            handles
                .into_iter()
                .map(|handle| handle.join().expect("raster thread panicked"))
                .reduce(merge_bounds)
                .unwrap_or(empty)
        });
    }
}

/// 一段行区间及其独占的颜色/深度切片
struct Band<'a> {
    pixels: &'a mut [u8],
    depth: &'a mut [f32],
    width: usize,
    height: usize,
    start_row: usize,
    end_row: usize,
    /// 区间内的写入包围盒
    bounds: [i32; 4],
}

fn merge_bounds(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

#[cfg(test)]
static TEST_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 光栅线程数：小帧不值得分片开销；条带轮转下最慢的核心决定整帧
/// 尾延迟，大小核异构设备只用大核，避免小核拖尾。
fn raster_threads(width: usize, height: usize) -> usize {
    #[cfg(test)]
    {
        let forced = TEST_THREADS.load(std::sync::atomic::Ordering::Relaxed);
        if forced != 0 {
            return forced;
        }
    }
    if width * height < 64 * 1024 {
        return 1;
    }
    static BIG_CORES: OnceLock<Option<usize>> = OnceLock::new();
    let available = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
    match *BIG_CORES.get_or_init(detect_big_cores) {
        Some(big) if big >= 2 => big.min(available),
        _ => available,
    }
}

/// 大核探测：优先读内核归一化算力 cpu_capacity，缺失时按核心最高
/// 频率之比估计（≥ 最快核心 75% 视为大核）；都读不到返回 None，
/// 按全部核心处理。
fn detect_big_cores() -> Option<usize> {
    let mut capacities = Vec::new();
    let mut freqs = Vec::new();
    for cpu in 0..16u32 {
        if let Some(v) =
            read_sysfs_f32(format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity"))
        {
            capacities.push(v);
        }
        if let Some(v) =
            read_sysfs_u32(format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq"))
        {
            freqs.push(v);
        }
    }
    if capacities.len() >= 4 {
        let max = capacities.iter().cloned().fold(0., f32::max);
        return Some(capacities.iter().filter(|&&c| c >= max * 0.75).count());
    }
    if freqs.len() >= 4 {
        let max = *freqs.iter().max()?;
        return Some(freqs.iter().filter(|&&f| u64::from(f) * 4 >= u64::from(max) * 3).count());
    }
    None
}

fn read_sysfs_u32(path: String) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_sysfs_f32(path: String) -> Option<f32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn draw_box(band: &mut Band, b: &BoxPart, tex: &Texture, light: &Lighting) {
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
                        band.width as f32,
                        band.height as f32,
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
                triangle(
                    band,
                    [pos[ids[0]], pos[ids[1]], pos[ids[2]]],
                    [uv[ids[0]], uv[ids[1]], uv[ids[2]]],
                    tex,
                    lut,
                );
            }
        }
}

fn triangle(band: &mut Band, p: [Vec3; 3], uv: [Vec2; 3], tex: &Texture, lut: &[[u8; 256]; 3]) {
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
            .min(band.width as f32) as usize;
        let miny = p
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.)
            .max(band.start_row as f32) as usize;
        let maxy = p
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(band.height as f32)
            .min(band.end_row as f32) as usize;
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
            let row = (y - band.start_row) * band.width;
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
                let i = row + x;
                if z >= band.depth[i] {
                    continue;
                }
                let tx = t[1].x + ta.x * a + tc.x * c;
                let ty = t[1].y + ta.y * a + tc.y * c;
                let xx = (tx as u32).min(tex.image.width() - 1);
                let yy = (ty as u32).min(tex.image.height() - 1);
                let ti = (yy * tex.image.width() + xx) as usize;
                let base = &tex.colors[ti];
                if base[3] < 16 {
                    continue;
                }
                let alpha = base[3];
                let color = [
                    lut[0][base[0] as usize],
                    lut[1][base[1] as usize],
                    lut[2][base[2] as usize],
                ];
                let off = i * 4;
                if alpha == 255 {
                    band.pixels[off..off + 4].copy_from_slice(&[color[2], color[1], color[0], 255]);
                } else {
                    let aa = alpha as u32;
                    let inv = 255 - aa;
                    let channels = [color[2], color[1], color[0]];
                    for ch in 0..3 {
                        band.pixels[off + ch] =
                            ((channels[ch] as u32 * aa + band.pixels[off + ch] as u32 * inv + 127)
                                / 255) as u8;
                    }
                    band.pixels[off + 3] =
                        (aa + (band.pixels[off + 3] as u32 * inv + 127) / 255) as u8;
                }
                band.depth[i] = z;
                band.bounds[0] = band.bounds[0].min(x as i32);
                band.bounds[1] = band.bounds[1].min(y as i32);
                band.bounds[2] = band.bounds[2].max(x as i32 + 1);
                band.bounds[3] = band.bounds[3].max(y as i32 + 1);
            }
        }
}

impl Frame {
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
            let threads = raster_threads(w, h);
            if self.width == w * 2 && self.height == h * 2 {
                // 精确 2×2 降采样：每个源像素权重恰为 256/256，与通用权重路径
                // 等价，直接四像素求和免去逐权重乘加。
                let pixels = &self.pixels[..];
                let src_width = self.width;
                parallel_rows(out, w, h, threads, |rows, y0| {
                    downsample_exact2(pixels, src_width, rows, w, y0)
                });
            } else {
                if self.resample.as_ref().map(|r| r.source) != Some((self.width, self.height)) {
                    self.resample = Some(Resample {
                        source: (self.width, self.height),
                        columns: box_filter(self.width, w),
                        rows: box_filter(self.height, h),
                    });
                }
                let pixels = &self.pixels[..];
                let src_width = self.width;
                let resample = self.resample.as_ref().unwrap();
                parallel_rows(out, w, h, threads, |rows, y0| {
                    downsample_weighted(pixels, src_width, resample, rows, w, y0)
                });
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
                        out[dst_off + x * 4..dst_off + x * 4 + 4].copy_from_slice(&[
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

/// 按输出行区间并行执行 `draw`：区间互不重叠，各行结果只依赖自身行号。
fn parallel_rows(
    out: &mut [u8],
    w: usize,
    h: usize,
    threads: usize,
    draw: impl Fn(&mut [u8], usize) + Sync,
) {
    if threads <= 1 {
        draw(out, 0);
        return;
    }
    let draw = &draw;
    let rows_per_band = h.div_ceil(threads);
    std::thread::scope(|scope| {
        let mut rest = out;
        let mut start = 0;
        while start < h {
            let end = (start + rows_per_band).min(h);
            let (chunk, tail) = rest.split_at_mut((end - start) * w * 4);
            rest = tail;
            scope.spawn(move || draw(chunk, start));
            start = end;
        }
    });
}

/// 目标第 [y0, y0 + 行数) 行的精确 2×2 盒式平均。
fn downsample_exact2(pixels: &[u8], src_width: usize, out: &mut [u8], w: usize, y0: usize) {
    let rows = out.len() / (w * 4);
    for row in 0..rows {
        let y = y0 + row;
        let src_row = (y * 2 * src_width) * 4;
        for x in 0..w {
            let l = x * 8;
            let out_off = (row * w + x) * 4;
            for (dst_ch, src_ch) in [(0usize, 2usize), (1, 1), (2, 0), (3, 3)] {
                let sum = u32::from(pixels[src_row + l + src_ch])
                    + u32::from(pixels[src_row + l + 4 + src_ch])
                    + u32::from(pixels[src_row + src_width * 4 + l + src_ch])
                    + u32::from(pixels[src_row + src_width * 4 + l + 4 + src_ch]);
                out[out_off + dst_ch] = ((sum + 2) >> 2) as u8;
            }
        }
    }
}

/// 目标第 [y0, y0 + 行数) 行的权重盒式平均。
fn downsample_weighted(
    pixels: &[u8],
    src_width: usize,
    resample: &Resample,
    out: &mut [u8],
    w: usize,
    y0: usize,
) {
    let rows = out.len() / (w * 4);
    for (row, weights) in resample.rows[y0..y0 + rows].iter().enumerate() {
        for (x, col) in resample.columns.iter().enumerate() {
            let mut acc = [0u32; 4];
            let mut total = 0u32;
            for (dy, &wy) in weights.factors.iter().enumerate() {
                let src = pixels[(weights.start + dy) * src_width * 4 + col.start * 4..].as_ptr();
                for (dx, &wx) in col.factors.iter().enumerate() {
                    let weight = wx * wy;
                    total += weight;
                    let px = unsafe { src.add(dx * 4) };
                    for ch in 0..4 {
                        acc[ch] += unsafe { *px.add(ch) } as u32 * weight;
                    }
                }
            }
            let out_off = (row * w + x) * 4;
            out[out_off..out_off + 4].copy_from_slice(&[
                ((acc[2] + total / 2) / total) as u8,
                ((acc[1] + total / 2) / total) as u8,
                ((acc[0] + total / 2) / total) as u8,
                ((acc[3] + total / 2) / total) as u8,
            ]);
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
    use std::io::Cursor;

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
            assert!(
                covered.iter().all(|&c| c == 256),
                "{src}->{dst}: {covered:?}"
            );
        }
    }

    #[test]
    fn downsample_averages_premultiplied_colors() {
        let mut frame = Frame::new(4, 4);
        // 两行不透明红（预乘 BGRA），两行半透明红。
        for y in 0..4 {
            for x in 0..4 {
                let off = (y * 4 + x) * 4;
                let v = if y < 2 {
                    [0, 0, 255, 255]
                } else {
                    [0, 0, 128, 128]
                };
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

    #[test]
    fn downsample_exact_double_matches_weighted_reference() {
        let mut frame = Frame::new(4, 4);
        for (i, px) in frame.pixels.chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[
                (i * 53 % 256) as u8,
                (i * 97 % 256) as u8,
                (i * 29 % 256) as u8,
                (i * 11 % 255) as u8,
            ]);
        }
        let mut out = vec![0u8; 2 * 2 * 4];
        frame.present_into(&mut out, 2, 2);
        // 参考值：按通用盒式滤波权重（每源像素 256/256）逐通道独立计算。
        for y in 0..2 {
            for x in 0..2 {
                let mut expected = [0u8; 4];
                for (dst_ch, src_ch) in [(0usize, 2usize), (1, 1), (2, 0), (3, 3)] {
                    let mut acc = 0u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let si = ((y * 2 + dy) * 4 + x * 2 + dx) * 4 + src_ch;
                            acc += u32::from(frame.pixels[si]) * 65536;
                        }
                    }
                    expected[dst_ch] = ((acc + 131072) / 262144) as u8;
                }
                let off = (y * 2 + x) * 4;
                assert_eq!(&out[off..off + 4], &expected, "pixel ({x},{y})");
            }
        }
    }

    /// 矩形清屏的不变量：零写入的一帧必须不留任何上一帧痕迹。
    #[test]
    fn draw_clears_everything_left_by_the_previous_frame() {
        let mut img = image::RgbaImage::new(64, 64);
        for p in img.pixels_mut() {
            *p = image::Rgba([255, 0, 0, 255]);
        }
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let skin = Skin::load(&png, None).unwrap();
        let light = Lighting::new(&crate::api::LightingParams::default());

        let mut frame = Frame::new(8, 8);
        // 人为留下上一帧的写入痕迹与深度
        for y in 1..3 {
            for x in 2..5 {
                let off = (y * 8 + x) * 4;
                frame.pixels[off..off + 4].copy_from_slice(&[9, 9, 9, 255]);
                frame.depth[y * 8 + x] = 1.;
            }
        }
        frame.bounds = [2, 1, 5, 3];
        frame.draw(&[], &skin, None, &light);
        assert!(frame.pixels.iter().all(|&v| v == 0));
        assert!(frame.depth.iter().all(|d| d.is_infinite()));
    }

    /// 并行条带与单线程必须产出逐字节一致的帧。
    #[test]
    fn parallel_raster_matches_serial_output() {
        use std::sync::atomic::Ordering;

        fn skin_png() -> Vec<u8> {
            let mut img = image::RgbaImage::new(64, 64);
            for p in img.pixels_mut() {
                *p = image::Rgba([120, 90, 60, 255]);
            }
            let mut png = Vec::new();
            img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            png
        }

        fn stream(threads: usize) -> Vec<u8> {
            TEST_THREADS.store(threads, Ordering::Relaxed);
            let mut model =
                crate::api::PetModel::new(&skin_png(), None).expect("model");
            model.play_animation(crate::animation::AnimationId::Walking, 1., true);
            let (w, h) = (150usize, 170usize);
            let mut out = vec![0u8; w * h * 4];
            for i in 0..40 {
                model
                    .render(w, h, i as f32 / 60., 1. / 60., &mut out)
                    .unwrap();
            }
            out
        }

        let serial = stream(1);
        let parallel = stream(4);
        TEST_THREADS.store(0, Ordering::Relaxed);
        assert_eq!(serial, parallel);
    }
}
