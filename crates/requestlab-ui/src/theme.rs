//! Design tokens for RequestLab.
//!
//! Black, electric blue and white. Every colour, radius and size the interface
//! draws with is declared here exactly once; nothing outside this module writes
//! a raw colour literal. The same tokens are pushed into Ely's global palette,
//! so Ely components and RequestLab widgets cannot disagree about a colour.

use ely_gpui_component::theme::{Mode as ElyMode, Palette};
use gpui::{px, Hsla, Pixels, Rgba};
use requestlab_models::HttpMethod;

/// Which palette is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }

    /// The corresponding mode in Ely's global theme.
    pub fn ely_mode(self) -> ElyMode {
        match self {
            ThemeMode::Dark => ElyMode::Dark,
            ThemeMode::Light => ElyMode::Light,
        }
    }
}

/// A resolved palette plus the shared geometry values.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,

    /// Application background.
    pub background: Rgba,
    /// Sidebar and primary panels.
    pub sidebar: Rgba,
    /// Cards and secondary panels.
    pub surface: Rgba,
    /// Elevated surfaces: dialogs, popovers, tooltips.
    pub elevated: Rgba,
    /// Scrim behind a modal.
    pub overlay: Rgba,
    /// Hover state for a surface.
    pub hover: Rgba,
    /// Selected state for a surface.
    pub active: Rgba,

    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,

    pub border: Rgba,
    /// Focused border and strong separators.
    pub border_strong: Rgba,
    pub accent: Rgba,
    /// Hovered primary action.
    pub accent_hover: Rgba,
    /// Tinted background for selected rows.
    pub accent_muted: Rgba,

    pub success: Rgba,
    pub warning: Rgba,
    pub error: Rgba,
    pub info: Rgba,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

impl Theme {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Light => Theme::light(),
        }
    }

    /// The default dark palette: deep charcoal, electric blue, cool white.
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            background: rgb(0x080B12),
            sidebar: rgb(0x0D111B),
            surface: rgb(0x121827),
            elevated: rgb(0x192235),
            overlay: rgba(0x04060ACC),
            hover: rgb(0x1D2A40),
            active: rgb(0x142B50),
            text_primary: rgb(0xF1F5FC),
            text_secondary: rgb(0xA1AEC2),
            text_muted: rgb(0x68778E),
            border: rgb(0x253249),
            border_strong: rgb(0x347FFF),
            accent: rgb(0x2878FF),
            accent_hover: rgb(0x4895FF),
            accent_muted: rgb(0x142B50),
            success: rgb(0x31C48D),
            warning: rgb(0xF5B942),
            error: rgb(0xF06470),
            info: rgb(0x55A5FF),
        }
    }

    /// The light palette: paper white with the same blue.
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            background: rgb(0xF5F8FD),
            sidebar: rgb(0xFFFFFF),
            surface: rgb(0xEDF3FA),
            elevated: rgb(0xFFFFFF),
            overlay: rgba(0x14203333),
            hover: rgb(0xE3ECF8),
            active: rgb(0xE6F0FF),
            text_primary: rgb(0x142033),
            text_secondary: rgb(0x53647B),
            text_muted: rgb(0x8795A9),
            border: rgb(0xDCE5F0),
            border_strong: rgb(0x347FFF),
            accent: rgb(0x216BEB),
            accent_hover: rgb(0x347FFF),
            accent_muted: rgb(0xE6F0FF),
            success: rgb(0x1F9D6B),
            warning: rgb(0xB47812),
            error: rgb(0xD1495A),
            info: rgb(0x2C7ADB),
        }
    }

    /// Colour for an HTTP method chip.
    pub fn method_color(&self, method: HttpMethod) -> Rgba {
        match method {
            HttpMethod::Get => self.success,
            HttpMethod::Post => self.accent,
            HttpMethod::Put => self.warning,
            HttpMethod::Patch => rgb(0xA98BFF),
            HttpMethod::Delete => self.error,
            HttpMethod::Head => self.text_secondary,
            HttpMethod::Options => self.text_secondary,
        }
    }

    /// Colour for a response status.
    pub fn status_color(&self, status: Option<u16>) -> Rgba {
        match status {
            None => self.text_muted,
            Some(code) if (200..300).contains(&code) => self.success,
            Some(code) if (300..400).contains(&code) => self.info,
            Some(code) if (400..500).contains(&code) => self.warning,
            Some(code) if (500..600).contains(&code) => self.error,
            Some(_) => self.text_secondary,
        }
    }

    /// Colour for a JSON token type.
    pub fn json_color(&self, token: JsonToken) -> Rgba {
        match token {
            JsonToken::Key => self.info,
            JsonToken::String => self.success,
            JsonToken::Number => self.warning,
            JsonToken::Boolean => rgb(0xA98BFF),
            JsonToken::Null => self.text_muted,
            JsonToken::Punctuation => self.text_secondary,
        }
    }

    pub fn sans_family(&self) -> &'static str {
        SANS_FAMILY
    }

    pub fn mono_family(&self) -> &'static str {
        MONO_FAMILY
    }

    /// This palette expressed as Ely's tokens.
    fn ely_palette(&self) -> Palette {
        let mut palette = match self.mode {
            ThemeMode::Dark => Palette::dark(false),
            ThemeMode::Light => Palette::light(false),
        };
        palette.bg = self.background.into();
        palette.surface = self.surface.into();
        palette.sunken = self.sidebar.into();
        palette.overlay = self.elevated.into();
        palette.hover = self.hover.into();
        palette.active = self.active.into();
        palette.border = self.border.into();
        palette.border_strong = self.border_strong.into();
        palette.fg = self.text_primary.into();
        palette.fg_muted = self.text_secondary.into();
        palette.fg_subtle = self.text_muted.into();
        palette.fg_disabled = self.text_muted.into();
        palette.accent = self.accent.into();
        palette.accent_hover = self.accent_hover.into();
        palette.on_accent = self.background.into();
        palette.focus = self.border_strong.into();
        palette.link = self.accent_hover.into();
        palette.selection = self.active.into();
        palette.success = self.success.into();
        palette.warning = self.warning.into();
        palette.danger = self.error.into();
        palette.info = self.info.into();
        palette.backdrop = self.overlay.into();
        palette.tooltip_bg = self.elevated.into();
        palette.tooltip_fg = self.text_primary.into();
        palette
    }
}

/// Push RequestLab's palette into Ely and select the matching mode.
///
/// Called at start-up and on every theme change so Ely components (segmented
/// controls, buttons, icons) paint from exactly the same tokens.
pub fn sync_ely(mode: ThemeMode, cx: &mut gpui::App) {
    let theme = Theme::for_mode(mode);
    ely_gpui_component::theme::Theme::set_palette(mode.ely_mode(), Some(theme.ely_palette()), cx);
    ely_gpui_component::theme::Theme::set_mode_now(mode.ely_mode(), cx);
}

/// Token classes used by the JSON viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonToken {
    Key,
    String,
    Number,
    Boolean,
    Null,
    Punctuation,
}

pub const SANS_FAMILY: &str = "Inter";
pub const MONO_FAMILY: &str = "JetBrains Mono";

pub fn rgb(hex: u32) -> Rgba {
    gpui::rgb(hex)
}

pub fn rgba(hex: u32) -> Rgba {
    gpui::rgba(hex)
}

/// Return `color` with a different alpha channel.
pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// Convert a token to the colour type GPUI components take.
pub fn hsla(color: Rgba) -> Hsla {
    color.into()
}

/// Shared spacing scale: 4 / 8 / 12 / 16 / 24.
pub mod space {
    use gpui::{px, Pixels};

    pub fn xs() -> Pixels {
        px(4.0)
    }
    pub fn sm() -> Pixels {
        px(8.0)
    }
    pub fn md() -> Pixels {
        px(12.0)
    }
    pub fn lg() -> Pixels {
        px(16.0)
    }
    pub fn xl() -> Pixels {
        px(24.0)
    }
}

/// Shared type scale: 11 / 12 / 13 / 14 / 20 / 26.
pub mod text {
    use gpui::{px, Pixels};

    pub fn micro() -> Pixels {
        px(11.0)
    }
    pub fn small() -> Pixels {
        px(12.0)
    }
    pub fn body() -> Pixels {
        px(13.0)
    }
    pub fn title() -> Pixels {
        px(14.0)
    }
    pub fn heading() -> Pixels {
        px(20.0)
    }
    pub fn display() -> Pixels {
        px(26.0)
    }
}

/// Shared corner radii: 6 / 8 / 12.
pub mod radius {
    use gpui::{px, Pixels};

    pub fn sm() -> Pixels {
        px(6.0)
    }
    pub fn md() -> Pixels {
        px(8.0)
    }
    pub fn lg() -> Pixels {
        px(12.0)
    }
}

/// Window chrome constants.
pub const TITLE_BAR_HEIGHT: Pixels = px(44.0);
pub const SIDEBAR_WIDTH: Pixels = px(260.0);
pub const MIN_WINDOW_WIDTH: f32 = 940.0;
pub const MIN_WINDOW_HEIGHT: f32 = 600.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_modes_round_trip() {
        assert_eq!(ThemeMode::from_setting("light"), ThemeMode::Light);
        assert_eq!(ThemeMode::from_setting("dark"), ThemeMode::Dark);
        assert_eq!(ThemeMode::from_setting("nonsense"), ThemeMode::Dark);
        assert_eq!(ThemeMode::Light.as_setting(), "light");
        assert_eq!(ThemeMode::Light.ely_mode(), ElyMode::Light);
    }

    #[test]
    fn the_palettes_use_the_design_tokens() {
        let dark = Theme::dark();
        assert_eq!(dark.background, rgb(0x080B12));
        assert_eq!(dark.surface, rgb(0x121827));
        assert_eq!(dark.accent, rgb(0x2878FF));
        assert_eq!(dark.border, rgb(0x253249));
        assert_eq!(dark.text_primary, rgb(0xF1F5FC));

        let light = Theme::light();
        assert_eq!(light.background, rgb(0xF5F8FD));
        assert_eq!(light.accent, rgb(0x216BEB));
        assert_eq!(light.text_primary, rgb(0x142033));
        assert_ne!(dark.background, light.background);
    }

    #[test]
    fn palettes_cover_status_colors() {
        let dark = Theme::dark();
        assert_eq!(dark.status_color(Some(200)), dark.success);
        assert_eq!(dark.status_color(Some(302)), dark.info);
        assert_eq!(dark.status_color(Some(404)), dark.warning);
        assert_eq!(dark.status_color(Some(500)), dark.error);
        assert_eq!(dark.status_color(None), dark.text_muted);
    }

    #[test]
    fn methods_have_distinct_colors() {
        let theme = Theme::dark();
        assert_ne!(
            theme.method_color(HttpMethod::Get),
            theme.method_color(HttpMethod::Post)
        );
        assert_eq!(theme.method_color(HttpMethod::Delete), theme.error);
    }
}
