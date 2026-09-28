// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;
use clock::app::{AppTheme, ClockApp, Flags};
use i18n_embed::DesktopLanguageRequester;

#[derive(Debug, Parser)]
#[command(
    name = "clock",
    about = "World clocks, alarms, timers, and a stopwatch for COSMIC",
    version = env!("GIT_VERSION")
)]
struct Cli {
    /// Screenshot harness only: load fixed application data and time.
    #[arg(long, hide = true)]
    preview: bool,

    /// Screenshot harness only: override the initial window size.
    #[arg(long, hide = true, value_name = "WIDTHxHEIGHT", value_parser = parse_window_size)]
    preview_window: Option<(f32, f32)>,

    /// Screenshot harness only: force a deterministic light or dark theme.
    #[arg(long, hide = true, value_enum)]
    preview_theme: Option<PreviewTheme>,

    /// Screenshot harness only: select the initial page.
    #[arg(long, hide = true, value_enum, default_value_t = PreviewPage::WorldClocks)]
    preview_page: PreviewPage,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum PreviewTheme {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
enum PreviewPage {
    #[default]
    WorldClocks,
    Alarms,
    Timers,
    Stopwatch,
}

impl PreviewPage {
    const fn position(self) -> u16 {
        match self {
            Self::WorldClocks => 0,
            Self::Alarms => 1,
            Self::Timers => 2,
            Self::Stopwatch => 3,
        }
    }
}

impl From<PreviewTheme> for AppTheme {
    fn from(theme: PreviewTheme) -> Self {
        match theme {
            PreviewTheme::Dark => Self::Dark,
            PreviewTheme::Light => Self::Light,
        }
    }
}

fn parse_window_size(value: &str) -> Result<(f32, f32), String> {
    let (width, height) = value
        .split_once('x')
        .ok_or_else(|| "window size must be WIDTHxHEIGHT".to_owned())?;
    let width = width
        .parse::<f32>()
        .map_err(|error| format!("invalid window width: {error}"))?;
    let height = height
        .parse::<f32>()
        .map_err(|error| format!("invalid window height: {error}"))?;
    if !width.is_finite() || !height.is_finite() || width < 320.0 || height < 320.0 {
        return Err("window dimensions must be finite and at least 320".to_owned());
    }
    Ok((width, height))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("clock=info")),
        )
        .init();

    let cli = Cli::parse();
    let requested_languages = DesktopLanguageRequester::requested_languages();
    let locale = requested_languages
        .first()
        .map(ToString::to_string)
        .unwrap_or_else(|| "en-US".to_owned());
    clock::i18n::init(&requested_languages);
    let window_size = cli.preview_window.unwrap_or((900.0, 640.0));
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(window_size.0, window_size.1))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(320.0)
                .min_height(320.0),
        );
    cosmic::app::run::<ClockApp>(
        settings,
        Flags {
            initial_theme: cli.preview_theme.map(Into::into),
            initial_page: cli.preview_page.position(),
            locale,
            preview: cli.preview,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn preview_options_produce_deterministic_initial_state() {
        let cli = Cli::try_parse_from([
            "clock",
            "--preview",
            "--preview-window",
            "400x880",
            "--preview-theme",
            "dark",
            "--preview-page",
            "stopwatch",
        ])
        .expect("preview harness options should parse");
        assert!(cli.preview);
        assert!(matches!(cli.preview_theme, Some(PreviewTheme::Dark)));
        assert!(matches!(cli.preview_page, PreviewPage::Stopwatch));
        assert_eq!(parse_window_size("400x880"), Ok((400.0, 880.0)));
        assert!(parse_window_size("200x880").is_err());
    }

    #[test]
    fn cli_uses_the_build_version() {
        assert_eq!(Cli::command().get_version(), Some(env!("GIT_VERSION")));
    }
}
