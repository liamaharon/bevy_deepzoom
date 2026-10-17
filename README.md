# bevy_deepzoom

![Crates.io Version](https://img.shields.io/crates/v/bevy_deepzoom)

Render [Deep Zoom](https://en.wikipedia.org/wiki/Deep_Zoom) image pyramids in Bevy.

![bevy_deepzoom demo](readme-screencap.gif)

## Compatibility

| `bevy_deepzoom` | `bevy` |
| --------------- | ------ |
| `0.0.5–0.0.8`   | `0.19` |
| `0.0.4`         | `0.18` |
| `0.0.1`         | `0.17` |

## Usage

```rust
use bevy::prelude::*;
use bevy_deepzoom::{DeepZoom, DeepZoomConfig, DeepZoomPlugin};

fn spawn_camera(mut commands: Commands) {
    let config = DeepZoomConfig::new("assets/map/tiles.dzi", "assets/map/tiles_files");
    commands.spawn((Camera2d, DeepZoom::from_config(config)));
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DeepZoomPlugin)
        .add_systems(Startup, spawn_camera)
        .run();
}
```

### Drawing tiles yourself

By default each tile is a `Sprite`. To draw tiles with your own material (for
example to tint them or add an effect in the tile shader), set
`DeepZoomTileVisual::Custom` and add your visual when a `DziTile` spawns. The
tile's `Transform` is its top-left corner, `DziTile::size` its extent growing
right and down, and `DziTile::image` its texture.

```rust,ignore
let config = DeepZoomConfig::new("assets/map/tiles.dzi", "assets/map/tiles_files")
    .with_tile_visual(DeepZoomTileVisual::Custom);

fn draw_tile(add: On<Add, DziTile>, tiles: Query<&DziTile>, mut commands: Commands) {
    let tile = tiles.get(add.entity).unwrap();
    // Insert e.g. a `Mesh2d` covering `tile.size` from the top-left, with a
    // material sampling `tile.image`.
}
```

## Docs

See [`https://docs.rs/bevy_deepzoom`](https://docs.rs/bevy_deepzoom).
