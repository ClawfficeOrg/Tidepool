//! The console's identity: the Tidepool banner, the pool cell sprite and the
//! Clawffice palette.
//!
//! The art and the colours are not literals in this file. `assets/design/`
//! holds a pinned copy of the `clawffice-design` sources the console renders:
//! `banners/tidepool/tidepool-banner-static.ans` (a true-colour terminal
//! banner), its plain-text twin, `tokens/core.json`, `tokens/apps/tidepool.json`,
//! `tokens/cells.json` and all five palette files. [`SOURCE.json`] records the
//! design commit and the sha256 of every file, and
//! `design_source_matches_the_checkout` compares the copies to a sibling
//! checkout when one is present.
//!
//! The banner is parsed from its ANSI source at run time rather than copied
//! into Rust as a string, so the art in the design repository is the art the
//! console shows. Colours are RGB values from the palette files, never
//! `Color::Yellow` or another terminal default: a palette's role tokens drive
//! every role, and the Tidepool product hue (`tide`) drives focus and borders
//! as the design system requires. A terminal that cannot show colour gets the
//! same art with every style stripped, and a terminal narrower than the art
//! gets a cropped banner and, below that, a one-line wordmark instead of a
//! panic.

use std::cell::Cell;
use std::sync::OnceLock;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use serde::Deserialize;

/// The banner source, exactly as `clawffice-design` writes it.
const BANNER_ANSI: &str =
    include_str!("../../assets/design/banners/tidepool/tidepool-banner-static.ans");
/// The plain-text twin of the banner, used by the tests to prove the ANSI
/// parser reproduces the design's own text.
#[cfg(test)]
const BANNER_TEXT: &str =
    include_str!("../../assets/design/banners/tidepool/tidepool-banner-static.txt");
const SOURCE_JSON: &str = include_str!("../../assets/design/SOURCE.json");
const CORE_JSON: &str = include_str!("../../assets/design/tokens/core.json");
const APP_JSON: &str = include_str!("../../assets/design/tokens/apps/tidepool.json");
const CELLS_JSON: &str = include_str!("../../assets/design/tokens/cells.json");
const PALETTE_JSON: &[(&str, &str)] = &[
    (
        "biolume",
        include_str!("../../assets/design/tokens/palettes/biolume.json"),
    ),
    (
        "boiled",
        include_str!("../../assets/design/tokens/palettes/boiled.json"),
    ),
    (
        "deep-tide",
        include_str!("../../assets/design/tokens/palettes/deep-tide.json"),
    ),
    (
        "phosphor",
        include_str!("../../assets/design/tokens/palettes/phosphor.json"),
    ),
    (
        "sea-glass",
        include_str!("../../assets/design/tokens/palettes/sea-glass.json"),
    ),
];

/// The console's own version, reported on the about screen.
pub const CONSOLE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The contract version the console's SDK was generated from, reported on the
/// about screen.
pub const CONTRACT_VERSION: &str = tidepool_client::CONTRACT_VERSION;

/// Below this width the banner is cropped; below [`COMPACT_BELOW`] it becomes
/// the one-line wordmark.
const BANNER_WIDTH: u16 = 80;
/// The width at which the full art stops being legible and the wordmark takes
/// over.
const COMPACT_BELOW: u16 = 40;

/// One selectable Clawffice palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteName {
    /// Deep-sea ink, phosphor lime primary, claw orange second. The default.
    Biolume,
    /// Aubergine ground, lobster red, butter yellow.
    Boiled,
    /// Navy ground, claw orange, spectre cyan.
    DeepTide,
    /// Near-black ground, phosphor lime, lobster orange.
    Phosphor,
    /// The light scheme: frosted grey-green ground, teal ink.
    SeaGlass,
}

impl PaletteName {
    /// Every palette, in the order the design system lists them.
    pub const ALL: [PaletteName; 5] = [
        PaletteName::Biolume,
        PaletteName::Boiled,
        PaletteName::DeepTide,
        PaletteName::Phosphor,
        PaletteName::SeaGlass,
    ];

    /// The palette's file name and `TIDEPOOL_PALETTE` value.
    pub fn as_str(self) -> &'static str {
        match self {
            PaletteName::Biolume => "biolume",
            PaletteName::Boiled => "boiled",
            PaletteName::DeepTide => "deep-tide",
            PaletteName::Phosphor => "phosphor",
            PaletteName::SeaGlass => "sea-glass",
        }
    }

    /// The palette's display name, as the design file spells it.
    pub fn title(self) -> &'static str {
        match self {
            PaletteName::Biolume => "Biolume",
            PaletteName::Boiled => "Boiled",
            PaletteName::DeepTide => "Deep Tide",
            PaletteName::Phosphor => "Phosphor",
            PaletteName::SeaGlass => "Sea Glass",
        }
    }

    /// Parse a palette name, case-insensitively.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|palette| palette.as_str() == name)
    }
}

/// The role a colour plays on screen.
///
/// Components ask for a role rather than a terminal colour, so changing the
/// palette changes every screen and a component cannot invent its own colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleRole {
    /// The window ground.
    Background,
    /// A raised panel ground.
    Surface,
    /// The terminal ground, used behind raw output.
    TerminalBackground,
    /// A border that must stand out.
    BorderStrong,
    /// Body text.
    Text,
    /// Secondary or disabled text.
    Muted,
    /// Emphasised text.
    Strong,
    /// The palette's primary hue.
    Primary,
    /// The palette's second hue.
    Second,
    /// The Tidepool product hue: focus, selection and borders.
    Focus,
    /// A successful state.
    Success,
    /// A state that needs attention.
    Warning,
    /// A failed state.
    Danger,
    /// Informational state.
    Info,
}

impl StyleRole {
    /// Every role, in a stable order for tests.
    #[cfg(test)]
    pub const ALL: [StyleRole; 14] = [
        StyleRole::Background,
        StyleRole::Surface,
        StyleRole::TerminalBackground,
        StyleRole::BorderStrong,
        StyleRole::Text,
        StyleRole::Muted,
        StyleRole::Strong,
        StyleRole::Primary,
        StyleRole::Second,
        StyleRole::Focus,
        StyleRole::Success,
        StyleRole::Warning,
        StyleRole::Danger,
        StyleRole::Info,
    ];
}

/// A palette resolved to ratatui colours.
///
/// `colour` is false on a terminal that cannot show colour; every role then
/// resolves to [`Color::Reset`] and every style is empty, which keeps the
/// console usable instead of pretending it can paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    palette: PaletteName,
    colour: bool,
    background: Color,
    surface: Color,
    terminal_background: Color,
    border_strong: Color,
    text: Color,
    muted: Color,
    strong: Color,
    primary: Color,
    second: Color,
    focus: Color,
    success: Color,
    warning: Color,
    danger: Color,
    info: Color,
    claw: Color,
}

impl Theme {
    /// Build a theme from a palette file.
    ///
    /// A missing or malformed asset yields a plain theme rather than a panic;
    /// the tests assert every shipped palette loads, so that fallback is only
    /// reachable from a broken build.
    pub fn new(palette: PaletteName, colour: bool) -> Self {
        let Some(design) = design() else {
            return Self::plain(palette, colour);
        };
        let Some(file) = design
            .palettes
            .iter()
            .find(|(name, _)| *name == palette.as_str())
            .map(|(_, file)| file)
        else {
            return Self::plain(palette, colour);
        };
        let scheme = if file.scheme == "light" {
            "light"
        } else {
            "dark"
        };
        let product = |name: &str| {
            design
                .core
                .color
                .product
                .get(name)
                .map(|hue| match scheme {
                    "light" => parse_hex(&hue.light),
                    _ => parse_hex(&hue.dark),
                })
                .filter(|colour| *colour != Color::Reset)
        };
        let role = |value: &str| parse_hex(value);

        Self {
            palette,
            colour,
            background: role(&file.role.window_background),
            surface: role(&file.role.surface),
            terminal_background: role(&file.role.terminal_background),
            border_strong: role(&file.role.border_strong),
            text: role(&file.role.text),
            muted: role(&file.role.text_muted),
            strong: role(&file.role.text_strong),
            primary: role(&file.role.primary),
            second: role(&file.role.second),
            focus: product("tide").unwrap_or_else(|| role(&file.role.primary)),
            success: role(&file.status_color.success),
            warning: role(&file.status_color.warning),
            danger: role(&file.status_color.danger),
            info: role(&file.status_color.info),
            claw: product("claw").unwrap_or_else(|| role(&file.role.second)),
        }
    }

    /// The theme this terminal can actually show.
    ///
    /// `TIDEPOOL_PALETTE` chooses a palette (case-insensitively); no value, or
    /// an unknown one, keeps the design system's default. `NO_COLOR` and
    /// `TERM=dumb` turn colour off, which is the standard a terminal user
    /// already expects.
    pub fn from_environment() -> Self {
        let name = std::env::var("TIDEPOOL_PALETTE")
            .ok()
            .as_deref()
            .and_then(PaletteName::from_name)
            .unwrap_or_else(default_palette);
        let no_color = std::env::var("NO_COLOR").ok();
        let term = std::env::var("TERM").ok();
        Self::new(name, colour_supported(no_color.as_deref(), term.as_deref()))
    }

    /// The palette this theme was built from.
    pub fn palette(&self) -> PaletteName {
        self.palette
    }

    /// Whether colour is painted at all.
    pub fn colour(&self) -> bool {
        self.colour
    }

    /// The colour a role resolves to.
    pub fn colour_of(&self, role: StyleRole) -> Color {
        if !self.colour {
            return Color::Reset;
        }
        match role {
            StyleRole::Background => self.background,
            StyleRole::Surface => self.surface,
            StyleRole::TerminalBackground => self.terminal_background,
            StyleRole::BorderStrong => self.border_strong,
            StyleRole::Text => self.text,
            StyleRole::Muted => self.muted,
            StyleRole::Strong => self.strong,
            StyleRole::Primary => self.primary,
            StyleRole::Second => self.second,
            StyleRole::Focus => self.focus,
            StyleRole::Success => self.success,
            StyleRole::Warning => self.warning,
            StyleRole::Danger => self.danger,
            StyleRole::Info => self.info,
        }
    }

    /// The style a role resolves to.
    ///
    /// Background roles paint a background; text roles paint a foreground.
    /// With colour off the style is empty, so the terminal's own colours and
    /// its emulator's contrast setting win.
    pub fn style(&self, role: StyleRole) -> Style {
        if !self.colour {
            return Style::new();
        }
        match role {
            StyleRole::Background => Style::new().fg(self.text).bg(self.background),
            StyleRole::Surface => Style::new().bg(self.surface),
            StyleRole::TerminalBackground => Style::new().bg(self.terminal_background),
            _ => Style::new().fg(self.colour_of(role)),
        }
    }

    /// The style for the whole frame: body text on the palette ground.
    pub fn base(&self) -> Style {
        self.style(StyleRole::Background)
    }

    /// A plain theme with no colours, for the asset-failure path and tests.
    fn plain(palette: PaletteName, colour: bool) -> Self {
        Self {
            palette,
            colour,
            background: Color::Reset,
            surface: Color::Reset,
            terminal_background: Color::Reset,
            border_strong: Color::Reset,
            text: Color::Reset,
            muted: Color::Reset,
            strong: Color::Reset,
            primary: Color::Reset,
            second: Color::Reset,
            focus: Color::Reset,
            success: Color::Reset,
            warning: Color::Reset,
            danger: Color::Reset,
            info: Color::Reset,
            claw: Color::Reset,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(PaletteName::Biolume, true)
    }
}

thread_local! {
    /// The theme the renderers use, set for the duration of one frame. A
    /// thread-local keeps it out of every helper signature; `with_theme`
    /// restores the previous value even if drawing panics.
    static ACTIVE: Cell<Option<Theme>> = const { Cell::new(None) };
}

/// Run `draw` with `theme` active, restoring the previous theme afterwards.
pub fn with_theme<R>(theme: Theme, draw: impl FnOnce() -> R) -> R {
    struct Restore(Option<Theme>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(self.0));
        }
    }
    let previous = ACTIVE.with(|active| active.replace(Some(theme)));
    let _restore = Restore(previous);
    draw()
}

/// The active theme.
pub fn active_theme() -> Theme {
    ACTIVE.with(Cell::get).unwrap_or_default()
}

/// The active theme's colour for a role.
pub fn colour(role: StyleRole) -> Color {
    active_theme().colour_of(role)
}

/// The active theme's style for a role.
pub fn style(role: StyleRole) -> Style {
    active_theme().style(role)
}

/// Whether a terminal should be given colour.
///
/// The arguments are split out so the decision is testable without mutating
/// the test process's environment: `no_color` is `NO_COLOR` when set, `term` is
/// `TERM` when set.
pub fn colour_supported(no_color: Option<&str>, term: Option<&str>) -> bool {
    if no_color.is_some_and(|value| !value.trim().is_empty()) {
        return false;
    }
    !term.is_some_and(|value| value.eq_ignore_ascii_case("dumb"))
}

/// Draw the startup banner, centred in `area`.
///
/// Returns the number of rows the banner used, which the splash uses to place
/// its version line.
pub fn render_banner(theme: &Theme, frame: &mut Frame, area: Rect) -> u16 {
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    let (lines, width) = banner_lines(theme, area.width);
    let height = (lines.len() as u16).min(area.height);
    let width = width.min(area.width);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height,
    };
    frame.render_widget(Paragraph::new(lines), rect);
    height
}

/// Draw the startup splash: banner, versions and the key hint.
pub fn render_splash(theme: &Theme, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    fill(theme, frame, area);

    let banner_height = if area.width >= COMPACT_BELOW && area.height >= 12 {
        (area.height.saturating_sub(5)).min(banner_height())
    } else {
        1
    };
    let used = banner_height.saturating_add(4);
    let top = area.y + area.height.saturating_sub(used) / 2;
    let banner_area = Rect {
        x: area.x,
        y: top,
        width: area.width,
        height: banner_height.min(area.height),
    };
    let drawn = render_banner(theme, frame, banner_area);

    let info_top = top.saturating_add(drawn);
    let info_area = Rect {
        x: area.x,
        y: info_top,
        width: area.width,
        height: area.height.saturating_sub(info_top.saturating_sub(area.y)),
    };
    let info = vec![
        Line::from(Span::styled(
            format!("console {CONSOLE_VERSION}  ·  contract {CONTRACT_VERSION}"),
            theme.style(StyleRole::Muted),
        )),
        Line::from(Span::styled(
            format!(
                "{} palette  ·  {} design",
                theme.palette().title(),
                design_revision()
            ),
            theme.style(StyleRole::Muted),
        )),
        Line::from(Span::styled(
            "Press any key to begin.",
            theme.style(StyleRole::Strong),
        )),
    ];
    frame.render_widget(
        Paragraph::new(info)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        info_area,
    );
}

/// Draw the about screen: banner, cell sprite, versions and the design source.
///
/// The full layout puts the sprite beside the facts when there is room; a
/// narrow terminal stacks the facts first because versions must remain
/// readable, and an even narrower one keeps only the compact wordmark.
pub fn render_about(
    theme: &Theme,
    pool: Option<&str>,
    member: Option<&str>,
    frame: &mut Frame,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    fill(theme, frame, area);

    let banner_height = if area.width >= COMPACT_BELOW && area.height >= 20 {
        banner_height().min(area.height.saturating_sub(8))
    } else {
        1
    };
    let banner_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: banner_height.min(area.height),
    };
    let drawn = render_banner(theme, frame, banner_area);
    let consumed = drawn.saturating_add(1).min(area.height);
    let rest = Rect {
        x: area.x,
        y: area.y + consumed,
        width: area.width,
        height: area.height.saturating_sub(consumed),
    };
    if rest.height == 0 || rest.width == 0 {
        return;
    }

    if rest.width >= 72 && rest.height >= 8 {
        let [sprite_area, facts_area] =
            Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(rest);
        draw_sprite(theme, frame, sprite_area);
        draw_about_facts(theme, pool, member, frame, facts_area);
    } else {
        let facts_height = 8.min(rest.height);
        let [facts_area, sprite_area] =
            Layout::vertical([Constraint::Length(facts_height), Constraint::Min(0)]).areas(rest);
        draw_about_facts(theme, pool, member, frame, facts_area);
        if sprite_area.height >= 6 {
            draw_sprite(theme, frame, sprite_area);
        }
    }
}

fn fill(theme: &Theme, frame: &mut Frame, area: Rect) {
    frame.render_widget(Block::new().style(theme.base()), area);
}

fn draw_about_facts(
    theme: &Theme,
    pool: Option<&str>,
    member: Option<&str>,
    frame: &mut Frame,
    area: Rect,
) {
    let app = design_name();
    let facts = vec![
        Line::from(Span::styled(
            format!("About {app}"),
            theme.style(StyleRole::Primary).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        fact(theme, "console", CONSOLE_VERSION),
        fact(theme, "contract", CONTRACT_VERSION),
        fact(
            theme,
            "palette",
            &format!(
                "{} ({})",
                theme.palette().title(),
                if theme.colour() {
                    "colour"
                } else {
                    "no colour"
                }
            ),
        ),
        fact(
            theme,
            "design",
            &format!("clawffice-design {}", design_revision()),
        ),
        fact(theme, "pool", pool.unwrap_or("none configured")),
        fact(theme, "member", member.unwrap_or("not signed in")),
        Line::from(""),
        Line::from(Span::styled(
            "Esc or a closes this screen.",
            theme.style(StyleRole::Muted),
        )),
    ];
    frame.render_widget(Paragraph::new(facts).wrap(Wrap { trim: true }), area);
}

fn fact(theme: &Theme, label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), theme.style(StyleRole::Muted)),
        Span::styled(value.to_owned(), theme.style(StyleRole::Text)),
    ])
}

/// Draw the Tidepool cell sprite, `██` per pixel so it stays square.
fn draw_sprite(theme: &Theme, frame: &mut Frame, area: Rect) {
    let lines = sprite_lines(theme);
    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .min(usize::from(area.width)) as u16;
    let height = (lines.len() as u16).min(area.height);
    let rect = Rect {
        x: area.x,
        y: area.y,
        width,
        height,
    };
    frame.render_widget(Paragraph::new(lines), rect);
}

fn sprite_lines(theme: &Theme) -> Vec<Line<'static>> {
    let Some(rows) = design().map(|design| design.cells.sprites.pool.rows.as_slice()) else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| {
            let mut spans = Vec::new();
            for glyph in row.chars() {
                if glyph == '.' {
                    spans.push(Span::raw("  "));
                    continue;
                }
                if theme.colour() {
                    spans.push(Span::styled(
                        "██",
                        Style::new().fg(sprite_colour(theme, glyph)),
                    ));
                } else {
                    spans.push(Span::raw(format!("{glyph} ")));
                }
            }
            Line::from(spans)
        })
        .collect()
}

fn sprite_colour(theme: &Theme, glyph: char) -> Color {
    match glyph {
        '#' => theme.colour_of(StyleRole::Focus),
        'o' => theme.claw,
        'w' => theme.colour_of(StyleRole::Strong),
        'k' => theme.colour_of(StyleRole::TerminalBackground),
        _ => theme.colour_of(StyleRole::Muted),
    }
}

fn banner_height() -> u16 {
    parse_banner().len() as u16
}

fn banner_lines(theme: &Theme, max_width: u16) -> (Vec<Line<'static>>, u16) {
    if max_width >= BANNER_WIDTH {
        (styled_banner_lines(theme, BANNER_WIDTH), BANNER_WIDTH)
    } else if max_width >= COMPACT_BELOW {
        (styled_banner_lines(theme, max_width), max_width)
    } else {
        (compact_lines(theme), max_width)
    }
}

/// The banner cropped to `width` columns, with the design's colours or none.
fn styled_banner_lines(theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let limit = usize::from(width);
    parse_banner()
        .iter()
        .map(|line| {
            let mut spans = Vec::new();
            let mut remaining = limit;
            for segment in &line.segments {
                if remaining == 0 {
                    break;
                }
                let text: String = segment.text.chars().take(remaining).collect::<String>();
                remaining = remaining.saturating_sub(text.chars().count());
                if text.is_empty() {
                    continue;
                }
                let style = if theme.colour() {
                    segment.style.to_style()
                } else {
                    Style::new()
                };
                spans.push(Span::styled(text, style));
            }
            Line::from(spans)
        })
        .collect()
}

/// A one-line wordmark for terminals too narrow for the art.
///
/// The wordmark and the tagline come from the design sources, not from this
/// file: `tokens/apps/tidepool.json` names the wordmark and the last banner
/// line is the tagline.
fn compact_lines(theme: &Theme) -> Vec<Line<'static>> {
    let wordmark = design()
        .map(|design| design.app.logo.wordmark.clone())
        .unwrap_or_else(|| "tidepool".to_owned());
    let tagline = banner_tagline();
    let mut spans = vec![Span::styled(
        wordmark,
        theme.style(StyleRole::Focus).add_modifier(Modifier::BOLD),
    )];
    if !tagline.is_empty() {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(tagline, theme.style(StyleRole::Muted)));
    }
    vec![Line::from(spans)]
}

fn banner_tagline() -> String {
    parse_banner()
        .last()
        .map(|line| {
            line.segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .unwrap_or_default()
}

/// The design commit the vendored files came from.
fn design_revision() -> String {
    design()
        .map(|design| design.source.source.commit.clone())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn design_name() -> String {
    design()
        .map(|design| design.app.name.clone())
        .unwrap_or_else(|| "Tidepool".to_owned())
}

fn default_palette() -> PaletteName {
    design()
        .and_then(|design| PaletteName::from_name(&design.core.palette.default))
        .unwrap_or(PaletteName::Biolume)
}

// --- design sources -------------------------------------------------------

#[derive(Debug)]
struct Design {
    source: SourceFile,
    core: CoreTokens,
    app: AppTokens,
    cells: CellTokens,
    palettes: Vec<(&'static str, PaletteTokens)>,
}

#[derive(Debug, Deserialize)]
struct SourceFile {
    source: SourceInfo,
}

#[derive(Debug, Deserialize)]
struct SourceInfo {
    commit: String,
}

#[derive(Debug, Deserialize)]
struct CoreTokens {
    palette: CorePalette,
    color: CoreColor,
}

#[derive(Debug, Deserialize)]
struct CorePalette {
    default: String,
}

#[derive(Debug, Deserialize)]
struct CoreColor {
    product: std::collections::BTreeMap<String, ProductHue>,
}

#[derive(Debug, Deserialize)]
struct ProductHue {
    dark: String,
    light: String,
}

#[derive(Debug, Deserialize)]
struct AppTokens {
    name: String,
    logo: AppLogo,
}

#[derive(Debug, Deserialize)]
struct AppLogo {
    wordmark: String,
}

#[derive(Debug, Deserialize)]
struct CellTokens {
    sprites: Sprites,
}

#[derive(Debug, Deserialize)]
struct Sprites {
    pool: Sprite,
}

#[derive(Debug, Deserialize)]
struct Sprite {
    rows: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PaletteTokens {
    scheme: String,
    role: Roles,
    #[serde(rename = "status-color")]
    status_color: StatusColors,
}

#[derive(Debug, Deserialize)]
struct Roles {
    #[serde(rename = "window-background")]
    window_background: String,
    surface: String,
    #[serde(rename = "terminal-background")]
    terminal_background: String,
    #[serde(rename = "border-strong")]
    border_strong: String,
    text: String,
    #[serde(rename = "text-muted")]
    text_muted: String,
    #[serde(rename = "text-strong")]
    text_strong: String,
    primary: String,
    second: String,
}

#[derive(Debug, Deserialize)]
struct StatusColors {
    success: String,
    warning: String,
    danger: String,
    info: String,
}

fn design() -> Option<&'static Design> {
    static DESIGN: OnceLock<Option<Design>> = OnceLock::new();
    DESIGN.get_or_init(load_design).as_ref()
}

fn load_design() -> Option<Design> {
    let source: SourceFile = serde_json::from_str(SOURCE_JSON).ok()?;
    let core: CoreTokens = serde_json::from_str(CORE_JSON).ok()?;
    let app: AppTokens = serde_json::from_str(APP_JSON).ok()?;
    let cells: CellTokens = serde_json::from_str(CELLS_JSON).ok()?;
    let mut palettes = Vec::with_capacity(PALETTE_JSON.len());
    for (name, json) in PALETTE_JSON {
        palettes.push((*name, serde_json::from_str::<PaletteTokens>(json).ok()?));
    }
    Some(Design {
        source,
        core,
        app,
        cells,
        palettes,
    })
}

fn parse_hex(value: &str) -> Color {
    let Some(hex) = value.strip_prefix('#') else {
        return Color::Reset;
    };
    if hex.len() != 6 || !hex.is_ascii() {
        return Color::Reset;
    }
    let channel = |range: std::ops::Range<usize>| -> Option<u8> {
        u8::from_str_radix(hex.get(range)?, 16).ok()
    };
    match (channel(0..2), channel(2..4), channel(4..6)) {
        (Some(red), Some(green), Some(blue)) => Color::Rgb(red, green, blue),
        _ => Color::Reset,
    }
}

// --- banner parsing -------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct BannerStyle {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
}

impl BannerStyle {
    fn to_style(self) -> Style {
        let mut style = Style::new();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

#[derive(Debug, Clone, Default)]
struct BannerSegment {
    text: String,
    style: BannerStyle,
}

#[derive(Debug, Clone, Default)]
struct BannerLine {
    segments: Vec<BannerSegment>,
}

impl BannerLine {
    #[cfg(test)]
    fn text(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect()
    }
}

fn parse_banner() -> &'static [BannerLine] {
    static BANNER: OnceLock<Vec<BannerLine>> = OnceLock::new();
    BANNER.get_or_init(|| parse_ansi(BANNER_ANSI)).as_slice()
}

/// Split an ANSI true-colour line into styled segments.
///
/// Only SGR sequences are interpreted, which is all the design banners use:
/// reset, bold, default foreground/background and 24-bit `38;2` / `48;2`
/// colours. An unknown parameter is ignored rather than treated as text, so a
/// newer banner still renders instead of printing escape codes.
fn parse_ansi(source: &str) -> Vec<BannerLine> {
    let mut lines = Vec::new();
    for raw in source.lines() {
        let raw = raw.trim_end_matches('\r');
        let mut segments = Vec::new();
        let mut style = BannerStyle::default();
        let mut text = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(character) = chars.next() {
            if character != '\u{1b}' {
                text.push(character);
                continue;
            }
            if chars.peek() != Some(&'[') {
                continue;
            }
            chars.next();
            let mut params = String::new();
            let mut terminator = None;
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    terminator = Some(next);
                    break;
                }
                params.push(next);
            }
            if terminator != Some('m') {
                continue;
            }
            if !text.is_empty() {
                segments.push(BannerSegment {
                    text: std::mem::take(&mut text),
                    style,
                });
            }
            apply_sgr(&params, &mut style);
        }
        if !text.is_empty() {
            segments.push(BannerSegment { text, style });
        }
        lines.push(BannerLine { segments });
    }
    lines
}

fn apply_sgr(params: &str, style: &mut BannerStyle) {
    let values: Vec<u16> = if params.trim().is_empty() {
        vec![0]
    } else {
        params
            .split(';')
            .map(|value| value.parse().unwrap_or(0))
            .collect()
    };
    let mut index = 0;
    while index < values.len() {
        match values[index] {
            0 => *style = BannerStyle::default(),
            1 => style.bold = true,
            22 => style.bold = false,
            38 => {
                if values.get(index + 1) == Some(&2)
                    && let (Some(red), Some(green), Some(blue)) = (
                        values.get(index + 2),
                        values.get(index + 3),
                        values.get(index + 4),
                    )
                {
                    style.fg = Some(Color::Rgb(
                        u8::try_from(*red).unwrap_or(0),
                        u8::try_from(*green).unwrap_or(0),
                        u8::try_from(*blue).unwrap_or(0),
                    ));
                    index = index.saturating_add(4);
                }
            }
            48 => {
                if values.get(index + 1) == Some(&2)
                    && let (Some(red), Some(green), Some(blue)) = (
                        values.get(index + 2),
                        values.get(index + 3),
                        values.get(index + 4),
                    )
                {
                    style.bg = Some(Color::Rgb(
                        u8::try_from(*red).unwrap_or(0),
                        u8::try_from(*green).unwrap_or(0),
                        u8::try_from(*blue).unwrap_or(0),
                    ));
                    index = index.saturating_add(4);
                }
            }
            39 => style.fg = None,
            49 => style.bg = None,
            _ => {}
        }
        index = index.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn draw(
        width: u16,
        height: u16,
        theme: Theme,
        render: impl FnOnce(&mut Frame),
    ) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        with_theme(theme, || {
            terminal.draw(|frame| render(frame)).expect("draw a frame")
        });
        terminal.backend().buffer().clone()
    }

    fn screen_text(buffer: &ratatui::buffer::Buffer) -> String {
        let mut screen = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                screen.push_str(buffer[(x, y)].symbol());
            }
            screen.push('\n');
        }
        screen
    }

    fn plain_screen(buffer: &ratatui::buffer::Buffer) -> bool {
        (0..buffer.area.height).all(|y| {
            (0..buffer.area.width).all(|x| {
                let cell = &buffer[(x, y)];
                cell.fg == Color::Reset && cell.bg == Color::Reset
            })
        })
    }

    /// The design checkout, when this machine has one.
    fn design_checkout() -> Option<PathBuf> {
        let candidates = [
            std::env::var_os("CLAWFFICE_DESIGN_DIR").map(PathBuf::from),
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .map(|root| root.join("clawffice-design"))?,
            ),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|candidate| candidate.join("tokens/core.json").is_file())
    }

    #[test]
    fn the_banner_is_the_design_sources_art_not_a_hand_copied_string() {
        let parsed = parse_banner();
        let expected: Vec<&str> = BANNER_TEXT.lines().collect();
        assert_eq!(parsed.len(), expected.len(), "banner row count");
        for (row, (line, expected)) in parsed.iter().zip(expected).enumerate() {
            assert_eq!(
                line.text().trim_end(),
                expected.trim_end(),
                "banner row {row} differs from the design"
            );
        }
        assert!(
            BANNER_ANSI.contains("\u{1b}[0;38;2;74;159;224m"),
            "the ANSI source carries the design's true-colour tide hue"
        );
        assert_eq!(banner_height(), 8);
        assert_eq!(parse_banner()[0].text().chars().count(), 80);
    }

    #[test]
    fn every_vendored_file_matches_the_hash_recorded_in_source_json() {
        use sha2::{Digest, Sha256};

        let doc: serde_json::Value = serde_json::from_str(SOURCE_JSON).expect("SOURCE.json parses");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/design");
        let hashes = doc["sha256"]
            .as_object()
            .expect("SOURCE.json records a hash per file");
        assert_eq!(
            hashes.len(),
            PALETTE_JSON.len() + 5,
            "the banner, both token files and five palettes are hashed"
        );
        for (relative, expected) in hashes {
            let bytes = std::fs::read(root.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            let actual = format!("{:x}", Sha256::digest(&bytes));
            assert_eq!(
                actual,
                expected.as_str().unwrap_or_default(),
                "{relative} does not match its recorded design hash"
            );
        }
        let commit = doc["source"]["commit"].as_str().unwrap_or_default();
        assert_eq!(commit.len(), 12, "the design commit is recorded");
        assert!(
            commit
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn the_vendored_design_files_are_byte_identical_to_the_checkout() {
        let Some(checkout) = design_checkout() else {
            return;
        };
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/design");
        for (relative, vendored) in [
            ("banners/tidepool/tidepool-banner-static.ans", BANNER_ANSI),
            ("banners/tidepool/tidepool-banner-static.txt", BANNER_TEXT),
            ("tokens/core.json", CORE_JSON),
            ("tokens/apps/tidepool.json", APP_JSON),
            ("tokens/cells.json", CELLS_JSON),
        ] {
            let checkout_file = std::fs::read_to_string(checkout.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            assert_eq!(
                vendored, checkout_file,
                "{relative} drifted from the checkout"
            );
            let local = std::fs::read_to_string(source.join(relative))
                .unwrap_or_else(|error| panic!("read vendored {relative}: {error}"));
            assert_eq!(
                local, vendored,
                "{relative} is not the file this build embedded"
            );
        }
        for (name, vendored) in PALETTE_JSON {
            let relative = format!("tokens/palettes/{name}.json");
            let checkout_file = std::fs::read_to_string(checkout.join(&relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            assert_eq!(
                *vendored, checkout_file,
                "{relative} drifted from the checkout"
            );
        }
    }

    #[test]
    fn every_palette_resolves_its_own_design_tokens() {
        for name in PaletteName::ALL {
            let theme = Theme::new(name, true);
            let file: PaletteTokens = serde_json::from_str(
                PALETTE_JSON
                    .iter()
                    .find(|(palette, _)| *palette == name.as_str())
                    .map(|(_, json)| *json)
                    .expect("the palette has a vendored file"),
            )
            .expect("a palette file parses");
            assert_eq!(theme.palette(), name);
            assert_eq!(theme.colour_of(StyleRole::Text), parse_hex(&file.role.text));
            assert_eq!(
                theme.colour_of(StyleRole::Muted),
                parse_hex(&file.role.text_muted)
            );
            assert_eq!(
                theme.colour_of(StyleRole::Background),
                parse_hex(&file.role.window_background)
            );
            assert_eq!(
                theme.colour_of(StyleRole::Primary),
                parse_hex(&file.role.primary)
            );
            assert_eq!(
                theme.colour_of(StyleRole::Danger),
                parse_hex(&file.status_color.danger)
            );
            for role in StyleRole::ALL {
                assert_ne!(
                    theme.colour_of(role),
                    Color::Reset,
                    "{name:?} left {role:?} unset"
                );
            }
        }
    }

    #[test]
    fn the_default_palette_is_the_one_the_design_names() {
        assert_eq!(default_palette(), PaletteName::Biolume);
        assert_eq!(PaletteName::ALL.len(), PALETTE_JSON.len());
        let theme = Theme::default();
        assert_eq!(theme.palette(), PaletteName::Biolume);
        // The Tidepool product hue drives focus, not the palette primary.
        assert_eq!(theme.colour_of(StyleRole::Focus), Color::Rgb(74, 159, 224));
        assert_eq!(
            theme.colour_of(StyleRole::Primary),
            Color::Rgb(198, 242, 78)
        );
    }

    #[test]
    fn a_no_colour_terminal_gets_no_styles_but_the_same_art() {
        assert!(!colour_supported(Some("1"), Some("xterm-256color")));
        assert!(!colour_supported(None, Some("dumb")));
        assert!(colour_supported(None, Some("xterm-256color")));
        assert!(colour_supported(Some(""), Some("xterm-256color")));

        let theme = Theme::new(PaletteName::Biolume, false);
        for role in StyleRole::ALL {
            assert_eq!(theme.colour_of(role), Color::Reset, "{role:?}");
            assert_eq!(theme.style(role).fg, None, "{role:?}");
            assert_eq!(theme.style(role).bg, None, "{role:?}");
        }
        let buffer = draw(80, 8, theme, |frame| {
            render_banner(&theme, frame, frame.area());
        });
        assert!(plain_screen(&buffer), "no-colour frame painted a colour");
        assert!(screen_text(&buffer).contains("▄▄█▀▀▀▀█▄▄"));
    }

    #[test]
    fn a_colour_terminal_paints_the_designs_rgb_values() {
        let theme = Theme::new(PaletteName::Biolume, true);
        let buffer = draw(80, 8, theme, |frame| {
            render_banner(&theme, frame, frame.area());
        });
        let screen = screen_text(&buffer);
        assert!(screen.contains("▄▄█▀▀▀▀█▄▄"), "{screen}");
        // Row 2 of the source starts with twelve spaces, then the tide hue.
        assert_eq!(buffer[(12, 1)].fg, Color::Rgb(74, 159, 224));
        assert!(
            (0..buffer.area.height).any(
                |y| (0..buffer.area.width).any(|x| matches!(buffer[(x, y)].fg, Color::Rgb(..)))
            )
        );
    }

    #[test]
    fn the_banner_and_splash_render_at_three_widths_with_and_without_colour() {
        for colour in [true, false] {
            let theme = Theme::new(PaletteName::Biolume, colour);
            for (width, marker) in [(80_u16, "▄▄█▀▀▀▀█▄▄"), (60, "▄▄█▀▀▀▀█▄▄"), (30, "tidepool")]
            {
                let buffer = draw(width, 30, theme, |frame| {
                    render_splash(&theme, frame, frame.area());
                });
                let screen = screen_text(&buffer);
                assert!(
                    screen.contains(marker),
                    "colour={colour} width={width} lost the banner:\n{screen}"
                );
                assert!(screen.contains(CONSOLE_VERSION), "{screen}");
                assert!(screen.contains(CONTRACT_VERSION), "{screen}");
                assert_eq!(
                    plain_screen(&buffer),
                    !colour,
                    "width={width} colour={colour}"
                );
            }
        }
    }

    #[test]
    fn the_about_screen_reports_both_versions_at_three_widths() {
        for colour in [true, false] {
            let theme = Theme::new(PaletteName::SeaGlass, colour);
            for (width, height) in [(100_u16, 30_u16), (60, 24), (30, 24)] {
                let buffer = draw(width, height, theme, |frame| {
                    render_about(
                        &theme,
                        Some("https://pool.test"),
                        Some("ops@example.com"),
                        frame,
                        frame.area(),
                    );
                });
                let screen = screen_text(&buffer);
                assert!(screen.contains("About Tidepool"), "{screen}");
                assert!(screen.contains("console"), "{screen}");
                assert!(screen.contains(CONSOLE_VERSION), "{screen}");
                assert!(screen.contains("contract"), "{screen}");
                assert!(screen.contains(CONTRACT_VERSION), "{screen}");
                assert!(screen.contains("Sea Glass"), "{screen}");
                assert_eq!(
                    plain_screen(&buffer),
                    !colour,
                    "width={width} colour={colour}"
                );
            }
        }
    }

    #[test]
    fn the_cell_sprite_is_drawn_from_the_design_file() {
        let design = design().expect("the design assets load");
        let rows = &design.cells.sprites.pool.rows;
        let theme = Theme::new(PaletteName::Biolume, true);
        let buffer = draw(26, 12, theme, |frame| {
            draw_sprite(&theme, frame, frame.area());
        });
        let screen = screen_text(&buffer);
        assert!(
            screen.contains("██"),
            "the sprite is terminal art:\n{screen}"
        );
        assert_eq!(
            buffer[(6, 0)].fg,
            Color::Rgb(74, 159, 224),
            "the first sprite pixel is the product hue"
        );
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0].chars().count(), 12);

        let no_colour = Theme::new(PaletteName::Biolume, false);
        let buffer = draw(26, 12, no_colour, |frame| {
            draw_sprite(&no_colour, frame, frame.area());
        });
        let screen = screen_text(&buffer);
        assert!(
            screen.contains('#'),
            "the glyph legend survives no colour:\n{screen}"
        );
        assert!(plain_screen(&buffer));
    }

    #[test]
    fn an_unknown_sgr_parameter_is_ignored_instead_of_printed() {
        let parsed = parse_ansi("\u{1b}[0;38;2;1;2;3mab\u{1b}[99mcd");
        assert_eq!(parsed[0].text(), "abcd");
        assert_eq!(parsed[0].segments[0].style.fg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn a_one_pixel_area_does_not_panic() {
        for (width, height) in [(0_u16, 0_u16), (1, 1), (2, 0), (0, 2)] {
            let theme = Theme::default();
            let _ = draw(width, height, theme, |frame| {
                render_splash(&theme, frame, frame.area());
                render_about(&theme, None, None, frame, frame.area());
                render_banner(&theme, frame, frame.area());
            });
        }
    }
}
