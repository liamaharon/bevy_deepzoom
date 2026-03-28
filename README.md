# bevy_deepzoom

![Crates.io Version](https://img.shields.io/crates/v/bevy_deepzoom)

Render [Deep Zoom](https://en.wikipedia.org/wiki/Deep_Zoom) image pyramids in Bevy.

![bevy_deepzoom demo](readme-screencap.gif)

## Compatibility

| `bevy_deepzoom` | `bevy` |
| --------------- | ------ |
| `0.0.2`         | `0.18` |
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

## Docs

See [`https://docs.rs/bevy_deepzoom`](https://docs.rs/bevy_deepzoom).
