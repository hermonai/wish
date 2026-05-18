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
