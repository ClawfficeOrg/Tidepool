//! The client's Clawffice palettes.
//!
//! The five palettes come from `clawffice-design` at commit `995b41606e65`, the
//! same pinned commit the console and control plane render. The values are
//! copied here rather than read at run time so a checkout with no sibling
//! design repository still builds; [`PaletteName`] is the single place a colour
//! is written, and every surface resolves a role from it.
//!
//! Three rules keep the choice honest:
//!
//! - [`PaletteName::default`] is [`PaletteName::Biolume`], the design system's
//!   own default.
//! - [`PaletteName::theme_config`] maps every role the component library paints
//!   with — background, surface, border, text, status and every component
//!   ground — so switching a palette cannot leave a surface on the previous
//!   one.
//! - [`apply`] stores the choice in [`PaletteChoice`] and in the settings file
//!   through the caller, so the next launch starts in the same palette.
//!
//! `tide` (the Tidepool product hue) drives focus, selection and links whatever
//! the palette's primary is, as the suite's design rules require.

use std::rc::Rc;

use gpui::{App, Global, Render, Window};
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use serde::{Deserialize, Serialize};

/// One selectable Clawffice palette.
///
/// The serde spelling is the stable key the settings file stores, so renaming a
/// variant is a settings migration, not a cosmetic change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaletteName {
    /// Deep-sea ink, phosphor lime primary, claw orange second. The default.
    #[default]
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
    pub const ALL: [Self; 5] = [
        Self::Biolume,
        Self::Boiled,
        Self::DeepTide,
        Self::Phosphor,
        Self::SeaGlass,
    ];

    /// The palette's file name and settings key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Biolume => "biolume",
            Self::Boiled => "boiled",
            Self::DeepTide => "deep-tide",
            Self::Phosphor => "phosphor",
            Self::SeaGlass => "sea-glass",
        }
    }

    /// The palette's display name, as the design file spells it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Biolume => "Biolume",
            Self::Boiled => "Boiled",
            Self::DeepTide => "Deep Tide",
            Self::Phosphor => "Phosphor",
            Self::SeaGlass => "Sea Glass",
        }
    }

    /// The light/dark scheme the palette's design file declares.
    pub fn scheme(self) -> ThemeMode {
        match self {
            Self::SeaGlass => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }

    /// Resolve a palette key, case-insensitively.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|palette| palette.as_str() == name)
    }

    /// The palette's colours, as the design file writes them.
    pub fn palette(self) -> Palette {
        match self {
            Self::Biolume => Palette {
                scheme: ThemeMode::Dark,
                focus: "#4A9FE0",
                accent: "#F26B21",
                role: PaletteRole {
                    window_background: "#06141B",
                    surface: "#0C2029",
                    terminal_background: "#030C11",
                    border: "#16343F",
                    border_strong: "#24505E",
                    text: "#BFD6D6",
                    text_muted: "#759695",
                    text_strong: "#E9FBF6",
                    primary: "#C6F24E",
                    primary_text: "#06141B",
                    second: "#F26B21",
                },
                status: StatusColours {
                    success: "#4CC38A",
                    warning: "#F2B441",
                    danger: "#EF5B5B",
                    info: "#4A9FE0",
                },
            },
            Self::Boiled => Palette {
                scheme: ThemeMode::Dark,
                focus: "#4A9FE0",
                accent: "#F26B21",
                role: PaletteRole {
                    window_background: "#1B1218",
                    surface: "#281A22",
                    terminal_background: "#140D12",
                    border: "#3D2833",
                    border_strong: "#5C3D4C",
                    text: "#E3CFD3",
                    text_muted: "#A88C95",
                    text_strong: "#FFF1EC",
                    primary: "#F0483E",
                    primary_text: "#1B1218",
                    second: "#F6C85F",
                },
                status: StatusColours {
                    success: "#7FD18B",
                    warning: "#F6C85F",
                    danger: "#FF7B72",
                    info: "#7FD1C7",
                },
            },
            Self::DeepTide => Palette {
                scheme: ThemeMode::Dark,
                focus: "#4A9FE0",
                accent: "#F26B21",
                role: PaletteRole {
                    window_background: "#0B1A2A",
                    surface: "#11263B",
                    terminal_background: "#07111C",
                    border: "#1A3550",
                    border_strong: "#2A4A6B",
                    text: "#C6D4E3",
                    text_muted: "#7F97B0",
                    text_strong: "#EEF4FA",
                    primary: "#F26B21",
                    primary_text: "#0B1A2A",
                    second: "#3FD0E8",
                },
                status: StatusColours {
                    success: "#4CC38A",
                    warning: "#F2B441",
                    danger: "#EF5B5B",
                    info: "#4A9FE0",
                },
            },
            Self::Phosphor => Palette {
                scheme: ThemeMode::Dark,
                focus: "#4A9FE0",
                accent: "#F26B21",
                role: PaletteRole {
                    window_background: "#0A0C0A",
                    surface: "#131713",
                    terminal_background: "#050605",
                    border: "#263026",
                    border_strong: "#3D4D3D",
                    text: "#B9C7B4",
                    text_muted: "#7D8C78",
                    text_strong: "#EAF5E4",
                    primary: "#C6F24E",
                    primary_text: "#0A0C0A",
                    second: "#FF5A36",
                },
                status: StatusColours {
                    success: "#6EE7A0",
                    warning: "#FFB000",
                    danger: "#FF6B6B",
                    info: "#5CE1E6",
                },
            },
            Self::SeaGlass => Palette {
                scheme: ThemeMode::Light,
                focus: "#1864AB",
                accent: "#C7410C",
                role: PaletteRole {
                    window_background: "#EEF3F1",
                    surface: "#FFFFFF",
                    terminal_background: "#FFFFFF",
                    border: "#CFDAD6",
                    border_strong: "#8FA39E",
                    text: "#24393A",
                    text_muted: "#5A7170",
                    text_strong: "#0E2224",
                    primary: "#C7410C",
                    primary_text: "#FFFFFF",
                    second: "#0B7285",
                },
                status: StatusColours {
                    success: "#1E7B4F",
                    warning: "#8A5A00",
                    danger: "#B42318",
                    info: "#1864AB",
                },
            },
        }
    }

    /// The component library configuration this palette paints with.
    ///
    /// Every field the library reads for a surface is set explicitly: a field
    /// left `None` would fall back to the library's own theme, which is how a
    /// palette switch leaves one panel behind.
    pub fn theme_config(self) -> ThemeConfig {
        let palette = self.palette();
        let role = &palette.role;
        let status = &palette.status;
        let mut config = ThemeConfig {
            name: format!("Tidepool {}", self.title()).into(),
            mode: palette.scheme,
            ..ThemeConfig::default()
        };
        let colors = &mut config.colors;

        colors.background = Some(role.window_background.into());
        colors.foreground = Some(role.text.into());
        colors.popover = Some(role.surface.into());
        colors.popover_foreground = Some(role.text.into());
        colors.border = Some(role.border.into());
        colors.window_border = Some(role.border_strong.into());
        colors.input = Some(role.border_strong.into());
        colors.ring = Some(palette.focus.into());
        colors.caret = Some(palette.focus.into());
        colors.selection = Some(palette.focus.into());
        colors.link = Some(palette.focus.into());
        colors.link_hover = Some(role.primary.into());
        colors.link_active = Some(role.second.into());

        colors.primary = Some(role.primary.into());
        colors.primary_foreground = Some(role.primary_text.into());
        colors.primary_hover = Some(role.primary.into());
        colors.primary_active = Some(role.primary.into());
        colors.secondary = Some(role.second.into());
        colors.secondary_foreground = Some(role.window_background.into());
        colors.secondary_hover = Some(role.second.into());
        colors.secondary_active = Some(role.second.into());
        colors.muted = Some(role.surface.into());
        colors.muted_foreground = Some(role.text_muted.into());
        colors.accent = Some(role.border.into());
        colors.accent_foreground = Some(role.text_strong.into());

        colors.danger = Some(status.danger.into());
        colors.danger_foreground = Some(role.window_background.into());
        colors.danger_hover = Some(status.danger.into());
        colors.danger_active = Some(status.danger.into());
        colors.warning = Some(status.warning.into());
        colors.warning_foreground = Some(role.window_background.into());
        colors.warning_hover = Some(status.warning.into());
        colors.warning_active = Some(status.warning.into());
        colors.success = Some(status.success.into());
        colors.success_foreground = Some(role.window_background.into());
        colors.success_hover = Some(status.success.into());
        colors.success_active = Some(status.success.into());
        colors.info = Some(status.info.into());
        colors.info_foreground = Some(role.window_background.into());
        colors.info_hover = Some(status.info.into());
        colors.info_active = Some(status.info.into());

        colors.button = Some(role.surface.into());
        colors.button_hover = Some(role.border.into());
        colors.button_active = Some(role.border_strong.into());
        colors.button_foreground = Some(role.text.into());
        colors.button_primary = Some(role.primary.into());
        colors.button_primary_hover = Some(role.primary.into());
        colors.button_primary_active = Some(role.primary.into());
        colors.button_primary_foreground = Some(role.primary_text.into());
        colors.button_secondary = Some(role.second.into());
        colors.button_secondary_hover = Some(role.second.into());
        colors.button_secondary_active = Some(role.second.into());
        colors.button_secondary_foreground = Some(role.window_background.into());
        colors.button_danger = Some(status.danger.into());
        colors.button_danger_hover = Some(status.danger.into());
        colors.button_danger_active = Some(status.danger.into());
        colors.button_danger_foreground = Some(role.window_background.into());
        colors.button_success = Some(status.success.into());
        colors.button_success_hover = Some(status.success.into());
        colors.button_success_active = Some(status.success.into());
        colors.button_success_foreground = Some(role.window_background.into());
        colors.button_warning = Some(status.warning.into());
        colors.button_warning_hover = Some(status.warning.into());
        colors.button_warning_active = Some(status.warning.into());
        colors.button_warning_foreground = Some(role.window_background.into());
        colors.button_info = Some(status.info.into());
        colors.button_info_hover = Some(status.info.into());
        colors.button_info_active = Some(status.info.into());
        colors.button_info_foreground = Some(role.window_background.into());

        colors.accordion = Some(role.surface.into());
        colors.group_box = Some(role.surface.into());
        colors.group_box_foreground = Some(role.text.into());
        colors.group_box_title_foreground = Some(role.text_strong.into());
        colors.description_list_label = Some(role.surface.into());
        colors.description_list_label_foreground = Some(role.text_muted.into());
        colors.drop_target = Some(role.surface.into());
        colors.drag_border = Some(palette.focus.into());
        colors.overlay = Some(role.window_background.into());
        colors.skeleton = Some(role.border.into());

        colors.scrollbar = Some(role.surface.into());
        colors.scrollbar_thumb = Some(role.border_strong.into());
        colors.scrollbar_thumb_hover = Some(role.text_muted.into());
        colors.switch = Some(role.border_strong.into());
        colors.switch_thumb = Some(role.text_strong.into());
        colors.slider_bar = Some(role.border.into());
        colors.slider_thumb = Some(palette.focus.into());
        colors.progress_bar = Some(palette.focus.into());

        colors.list = Some(role.surface.into());
        colors.list_even = Some(role.window_background.into());
        colors.list_hover = Some(role.border.into());
        colors.list_active = Some(role.border.into());
        colors.list_active_border = Some(palette.focus.into());
        colors.list_head = Some(role.surface.into());
        colors.table = Some(role.surface.into());
        colors.table_even = Some(role.window_background.into());
        colors.table_hover = Some(role.border.into());
        colors.table_active = Some(role.border.into());
        colors.table_active_border = Some(palette.focus.into());
        colors.table_head = Some(role.surface.into());
        colors.table_head_foreground = Some(role.text_strong.into());
        colors.table_foot = Some(role.surface.into());
        colors.table_foot_foreground = Some(role.text_muted.into());
        colors.table_row_border = Some(role.border.into());

        colors.tab_bar = Some(role.window_background.into());
        colors.tab_bar_segmented = Some(role.surface.into());
        colors.tab = Some(role.window_background.into());
        colors.tab_active = Some(role.surface.into());
        colors.tab_active_foreground = Some(role.text_strong.into());
        colors.tab_foreground = Some(role.text_muted.into());

        colors.sidebar = Some(role.surface.into());
        colors.sidebar_foreground = Some(role.text.into());
        colors.sidebar_border = Some(role.border.into());
        colors.sidebar_accent = Some(role.border.into());
        colors.sidebar_accent_foreground = Some(role.text_strong.into());
        colors.sidebar_primary = Some(role.primary.into());
        colors.sidebar_primary_foreground = Some(role.primary_text.into());
        colors.title_bar = Some(role.surface.into());
        colors.title_bar_border = Some(role.border.into());
        colors.status_bar = Some(role.surface.into());
        colors.status_bar_border = Some(role.border.into());

        colors.chart_1 = Some(role.primary.into());
        colors.chart_2 = Some(role.second.into());
        colors.chart_3 = Some(palette.focus.into());
        colors.chart_4 = Some(status.success.into());
        colors.chart_5 = Some(status.warning.into());
        colors.chart_bullish = Some(status.success.into());
        colors.chart_bearish = Some(status.danger.into());

        config
    }
}

/// The palette choice in effect for the process.
///
/// A global so a view can ask [`current`] without threading a settings handle
/// through every window; [`apply`] is the only writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteChoice(pub PaletteName);

impl Global for PaletteChoice {}

/// The palette in effect, Biolume until [`apply`] runs.
pub fn current(cx: &App) -> PaletteName {
    cx.try_global::<PaletteChoice>()
        .map(|choice| choice.0)
        .unwrap_or_default()
}

/// Make `palette` the app's palette and repaint every open window.
///
/// Call after `gpui_kit::init`, which installs the component library's own
/// default theme. The palette's own scheme decides light or dark: Sea Glass is
/// the design system's light palette, the other four are dark.
pub fn apply(palette: PaletteName, cx: &mut App) {
    let scheme = palette.scheme();
    Theme::change(scheme, None, cx);
    let config = Rc::new(palette.theme_config());
    Theme::global_mut(cx).apply_config(&config);
    Theme::sync_base(cx);
    cx.set_global(PaletteChoice(palette));
    cx.refresh_windows();
}

/// Make sure a window that is opening paints with the current palette.
///
/// A window created after [`apply`] already sees the global theme; this also
/// installs Biolume when the process has not chosen yet (a test, or a launch
/// whose settings file was unreadable).
pub fn follow_theme(window: &mut Window, cx: &mut App) {
    if !cx.has_global::<PaletteChoice>() {
        apply(PaletteName::default(), cx);
    }
    window.refresh();
}

/// One palette's role colours, exactly as its design file writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRole {
    /// The window ground.
    pub window_background: &'static str,
    /// A raised panel ground.
    pub surface: &'static str,
    /// The ground behind raw output.
    pub terminal_background: &'static str,
    /// A quiet border.
    pub border: &'static str,
    /// A border that must stand out.
    pub border_strong: &'static str,
    /// Body text.
    pub text: &'static str,
    /// Secondary or disabled text.
    pub text_muted: &'static str,
    /// Emphasised text.
    pub text_strong: &'static str,
    /// The palette's primary hue.
    pub primary: &'static str,
    /// Text drawn on the primary hue.
    pub primary_text: &'static str,
    /// The palette's second hue.
    pub second: &'static str,
}

/// The palette's status colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusColours {
    /// A successful state.
    pub success: &'static str,
    /// A state that needs attention.
    pub warning: &'static str,
    /// A failed state.
    pub danger: &'static str,
    /// Informational state.
    pub info: &'static str,
}

/// A palette resolved to roles, its scheme and the product hues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Light or dark, from the design file's `scheme`.
    pub scheme: ThemeMode,
    /// The Tidepool product hue (`tide`): focus, selection and links.
    pub focus: &'static str,
    /// The Clawffice accent (`claw`), used where the palette needs the suite's
    /// second identity colour rather than its own.
    pub accent: &'static str,
    /// The palette's role colours.
    pub role: PaletteRole,
    /// The palette's status colours.
    pub status: StatusColours,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use gpui::prelude::*;

    fn is_hex(value: &str) -> bool {
        value.len() == 7
            && value.starts_with('#')
            && value[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn biolume_is_the_default_and_the_first_listed() {
        assert_eq!(PaletteName::default(), PaletteName::Biolume);
        assert_eq!(PaletteName::ALL[0], PaletteName::Biolume);
    }

    #[test]
    fn every_palette_key_round_trips() {
        for palette in PaletteName::ALL {
            assert_eq!(PaletteName::from_name(palette.as_str()), Some(palette));
            assert_eq!(
                PaletteName::from_name(&palette.as_str().to_ascii_uppercase()),
                Some(palette)
            );
        }
        assert_eq!(PaletteName::from_name("no-such-palette"), None);
    }

    #[test]
    fn the_settings_keys_are_stable() {
        assert_eq!(
            PaletteName::ALL.map(PaletteName::as_str),
            ["biolume", "boiled", "deep-tide", "phosphor", "sea-glass"]
        );
    }

    #[test]
    fn every_palette_has_a_colour_for_every_role() {
        for palette in PaletteName::ALL {
            let resolved = palette.palette();
            let roles = [
                resolved.role.window_background,
                resolved.role.surface,
                resolved.role.terminal_background,
                resolved.role.border,
                resolved.role.border_strong,
                resolved.role.text,
                resolved.role.text_muted,
                resolved.role.text_strong,
                resolved.role.primary,
                resolved.role.primary_text,
                resolved.role.second,
                resolved.status.success,
                resolved.status.warning,
                resolved.status.danger,
                resolved.status.info,
                resolved.focus,
                resolved.accent,
            ];
            for role in roles {
                assert!(
                    is_hex(role),
                    "{} has a bad role colour {role}",
                    palette.title()
                );
            }
            assert_ne!(
                resolved.role.window_background,
                resolved.role.text,
                "{} must contrast its ground and text",
                palette.title()
            );
        }
    }

    #[test]
    fn sea_glass_is_the_light_scheme() {
        assert_eq!(PaletteName::SeaGlass.scheme(), ThemeMode::Light);
        for palette in PaletteName::ALL {
            if palette != PaletteName::SeaGlass {
                assert_eq!(palette.scheme(), ThemeMode::Dark);
            }
        }
    }

    #[test]
    fn the_config_sets_every_component_surface() {
        for palette in PaletteName::ALL {
            let config = palette.theme_config();
            assert_eq!(config.mode, palette.scheme());
            assert!(config.name.contains(palette.title()));
            let colors = &config.colors;
            let surfaces = [
                &colors.background,
                &colors.foreground,
                &colors.popover,
                &colors.popover_foreground,
                &colors.border,
                &colors.window_border,
                &colors.input,
                &colors.ring,
                &colors.caret,
                &colors.selection,
                &colors.primary,
                &colors.primary_foreground,
                &colors.secondary,
                &colors.secondary_foreground,
                &colors.muted,
                &colors.muted_foreground,
                &colors.accent,
                &colors.accent_foreground,
                &colors.danger,
                &colors.warning,
                &colors.success,
                &colors.info,
                &colors.button,
                &colors.button_primary,
                &colors.button_secondary,
                &colors.scrollbar,
                &colors.scrollbar_thumb,
                &colors.list,
                &colors.table,
                &colors.tab_bar,
                &colors.sidebar,
                &colors.title_bar,
                &colors.status_bar,
                &colors.group_box,
            ];
            for surface in surfaces {
                assert!(
                    surface.is_some(),
                    "{} leaves a surface on the library default",
                    palette.title()
                );
            }
        }
    }

    #[test]
    fn every_palette_resolves_focus_to_the_product_hue() {
        for palette in PaletteName::ALL {
            let resolved = palette.palette();
            assert_eq!(
                palette.theme_config().colors.ring.as_deref(),
                Some(resolved.focus)
            );
        }
    }

    /// A view that mounts every window surface, so a palette switch is proven
    /// to reach each one.
    struct EverySurface {
        main: gpui::Entity<crate::main_window::MainWindow>,
        popover: gpui::Entity<crate::popover_view::PopoverView>,
        settings: gpui::Entity<crate::settings_window::SettingsWindow>,
    }

    impl Render for EverySurface {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            gpui::div()
                .flex()
                .flex_col()
                .size_full()
                .child(self.main.clone())
                .child(self.popover.clone())
                .child(self.settings.clone())
        }
    }

    #[gpui::test]
    fn every_palette_renders_every_surface(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for palette in PaletteName::ALL {
            cx.update(|cx| apply(palette, cx));
            assert_eq!(
                cx.update(|cx| current(cx)),
                palette,
                "the global must hold the applied palette"
            );

            let view = cx.update(|cx| {
                let main = cx.new(|cx| {
                    crate::main_window::MainWindow::new("Tidepool".into(), "Tidepool".into(), cx)
                });
                let popover =
                    cx.new(|_cx| crate::popover_view::PopoverView::new("Tidepool", "Tidepool"));
                let settings =
                    cx.new(|cx| crate::settings_window::SettingsWindow::new("Tidepool".into(), cx));
                cx.new(|_cx| EverySurface {
                    main,
                    popover,
                    settings,
                })
            });

            cx.update(|cx| {
                cx.open_window(gpui::WindowOptions::default(), |_, _cx| view.clone())
                    .expect("open a window to render every surface");
            });
            cx.run_until_parked();

            // Every surface resolved a colour from this palette, not from the
            // component library's default theme.
            let expected = palette.palette();
            cx.update(|cx| {
                let theme = Theme::global(cx);
                assert_eq!(
                    theme.background,
                    gpui::Rgba::try_from(expected.role.window_background)
                        .expect("palette background parses")
                        .into(),
                    "{} did not reach the window ground",
                    palette.title()
                );
            });
        }
    }
}
