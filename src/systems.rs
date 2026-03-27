use super::*;
use crate::ortho_ext::OrthoExt as _;
use bevy::{
    asset::{LoadState, RenderAssetUsages},
    color::palettes::basic::{GREEN, YELLOW},
    image::ImageLoaderSettings,
    math::ops::log2,
    prelude::*,
    sprite::Anchor,
};
use std::cmp::max;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LevelGeometry {
    width: u32,
    height: u32,
    scale: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TileGeometry {
    top_left: Vec2,
    size: Vec2,
    rect: Rect,
}

pub(crate) fn load_dzi_asset(
    asset_server: Res<AssetServer>,
    mut viewers: Query<&mut DeepZoom, (Added<DeepZoom>, With<Camera2d>)>,
) {
    for mut deep_zoom in viewers.iter_mut() {
        deep_zoom.state = DeepZoomState {
            dzi: Some(asset_server.load(deep_zoom.config.dzi_path.clone())),
            ..Default::default()
        };
    }
}

pub(crate) fn finish_loading_dzi(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    dzi_assets: Res<Assets<DziContents>>,
    mut viewers: Query<(Entity, &mut Projection, &mut DeepZoom), With<Camera2d>>,
) {
    for (entity, mut projection, mut deep_zoom) in viewers.iter_mut() {
        if deep_zoom.state.load_state != DeepZoomLoadState::Loading {
            continue;
        }

        let dzi_handle = deep_zoom
            .state
            .dzi
            .as_ref()
            .expect("DeepZoom viewer must have a DZI handle while loading");

        if dzi_assets.get(dzi_handle).is_some() {
            if deep_zoom.config.initial_view == DeepZoomInitialView::FitWidth {
                let dzi = crate::loaded_dzi(&deep_zoom, &dzi_assets)
                    .expect("loaded DeepZoom viewer must have a loaded DZI asset");
                let projection = projection.ortho_mut().expect("2d cam is ortho");
                projection.scale = crate::fit_width_scale(dzi, projection);
            }

            deep_zoom.state.load_state = DeepZoomLoadState::Loaded;
            commands.trigger(DeepZoomLoaded(entity));
            continue;
        }

        if let Some(LoadState::Failed(error)) = asset_server.get_load_state(dzi_handle.id()) {
            deep_zoom.state.load_state = DeepZoomLoadState::Failed;
            commands.trigger(DeepZoomLoadFailed(entity, error));
        }
    }
}

pub(crate) fn update_zoom_level(
    dzi_assets: Res<Assets<DziContents>>,
    mut viewers: Query<(&mut Projection, &mut DeepZoom), With<Camera2d>>,
) {
    for (mut proj, mut deep_zoom) in viewers.iter_mut() {
        if deep_zoom.state.load_state != DeepZoomLoadState::Loaded {
            continue;
        }

        let dzi = crate::loaded_dzi(&deep_zoom, &dzi_assets)
            .expect("loaded DeepZoom viewer must have a loaded DZI asset");

        let proj = proj.ortho_mut().expect("2d cam is ortho");
        let full_resolution_level = get_zoom_levels(dzi, deep_zoom.config.pyramid_depth);
        let required_zoom_level = get_required_zoom_level(
            full_resolution_level,
            proj.scale,
            deep_zoom.config.zoom_level_bias,
        )
        .min(full_resolution_level);
        if deep_zoom.state.zoom_level == required_zoom_level {
            continue;
        }

        deep_zoom.state.zoom_level = required_zoom_level;
        match deep_zoom.state.lowest_zoom_level {
            Some(lowest) if deep_zoom.state.zoom_level < lowest => {
                deep_zoom.state.lowest_zoom_level = Some(deep_zoom.state.zoom_level)
            }
            None => deep_zoom.state.lowest_zoom_level = Some(deep_zoom.state.zoom_level),
            _ => {}
        }
    }
}

pub(crate) fn render_debug_ui(
    viewers: Query<&DeepZoom, With<Camera2d>>,
    tiles: Query<&DziTile>,
    mut gizmos: Gizmos,
) {
    for tile in tiles.iter() {
        let Ok(deep_zoom) = viewers.get(tile.owner_entity) else {
            continue;
        };
        if !deep_zoom.config.draw_debug_ui {
            continue;
        }

        let color = if deep_zoom.state.tiles_loading.contains(&tile.id) {
            YELLOW
        } else {
            GREEN
        };
        let size = tile.rect.size();
        let center = tile.rect.center();
        let iso = Isometry2d::new(center, 0.0.into());
        gizmos.rect_2d(iso, size, Color::Srgba(color));
    }
}

pub(crate) fn cleanup_orphaned_tiles(
    mut commands: Commands,
    tiles: Query<(Entity, &DziTile)>,
    viewers: Query<(), (With<Camera2d>, With<DeepZoom>)>,
) {
    for (entity, tile) in tiles.iter() {
        if viewers.get(tile.owner_entity).is_err() {
            commands.entity(entity).despawn();
        }
    }
}

pub(crate) fn despawn_out_of_view_tiles(
    mut commands: Commands,
    tiles: Query<(Entity, &DziTile)>,
    mut viewers: Query<(Entity, &Projection, &Transform, &mut DeepZoom), With<Camera2d>>,
) {
    for (viewer_entity, proj, transform, mut deep_zoom) in viewers.iter_mut() {
        if deep_zoom.state.load_state != DeepZoomLoadState::Loaded {
            continue;
        }

        let proj = proj.ortho().expect("we have 2d cam");
        let viewport_rect = get_rect_in_view(proj, transform);
        let zoom_level = deep_zoom.state.zoom_level;
        let lowest_zoom_level = deep_zoom.state.lowest_zoom_level;

        for (entity, tile) in tiles.iter() {
            if tile.owner_entity != viewer_entity {
                continue;
            }

            // Keep around the lowest zoom level so there's always something to see on the screen
            if matches!(lowest_zoom_level, Some(lowest) if tile.zoom_level == lowest) {
                continue;
            }

            if viewport_rect.intersect(tile.rect).is_empty() || tile.zoom_level > zoom_level {
                commands.entity(entity).despawn();
                deep_zoom.state.tiles_loading.remove(&tile.id);
                deep_zoom.state.tiles_loaded.remove(&tile.id);
            }
        }
    }
}

pub(crate) fn check_tile_loading_status(
    mut messages: MessageReader<AssetEvent<Image>>,
    tiles: Query<(&DziTile, &Sprite)>,
    mut viewers: Query<&mut DeepZoom, With<Camera2d>>,
) {
    for message in messages.read() {
        if let AssetEvent::LoadedWithDependencies { id } = message {
            for (tile, sprite) in tiles.iter() {
                if sprite.image.id() == *id {
                    let Ok(mut deep_zoom) = viewers.get_mut(tile.owner_entity) else {
                        continue;
                    };
                    deep_zoom.state.tiles_loading.remove(&tile.id);
                    deep_zoom.state.tiles_loaded.insert(tile.id);
                }
            }
        }
    }
}

pub(crate) fn spawn_in_view_tiles(
    mut commands: Commands,
    dzi_assets: Res<Assets<DziContents>>,
    asset_server: Res<AssetServer>,
    mut viewers: Query<(Entity, &Projection, &Transform, &mut DeepZoom), With<Camera2d>>,
) {
    'viewers: for (viewer_entity, proj, transform, mut deep_zoom) in viewers.iter_mut() {
        if deep_zoom.state.load_state != DeepZoomLoadState::Loaded {
            continue;
        }
        if deep_zoom.state.tiles_loading.len() >= deep_zoom.config.max_concurrent_tile_loads {
            continue;
        }

        let dzi = crate::loaded_dzi(&deep_zoom, &dzi_assets)
            .expect("loaded DeepZoom viewer must have a loaded DZI asset");

        let proj = proj.ortho().expect("we have 2d cam");
        let viewport_rect = get_rect_in_view(proj, transform);
        let max_tile_size = dzi.tile_size;

        for level in 0..=deep_zoom.state.zoom_level {
            let geometry = level_geometry(dzi, deep_zoom.config.pyramid_depth, level);
            let tiles_across = geometry.width.div_ceil(max_tile_size);
            let tiles_down = geometry.height.div_ceil(max_tile_size);

            for i in 0..tiles_across {
                for j in 0..tiles_down {
                    if deep_zoom.state.tiles_loading.len()
                        >= deep_zoom.config.max_concurrent_tile_loads
                    {
                        continue 'viewers;
                    }

                    let id = (level as u16, i as u16, j as u16);
                    if deep_zoom.state.tiles_loading.contains(&id)
                        || deep_zoom.state.tiles_loaded.contains(&id)
                    {
                        continue;
                    }

                    let tile = tile_geometry(dzi, geometry.scale, i, j, tiles_across, tiles_down);

                    if viewport_rect.intersect(tile.rect).is_empty() {
                        continue;
                    }

                    let tiles_base_path = &deep_zoom.config.tiles_base_path;
                    let tile_format = &dzi.format;
                    let path = format!("{tiles_base_path}/{level}/{i}_{j}.{tile_format}");
                    let handle = asset_server.load_with_settings(
                        path,
                        |settings: &mut ImageLoaderSettings| {
                            settings.asset_usage = RenderAssetUsages::RENDER_WORLD;
                        },
                    );
                    deep_zoom.state.tiles_loading.insert(id);
                    commands.spawn((
                        Name::from(format!("DZI Tile {level}/{i}_{j}")),
                        DziTile {
                            owner_entity: viewer_entity,
                            id,
                            zoom_level: level,
                            rect: tile.rect,
                        },
                        Sprite {
                            image: handle,
                            custom_size: Some(tile.size),
                            color: Color::WHITE,
                            ..Default::default()
                        },
                        Anchor::TOP_LEFT,
                        Transform::from_translation(Vec3::new(
                            tile.top_left.x,
                            tile.top_left.y,
                            level as f32 * 0.001 + deep_zoom.config.tile_layer,
                        )),
                    ));

                    continue 'viewers;
                }
            }
        }
    }
}

fn get_required_zoom_level(full_resolution_level: u32, scale: f32, zoom_level_bias: u32) -> u32 {
    full_resolution_level - (log2(scale).ceil() as u32).min(full_resolution_level) + zoom_level_bias
}

fn get_zoom_levels(dzi: &DziContents, pyramid_depth: DeepZoomPyramidDepth) -> u32 {
    let largest_side = max(dzi.size.width, dzi.size.height);
    match pyramid_depth {
        DeepZoomPyramidDepth::OnePixel => log2(largest_side.max(1) as f32).ceil() as u32,
        DeepZoomPyramidDepth::OneTile => {
            log2((largest_side as f32 / dzi.tile_size as f32).max(1.0)).ceil() as u32
        }
    }
}

fn level_geometry(
    dzi: &DziContents,
    pyramid_depth: DeepZoomPyramidDepth,
    level: u32,
) -> LevelGeometry {
    let full_resolution_level = get_zoom_levels(dzi, pyramid_depth);
    let level_offset = full_resolution_level.saturating_sub(level);
    let scale = 2u64.pow(level_offset);

    LevelGeometry {
        width: (dzi.size.width as u64).div_ceil(scale) as u32,
        height: (dzi.size.height as u64).div_ceil(scale) as u32,
        scale,
    }
}

fn tile_geometry(
    dzi: &DziContents,
    scale: u64,
    x_index: u32,
    y_index: u32,
    tiles_across: u32,
    tiles_down: u32,
) -> TileGeometry {
    let (left, right) = tile_axis_bounds(
        dzi.size.width,
        dzi.tile_size,
        dzi.overlap,
        scale,
        x_index,
        tiles_across,
    );
    let (top, bottom) = tile_axis_bounds(
        dzi.size.height,
        dzi.tile_size,
        dzi.overlap,
        scale,
        y_index,
        tiles_down,
    );

    let half_width = dzi.size.width as f32 * 0.5;
    let half_height = dzi.size.height as f32 * 0.5;
    let top_left = Vec2::new(left as f32 - half_width, half_height - top as f32);
    let bottom_right = Vec2::new(right as f32 - half_width, half_height - bottom as f32);

    TileGeometry {
        top_left,
        size: Vec2::new(bottom_right.x - top_left.x, top_left.y - bottom_right.y),
        rect: Rect::from_corners(top_left, bottom_right),
    }
}

fn tile_axis_bounds(
    full_size: u32,
    tile_size: u32,
    overlap: u32,
    scale: u64,
    index: u32,
    tile_count: u32,
) -> (u64, u64) {
    let full_size = full_size as u64;
    let tile_size = tile_size as u64;
    let overlap = overlap as u64 * scale;

    let unique_start = (index as u64 * tile_size * scale).min(full_size);
    let unique_end = ((index as u64 + 1) * tile_size * scale).min(full_size);
    let start = if index == 0 {
        unique_start
    } else {
        unique_start.saturating_sub(overlap)
    };
    let end = if index + 1 == tile_count {
        unique_end
    } else {
        (unique_end + overlap).min(full_size)
    };

    (start, end)
}

fn get_rect_in_view(proj: &OrthographicProjection, transform: &Transform) -> Rect {
    let position = transform.translation.truncate();
    Rect::from_corners(proj.area.min + position, proj.area.max + position)
}
