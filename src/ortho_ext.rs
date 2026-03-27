use bevy::camera::{OrthographicProjection, Projection};

pub(crate) trait OrthoExt {
    fn ortho(&self) -> Option<&OrthographicProjection>;
    fn ortho_mut(&mut self) -> Option<&mut OrthographicProjection>;
}

impl OrthoExt for Projection {
    fn ortho(&self) -> Option<&OrthographicProjection> {
        if let Projection::Orthographic(o) = self {
            Some(o)
        } else {
            None
        }
    }

    fn ortho_mut(&mut self) -> Option<&mut OrthographicProjection> {
        if let Projection::Orthographic(o) = self {
            Some(o)
        } else {
            None
        }
    }
}
