//! `dashboard/theme.json`: the dashboard's visual values, in shadcn's
//! `cssVars` layout. `theme` holds the fonts and the lengths (text sizes,
//! spacing, radii, row heights, the sidebar's width, the DAG's geometry),
//! `light` and `dark` the colors of each appearance. Every token is
//! required and no other is allowed; a color is `#rrggbb`, a length
//! `<number>px`, a font a CSS family list. `build.rs` includes this file
//! too, so the web build rejects a theme with the same messages the macOS
//! app shows.

use serde_json::{Map, Value};

use super::schema::{ConfigError, HexColor};

/// `dashboard/theme.json`, relative to the PM config directory.
pub const THEME_FILE: &str = "dashboard/theme.json";

/// A font family token (`cssVars.theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontToken {
    Sans,
    Mono,
}

impl FontToken {
    pub const ALL: [FontToken; 2] = [FontToken::Sans, FontToken::Mono];

    pub fn name(self) -> &'static str {
        match self {
            FontToken::Sans => "font-sans",
            FontToken::Mono => "font-mono",
        }
    }
}

/// A length token in points (`cssVars.theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthToken {
    Radius,
    RadiusControl,
    TextTitle,
    TextHeading,
    TextBody,
    TextDense,
    TextMeta,
    TextKpi,
    Space1,
    Space2,
    Space3,
    Space4,
    Space5,
    Space6,
    RowDense,
    RowAttention,
    SidebarWidth,
    DagNodeWidth,
    DagNodeHeight,
    DagLayerGap,
    DagNodeGap,
}

impl LengthToken {
    pub const ALL: [LengthToken; 21] = [
        LengthToken::Radius,
        LengthToken::RadiusControl,
        LengthToken::TextTitle,
        LengthToken::TextHeading,
        LengthToken::TextBody,
        LengthToken::TextDense,
        LengthToken::TextMeta,
        LengthToken::TextKpi,
        LengthToken::Space1,
        LengthToken::Space2,
        LengthToken::Space3,
        LengthToken::Space4,
        LengthToken::Space5,
        LengthToken::Space6,
        LengthToken::RowDense,
        LengthToken::RowAttention,
        LengthToken::SidebarWidth,
        LengthToken::DagNodeWidth,
        LengthToken::DagNodeHeight,
        LengthToken::DagLayerGap,
        LengthToken::DagNodeGap,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LengthToken::Radius => "radius",
            LengthToken::RadiusControl => "radius-control",
            LengthToken::TextTitle => "text-title",
            LengthToken::TextHeading => "text-heading",
            LengthToken::TextBody => "text-body",
            LengthToken::TextDense => "text-dense",
            LengthToken::TextMeta => "text-meta",
            LengthToken::TextKpi => "text-kpi",
            LengthToken::Space1 => "space-1",
            LengthToken::Space2 => "space-2",
            LengthToken::Space3 => "space-3",
            LengthToken::Space4 => "space-4",
            LengthToken::Space5 => "space-5",
            LengthToken::Space6 => "space-6",
            LengthToken::RowDense => "row-dense",
            LengthToken::RowAttention => "row-attention",
            LengthToken::SidebarWidth => "sidebar-width",
            LengthToken::DagNodeWidth => "dag-node-width",
            LengthToken::DagNodeHeight => "dag-node-height",
            LengthToken::DagLayerGap => "dag-layer-gap",
            LengthToken::DagNodeGap => "dag-node-gap",
        }
    }
}

/// A color token, with a value in `cssVars.light` and one in
/// `cssVars.dark`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorToken {
    Background,
    Foreground,
    Card,
    CardForeground,
    Popover,
    PopoverForeground,
    Primary,
    PrimaryForeground,
    Secondary,
    SecondaryForeground,
    Muted,
    MutedForeground,
    Accent,
    AccentForeground,
    Destructive,
    Border,
    Input,
    Ring,
    Ink2,
    BorderStrong,
    Selection,
    Critical,
    CriticalText,
    CriticalSubtle,
    Warning,
    WarningText,
    WarningSubtle,
    Good,
    GoodText,
    GoodSubtle,
    Active,
    ActiveText,
    ActiveSubtle,
    Neutral,
    NeutralText,
    NeutralSubtle,
    Edge,
    CanvasDot,
}

impl ColorToken {
    pub const ALL: [ColorToken; 38] = [
        ColorToken::Background,
        ColorToken::Foreground,
        ColorToken::Card,
        ColorToken::CardForeground,
        ColorToken::Popover,
        ColorToken::PopoverForeground,
        ColorToken::Primary,
        ColorToken::PrimaryForeground,
        ColorToken::Secondary,
        ColorToken::SecondaryForeground,
        ColorToken::Muted,
        ColorToken::MutedForeground,
        ColorToken::Accent,
        ColorToken::AccentForeground,
        ColorToken::Destructive,
        ColorToken::Border,
        ColorToken::Input,
        ColorToken::Ring,
        ColorToken::Ink2,
        ColorToken::BorderStrong,
        ColorToken::Selection,
        ColorToken::Critical,
        ColorToken::CriticalText,
        ColorToken::CriticalSubtle,
        ColorToken::Warning,
        ColorToken::WarningText,
        ColorToken::WarningSubtle,
        ColorToken::Good,
        ColorToken::GoodText,
        ColorToken::GoodSubtle,
        ColorToken::Active,
        ColorToken::ActiveText,
        ColorToken::ActiveSubtle,
        ColorToken::Neutral,
        ColorToken::NeutralText,
        ColorToken::NeutralSubtle,
        ColorToken::Edge,
        ColorToken::CanvasDot,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ColorToken::Background => "background",
            ColorToken::Foreground => "foreground",
            ColorToken::Card => "card",
            ColorToken::CardForeground => "card-foreground",
            ColorToken::Popover => "popover",
            ColorToken::PopoverForeground => "popover-foreground",
            ColorToken::Primary => "primary",
            ColorToken::PrimaryForeground => "primary-foreground",
            ColorToken::Secondary => "secondary",
            ColorToken::SecondaryForeground => "secondary-foreground",
            ColorToken::Muted => "muted",
            ColorToken::MutedForeground => "muted-foreground",
            ColorToken::Accent => "accent",
            ColorToken::AccentForeground => "accent-foreground",
            ColorToken::Destructive => "destructive",
            ColorToken::Border => "border",
            ColorToken::Input => "input",
            ColorToken::Ring => "ring",
            ColorToken::Ink2 => "ink-2",
            ColorToken::BorderStrong => "border-strong",
            ColorToken::Selection => "selection",
            ColorToken::Critical => "critical",
            ColorToken::CriticalText => "critical-text",
            ColorToken::CriticalSubtle => "critical-subtle",
            ColorToken::Warning => "warning",
            ColorToken::WarningText => "warning-text",
            ColorToken::WarningSubtle => "warning-subtle",
            ColorToken::Good => "good",
            ColorToken::GoodText => "good-text",
            ColorToken::GoodSubtle => "good-subtle",
            ColorToken::Active => "active",
            ColorToken::ActiveText => "active-text",
            ColorToken::ActiveSubtle => "active-subtle",
            ColorToken::Neutral => "neutral",
            ColorToken::NeutralText => "neutral-text",
            ColorToken::NeutralSubtle => "neutral-subtle",
            ColorToken::Edge => "edge",
            ColorToken::CanvasDot => "canvas-dot",
        }
    }
}

/// A color's value in each appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColor {
    pub light: HexColor,
    pub dark: HexColor,
}

/// `dashboard/theme.json`, parsed: every token's value, indexed as the
/// tokens' `ALL` lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    fonts: [String; FontToken::ALL.len()],
    lengths: [f32; LengthToken::ALL.len()],
    colors: [ThemeColor; ColorToken::ALL.len()],
}

impl Theme {
    /// The CSS family list of `token`.
    pub fn font(&self, token: FontToken) -> &str {
        &self.fonts[token as usize]
    }

    /// The length of `token`, in points.
    pub fn length(&self, token: LengthToken) -> f32 {
        self.lengths[token as usize]
    }

    /// The color of `token` in each appearance.
    pub fn color(&self, token: ColorToken) -> ThemeColor {
        self.colors[token as usize]
    }
}

/// Parses `dashboard/theme.json`: the first missing, unknown or bad token,
/// with its key path (`cssVars.light.good-text`).
pub fn parse(text: &str) -> Result<Theme, ConfigError> {
    let error = |path: String, message: String| ConfigError {
        file: THEME_FILE.to_owned(),
        path,
        message,
    };
    let root: Value =
        serde_json::from_str(text).map_err(|e| error(String::new(), format!("不是 JSON：{e}")))?;
    let root = object(&root, "", &["cssVars"]).map_err(|(p, m)| error(p, m))?;
    let vars = object(&root["cssVars"], "cssVars", &["theme", "light", "dark"])
        .map_err(|(p, m)| error(p, m))?;

    let theme_names: Vec<&str> = FontToken::ALL
        .iter()
        .map(|t| t.name())
        .chain(LengthToken::ALL.iter().map(|t| t.name()))
        .collect();
    let color_names: Vec<&str> = ColorToken::ALL.iter().map(|t| t.name()).collect();
    let theme =
        object(&vars["theme"], "cssVars.theme", &theme_names).map_err(|(p, m)| error(p, m))?;
    let light =
        object(&vars["light"], "cssVars.light", &color_names).map_err(|(p, m)| error(p, m))?;
    let dark = object(&vars["dark"], "cssVars.dark", &color_names).map_err(|(p, m)| error(p, m))?;

    let value = |section: &str, fields: &Map<String, Value>, name: &str| {
        let path = format!("cssVars.{section}.{name}");
        match &fields[name] {
            Value::String(text) => Ok((path, text.clone())),
            other => Err(error(path, format!("应为字符串，实为 {other}"))),
        }
    };
    let font = |token: FontToken| {
        let (path, text) = value("theme", theme, token.name())?;
        if text
            .split(',')
            .any(|family| family.trim().trim_matches(['"', '\'']).is_empty())
        {
            return Err(error(path, format!("{text:?} 不是 CSS 字体族列表")));
        }
        Ok(text)
    };
    let length = |token: LengthToken| {
        let (path, text) = value("theme", theme, token.name())?;
        text.strip_suffix("px")
            .and_then(|n| n.parse::<f32>().ok())
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or_else(|| error(path, format!("{text:?} 不是正的 <数字>px")))
    };
    let color = |section: &str, fields: &Map<String, Value>, token: ColorToken| {
        let (path, text) = value(section, fields, token.name())?;
        HexColor::try_from(text).map_err(|message| error(path, message))
    };

    let mut fonts: [String; FontToken::ALL.len()] = Default::default();
    for token in FontToken::ALL {
        fonts[token as usize] = font(token)?;
    }
    let mut lengths = [0.0; LengthToken::ALL.len()];
    for token in LengthToken::ALL {
        lengths[token as usize] = length(token)?;
    }
    let black = HexColor::try_from("#000000".to_owned()).expect("#000000 is a color");
    let mut colors = [ThemeColor {
        light: black,
        dark: black,
    }; ColorToken::ALL.len()];
    for token in ColorToken::ALL {
        colors[token as usize] = ThemeColor {
            light: color("light", light, token)?,
            dark: color("dark", dark, token)?,
        };
    }
    Ok(Theme {
        fonts,
        lengths,
        colors,
    })
}

/// `value` as an object with exactly the keys `names`: the first missing
/// one, else the first other one, is the error (key path, message).
fn object<'a>(
    value: &'a Value,
    path: &str,
    names: &[&str],
) -> Result<&'a Map<String, Value>, (String, String)> {
    let join = |name: &str| {
        if path.is_empty() {
            name.to_owned()
        } else {
            format!("{path}.{name}")
        }
    };
    let Value::Object(fields) = value else {
        return Err((path.to_owned(), format!("应为对象，实为 {value}")));
    };
    if let Some(missing) = names.iter().find(|name| !fields.contains_key(**name)) {
        return Err((join(missing), "缺少该 token".to_owned()));
    }
    if let Some(extra) = fields.keys().find(|key| !names.contains(&key.as_str())) {
        return Err((join(extra), "不认识的 token".to_owned()));
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../config.example/dashboard/theme.json");

    fn edited(edit: impl FnOnce(&mut Value)) -> String {
        let mut json: Value = serde_json::from_str(EXAMPLE).unwrap();
        edit(&mut json);
        json.to_string()
    }

    fn error_of(text: &str) -> ConfigError {
        parse(text).unwrap_err()
    }

    #[test]
    fn the_example_parses_with_every_token() {
        let theme = parse(EXAMPLE).unwrap();
        assert_eq!(theme.length(LengthToken::SidebarWidth), 220.0);
        assert_eq!(theme.length(LengthToken::DagNodeHeight), 84.0);
        assert_eq!(theme.color(ColorToken::Critical).light.packed(), 0xd03b3b);
        assert_eq!(theme.color(ColorToken::GoodText).dark.packed(), 0x3cc79b);
        assert_eq!(
            theme.font(FontToken::Mono),
            "\"SF Mono\", ui-monospace, Menlo, monospace"
        );
    }

    #[test]
    fn a_missing_token_names_the_file_and_the_token() {
        let text = edited(|json| {
            json["cssVars"]["light"]
                .as_object_mut()
                .unwrap()
                .remove("good-text");
        });
        let error = error_of(&text);
        assert_eq!(error.file, "dashboard/theme.json");
        assert_eq!(error.path, "cssVars.light.good-text");
        assert!(error.to_string().contains("good-text"), "{error}");
        let text = edited(|json| {
            json["cssVars"]["theme"]
                .as_object_mut()
                .unwrap()
                .remove("dag-node-gap");
        });
        assert_eq!(error_of(&text).path, "cssVars.theme.dag-node-gap");
    }

    #[test]
    fn an_extra_token_is_an_error() {
        let text = edited(|json| {
            json["cssVars"]["dark"]["chart-1"] = Value::from("#123456");
        });
        let error = error_of(&text);
        assert_eq!(error.path, "cssVars.dark.chart-1");
        assert_eq!(error.message, "不认识的 token");
        let text = edited(|json| json["shadows"] = Value::from(1));
        assert_eq!(error_of(&text).path, "shadows");
    }

    #[test]
    fn a_value_that_does_not_parse_is_an_error() {
        for (section, name, value) in [
            ("light", "critical", Value::from("red")),
            ("dark", "edge", Value::from("#12345")),
            ("theme", "space-2", Value::from("8")),
            ("theme", "radius", Value::from("-2px")),
            ("theme", "text-body", Value::from(14)),
            ("theme", "font-mono", Value::from("Menlo,,")),
        ] {
            let text = edited(|json| json["cssVars"][section][name] = value.clone());
            let error = error_of(&text);
            assert_eq!(error.path, format!("cssVars.{section}.{name}"), "{value}");
        }
        assert_eq!(error_of("{").path, "");
    }
}
