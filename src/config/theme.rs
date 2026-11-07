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
    TextTitle,
    TextHeading,
    TextBody,
    TextSmall,
    TextNum,
    TextMono,
    Space1,
    Space2,
    Space3,
    Space4,
    Space5,
    Space6,
    Space8,
    Row,
    RowLog,
    RowGroup,
    Control,
    Toolbar,
    SidebarWidth,
    InspectorWidth,
    Mark,
    RadiusControl,
    RadiusPanel,
    RadiusChip,
    DagNodeWidth,
    DagNodeHeight,
    DagColumnGap,
    DagNodeGap,
}

impl LengthToken {
    pub const ALL: [LengthToken; 28] = [
        LengthToken::TextTitle,
        LengthToken::TextHeading,
        LengthToken::TextBody,
        LengthToken::TextSmall,
        LengthToken::TextNum,
        LengthToken::TextMono,
        LengthToken::Space1,
        LengthToken::Space2,
        LengthToken::Space3,
        LengthToken::Space4,
        LengthToken::Space5,
        LengthToken::Space6,
        LengthToken::Space8,
        LengthToken::Row,
        LengthToken::RowLog,
        LengthToken::RowGroup,
        LengthToken::Control,
        LengthToken::Toolbar,
        LengthToken::SidebarWidth,
        LengthToken::InspectorWidth,
        LengthToken::Mark,
        LengthToken::RadiusControl,
        LengthToken::RadiusPanel,
        LengthToken::RadiusChip,
        LengthToken::DagNodeWidth,
        LengthToken::DagNodeHeight,
        LengthToken::DagColumnGap,
        LengthToken::DagNodeGap,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LengthToken::TextTitle => "text-title",
            LengthToken::TextHeading => "text-heading",
            LengthToken::TextBody => "text-body",
            LengthToken::TextSmall => "text-small",
            LengthToken::TextNum => "text-num",
            LengthToken::TextMono => "text-mono",
            LengthToken::Space1 => "space-1",
            LengthToken::Space2 => "space-2",
            LengthToken::Space3 => "space-3",
            LengthToken::Space4 => "space-4",
            LengthToken::Space5 => "space-5",
            LengthToken::Space6 => "space-6",
            LengthToken::Space8 => "space-8",
            LengthToken::Row => "row",
            LengthToken::RowLog => "row-log",
            LengthToken::RowGroup => "row-group",
            LengthToken::Control => "control",
            LengthToken::Toolbar => "toolbar",
            LengthToken::SidebarWidth => "sidebar-width",
            LengthToken::InspectorWidth => "inspector-width",
            LengthToken::Mark => "mark",
            LengthToken::RadiusControl => "radius-control",
            LengthToken::RadiusPanel => "radius-panel",
            LengthToken::RadiusChip => "radius-chip",
            LengthToken::DagNodeWidth => "dag-node-width",
            LengthToken::DagNodeHeight => "dag-node-height",
            LengthToken::DagColumnGap => "dag-column-gap",
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
    Sidebar,
    Sunken,
    Selected,
    NavSelected,
    Selection,
    Ink2,
    Ink3,
    Ink4,
    BorderStrong,
    Fail,
    FailText,
    FailSubtle,
    Warn,
    WarnText,
    WarnSubtle,
    Ok,
    OkText,
    OkSubtle,
    Run,
    RunText,
    RunSubtle,
    Wait,
    WaitText,
    WaitSubtle,
    Done,
    Human,
    HumanText,
    HumanSubtle,
    DiffAdd,
    DiffAddText,
    DiffDel,
    DiffDelText,
    CanvasDot,
}

impl ColorToken {
    pub const ALL: [ColorToken; 51] = [
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
        ColorToken::Sidebar,
        ColorToken::Sunken,
        ColorToken::Selected,
        ColorToken::NavSelected,
        ColorToken::Selection,
        ColorToken::Ink2,
        ColorToken::Ink3,
        ColorToken::Ink4,
        ColorToken::BorderStrong,
        ColorToken::Fail,
        ColorToken::FailText,
        ColorToken::FailSubtle,
        ColorToken::Warn,
        ColorToken::WarnText,
        ColorToken::WarnSubtle,
        ColorToken::Ok,
        ColorToken::OkText,
        ColorToken::OkSubtle,
        ColorToken::Run,
        ColorToken::RunText,
        ColorToken::RunSubtle,
        ColorToken::Wait,
        ColorToken::WaitText,
        ColorToken::WaitSubtle,
        ColorToken::Done,
        ColorToken::Human,
        ColorToken::HumanText,
        ColorToken::HumanSubtle,
        ColorToken::DiffAdd,
        ColorToken::DiffAddText,
        ColorToken::DiffDel,
        ColorToken::DiffDelText,
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
            ColorToken::Sidebar => "sidebar",
            ColorToken::Sunken => "sunken",
            ColorToken::Selected => "selected",
            ColorToken::NavSelected => "nav-selected",
            ColorToken::Selection => "selection",
            ColorToken::Ink2 => "ink-2",
            ColorToken::Ink3 => "ink-3",
            ColorToken::Ink4 => "ink-4",
            ColorToken::BorderStrong => "border-strong",
            ColorToken::Fail => "fail",
            ColorToken::FailText => "fail-text",
            ColorToken::FailSubtle => "fail-subtle",
            ColorToken::Warn => "warn",
            ColorToken::WarnText => "warn-text",
            ColorToken::WarnSubtle => "warn-subtle",
            ColorToken::Ok => "ok",
            ColorToken::OkText => "ok-text",
            ColorToken::OkSubtle => "ok-subtle",
            ColorToken::Run => "run",
            ColorToken::RunText => "run-text",
            ColorToken::RunSubtle => "run-subtle",
            ColorToken::Wait => "wait",
            ColorToken::WaitText => "wait-text",
            ColorToken::WaitSubtle => "wait-subtle",
            ColorToken::Done => "done",
            ColorToken::Human => "human",
            ColorToken::HumanText => "human-text",
            ColorToken::HumanSubtle => "human-subtle",
            ColorToken::DiffAdd => "diff-add",
            ColorToken::DiffAddText => "diff-add-text",
            ColorToken::DiffDel => "diff-del",
            ColorToken::DiffDelText => "diff-del-text",
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
/// with its key path (`cssVars.light.ok-text`).
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
        assert_eq!(theme.length(LengthToken::SidebarWidth), 224.0);
        assert_eq!(theme.length(LengthToken::DagNodeHeight), 52.0);
        assert_eq!(theme.color(ColorToken::Fail).light.packed(), 0xd93036);
        assert_eq!(theme.color(ColorToken::OkText).dark.packed(), 0x3cc79b);
        assert_eq!(
            theme.font(FontToken::Mono),
            "ui-monospace, \"SF Mono\", \"Cascadia Mono\", \"JetBrains Mono\", Menlo, Consolas, monospace"
        );
    }

    #[test]
    fn a_missing_token_names_the_file_and_the_token() {
        let text = edited(|json| {
            json["cssVars"]["light"]
                .as_object_mut()
                .unwrap()
                .remove("ok-text");
        });
        let error = error_of(&text);
        assert_eq!(error.file, "dashboard/theme.json");
        assert_eq!(error.path, "cssVars.light.ok-text");
        assert!(error.to_string().contains("ok-text"), "{error}");
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
            ("light", "fail", Value::from("red")),
            ("dark", "run", Value::from("#12345")),
            ("theme", "space-2", Value::from("8")),
            ("theme", "radius-panel", Value::from("-2px")),
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
