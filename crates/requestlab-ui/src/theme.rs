//! Design tokens for RequestLab.
//!
//! Every colour, radius and size used by the interface comes from here. The two
//! palettes follow the product spec: charcoal dark and newspaper light.

use gpui::{px, Pixels, Rgba};
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
}

/// A resolved palette plus the shared geometry values.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,

    pub background: Rgba,
    pub sidebar: Rgba,
    pub surface: Rgba,
    pub elevated: Rgba,
    pub overlay: Rgba,
    pub hover: Rgba,
    pub active: Rgba,

    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,

    pub border: Rgba,
    pub border_strong: Rgba,
    pub accent: Rgba,
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

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            background: rgb(0x17191D),
            sidebar: rgb(0x202329),
            surface: rgb(0x282C33),
            elevated: rgb(0x30353E),
            overlay: rgba(0x0E1013CC),
            hover: rgba(0xFFFFFF0D),
            active: rgba(0x8CA9FF1F),
            text_primary: rgb(0xF1F2F4),
            text_secondary: rgb(0xA0A5AE),
            text_muted: rgb(0x6C717B),
            border: rgb(0x373C45),
            border_strong: rgb(0x4A505B),
            accent: rgb(0x8CA9FF),
            accent_muted: rgba(0x8CA9FF33),
            success: rgb(0x62C995),
            warning: rgb(0xE8B86D),
            error: rgb(0xE47777),
            info: rgb(0x7FC2E0),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            background: rgb(0xF5F6F8),
            sidebar: rgb(0xEBEDF0),
            surface: rgb(0xFFFFFF),
            elevated: rgb(0xFFFFFF),
            overlay: rgba(0x2428304D),
            hover: rgba(0x2428300D),
            active: rgba(0x416DD91F),
            text_primary: rgb(0x242830),
            text_secondary: rgb(0x707783),
            text_muted: rgb(0x98A0AB),
            border: rgb(0xDDE0E5),
            border_strong: rgb(0xC3C8D0),
            accent: rgb(0x416DD9),
            accent_muted: rgba(0x416DD926),
            success: rgb(0x2E9E68),
            warning: rgb(0xB07C22),
            error: rgb(0xC24B4B),
            info: rgb(0x2A7C9B),
        }
    }

    /// Colour for an HTTP method chip.
    pub fn method_color(&self, method: HttpMethod) -> Rgba {
        match method {
            HttpMethod::Get => self.success,
            HttpMethod::Post => self.accent,
            HttpMethod::Put => self.warning,
            HttpMethod::Patch => self.info,
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
            JsonToken::Key => self.accent,
            JsonToken::String => self.success,
            JsonToken::Number => self.warning,
            JsonToken::Boolean => self.info,
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

pub const SANS_FAMILY: &str = "Segoe UI";
pub const MONO_FAMILY: &str = "Cascadia Mono";

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

/// Shared geometry tokens.
pub mod space {
    use gpui::{px, Pixels};

    pub fn xs() -> Pixels {
        px(4.0)
    }
    pub fn sm() -> Pixels {
        px(6.0)
    }
    pub fn md() -> Pixels {
        px(10.0)
    }
    pub fn lg() -> Pixels {
        px(14.0)
    }
    pub fn xl() -> Pixels {
        px(20.0)
    }
}

/// Shared type scale.
pub mod text {
    use gpui::{px, Pixels};

    pub fn micro() -> Pixels {
        px(10.5)
    }
    pub fn small() -> Pixels {
        px(11.5)
    }
    pub fn body() -> Pixels {
        px(12.5)
    }
    pub fn title() -> Pixels {
        px(14.0)
    }
    pub fn heading() -> Pixels {
        px(16.0)
    }
}

/// Shared corner radii.
pub mod radius {
    use gpui::{px, Pixels};

    pub fn sm() -> Pixels {
        px(4.0)
    }
    pub fn md() -> Pixels {
        px(6.0)
    }
    pub fn lg() -> Pixels {
        px(9.0)
    }
}

/// Window chrome constants.
pub const TITLE_BAR_HEIGHT: Pixels = px(38.0);
pub const SIDEBAR_WIDTH: Pixels = px(258.0);
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
    }

    #[test]
    fn palettes_differ_and_cover_status_colors() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_ne!(dark.background, light.background);
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
