use anyhow::{bail, Context, Result};
use clap::Parser;
use matui::{
    cli::Args,
    ui::{self, App, PlayerView, TrackView},
};
use std::io::IsTerminal;

fn demo() -> App {
    App {
        demo: true,
        connected: true,
        title: "Sample track — offline preview".into(),
        artist: "Fictional artist · no audio or network".into(),
        status: "Offline demo: controls do not affect any server".into(),
        audio_status: "Sendspin 0.3.7 · disabled in demo".into(),
        selected_id: Some("demo".into()),
        players: vec![PlayerView {
            details: serde_json::Value::Null,
            id: "demo".into(),
            name: "This computer (demo)".into(),
            available: true,
            state: "paused".into(),
            volume: Some(30),
        }],
        queue: vec![TrackView {
            title: "Sample track — offline preview".into(),
            artist: "Fictional artist".into(),
            duration: 240.0,
            ..Default::default()
        }],
        elapsed: 72.0,
        duration: 240.0,
        ..App::default()
    }
}

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.init {
        let path = config_path(args.config)?;
        matui::cli::initialize(&path)?;
        println!(
            "Created {}. Run matui --setup to configure the server, login and local speaker.",
            path.display()
        );
        return Ok(());
    }
    if args.demo && args.snapshot {
        let mut app = demo();
        matui::theme::reload(&mut app.palette, &matui::theme::paths());
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30))?;
        terminal.draw(|f| ui::draw(f, &mut app))?;
        for row in terminal.backend().buffer().content.chunks(110) {
            println!("{}", row.iter().map(|c| c.symbol()).collect::<String>());
        }
        return Ok(());
    }
    if args.demo {
        return matui::terminal_ui::run(
            demo(),
            |_| {},
            |app, action| {
                if let ui::Action::Search(query) = action {
                    app.results = app
                        .queue
                        .iter()
                        .filter(|t| t.title.to_lowercase().contains(&query.to_lowercase()))
                        .cloned()
                        .collect();
                    app.status = "Demo search complete (fictional offline data)".into();
                } else if let ui::Action::Browse { generation, target } = action {
                    app.music
                        .apply(generation, Ok((matui::music::demo_listing(&target), None)));
                } else {
                    app.status = "Offline demo: no command was sent".into();
                }
            },
        )
        .map(|_| ());
    }
    if args.list_devices {
        let devices = matui::audio::devices()?;
        if devices.is_empty() {
            println!("No usable audio output devices found");
        }
        for device in devices {
            println!("{}\n  {}", device.id, device.name);
        }
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("An interactive terminal is required; use --demo --snapshot for plain output");
    }
    let path = config_path(args.config)?;
    let mut config = if path.exists() {
        matui::config::Config::parse(
            &std::fs::read_to_string(&path).context("Cannot read configuration")?,
        )?
    } else {
        matui::config::Config {
            local_playback: true,
            ..Default::default()
        }
    };
    let mut token = std::env::var("MATUI_TOKEN").ok();
    if token.is_none() && path.exists() {
        token = matui::credentials::load(&config.server, &config.player_id)
            .await
            .ok();
    }
    let mut setup = args.setup || token.is_none() || !path.exists();
    loop {
        if setup {
            if let Some((next_config, next_token)) =
                matui::settings::run(config.clone(), token.clone(), &path)?
            {
                config = next_config;
                token = Some(next_token);
            } else if token.is_none() {
                return Ok(());
            }
        }
        let Some(token_value) = token.as_ref() else {
            return Ok(());
        };
        let api = matui::api::ApiClient::new(&config.server, token_value)?;
        let audio = if (args.local || config.local_playback) && !args.remote_only {
            Some(matui::audio::start(matui::audio::AudioConfig {
                server: config.server.clone(),
                token: token_value.clone(),
                player_id: config.player_id.clone(),
                player_name: config.player_name.clone(),
                device_id: config.device_id.clone(),
                volume: config.volume,
                muted: false,
            })?)
        } else {
            None
        };
        let mut controller = matui::controller::Controller::start(api);
        let requests = controller.requests.clone();
        let selection = controller.selection.clone();
        let audio_status = audio.as_ref().map(|a| a.status.clone());
        let local_id = if audio.is_some() {
            Some(config.player_id.as_str())
        } else {
            None
        };
        let result = matui::terminal_ui::run(
            App::default(),
            |app| {
                while let Ok(update) = controller.updates.try_recv() {
                    matui::presentation::apply(app, update);
                }
                if app.selected_id.is_none() {
                    if let Some(player) = app
                        .players
                        .iter()
                        .find(|p| Some(p.id.as_str()) == local_id && p.available)
                    {
                        app.selected_id = Some(player.id.clone());
                        let _ = selection.send(app.selected_id.clone());
                    }
                }
                if let Some(status) = &audio_status {
                    let status = status.borrow();
                    app.audio_status =
                        format!("Local audio · {} · {}", status.state, status.detail);
                }
            },
            |app, action| {
                if let ui::Action::Select(id) = action {
                    if selection.send(Some(id)).is_err() {
                        app.status = "API worker stopped".into();
                    }
                } else {
                    let searching = matches!(action, ui::Action::Search(_));
                    let browsing = matches!(action, ui::Action::Browse { .. });
                    if requests
                        .try_send(matui::controller::Request::new(
                            app.selected_id.clone(),
                            action,
                        ))
                        .is_err()
                    {
                        app.status = "Busy: command not sent; try again".into();
                        if browsing {
                            app.music.apply(
                                app.music.generation,
                                Err("Busy: press r to retry loading music".into()),
                            );
                        }
                    } else if searching {
                        app.results.clear();
                        app.status = "Searching…".into();
                    } else if browsing {
                        app.status =
                            "Browsing music · Enter opens collections; P chooses playback".into();
                    } else {
                        app.status = "Command pending…".into();
                    }
                }
            },
        );
        controller.shutdown().await;
        if let Some(audio) = audio {
            audio.shutdown().await;
        }
        match result? {
            ui::Action::OpenSettings => setup = true,
            _ => return Ok(()),
        }
    }
}

fn config_path(explicit: Option<std::path::PathBuf>) -> Result<std::path::PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(home) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Ok(std::path::PathBuf::from(home).join("matui/config.toml"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow::anyhow!("Use --config when HOME is unset"))?;
    Ok(std::path::PathBuf::from(home).join(".config/matui/config.toml"))
}
