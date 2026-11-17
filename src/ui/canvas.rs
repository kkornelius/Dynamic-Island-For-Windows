use super::graphics::Graphics;
use crate::{media::Artwork, storage::DockMode};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::*,
            Gdi::*,
        },
        UI::WindowsAndMessaging::*,
    },
    core::*,
};
use windows_numerics::{Matrix3x2, Vector2};

pub const BG: u32 = 0x000000;
pub const CARD: u32 = 0x1c1c1c;
pub const LINE: u32 = 0x333333;
pub const WHITE: u32 = 0xf5f5f5;
pub const MUTED: u32 = 0xa1a1a1;
pub const DIM: u32 = 0x737373;
pub const SPOTIFY: u32 = 0x1ed760;
fn rgba(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 255) as f32 / 255.0,
        g: ((rgb >> 8) & 255) as f32 / 255.0,
        b: (rgb & 255) as f32 / 255.0,
        a: 1.0,
    }
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x as f32,
        top: y as f32,
        right: (x + w) as f32,
        bottom: (y + h) as f32,
    }
}
struct MaskCache {
    radius: i32,
    mode: DockMode,
    style: Option<(u32, f32, f32)>,
    width: i32,
    top: Vec<Vec<EdgePixel>>,
    bottom: Vec<Vec<EdgePixel>>,
    sides: Vec<EdgePixel>,
}
struct EdgePixel {
    index: usize,
    distance: f32,
    color_weights: [u16; 4],
    retain: f32,
    light: [f32; 3],
    alpha: u8,
}
struct GlassStyle {
    base: [f32; 3],
    tint: [f32; 3],
    emission: [f32; 3],
    width: f32,
    strength: f32,
}
impl GlassStyle {
    fn new((color, width, strength): (u32, f32, f32)) -> Self {
        let base = std::array::from_fn(|k| ((color >> (k * 8)) & 255) as f32);
        Self {
            base,
            tint: base.map(|c| (c / 255.).powf(1.15) * 255.),
            emission: base.map(|c| (c / 255.).powf(1.45) * 255.),
            width,
            strength,
        }
    }
}
fn gaussian(value: f32, width: f32) -> f32 {
    (-0.5 * (value / width).powi(2)).exp()
}
#[derive(Clone, Copy, Default)]
struct Kernel {
    inside: f32,
    rim: f32,
    sharp: f32,
    shoulder: f32,
    outer: f32,
}
fn shape_style(s: Option<(u32, f32, f32)>) -> Option<(u32, u32)> {
    s.map(|(_, w, g)| (w.to_bits(), g.to_bits()))
}
struct GlassLut {
    radius: i32,
    style: Option<(u32, f32, f32)>,
    palette: Option<GlassStyle>,
    minimum: f32,
    samples: Vec<Kernel>,
}
impl GlassLut {
    fn new(radius: i32, style: Option<(u32, f32, f32)>) -> Self {
        let r = radius.max(1) as f32;
        let minimum = -(r * 1.5).max(48.0) - 1.0;
        let palette = style.map(GlassStyle::new);
        let count = ((super::HALO as f32 + 1.0 - minimum) * 64.0).ceil() as usize + 1;
        let samples = (0..count)
            .map(|i| {
                let d = minimum + i as f32 / 64.0;
                let depth = (-d).max(0.0);
                let distance = d.max(0.0);
                let Some(p) = palette.as_ref() else {
                    return Kernel::default();
                };
                let fade = ((super::HALO as f32 - distance) / 4.0).clamp(0.0, 1.0);
                let fade = fade * fade * (3.0 - 2.0 * fade);
                Kernel {
                    inside: p.strength
                        * (1.15 * (-depth / (r * 0.245)).exp() + 0.12 * gaussian(depth, r * 0.060)),
                    rim: gaussian(d + 0.20, 0.55 + 0.12 * p.width) * 0.45,
                    sharp: gaussian(d + 0.48 + 0.12 * p.width, 0.28 + 0.065 * p.width),
                    shoulder: (0.90 * gaussian(depth, r * 0.038)
                        + 0.07 * gaussian(depth, r * 0.082))
                        * p.strength,
                    outer: p.strength
                        * (0.65 * (-distance / (r * 0.22)).exp()
                            + 0.55 * (-distance / (r * 0.055)).exp())
                        * fade,
                }
            })
            .collect();
        Self {
            radius,
            style,
            palette,
            minimum,
            samples,
        }
    }
    fn sample(&self, d: f32) -> Kernel {
        let at = ((d - self.minimum) * 64.0).clamp(0.0, (self.samples.len() - 2) as f32);
        let i = at as usize;
        let f = at - i as f32;
        let a = self.samples[i];
        let b = self.samples[i + 1];
        let mix = |x: f32, y: f32| x + (y - x) * f;
        Kernel {
            inside: mix(a.inside, b.inside),
            rim: mix(a.rim, b.rim),
            sharp: mix(a.sharp, b.sharp),
            shoulder: mix(a.shoulder, b.shoulder),
            outer: mix(a.outer, b.outer),
        }
    }
    fn edge(&self, index: usize, sd: f32, scale: f32, reflection: f32) -> EdgePixel {
        let coverage = (0.5 - sd).clamp(0.0, 1.0);
        let mut result = EdgePixel {
            index,
            distance: sd,
            color_weights: [0; 4],
            retain: coverage,
            light: [0.0; 3],
            alpha: (coverage * 255.0 + 0.5) as u8,
        };
        let Some(p) = self.palette.as_ref() else {
            return result;
        };
        let d = sd / scale;
        if d >= super::HALO as f32 {
            return result;
        }
        let k = self.sample(d);
        let inside = (k.inside * reflection).clamp(0.0, 0.99);
        let sharp = k.sharp * (0.45 + 0.55 * reflection);
        let shoulder = (k.shoulder * reflection).clamp(0.0, 0.96);
        let core = 1.0 - (1.0 - sharp) * (1.0 - shoulder);
        let outer = (k.outer * reflection).min(0.98) * (1.0 - coverage);
        result.retain = (1.0 - inside) * (1.0 - k.rim) * (1.0 - core) * coverage;
        let q = 0.72 + 0.25 * reflection;
        result.color_weights = [
            inside * (1.0 - k.rim) * (1.0 - core) * coverage,
            (k.rim * (1.0 - core) + (1.0 - q) * core) * coverage,
            q * core * coverage,
            outer,
        ]
        .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16);
        for c in 0..3 {
            let pearl = p.base[c] + (255.0 - p.base[c]) * (0.72 + 0.25 * reflection);
            result.light[c] = (p.tint[c] * inside * (1.0 - k.rim) * (1.0 - core)
                + p.base[c] * k.rim * (1.0 - core)
                + pearl * core)
                * coverage
                + p.emission[c] * outer;
        }
        result.alpha = ((coverage + outer) * 255.0 + 0.5) as u8;
        result
    }
}

fn shape_distance(px: f32, py: f32, w: f32, h: f32, r: f32, mode: DockMode, shoulder: f32) -> f32 {
    if mode == DockMode::Floating {
        let qx = (px - w / 2.0).abs() - (w / 2.0 - r);
        let qy = (py - h / 2.0).abs() - (h / 2.0 - r);
        if qx > 0.0 && qy > 0.0 {
            qx.hypot(qy) - r
        } else {
            qx.max(qy) - r
        }
    } else {
        let side = px.min(w - px);
        let edge = if py < shoulder {
            shoulder - side.max(0.0).hypot(py - shoulder)
        } else if py > h - r {
            (side - (shoulder + r)).min(0.0).hypot(py - (h - r)) - r
        } else {
            shoulder - side
        };
        edge.max(-py).max(py - h).max(-side)
    }
}
#[inline(always)]
fn composite(pixels: &mut [u8], offset: usize, edge: &EdgePixel, scale: f32) {
    let i = offset + edge.index;
    if edge.alpha == 0 {
        pixels[i..i + 4].fill(0);
        return;
    }
    if edge.retain == 1.0 && edge.alpha == 255 {
        return;
    }
    let foreground = if edge.distance < -3.0 * scale {
        let peak = pixels[i].max(pixels[i + 1]).max(pixels[i + 2]) as f32;
        if peak >= 176.0 {
            return;
        }
        ((peak - 96.0) / 80.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let retain = edge.retain * (1.0 - foreground) + foreground;
    for k in 0..3 {
        pixels[i + k] = ((pixels[i + k] as f32 * retain + edge.light[k] * (1.0 - foreground))
            .clamp(0.0, edge.alpha as f32)
            + 0.5) as u8;
    }
    pixels[i + 3] = edge.alpha;
}

type LayoutKey = (String, i32, i32, i32, bool, bool, bool);
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
pub struct Canvas {
    pub bits: *mut u8,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
    pub stride: i32,
    capacity_height: i32,
    pub profiling: bool,
    pub timings: Cell<[f64; 3]>,
    graphics: Rc<Graphics>,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    drawing: Cell<bool>,
    layouts: HashMap<LayoutKey, IDWriteTextLayout>,
    note_layout: Option<(String, IDWriteTextLayout)>,
    artwork: Option<(Arc<Artwork>, ID2D1Bitmap)>,
    color_wheel: Option<(u32, ID2D1Bitmap)>,
    border: Cell<Option<(u32, f32, f32)>>,
    mask_cache: RefCell<Option<MaskCache>>,
    partial: Cell<Option<[i32; 4]>>,
    border_luts: RefCell<Vec<GlassLut>>,
    surface: Surface,
}
impl Canvas {
    pub fn new(width: i32, height: i32, scale: f32) -> Result<Self> {
        Self::with_capacity(width, height, width, height, scale)
    }
    pub fn with_capacity(
        width: i32,
        height: i32,
        capacity_width: i32,
        capacity_height: i32,
        scale: f32,
    ) -> Result<Self> {
        unsafe {
            let graphics = Graphics::shared()?;
            let pw = (width as f32 * scale).round() as i32;
            let ph = (height as f32 * scale).round() as i32;
            let cw = (capacity_width.max(width) as f32 * scale).round() as i32;
            let ch = (capacity_height.max(height) as f32 * scale).round() as i32;
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(Error::from_thread());
            }
            let info = bitmap_info(cw, ch);
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(b) => b,
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            let old = SelectObject(dc, bitmap.into());
            let opened = (|| -> Result<(ID2D1DCRenderTarget, ID2D1SolidColorBrush)> {
                let target =
                    graphics
                        .drawing
                        .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                            pixelFormat: D2D1_PIXEL_FORMAT {
                                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                alphaMode: D2D1_ALPHA_MODE_IGNORE,
                            },
                            dpiX: 96.0 * scale,
                            dpiY: 96.0 * scale,
                            ..Default::default()
                        })?;
                target.BindDC(
                    dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: cw,
                        bottom: ch,
                    },
                )?;
                target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                let brush = target.CreateSolidColorBrush(&rgba(WHITE), None)?;
                Ok((target, brush))
            })();
            let (target, brush) = match opened {
                Ok(o) => o,
                Err(e) => {
                    SelectObject(dc, old);
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            Ok(Self {
                bits: bits.cast(),
                width: pw,
                height: ph,
                stride: cw,
                capacity_height: ch,
                profiling: false,
                timings: Cell::new([0.0; 3]),
                scale,
                graphics,
                target,
                brush,
                drawing: Cell::new(false),
                layouts: HashMap::new(),
                note_layout: None,
                artwork: None,
                color_wheel: None,
                border: Cell::new(None),
                mask_cache: RefCell::new(None),
                partial: Cell::new(None),
                border_luts: RefCell::new(Vec::new()),
                surface: Surface { dc, bitmap, old },
            })
        }
    }
    pub fn resize(&mut self, width: i32, height: i32) -> bool {
        let w = (width as f32 * self.scale).round() as i32;
        let h = (height as f32 * self.scale).round() as i32;
        if w < 1 || h < 1 || w > self.stride || h > self.capacity_height {
            return false;
        }
        self.width = w;
        self.height = h;
        true
    }
    pub fn set_partial(&self, region: Option<[i32; 4]>) {
        self.partial.set(region);
    }
    pub fn visible(&self, r: [i32; 4]) -> bool {
        self.partial.get().is_none_or(|p| {
            r[0] < p[0] + p[2] && r[0] + r[2] > p[0] && r[1] < p[1] + p[3] && r[1] + r[3] > p[1]
        })
    }
    pub fn begin(&self, origin: i32, top: i32) {
        unsafe {
            self.target.BeginDraw();
            self.drawing.set(true);
            self.target.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: origin as f32,
                M32: top as f32,
            });
            if let Some([x, y, w, h]) = self.partial.get() {
                let clip = D2D_RECT_F {
                    left: ((x + origin) as f32 * self.scale).floor() / self.scale - origin as f32,
                    top: ((y + top) as f32 * self.scale).floor() / self.scale - top as f32,
                    right: ((x + w + origin) as f32 * self.scale).ceil() / self.scale
                        - origin as f32,
                    bottom: ((y + h + top) as f32 * self.scale).ceil() / self.scale - top as f32,
                };
                self.target
                    .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
                self.brush.SetColor(&rgba(BG));
                self.target.FillRectangle(&clip, &self.brush);
            } else {
                self.target.Clear(Some(&rgba(BG)));
            }
        }
    }
    pub fn finish(&self) -> Result<()> {
        if self.drawing.replace(false) {
            unsafe {
                if self.partial.get().is_some() {
                    self.target.PopAxisAlignedClip();
                }
                self.target.EndDraw(None, None)?;
            }
        }
        Ok(())
    }
    pub fn round(&self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32) {
        if !self.visible([x, y, w, h]) {
            return;
        }
        unsafe {
            self.brush.SetColor(&rgba(c));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(x, y, w, h),
                    radiusX: r as f32,
                    radiusY: r as f32,
                },
                &self.brush,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn gradient(&self, x: i32, y: i32, w: i32, h: i32, r: i32, top: u32, bottom: u32) {
        if !self.visible([x, y, w, h]) {
            return;
        }
        unsafe {
            let draw = (|| -> Result<()> {
                let stops = self.target.CreateGradientStopCollection(
                    &[
                        D2D1_GRADIENT_STOP {
                            position: 0.0,
                            color: rgba(top),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.0,
                            color: rgba(bottom),
                        },
                    ],
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )?;
                let brush = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 {
                            X: x as f32,
                            Y: y as f32,
                        },
                        endPoint: Vector2 {
                            X: (x + w / 3) as f32,
                            Y: (y + h) as f32,
                        },
                    },
                    None,
                    &stops,
                )?;
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: r as f32,
                        radiusY: r as f32,
                    },
                    &brush,
                );
                Ok(())
            })();
            if draw.is_err() {
                self.round(x, y, w, h, r, top);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        size: i32,
        bold: bool,
        c: u32,
        center: bool,
    ) {
        self.text_inner(text, x, y, w, h, size, bold, c, center, false);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn text_block(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        size: i32,
        bold: bool,
        c: u32,
    ) {
        self.text_inner(text, x, y, w, h, size, bold, c, false, true);
    }
    #[allow(clippy::too_many_arguments)]
    fn text_inner(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        size: i32,
        bold: bool,
        c: u32,
        center: bool,
        wrap: bool,
    ) {
        unsafe {
            if text.is_empty() || w <= 0 || h <= 0 || !self.visible([x, y, w, h]) {
                return;
            }
            if self.layouts.len() > 180 {
                self.layouts.clear();
            }
            let key = (text.to_owned(), w, h, size, bold, center, wrap);
            let layout = if let Some(l) = self.layouts.get(&key) {
                l.clone()
            } else {
                let Ok(format) = self.graphics.format(size, bold, center, wrap) else {
                    return;
                };
                let utf: Vec<_> = text.encode_utf16().collect();
                let Ok(layout) = self
                    .graphics
                    .writing
                    .CreateTextLayout(&utf, &format, w as f32, h as f32)
                else {
                    return;
                };
                self.layouts.insert(key, layout.clone());
                layout
            };
            self.brush.SetColor(&rgba(c));
            self.target.DrawTextLayout(
                Vector2 {
                    X: x as f32,
                    Y: y as f32,
                },
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
    }
    fn note_layout(&mut self, text: &str) -> Result<IDWriteTextLayout> {
        if let Some((cached, layout)) = &self.note_layout
            && cached == text
        {
            return Ok(layout.clone());
        }
        unsafe {
            let format = self.graphics.format(14, false, false, true)?;
            let utf: Vec<_> = text.encode_utf16().collect();
            let layout = self.graphics.writing.CreateTextLayout(
                &utf,
                &format,
                crate::notes::TEXT[2] as f32,
                1_000_000.,
            )?;
            layout.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, 22., 17.)?;
            layout.SetTrimming(&DWRITE_TRIMMING::default(), None)?;
            self.note_layout = Some((text.to_owned(), layout.clone()));
            Ok(layout)
        }
    }
    pub fn note_caret(&mut self, e: &crate::notes::Editor) -> (f32, f32) {
        let mut x = 0.;
        let mut y = 0.;
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        if let Ok(l) = self.note_layout(&e.text) {
            unsafe {
                let _ = l.HitTestTextPosition(e.utf16(e.cursor), false, &mut x, &mut y, &mut m);
            }
        }
        (x, y)
    }
    pub fn note_height(&mut self, text: &str) -> f32 {
        let mut m = DWRITE_TEXT_METRICS::default();
        if let Ok(l) = self.note_layout(text) {
            unsafe {
                let _ = l.GetMetrics(&mut m);
            }
        }
        m.height.max(22.)
    }
    pub fn note_hit(&mut self, e: &crate::notes::Editor, x: f32, y: f32) -> usize {
        let mut trailing = BOOL(0);
        let mut inside = BOOL(0);
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        if let Ok(l) = self.note_layout(&e.text) {
            unsafe {
                let _ = l.HitTestPoint(x.max(0.), y.max(0.), &mut trailing, &mut inside, &mut m);
            }
        }
        e.byte(m.textPosition + if trailing.as_bool() { m.length } else { 0 })
    }
    pub fn note_draw(&mut self, e: &crate::notes::Editor, caret: bool, active: bool) {
        let [x, y, w, h] = crate::notes::TEXT;
        if !self.visible([x, y, w, h]) {
            return;
        }
        let Ok(layout) = self.note_layout(&e.text) else {
            return;
        };
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect(x, y, w, h), D2D1_ANTIALIAS_MODE_ALIASED);
            let r = e.selection();
            if active && !r.is_empty() {
                let start = e.utf16(r.start);
                let length = e.utf16(r.end) - start;
                let mut count = 0;
                let _ = layout.HitTestTextRange(
                    start,
                    length,
                    x as f32,
                    y as f32 - e.scroll,
                    None,
                    &mut count,
                );
                let mut areas = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
                if layout
                    .HitTestTextRange(
                        start,
                        length,
                        x as f32,
                        y as f32 - e.scroll,
                        Some(&mut areas),
                        &mut count,
                    )
                    .is_ok()
                {
                    self.brush.SetColor(&rgba(0x4a4a4a));
                    for a in areas {
                        self.target.FillRectangle(
                            &D2D_RECT_F {
                                left: a.left,
                                top: a.top,
                                right: a.left + a.width.max(3.),
                                bottom: a.top + a.height,
                            },
                            &self.brush,
                        );
                    }
                }
            }
            self.brush.SetColor(&rgba(WHITE));
            self.target.DrawTextLayout(
                Vector2 {
                    X: x as f32,
                    Y: y as f32 - e.scroll,
                },
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            if caret {
                let (cx, cy) = self.note_caret(e);
                self.line(
                    x + cx as i32,
                    y + (cy - e.scroll) as i32,
                    x + cx as i32,
                    y + (cy + 20. - e.scroll) as i32,
                    WHITE,
                    1,
                );
            }
            self.target.PopAxisAlignedClip();
        }
        let height = self.note_height(&e.text);
        if height > h as f32 {
            let thumb = (h as f32 * h as f32 / height).max(22.);
            let top = (h as f32 - thumb) * e.scroll / (height - h as f32).max(1.);
            self.round(x + w + 7, y + top as i32, 3, thumb as i32, 2, 0x666666);
        }
    }
    pub fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32, c: u32, width: i32) {
        unsafe {
            self.brush.SetColor(&rgba(c));
            self.target.DrawLine(
                Vector2 {
                    X: x1 as f32,
                    Y: y1 as f32,
                },
                Vector2 {
                    X: x2 as f32,
                    Y: y2 as f32,
                },
                &self.brush,
                width as f32,
                None,
            );
        }
    }
    pub fn circle(&self, x: i32, y: i32, size: i32, c: u32) {
        self.round(x, y, size, size, size / 2, c);
    }
    pub fn droplet(&self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        unsafe {
            let draw = (|| -> Result<()> {
                let geometry = self.graphics.drawing.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.BeginFigure(
                    Vector2 {
                        X: (x + w / 2) as f32,
                        Y: y as f32,
                    },
                    D2D1_FIGURE_BEGIN_HOLLOW,
                );
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: (x + w) as f32,
                        Y: (y + h / 2) as f32,
                    },
                    point2: Vector2 {
                        X: (x + w) as f32,
                        Y: (y + h) as f32,
                    },
                    point3: Vector2 {
                        X: (x + w / 2) as f32,
                        Y: (y + h) as f32,
                    },
                });
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: x as f32,
                        Y: (y + h) as f32,
                    },
                    point2: Vector2 {
                        X: x as f32,
                        Y: (y + h / 2) as f32,
                    },
                    point3: Vector2 {
                        X: (x + w / 2) as f32,
                        Y: y as f32,
                    },
                });
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                self.brush.SetColor(&rgba(c));
                self.target.DrawGeometry(&geometry, &self.brush, 2.0, None);
                Ok(())
            })();
            let _ = draw;
        }
    }
    /// Spotify's circular mark and three curved sound waves, rendered as vectors.
    pub fn spotify(&self, x: i32, y: i32, size: i32) {
        unsafe {
            self.circle(x, y, size, SPOTIFY);
            let draw = (|| -> Result<()> {
                let geometry = self.graphics.drawing.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                for (start_y, depth, width) in
                    [(0.35, 0.05, 0.64), (0.49, 0.04, 0.54), (0.64, 0.03, 0.43)]
                {
                    let left = x as f32 + size as f32 * 0.2;
                    let top = y as f32 + size as f32 * start_y;
                    sink.BeginFigure(Vector2 { X: left, Y: top }, D2D1_FIGURE_BEGIN_HOLLOW);
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: Vector2 {
                            X: left + size as f32 * 0.2,
                            Y: top - size as f32 * depth,
                        },
                        point2: Vector2 {
                            X: left + size as f32 * 0.42,
                            Y: top - size as f32 * depth,
                        },
                        point3: Vector2 {
                            X: left + size as f32 * width,
                            Y: top + size as f32 * 0.06,
                        },
                    });
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                }
                sink.Close()?;
                self.brush.SetColor(&rgba(0x092111));
                self.target.DrawGeometry(
                    &geometry,
                    &self.brush,
                    (size as f32 * 0.08).max(1.3),
                    None,
                );
                Ok(())
            })();
            let _ = draw;
        }
    }
    /// The full-brightness wheel is rasterized once per DPI, then reused on every drag frame.
    pub fn color_wheel(&mut self, hsv: crate::color::Hsv) {
        let [x, y, size, _] = crate::color::WHEEL;
        if !self.visible([x, y, size, size]) {
            return;
        }
        let pixels = (size as f32 * self.scale).round().clamp(1.0, 1024.0) as u32;
        unsafe {
            if self.color_wheel.as_ref().is_none_or(|(n, _)| *n != pixels) {
                let mut data = vec![0u8; (pixels * pixels * 4) as usize];
                let radius = pixels as f32 / 2.0;
                for py in 0..pixels {
                    for px in 0..pixels {
                        let dx = (px as f32 + 0.5 - radius) / radius;
                        let dy = (py as f32 + 0.5 - radius) / radius;
                        let distance = dx.hypot(dy);
                        let alpha = ((1.0 - distance) * radius + 0.5).clamp(0.0, 1.0);
                        let rgb = crate::color::Hsv {
                            h: (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0),
                            s: distance.min(1.0),
                            v: 1.0,
                        }
                        .rgb();
                        let i = ((py * pixels + px) * 4) as usize;
                        for (j, shift) in [0, 8, 16].into_iter().enumerate() {
                            data[i + j] = (((rgb >> shift) & 255) as f32 * alpha).round() as u8;
                        }
                        data[i + 3] = (255.0 * alpha).round() as u8;
                    }
                }
                if let Ok(bitmap) = self.target.CreateBitmap(
                    D2D_SIZE_U {
                        width: pixels,
                        height: pixels,
                    },
                    Some(data.as_ptr().cast()),
                    pixels * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    },
                ) {
                    self.color_wheel = Some((pixels, bitmap));
                }
            }
            if let Some((_, bitmap)) = &self.color_wheel {
                self.target.DrawBitmap(
                    bitmap,
                    Some(&rect(x, y, size, size)),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
            self.brush.SetColor(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0 - hsv.v,
            });
            self.target.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 {
                        X: (x + size / 2) as f32,
                        Y: (y + size / 2) as f32,
                    },
                    radiusX: size as f32 / 2.0 - 0.5,
                    radiusY: size as f32 / 2.0 - 0.5,
                },
                &self.brush,
            );
        }
        let (mx, my) = hsv.marker();
        self.circle(mx - 7, my - 7, 14, 0x121212);
        self.circle(mx - 5, my - 5, 10, WHITE);
        self.circle(mx - 3, my - 3, 6, hsv.rgb());
    }
    pub fn horizontal_gradient(&self, r: [i32; 4], from: u32, to: u32) {
        let [x, y, w, h] = r;
        if !self.visible(r) {
            return;
        }
        unsafe {
            let draw = (|| -> Result<()> {
                let stops = self.target.CreateGradientStopCollection(
                    &[
                        D2D1_GRADIENT_STOP {
                            position: 0.0,
                            color: rgba(from),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.0,
                            color: rgba(to),
                        },
                    ],
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )?;
                let brush = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 {
                            X: x as f32,
                            Y: y as f32,
                        },
                        endPoint: Vector2 {
                            X: (x + w) as f32,
                            Y: y as f32,
                        },
                    },
                    None,
                    &stops,
                )?;
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: h as f32 / 2.0,
                        radiusY: h as f32 / 2.0,
                    },
                    &brush,
                );
                Ok(())
            })();
            let _ = draw;
        }
    }
    pub fn artwork(&mut self, art: &Arc<Artwork>, x: i32, y: i32, size: i32) {
        if !self.visible([x, y, size, size]) {
            return;
        }
        unsafe {
            let matches = self
                .artwork
                .as_ref()
                .is_some_and(|(a, _)| Arc::ptr_eq(a, art));
            if !matches
                && let Ok(bitmap) = self.target.CreateBitmap(
                    D2D_SIZE_U {
                        width: art.width as u32,
                        height: art.height as u32,
                    },
                    Some(art.pixels.as_ptr().cast()),
                    art.width as u32 * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    },
                )
            {
                self.artwork = Some((art.clone(), bitmap));
            }
            if let Some((_, bitmap)) = &self.artwork {
                let mask = self
                    .graphics
                    .drawing
                    .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                        rect: rect(x, y, size, size),
                        radiusX: size as f32 * 0.21,
                        radiusY: size as f32 * 0.21,
                    })
                    .and_then(|g| g.cast::<ID2D1Geometry>());
                if let Ok(mask) = &mask {
                    let mut parameters = D2D1_LAYER_PARAMETERS {
                        contentBounds: rect(x, y, size, size),
                        geometricMask: std::mem::ManuallyDrop::new(Some(mask.clone())),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: Matrix3x2::identity(),
                        opacity: 1.0,
                        ..Default::default()
                    };
                    self.target.PushLayer(&parameters, None);
                    std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
                }
                self.target.DrawBitmap(
                    bitmap,
                    Some(&rect(x, y, size, size)),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
                if mask.is_ok() {
                    self.target.PopLayer();
                }
            }
        }
    }
    pub fn present(
        &self,
        hwnd: HWND,
        x: i32,
        y: i32,
        alpha: u8,
        radius: i32,
        mode: DockMode,
    ) -> Result<()> {
        self.finish()?;
        self.mask(radius, mode);
        self.upload(hwnd, Some(POINT { x, y }), alpha)
    }
    /// Reuse already-masked pixels: an opacity frame never redraws or reapplies the glass mask.
    pub fn fade_alpha(&self, hwnd: HWND, alpha: u8) -> Result<()> {
        self.upload(hwnd, None, alpha)
    }
    fn upload(&self, hwnd: HWND, position: Option<POINT>, alpha: u8) -> Result<()> {
        let upload = self.profiling.then(std::time::Instant::now);
        let result = unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                position.as_ref().map(|p| p as *const POINT),
                Some(&SIZE {
                    cx: self.width,
                    cy: self.height,
                }),
                Some(self.surface.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: alpha,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )
        };
        if let Some(upload) = upload {
            let mut times = self.timings.get();
            times[2] = upload.elapsed().as_secs_f64() * 1000.0;
            self.timings.set(times);
        }
        result
    }
    pub fn set_border(&self, enabled: bool, color: u32, width: u8, glow: u8) {
        self.border
            .set(enabled.then_some((color, width as f32, glow as f32 / 100.0)));
    }
    pub fn hit_test(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return false;
        }
        let cache = self.mask_cache.borrow();
        let Some(c) = cache.as_ref() else {
            return false;
        };
        let pad = super::HALO as f32 * self.scale;
        let top = if c.mode == DockMode::Anchored {
            0.0
        } else {
            pad
        };
        shape_distance(
            x as f32 + 0.5 - pad,
            y as f32 + 0.5 - top,
            self.width as f32 - 2.0 * pad,
            self.height as f32 - top - pad,
            c.radius as f32 * self.scale,
            c.mode,
            12.0 * self.scale,
        ) <= 0.0
    }
    pub fn mask(&self, radius: i32, mode: DockMode) {
        let started = self.profiling.then(std::time::Instant::now);
        let pad = super::HALO as f32 * self.scale;
        let top = if mode == DockMode::Anchored { 0.0 } else { pad };
        let body_height = self.height as f32 - top - pad;
        let radius = radius
            .min((body_height / (2.0 * self.scale)).floor() as i32)
            .max(1);
        let r = radius as f32 * self.scale;
        let band = (r * 1.5).max(48.0 * self.scale).ceil() as i32;
        let mut cache = self.mask_cache.borrow_mut();
        if cache.as_ref().is_none_or(|c| {
            c.radius != radius
                || c.mode != mode
                || shape_style(c.style) != shape_style(self.border.get())
                || c.width != self.width
        }) {
            let w = self.width as f32 - 2.0 * pad;
            // Canonical top/bottom slices and a single side row survive every height-only resize.
            let h = 2.0 * band as f32 + 2.0;
            let mut luts = self.border_luts.borrow_mut();
            if luts
                .first()
                .is_some_and(|l| shape_style(l.style) != shape_style(self.border.get()))
            {
                luts.clear();
            }
            let li = if let Some(i) = luts.iter().position(|l| l.radius == radius) {
                i
            } else {
                if luts.len() >= 3 {
                    luts.remove(0);
                }
                luts.push(GlassLut::new(radius, self.border.get()));
                luts.len() - 1
            };
            let lut = &mut luts[li];
            if lut.style != self.border.get() {
                lut.style = self.border.get();
                lut.palette = lut.style.map(GlassStyle::new);
            }
            let columns: Vec<_> = (0..self.width)
                .map(|x| {
                    let px = x as f32 + 0.5 - pad;
                    let qx = (px - w / 2.0).abs() - (w / 2.0 - r);
                    let fx = (px / w).clamp(0.0, 1.0);
                    (
                        px,
                        qx,
                        gaussian(fx - 0.20, 0.11) * 0.58 + gaussian(fx - 0.61, 0.17) * 0.65,
                        gaussian(fx - 0.26, 0.13) * 0.7 + gaussian(fx - 0.81, 0.12) * 0.65,
                    )
                })
                .collect();
            let row = |py: f32, all: bool| -> Vec<EdgePixel> {
                let qy = (py - h / 2.0).abs() - (h / 2.0 - r);
                let mut result = Vec::with_capacity(self.width as usize);
                for (x, &(px, qx, a, b)) in columns.iter().enumerate() {
                    let sd = shape_distance(px, py, w, h, r, mode, 12.0 * self.scale);
                    let limit = if self.border.get().is_some() {
                        -band as f32
                    } else {
                        -0.5
                    };
                    if all || sd > limit {
                        let ny = if qy > 0.0 {
                            if qx > 0.0 { qy / qx.hypot(qy) } else { 1.0 }
                        } else {
                            0.0
                        };
                        let power = if ny == 0.0 || ny == 1.0 {
                            ny
                        } else {
                            ny.powf(1.2)
                        };
                        let reflection =
                            (0.42 + 0.36 * power + 0.30 * if py < h / 2.0 { a } else { b } * ny)
                                .clamp(0.0, 1.0);
                        result.push(lut.edge(x * 4, sd, self.scale, reflection));
                    }
                }
                result
            };
            let top_rows = (top as i32 + band) as usize;
            let bottom_rows = (pad as i32 + band) as usize;
            let upper = (0..top_rows)
                .map(|y| row(y as f32 + 0.5 - top, true))
                .collect();
            let lower = (0..bottom_rows)
                .map(|y| row(h + pad - y as f32 - 0.5, true))
                .collect();
            let sides = row(band as f32 + 1.0, false);
            *cache = Some(MaskCache {
                radius,
                mode,
                style: self.border.get(),
                width: self.width,
                top: upper,
                bottom: lower,
                sides,
            });
        }
        if cache.as_ref().is_some_and(|c| c.style != self.border.get()) {
            let mut luts = self.border_luts.borrow_mut();
            let index = if let Some(i) = luts.iter().position(|l| {
                l.radius == radius && shape_style(l.style) == shape_style(self.border.get())
            }) {
                i
            } else {
                luts.push(GlassLut::new(radius, self.border.get()));
                luts.len() - 1
            };
            let lut = &mut luts[index];
            lut.style = self.border.get();
            lut.palette = lut.style.map(GlassStyle::new);
            let c = cache.as_mut().unwrap();
            for e in c
                .top
                .iter_mut()
                .chain(c.bottom.iter_mut())
                .flat_map(|row| row.iter_mut())
                .chain(c.sides.iter_mut())
            {
                if let Some(p) = &lut.palette {
                    let [tint, base, white, emission] = e.color_weights.map(|v| v as f32 / 65535.0);
                    e.light = std::array::from_fn(|i| {
                        p.tint[i] * tint
                            + p.base[i] * base
                            + 255.0 * white
                            + p.emission[i] * emission
                    });
                }
            }
            c.style = self.border.get();
        }
        let built = started
            .map(|t| t.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        let c = cache.as_ref().unwrap();
        let split = (top + body_height / 2.0).ceil() as usize;
        let upper = c.top.len().min(split);
        let lower = c.bottom.len().min(self.height as usize - split);
        unsafe {
            let pixels = std::slice::from_raw_parts_mut(
                self.bits,
                self.stride as usize * self.capacity_height as usize * 4,
            );
            let region = self
                .partial
                .get()
                .map(|r| {
                    let x0 = ((r[0]
                        + (super::HALO + if c.mode == DockMode::Anchored { 12 } else { 0 }))
                        as f32
                        * self.scale)
                        .floor() as i32;
                    let y0 = ((r[1] as f32 * self.scale) + top).floor() as i32;
                    let x1 = (((r[0]
                        + r[2]
                        + (super::HALO + if c.mode == DockMode::Anchored { 12 } else { 0 }))
                        as f32)
                        * self.scale)
                        .ceil() as i32;
                    let y1 = (((r[1] + r[3]) as f32 * self.scale) + top).ceil() as i32;
                    [
                        x0.clamp(0, self.width),
                        y0.clamp(0, self.height),
                        x1.clamp(0, self.width),
                        y1.clamp(0, self.height),
                    ]
                })
                .unwrap_or([0, 0, self.width, self.height]);
            for y in region[1] as usize..region[3] as usize {
                let start = y * self.stride as usize * 4;
                for p in pixels[start + region[0] as usize * 4..start + region[2] as usize * 4]
                    .as_chunks_mut::<4>()
                    .0
                {
                    p[3] = 255;
                }
            }
            for (y, row) in c
                .top
                .iter()
                .take(upper)
                .enumerate()
                .filter(|(y, _)| *y >= region[1] as usize && *y < region[3] as usize)
            {
                let offset = y * self.stride as usize * 4;
                for edge in row.iter().filter(|e| {
                    e.index / 4 >= region[0] as usize && e.index / 4 < region[2] as usize
                }) {
                    composite(pixels, offset, edge, self.scale);
                }
            }
            for (y, row) in c.bottom.iter().take(lower).enumerate().filter(|(y, _)| {
                let py = self.height as usize - 1 - *y;
                py >= region[1] as usize && py < region[3] as usize
            }) {
                let offset = (self.height as usize - 1 - y) * self.stride as usize * 4;
                for edge in row.iter().filter(|e| {
                    e.index / 4 >= region[0] as usize && e.index / 4 < region[2] as usize
                }) {
                    composite(pixels, offset, edge, self.scale);
                }
            }
            for y in upper.max(region[1] as usize)
                ..(self.height as usize - lower).min(region[3] as usize)
            {
                let offset = y * self.stride as usize * 4;
                for edge in c.sides.iter().filter(|e| {
                    e.index / 4 >= region[0] as usize && e.index / 4 < region[2] as usize
                }) {
                    composite(pixels, offset, edge, self.scale);
                }
            }
        }
        if let Some(started) = started {
            let mut phases = self.timings.get();
            phases[0] = built;
            phases[1] = started.elapsed().as_secs_f64() * 1000.0 - built;
            self.timings.set(phases);
        }
    }
    pub fn spotify_gradient(&self, x: i32, y: i32, w: i32, h: i32) {
        if !self.visible([x, y, w, h]) {
            return;
        }
        unsafe {
            let draw = (|| -> Result<()> {
                let stops = self.target.CreateGradientStopCollection(
                    &[
                        D2D1_GRADIENT_STOP {
                            position: 0.,
                            color: rgba(0x0a3d20),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 0.33,
                            color: rgba(0x061e10),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 0.7,
                            color: rgba(0x050b07),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.,
                            color: rgba(0x000000),
                        },
                    ],
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )?;
                let brush = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 {
                            X: x as f32,
                            Y: y as f32,
                        },
                        endPoint: Vector2 {
                            X: (x + w) as f32,
                            Y: (y + h / 2) as f32,
                        },
                    },
                    None,
                    &stops,
                )?;
                self.target.FillRectangle(&rect(x, y, w, h), &brush);
                Ok(())
            })();
            let _ = draw;
        }
    }
    /// Rounded vector artwork composed of cubic segments in local coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn curve(
        &self,
        start: (f32, f32),
        segments: &[[f32; 6]],
        color: u32,
        width: f32,
        fill: bool,
        closed: bool,
    ) {
        unsafe {
            let draw = (|| -> Result<()> {
                let geometry = self.graphics.drawing.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.BeginFigure(
                    Vector2 {
                        X: start.0,
                        Y: start.1,
                    },
                    if fill {
                        D2D1_FIGURE_BEGIN_FILLED
                    } else {
                        D2D1_FIGURE_BEGIN_HOLLOW
                    },
                );
                for s in segments {
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: Vector2 { X: s[0], Y: s[1] },
                        point2: Vector2 { X: s[2], Y: s[3] },
                        point3: Vector2 { X: s[4], Y: s[5] },
                    });
                }
                sink.EndFigure(if closed {
                    D2D1_FIGURE_END_CLOSED
                } else {
                    D2D1_FIGURE_END_OPEN
                });
                sink.Close()?;
                self.brush.SetColor(&rgba(color));
                if fill {
                    self.target.FillGeometry(&geometry, &self.brush, None);
                } else {
                    self.target.DrawGeometry(
                        &geometry,
                        &self.brush,
                        width,
                        Some(&self.graphics.stroke),
                    );
                }
                Ok(())
            })();
            let _ = draw;
        }
    }
    pub fn path(&self, points: &[(f32, f32)], color: u32, width: f32, fill: bool, closed: bool) {
        if points.is_empty() {
            return;
        }
        unsafe {
            let draw = (|| -> Result<()> {
                let geometry = self.graphics.drawing.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.BeginFigure(
                    Vector2 {
                        X: points[0].0,
                        Y: points[0].1,
                    },
                    if fill {
                        D2D1_FIGURE_BEGIN_FILLED
                    } else {
                        D2D1_FIGURE_BEGIN_HOLLOW
                    },
                );
                for &(x, y) in &points[1..] {
                    sink.AddLine(Vector2 { X: x, Y: y });
                }
                sink.EndFigure(if closed {
                    D2D1_FIGURE_END_CLOSED
                } else {
                    D2D1_FIGURE_END_OPEN
                });
                sink.Close()?;
                self.brush.SetColor(&rgba(color));
                if fill {
                    self.target.FillGeometry(&geometry, &self.brush, None);
                } else {
                    self.target.DrawGeometry(
                        &geometry,
                        &self.brush,
                        width,
                        Some(&self.graphics.stroke),
                    );
                }
                Ok(())
            })();
            let _ = draw;
        }
    }
    /// Rounded audio bars use direct line drawing without per-frame path allocation.
    pub fn rounded_lines(&self, lines: &[[(f32, f32); 2]], color: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&rgba(color));
            for &[a, b] in lines {
                self.target.DrawLine(
                    Vector2 { X: a.0, Y: a.1 },
                    Vector2 { X: b.0, Y: b.1 },
                    &self.brush,
                    width,
                    Some(&self.graphics.stroke),
                );
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn outline(&self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&rgba(color));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(x, y, w, h),
                    radiusX: r as f32,
                    radiusY: r as f32,
                },
                &self.brush,
                width,
                None,
            );
        }
    }
    pub fn save_bmp(&self, path: &std::path::Path) -> std::io::Result<()> {
        self.finish().map_err(std::io::Error::other)?;
        let bytes = (self.width * self.height * 4) as u32;
        let mut file = Vec::with_capacity(54 + bytes as usize);
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&(54 + bytes).to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&self.width.to_le_bytes());
        file.extend_from_slice(&(-self.height).to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        unsafe {
            for y in 0..self.height as usize {
                file.extend_from_slice(std::slice::from_raw_parts(
                    self.bits.add(y * self.stride as usize * 4),
                    self.width as usize * 4,
                ));
            }
        }
        std::fs::write(path, file)
    }
}
impl Drop for Canvas {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}
