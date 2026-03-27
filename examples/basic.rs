use bevy::{
    camera::CameraProjection,
    ecs::system::{lifetimeless::SQuery, SystemParam},
    input::mouse::MouseWheel,
    prelude::*,
    window::PrimaryWindow,
};
use bevy_deepzoom::{DeepZoom, DeepZoomConfig, DeepZoomPlugin};
use iyes_perf_ui::{prelude::*, PerfUiPlugin};

const DZI_PATH: &str = "https://openseadragon.github.io/example-images/highsmith/highsmith.dzi";
const TILES_BASE_PATH: &str =
    "https://openseadragon.github.io/example-images/highsmith/highsmith_files";

fn main() {
    App::new()
        .init_resource::<DragState>()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin::default())
        .add_plugins(PerfUiPlugin)
        .add_plugins(DeepZoomPlugin)
        .add_perf_ui_simple_entry::<PerfUiRequiredZoomLevel>()
        .add_systems(Startup, (spawn_camera, spawn_perf_ui))
        .add_systems(Update, (drag_pan_camera, zoom_camera))
        .run();
}

fn spawn_camera(mut commands: Commands) {
    let config = DeepZoomConfig::new(DZI_PATH, TILES_BASE_PATH)
        // Don't hammer openseadragon too hard
        .with_max_concurrent_tile_loads(4)
        .with_draw_debug_ui(true);

    commands.spawn((Camera2d, DeepZoom::from_config(config)));
}

fn spawn_perf_ui(mut commands: Commands) {
    commands.spawn((
        PerfUiRoot {
            values_col_width: 56.0,
            ..default()
        },
        PerfUiEntryFPS::default(),
        PerfUiEntryEntityCount::default(),
        PerfUiRequiredZoomLevel,
    ));
}

#[derive(Resource, Default)]
struct DragState {
    last_cursor: Option<Vec2>,
}

fn drag_pan_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut drag_state: ResMut<DragState>,
    camera: Single<(&Projection, &mut Transform), With<Camera2d>>,
) {
    let window = window.into_inner();
    let Some(cursor_position) = window.cursor_position() else {
        drag_state.last_cursor = None;
        return;
    };

    if mouse_buttons.just_released(MouseButton::Left) {
        drag_state.last_cursor = None;
        return;
    }

    if !mouse_buttons.pressed(MouseButton::Left) {
        drag_state.last_cursor = Some(cursor_position);
        return;
    }

    let Some(last_cursor) = drag_state.last_cursor else {
        drag_state.last_cursor = Some(cursor_position);
        return;
    };

    let (projection, mut transform) = camera.into_inner();

    let Projection::Orthographic(projection) = projection else {
        drag_state.last_cursor = Some(cursor_position);
        return;
    };

    let delta = cursor_position - last_cursor;
    let world_delta = Vec2::new(delta.x, -delta.y) * projection.area.size() / window.size();
    transform.translation -= world_delta.extend(0.0);
    drag_state.last_cursor = Some(cursor_position);
}

fn zoom_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    camera: Single<&mut Projection, With<Camera2d>>,
) {
    let scroll_delta: f32 = mouse_wheel.read().map(|event| event.y).sum();
    if scroll_delta.abs() <= f32::EPSILON {
        return;
    }

    let window = window.into_inner();
    let mut projection = camera.into_inner();

    let Projection::Orthographic(projection) = &mut *projection else {
        return;
    };

    projection.scale *= (1.0 - scroll_delta * 0.12).clamp(0.5, 2.0);
    projection.update(window.width(), window.height());
}

#[derive(Component, Default)]
struct PerfUiRequiredZoomLevel;

impl iyes_perf_ui::entry::PerfUiEntry for PerfUiRequiredZoomLevel {
    type Value = u32;
    type SystemParam = SQuery<&'static DeepZoom, With<Camera2d>>;

    fn label(&self) -> &str {
        "Zoom Level"
    }

    fn sort_key(&self) -> i32 {
        10000
    }

    fn update_value(
        &self,
        deep_zoom_query: &mut <Self::SystemParam as SystemParam>::Item<'_, '_>,
    ) -> Option<Self::Value> {
        deep_zoom_query.iter().next().map(DeepZoom::zoom_level)
    }
}
