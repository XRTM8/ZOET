//! Strongly typed domain models for ZOET media items, episodes, streams, and subtitles.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "krmzi")]
    Krmzi,
    #[serde(rename = "cinemana")]
    Cinemana,
    #[serde(rename = "all")]
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    #[serde(rename = "series")]
    Series,
    #[serde(rename = "movie")]
    Movie,
}

/// A media item displayed in the catalogue or search grid
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaCard {
    pub url: String,
    pub title: String,
    pub thumbnail: String,
    pub source: ProviderKind,
    #[serde(rename = "type")]
    pub kind: MediaKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
}

/// A playable video stream source (direct MP4, HLS, or embed)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StreamSource {
    pub label: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_id: Option<String>,
    #[serde(default)]
    pub is_hls: bool,
}

/// An external or embedded subtitle track
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub label: String,
    pub lang: String,
    pub url: String,
}

/// Detailed episode information including playable streams and subtitles
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeDetail {
    pub url: String,
    pub title: String,
    #[serde(rename = "seriesTitle")]
    pub series_title: String,
    #[serde(rename = "episodeNum")]
    pub episode_num: String,
    pub season: String,
    #[serde(rename = "seriesUrl")]
    pub series_url: String,
    pub artwork: String,
    #[serde(rename = "directStreams", default)]
    pub direct_streams: Vec<StreamSource>,
    #[serde(default)]
    pub servers: Vec<StreamSource>,
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
}
