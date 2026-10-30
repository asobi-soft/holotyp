//! Host-side settings of one preview session.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What the viewer paints behind a page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum PageBackground {
    /// The page's own paper: white, unless the document paints something else.
    #[default]
    White,
    /// Nothing: the viewer's canvas shows through.
    Transparent,
    /// The checkerboard editors use to mark transparent areas.
    Checkerboard,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase", default)]
pub struct PreviewSettings {
    /// Invert the colours of the rendered pages.
    #[cfg_attr(feature = "clap", arg(long))]
    pub invert: bool,
    /// What is painted behind each page.
    #[cfg_attr(
        feature = "clap",
        arg(
            long = "page-background",
            value_name = "PAGE_BACKGROUND",
            value_enum,
            default_value = "white"
        )
    )]
    pub background: PageBackground,
}
