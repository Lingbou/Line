use std::{
    io::{self, IsTerminal},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossterm::event::{self, Event, KeyEventKind};
use line::{
    app::{App, AppAction},
    config::ConfigStore,
    ssh::SshRunner,
    ui,
};

use super::{
    DynError,
    path::complete_path,
    persistence::{
        delete_profile, load_with_recovery, maybe_recover_runtime_config, refresh_key_choices,
        save_profile,
    },
    session::connect,
    terminal::TuiSession,
};

pub(crate) fn run() -> Result<(), DynError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("line needs an interactive terminal".into());
    }

    let store = ConfigStore::in_home()?;
    let profiles = load_with_recovery(&store, None)?;
    let runner = SshRunner::new(store.root())?;
    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGQUIT,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&shutdown))?;
    }
    let mut app = App::new(profiles.profiles);
    refresh_key_choices(&store, &mut app)?;

    let mut terminal = TuiSession::enter()?;
    terminal.draw(|frame| ui::draw(frame, &mut app))?;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let event = event::read()?;
        if matches!(event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            continue;
        }

        match app.handle_event(event) {
            AppAction::None => {}
            AppAction::Quit => break,
            AppAction::Save(draft) => match save_profile(&store, &app, draft) {
                Ok((latest, saved)) => {
                    app.set_profiles(latest.profiles);
                    app.finish_save(saved);
                    if let Err(error) = refresh_key_choices(&store, &mut app) {
                        app.set_error(error.to_string());
                    }
                }
                Err(error) => {
                    if let Some(restored) =
                        maybe_recover_runtime_config(&store, &mut terminal, &shutdown)?
                    {
                        app = App::new(restored.profiles);
                        refresh_key_choices(&store, &mut app)?;
                        app.set_status("Restored profiles.json.bak");
                    } else if let Ok(latest) = store.load() {
                        app.set_profiles(latest.profiles);
                        let _ = refresh_key_choices(&store, &mut app);
                        app.set_error(format!(
                            "{error}\n\nLatest connections were reloaded. Cancel this form and edit the connection again."
                        ));
                    } else {
                        app.set_error(error);
                    }
                }
            },
            AppAction::Delete(id) => match delete_profile(&store, &app, &id) {
                Ok(latest) => {
                    app.set_profiles(latest.profiles);
                    app.finish_delete(&id);
                    if app.profiles().is_empty() {
                        app.begin_add();
                    }
                    if let Err(error) = refresh_key_choices(&store, &mut app) {
                        app.set_error(error.to_string());
                    }
                }
                Err(error) => {
                    if let Some(restored) =
                        maybe_recover_runtime_config(&store, &mut terminal, &shutdown)?
                    {
                        app = App::new(restored.profiles);
                        refresh_key_choices(&store, &mut app)?;
                        app.set_status("Restored profiles.json.bak");
                    } else {
                        app.cancel_delete();
                    }
                    if let Ok(latest) = store.load() {
                        app.set_profiles(latest.profiles);
                        let _ = refresh_key_choices(&store, &mut app);
                    }
                    if app.status_message().is_none() {
                        app.set_error(error);
                    }
                }
            },
            AppAction::CompletePath(value) => {
                app.finish_path_completion(complete_path(&value));
            }
            AppAction::DeleteKey(private_key) => {
                let deleted = match store.delete_key_if_unused(&private_key) {
                    Ok(()) => {
                        app.finish_delete_key(&private_key);
                        true
                    }
                    Err(error) => {
                        app.cancel_delete_key();
                        if let Ok(latest) = store.load() {
                            app.set_profiles(latest.profiles);
                        }
                        let _ = refresh_key_choices(&store, &mut app);
                        app.set_error(error.to_string());
                        false
                    }
                };
                if deleted && let Err(error) = refresh_key_choices(&store, &mut app) {
                    app.set_error(error.to_string());
                }
            }
            AppAction::Connect(id) => {
                connect(&store, &runner, &mut terminal, &mut app, &id, &shutdown)?;
            }
        }
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
    }

    Ok(())
}
