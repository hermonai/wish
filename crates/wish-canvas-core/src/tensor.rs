//! Tensor substrate — UI-agnostic shape, dtype and slicing math.
//!
//! ## Why this lives in `wish-canvas-core`
//!
//! The canvas already speaks "semantic node bound to a world entity". A
//! tensor *is* a kind of world entity — an embedding matrix, an attention
//! head, a layer activation. We want it to appear as a `CanvasNode`
//! alongside files and functions so the same hit-testing, layout, and
//! patching code applies. Putting the substrate here (and not in a new
//! crate) keeps the dep graph flat: any future `wish-tensor-view` crate
//! just imports `wish-canvas-core` and renders.
//!
//! ## Design choices
//!
//! - **Shape is `Vec<usize>`, not `[usize; N]`.** We don't know the rank
//!   at compile time — wishUI surfaces 1D / 2D / 3D / N-D tensors all in
//!   the same panel.
//! - **Data is `TensorRef`, not bytes by default.** Most tensors are too
//!   large to live in a `Canvas`; we carry a *handle* (a URI / blob id /
//!   inline-for-tiny). Renderers resolve handles on demand.
//! - **No GPU work here.** This crate is pure CPU types + index math.
//!   `wish-render` / `wish-tensor-view` upload textures.
//! - **Stable serialization.** `serde_derive` everywhere so a canvas
//!   round-trips through `wish-world-studio` exports unchanged.
//!
//! ## What the views built on top need from us
//!
//! 1. `TensorSpec::byte_size()` — buffer planning before upload.
//! 2. `TensorSpec::flat_index(coords)` — go from N-D coord → row-major
//!    offset, the indexing that every renderer ends up doing.
//! 3. `TensorSlice` — declarative "give me row 7" / "give me the
//!    [z=3] plane" without materializing data on this side.
//! 4. `CanvasNodeKind::Tensor(TensorSpec)` — placeable on a canvas.

use serde::{Deserialize, Serialize};

/// Element data type. We track the wishUI-relevant subset; bigger
/// tensors from real ML stacks (bfloat16, int4) get bucketed to the
/// nearest one for display until we add explicit support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TensorDType {
    F32,
    F64,
    I32,
    I64,
    U8,
    Bool,
}

impl TensorDType {
    /// Bytes per element. Bool is one byte (we don't pack — wishUI views
    /// don't benefit from the packing, and unpacking is annoying).
    pub const fn byte_size(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F64 | Self::I64 => 8,
            Self::U8 | Self::Bool => 1,
        }
    }
}

/// Where the tensor's element data actually lives.
///
/// Most real tensors stay out-of-canvas; the canvas just remembers the
/// shape and a handle. `Inline` is a convenience for tests and tiny
/// constants ("look, here's a 3×3 attention pattern") — guarded by a
/// soft cap so we don't accidentally serialize a gigabyte of weights
/// into a JSON canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TensorRef {
    /// Externally-owned blob — a Hermon `tensors/{id}` row, an S3 URI,
    /// a file path. Resolution is the renderer's job.
    External { uri: String },
    /// Inline f32 elements, row-major. Only suitable when
    /// `dims.iter().product() * 4 <= TENSOR_INLINE_MAX_BYTES`.
    InlineF32 { data: Vec<f32> },
    /// Inline u8 elements (greyscale heatmaps, masks, etc.).
    InlineU8 { data: Vec<u8> },
    /// No data attached — used when we want a shape-only placeholder on
    /// the canvas (e.g. layout/architecture mode).
    Empty,
}

/// Soft cap for `InlineF32` / `InlineU8` payloads. Anything bigger
/// should use `External` — otherwise the canvas serializes to disk
/// becomes a pain.
pub const TENSOR_INLINE_MAX_BYTES: usize = 1 << 20; // 1 MiB

/// Shape + dtype + data handle. The canonical "what's in this tensor"
/// record. `CanvasNodeKind::Tensor(TensorSpec)` is how a tensor shows
/// up on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TensorSpec {
    /// Row-major shape. Empty `dims` means scalar.
    pub dims: Vec<usize>,
    pub dtype: TensorDType,
    pub data: TensorRef,
    /// Optional human label rendered in the corner of a tensor view —
    /// for instance "attention[head=3]" or "weights/wte".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl TensorSpec {
    pub fn new(dims: Vec<usize>, dtype: TensorDType, data: TensorRef) -> Self {
        Self {
            dims,
            dtype,
            data,
            label: None,
        }
    }

    /// `true` for an empty-dims scalar.
    pub fn is_scalar(&self) -> bool {
        self.dims.is_empty()
    }

    pub fn rank(&self) -> usize {
        self.dims.len()
    }

    /// Total element count (1 for scalars). Returns `None` on overflow
    /// — pathologically large shapes shouldn't crash callers.
    pub fn element_count(&self) -> Option<usize> {
        self.dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d))
    }

    /// Number of bytes the dense data *would* take, or `None` on
    /// overflow.
    pub fn byte_size(&self) -> Option<usize> {
        self.element_count()?
            .checked_mul(self.dtype.byte_size())
    }

    /// Validate that an inline `TensorRef` matches the declared shape /
    /// dtype. `External` and `Empty` always pass — we trust the source.
    pub fn validate(&self) -> Result<(), TensorError> {
        match &self.data {
            TensorRef::Empty | TensorRef::External { .. } => Ok(()),
            TensorRef::InlineF32 { data } => {
                if self.dtype != TensorDType::F32 {
                    return Err(TensorError::DTypeMismatch);
                }
                let expected = self.element_count().ok_or(TensorError::ShapeOverflow)?;
                if data.len() != expected {
                    return Err(TensorError::ShapeMismatch {
                        expected,
                        actual: data.len(),
                    });
                }
                let bytes = expected.checked_mul(4).ok_or(TensorError::ShapeOverflow)?;
                if bytes > TENSOR_INLINE_MAX_BYTES {
                    return Err(TensorError::InlineTooLarge(bytes));
                }
                Ok(())
            }
            TensorRef::InlineU8 { data } => {
                if self.dtype != TensorDType::U8 && self.dtype != TensorDType::Bool {
                    return Err(TensorError::DTypeMismatch);
                }
                let expected = self.element_count().ok_or(TensorError::ShapeOverflow)?;
                if data.len() != expected {
                    return Err(TensorError::ShapeMismatch {
                        expected,
                        actual: data.len(),
                    });
                }
                if expected > TENSOR_INLINE_MAX_BYTES {
                    return Err(TensorError::InlineTooLarge(expected));
                }
                Ok(())
            }
        }
    }

    /// Row-major flat index from N-D coordinates. Returns `None` if
    /// `coords.len() != rank()` or any coord is out of range.
    ///
    /// ```text
    /// dims      = [D0, D1, …, Dk]
    /// strides   = [D1*…*Dk, D2*…*Dk, …, 1]
    /// idx(c)    = Σ ci * strides_i
    /// ```
    pub fn flat_index(&self, coords: &[usize]) -> Option<usize> {
        if coords.len() != self.dims.len() {
            return None;
        }
        let mut idx = 0usize;
        let mut stride = 1usize;
        for i in (0..self.dims.len()).rev() {
            if coords[i] >= self.dims[i] {
                return None;
            }
            idx = idx.checked_add(coords[i].checked_mul(stride)?)?;
            stride = stride.checked_mul(self.dims[i])?;
        }
        Some(idx)
    }

    /// Compute row-major strides (in *elements*, not bytes). The last
    /// stride is always 1. Returns `None` on overflow.
    pub fn strides(&self) -> Option<Vec<usize>> {
        if self.dims.is_empty() {
            return Some(Vec::new());
        }
        let mut s = vec![1usize; self.dims.len()];
        for i in (0..self.dims.len() - 1).rev() {
            s[i] = s[i + 1].checked_mul(self.dims[i + 1])?;
        }
        Some(s)
    }
}

/// Errors callers can hit when building or validating a tensor.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TensorError {
    #[error("tensor data length doesn't match declared shape (expected {expected}, got {actual})")]
    ShapeMismatch { expected: usize, actual: usize },
    #[error("tensor data dtype doesn't match declared dtype")]
    DTypeMismatch,
    #[error("tensor shape overflows usize")]
    ShapeOverflow,
    #[error("inline tensor data exceeds {TENSOR_INLINE_MAX_BYTES}-byte cap (got {0} bytes) — use TensorRef::External")]
    InlineTooLarge(usize),
    #[error("tensor slice axis {axis} out of bounds for rank {rank}")]
    SliceAxisOob { axis: usize, rank: usize },
    #[error("tensor slice index {index} out of bounds for axis size {size}")]
    SliceIndexOob { index: usize, size: usize },
}

/// Declarative slice request — "give me everything where axis A = i".
/// Multiple `Pin`s on different axes are AND-ed; the returned subspec
/// keeps only the unpinned axes.
///
/// Examples:
/// - Pin axis 0 to row 7 of a [N, D] matrix  → returns a [D] vector spec.
/// - Pin axis 2 to plane 3 of a [H, W, D]    → returns a [H, W] heatmap.
/// - No pins on a rank-1 spec                → returns the spec itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TensorSlice {
    pub pins: Vec<SliceAxisPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceAxisPin {
    pub axis: usize,
    pub index: usize,
}

impl TensorSlice {
    pub fn new() -> Self {
        Self { pins: Vec::new() }
    }

    pub fn pin(mut self, axis: usize, index: usize) -> Self {
        self.pins.push(SliceAxisPin { axis, index });
        self
    }

    /// Compute the (dims, base flat offset, inner stride layout) needed
    /// to iterate the slice. Returns an error if any pin is out of
    /// range. The returned `subspec.dims` is the remaining shape; the
    /// returned `offset` is the row-major flat offset of element (0,0,…)
    /// of the slice in the parent tensor.
    pub fn project(&self, parent: &TensorSpec) -> Result<TensorSliceProjection, TensorError> {
        // Validate each pin.
        for pin in &self.pins {
            if pin.axis >= parent.dims.len() {
                return Err(TensorError::SliceAxisOob {
                    axis: pin.axis,
                    rank: parent.dims.len(),
                });
            }
            let size = parent.dims[pin.axis];
            if pin.index >= size {
                return Err(TensorError::SliceIndexOob {
                    index: pin.index,
                    size,
                });
            }
        }

        let strides = parent.strides().ok_or(TensorError::ShapeOverflow)?;
        let mut offset = 0usize;
        let pinned: std::collections::HashSet<usize> =
            self.pins.iter().map(|p| p.axis).collect();
        for pin in &self.pins {
            offset = offset
                .checked_add(pin.index.checked_mul(strides[pin.axis]).ok_or(TensorError::ShapeOverflow)?)
                .ok_or(TensorError::ShapeOverflow)?;
        }

        let sub_dims: Vec<usize> = parent
            .dims
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| if pinned.contains(&i) { None } else { Some(d) })
            .collect();
        let sub_strides: Vec<usize> = strides
            .iter()
            .enumerate()
            .filter_map(|(i, &s)| if pinned.contains(&i) { None } else { Some(s) })
            .collect();

        Ok(TensorSliceProjection {
            sub_dims,
            sub_strides,
            offset,
        })
    }
}

/// What a renderer needs to walk a sliced subview without materializing
/// it. `sub_dims` is the visible shape post-slicing; `sub_strides` are
/// the parent-tensor strides for those axes (in elements); `offset` is
/// the parent-tensor flat index of the slice's origin.
#[derive(Debug, Clone, PartialEq)]
pub struct TensorSliceProjection {
    pub sub_dims: Vec<usize>,
    pub sub_strides: Vec<usize>,
    pub offset: usize,
}

impl TensorSliceProjection {
    /// Map a coordinate in the *sliced* shape to a flat index in the
    /// *parent* tensor's data.
    pub fn flat_index_in_parent(&self, sub_coords: &[usize]) -> Option<usize> {
        if sub_coords.len() != self.sub_dims.len() {
            return None;
        }
        let mut idx = self.offset;
        for (i, &c) in sub_coords.iter().enumerate() {
            if c >= self.sub_dims[i] {
                return None;
            }
            idx = idx.checked_add(c.checked_mul(self.sub_strides[i])?)?;
        }
        Some(idx)
    }

    pub fn element_count(&self) -> Option<usize> {
        self.sub_dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d))
    }
}

// ─────────────────────────────────────────────────────────────────────
// Session 2 — sampling, scanning, golden constructors
// ─────────────────────────────────────────────────────────────────────
//
// Future tensor renderers need three things from us beyond shape and
// slicing:
//
// 1. **Read elements as f32.** A heatmap doesn't care whether the
//    underlying dtype is u8 or f32 — it wants a normalized number. We
//    expose `read_f32` that lifts each supported dtype into f32 with
//    integer-to-float coercion (i32/i64 → f32, u8 → f32, bool → 0/1).
//
// 2. **Min / max over a slice.** Heatmap color mapping needs the value
//    range to choose a domain. Computing it on every frame is fine for
//    inline tensors (< 1 MiB cap); external tensors are the renderer's
//    problem.
//
// 3. **Golden constructors** — `linspace`, `eye`, `from_fn`, `zeros`.
//    Both for tests and for the "look at this synthetic tensor" demo
//    panes the future Tensor view ships with.
//
// All of this stays pure-CPU and dependency-free. No GPU, no rand,
// no ndarray — the renderer will glue to wgpu in a separate crate.

/// Stats over (a slice of) a tensor. Used by views to pick a color
/// domain or label a sparkline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TensorStats {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub count: usize,
}

impl TensorStats {
    /// `true` when `max - min` is so small the range is effectively
    /// degenerate — useful for renderers that want to fall back to a
    /// neutral tint rather than divide by ~0.
    pub fn is_degenerate(&self) -> bool {
        (self.max - self.min).abs() < f32::EPSILON
    }
}

impl TensorSpec {
    /// Read element `i` as `f32`. Returns `None` if the tensor has no
    /// resident data (`TensorRef::External` / `Empty`) or if the index
    /// is out of range. This is the "give me a number for color
    /// mapping" entrypoint; renderers can call it inside a tight loop.
    pub fn read_f32(&self, flat: usize) -> Option<f32> {
        match &self.data {
            TensorRef::InlineF32 { data } => data.get(flat).copied(),
            TensorRef::InlineU8 { data } => data.get(flat).map(|&b| b as f32),
            TensorRef::External { .. } | TensorRef::Empty => None,
        }
    }

    /// Convenience: `read_f32` indexed by N-D coords.
    pub fn read_f32_at(&self, coords: &[usize]) -> Option<f32> {
        let i = self.flat_index(coords)?;
        self.read_f32(i)
    }

    /// Scan every resident element and return min / max / mean / count.
    /// Returns `None` if the data isn't resident or the tensor is empty.
    pub fn stats(&self) -> Option<TensorStats> {
        let n = self.element_count()?;
        if n == 0 {
            return None;
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for i in 0..n {
            let v = self.read_f32(i)?;
            // NaN poisons the stats — skip it rather than propagate,
            // mirroring how matplotlib / numpy heatmaps behave by default.
            if !v.is_finite() {
                continue;
            }
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
            sum += v as f64;
            count += 1;
        }
        if count == 0 {
            return None;
        }
        Some(TensorStats {
            min,
            max,
            mean: (sum / count as f64) as f32,
            count,
        })
    }

    /// Stats over a `TensorSlice` projection. Same semantics as
    /// `stats()`, but only walks the sub-elements.
    pub fn stats_for_slice(&self, slice: &TensorSlice) -> Option<TensorStats> {
        let proj = slice.project(self).ok()?;
        let n = proj.element_count()?;
        if n == 0 {
            return None;
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0f64;
        let mut count = 0usize;
        // Walk the sub-shape in row-major order, mapping each sub
        // coord to its parent flat index via the projection.
        let mut coords = vec![0usize; proj.sub_dims.len()];
        loop {
            let parent_idx = proj.flat_index_in_parent(&coords)?;
            let v = self.read_f32(parent_idx)?;
            if v.is_finite() {
                if v < min {
                    min = v;
                }
                if v > max {
                    max = v;
                }
                sum += v as f64;
                count += 1;
            }
            if !next_coord(&mut coords, &proj.sub_dims) {
                break;
            }
        }
        if count == 0 {
            return None;
        }
        Some(TensorStats {
            min,
            max,
            mean: (sum / count as f64) as f32,
            count,
        })
    }

    /// Bilinearly sample a rank-2 tensor at fractional coordinates
    /// `(y, x)` in `[0, dims[0]-1] × [0, dims[1]-1]`. Returns `None` if
    /// the rank isn't 2 or the coords are out of range. Useful when
    /// rendering a small tensor into a larger pane: instead of nearest-
    /// neighbor blocky output, the view can resample.
    pub fn sample_2d_bilinear(&self, y: f32, x: f32) -> Option<f32> {
        if self.dims.len() != 2 {
            return None;
        }
        let (h, w) = (self.dims[0], self.dims[1]);
        if h == 0 || w == 0 {
            return None;
        }
        if !y.is_finite() || !x.is_finite() {
            return None;
        }
        let y = y.clamp(0.0, (h - 1) as f32);
        let x = x.clamp(0.0, (w - 1) as f32);
        let y0 = y.floor() as usize;
        let x0 = x.floor() as usize;
        let y1 = (y0 + 1).min(h - 1);
        let x1 = (x0 + 1).min(w - 1);
        let ty = y - y0 as f32;
        let tx = x - x0 as f32;
        let v00 = self.read_f32_at(&[y0, x0])?;
        let v01 = self.read_f32_at(&[y0, x1])?;
        let v10 = self.read_f32_at(&[y1, x0])?;
        let v11 = self.read_f32_at(&[y1, x1])?;
        let v0 = v00 + (v01 - v00) * tx;
        let v1 = v10 + (v11 - v10) * tx;
        Some(v0 + (v1 - v0) * ty)
    }
}

/// Advance row-major coordinates in `coords` over `dims` in place.
/// Returns `false` once the coords have wrapped past the last cell —
/// the standard "carry-from-right" odometer over an N-D shape. Empty
/// `dims` is treated as a single scalar position (returns `false` on
/// the first call, since the lone coord can't advance).
fn next_coord(coords: &mut [usize], dims: &[usize]) -> bool {
    debug_assert_eq!(coords.len(), dims.len());
    for i in (0..coords.len()).rev() {
        coords[i] += 1;
        if coords[i] < dims[i] {
            return true;
        }
        coords[i] = 0;
    }
    false
}

// ─────────────────────────────────────────────────────────────────────
// Golden constructors — small builders the tensor view ships demos with.
// All produce `InlineF32` tensors, so they're always self-contained.
// ─────────────────────────────────────────────────────────────────────

impl TensorSpec {
    /// Dense zeros of the given shape, f32. Panics only on shape
    /// overflow (caller's fault — pass a sane shape).
    pub fn zeros_f32(dims: Vec<usize>) -> Self {
        let n = dims.iter().product::<usize>();
        Self::new(
            dims,
            TensorDType::F32,
            TensorRef::InlineF32 { data: vec![0.0; n] },
        )
    }

    /// `len`-element evenly-spaced values from `start` to `end`
    /// inclusive. `len == 0` returns an empty f32 tensor; `len == 1`
    /// returns `[start]`.
    pub fn linspace_f32(start: f32, end: f32, len: usize) -> Self {
        let data: Vec<f32> = if len == 0 {
            Vec::new()
        } else if len == 1 {
            vec![start]
        } else {
            let step = (end - start) / (len - 1) as f32;
            (0..len).map(|i| start + step * i as f32).collect()
        };
        Self::new(vec![len], TensorDType::F32, TensorRef::InlineF32 { data })
    }

    /// `n × n` identity matrix. Useful as a "is your renderer actually
    /// reading the right strides?" sanity test.
    pub fn eye_f32(n: usize) -> Self {
        let mut data = vec![0.0f32; n * n];
        for i in 0..n {
            data[i * n + i] = 1.0;
        }
        Self::new(vec![n, n], TensorDType::F32, TensorRef::InlineF32 { data })
    }

    /// Build a tensor from a closure indexed by N-D coordinates. The
    /// closure is called in row-major order. Returns `None` on shape
    /// overflow.
    pub fn from_fn_f32<F: FnMut(&[usize]) -> f32>(dims: Vec<usize>, mut f: F) -> Option<Self> {
        let n = dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d))?;
        let mut data = Vec::with_capacity(n);
        if dims.is_empty() {
            data.push(f(&[]));
        } else {
            let mut coords = vec![0usize; dims.len()];
            loop {
                data.push(f(&coords));
                if !next_coord(&mut coords, &dims) {
                    break;
                }
            }
        }
        Some(Self::new(
            dims,
            TensorDType::F32,
            TensorRef::InlineF32 { data },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_2d() -> TensorSpec {
        // 3 rows × 4 cols, row-major.
        TensorSpec::new(
            vec![3, 4],
            TensorDType::F32,
            TensorRef::InlineF32 {
                data: (0..12).map(|i| i as f32).collect(),
            },
        )
    }

    #[test]
    fn dtype_byte_sizes() {
        assert_eq!(TensorDType::F32.byte_size(), 4);
        assert_eq!(TensorDType::F64.byte_size(), 8);
        assert_eq!(TensorDType::I32.byte_size(), 4);
        assert_eq!(TensorDType::I64.byte_size(), 8);
        assert_eq!(TensorDType::U8.byte_size(), 1);
        assert_eq!(TensorDType::Bool.byte_size(), 1);
    }

    #[test]
    fn spec_basic_metadata() {
        let s = spec_2d();
        assert_eq!(s.rank(), 2);
        assert!(!s.is_scalar());
        assert_eq!(s.element_count(), Some(12));
        assert_eq!(s.byte_size(), Some(48));
    }

    #[test]
    fn scalar_spec() {
        let s = TensorSpec::new(vec![], TensorDType::F32, TensorRef::Empty);
        assert!(s.is_scalar());
        assert_eq!(s.rank(), 0);
        assert_eq!(s.element_count(), Some(1));
        assert_eq!(s.strides(), Some(vec![]));
        // A scalar has no coords — empty slice projects to itself.
        let proj = TensorSlice::new().project(&s).unwrap();
        assert_eq!(proj.sub_dims, Vec::<usize>::new());
        assert_eq!(proj.offset, 0);
    }

    #[test]
    fn strides_row_major() {
        let s = TensorSpec::new(vec![2, 3, 4], TensorDType::F32, TensorRef::Empty);
        // Innermost axis stride = 1; next = 4; next = 12.
        assert_eq!(s.strides(), Some(vec![12, 4, 1]));
    }

    #[test]
    fn flat_index_matches_strides() {
        let s = spec_2d();
        // (1, 2) in a 3×4 row-major is offset 1*4 + 2 = 6.
        assert_eq!(s.flat_index(&[1, 2]), Some(6));
        // Wrong arity rejected.
        assert_eq!(s.flat_index(&[1]), None);
        // Out of bounds rejected.
        assert_eq!(s.flat_index(&[3, 0]), None);
        assert_eq!(s.flat_index(&[0, 4]), None);
    }

    #[test]
    fn validate_inline_f32_ok() {
        spec_2d().validate().unwrap();
    }

    #[test]
    fn validate_inline_f32_size_mismatch() {
        let bad = TensorSpec::new(
            vec![3, 4],
            TensorDType::F32,
            TensorRef::InlineF32 { data: vec![0.0; 11] },
        );
        assert_eq!(
            bad.validate(),
            Err(TensorError::ShapeMismatch {
                expected: 12,
                actual: 11,
            })
        );
    }

    #[test]
    fn validate_inline_dtype_mismatch() {
        let bad = TensorSpec::new(
            vec![4],
            TensorDType::I32, // declared int…
            TensorRef::InlineF32 { data: vec![0.0; 4] }, // …but float data
        );
        assert_eq!(bad.validate(), Err(TensorError::DTypeMismatch));
    }

    #[test]
    fn validate_external_is_trusted() {
        let s = TensorSpec::new(
            vec![1024, 1024, 1024],
            TensorDType::F32,
            TensorRef::External {
                uri: "hermon://tensors/abc".into(),
            },
        );
        // Even at gigabyte scale, External validates — sizing is the
        // renderer's job once it resolves the handle.
        s.validate().unwrap();
    }

    #[test]
    fn validate_inline_too_large_rejected() {
        // 300k f32 = 1.2 MiB, just above the cap.
        let dims = vec![300_000];
        let data = vec![0.0f32; 300_000];
        let bad = TensorSpec::new(dims, TensorDType::F32, TensorRef::InlineF32 { data });
        assert!(matches!(
            bad.validate(),
            Err(TensorError::InlineTooLarge(_))
        ));
    }

    #[test]
    fn slice_pin_row_of_matrix() {
        let s = spec_2d(); // [3, 4]
        let slice = TensorSlice::new().pin(0, 1);
        let proj = slice.project(&s).unwrap();
        assert_eq!(proj.sub_dims, vec![4]); // row vector
        assert_eq!(proj.sub_strides, vec![1]);
        assert_eq!(proj.offset, 4); // start of row 1
        // Cell 2 in the sliced view → flat 4 + 2 = 6 in the parent.
        assert_eq!(proj.flat_index_in_parent(&[2]), Some(6));
        assert_eq!(proj.element_count(), Some(4));
    }

    #[test]
    fn slice_pin_plane_of_3d() {
        // [H=2, W=3, D=4] row-major → strides [12, 4, 1].
        let s = TensorSpec::new(vec![2, 3, 4], TensorDType::F32, TensorRef::Empty);
        // Pin D=2 → expect a 2×3 heatmap with stride pattern from the parent.
        let proj = TensorSlice::new().pin(2, 2).project(&s).unwrap();
        assert_eq!(proj.sub_dims, vec![2, 3]);
        assert_eq!(proj.sub_strides, vec![12, 4]);
        assert_eq!(proj.offset, 2);
        // (h=1, w=2) on the plane → 1*12 + 2*4 + 2 = 22 in the parent.
        assert_eq!(proj.flat_index_in_parent(&[1, 2]), Some(22));
    }

    #[test]
    fn slice_multi_pin_to_scalar() {
        let s = TensorSpec::new(vec![2, 3, 4], TensorDType::F32, TensorRef::Empty);
        let proj = TensorSlice::new()
            .pin(0, 1)
            .pin(1, 2)
            .pin(2, 3)
            .project(&s)
            .unwrap();
        assert!(proj.sub_dims.is_empty());
        // 1*12 + 2*4 + 3*1 = 23.
        assert_eq!(proj.offset, 23);
        assert_eq!(proj.element_count(), Some(1));
    }

    #[test]
    fn slice_rejects_oob_axis() {
        let s = spec_2d(); // rank 2
        let err = TensorSlice::new().pin(5, 0).project(&s).unwrap_err();
        assert_eq!(err, TensorError::SliceAxisOob { axis: 5, rank: 2 });
    }

    #[test]
    fn slice_rejects_oob_index() {
        let s = spec_2d(); // [3, 4]
        let err = TensorSlice::new().pin(0, 99).project(&s).unwrap_err();
        assert_eq!(err, TensorError::SliceIndexOob { index: 99, size: 3 });
    }

    #[test]
    fn shape_overflow_doesnt_panic() {
        let s = TensorSpec::new(
            vec![usize::MAX, 2],
            TensorDType::F32,
            TensorRef::Empty,
        );
        assert_eq!(s.element_count(), None);
        assert_eq!(s.byte_size(), None);
        // flat_index with valid coords still works in the unsaturated dim,
        // but multiplying strides may overflow — we just want no panic.
        let _ = s.flat_index(&[0, 1]);
    }

    #[test]
    fn round_trip_through_json() {
        let s = spec_2d();
        let json = serde_json::to_string(&s).unwrap();
        let back: TensorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    // ─────────────────────────────────────────────────────────────
    // Session 2 — sampling, stats, golden constructors
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn read_f32_inline_f32_and_u8() {
        let f = spec_2d();
        assert_eq!(f.read_f32_at(&[1, 2]), Some(6.0));
        let u = TensorSpec::new(
            vec![3],
            TensorDType::U8,
            TensorRef::InlineU8 { data: vec![0, 128, 255] },
        );
        assert_eq!(u.read_f32(0), Some(0.0));
        assert_eq!(u.read_f32(1), Some(128.0));
        assert_eq!(u.read_f32(2), Some(255.0));
        // External tensors don't carry data — readers must resolve them.
        let ext = TensorSpec::new(
            vec![3],
            TensorDType::F32,
            TensorRef::External { uri: "x".into() },
        );
        assert_eq!(ext.read_f32(0), None);
    }

    #[test]
    fn stats_over_inline_matrix() {
        // [3,4] tensor with values 0..12.
        let s = spec_2d();
        let st = s.stats().unwrap();
        assert_eq!(st.min, 0.0);
        assert_eq!(st.max, 11.0);
        assert!((st.mean - 5.5).abs() < 1e-5);
        assert_eq!(st.count, 12);
        assert!(!st.is_degenerate());
    }

    #[test]
    fn stats_skip_nan_and_inf() {
        let s = TensorSpec::new(
            vec![4],
            TensorDType::F32,
            TensorRef::InlineF32 {
                data: vec![1.0, f32::NAN, 3.0, f32::INFINITY],
            },
        );
        let st = s.stats().unwrap();
        assert_eq!(st.count, 2);
        assert_eq!(st.min, 1.0);
        assert_eq!(st.max, 3.0);
    }

    #[test]
    fn stats_for_slice_only_walks_slice() {
        // 3×4 matrix with row 1 = [4,5,6,7]. Pin axis 0 = 1, expect
        // stats over those 4 values.
        let s = spec_2d();
        let slice = TensorSlice::new().pin(0, 1);
        let st = s.stats_for_slice(&slice).unwrap();
        assert_eq!(st.min, 4.0);
        assert_eq!(st.max, 7.0);
        assert_eq!(st.count, 4);
        assert!((st.mean - 5.5).abs() < 1e-5);
    }

    #[test]
    fn stats_degenerate_when_all_equal() {
        let s = TensorSpec::new(
            vec![5],
            TensorDType::F32,
            TensorRef::InlineF32 { data: vec![3.0; 5] },
        );
        let st = s.stats().unwrap();
        assert!(st.is_degenerate());
    }

    #[test]
    fn bilinear_sample_at_integer_grid_matches_read() {
        let s = TensorSpec::from_fn_f32(vec![3, 3], |c| (c[0] * 10 + c[1]) as f32).unwrap();
        // At integer coords, bilinear should exactly match read_f32_at.
        for y in 0..3 {
            for x in 0..3 {
                let exact = s.read_f32_at(&[y, x]).unwrap();
                let bilin = s.sample_2d_bilinear(y as f32, x as f32).unwrap();
                assert!((exact - bilin).abs() < 1e-5, "({y},{x}): {exact} vs {bilin}");
            }
        }
    }

    #[test]
    fn bilinear_sample_midpoint_averages_neighbors() {
        // 2×2 with corners 0,1,2,3 → middle should be (0+1+2+3)/4 = 1.5.
        let s = TensorSpec::new(
            vec![2, 2],
            TensorDType::F32,
            TensorRef::InlineF32 { data: vec![0.0, 1.0, 2.0, 3.0] },
        );
        let v = s.sample_2d_bilinear(0.5, 0.5).unwrap();
        assert!((v - 1.5).abs() < 1e-5);
    }

    #[test]
    fn bilinear_sample_rejects_non_2d() {
        let s = TensorSpec::linspace_f32(0.0, 1.0, 5);
        assert_eq!(s.sample_2d_bilinear(0.0, 0.0), None);
    }

    #[test]
    fn bilinear_sample_clamps_to_edge() {
        let s = TensorSpec::from_fn_f32(vec![2, 2], |c| (c[0] + c[1]) as f32).unwrap();
        // Way outside the grid — should clamp to the (1,1) corner = 2.
        let v = s.sample_2d_bilinear(99.0, 99.0).unwrap();
        assert!((v - 2.0).abs() < 1e-5);
    }

    #[test]
    fn zeros_constructor_shape_and_data() {
        let s = TensorSpec::zeros_f32(vec![2, 3]);
        s.validate().unwrap();
        assert_eq!(s.element_count(), Some(6));
        for i in 0..6 {
            assert_eq!(s.read_f32(i), Some(0.0));
        }
    }

    #[test]
    fn linspace_endpoints_and_length() {
        let s = TensorSpec::linspace_f32(0.0, 1.0, 5);
        s.validate().unwrap();
        assert_eq!(s.read_f32(0), Some(0.0));
        assert_eq!(s.read_f32(4), Some(1.0));
        assert!((s.read_f32(2).unwrap() - 0.5).abs() < 1e-5);
        // Single-element and zero-element corners.
        let one = TensorSpec::linspace_f32(7.0, 9.0, 1);
        assert_eq!(one.read_f32(0), Some(7.0));
        let none = TensorSpec::linspace_f32(0.0, 1.0, 0);
        assert_eq!(none.element_count(), Some(0));
    }

    #[test]
    fn eye_matrix_has_diagonal_ones() {
        let s = TensorSpec::eye_f32(4);
        s.validate().unwrap();
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_eq!(s.read_f32_at(&[i, j]), Some(expected));
            }
        }
    }

    #[test]
    fn from_fn_walks_row_major() {
        // Each cell carries its row-major flat index, so we can confirm
        // the iteration order matches our flat_index math.
        let dims = vec![2, 3, 4];
        let s = TensorSpec::from_fn_f32(dims.clone(), |c| {
            (c[0] * 12 + c[1] * 4 + c[2]) as f32
        })
        .unwrap();
        let probe = TensorSpec::new(dims, TensorDType::F32, TensorRef::Empty);
        for i in 0..24 {
            assert_eq!(s.read_f32(i), Some(i as f32), "flat={i}");
        }
        assert_eq!(probe.flat_index(&[1, 2, 3]), Some(23));
    }

    #[test]
    fn round_trip_external() {
        let s = TensorSpec {
            dims: vec![768],
            dtype: TensorDType::F32,
            data: TensorRef::External {
                uri: "hermon://tensors/wte".into(),
            },
            label: Some("token_embeddings".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: TensorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
