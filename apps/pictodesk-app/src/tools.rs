//! Tools: the tool set, brush dynamics, strokes, free transform, paths, and audio notes.
use super::*;
use crate::actions::AudioRec;
use crate::dialogs::DialogKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    Move,
    MarqueeRect,
    MarqueeEllipse,
    MarqueeRow,
    MarqueeCol,
    Lasso,
    PolyLasso,
    MagneticLasso,
    Wand,
    Eyedropper,
    ColorSampler,
    Measure,
    Hand,
    Zoom,
    Brush,
    Airbrush,
    Pencil,
    HistoryBrush,
    ArtHistoryBrush,
    Eraser,
    MagicEraser,
    BackgroundEraser,
    Stamp,
    PatternStamp,
    Heal,
    Patch,
    Pen,
    FreeformPen,
    MagneticPen,
    ConvertAnchor,
    Bucket,
    Gradient,
    Blur,
    Sharpen,
    Smudge,
    Dodge,
    Burn,
    Sponge,
    Type,
    VertType,
    TypeMask,
    VertTypeMask,
    // Phase 6 tools.
    ShapeRect,
    ShapeRounded,
    ShapeEllipse,
    ShapePolygon,
    ShapeLine,
    ShapeCustom,
    Slice,
    SliceSelect,
    Notes,
}

impl Tool {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::MarqueeRect => "Rectangular Marquee",
            Tool::MarqueeEllipse => "Elliptical Marquee",
            Tool::MarqueeRow => "Single Row Marquee",
            Tool::MarqueeCol => "Single Column Marquee",
            Tool::Lasso => "Lasso",
            Tool::PolyLasso => "Polygonal Lasso",
            Tool::MagneticLasso => "Magnetic Lasso",
            Tool::Wand => "Magic Wand",
            Tool::Eyedropper => "Eyedropper",
            Tool::ColorSampler => "Color Sampler",
            Tool::Measure => "Measure",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
            Tool::Brush => "Paintbrush",
            Tool::Airbrush => "Airbrush",
            Tool::Pencil => "Pencil",
            Tool::HistoryBrush => "History Brush",
            Tool::ArtHistoryBrush => "Art History Brush",
            Tool::Eraser => "Eraser",
            Tool::MagicEraser => "Magic Eraser",
            Tool::BackgroundEraser => "Background Eraser",
            Tool::Stamp => "Clone Stamp",
            Tool::PatternStamp => "Pattern Stamp",
            Tool::Heal => "Healing Brush",
            Tool::Patch => "Patch",
            Tool::Pen => "Pen",
            Tool::FreeformPen => "Freeform Pen",
            Tool::MagneticPen => "Magnetic Pen",
            Tool::ConvertAnchor => "Convert Anchor Point",
            Tool::Bucket => "Paint Bucket",
            Tool::Type => "Type",
            Tool::VertType => "Vertical Type",
            Tool::TypeMask => "Type Mask",
            Tool::VertTypeMask => "Vertical Type Mask",
            Tool::ShapeRect => "Rectangle",
            Tool::ShapeRounded => "Rounded Rectangle",
            Tool::ShapeEllipse => "Ellipse",
            Tool::ShapePolygon => "Polygon",
            Tool::ShapeLine => "Line",
            Tool::ShapeCustom => "Custom Shape",
            Tool::Slice => "Slice",
            Tool::SliceSelect => "Slice Select",
            Tool::Notes => "Notes",
            Tool::Gradient => "Gradient",
            Tool::Blur => "Blur",
            Tool::Sharpen => "Sharpen",
            Tool::Smudge => "Smudge",
            Tool::Dodge => "Dodge",
            Tool::Burn => "Burn",
            Tool::Sponge => "Sponge",
        }
    }
    pub(crate) fn key(&self) -> char {
        match self {
            Tool::Move => 'v',
            Tool::MarqueeRect | Tool::MarqueeEllipse | Tool::MarqueeRow | Tool::MarqueeCol => 'm',
            Tool::Lasso | Tool::PolyLasso | Tool::MagneticLasso => 'l',
            Tool::Wand => 'w',
            Tool::Eyedropper | Tool::ColorSampler | Tool::Measure => 'i',
            Tool::Hand => 'h',
            Tool::Zoom => 'z',
            Tool::Brush | Tool::Airbrush => 'b',
            Tool::Pencil => 'b',
            Tool::HistoryBrush | Tool::ArtHistoryBrush => 'y',
            Tool::Eraser | Tool::MagicEraser | Tool::BackgroundEraser => 'e',
            Tool::Stamp | Tool::PatternStamp => 's',
            Tool::Heal | Tool::Patch => 'j',
            Tool::Pen | Tool::FreeformPen | Tool::MagneticPen | Tool::ConvertAnchor => 'p',
            Tool::Type | Tool::VertType | Tool::TypeMask | Tool::VertTypeMask => 't',
            Tool::Bucket => 'k',
            Tool::Gradient => 'g',
            Tool::Blur | Tool::Sharpen | Tool::Smudge => 'r',
            Tool::Dodge | Tool::Burn | Tool::Sponge => 'o',
            Tool::ShapeRect
            | Tool::ShapeRounded
            | Tool::ShapeEllipse
            | Tool::ShapePolygon
            | Tool::ShapeLine
            | Tool::ShapeCustom => 'u',
            Tool::Slice | Tool::SliceSelect => 'c',
            Tool::Notes => 'n',
        }
    }
}

/// An in-progress Free Transform: the document is re-derived from
/// `origin` on every mouse move, so drags never accumulate error.
/// Enter commits (one undo step), Esc cancels.
pub(crate) struct FtState {
    pub(crate) origin: Document,
    pub(crate) targets: Vec<usize>,
    pub(crate) bounds: (f32, f32, f32, f32), // active layer's painted bounds
    pub(crate) sx: f32,
    pub(crate) sy: f32,
    pub(crate) rot: f32,
    pub(crate) skew: f32,
    pub(crate) dx: f32,
    pub(crate) dy: f32,
    pub(crate) drag: FtDrag,
    pub(crate) drag_start_doc: (f32, f32),
    pub(crate) start: (f32, f32, f32, f32, f32, f32), // params at drag start
    pub(crate) start_angle: f32,
    /// Distort mode: the four corners (TL, TR, BR, BL) in doc
    /// coords, once a corner has been dragged freely. While set,
    /// the affine params are ignored.
    pub(crate) quad: Option<[(f32, f32); 4]>,
    pub(crate) start_quad: Option<[(f32, f32); 4]>,
    /// Transform Selection mode: warp the selection mask instead
    /// of any layer; `origin_sel` is the mask being warped.
    pub(crate) sel_mode: bool,
    pub(crate) origin_sel: Option<Selection>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum DragMode {
    None,
    Paint,
    Pan,
    SelectRect,
    Lasso,
    GradientLine,
    MoveLayer,
    MenuNav,
    PenNew,
    PenDrag,
    PenConvert,
    NavDrag,
    GuideDrag,
    MeasureDrag,
    MagLasso,
    FreePen,
    MagPen,
    PatchDrag,
    // Phase 6 drags.
    ShapeDrag,
    SliceDrag,
}

/// Phase 7 paint-engine settings (the Brushes palette's
/// sections). Controls: 0 = Off, 1 = Fade, 2+ = pen
/// controls (present in the palette; this build has no
/// tablet pressure input, so they behave as Off — named in
/// the phase delivery). One shared Fade length (fade_steps)
/// serves every Fade control.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrushDynamics {
    pub(crate) spacing: f32,
    pub(crate) angle_deg: f32,
    pub(crate) roundness: f32,
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    pub(crate) size_jitter: f32,
    pub(crate) size_control: u8,
    pub(crate) min_diameter: f32,
    pub(crate) angle_jitter: f32,
    pub(crate) angle_control: u8,
    pub(crate) round_jitter: f32,
    pub(crate) round_control: u8,
    pub(crate) min_roundness: f32,
    pub(crate) fade_steps: u32,
    pub(crate) scatter: f32,
    pub(crate) both_axes: bool,
    pub(crate) count: u8,
    pub(crate) count_jitter: f32,
    pub(crate) texture_on: bool,
    pub(crate) tex_scale: f32,
    pub(crate) tex_depth: f32,
    pub(crate) tex_min_depth: f32,
    pub(crate) tex_mode: u8,
    pub(crate) tex_invert: bool,
    pub(crate) protect_texture: bool,
    pub(crate) dual_on: bool,
    pub(crate) dual_size: f32,
    pub(crate) dual_hardness: f32,
    pub(crate) dual_mode: u8,
    pub(crate) dual_scatter: f32,
    pub(crate) fb_jitter: f32,
    pub(crate) hue_jitter: f32,
    pub(crate) sat_jitter: f32,
    pub(crate) bri_jitter: f32,
    pub(crate) opacity_jitter: f32,
    pub(crate) opacity_control: u8,
    pub(crate) flow_jitter: f32,
    pub(crate) flow_control: u8,
    pub(crate) noise: bool,
    pub(crate) wet_edges: bool,
    pub(crate) airbrush: bool,
    pub(crate) smoothing: bool,
}

impl Default for BrushDynamics {
    fn default() -> Self {
        BrushDynamics {
            spacing: 0.25,
            angle_deg: 0.0,
            roundness: 1.0,
            flip_x: false,
            flip_y: false,
            size_jitter: 0.0,
            size_control: 0,
            min_diameter: 0.0,
            angle_jitter: 0.0,
            angle_control: 0,
            round_jitter: 0.0,
            round_control: 0,
            min_roundness: 0.0,
            fade_steps: 100,
            scatter: 0.0,
            both_axes: false,
            count: 1,
            count_jitter: 0.0,
            texture_on: false,
            tex_scale: 1.0,
            tex_depth: 0.5,
            tex_min_depth: 0.0,
            tex_mode: 0,
            tex_invert: false,
            protect_texture: false,
            dual_on: false,
            dual_size: 13.0,
            dual_hardness: 1.0,
            dual_mode: 1,
            dual_scatter: 0.0,
            fb_jitter: 0.0,
            hue_jitter: 0.0,
            sat_jitter: 0.0,
            bri_jitter: 0.0,
            opacity_jitter: 0.0,
            opacity_control: 0,
            flow_jitter: 0.0,
            flow_control: 0,
            noise: false,
            wet_edges: false,
            airbrush: false,
            smoothing: false,
        }
    }
}

pub(crate) struct StrokeState {
    pub(crate) cmd: TileEditCommand,
    pub(crate) start_point: (f32, f32),
    pub(crate) last_point: (f32, f32),
    pub(crate) smudge_carry: Pixel16,
    pub(crate) tiles_seen: Vec<(i32, i32)>,
    /// Phase 7 engine state: RNG, stamp counter (Fade),
    /// the smoothed point, and the stroke's texture.
    pub(crate) rng: u32,
    pub(crate) stamps: u32,
    pub(crate) smooth: (f32, f32),
    pub(crate) tex: Option<std::rc::Rc<pictodesk_core::brush::BrushTexture>>,
}

/// What the canvas is showing: the composite, one color channel,
/// or one alpha/spot channel (as grayscale, or tinted for spot).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ChannelView {
    Composite,
    Channel(usize), // index into the mode's channel list (R,G,B / C,M,Y,K / Gray)
    Alpha(usize),
    /// A layer mask, shown as grayscale (Phase 4).
    Mask(usize),
}

/// Where painting lands: the image itself, one alpha channel, or a
/// layer mask (Phase 4).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PaintTarget {
    Image,
    Alpha(usize),
    Mask(usize),
}

/// A channel choice in the Apply Image / Calculations
/// dialogs (Phase 6.5 Batch 3).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ChanRef {
    Gray,
    /// Color channel index in the document mode's own terms
    /// (RGB 0..2, CMYK inks 0..3, Lab slots 0..2).
    Color(u8),
    /// The composite / layer transparency.
    Transparency,
    /// A saved alpha channel.
    Alpha(usize),
}

/// Start capturing the default microphone into memory.
/// Errors are plain strings for the status line.
pub(crate) fn audio_record_start() -> Result<AudioRec, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no microphone found".to_string())?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = supported.sample_rate().0;
    let channels = supported.channels();
    let fmt = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = samples.clone();
    let push_f32 = move |sink: &std::sync::Arc<std::sync::Mutex<Vec<f32>>>, data: &[f32]| {
        if let Ok(mut g) = sink.lock() {
            g.extend_from_slice(data);
        }
    };
    let stream = match fmt {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| push_f32(&sink, data),
            |_| {},
            None,
        ),
        cpal::SampleFormat::I16 => {
            let sink = samples.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    let conv: Vec<f32> = data.iter().map(|v| *v as f32 / 32768.0).collect();
                    if let Ok(mut g) = sink.lock() {
                        g.extend_from_slice(&conv);
                    }
                },
                |_| {},
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let sink = samples.clone();
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    let conv: Vec<f32> = data
                        .iter()
                        .map(|v| (*v as f32 - 32768.0) / 32768.0)
                        .collect();
                    if let Ok(mut g) = sink.lock() {
                        g.extend_from_slice(&conv);
                    }
                },
                |_| {},
                None,
            )
        }
        _ => return Err("unsupported microphone format".to_string()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(AudioRec {
        stream,
        samples: {
            // The F32 branch moved `samples`'s clone; rebuild
            // the handle from the sink used there is not
            // possible, so keep a second clone before matching.
            samples
        },
        rate,
        channels,
    })
}

/// Encode captured samples as a 16-bit mono PCM WAV (channels
/// downmixed). This is the audio-note file format.
pub(crate) fn wav_encode(samples: &[f32], rate: u32, channels: u16) -> Vec<u8> {
    let ch = channels.max(1) as usize;
    let frames = samples.len() / ch;
    let data_len = (frames * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..ch {
            acc += samples[f * ch + c];
        }
        let v = (acc / ch as f32).clamp(-1.0, 1.0);
        out.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Play a WAV audio note (the format `wav_encode` writes)
/// through the default output device.
pub(crate) fn audio_play(wav: &[u8]) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    if wav.len() < 44 || &wav[0..4] != b"RIFF" {
        return Err("bad audio data".to_string());
    }
    let src_rate = u32::from_le_bytes(wav[24..28].try_into().unwrap()).max(1);
    let samples: std::sync::Arc<Vec<f32>> = std::sync::Arc::new(
        wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
            .collect(),
    );
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no output device".to_string())?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let out_rate = supported.sample_rate().0.max(1);
    let out_ch = supported.channels().max(1);
    let fmt = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let pos = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let next = move || -> f32 {
        let mut p = pos.lock().unwrap();
        let src_idx = *p * src_rate as usize / out_rate as usize;
        *p += 1;
        samples.get(src_idx).copied().unwrap_or(0.0)
    };
    let stream = match fmt {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                for frame in out.chunks_mut(out_ch as usize) {
                    let v = next();
                    for s in frame.iter_mut() {
                        *s = v;
                    }
                }
            },
            |_| {},
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |out: &mut [i16], _| {
                for frame in out.chunks_mut(out_ch as usize) {
                    let v = (next().clamp(-1.0, 1.0) * 32767.0) as i16;
                    for s in frame.iter_mut() {
                        *s = v;
                    }
                }
            },
            |_| {},
            None,
        ),
        _ => return Err("unsupported output format".to_string()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

impl App {
    pub(crate) fn begin_free_transform(&mut self) {
        if self.ft.is_some() {
            self.ft_commit();
            return;
        }
        let a = self.doc.active_layer;
        let Some(b) = pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[a]) else {
            self.status = "Free Transform: the active layer is empty".into();
            return;
        };
        let targets: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i == a || l.linked)
            .map(|(i, _)| i)
            .collect();
        self.ft = Some(FtState {
            origin: self.doc.clone(),
            targets,
            bounds: (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32),
            sx: 1.0,
            sy: 1.0,
            rot: 0.0,
            skew: 0.0,
            dx: 0.0,
            dy: 0.0,
            drag: FtDrag::None,
            drag_start_doc: (0.0, 0.0),
            start: (1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            start_angle: 0.0,
            quad: None,
            start_quad: None,
            sel_mode: false,
            origin_sel: None,
        });
        self.status =
            "Free Transform: drag handles to scale, inside to move, outside to rotate, Ctrl+drag to skew, Ctrl+drag a corner to distort — Enter applies, Esc cancels"
                .into();
    }

    /// Edit > Transform Again: re-apply the last Free Transform
    /// gesture to the active layer (and its linked partners).
    pub(crate) fn transform_again(&mut self) {
        let Some((sx, sy, rot, skew, dx, dy, rel_quad)) = self.last_ft else {
            self.status = "Transform Again: nothing to repeat yet".into();
            return;
        };
        let a = self.doc.active_layer;
        let Some(b) = pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[a]) else {
            self.status = "Transform Again: the active layer is empty".into();
            return;
        };
        let targets: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i == a || l.linked)
            .map(|(i, _)| i)
            .collect();
        self.ft = Some(FtState {
            origin: self.doc.clone(),
            targets: if rel_quad.is_some() { vec![a] } else { targets },
            bounds: (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32),
            sx,
            sy,
            rot,
            skew,
            dx,
            dy,
            drag: FtDrag::None,
            drag_start_doc: (0.0, 0.0),
            start: (sx, sy, rot, skew, dx, dy),
            start_angle: 0.0,
            quad: rel_quad.map(|rel| {
                let base = [
                    (b.0 as f32, b.1 as f32),
                    (b.2 as f32, b.1 as f32),
                    (b.2 as f32, b.3 as f32),
                    (b.0 as f32, b.3 as f32),
                ];
                let mut q = [(0.0f32, 0.0f32); 4];
                for i in 0..4 {
                    q[i] = (base[i].0 + rel[i].0, base[i].1 + rel[i].1);
                }
                q
            }),
            start_quad: None,
            sel_mode: false,
            origin_sel: None,
        });
        self.ft_apply();
        self.ft_commit();
    }

    /// Painted bounds of a layer as (x0, y0, x1, y1) exclusive.
    pub(crate) fn layer_bounds(&self, li: usize) -> Option<(f32, f32, f32, f32)> {
        pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[li])
            .map(|b| (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32))
    }

    /// Layer > Align Linked: line up every linked layer against
    /// the active layer. Modes: 0 top, 1 v-center, 2 bottom,
    /// 3 left, 4 h-center, 5 right.
    pub(crate) fn align_linked(&mut self, mode: u8) {
        let a = self.doc.active_layer;
        let Some(r) = self.layer_bounds(a) else {
            self.status = "Align: the active layer is empty".into();
            return;
        };
        let targets: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != a && l.linked)
            .map(|(i, _)| i)
            .collect();
        if targets.is_empty() {
            self.status = "Align Linked: link some layers first".into();
            return;
        }
        let mut moves: Vec<(usize, i32, i32)> = Vec::new();
        for t in &targets {
            let Some(b) = self.layer_bounds(*t) else {
                continue;
            };
            let (mut dx, mut dy) = (0.0f32, 0.0f32);
            match mode {
                0 => dy = r.1 - b.1,
                1 => dy = (r.1 + r.3) / 2.0 - (b.1 + b.3) / 2.0,
                2 => dy = r.3 - b.3,
                3 => dx = r.0 - b.0,
                4 => dx = (r.0 + r.2) / 2.0 - (b.0 + b.2) / 2.0,
                _ => dx = r.2 - b.2,
            }
            moves.push((*t, dx.round() as i32, dy.round() as i32));
        }
        if moves.is_empty() {
            return;
        }
        self.structural("Align Linked", |doc| {
            for (t, dx, dy) in &moves {
                let o = doc.layers[*t].offset;
                doc.layers[*t].offset = (o.0 + dx, o.1 + dy);
            }
        });
        self.status = "Layers aligned".into();
    }

    /// Layer > Distribute Linked: space the linked layers (plus
    /// the active one) evenly along an axis; the two end layers
    /// stay put. Needs at least three layers in the set.
    pub(crate) fn distribute_linked(&mut self, mode: u8) {
        let a = self.doc.active_layer;
        let mut set: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i == a || l.linked)
            .map(|(i, _)| i)
            .collect();
        if set.len() < 3 {
            self.status = "Distribute needs the active layer plus two linked".into();
            return;
        }
        // Anchor per layer: the mode's edge/center coordinate.
        let anchor = |me: &Self, li: usize| -> Option<f32> {
            let b = me.layer_bounds(li)?;
            Some(match mode {
                0 => b.1,
                1 => (b.1 + b.3) / 2.0,
                2 => b.3,
                3 => b.0,
                4 => (b.0 + b.2) / 2.0,
                _ => b.2,
            })
        };
        let mut keyed: Vec<(usize, f32)> = Vec::new();
        for li in set.drain(..) {
            if let Some(v) = anchor(self, li) {
                keyed.push((li, v));
            }
        }
        if keyed.len() < 3 {
            self.status = "Distribute: too many empty layers".into();
            return;
        }
        keyed.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        let first = keyed.first().unwrap().1;
        let last = keyed.last().unwrap().1;
        let step = (last - first) / (keyed.len() - 1) as f32;
        let vertical = mode <= 2;
        let mut moves: Vec<(usize, i32, i32)> = Vec::new();
        for (k, (li, cur)) in keyed.iter().enumerate() {
            let want = first + step * k as f32;
            let d = (want - cur).round() as i32;
            if d != 0 {
                moves.push((
                    *li,
                    if vertical { 0 } else { d },
                    if vertical { d } else { 0 },
                ));
            }
        }
        if moves.is_empty() {
            self.status = "Already evenly spaced".into();
            return;
        }
        self.structural("Distribute Linked", |doc| {
            for (t, dx, dy) in &moves {
                let o = doc.layers[*t].offset;
                doc.layers[*t].offset = (o.0 + dx, o.1 + dy);
            }
        });
        self.status = "Layers distributed".into();
    }

    /// Select > Transform Selection: the same transform box, but
    /// it warps the selection mask rather than any layer.
    pub(crate) fn begin_transform_selection(&mut self) {
        if self.ft.is_some() {
            self.ft_commit();
            return;
        }
        let Some(sel) = self.sel.clone() else {
            self.status = "Transform Selection needs a selection".into();
            return;
        };
        let Some((bx, by, bw, bh)) = sel.bounds() else {
            self.status = "Transform Selection needs a selection".into();
            return;
        };
        self.ft = Some(FtState {
            origin: self.doc.clone(),
            targets: Vec::new(),
            bounds: (
                bx as f32,
                by as f32,
                (bx + bw as i32) as f32,
                (by + bh as i32) as f32,
            ),
            sx: 1.0,
            sy: 1.0,
            rot: 0.0,
            skew: 0.0,
            dx: 0.0,
            dy: 0.0,
            drag: FtDrag::None,
            drag_start_doc: (0.0, 0.0),
            start: (1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            start_angle: 0.0,
            quad: None,
            start_quad: None,
            sel_mode: true,
            origin_sel: Some(sel),
        });
        self.status = "Transform Selection: drag handles — Enter applies, Esc cancels".into();
    }

    /// Re-derive the document from the FT origin + current params.
    pub(crate) fn ft_apply(&mut self) {
        let Some(ft) = &self.ft else { return };
        let (sx, sy, rot, skew, dx, dy) = (ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy);
        let targets = ft.targets.clone();
        // Transform Selection: warp the mask, leave the doc alone.
        if ft.sel_mode {
            if let Some(origin_sel) = &ft.origin_sel {
                let corners = Self::ft_corners(ft);
                let mut s = origin_sel.clone();
                pictodesk_core::transform::transform_selection_quad(&mut s, corners);
                self.sel = Some(s);
            }
            self.status = "Transform Selection".into();
            self.touch();
            return;
        }
        let identity = (sx - 1.0).abs() < 1e-6
            && (sy - 1.0).abs() < 1e-6
            && rot.abs() < 1e-6
            && skew.abs() < 1e-6;
        self.doc = ft.origin.clone();
        if let Some(quad) = ft.quad {
            for t in &targets {
                pictodesk_core::transform::transform_layer_quad(&mut self.doc, *t, quad);
            }
            self.status = "Free Transform — Distort".into();
            self.touch();
            return;
        }
        for t in &targets {
            if identity {
                let o = self.doc.layers[*t].offset;
                self.doc.layers[*t].offset = (o.0 + dx.round() as i32, o.1 + dy.round() as i32);
            } else {
                pictodesk_core::transform::transform_layer(&mut self.doc, *t, sx, sy, rot, skew);
                self.doc.layers[*t].offset = (dx.round() as i32, dy.round() as i32);
            }
        }
        self.status = format!(
            "Free Transform — W: {}%  H: {}%  Angle: {:.1}°  Skew: {:.1}°",
            (sx * 100.0).round() as i32,
            (sy * 100.0).round() as i32,
            rot,
            skew
        );
        self.touch();
    }

    /// The four transform-box corners under the current params:
    /// the free quad when distorting, else the affine image of
    /// the bounds corners.
    pub(crate) fn ft_corners(ft: &FtState) -> [(f32, f32); 4] {
        if let Some(q) = ft.quad {
            return q;
        }
        let (x0, y0, x1, y1) = ft.bounds;
        [
            Self::ft_point(ft, (x0, y0)),
            Self::ft_point(ft, (x1, y0)),
            Self::ft_point(ft, (x1, y1)),
            Self::ft_point(ft, (x0, y1)),
        ]
    }

    pub(crate) fn ft_commit(&mut self) {
        if let Some(ft) = self.ft.take() {
            if ft.sel_mode {
                self.status = "Transform Selection applied".into();
                self.touch();
                return;
            }
            // Remember the gesture for Edit > Transform Again.
            let (x0, y0, x1, y1) = ft.bounds;
            let base = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
            let rel_quad = ft.quad.map(|q| {
                let mut rel = [(0.0f32, 0.0f32); 4];
                for i in 0..4 {
                    rel[i] = (q[i].0 - base[i].0, q[i].1 - base[i].1);
                }
                rel
            });
            self.last_ft = Some((ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy, rel_quad));
            if ft.origin != self.doc {
                let mut cmd = DocumentCommand::begin("Free Transform", &ft.origin);
                cmd.finish(&self.doc);
                self.push_cmd(Box::new(cmd));
                self.last_label = Some("Free Transform".into());
            }
            self.status = "Free Transform applied".into();
            self.touch();
        }
    }

    pub(crate) fn ft_cancel(&mut self) {
        if let Some(ft) = self.ft.take() {
            if ft.sel_mode {
                self.sel = ft.origin_sel.clone();
                self.status = "Transform Selection cancelled".into();
                self.touch();
                return;
            }
            self.doc = ft.origin;
            self.status = "Free Transform cancelled".into();
            self.touch();
        }
    }

    /// Map a document point through the current FT params (matches
    /// the core transform: scale, then skew, then rotate, about the
    /// bounds center, then translate).
    pub(crate) fn ft_point(ft: &FtState, p: (f32, f32)) -> (f32, f32) {
        let cx = (ft.bounds.0 + ft.bounds.2) / 2.0;
        let cy = (ft.bounds.1 + ft.bounds.3) / 2.0;
        let rel = (p.0 - cx, p.1 - cy);
        let mut v = (rel.0 * ft.sx, rel.1 * ft.sy);
        v.0 += ft.skew.to_radians().tan() * v.1;
        let th = ft.rot.to_radians();
        let (sn, cs) = (th.sin(), th.cos());
        let r = (v.0 * cs - v.1 * sn, v.0 * sn + v.1 * cs);
        (r.0 + cx + ft.dx, r.1 + cy + ft.dy)
    }

    /// The 8 handle positions (doc coords): corners then edge
    /// midpoints, in the transformed frame.
    pub(crate) fn ft_handles(ft: &FtState) -> [(f32, f32); 8] {
        let c = Self::ft_corners(ft);
        let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        [
            c[0],
            c[1],
            c[2],
            c[3],
            mid(c[0], c[1]),
            mid(c[1], c[2]),
            mid(c[2], c[3]),
            mid(c[3], c[0]),
        ]
    }

    /// Begin an FT drag if the canvas click hits the transform box.
    pub(crate) fn ft_mouse_down(&mut self, dp: (f32, f32), sx: f32, sy: f32) -> bool {
        let Some(ft) = &self.ft else { return false };
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let handles = Self::ft_handles(ft);
        let mut drag = FtDrag::None;
        for (i, h) in handles.iter().enumerate() {
            let s = self.viewport.doc_to_screen(*h);
            let (hx, hy) = (canvas.x as f32 + s.0, canvas.y as f32 + s.1);
            if (sx - hx).abs() <= 7.0 && (sy - hy).abs() <= 7.0 {
                let primary = self.ctrl_down || self.super_down;
                drag = if i < 4 && (primary || ft.quad.is_some()) {
                    // Corner + Ctrl (or already distorted): free
                    // corner; with Shift it is Perspective.
                    FtDrag::Quad(i, self.shift_down)
                } else if primary {
                    FtDrag::Skew
                } else {
                    FtDrag::Scale(i)
                };
                break;
            }
        }
        if drag == FtDrag::None {
            // Inside the (axis-aligned bound of the) box: move.
            // Just outside it: rotate.
            let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for h in &handles {
                bb.0 = bb.0.min(h.0);
                bb.1 = bb.1.min(h.1);
                bb.2 = bb.2.max(h.0);
                bb.3 = bb.3.max(h.1);
            }
            if dp.0 >= bb.0 && dp.0 <= bb.2 && dp.1 >= bb.1 && dp.1 <= bb.3 {
                drag = FtDrag::Move;
            } else {
                let pad = 24.0 / self.viewport.scale.max(0.01);
                if dp.0 >= bb.0 - pad
                    && dp.0 <= bb.2 + pad
                    && dp.1 >= bb.1 - pad
                    && dp.1 <= bb.3 + pad
                {
                    drag = FtDrag::Rotate;
                }
            }
        }
        if drag == FtDrag::None {
            return false;
        }
        let ft = self.ft.as_mut().unwrap();
        if matches!(drag, FtDrag::Quad(..)) && ft.quad.is_none() {
            // Enter Distort: freeze the current affine corners as
            // the free quad; linked targets drop out (a quad is
            // fitted to one layer's bounds).
            ft.quad = Some(Self::ft_corners(ft));
            let a = ft.origin.active_layer;
            ft.targets = vec![a];
        }
        ft.start_quad = ft.quad;
        ft.drag = drag;
        ft.drag_start_doc = dp;
        ft.start = (ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy);
        let cx = (ft.bounds.0 + ft.bounds.2) / 2.0 + ft.dx;
        let cy = (ft.bounds.1 + ft.bounds.3) / 2.0 + ft.dy;
        ft.start_angle = (dp.1 - cy).atan2(dp.0 - cx);
        true
    }

    pub(crate) fn ft_mouse_move(&mut self, dp: (f32, f32)) {
        let Some(ft) = &mut self.ft else { return };
        if ft.drag == FtDrag::None {
            return;
        }
        let (sx0, sy0, rot0, skew0, dx0, dy0) = ft.start;
        let (x0, y0, x1, y1) = ft.bounds;
        let (bw, bh) = (x1 - x0, y1 - y0);
        let center = ((x0 + x1) / 2.0 + dx0, (y0 + y1) / 2.0 + dy0);
        match ft.drag {
            FtDrag::Move => {
                ft.dx = dx0 + (dp.0 - ft.drag_start_doc.0);
                ft.dy = dy0 + (dp.1 - ft.drag_start_doc.1);
            }
            FtDrag::Rotate => {
                let ang = (dp.1 - center.1).atan2(dp.0 - center.0);
                let mut deg = rot0 + (ang - ft.start_angle).to_degrees();
                while deg > 180.0 {
                    deg -= 360.0;
                }
                while deg < -180.0 {
                    deg += 360.0;
                }
                ft.rot = deg;
            }
            FtDrag::Scale(i) => {
                let hx = [
                    -bw / 2.0,
                    bw / 2.0,
                    bw / 2.0,
                    -bw / 2.0,
                    0.0,
                    bw / 2.0,
                    0.0,
                    -bw / 2.0,
                ][i];
                let hy = [
                    -bh / 2.0,
                    -bh / 2.0,
                    bh / 2.0,
                    bh / 2.0,
                    -bh / 2.0,
                    0.0,
                    bh / 2.0,
                    0.0,
                ][i];
                let th = rot0.to_radians();
                let (sn, cs) = (th.sin(), th.cos());
                let rel = (dp.0 - center.0, dp.1 - center.1);
                let mu = rel.0 * cs + rel.1 * sn;
                let mv = -rel.0 * sn + rel.1 * cs;
                if hx != 0.0 {
                    ft.sx = (mu / hx).clamp(0.05, 20.0);
                }
                if hy != 0.0 {
                    ft.sy = (mv / hy).clamp(0.05, 20.0);
                }
                let _ = (sx0, sy0);
            }
            FtDrag::Skew => {
                let th = rot0.to_radians();
                let (sn, cs) = (th.sin(), th.cos());
                let d = (dp.0 - ft.drag_start_doc.0, dp.1 - ft.drag_start_doc.1);
                let du = d.0 * cs + d.1 * sn;
                let shear = skew0.to_radians().tan() + du / bh.max(1.0);
                ft.skew = shear.atan().to_degrees().clamp(-80.0, 80.0);
            }
            FtDrag::Quad(i, perspective) => {
                if let (Some(q), Some(sq)) = (&mut ft.quad, ft.start_quad) {
                    let d = (dp.0 - ft.drag_start_doc.0, dp.1 - ft.drag_start_doc.1);
                    if perspective {
                        // Move along the dominant axis only; the
                        // partner corners converge symmetrically.
                        if d.0.abs() >= d.1.abs() {
                            q[i] = (sq[i].0 + d.0, sq[i].1);
                            let p = i ^ 1;
                            q[p] = (sq[p].0 - d.0, sq[p].1);
                        } else {
                            q[i] = (sq[i].0, sq[i].1 + d.1);
                            let p = i ^ 3;
                            q[p] = (sq[p].0, sq[p].1 - d.1);
                        }
                    } else {
                        q[i] = dp;
                    }
                }
            }
            FtDrag::None => {}
        }
        self.ft_apply();
    }

    /// Begin a pen/path edit gesture: snapshot the document so the
    /// whole gesture becomes a single undo step on commit.
    pub(crate) fn begin_path_edit(&mut self) {
        if self.path_before.is_none() {
            self.path_before = Some(self.doc.clone());
        }
    }

    /// Finish a pen/path edit gesture: if the document changed,
    /// push the snapshot pair as one history command.
    pub(crate) fn commit_path_edit(&mut self, label: &str) {
        if let Some(before) = self.path_before.take() {
            if before != self.doc {
                let mut cmd = DocumentCommand::begin(label, &before);
                cmd.finish(&self.doc);
                self.push_cmd(Box::new(cmd));
                self.last_label = Some(label.to_string());
            }
        }
    }

    /// One brush/pencil/eraser stamp into the targeted alpha channel.
    pub(crate) fn alpha_stamp(&mut self, center: (f32, f32)) {
        let PaintTarget::Alpha(a) = self.paint_target else {
            return;
        };
        let value = if self.tool == Tool::Eraser {
            0
        } else {
            self.fg_luminance()
        };
        let tip = if self.tool == Tool::Pencil {
            BrushTip::new(self.tip.size, 1.0)
        } else {
            self.tip.clone()
        };
        let (w, h) = (self.doc.width, self.doc.height);
        if let Some(ch) = self.doc.alphas.get_mut(a) {
            brush::stamp_channel(
                &mut ch.data,
                w,
                h,
                center.0,
                center.1,
                &tip,
                value,
                self.opacity,
            );
        }
        self.touch();
    }

    /// One brush/pencil/eraser stamp into the targeted layer mask.
    /// Painting with white reveals, black hides; the eraser paints
    /// with the background color, as on a mask in the classic editor.
    pub(crate) fn mask_stamp(&mut self, center: (f32, f32)) {
        let PaintTarget::Mask(li) = self.paint_target else {
            return;
        };
        let value = if self.tool == Tool::Eraser {
            let (r, g, b, _) = self.bg.to_rgba8();
            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
        } else {
            self.fg_luminance()
        };
        let tip = if self.tool == Tool::Pencil {
            BrushTip::new(self.tip.size, 1.0)
        } else {
            self.tip.clone()
        };
        let (w, h) = (self.doc.width, self.doc.height);
        if li < self.doc.layers.len() && self.doc.layers[li].mask.is_some() {
            brush::stamp_mask(
                &mut self.doc.layers[li],
                w,
                h,
                center.0,
                center.1,
                &tip,
                value,
                self.opacity,
            );
        }
        self.touch();
    }

    /// Pen tool click: grab a handle/anchor if near one, close the
    /// current subpath if the first anchor is clicked, otherwise add
    /// an anchor (drag turns it into a smooth point).
    /// Snap a point to the strongest luminance edge within a
    /// 10 px radius (Magnetic Lasso / Magnetic Pen). The luma
    /// plane of the flattened image is cached per doc_version.
    pub(crate) fn mag_snap(&mut self, dp: (f32, f32)) -> (f32, f32) {
        if self.mag_cache.as_ref().map(|(v, _)| *v) != Some(self.doc_version) {
            let rgba = self.doc.render_full_rgba8();
            let mut luma = Vec::with_capacity((self.doc.width * self.doc.height) as usize);
            for px in rgba.as_chunks::<4>().0 {
                luma.push(
                    ((px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29) / 256) as u8,
                );
            }
            self.mag_cache = Some((self.doc_version, luma));
        }
        let (w, h) = (self.doc.width as i32, self.doc.height as i32);
        let (_, luma) = self.mag_cache.as_ref().unwrap();
        let at = |x: i32, y: i32| -> i32 {
            if x < 0 || y < 0 || x >= w || y >= h {
                0
            } else {
                luma[(y * w + x) as usize] as i32
            }
        };
        let (cx, cy) = (dp.0.round() as i32, dp.1.round() as i32);
        let mut best = (dp.0, dp.1);
        let mut best_score = -1i32;
        for yy in (cy - 10)..=(cy + 10) {
            for xx in (cx - 10)..=(cx + 10) {
                if xx < 0 || yy < 0 || xx >= w || yy >= h {
                    continue;
                }
                let g = (at(xx + 1, yy) - at(xx - 1, yy)).abs()
                    + (at(xx, yy + 1) - at(xx, yy - 1)).abs();
                // Prefer strong edges; break ties toward the cursor.
                let d2 = (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy);
                let score = g * 64 - d2;
                if score > best_score {
                    best_score = score;
                    best = (xx as f32, yy as f32);
                }
            }
        }
        best
    }

    /// 3D Transform dialog: the wireframe preview rect inside
    /// the dialog (below the shape buttons, above the sliders).
    pub(crate) fn wireframe_rect(&self) -> Option<Rect> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        Some(Rect::new(r.x + 12, r.y + 54, r.w - 24, 188))
    }

    /// Extract dialog: the mini-editor rect inside the dialog.
    pub(crate) fn extract_editor_rect(&self) -> Option<Rect> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        Some(Rect::new(r.x + 12, r.y + 30, r.w - 24, 276))
    }

    /// Map a window point in the Extract editor to doc coords.
    pub(crate) fn extract_doc_point(&self, mx: i32, my: i32) -> Option<(i32, i32)> {
        let er = self.extract_editor_rect()?;
        if !er.contains(mx, my) {
            return None;
        }
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let scale = (er.w as f32 / w).min(er.h as f32 / h);
        let (ox, oy) = (
            er.x as f32 + (er.w as f32 - w * scale) / 2.0,
            er.y as f32 + (er.h as f32 - h * scale) / 2.0,
        );
        let dx = ((mx as f32 - ox) / scale).floor() as i32;
        let dy = ((my as f32 - oy) / scale).floor() as i32;
        if dx >= 0 && dy >= 0 && (dx as u32) < self.doc.width && (dy as u32) < self.doc.height {
            Some((dx, dy))
        } else {
            None
        }
    }

    /// Paint one Extract tool dab at a doc point.
    pub(crate) fn extract_paint(&mut self, dp: (i32, i32)) {
        let mut on_highlight = false;
        {
            let Some(d) = &mut self.dialog else { return };
            if d.kind != DialogKind::Extract {
                return;
            }
            let (w, h) = (self.doc.width as i32, self.doc.height as i32);
            let idx = |x: i32, y: i32| (y * w + x) as usize;
            match d.extract_mode {
                0 => {
                    // Smart Highlighting (6.0): snap the brush
                    // centre to the strongest luminance edge in
                    // a 7x7 neighbourhood before stamping.
                    let dp = if d.extract_smart {
                        let layer = &self.doc.layers[self.doc.active_layer];
                        let lum = |x: i32, y: i32| -> i32 {
                            let p = layer.get_pixel(x, y).to_rgba8();
                            (p.0 as i32 * 3 + p.1 as i32 * 5 + p.2 as i32 * 2) / 10
                        };
                        let (mut best, mut bd) = (dp, -1i32);
                        for yy in (dp.1 - 3)..=(dp.1 + 3) {
                            for xx in (dp.0 - 3)..=(dp.0 + 3) {
                                let g = (lum(xx + 1, yy) - lum(xx - 1, yy)).abs()
                                    + (lum(xx, yy + 1) - lum(xx, yy - 1)).abs();
                                if g > bd {
                                    bd = g;
                                    best = (xx, yy);
                                }
                            }
                        }
                        best
                    } else {
                        dp
                    };
                    // Highlight brush: a soft disc of edge marks.
                    for yy in (dp.1 - 6)..=(dp.1 + 6) {
                        for xx in (dp.0 - 6)..=(dp.0 + 6) {
                            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                                let dd = (xx - dp.0) * (xx - dp.0) + (yy - dp.1) * (yy - dp.1);
                                if dd <= 36 {
                                    d.extract_hi[idx(xx, yy)] = 255;
                                }
                            }
                        }
                    }
                }
                1 => {
                    // Fill: flood the region the highlight encloses.
                    if dp.0 < 0 || dp.1 < 0 || dp.0 >= w || dp.1 >= h {
                        return;
                    }
                    if d.extract_hi[idx(dp.0, dp.1)] > 0 {
                        on_highlight = true;
                    }
                    let mut stack = vec![(dp.0, dp.1)];
                    while let Some((cx, cy)) = stack.pop() {
                        if cx < 0 || cy < 0 || cx >= w || cy >= h {
                            continue;
                        }
                        let i = idx(cx, cy);
                        if d.extract_hi[i] > 0 || d.extract_fill[i] > 0 {
                            continue;
                        }
                        d.extract_fill[i] = 255;
                        stack.push((cx - 1, cy));
                        stack.push((cx + 1, cy));
                        stack.push((cx, cy - 1));
                        stack.push((cx, cy + 1));
                    }
                }
                3 => {
                    // Edge Touchup (6.0): mark pixels whose
                    // original content is painted back after
                    // the extraction runs.
                    for yy in (dp.1 - 6)..=(dp.1 + 6) {
                        for xx in (dp.0 - 6)..=(dp.0 + 6) {
                            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                                let dd = (xx - dp.0) * (xx - dp.0) + (yy - dp.1) * (yy - dp.1);
                                if dd <= 36 {
                                    d.extract_touch[idx(xx, yy)] = 255;
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Eraser: clear both masks under the brush.
                    for yy in (dp.1 - 6)..=(dp.1 + 6) {
                        for xx in (dp.0 - 6)..=(dp.0 + 6) {
                            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                                let dd = (xx - dp.0) * (xx - dp.0) + (yy - dp.1) * (yy - dp.1);
                                if dd <= 36 {
                                    d.extract_hi[idx(xx, yy)] = 0;
                                    d.extract_fill[idx(xx, yy)] = 0;
                                }
                            }
                        }
                    }
                }
            }
        }
        if on_highlight {
            self.status = "Fill inside the highlighted edge".into();
        }
        self.extract_preview_cache = None;
        if self
            .dialog
            .as_ref()
            .map(|d| d.extract_preview)
            .unwrap_or(false)
        {
            self.compute_extract_preview();
        }
    }

    /// Recompute the Extract preview (the filter applied to a
    /// scratch copy of the document).
    pub(crate) fn compute_extract_preview(&mut self) {
        let Some(d) = &self.dialog else { return };
        if d.kind != DialogKind::Extract {
            return;
        }
        let (hi, fill) = (d.extract_hi.clone(), d.extract_fill.clone());
        let mut scratch = self.doc.clone();
        let layer = scratch.active_layer;
        filter::extract(&mut scratch, layer, &hi, &fill);
        self.extract_preview_cache = Some(scratch.render_full_rgba8());
    }

    /// Edit > Define Custom Shape: from the active path when one
    /// exists (its anchors, normalized to their bounds), else
    /// from the current selection's bounds as a rectangle shape.
    pub(crate) fn define_custom_shape(&mut self) {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        if let Some((_, path)) = self.active_path_clone() {
            let mut anchors: Vec<(f32, f32)> = path
                .subpaths
                .iter()
                .flat_map(|sp| sp.anchors.iter().map(|a| a.point))
                .collect();
            if anchors.len() >= 3 {
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y) in &anchors {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(*x);
                    y1 = y1.max(*y);
                }
                let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
                pts = anchors
                    .drain(..)
                    .map(|(x, y)| ((x - x0) / bw, (y - y0) / bh))
                    .collect();
            }
        }
        if pts.len() < 3 {
            match self.sel.as_ref().and_then(|s| s.bounds()) {
                Some((x, y, w, h)) => {
                    let _ = (x, y, w, h);
                    pts = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
                }
                None => {
                    self.status =
                        "Define Custom Shape: draw a path or make a selection first".into();
                    return;
                }
            }
        }
        let n = self.doc.custom_shapes.len() + 1;
        let name = format!("Shape {n}");
        self.structural("Define Custom Shape", |d| {
            d.custom_shapes.push(pictodesk_core::document::CustomShape {
                name: name.clone(),
                points: pts.clone(),
            });
        });
        self.status = format!("Custom shape '{name}' defined");
    }

    /// Close the in-progress polygonal selection.
    pub(crate) fn close_poly_selection(&mut self) {
        if self.poly_points.len() >= 3 {
            let p = Selection::polygon(self.doc.width, self.doc.height, &self.poly_points);
            self.replace_sel(Some(p));
            self.status = "Selection closed".into();
        }
        self.poly_points.clear();
    }

    /// The custom-shape choices: three built-ins, then the
    /// document's defined shapes. Returns (name, points).
    pub(crate) fn custom_shape_choice(&self, idx: usize) -> (String, Vec<(f32, f32)>) {
        let builtins: [(&str, Vec<(f32, f32)>); 3] = [
            (
                "Star",
                (0..10)
                    .map(|i| {
                        let a =
                            i as f32 / 10.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                        let f = if i % 2 == 1 { 0.5 } else { 1.0 };
                        (0.5 + a.cos() * 0.5 * f, 0.5 + a.sin() * 0.5 * f)
                    })
                    .collect(),
            ),
            (
                "Arrow",
                vec![
                    (0.0, 0.3),
                    (0.6, 0.3),
                    (0.6, 0.0),
                    (1.0, 0.5),
                    (0.6, 1.0),
                    (0.6, 0.7),
                    (0.0, 0.7),
                ],
            ),
            (
                "Heart",
                vec![
                    (0.5, 0.95),
                    (0.05, 0.55),
                    (0.0, 0.3),
                    (0.15, 0.05),
                    (0.35, 0.05),
                    (0.5, 0.25),
                    (0.65, 0.05),
                    (0.85, 0.05),
                    (1.0, 0.3),
                    (0.95, 0.55),
                ],
            ),
        ];
        if idx < builtins.len() {
            let (n, pts) = &builtins[idx];
            return (n.to_string(), pts.clone());
        }
        match self.doc.custom_shapes.get(idx - builtins.len()) {
            Some(cs) => (cs.name.clone(), cs.points.clone()),
            None => (builtins[0].0.to_string(), builtins[0].1.clone()),
        }
    }

    /// Shape tool press: edit the active shape layer when it
    /// carries the same kind, else start a new shape layer.
    pub(crate) fn shape_press(&mut self, dp: (f32, f32)) {
        use pictodesk_core::shapes::ShapeSpec;
        let kind: u8 = match self.tool {
            Tool::ShapeRect => 0,
            Tool::ShapeRounded => 1,
            Tool::ShapeEllipse => 2,
            Tool::ShapePolygon => 3,
            Tool::ShapeLine => 4,
            _ => 5,
        };
        self.begin_path_edit();
        let a = self.doc.active_layer;
        let reuse = self
            .doc
            .layers
            .get(a)
            .and_then(|l| l.shape.as_ref())
            .map(|sp| sp.kind == kind)
            .unwrap_or(false);
        let (x, y) = (dp.0.floor() as i32, dp.1.floor() as i32);
        if reuse {
            self.shape_drag_layer = Some(a);
            if let Some(sp) = &mut self.doc.layers[a].shape {
                sp.rect = (x, y, 1, 1);
            }
        } else {
            let mut spec = ShapeSpec {
                kind,
                rect: (x, y, 1, 1),
                sides: self.shape_sides,
                star: self.shape_star,
                fill: self.fg,
                fill_on: true,
                stroke: self.bg,
                stroke_width: if kind == 4 {
                    self.shape_stroke.max(1)
                } else {
                    self.shape_stroke
                },
                ..Default::default()
            };
            if kind == 5 {
                let (_, pts) = self.custom_shape_choice(self.custom_idx);
                spec.points = pts;
            }
            if kind == 4 {
                spec.arrow_start = self.line_arrow_start;
                spec.arrow_end = self.line_arrow_end;
            }
            let name = self.tool.name().to_string();
            let li = self.doc.add_layer(name);
            self.doc.layers[li].shape = Some(spec);
            self.doc.active_layer = li;
            self.shape_drag_layer = Some(li);
        }
        self.drag = DragMode::ShapeDrag;
        self.touch();
    }

    /// Convert Anchor Point tool (Phase 6.5 Batch 6c): clicking a
    /// smooth anchor collapses it to a corner; dragging a corner
    /// anchor pulls symmetric handles out of it.
    pub(crate) fn convert_anchor_down(&mut self, x: f32, y: f32) {
        self.begin_path_edit();
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let to_scr = |p: (f32, f32)| {
            let s = self.viewport.doc_to_screen(p);
            (canvas.x as f32 + s.0, canvas.y as f32 + s.1)
        };
        let Some((_, path)) = self.active_path_clone() else {
            self.status = "Convert: no active path".into();
            self.path_before = None;
            return;
        };
        for (si, sub) in path.subpaths.iter().enumerate() {
            for (ai, a) in sub.anchors.iter().enumerate() {
                let sp = to_scr(a.point);
                if (x - sp.0).abs() < 5.0 && (y - sp.1).abs() < 5.0 {
                    if a.is_corner() {
                        self.pen_hit = Some((si, ai, 2));
                        self.pen_anchor = Some((si, ai));
                        self.drag = DragMode::PenConvert;
                        self.status = "Drag to pull handles out".into();
                    } else if let Some(p) = self.target_path_mut() {
                        let an = &mut p.subpaths[si].anchors[ai];
                        an.in_ctrl = an.point;
                        an.out_ctrl = an.point;
                        self.commit_path_edit("Convert Anchor Point");
                        self.status = "Anchor converted to corner point".into();
                        self.touch();
                    }
                    return;
                }
            }
        }
        self.path_before = None;
        self.status = "Convert: click an anchor point".into();
    }

    /// Airbrush build-up (Phase 6.5 Batch 6c): while an airbrush
    /// stroke is held, each tick lays another low-opacity stamp at
    /// the current point, so color builds the longer you hold.
    pub(crate) fn airbrush_tick(&mut self) {
        let air = self.tool == Tool::Airbrush
            || (matches!(self.tool, Tool::Brush | Tool::Pencil) && self.brush_dyn.airbrush);
        if !air || self.drag != DragMode::Paint {
            return;
        }
        if self.alpha_cmd.is_some() {
            let p = self.alpha_last;
            if matches!(self.paint_target, PaintTarget::Mask(_)) {
                self.mask_stamp(p);
            } else {
                self.alpha_stamp(p);
            }
            return;
        }
        if self.stroke.is_some() {
            let p = self.stroke.as_ref().unwrap().last_point;
            self.capture_segment_tiles(p);
            self.stamp_at(p);
        }
    }

    pub(crate) fn pen_down(&mut self, x: f32, y: f32, dp: (f32, f32)) {
        self.begin_path_edit();
        self.ensure_pen_target();
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let to_scr = |p: (f32, f32)| {
            let s = self.viewport.doc_to_screen(p);
            (canvas.x as f32 + s.0, canvas.y as f32 + s.1)
        };
        if let Some((_, path)) = self.active_path_clone() {
            // handles of the current anchor first
            if let Some((si, ai)) = self.pen_anchor {
                if let Some(a) = path.subpaths.get(si).and_then(|sub| sub.anchors.get(ai)) {
                    if !a.is_corner() {
                        for (part, cp) in [(1u8, a.in_ctrl), (2u8, a.out_ctrl)] {
                            let sp = to_scr(cp);
                            if (x - sp.0).abs() < 5.0 && (y - sp.1).abs() < 5.0 {
                                self.pen_hit = Some((si, ai, part));
                                self.drag = DragMode::PenDrag;
                                return;
                            }
                        }
                    }
                }
            }
            // any anchor
            for (si, sub) in path.subpaths.iter().enumerate() {
                for (ai, a) in sub.anchors.iter().enumerate() {
                    let sp = to_scr(a.point);
                    if (x - sp.0).abs() < 5.0 && (y - sp.1).abs() < 5.0 {
                        self.pen_hit = Some((si, ai, 0));
                        self.pen_anchor = Some((si, ai));
                        self.drag = DragMode::PenDrag;
                        return;
                    }
                }
            }
            // close: click the first anchor of the current open subpath
            if let Some(li) = path.subpaths.iter().rposition(|sub| !sub.closed) {
                let sub = &path.subpaths[li];
                if sub.anchors.len() >= 2 {
                    let sp = to_scr(sub.anchors[0].point);
                    if (x - sp.0).abs() < 6.0 && (y - sp.1).abs() < 6.0 {
                        if let Some(p) = self.target_path_mut() {
                            p.subpaths[li].closed = true;
                        }
                        self.commit_path_edit("Close Path");
                        self.status = "Path closed".into();
                        self.touch();
                        return;
                    }
                }
            }
        }
        // add an anchor to the current open subpath (or a fresh one)
        if let Some(p) = self.target_path_mut() {
            let si = match p.subpaths.iter().rposition(|sub| !sub.closed) {
                Some(i) => i,
                None => {
                    p.subpaths.push(Subpath {
                        anchors: Vec::new(),
                        closed: false,
                    });
                    p.subpaths.len() - 1
                }
            };
            p.subpaths[si].anchors.push(Anchor::corner(dp.0, dp.1));
            let ai = p.subpaths[si].anchors.len() - 1;
            self.pen_hit = Some((si, ai, 0));
            self.pen_anchor = Some((si, ai));
        }
        self.drag = DragMode::PenNew;
        self.touch();
    }

    pub(crate) fn pen_move(&mut self, dp: (f32, f32)) {
        let Some((si, ai, part)) = self.pen_hit else {
            return;
        };
        let drag = self.drag;
        if let Some(p) = self.target_path_mut() {
            let Some(a) = p
                .subpaths
                .get_mut(si)
                .and_then(|sub| sub.anchors.get_mut(ai))
            else {
                return;
            };
            match drag {
                DragMode::PenNew => {
                    let pt = a.point;
                    if (dp.0 - pt.0).abs() > 0.5 || (dp.1 - pt.1).abs() > 0.5 {
                        a.out_ctrl = dp;
                        a.in_ctrl = (2.0 * pt.0 - dp.0, 2.0 * pt.1 - dp.1);
                    }
                }
                DragMode::PenConvert => {
                    a.out_ctrl = dp;
                    a.in_ctrl = (2.0 * a.point.0 - dp.0, 2.0 * a.point.1 - dp.1);
                }
                DragMode::PenDrag => match part {
                    0 => {
                        let d = (dp.0 - a.point.0, dp.1 - a.point.1);
                        a.point = dp;
                        a.in_ctrl = (a.in_ctrl.0 + d.0, a.in_ctrl.1 + d.1);
                        a.out_ctrl = (a.out_ctrl.0 + d.0, a.out_ctrl.1 + d.1);
                    }
                    1 => {
                        a.in_ctrl = dp;
                        if !a.is_corner() {
                            let v = (a.point.0 - dp.0, a.point.1 - dp.1);
                            let len = ((a.out_ctrl.0 - a.point.0).powi(2)
                                + (a.out_ctrl.1 - a.point.1).powi(2))
                            .sqrt();
                            let vl = (v.0.powi(2) + v.1.powi(2)).sqrt().max(0.001);
                            a.out_ctrl = (a.point.0 + v.0 / vl * len, a.point.1 + v.1 / vl * len);
                        }
                    }
                    _ => {
                        a.out_ctrl = dp;
                        if !a.is_corner() {
                            let v = (a.point.0 - dp.0, a.point.1 - dp.1);
                            let len = ((a.in_ctrl.0 - a.point.0).powi(2)
                                + (a.in_ctrl.1 - a.point.1).powi(2))
                            .sqrt();
                            let vl = (v.0.powi(2) + v.1.powi(2)).sqrt().max(0.001);
                            a.in_ctrl = (a.point.0 + v.0 / vl * len, a.point.1 + v.1 / vl * len);
                        }
                    }
                },
                _ => {}
            }
        }
    }

    pub(crate) fn begin_stroke(&mut self, dp: (f32, f32)) {
        let layer = self.doc.active_layer;
        let carry = if self.tool == Tool::Smudge {
            brush::sample_average(&self.doc, layer, dp.0 as i32, dp.1 as i32, 2)
        } else {
            self.fg
        };
        self.stroke_seed = self.stroke_seed.wrapping_add(1);
        let tex = if self.brush_dyn.texture_on {
            self.doc.pattern.as_ref().map(|p| {
                let lum: Vec<f32> =
                    p.px.iter()
                        .map(|px| {
                            let (r, g, b, _) = px.to_rgba8();
                            (0.3 * r as f32 + 0.59 * g as f32 + 0.11 * b as f32) / 255.0
                        })
                        .collect();
                std::rc::Rc::new(pictodesk_core::brush::BrushTexture {
                    w: p.width,
                    h: p.height,
                    lum,
                    scale: self.brush_dyn.tex_scale,
                    depth: self.brush_dyn.tex_depth,
                    min_depth: self.brush_dyn.tex_min_depth,
                    mode: self.brush_dyn.tex_mode,
                    invert: self.brush_dyn.tex_invert,
                })
            })
        } else {
            None
        };
        self.stroke = Some(StrokeState {
            cmd: TileEditCommand::new(self.tool.name()),
            start_point: dp,
            last_point: dp,
            smudge_carry: carry,
            tiles_seen: Vec::new(),
            rng: 0x9E37_79B9 ^ self.stroke_seed.wrapping_mul(7919),
            stamps: 0,
            smooth: dp,
            tex,
        });
        // Snapshot BEFORE the first stamp lands, or undo would keep it.
        self.capture_segment_tiles(dp);
        self.stamp_at(dp);
    }

    /// Phase 7 wrapper: apply the paint engine (dynamics,
    /// scattering, color dynamics, flow) around stamp_once.
    /// With default dynamics this is exactly one plain stamp.
    pub(crate) fn stamp_at(&mut self, center: (f32, f32)) {
        let bd = self.brush_dyn.clone();
        let tool = self.tool;
        let driven = matches!(
            tool,
            Tool::Brush
                | Tool::Airbrush
                | Tool::Pencil
                | Tool::Eraser
                | Tool::Stamp
                | Tool::PatternStamp
                | Tool::Heal
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Smudge
                | Tool::Dodge
                | Tool::Burn
                | Tool::Sponge
                | Tool::HistoryBrush
                | Tool::ArtHistoryBrush
                | Tool::BackgroundEraser
        );
        if !driven {
            self.stamp_once(center, self.tip.clone(), self.fg, self.opacity);
            return;
        }
        let idx = self.stroke.as_ref().map(|s| s.stamps).unwrap_or(0);
        if let Some(st) = &mut self.stroke {
            st.stamps += 1;
        }
        let fade = |control: u8| -> f32 {
            if control == 1 {
                (1.0 - idx as f32 / bd.fade_steps.max(1) as f32).clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        let mut rng = self.stroke.as_ref().map(|s| s.rng).unwrap_or(0x2545);
        let mut n = bd.count.max(1) as i32;
        if bd.count_jitter > 0.0 {
            n = (n as f32 * (1.0 - bd.count_jitter * next_rand(&mut rng)))
                .round()
                .max(1.0) as i32;
        }
        let last = self.stroke.as_ref().map(|s| s.last_point).unwrap_or(center);
        let (mut dirx, mut diry) = (center.0 - last.0, center.1 - last.1);
        let dl = (dirx * dirx + diry * diry).sqrt();
        if dl > 0.001 {
            dirx /= dl;
            diry /= dl;
        } else {
            dirx = 1.0;
            diry = 0.0;
        }
        let (perpx, perpy) = (-diry, dirx);
        let base_size = self.tip.size;
        for _ in 0..n {
            let mut tip = self.tip.clone();
            // Shape dynamics.
            let min_abs = bd.min_diameter * base_size;
            let mut size = min_abs + (base_size - min_abs) * fade(bd.size_control);
            if bd.size_jitter > 0.0 {
                size *= 1.0 - bd.size_jitter * next_rand(&mut rng);
            }
            tip.size = size.max(1.0);
            tip.angle = bd.angle_deg.to_radians()
                + bd.angle_jitter
                    * (next_rand(&mut rng) * 2.0 - 1.0)
                    * std::f32::consts::PI
                    * fade(bd.angle_control);
            let min_r = bd.min_roundness;
            let mut round = min_r + (bd.roundness - min_r) * fade(bd.round_control);
            if bd.round_jitter > 0.0 {
                round *= 1.0 - bd.round_jitter * next_rand(&mut rng);
            }
            tip.roundness = round.clamp(0.02, 1.0);
            tip.flip_x = bd.flip_x;
            tip.flip_y = bd.flip_y;
            tip.noise = bd.noise;
            tip.wet_edges = bd.wet_edges;
            tip.tex = self.stroke.as_ref().and_then(|s| s.tex.clone());
            if bd.dual_on {
                let dmag = bd.dual_scatter * bd.dual_size;
                tip.dual = Some(Box::new(pictodesk_core::brush::DualTip {
                    size: bd.dual_size,
                    hardness: bd.dual_hardness,
                    mask: None,
                    mode: bd.dual_mode,
                    dx: (next_rand(&mut rng) * 2.0 - 1.0) * dmag,
                    dy: (next_rand(&mut rng) * 2.0 - 1.0) * dmag,
                }));
            }
            // Scattering: perpendicular to the stroke, or
            // both axes.
            let mag = bd.scatter * base_size;
            let mut c = center;
            if mag > 0.0 {
                let a = (next_rand(&mut rng) * 2.0 - 1.0) * mag;
                c.0 += perpx * a;
                c.1 += perpy * a;
                if bd.both_axes {
                    let b = (next_rand(&mut rng) * 2.0 - 1.0) * mag;
                    c.0 += dirx * b;
                    c.1 += diry * b;
                }
            }
            // Color dynamics (foreground painting tools).
            let mut fg = self.fg;
            if matches!(tool, Tool::Brush | Tool::Airbrush | Tool::Pencil)
                && (bd.fb_jitter > 0.0
                    || bd.hue_jitter > 0.0
                    || bd.sat_jitter > 0.0
                    || bd.bri_jitter > 0.0)
            {
                let (fr, fg_, fb, fa) = fg.to_rgba8();
                let (br, bg_, bb, _) = self.bg.to_rgba8();
                let t = bd.fb_jitter * next_rand(&mut rng);
                let mix = |a: u8, b: u8| -> f32 { (a as f32 * (1.0 - t) + b as f32 * t) / 255.0 };
                let (mut h, mut s_, mut l) =
                    rgb_to_hsl_turns(mix(fr, br), mix(fg_, bg_), mix(fb, bb));
                h += bd.hue_jitter * (next_rand(&mut rng) - 0.5);
                s_ = (s_ * (1.0 + bd.sat_jitter * (next_rand(&mut rng) * 2.0 - 1.0)))
                    .clamp(0.0, 1.0);
                l = (l * (1.0 + bd.bri_jitter * (next_rand(&mut rng) * 2.0 - 1.0))).clamp(0.0, 1.0);
                let (r, g, b) = hsl_turns_to_rgb(h, s_, l);
                fg = Pixel16::from_rgba8(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    fa,
                );
            }
            // Opacity + flow dynamics.
            let mut op = self.opacity;
            if matches!(
                tool,
                Tool::Brush
                    | Tool::Airbrush
                    | Tool::Pencil
                    | Tool::Heal
                    | Tool::Stamp
                    | Tool::PatternStamp
            ) {
                let mut fl = self.flow;
                if bd.flow_jitter > 0.0 {
                    fl *= 1.0 - bd.flow_jitter * next_rand(&mut rng);
                }
                if bd.flow_control == 1 {
                    fl *= fade(1);
                }
                op *= fl;
            }
            if bd.opacity_control == 1 {
                op *= fade(1);
            }
            if bd.opacity_jitter > 0.0 {
                op *= 1.0 - bd.opacity_jitter * next_rand(&mut rng);
            }
            if tool == Tool::Airbrush || (tool == Tool::Brush && bd.airbrush) {
                op *= 0.35;
            }
            self.stamp_once(c, tip, fg, op);
        }
        if let Some(st) = &mut self.stroke {
            st.rng = rng;
        }
    }

    pub(crate) fn stamp_once(
        &mut self,
        center: (f32, f32),
        tip: BrushTip,
        fg: Pixel16,
        opacity: f32,
    ) {
        let layer = self.doc.active_layer;
        let sel = self.sel.clone();
        let tool = self.tool;
        let (clone_src, start) = (
            self.clone_src,
            self.stroke
                .as_ref()
                .map(|s| s.start_point)
                .unwrap_or(center),
        );
        let bbox = match tool {
            Tool::Brush | Tool::Airbrush => brush::stamp_brush(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                fg,
                &tip,
                opacity,
            ),
            Tool::Pencil => brush::stamp_pencil(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                fg,
                tip.size,
                opacity,
            ),
            Tool::Eraser => {
                if self.eraser_to_history {
                    match self.hist_source_doc() {
                        Some(src) => brush::stamp_erase_to_history(
                            &mut self.doc,
                            layer,
                            sel.as_ref(),
                            center.0,
                            center.1,
                            &tip,
                            opacity,
                            &src,
                        ),
                        None => (0, 0, 0, 0),
                    }
                } else {
                    brush::stamp_eraser(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        &tip,
                        opacity,
                    )
                }
            }
            Tool::HistoryBrush => match self.hist_source_doc() {
                Some(src) => brush::stamp_sample(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    &tip,
                    opacity,
                    &|x, y| src.render_pixel(x, y),
                ),
                None => (0, 0, 0, 0),
            },
            Tool::ArtHistoryBrush => match self.hist_source_doc() {
                Some(src) => {
                    // Art History: the sample point wanders around
                    // the brush center; the style sets how far.
                    let mag = match self.art_hist_style {
                        0 => 2.0,
                        1 => 6.0,
                        _ => 3.0,
                    };
                    let seed = (center.0 * 7919.0 + center.1 * 104729.0) as i64;
                    let jx = ((seed % 13) as f32 / 6.0 - 1.0) * mag;
                    let jy = (((seed / 13) % 11) as f32 / 5.0 - 1.0) * mag;
                    let tip2 = if self.art_hist_style == 2 {
                        BrushTip::new(tip.size * 1.5, tip.hardness)
                    } else {
                        tip
                    };
                    brush::stamp_sample(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        &tip2,
                        opacity,
                        &|x, y| src.render_pixel(x + jx as i32, y + jy as i32),
                    )
                }
                None => (0, 0, 0, 0),
            },
            Tool::PatternStamp => brush::stamp_pattern(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                opacity,
            )
            .unwrap_or((0, 0, 0, 0)),
            Tool::BackgroundEraser => {
                let sample = self.doc.layers[layer].get_pixel(center.0 as i32, center.1 as i32);
                brush::stamp_erase_bg(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    &tip,
                    opacity,
                    sample,
                    self.eraser_tolerance,
                )
            }
            Tool::Stamp => {
                let src = clone_src.unwrap();
                let aligned = (src.0 + (center.0 - start.0), src.1 + (center.1 - start.1));
                if self.stamp_sample_merged {
                    brush::stamp_clone_merged(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        aligned,
                        &tip,
                        opacity,
                    )
                } else {
                    brush::stamp_clone(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        aligned,
                        &tip,
                        opacity,
                    )
                }
            }
            Tool::Heal => {
                if self.heal_use_pattern {
                    pictodesk_core::heal::stamp_heal_pattern(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        &tip,
                        opacity,
                    )
                } else {
                    let src = self.heal_src.unwrap_or(center);
                    let aligned = (src.0 + (center.0 - start.0), src.1 + (center.1 - start.1));
                    pictodesk_core::heal::stamp_heal(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        aligned,
                        &tip,
                        opacity,
                    )
                }
            }
            Tool::Blur => brush::stamp_blur(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                0.5,
            ),
            Tool::Sharpen => brush::stamp_sharpen(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                0.5,
            ),
            Tool::Smudge => {
                let carry = self.stroke.as_ref().unwrap().smudge_carry;
                let b = brush::stamp_smudge(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    carry,
                    &tip,
                    0.6,
                );
                let new_carry =
                    brush::sample_average(&self.doc, layer, center.0 as i32, center.1 as i32, 2);
                if let Some(s) = &mut self.stroke {
                    s.smudge_carry = new_carry;
                }
                b
            }
            Tool::Dodge | Tool::Burn | Tool::Sponge => brush::stamp_tone(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                match tool {
                    Tool::Dodge => brush::ToneKind::Dodge,
                    Tool::Burn => brush::ToneKind::Burn,
                    _ => brush::ToneKind::Sponge,
                },
                self.tone_range,
                self.tone_exposure,
                self.sponge_saturate,
                opacity,
            ),
            _ => return,
        };
        // Register tiles for undo. Segment capture (mouse_move /
        // begin_stroke) snapshots tiles before stamps land; anything
        // first seen here is captured now as a safety net.
        let tiles = Selection::tiles_in_rect(
            bbox.0,
            bbox.1,
            (bbox.2 - bbox.0) as u32,
            (bbox.3 - bbox.1) as u32,
        );
        if let Some(s) = &mut self.stroke {
            for t in tiles {
                if !s.tiles_seen.contains(&t) {
                    s.cmd.capture_before(&self.doc, layer, t);
                    s.tiles_seen.push(t);
                }
            }
        }
        self.doc_version += 1;
    }
}
