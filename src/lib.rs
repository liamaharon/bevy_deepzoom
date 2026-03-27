#![forbid(unsafe_code)]
#![doc(
    html_logo_url = "https://bevy.org/assets/icon.png",
    html_favicon_url = "https://bevy.org/assets/icon.png"
)]

//! Deep Zoom image rendering for 2D Bevy scenes.
//!
//! `bevy_deepzoom` allows streaming [Deep Zoom Image
//! (DZI)](https://en.wikipedia.org/wiki/Deep_Zoom) pyramids
//! into Bevy scenes by attaching [`DeepZoom`] components to [`Camera2d`] entities.
//!
//! # Quick start
//!
//! Add [`DeepZoomPlugin`] to your app and a [`DeepZoom`] component to your [`Camera2d`].
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_deepzoom::{DeepZoom, DeepZoomConfig, DeepZoomInitialView, DeepZoomPlugin};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(DeepZoomPlugin)
//!         .add_systems(Startup, spawn_camera)
//!         .run();
//! }
//!
//! fn spawn_camera(mut commands: Commands) {
//!     let config = DeepZoomConfig::new("assets/map/tiles.dzi", "assets/map/tiles_files")
//!         .with_initial_view(DeepZoomInitialView::FitWidth)
//!         .with_zoom_level_bias(1)
//!         .with_max_concurrent_tile_loads(8);
//!
//!     commands.spawn((
//!         Camera2d,
//!         DeepZoom::from_config(config),
//!     ));
//! }
//! ```
//!
//! The plugin
//! - loads the .dzi manifest
//! - derives the required tile zoom level from the camera projection scale
//! - streams in visible tiles for that level
//! - despawns tiles that are out of view or too high res for the camera scale
//! - clean up tiles when their owning camera is removed
//!
//! # Asset layout
//!
//! `bevy_deepzoom` expects a .dzi manifest and matching tile directory, typically:
//!
//! ```text
//! assets/map/tiles.dzi
//! assets/map/tiles_files/
//! assets/map/tiles_files/0/0_0.jpeg
//! assets/map/tiles_files/1/0_0.jpeg
//! assets/map/tiles_files/8/12_4.jpeg
//! ```
//!
//! # Generating a pyramid from a source image
//!
//! To create new DZI assets, `libvips dzsave` works well. e.g.:
//!
//! ```bash
//! vips dzsave "source.png" "tiles" \
//!   --layout dz \
//!   --tile-size 256 \
//!   --overlap 0 \
//!   --depth onepixel \
//!   --suffix ".jpeg[Q=90]"
//! ```
//!
//! # Pyramid depth
//!
//! [`DeepZoomPyramidDepth`] should be set based on the `libvips dzsave --depth ...` value used
//! when generating the DZI assets.
//!
//! # Tuning sharpness and request volume
//!
//! [`DeepZoomConfig::with_zoom_level_bias`] controls how aggressively the viewer prefers sharper
//! tiles.
//! A higher value loads higher-resolution levels sooner.
//!
//! [`DeepZoomConfig::with_max_concurrent_tile_loads`] limits how many tile loads are allowed to be
//! in flight for a viewer at once. Reduce this if you get rate limited or experience performance
//! issues when loading many tiles in parallel.
//!
//! # Events
//!
//! Use [`DeepZoomLoaded`] to run app-specific setup once the .dzi is loaded.
//! Use [`DeepZoomLoadFailed`] to react to failed loads.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_deepzoom::{DeepZoomLoadFailed, DeepZoomLoaded};
//!
//! fn on_deepzoom_loaded(
//!     loaded: On<DeepZoomLoaded>,
//!     mut cameras: Query<&mut Transform, With<Camera2d>>,
//! ) {
//!     let Ok(mut transform) = cameras.get_mut(loaded.event().0) else {
//!         return;
//!     };
//!
//!     transform.translation.z = 10.0;
//! }
//!
//! fn on_deepzoom_load_failed(failed: On<DeepZoomLoadFailed>) {
//!     let entity = failed.event().0;
//!     let reason = &failed.event().1;
//! }
//! ```

pub mod dzi_asset_loader;
mod ortho_ext;
mod systems;

use bevy::asset::AssetLoadError;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use dzi_asset_loader::DziContents;
use std::sync::Arc;

/// Default maximum number of concurrent in-flight tile loads.
pub const DEFAULT_MAX_CONCURRENT_TILE_LOADS: usize = 16;
/// Default number of zoom levels to bias toward sharper tiles.
pub const DEFAULT_ZOOM_LEVEL_BIAS: u32 = 2;

/// `(zoom_level, x, y)` identifier for a tile.
pub type TileId = (u16, u16, u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeepZoomInitialView {
    #[default]
    /// Fit the full image width into the current camera viewport when the .dzi finishes loading.
    FitWidth,
    /// Don't touch the camera projection.
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeepZoomPyramidDepth {
    #[default]
    /// Use with pyramids generated with `libvips dzsave --depth onepixel`.
    OnePixel,
    /// Use with pyramids generated with `libvips dzsave --depth onetile`.
    OneTile,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeepZoomLoadState {
    #[default]
    /// The .dzi manifest is loading.
    Loading,
    /// The .dzi manifest has loaded.
    Loaded,
    /// The .dzi manifest failed to load.
    Failed,
}

/// Attach this to a [`Camera2d`] entity.
#[derive(Component, Debug, Clone)]
pub struct DeepZoom {
    /// User supplied configuration.
    config: DeepZoomConfig,
    /// Runtime state managed internally by the crate.
    state: DeepZoomState,
}

#[derive(Debug, Clone)]
pub struct DeepZoomConfig {
    /// Path or URL to the `.dzi` manifest.
    dzi_path: String,
    /// Path or URL to the matching `*_files` directory.
    tiles_base_path: String,
    /// Automatic camera behavior when the manifest finishes loading.
    initial_view: DeepZoomInitialView,
    /// Pyramid depth convention used by the tile directory.
    pyramid_depth: DeepZoomPyramidDepth,
    /// Additional zoom levels to keep loaded above the base calculated level.
    ///
    /// Increase this to load higher resolution levels sooner.
    /// Decrease this to improve performance.
    zoom_level_bias: u32,
    /// z offset applied to spawned tile [`Transform`]s.
    tile_layer: f32,
    /// Draw gizmo rectangles visualising loaded and loading tiles.
    draw_debug_ui: bool,
    /// `16` is a sane default.
    ///
    /// Increase this to load more tiles in parallel.
    /// Decrease this to reduce system and network load.
    max_concurrent_tile_loads: usize,
}

/// Runtime state managed internally for each viewer.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeepZoomState {
    /// Loaded lazily after the component is attached to a camera entity.
    dzi: Option<Handle<DziContents>>,
    /// Manifest load state.
    load_state: DeepZoomLoadState,
    /// Current zoom level based on the camera projection scale.
    zoom_level: u32,
    /// Lowest zoom level kept alive as a fallback to avoid blank frames.
    lowest_zoom_level: Option<u32>,
    /// Tiles that have been requested but not finished loading yet.
    tiles_loading: HashSet<TileId>,
    /// Tiles that are currently loaded and rendered.
    tiles_loaded: HashSet<TileId>,
}

impl DeepZoom {
    pub fn from_config(config: DeepZoomConfig) -> Self {
        Self {
            config,
            state: DeepZoomState::default(),
        }
    }

    /// Toggles gizmo rectangles visualising loaded and loading tiles.
    pub fn set_draw_debug_ui(&mut self, draw_debug_ui: bool) {
        self.config.draw_debug_ui = draw_debug_ui;
    }

    /// Returns the current DZI manifest load state.
    pub fn load_state(&self) -> DeepZoomLoadState {
        self.state.load_state
    }

    /// Get the current zoom level based on the projection scale.
    pub fn zoom_level(&self) -> u32 {
        self.state.zoom_level
    }
}

impl DeepZoomConfig {
    pub fn new(dzi_path: impl Into<String>, tiles_base_path: impl Into<String>) -> Self {
        Self {
            dzi_path: dzi_path.into(),
            tiles_base_path: tiles_base_path.into(),
            initial_view: Default::default(),
            pyramid_depth: Default::default(),
            zoom_level_bias: DEFAULT_ZOOM_LEVEL_BIAS,
            tile_layer: 0.0,
            draw_debug_ui: false,
            max_concurrent_tile_loads: DEFAULT_MAX_CONCURRENT_TILE_LOADS,
        }
    }

    /// Sets the tile [`Transform`] z layer.
    pub fn with_tile_layer(mut self, tile_layer: f32) -> Self {
        self.tile_layer = tile_layer;
        self
    }

    /// Sets [`DeepZoomInitialView`].
    pub fn with_initial_view(mut self, initial_view: DeepZoomInitialView) -> Self {
        self.initial_view = initial_view;
        self
    }

    /// Sets [`DeepZoomPyramidDepth`].
    pub fn with_pyramid_depth(mut self, pyramid_depth: DeepZoomPyramidDepth) -> Self {
        self.pyramid_depth = pyramid_depth;
        self
    }

    /// Increase the bias to load higher resolution levels earlier.
    pub fn with_zoom_level_bias(mut self, zoom_level_bias: u32) -> Self {
        self.zoom_level_bias = zoom_level_bias;
        self
    }

    pub fn with_max_concurrent_tile_loads(mut self, max_concurrent_tile_loads: usize) -> Self {
        self.max_concurrent_tile_loads = max_concurrent_tile_loads;
        self
    }

    /// Toggles gizmo rectangles visualising loaded and loading tiles.
    pub fn with_draw_debug_ui(mut self, draw_debug_ui: bool) -> Self {
        self.draw_debug_ui = draw_debug_ui;
        self
    }
}

/// Returns the loaded .dzi.
pub fn loaded_dzi<'a>(
    deep_zoom: &'a DeepZoom,
    dzi_assets: &'a Assets<DziContents>,
) -> Option<&'a DziContents> {
    let dzi_handle = deep_zoom.state.dzi.as_ref()?;
    dzi_assets.get(dzi_handle)
}

/// Computes the [`Camera2d`] scale required to fit the full image width in view.
pub fn fit_width_scale(dzi: &DziContents, projection: &OrthographicProjection) -> f32 {
    let width_at_scale_one = projection.area.width() / projection.scale;
    dzi.size.width as f32 / width_at_scale_one
}

/// Triggered when a .dzi manifest finishes loading.
#[derive(Event, Debug, Clone, Copy)]
pub struct DeepZoomLoaded(pub Entity);

/// Triggered when a .dzi manifest fails to load.
#[derive(Event, Debug, Clone)]
pub struct DeepZoomLoadFailed(pub Entity, pub Arc<AssetLoadError>);

#[derive(Component, Debug)]
pub(crate) struct DziTile {
    pub owner_entity: Entity,
    pub id: TileId,
    pub zoom_level: u32,
    pub rect: Rect,
}

#[derive(Default)]
pub struct DeepZoomPlugin;

impl Plugin for DeepZoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(dzi_asset_loader::plugin);

        app.add_systems(
            Update,
            (
                (systems::load_dzi_asset, systems::finish_loading_dzi).chain(),
                systems::update_zoom_level,
                systems::render_debug_ui,
                systems::cleanup_orphaned_tiles,
                (
                    systems::check_tile_loading_status,
                    systems::spawn_in_view_tiles,
                    systems::despawn_out_of_view_tiles,
                )
                    .chain(),
            ),
        );
    }
}
