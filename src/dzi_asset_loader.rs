//! Loader for the .dzi file format.

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::{App, Asset, AssetApp};
use bevy::reflect::TypePath;
use serde::Deserialize;

#[derive(Debug, Deserialize, Asset, TypePath)]
#[serde(rename_all = "PascalCase")]
#[serde(rename = "Image")]
pub struct DziContents {
    #[serde(rename = "@Format")]
    pub format: String,
    #[serde(rename = "@Overlap")]
    pub overlap: u32,
    #[serde(rename = "@TileSize")]
    pub tile_size: u32,
    #[serde(rename = "Size")]
    pub size: Size,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Size {
    #[serde(rename = "@Width")]
    pub width: u32,
    #[serde(rename = "@Height")]
    pub height: u32,
}

#[derive(Default, TypePath)]
pub struct DziAssetLoader;

impl AssetLoader for DziAssetLoader {
    type Asset = DziContents;
    type Settings = ();
    type Error = quick_xml::de::DeError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = vec![];
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| quick_xml::de::DeError::Custom(e.to_string()))?;
        let str: &str = std::str::from_utf8(bytes.as_slice())
            .map_err(|e| quick_xml::de::DeError::Custom(e.to_string()))?;
        quick_xml::de::from_str(str)
    }

    fn extensions(&self) -> &[&str] {
        &["dzi"]
    }
}

pub fn plugin(app: &mut App) {
    app.init_asset_loader::<DziAssetLoader>();
    app.init_asset::<DziContents>();
}
