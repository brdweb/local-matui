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
            "Created {}. Set server/device settings, then supply MATUI_TOKEN separately.",
            path.display()
        );
        return Ok(());
    }
    if args.demo && args.snapshot {
        let mut app = demo();
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
                } else {
                    app.status = "Offline demo: no command was sent".into();
                }
            },
        );
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
    let path = config_path(args.config)?;
    let text = std::fs::read_to_string(&path)
        .context("Cannot read configuration; run matui --init first")?;
    let config = matui::config::Config::parse(&text)?;
    let token = std::env::var("MATUI_TOKEN")
        .map_err(|_| anyhow::anyhow!("Set MATUI_TOKEN to a Music Assistant access token"))?;
    let api = matui::api::ApiClient::new(&config.server, &token)?;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("An interactive terminal is required; use --demo --snapshot for plain output");
    }
    let audio = if (args.local || config.local_playback) && !args.remote_only {
        Some(matui::audio::start(matui::audio::AudioConfig {
            server: config.server,
            token,
            player_id: config.player_id,
            player_name: config.player_name,
            device_id: config.device_id,
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
    let result = matui::terminal_ui::run(
        App::default(),
        |app| {
            while let Ok(update) = controller.updates.try_recv() {
                matui::presentation::apply(app, update);
            }
            if let Some(status) = &audio_status {
                let status = status.borrow();
                app.audio_status = format!("Local audio · {} · {}", status.state, status.detail);
            }
        },
        |app, action| {
            if let ui::Action::Select(id) = action {
                if selection.send(Some(id)).is_err() {
                    app.status = "API worker stopped".into();
                }
            } else {
                let searching = matches!(action, ui::Action::Search(_));
                if requests
                    .try_send(matui::controller::Request::new(
                        app.selected_id.clone(),
                        action,
                    ))
                    .is_err()
                {
                    app.status = "Busy: command not sent; try again".into();
                } else if searching {
                    app.results.clear();
                    app.status = "Searching…".into();
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
    result
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
