//! Wish Native Render — a real OS window + GPU rendering for the
//! `wish-canvas-core` `Canvas`. No HTML, no browser.
//!
//! Powered by `eframe` (winit + wgpu + egui). The painter renders
//! nodes as rounded rectangles, edges as straight segments with
//! arrowheads, and labels as text — all GPU-rasterized. Pan with
//! left-mouse drag, zoom with the scroll wheel, click any node (in the
//! sidebar or on the canvas) to select it. The full WorldLine summary
//! appears in a right-side panel when a world is loaded.
//!
//! This is the v0.5.0 visible bridge that replaces the browser
//! pop-out. The full `wgpu`-direct pipeline arrives in v0.6.0 as
//! `wish-scene-renderer`.

pub mod perspective;
pub mod scene3d;

pub use perspective::Perspective;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Stroke, Vec2};
use wish_canvas_core::types::{Canvas, CanvasNode, CanvasNodeId, CanvasNodeKind, EdgeKind};
use wish_provenance::WorldLine;
use wish_world_model::{SemanticId, WishWorld, WorldPatch};
use wish_world_studio::{world_to_canvas, WorldPlan};

use scene3d::Camera3D;

/// Open a native window rendering a `Canvas`. Title appears in the
/// window header and the in-app toolbar. `world` is optional metadata
/// shown in the right-rail panel.
pub fn run(title: &str, canvas: Canvas, world: Option<WishWorld>) -> eframe::Result<()> {
    run_with_perspective(title, canvas, world, Perspective::default())
}

/// Like [`run`], but starts in the given domain perspective. The user
/// can still switch via the toolbar dropdown.
pub fn run_with_perspective(
    title: &str,
    canvas: Canvas,
    world: Option<WishWorld>,
    perspective: Perspective,
) -> eframe::Result<()> {
    run_with_perspective_and_reveal(title, canvas, world, perspective, None)
}

/// Like [`run_with_perspective`], but also accepts an optional
/// [`SemanticId`] to **reveal** the moment the cinematic boot ends.
/// When set, the viewer pans the canvas so the matching node is at
/// viewport center and marks it as selected (highlight ring). This is
/// the implementation of the Reveal-in-Canvas protocol described in
/// `wish-design/.../01-strategy/09-reveal-in-canvas-protocol.md`.
///
/// If `reveal` is `None`, this behaves identically to
/// [`run_with_perspective`]. If the SemanticId doesn't resolve to a
/// node in the loaded canvas, a warning is logged and the viewer opens
/// at centroid as a graceful fallback.
pub fn run_with_perspective_and_reveal(
    title: &str,
    mut canvas: Canvas,
    world: Option<WishWorld>,
    perspective: Perspective,
    reveal: Option<SemanticId>,
) -> eframe::Result<()> {
    let title_owned = title.to_string();
    // Snap the canvas layout to the perspective's default before the
    // first frame so the user sees the lens immediately.
    canvas.layout = perspective.default_layout();
    wish_canvas_core::layout::run(&mut canvas);
    let mut state = AppState::new(canvas, world, title.to_string());
    state.perspective = perspective;
    state.reveal_pending = reveal;
    if perspective.prefers_3d() {
        state.mode = ViewMode::Scene3D;
    }
    let app_state = Arc::new(Mutex::new(state));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title(format!("Wish · {title}")),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Wish · {title_owned}"),
        options,
        Box::new(move |_cc| Ok(Box::new(WishApp { state: app_state }))),
    )
}

/// Open a native window in **time-travel mode**. A scrub slider in
/// the bottom panel lets the user replay any prefix of the WorldLine
/// — slide left to see the world earlier in its history, slide right
/// to bring it back to the present. The viewer re-projects the canvas
/// on every change.
///
/// `worldline_path` should point at the `.wishworld/provenance/worldline.jsonl`
/// you want to replay against. If the file doesn't exist or has zero
/// events, the slider is hidden and the viewer falls back to static
/// world rendering.
pub fn run_timetravel(
    title: &str,
    world: WishWorld,
    worldline_path: std::path::PathBuf,
) -> eframe::Result<()> {
    let title_owned = title.to_string();
    let worldline = WorldLine::open(worldline_path.clone()).ok();
    let event_count = worldline.as_ref().map(|w| w.len()).unwrap_or(0);
    let canvas = world_to_canvas(&world);
    let mut state = AppState::new(canvas, Some(world.clone()), title.to_string());
    state.timetravel_enabled = true;
    state.timetravel_baseline = Some(reset_to_baseline(&world));
    state.timetravel_position = event_count;
    state.timetravel_last_position = event_count;
    state.worldline = worldline;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title(format!("Wish · {title}")),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Wish · {title_owned}"),
        options,
        Box::new(move |_cc| {
            Ok(Box::new(WishApp {
                state: Arc::new(Mutex::new(state)),
            }))
        }),
    )
}

/// Open a native window in **hot-reload mode**. Polls the worldline
/// file every `poll_interval` and re-renders the world whenever new
/// events appear. This is the seam where an *external* Hermon agent
/// writing patches becomes visible in real time inside the Wish
/// viewer.
pub fn run_watch(
    title: &str,
    world_dir: std::path::PathBuf,
    poll_interval: Duration,
) -> eframe::Result<()> {
    let title_owned = title.to_string();
    let wl_path = world_dir.join("provenance").join("worldline.jsonl");
    let worldline = WorldLine::open(wl_path.clone()).ok();
    let event_count = worldline.as_ref().map(|w| w.len()).unwrap_or(0);
    // Read the world skeleton from disk so the viewer starts in a sane state.
    let bundle = wish_world_model::read_world_dir(&world_dir).ok();
    let world = bundle.map(|b| b.world).unwrap_or_else(|| {
        wish_world_model::WishWorld::new(title, wish_world_model::WorldKind::GenericProject)
    });
    let canvas = world_to_canvas(&world);
    let mut state = AppState::new(canvas, Some(world.clone()), title.to_string());
    state.worldline = worldline;
    state.watch_enabled = true;
    state.watch_poll = poll_interval;
    state.watch_path = Some(wl_path.clone());
    state.watch_last_mtime = std::fs::metadata(&wl_path)
        .ok()
        .and_then(|m| m.modified().ok());
    state.watch_last_count = event_count;
    state.timetravel_baseline = Some(reset_to_baseline(&world));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title(format!("Wish · {title}")),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Wish · {title_owned}"),
        options,
        Box::new(move |_cc| {
            Ok(Box::new(WishApp {
                state: Arc::new(Mutex::new(state)),
            }))
        }),
    )
}

/// Snapshot the *identity* fields of a world (id, name, kind, intent,
/// created_at) so a time-travel reset can rebuild a "pristine" version
/// of the same world that the WorldLine can be replayed onto.
fn reset_to_baseline(world: &WishWorld) -> WishWorld {
    let mut clean = WishWorld::new(world.name.clone(), world.kind.clone());
    clean.id = world.id.clone();
    clean.intent = world.intent.clone();
    clean.created_at = world.created_at;
    clean
}

/// Open a native window and **animate** a `WorldPlan` — apply one
/// `WorldPatch` every `step_delay`, re-projecting the canvas in
/// between. Pan / zoom / select stay live throughout.
///
/// This is the "intent → world" demo: the user types an intent, the
/// planner emits a plan, and the viewer materializes the world step
/// by step in a native window. The patches flow through a real
/// `WorldLine` in a temporary directory so the WorldLine inspector
/// in the right-rail updates as the build progresses.
pub fn run_live(plan: WorldPlan, step_delay: Duration) -> eframe::Result<()> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("wish-render-live-{nanos}"));
    std::fs::create_dir_all(&tmp).ok();
    let worldline = WorldLine::open_in_world_dir(&tmp).ok();

    let title = format!("{}  ·  {}", plan.world.name, plan.template);
    let canvas = world_to_canvas(&plan.world);
    let world = plan.world;
    let mut state = AppState::new(canvas, Some(world), title.clone());
    state.pending = VecDeque::from(plan.patches.clone());
    state.total_patches = plan.patches.len();
    state.step_delay = step_delay;
    state.last_step_at = Some(Instant::now());
    state.worldline = worldline;
    state.template = Some(plan.template);

    let app_state = Arc::new(Mutex::new(state));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title(format!("Wish · {title}")),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Wish · {title}"),
        options,
        Box::new(move |_cc| Ok(Box::new(WishApp { state: app_state }))),
    )
}

struct WishApp {
    state: Arc<Mutex<AppState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Canvas2D,
    Scene3D,
}

struct AppState {
    canvas: Canvas,
    world: Option<WishWorld>,
    title: String,
    pan: Vec2,
    zoom: f32,
    selected: Option<CanvasNodeId>,
    // True once we've auto-fit on first frame.
    initial_fit_done: bool,
    mode: ViewMode,
    camera3d: Camera3D,
    scene3d_initial_frame_done: bool,
    perspective: Perspective,

    // Live-build state. `pending` is the queue of WorldPatches still
    // to apply. The animation tick re-fits and re-projects the canvas
    // after each patch lands.
    pending: VecDeque<WorldPatch>,
    total_patches: usize,
    step_delay: Duration,
    last_step_at: Option<Instant>,
    worldline: Option<WorldLine>,
    template: Option<&'static str>,
    paused: bool,
    // Re-fit-on-grow until the user manually pans.
    user_has_panned: bool,

    // Time-travel state. When enabled, a slider in the bottom panel
    // controls how many WorldLine events to replay onto the baseline
    // world.
    timetravel_enabled: bool,
    timetravel_position: usize,
    timetravel_last_position: usize,
    timetravel_baseline: Option<WishWorld>,

    // Hot-reload watcher state. When enabled, every `watch_poll` we
    // re-read the WorldLine from disk; if events arrived we re-apply
    // them onto the baseline.
    watch_enabled: bool,
    watch_poll: Duration,
    watch_path: Option<std::path::PathBuf>,
    watch_last_mtime: Option<std::time::SystemTime>,
    watch_last_count: usize,
    watch_last_poll: Option<Instant>,

    // Cinematic startup: cycle through 5 perspectives in the first
    // few seconds to demonstrate the Tensorium thesis to first-time
    // viewers. Skippable on any user input.
    boot: BootState,
    boot_started_at: Option<Instant>,
    boot_target_perspective: Perspective,

    // Reveal-in-Canvas protocol (v0.5.0): when set, the viewer pans to
    // this `SemanticId` and selects it the moment the cinematic boot
    // ends. Cleared after the reveal fires so subsequent boot transitions
    // (e.g. perspective changes that re-run layout) don't keep re-centering.
    // See `wish-design/.../01-strategy/09-reveal-in-canvas-protocol.md`.
    reveal_pending: Option<SemanticId>,
}

/// Two-stage startup cinematic: a brief splash title, then a
/// perspective cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootState {
    /// Initial title overlay. ~600ms.
    Splash,
    /// Auto-cycling through 5 representative perspectives, ~700ms
    /// each, ending on `boot_target_perspective`.
    Cycling { step: u8 },
    /// User has dismissed or cinematic has finished.
    Done,
}

impl AppState {
    fn new(canvas: Canvas, world: Option<WishWorld>, title: String) -> Self {
        Self {
            canvas,
            world,
            title,
            pan: Vec2::ZERO,
            zoom: 1.0,
            selected: None,
            initial_fit_done: false,
            mode: ViewMode::Canvas2D,
            camera3d: Camera3D::default(),
            scene3d_initial_frame_done: false,
            perspective: Perspective::default(),
            pending: VecDeque::new(),
            total_patches: 0,
            step_delay: Duration::from_millis(0),
            last_step_at: None,
            worldline: None,
            template: None,
            paused: false,
            user_has_panned: false,
            timetravel_enabled: false,
            timetravel_position: 0,
            timetravel_last_position: usize::MAX,
            timetravel_baseline: None,
            watch_enabled: false,
            watch_poll: Duration::from_millis(500),
            watch_path: None,
            watch_last_mtime: None,
            watch_last_count: 0,
            watch_last_poll: None,
            boot: BootState::Splash,
            boot_started_at: None,
            boot_target_perspective: Perspective::default(),
            reveal_pending: None,
        }
    }

    /// Apply a pending [`SemanticId`] reveal: pan to the node and set
    /// it as the selected node so the highlight ring fires. Clears the
    /// reveal_pending slot regardless of outcome. If the node isn't
    /// found, logs a warning and proceeds — the canvas opens at
    /// centroid as a graceful fallback.
    fn apply_reveal_if_pending(&mut self) {
        let Some(target) = self.reveal_pending.take() else {
            return;
        };
        match self.canvas.reveal(&target) {
            Some(node_id) => {
                // Pan the screen-space view to match the canvas pan
                // that `Canvas::reveal` just set. The Canvas viewport
                // sets pan_x/pan_y in canvas coords; the AppState's
                // `pan` field is screen-space and gets applied on top.
                // Resetting AppState pan to zero lets Canvas's pan
                // dominate — exactly what we want for first-frame
                // reveal.
                self.pan = Vec2::ZERO;
                self.selected = Some(node_id);
                self.initial_fit_done = true; // suppress re-fit so the reveal sticks.
            }
            None => {
                eprintln!(
                    "wish-render: reveal target {} not found in canvas — opening at centroid",
                    target
                );
            }
        }
    }

    /// Replay the worldline up to `self.timetravel_position` onto the
    /// baseline world, then refresh the canvas. Only runs if the
    /// position changed from the last apply.
    fn apply_timetravel_if_dirty(&mut self) {
        if !self.timetravel_enabled {
            return;
        }
        if self.timetravel_position == self.timetravel_last_position {
            return;
        }
        let Some(wl) = &self.worldline else { return };
        let Some(baseline) = &self.timetravel_baseline else {
            return;
        };
        let mut world = baseline.clone();
        if wl.replay_into(&mut world, self.timetravel_position).is_ok() {
            self.canvas = world_to_canvas(&world);
            self.world = Some(world);
            self.timetravel_last_position = self.timetravel_position;
        }
    }

    /// Hot-reload tick: if enough time has passed since the last
    /// poll and the worldline file has grown, re-read it and replay
    /// onto the baseline. Returns the number of new events absorbed.
    fn watch_tick(&mut self) -> usize {
        if !self.watch_enabled {
            return 0;
        }
        let now = Instant::now();
        let due = self
            .watch_last_poll
            .map(|t| now.duration_since(t) >= self.watch_poll)
            .unwrap_or(true);
        if !due {
            return 0;
        }
        self.watch_last_poll = Some(now);
        let Some(path) = self.watch_path.clone() else {
            return 0;
        };
        let Some(wl) = self.worldline.as_mut() else {
            return 0;
        };
        let mut last_seen = self.watch_last_mtime;
        let _ = wl.reload_if_changed(&mut last_seen);
        self.watch_last_mtime = last_seen;
        let new_count = wl.len();
        if new_count == self.watch_last_count {
            return 0;
        }
        let delta = new_count - self.watch_last_count;
        self.watch_last_count = new_count;

        // Replay onto the baseline and refresh.
        let Some(baseline) = &self.timetravel_baseline else {
            return 0;
        };
        let mut world = baseline.clone();
        if wl.replay_into(&mut world, new_count).is_ok() {
            self.canvas = world_to_canvas(&world);
            self.world = Some(world);
        }
        // Hint to refit until the user pans.
        if !self.user_has_panned {
            self.initial_fit_done = false;
            self.scene3d_initial_frame_done = false;
        }
        log::info!(
            target: "wish.render",
            "watch: detected {} new event(s) in {}",
            delta,
            path.display()
        );
        delta
    }

    /// Returns true when there's a live build still in progress.
    fn is_animating(&self) -> bool {
        !self.pending.is_empty() && !self.paused
    }

    /// Auto-aim the 3D orbit camera at the world's centroid with a
    /// distance that frames the bounding extent.
    fn frame_camera_to_world(&mut self) {
        if let Some(w) = &self.world {
            let (target, extent) = scene3d::world_centroid_and_extent(w);
            self.camera3d.target = target;
            self.camera3d.distance = extent;
        }
    }

    /// Apply the next pending patch through the worldline (if any),
    /// then re-project the canvas. Returns the description for the
    /// status overlay.
    fn step_one(&mut self) -> Option<String> {
        let patch = self.pending.pop_front()?;
        let intent = patch.intent.clone();
        let world = self.world.as_mut()?;
        let outcome = if let Some(wl) = self.worldline.as_mut() {
            wl.apply_with_provenance(world, patch, 0.30).ok()
        } else {
            wish_world_model::apply_patch(world, &patch).ok().map(|_| {
                wish_provenance::ApplyOutcome::Applied {
                    event_id: "ev_local".into(),
                    gate: wish_provenance::ApprovalGate::Auto,
                }
            })
        };
        let _ = outcome?; // if it failed, just stop
        self.canvas = world_to_canvas(world);
        Some(intent)
    }

    fn fit_to_view(&mut self, available: Vec2) {
        let bb = canvas_bbox(&self.canvas);
        let Some((min_x, min_y, max_x, max_y)) = bb else {
            return;
        };
        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);
        let pad = 40.0;
        let zx = (available.x - pad * 2.0).max(50.0) / w;
        let zy = (available.y - pad * 2.0).max(50.0) / h;
        self.zoom = zx.min(zy).clamp(0.05, 4.0);
        // Center the bbox at the viewport center.
        let bbcx = (min_x + max_x) * 0.5;
        let bbcy = (min_y + max_y) * 0.5;
        let cx = available.x * 0.5;
        let cy = available.y * 0.5;
        self.pan = Vec2::new(cx - bbcx * self.zoom, cy - bbcy * self.zoom);
    }
}

/// The cinematic startup tour: five perspectives that demonstrate
/// the breadth of the Tensorium in the first few seconds the user
/// sees Wish. Order chosen to span 2D codegraph → 3D world → finance
/// → math → physics — visibly different visuals, all on the same
/// world. The final perspective in the cycle is the user's chosen
/// target (set in `boot_target_perspective`).
const BOOT_CYCLE: [Perspective; 5] = [
    Perspective::Engineering,
    Perspective::Spatial,
    Perspective::Financial,
    Perspective::Math,
    Perspective::Physics,
];

/// Splash duration before the perspective cycle begins.
const SPLASH_MS: u128 = 600;
/// Per-cycle dwell. 5 perspectives × 700 ms = 3.5 s of cycling.
const CYCLE_MS: u128 = 700;

impl eframe::App for WishApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = self.state.clone();

        // Cinematic boot tick. Runs only during the brief startup
        // phase. Any user input dismisses it immediately and locks
        // in `boot_target_perspective`.
        {
            let mut s = state.lock().unwrap();
            // Initialize start time on first frame.
            if s.boot_started_at.is_none() && s.boot != BootState::Done {
                s.boot_started_at = Some(Instant::now());
                s.boot_target_perspective = s.perspective;
            }
            // Dismiss on input.
            let any_input = ctx.input(|i| {
                i.pointer.any_pressed() || i.pointer.any_click() || !i.events.is_empty()
            });
            if any_input && s.boot != BootState::Done {
                s.boot = BootState::Done;
                s.perspective = s.boot_target_perspective;
                s.canvas.layout = s.perspective.default_layout();
                wish_canvas_core::layout::run(&mut s.canvas);
                s.initial_fit_done = false;
                s.apply_reveal_if_pending();
            }
            // Drive cinematic.
            if let Some(t0) = s.boot_started_at {
                let elapsed = t0.elapsed().as_millis();
                match s.boot {
                    BootState::Splash => {
                        if elapsed >= SPLASH_MS {
                            // Transition into cycle step 0.
                            s.boot = BootState::Cycling { step: 0 };
                            let p = BOOT_CYCLE[0];
                            s.perspective = p;
                            s.canvas.layout = p.default_layout();
                            wish_canvas_core::layout::run(&mut s.canvas);
                            s.initial_fit_done = false;
                        }
                        ctx.request_repaint_after(Duration::from_millis(33));
                    }
                    BootState::Cycling { step } => {
                        let cycle_start = SPLASH_MS + (step as u128 * CYCLE_MS);
                        let cycle_end = cycle_start + CYCLE_MS;
                        if elapsed >= cycle_end {
                            let next = step.saturating_add(1);
                            if (next as usize) < BOOT_CYCLE.len() {
                                let p = BOOT_CYCLE[next as usize];
                                s.boot = BootState::Cycling { step: next };
                                s.perspective = p;
                                s.canvas.layout = p.default_layout();
                                wish_canvas_core::layout::run(&mut s.canvas);
                                s.initial_fit_done = false;
                            } else {
                                // Cycle complete → settle on user's chosen lens.
                                s.boot = BootState::Done;
                                s.perspective = s.boot_target_perspective;
                                s.canvas.layout = s.perspective.default_layout();
                                wish_canvas_core::layout::run(&mut s.canvas);
                                s.initial_fit_done = false;
                                s.apply_reveal_if_pending();
                            }
                        }
                        ctx.request_repaint_after(Duration::from_millis(33));
                    }
                    BootState::Done => {}
                }
            }
        }

        // Hot-reload tick. Drives the watcher when enabled.
        {
            let mut s = state.lock().unwrap();
            let absorbed = s.watch_tick();
            if absorbed > 0 {
                // Keep the slider's "all the way right" semantics by
                // moving its target to the new count.
                let new_count = s.worldline.as_ref().map(|w| w.len()).unwrap_or(0);
                if s.timetravel_enabled {
                    s.timetravel_position = new_count;
                }
            }
            if s.watch_enabled {
                ctx.request_repaint_after(s.watch_poll);
            }
        }

        // Time-travel replay if the slider moved.
        {
            let mut s = state.lock().unwrap();
            s.apply_timetravel_if_dirty();
        }

        // Live-build animation tick. Runs once per frame; applies one
        // patch when the per-step delay has elapsed, refits the view
        // as the world grows (unless the user has panned), and
        // requests a repaint until the queue drains.
        {
            let mut s = state.lock().unwrap();
            if s.is_animating() {
                let due = s
                    .last_step_at
                    .map(|t| t.elapsed() >= s.step_delay)
                    .unwrap_or(true);
                if due {
                    let _intent = s.step_one();
                    s.last_step_at = Some(Instant::now());
                    // Refit until the user pans. Keeps the growing
                    // world centered while it materializes.
                    if !s.user_has_panned {
                        let size = ctx.screen_rect().size();
                        s.fit_to_view(size);
                    }
                }
                // Schedule the next repaint just after the next step.
                let until_next = s
                    .last_step_at
                    .map(|t| s.step_delay.saturating_sub(t.elapsed()))
                    .unwrap_or(Duration::from_millis(16));
                ctx.request_repaint_after(until_next.max(Duration::from_millis(16)));
            }
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&state.lock().unwrap().title);
                ui.separator();
                let (n_nodes, n_edges, zoom_val) = {
                    let s = state.lock().unwrap();
                    (s.canvas.nodes.len(), s.canvas.edges.len(), s.zoom)
                };
                ui.label(format!("{n_nodes} nodes · {n_edges} edges"));
                ui.separator();
                if ui.button("− zoom out").clicked() {
                    state.lock().unwrap().zoom = (zoom_val / 1.25).clamp(0.05, 8.0);
                }
                if ui.button("+ zoom in").clicked() {
                    state.lock().unwrap().zoom = (zoom_val * 1.25).clamp(0.05, 8.0);
                }
                ui.label(format!("zoom {:.2}x", zoom_val));
                ui.separator();
                if ui.button("fit").clicked() {
                    let avail = ctx.screen_rect().size();
                    let mut s = state.lock().unwrap();
                    s.fit_to_view(avail);
                    s.user_has_panned = false;
                    s.scene3d_initial_frame_done = false; // re-frame 3D on next tick too
                }
                ui.separator();
                // 2D / 3D mode toggle.
                let current_mode = state.lock().unwrap().mode;
                let world_has_transforms = state
                    .lock()
                    .unwrap()
                    .world
                    .as_ref()
                    .map(|w| w.entities.values().any(|e| e.components.iter().any(|c| matches!(c, wish_world_model::Component::Transform(_)))))
                    .unwrap_or(false);
                if ui
                    .selectable_label(current_mode == ViewMode::Canvas2D, "🗺 2D canvas")
                    .clicked()
                {
                    state.lock().unwrap().mode = ViewMode::Canvas2D;
                }
                let scene_response = ui.add_enabled(
                    world_has_transforms,
                    egui::SelectableLabel::new(current_mode == ViewMode::Scene3D, "🌐 3D scene"),
                );
                if scene_response.clicked() && world_has_transforms {
                    let mut s = state.lock().unwrap();
                    s.mode = ViewMode::Scene3D;
                    s.scene3d_initial_frame_done = false;
                }
                if !world_has_transforms {
                    scene_response.on_hover_text(
                        "3D scene needs entities with Transform components (load a world with one).",
                    );
                }
                ui.separator();
                // Live-build progress indicator + pause/resume.
                let (pending, total, paused, template) = {
                    let s = state.lock().unwrap();
                    (s.pending.len(), s.total_patches, s.paused, s.template)
                };
                if total > 0 {
                    let done = total.saturating_sub(pending);
                    ui.label(egui::RichText::new(format!(
                        "{} ▸ step {}/{}",
                        template.unwrap_or("plan"),
                        done,
                        total
                    )));
                    if pending > 0 {
                        if ui.button(if paused { "▶ resume" } else { "⏸ pause" }).clicked() {
                            state.lock().unwrap().paused = !paused;
                        }
                        if ui.button("⏭ all").clicked() {
                            let mut s = state.lock().unwrap();
                            while !s.pending.is_empty() {
                                let _ = s.step_one();
                            }
                            if !s.user_has_panned {
                                let size = ctx.screen_rect().size();
                                s.fit_to_view(size);
                            }
                        }
                    } else {
                        ui.label(egui::RichText::new("✓ built").color(Color32::from_rgb(63, 185, 80)));
                    }
                    ui.separator();
                }
                // Perspective dropdown — live-switchable, re-tints and
                // (if changed) snaps the view-mode and default layout.
                // Grouped by category: Domain lenses first, then a
                // separator, then the Scientific (Tensorium-fundamental)
                // lenses.
                let current_persp = state.lock().unwrap().perspective;
                egui::ComboBox::from_id_source("perspective")
                    .selected_text(current_persp.label())
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("— Domain —")
                                .small()
                                .color(Color32::from_rgb(140, 152, 168)),
                        );
                        for p in Perspective::ALL
                            .iter()
                            .filter(|p| p.category() == perspective::PerspectiveCategory::Domain)
                        {
                            select_perspective_row(ui, &state, *p, current_persp);
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new("— Science (Tensorium) —")
                                .small()
                                .color(Color32::from_rgb(140, 152, 168)),
                        );
                        for p in Perspective::ALL
                            .iter()
                            .filter(|p| p.category() == perspective::PerspectiveCategory::Science)
                        {
                            select_perspective_row(ui, &state, *p, current_persp);
                        }
                    });
                ui.label(egui::RichText::new(current_persp.tagline()).weak().italics());
                ui.separator();
                ui.weak("drag to pan · scroll to zoom · click to select");
            });
        });

        egui::SidePanel::left("entities")
            .min_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Entities");
                // Take an owned snapshot so we can release the lock before
                // entering the scroll-area closure (egui's borrow checker
                // wants the closure to outlive the lock guard).
                let (entries, selected): (Vec<CanvasNode>, Option<CanvasNodeId>) = {
                    let s = state.lock().unwrap();
                    let mut v: Vec<CanvasNode> = s.canvas.nodes.values().cloned().collect();
                    v.sort_by(|a, b| a.label.cmp(&b.label));
                    (v, s.selected)
                };
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for n in &entries {
                        let is_sel = selected == Some(n.id);
                        let mut text =
                            egui::RichText::new(format!("{}  {}", kind_glyph(&n.kind), n.label))
                                .small();
                        if is_sel {
                            text = text.color(Color32::from_rgb(97, 175, 239)).strong();
                        }
                        if ui
                            .selectable_label(is_sel, text)
                            .on_hover_text(n.semantic_id.to_string())
                            .clicked()
                        {
                            let mut s = state.lock().unwrap();
                            s.selected = Some(n.id);
                            // Recenter on the selected node.
                            let bounds = n.bounds;
                            let cx = bounds.x + bounds.w * 0.5;
                            let cy = bounds.y + bounds.h * 0.5;
                            let avail = ctx.screen_rect().size();
                            s.pan =
                                Vec2::new(avail.x * 0.5 - cx * s.zoom, avail.y * 0.5 - cy * s.zoom);
                        }
                    }
                });
            });

        // Optional world inspector on the right.
        let show_world_panel = state.lock().unwrap().world.is_some();
        if show_world_panel {
            egui::SidePanel::right("world")
                .min_width(280.0)
                .show(ctx, |ui| {
                    ui.heading("World");
                    let s = state.lock().unwrap();
                    if let Some(w) = &s.world {
                        ui.label(format!("name:     {}", w.name));
                        ui.label(format!("id:       {}", w.id));
                        ui.label(format!("kind:     {:?}", w.kind));
                        if !w.intent.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(&w.intent).italics());
                        }
                        ui.add_space(8.0);
                        ui.label(format!("entities: {}", w.entities.len()));
                        ui.label(format!("scenes:   {}", w.scenes.len()));
                        ui.label(format!("agents:   {}", w.agents.len()));
                        ui.label(format!("assets:   {}", w.assets.len()));
                        if !w.agents.is_empty() {
                            ui.add_space(8.0);
                            ui.collapsing("Agents", |ui| {
                                for a in w.agents.values() {
                                    ui.label(format!("• {}  ({})", a.display_name, a.role));
                                }
                            });
                        }
                    }
                });
        }

        // Bottom panel: time-travel slider + watch status.
        {
            let (tt_enabled, watch_enabled, total, position, current_event) = {
                let s = state.lock().unwrap();
                let total = s.worldline.as_ref().map(|w| w.len()).unwrap_or(0);
                let event = s.worldline.as_ref().and_then(|w| {
                    let p = s.timetravel_position;
                    if p > 0 && p <= w.len() {
                        w.iter().nth(p - 1).cloned()
                    } else {
                        None
                    }
                });
                (
                    s.timetravel_enabled,
                    s.watch_enabled,
                    total,
                    s.timetravel_position,
                    event,
                )
            };
            if tt_enabled || watch_enabled {
                egui::TopBottomPanel::bottom("timetravel").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if tt_enabled && total > 0 {
                            ui.label(egui::RichText::new("⏱ time-travel").strong());
                            let mut pos = position;
                            let resp = ui.add(
                                egui::Slider::new(&mut pos, 0..=total)
                                    .text(format!("event / {total}")),
                            );
                            if resp.changed() {
                                state.lock().unwrap().timetravel_position = pos;
                            }
                            if ui.button("⏮ start").clicked() {
                                state.lock().unwrap().timetravel_position = 0;
                            }
                            if ui.button("⏭ now").clicked() {
                                state.lock().unwrap().timetravel_position = total;
                            }
                        }
                        if watch_enabled {
                            ui.separator();
                            ui.label(
                                egui::RichText::new("👁 watching")
                                    .color(Color32::from_rgb(220, 180, 90)),
                            );
                            ui.label(format!("{total} events on disk"));
                        }
                        if let Some(ev) = current_event {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!("intent: {}", ev.intent))
                                    .color(Color32::from_rgb(180, 192, 204)),
                            );
                            ui.label(format!("risk={:.2}", ev.risk_score));
                            ui.label(format!("{:?}", ev.approval));
                        }
                    });
                });
            }
        }

        // Branch the central panel based on view mode. 3D uses an
        // orbit camera and depth-sorted projection; 2D uses the
        // existing pan/zoom/select painter.
        let mode = state.lock().unwrap().mode;
        if mode == ViewMode::Scene3D {
            self.scene3d_panel(ctx);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let response = ui.allocate_response(avail, egui::Sense::click_and_drag());
            let viewport = response.rect;

            // Pan from drag.
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                if delta.length_sq() > 0.0 {
                    let mut s = state.lock().unwrap();
                    s.pan += delta;
                    s.user_has_panned = true;
                }
            }

            // Zoom from scroll wheel (focused at cursor).
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                if let Some(hover) = response.hover_pos() {
                    let mut s = state.lock().unwrap();
                    let old_zoom = s.zoom;
                    let factor = (scroll.y * 0.002).exp();
                    let new_zoom = (old_zoom * factor).clamp(0.05, 8.0);
                    // Keep the point under the cursor stationary.
                    let local = hover - viewport.min;
                    let world_pt = (local - s.pan) / old_zoom;
                    s.zoom = new_zoom;
                    s.pan = local - world_pt * new_zoom;
                    s.user_has_panned = true;
                }
            }

            // First-frame fit so we always see something useful.
            {
                let mut s = state.lock().unwrap();
                if !s.initial_fit_done && !s.canvas.nodes.is_empty() {
                    s.fit_to_view(viewport.size());
                    s.initial_fit_done = true;
                }
            }

            // Capture local copies so the painter borrow ends before
            // we may set selection state.
            let (pan, zoom, selected) = {
                let s = state.lock().unwrap();
                (s.pan, s.zoom, s.selected)
            };

            let painter = ui.painter_at(viewport);
            painter.rect_filled(viewport, 0.0, Color32::from_rgb(14, 17, 22));

            // Helper to project a canvas (world) point into screen space.
            let project = |x: f32, y: f32| -> Pos2 {
                Pos2::new(
                    viewport.min.x + pan.x + x * zoom,
                    viewport.min.y + pan.y + y * zoom,
                )
            };

            // Draw edges first so nodes overlap them.
            {
                let s = state.lock().unwrap();
                for edge in s.canvas.edges.values() {
                    let (Some(a), Some(b)) =
                        (s.canvas.nodes.get(&edge.from), s.canvas.nodes.get(&edge.to))
                    else {
                        continue;
                    };
                    let (acx, acy) = a.bounds.center();
                    let (bcx, bcy) = b.bounds.center();
                    let p0 = project(acx, acy);
                    let p1 = project(bcx, bcy);
                    let stroke = Stroke::new(
                        (1.0_f32).max(edge.style.width * zoom * 0.8),
                        edge_color(&edge.kind),
                    );
                    painter.line_segment([p0, p1], stroke);
                    draw_arrowhead(&painter, p1, p0, stroke);
                }
            }

            // Draw nodes.
            let mut clicked_node: Option<CanvasNodeId> = None;
            {
                let s = state.lock().unwrap();
                let perspective = s.perspective;
                for node in s.canvas.nodes.values() {
                    let tl = project(node.bounds.x, node.bounds.y);
                    let br = project(node.bounds.x + node.bounds.w, node.bounds.y + node.bounds.h);
                    let rect = Rect::from_two_pos(tl, br);
                    let fill = node_fill_with(node, perspective);
                    let is_selected = selected == Some(node.id);
                    let stroke = if is_selected {
                        Stroke::new(2.5, Color32::from_rgb(97, 175, 239))
                    } else {
                        Stroke::new(1.0, Color32::from_rgb(60, 70, 84))
                    };
                    painter.rect(rect, Rounding::same(4.0), fill, stroke);

                    // Label, only when zoomed in enough to read.
                    if zoom > 0.45 {
                        let font_size = (12.0_f32 * zoom.min(1.5)).clamp(9.0, 18.0);
                        painter.text(
                            rect.left_top() + Vec2::new(6.0, 4.0),
                            egui::Align2::LEFT_TOP,
                            &node.label,
                            FontId::proportional(font_size),
                            Color32::from_rgb(220, 232, 244),
                        );
                    }
                }
            }

            // Click selection — pick the smallest-area node containing the click.
            if response.clicked() {
                if let Some(click) = response.interact_pointer_pos() {
                    let click_local = click - viewport.min;
                    let world_pt = Pos2::new(
                        (click_local.x - pan.x) / zoom,
                        (click_local.y - pan.y) / zoom,
                    );
                    let s = state.lock().unwrap();
                    let mut best: Option<(CanvasNodeId, f32)> = None;
                    for n in s.canvas.nodes.values() {
                        if n.bounds.contains((world_pt.x, world_pt.y)) {
                            let area = n.bounds.w * n.bounds.h;
                            if best.map(|(_, b)| area < b).unwrap_or(true) {
                                best = Some((n.id, area));
                            }
                        }
                    }
                    clicked_node = best.map(|(id, _)| id);
                }
            }

            if let Some(id) = clicked_node {
                state.lock().unwrap().selected = Some(id);
            }

            // Status overlay (bottom-right).
            let status = {
                let s = state.lock().unwrap();
                if let Some(id) = s.selected {
                    if let Some(n) = s.canvas.nodes.get(&id) {
                        format!("{}  ·  {}", n.label, n.semantic_id)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            };
            if !status.is_empty() {
                let pos = viewport.right_bottom() - Vec2::new(12.0, 12.0);
                painter.text(
                    pos,
                    egui::Align2::RIGHT_BOTTOM,
                    status,
                    FontId::proportional(12.0),
                    Color32::from_rgb(180, 192, 204),
                );
            }
        });

        // Cinematic boot overlay — rendered last so it sits on top of
        // the canvas/scene. Dismissed by the boot tick at the top of
        // `update`.
        self.boot_overlay(ctx);
    }
}

impl WishApp {
    /// Cinematic boot overlay: a large translucent title during the
    /// initial splash, and a soft "✨ lens: <name>" pill during the
    /// perspective cycle. Hidden during `BootState::Done`.
    fn boot_overlay(&mut self, ctx: &egui::Context) {
        let state = self.state.clone();
        let (boot, perspective) = {
            let s = state.lock().unwrap();
            (s.boot, s.perspective)
        };
        if matches!(boot, BootState::Done) {
            return;
        }
        let screen = ctx.screen_rect();
        let area_resp = egui::Area::new(egui::Id::new("wish-boot-overlay"))
            .fixed_pos(screen.left_top())
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.allocate_space(screen.size());
            });
        // The Area allocates the full screen so any pointer event
        // there bubbles to the dismiss check in `update`. We don't
        // actually consume the event.
        let _ = area_resp;

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("wish-boot-painter"),
        ));

        match boot {
            BootState::Splash => {
                // Dim the canvas behind the splash.
                painter.rect_filled(
                    screen,
                    0.0,
                    Color32::from_rgba_unmultiplied(11, 14, 19, 220),
                );
                let center = screen.center();
                painter.text(
                    center - Vec2::new(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    "✦ W I S H",
                    FontId::proportional(72.0),
                    Color32::from_rgb(220, 232, 244),
                );
                painter.text(
                    center + Vec2::new(0.0, 32.0),
                    egui::Align2::CENTER_CENTER,
                    "the World Model IDE  ·  v0.5.0  ·  the Tensorium",
                    FontId::proportional(15.0),
                    Color32::from_rgb(140, 165, 200),
                );
                painter.text(
                    center + Vec2::new(0.0, 56.0),
                    egui::Align2::CENTER_CENTER,
                    "click anywhere to skip",
                    FontId::proportional(11.0),
                    Color32::from_rgba_unmultiplied(140, 152, 168, 180),
                );
            }
            BootState::Cycling { .. } => {
                // Soft pill at the top showing the current lens being
                // cycled. The canvas stays fully visible behind it.
                let pill_w = 360.0_f32;
                let pill_h = 36.0_f32;
                let cx = screen.center().x;
                let top = screen.top() + 64.0;
                let rect = egui::Rect::from_center_size(
                    egui::Pos2::new(cx, top + pill_h * 0.5),
                    egui::Vec2::new(pill_w, pill_h),
                );
                painter.rect(
                    rect,
                    egui::Rounding::same(18.0),
                    Color32::from_rgba_unmultiplied(22, 27, 34, 220),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(97, 175, 239, 90)),
                );
                painter.text(
                    rect.left_center() + Vec2::new(14.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("✨ {}", perspective.label()),
                    FontId::proportional(13.0),
                    Color32::from_rgb(220, 232, 244),
                );
                painter.text(
                    rect.right_center() - Vec2::new(14.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    "  cycling… click to lock in",
                    FontId::proportional(11.0),
                    Color32::from_rgb(140, 152, 168),
                );
            }
            BootState::Done => {}
        }
    }

    fn scene3d_panel(&mut self, ctx: &egui::Context) {
        let state = self.state.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let response = ui.allocate_response(avail, egui::Sense::click_and_drag());
            let viewport = response.rect;

            // First-frame: auto-aim the camera at the world centroid.
            {
                let mut s = state.lock().unwrap();
                if !s.scene3d_initial_frame_done && s.world.is_some() {
                    s.frame_camera_to_world();
                    s.scene3d_initial_frame_done = true;
                }
            }

            // Orbit camera input.
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                if delta.length_sq() > 0.0 {
                    let mut s = state.lock().unwrap();
                    s.camera3d.yaw -= delta.x * 0.008;
                    s.camera3d.pitch = (s.camera3d.pitch - delta.y * 0.008).clamp(-1.3, 1.3);
                }
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 && response.hovered() {
                let mut s = state.lock().unwrap();
                let factor = (-scroll.y * 0.002).exp();
                s.camera3d.distance = (s.camera3d.distance * factor).clamp(2.0, 600.0);
            }

            // Render.
            let projected = {
                let s = state.lock().unwrap();
                if let Some(world) = &s.world {
                    let painter = ui.painter_at(viewport);
                    scene3d::render(
                        &painter,
                        viewport,
                        world,
                        &s.canvas.nodes,
                        &s.camera3d,
                        s.selected,
                    )
                } else {
                    Vec::new()
                }
            };

            // Click selection — pick the nearest projected node.
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let picked = scene3d::pick_at(&projected, pos, 40.0);
                    state.lock().unwrap().selected = picked;
                }
            }

            // Status overlay.
            let status = {
                let s = state.lock().unwrap();
                s.selected
                    .and_then(|id| s.canvas.nodes.get(&id).cloned())
                    .map(|n| format!("{}  ·  {}", n.label, n.semantic_id))
                    .unwrap_or_default()
            };
            let painter = ui.painter_at(viewport);
            if !status.is_empty() {
                let pos = viewport.right_bottom() - Vec2::new(12.0, 12.0);
                painter.text(
                    pos,
                    egui::Align2::RIGHT_BOTTOM,
                    status,
                    FontId::proportional(12.0),
                    Color32::from_rgb(180, 192, 204),
                );
            }
            let hint = "drag to orbit · scroll to dolly · click a node to select";
            let pos = viewport.left_bottom() + Vec2::new(12.0, -12.0);
            painter.text(
                pos,
                egui::Align2::LEFT_BOTTOM,
                hint,
                FontId::proportional(11.0),
                Color32::from_rgba_unmultiplied(140, 152, 168, 220),
            );
        });
    }
}

/// One row in the perspective dropdown — extracted so we can render
/// the same row inside both the Domain group and the Science group
/// without duplicating the click + state-snap logic.
fn select_perspective_row(
    ui: &mut egui::Ui,
    state: &Arc<Mutex<AppState>>,
    p: Perspective,
    current: Perspective,
) {
    let resp = ui
        .selectable_label(p == current, p.label())
        .on_hover_text(p.tagline());
    if resp.clicked() {
        let mut s = state.lock().unwrap();
        s.perspective = p;
        s.canvas.layout = p.default_layout();
        if p.prefers_3d() {
            s.mode = ViewMode::Scene3D;
            s.scene3d_initial_frame_done = false;
        } else {
            s.mode = ViewMode::Canvas2D;
        }
        wish_canvas_core::layout::run(&mut s.canvas);
        s.user_has_panned = false;
        s.initial_fit_done = false;
    }
}

fn canvas_bbox(canvas: &Canvas) -> Option<(f32, f32, f32, f32)> {
    if canvas.nodes.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for n in canvas.nodes.values() {
        min_x = min_x.min(n.bounds.x);
        min_y = min_y.min(n.bounds.y);
        max_x = max_x.max(n.bounds.x + n.bounds.w);
        max_y = max_y.max(n.bounds.y + n.bounds.h);
    }
    if !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

fn node_fill_with(node: &CanvasNode, perspective: Perspective) -> Color32 {
    let [r, g, b, a] = node.style.fill;
    let base = Color32::from_rgba_unmultiplied(r, g, b, a);
    // Tint by perspective + kind so a 3,000-node repo isn't a wall of
    // grey and switching perspective live re-colors meaningfully.
    let tint = perspective.tint(&node.kind);
    Color32::from_rgba_unmultiplied(
        ((base.r() as u16 + tint.r() as u16) / 2) as u8,
        ((base.g() as u16 + tint.g() as u16) / 2) as u8,
        ((base.b() as u16 + tint.b() as u16) / 2) as u8,
        base.a().max(220),
    )
}

/// Default-perspective tint for a canvas node kind. Used by 3D
/// scene rendering (which doesn't carry AppState directly). The
/// per-perspective overrides live in `perspective::Perspective::tint`.
pub(crate) fn kind_tint(kind: &CanvasNodeKind) -> Color32 {
    perspective::default_tint(kind)
}

fn kind_glyph(kind: &CanvasNodeKind) -> &'static str {
    match kind {
        CanvasNodeKind::File => "📄",
        CanvasNodeKind::Function => "ƒ",
        CanvasNodeKind::Crate => "📦",
        CanvasNodeKind::Package | CanvasNodeKind::Module => "▤",
        CanvasNodeKind::Service => "⚡",
        CanvasNodeKind::Agent => "🤖",
        CanvasNodeKind::ToolCall | CanvasNodeKind::PlanStep => "▷",
        CanvasNodeKind::Test => "✓",
        CanvasNodeKind::Commit | CanvasNodeKind::Branch => "⎇",
        CanvasNodeKind::Diff => "Δ",
        CanvasNodeKind::TerminalBlock => "❯",
        CanvasNodeKind::DocumentSection => "¶",
        CanvasNodeKind::Npc => "👤",
        CanvasNodeKind::Quest => "✦",
        CanvasNodeKind::Custom(_) => "◆",
    }
}

fn edge_color(kind: &EdgeKind) -> Color32 {
    match kind {
        EdgeKind::Imports | EdgeKind::DependsOn => {
            Color32::from_rgba_unmultiplied(97, 175, 239, 160)
        }
        EdgeKind::Calls => Color32::from_rgba_unmultiplied(140, 200, 130, 160),
        EdgeKind::Produces | EdgeKind::Triggers => {
            Color32::from_rgba_unmultiplied(220, 180, 110, 160)
        }
        EdgeKind::Tests => Color32::from_rgba_unmultiplied(120, 200, 120, 160),
        EdgeKind::Spawned => Color32::from_rgba_unmultiplied(200, 120, 200, 160),
        EdgeKind::SucceededBy => Color32::from_rgba_unmultiplied(100, 200, 100, 160),
        EdgeKind::FailedBy => Color32::from_rgba_unmultiplied(220, 100, 100, 160),
        EdgeKind::Mentions | EdgeKind::Custom(_) => {
            Color32::from_rgba_unmultiplied(140, 150, 165, 160)
        }
    }
}

fn draw_arrowhead(painter: &egui::Painter, tip: Pos2, from: Pos2, stroke: Stroke) {
    let v = tip - from;
    let len = (v.x * v.x + v.y * v.y).sqrt();
    if len < 1.0 {
        return;
    }
    let n = Vec2::new(v.x / len, v.y / len);
    let perp = Vec2::new(-n.y, n.x);
    let size = 6.0;
    let base = tip - n * size;
    let p_left = base + perp * (size * 0.6);
    let p_right = base - perp * (size * 0.6);
    painter.line_segment([tip, p_left], stroke);
    painter.line_segment([tip, p_right], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_canvas_core::{
        layout,
        types::{CanvasNode, CanvasNodeKind, LayoutMode, Rect as CRect},
    };
    use wish_world_model::SemanticId;

    #[test]
    fn bbox_of_empty_canvas_is_none() {
        let c = Canvas::new();
        assert!(canvas_bbox(&c).is_none());
    }

    #[test]
    fn bbox_after_layout_is_finite() {
        let mut c = Canvas::new();
        for i in 0..10 {
            c.upsert_node(CanvasNode::new(
                SemanticId::code_function(&format!("f_{i}")),
                format!("f_{i}"),
                CanvasNodeKind::Function,
                CRect {
                    x: 0.0,
                    y: 0.0,
                    w: 80.0,
                    h: 30.0,
                },
            ));
        }
        c.layout = LayoutMode::ForceDirected;
        layout::run(&mut c);
        let bb = canvas_bbox(&c).expect("bbox");
        assert!(bb.0.is_finite() && bb.1.is_finite() && bb.2.is_finite() && bb.3.is_finite());
        assert!(bb.2 - bb.0 > 0.0);
        assert!(bb.3 - bb.1 > 0.0);
    }

    #[test]
    fn kind_glyph_covers_every_canvas_node_kind() {
        // Compile-time exhaustive check that kind_glyph handles every
        // variant. If a new variant is added to CanvasNodeKind, this
        // test will fail to compile and force us to update the renderer.
        let kinds = [
            CanvasNodeKind::File,
            CanvasNodeKind::Function,
            CanvasNodeKind::Crate,
            CanvasNodeKind::Package,
            CanvasNodeKind::Module,
            CanvasNodeKind::Service,
            CanvasNodeKind::Agent,
            CanvasNodeKind::ToolCall,
            CanvasNodeKind::PlanStep,
            CanvasNodeKind::Test,
            CanvasNodeKind::Commit,
            CanvasNodeKind::Branch,
            CanvasNodeKind::Diff,
            CanvasNodeKind::TerminalBlock,
            CanvasNodeKind::DocumentSection,
            CanvasNodeKind::Npc,
            CanvasNodeKind::Quest,
            CanvasNodeKind::Custom("world".into()),
        ];
        for k in &kinds {
            assert!(!kind_glyph(k).is_empty());
        }
    }
}
