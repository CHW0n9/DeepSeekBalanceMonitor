#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app {
    use chrono::{DateTime, Local};
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use imageproc::drawing::draw_text_mut;
    use native_windows_gui as nwg;
    use reqwest::StatusCode;
    use rusttype::{point, Font, Scale};
    use serde::{Deserialize, Serialize};
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use winreg::enums::*;
    use winreg::RegKey;

    const APP_NAME: &str = "DeepSeek Balance Monitor";
    const APP_ID: &str = "deepseek-balance-monitor";
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    #[derive(Clone, Serialize, Deserialize)]
    struct AppConfig {
        #[serde(default)]
        api_key: String,
        #[serde(default = "default_interval")]
        interval_minutes: u64,
        #[serde(default = "default_threshold")]
        threshold_yuan: f64,
        #[serde(default = "default_lang")]
        language: String,
        #[serde(default)]
        auto_start: bool,
        #[serde(default = "default_alerts")]
        enable_alerts: bool,
    }

    impl Default for AppConfig {
        fn default() -> Self {
            Self {
                api_key: String::new(),
                interval_minutes: default_interval(),
                threshold_yuan: default_threshold(),
                language: default_lang(),
                auto_start: false,
                enable_alerts: true,
            }
        }
    }

    fn default_interval() -> u64 {
        10
    }

    fn default_threshold() -> f64 {
        1.0
    }

    fn default_lang() -> String {
        "zh".to_string()
    }

    fn default_alerts() -> bool {
        true
    }

    #[derive(Clone, Debug)]
    struct Balance {
        total_balance: f64,
        granted_balance: f64,
        topped_up_balance: f64,
    }

    #[derive(Default)]
    struct RuntimeState {
        config: AppConfig,
        balances: BTreeMap<String, Balance>,
        last_check: Option<DateTime<Local>>,
        error: Option<String>,
        checking: bool,
    }

    enum UiMessage {
        CheckFinished(Result<BTreeMap<String, Balance>, String>),
    }

    #[derive(Deserialize)]
    struct ApiResponse {
        #[allow(dead_code)]
        #[serde(default)]
        is_available: bool,
        #[serde(default)]
        balance_infos: Vec<ApiBalanceInfo>,
    }

    #[derive(Deserialize)]
    struct ApiBalanceInfo {
        #[serde(default = "default_currency")]
        currency: String,
        #[serde(default)]
        total_balance: String,
        #[serde(default)]
        granted_balance: String,
        #[serde(default)]
        topped_up_balance: String,
    }

    fn default_currency() -> String {
        "CNY".to_string()
    }

    pub fn run() -> Result<(), String> {
        nwg::init().map_err(|e| e.to_string())?;
        let ui = AppUi::build().map_err(|e| e.to_string())?;
        log_line("Rust Windows app started");

        if ui.state.lock().unwrap().config.api_key.trim().is_empty() {
            ui.show_settings();
        }

        ui.start_check();
        nwg::dispatch_thread_events();
        log_line("Rust Windows app exited");
        Ok(())
    }

    struct AppUi {
        window: nwg::MessageWindow,
        tray: nwg::TrayNotification,
        tray_menu: nwg::Menu,
        view_item: nwg::MenuItem,
        check_item: nwg::MenuItem,
        settings_item: nwg::MenuItem,
        quit_item: nwg::MenuItem,
        notice: nwg::Notice,
        timer: nwg::AnimationTimer,
        icon: RefCell<nwg::Icon>,
        icon_path: PathBuf,
        state: Arc<Mutex<RuntimeState>>,
        tx: Sender<UiMessage>,
        rx: RefCell<Receiver<UiMessage>>,
        handlers: RefCell<Vec<nwg::EventHandler>>,
        settings: RefCell<Option<Rc<SettingsWindow>>>,
    }

    impl AppUi {
        fn build() -> Result<Rc<Self>, nwg::NwgError> {
            let config = load_config();
            let state = Arc::new(Mutex::new(RuntimeState {
                config: config.clone(),
                ..RuntimeState::default()
            }));
            let icon_path = config_dir().join("tray.ico");
            let _ = write_tray_icon(&icon_path, "...", false);

            let mut window = Default::default();
            let mut icon = Default::default();
            let mut tray = Default::default();
            let mut tray_menu = Default::default();
            let mut view_item = Default::default();
            let mut check_item = Default::default();
            let mut settings_item = Default::default();
            let mut quit_item = Default::default();
            let mut notice = Default::default();
            let mut timer = Default::default();

            nwg::MessageWindow::builder().build(&mut window)?;
            nwg::Icon::builder()
                .source_file(Some(path_text(&icon_path).as_str()))
                .build(&mut icon)?;
            nwg::TrayNotification::builder()
                .parent(&window)
                .icon(Some(&icon))
                .tip(Some(tr(&config.language, "checking")))
                .build(&mut tray)?;
            nwg::Menu::builder()
                .popup(true)
                .parent(&window)
                .build(&mut tray_menu)?;
            nwg::MenuItem::builder()
                .text(tr(&config.language, "view_balance"))
                .parent(&tray_menu)
                .build(&mut view_item)?;
            nwg::MenuItem::builder()
                .text(tr(&config.language, "check_now"))
                .parent(&tray_menu)
                .build(&mut check_item)?;
            nwg::MenuItem::builder()
                .text(tr(&config.language, "settings"))
                .parent(&tray_menu)
                .build(&mut settings_item)?;
            nwg::MenuItem::builder()
                .text(tr(&config.language, "quit"))
                .parent(&tray_menu)
                .build(&mut quit_item)?;
            nwg::Notice::builder().parent(&window).build(&mut notice)?;
            nwg::AnimationTimer::builder()
                .parent(&window)
                .interval(Duration::from_secs(config.interval_minutes.max(1) * 60))
                .build(&mut timer)?;

            let (tx, rx) = mpsc::channel();
            let ui = Rc::new(Self {
                window,
                tray,
                tray_menu,
                view_item,
                check_item,
                settings_item,
                quit_item,
                notice,
                timer,
                icon: RefCell::new(icon),
                icon_path,
                state,
                tx,
                rx: RefCell::new(rx),
                handlers: RefCell::new(Vec::new()),
                settings: RefCell::new(None),
            });

            let weak = Rc::downgrade(&ui);
            let handler =
                nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
                    if let Some(ui) = weak.upgrade() {
                        ui.handle_event(evt, handle);
                    }
                });
            ui.handlers.borrow_mut().push(handler);
            ui.timer.start();
            Ok(ui)
        }

        fn handle_event(self: &Rc<Self>, evt: nwg::Event, handle: nwg::ControlHandle) {
            match evt {
                nwg::Event::OnContextMenu if &handle == &self.tray => self.show_menu(),
                nwg::Event::OnMousePress(nwg::MousePressEvent::MousePressLeftUp)
                    if &handle == &self.tray =>
                {
                    self.show_balance()
                }
                nwg::Event::OnMenuItemSelected if &handle == &self.view_item => self.show_balance(),
                nwg::Event::OnMenuItemSelected if &handle == &self.check_item => self.start_check(),
                nwg::Event::OnMenuItemSelected if &handle == &self.settings_item => {
                    self.show_settings()
                }
                nwg::Event::OnMenuItemSelected if &handle == &self.quit_item => self.quit(),
                nwg::Event::OnNotice if &handle == &self.notice => self.process_messages(),
                nwg::Event::OnTimerTick if &handle == &self.timer => self.start_check(),
                _ => {}
            }
        }

        fn show_menu(&self) {
            let (x, y) = nwg::GlobalCursor::position();
            self.tray_menu.popup(x, y);
        }

        fn start_check(&self) {
            let config = {
                let mut state = self.state.lock().unwrap();
                if state.checking {
                    return;
                }
                state.checking = true;
                state.error = None;
                state.config.clone()
            };
            self.update_tray();

            let tx = self.tx.clone();
            let notice = self.notice.sender();
            thread::spawn(move || {
                let result = if config.api_key.trim().is_empty() {
                    Err("No API Key configured".to_string())
                } else {
                    fetch_balance(&config.api_key)
                };
                let _ = tx.send(UiMessage::CheckFinished(result));
                notice.notice();
            });
        }

        fn process_messages(&self) {
            while let Ok(message) = self.rx.borrow_mut().try_recv() {
                match message {
                    UiMessage::CheckFinished(result) => {
                        let mut should_notify = false;
                        {
                            let mut state = self.state.lock().unwrap();
                            state.checking = false;
                            match result {
                                Ok(balances) => {
                                    state.balances = balances;
                                    state.last_check = Some(Local::now());
                                    state.error = None;
                                    should_notify = is_low_balance(&state);
                                    log_line("Balance check succeeded");
                                }
                                Err(error) => {
                                    state.balances.clear();
                                    state.error = Some(error.clone());
                                    log_line(&format!("Balance check failed: {error}"));
                                }
                            }
                        }
                        self.update_tray();
                        if should_notify {
                            self.notify_low_balance();
                        }
                    }
                }
            }
        }

        fn update_tray(&self) {
            let (tooltip, label, low_balance) = {
                let state = self.state.lock().unwrap();
                let lang = state.config.language.as_str();
                if state.checking {
                    (tr(lang, "checking").to_string(), "...".to_string(), false)
                } else if let Some(error) = &state.error {
                    (
                        format!("{}: {}", tr(lang, "error"), error),
                        "!".to_string(),
                        false,
                    )
                } else if let Some((currency, balance)) = preferred_balance(&state.balances) {
                    (
                        format!(
                            "{}: {} {}",
                            tr(lang, "total_balance"),
                            format_amount(balance.total_balance),
                            currency
                        ),
                        icon_label(balance.total_balance),
                        is_low_balance(&state),
                    )
                } else {
                    (tr(lang, "checking").to_string(), "...".to_string(), false)
                }
            };

            self.tray.set_tip(&tooltip);
            if let Err(error) = write_tray_icon(&self.icon_path, &label, low_balance) {
                log_line(&format!("Icon update failed: {error}"));
                return;
            }

            let mut icon = Default::default();
            if nwg::Icon::builder()
                .source_file(Some(path_text(&self.icon_path).as_str()))
                .build(&mut icon)
                .is_ok()
            {
                self.tray.set_icon(&icon);
                *self.icon.borrow_mut() = icon;
            }
        }

        fn show_balance(&self) {
            let (title, message) = {
                let state = self.state.lock().unwrap();
                let lang = state.config.language.as_str();
                if let Some(error) = &state.error {
                    (
                        tr(lang, "balance_error_title").to_string(),
                        format!("{}: {}", tr(lang, "error"), error),
                    )
                } else if state.balances.is_empty() {
                    (
                        tr(lang, "balance_title").to_string(),
                        tr(lang, "balance_empty").to_string(),
                    )
                } else {
                    let mut lines = Vec::new();
                    for (code, balance) in &state.balances {
                        lines.push(format!(
                            "{}: {}  ({} {}, {} {})",
                            code,
                            format_amount(balance.total_balance),
                            tr(lang, "topped_up"),
                            format_amount(balance.topped_up_balance),
                            tr(lang, "granted"),
                            format_amount(balance.granted_balance)
                        ));
                    }
                    if let Some(last) = state.last_check {
                        lines.push(format!(
                            "{}: {}",
                            tr(lang, "last_check"),
                            last.format("%Y-%m-%d %H:%M:%S")
                        ));
                    }
                    let title = if let Some((code, balance)) = preferred_balance(&state.balances) {
                        format!(
                            "DeepSeek: {} {}",
                            format_amount(balance.total_balance),
                            code
                        )
                    } else {
                        tr(lang, "balance_title").to_string()
                    };
                    (title, lines.join("\n"))
                }
            };
            self.tray.show(&message, Some(&title), None, None);
        }

        fn notify_low_balance(&self) {
            let (enabled, title, message) = {
                let state = self.state.lock().unwrap();
                if !state.config.enable_alerts {
                    return;
                }
                let lang = state.config.language.as_str();
                if let Some((code, balance)) = preferred_balance(&state.balances) {
                    (
                        true,
                        tr(lang, "low_balance_title").to_string(),
                        format!(
                            "{} {} {}, {} {} {}",
                            tr(lang, "low_balance_body"),
                            format_amount(balance.total_balance),
                            code,
                            tr(lang, "threshold"),
                            format_amount(state.config.threshold_yuan),
                            code
                        ),
                    )
                } else {
                    (false, String::new(), String::new())
                }
            };
            if enabled {
                self.tray.show(&message, Some(&title), None, None);
            }
        }

        fn show_settings(self: &Rc<Self>) {
            if let Some(settings) = self.settings.borrow().as_ref() {
                settings.window.set_visible(true);
                settings.window.set_focus();
                return;
            }

            match SettingsWindow::build(self.clone()) {
                Ok(settings) => {
                    settings.window.set_visible(true);
                    settings.api_input.set_focus();
                    self.settings.borrow_mut().replace(settings);
                }
                Err(error) => log_line(&format!("Settings build failed: {error}")),
            }
        }

        fn settings_closed(&self) {
            self.settings.borrow_mut().take();
        }

        fn apply_config(&self, config: AppConfig) {
            if let Err(error) = save_config(&config) {
                log_line(&format!("Config save failed: {error}"));
            }
            if let Err(error) = set_auto_start(config.auto_start) {
                log_line(&format!("Auto-start update failed: {error}"));
            }
            {
                let mut state = self.state.lock().unwrap();
                state.config = config.clone();
            }
            self.timer
                .set_interval(Duration::from_secs(config.interval_minutes.max(1) * 60));
            self.timer.start();
            self.start_check();
        }

        fn quit(&self) {
            self.tray.set_visibility(false);
            nwg::stop_thread_dispatch();
        }
    }

    impl Drop for AppUi {
        fn drop(&mut self) {
            for handler in self.handlers.borrow_mut().drain(..) {
                nwg::unbind_event_handler(&handler);
            }
        }
    }

    struct SettingsWindow {
        window: nwg::Window,
        _api_label: nwg::Label,
        api_input: nwg::TextInput,
        show_key: nwg::CheckBox,
        _interval_label: nwg::Label,
        interval_input: nwg::TextInput,
        _threshold_label: nwg::Label,
        threshold_input: nwg::TextInput,
        _language_label: nwg::Label,
        language_combo: nwg::ComboBox<&'static str>,
        auto_start: nwg::CheckBox,
        enable_alerts: nwg::CheckBox,
        _status_label: nwg::Label,
        save_button: nwg::Button,
        cancel_button: nwg::Button,
        handler: RefCell<Option<nwg::EventHandler>>,
    }

    impl SettingsWindow {
        fn build(app: Rc<AppUi>) -> Result<Rc<Self>, nwg::NwgError> {
            let config = app.state.lock().unwrap().config.clone();
            let lang = config.language.as_str();
            let checked = nwg::CheckBoxState::Checked;
            let unchecked = nwg::CheckBoxState::Unchecked;

            let mut window = Default::default();
            let mut api_label = Default::default();
            let mut api_input = Default::default();
            let mut show_key = Default::default();
            let mut interval_label = Default::default();
            let mut interval_input = Default::default();
            let mut threshold_label = Default::default();
            let mut threshold_input = Default::default();
            let mut language_label = Default::default();
            let mut language_combo = Default::default();
            let mut auto_start = Default::default();
            let mut enable_alerts = Default::default();
            let mut status_label = Default::default();
            let mut save_button = Default::default();
            let mut cancel_button = Default::default();

            nwg::Window::builder()
                .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
                .size((520, 390))
                .center(true)
                .title(tr(lang, "settings_title"))
                .build(&mut window)?;
            nwg::Label::builder()
                .text(tr(lang, "api_key_label"))
                .position((20, 20))
                .size((460, 22))
                .parent(&window)
                .build(&mut api_label)?;
            nwg::TextInput::builder()
                .text(&config.api_key)
                .position((20, 48))
                .size((460, 28))
                .parent(&window)
                .focus(true)
                .build(&mut api_input)?;
            api_input.set_password_char(Some('*'));
            nwg::CheckBox::builder()
                .text(tr(lang, "show_key"))
                .position((20, 82))
                .size((180, 24))
                .parent(&window)
                .check_state(unchecked)
                .build(&mut show_key)?;
            nwg::Label::builder()
                .text(tr(lang, "interval_label"))
                .position((20, 120))
                .size((220, 22))
                .parent(&window)
                .build(&mut interval_label)?;
            nwg::TextInput::builder()
                .text(&config.interval_minutes.to_string())
                .position((250, 116))
                .size((100, 28))
                .parent(&window)
                .build(&mut interval_input)?;
            nwg::Label::builder()
                .text(tr(lang, "threshold_label"))
                .position((20, 158))
                .size((220, 22))
                .parent(&window)
                .build(&mut threshold_label)?;
            nwg::TextInput::builder()
                .text(&format!("{:.2}", config.threshold_yuan))
                .position((250, 154))
                .size((100, 28))
                .parent(&window)
                .build(&mut threshold_input)?;
            nwg::Label::builder()
                .text(tr(lang, "language_label"))
                .position((20, 196))
                .size((220, 22))
                .parent(&window)
                .build(&mut language_label)?;
            nwg::ComboBox::builder()
                .collection(vec!["中文", "English"])
                .selected_index(Some(if config.language == "en" { 1 } else { 0 }))
                .position((250, 192))
                .size((140, 100))
                .parent(&window)
                .build(&mut language_combo)?;
            nwg::CheckBox::builder()
                .text(tr(lang, "auto_start"))
                .position((20, 235))
                .size((220, 24))
                .parent(&window)
                .check_state(if config.auto_start || get_auto_start_state() {
                    checked
                } else {
                    unchecked
                })
                .build(&mut auto_start)?;
            nwg::CheckBox::builder()
                .text(tr(lang, "enable_alerts"))
                .position((250, 235))
                .size((220, 24))
                .parent(&window)
                .check_state(if config.enable_alerts {
                    checked
                } else {
                    unchecked
                })
                .build(&mut enable_alerts)?;

            let status = app.status_line();
            nwg::Label::builder()
                .text(&status)
                .position((20, 275))
                .size((460, 38))
                .parent(&window)
                .build(&mut status_label)?;
            nwg::Button::builder()
                .text(tr(lang, "save"))
                .position((300, 325))
                .size((86, 30))
                .parent(&window)
                .build(&mut save_button)?;
            nwg::Button::builder()
                .text(tr(lang, "cancel"))
                .position((395, 325))
                .size((86, 30))
                .parent(&window)
                .build(&mut cancel_button)?;

            let settings = Rc::new(Self {
                window,
                _api_label: api_label,
                api_input,
                show_key,
                _interval_label: interval_label,
                interval_input,
                _threshold_label: threshold_label,
                threshold_input,
                _language_label: language_label,
                language_combo,
                auto_start,
                enable_alerts,
                _status_label: status_label,
                save_button,
                cancel_button,
                handler: RefCell::new(None),
            });

            let weak_settings = Rc::downgrade(&settings);
            let weak_app = Rc::downgrade(&app);
            let handler =
                nwg::full_bind_event_handler(&settings.window.handle, move |evt, _data, handle| {
                    let Some(settings) = weak_settings.upgrade() else {
                        return;
                    };
                    let Some(app) = weak_app.upgrade() else {
                        return;
                    };
                    match evt {
                        nwg::Event::OnWindowClose if &handle == &settings.window => {
                            app.settings_closed()
                        }
                        nwg::Event::OnButtonClick if &handle == &settings.cancel_button => {
                            app.settings_closed()
                        }
                        nwg::Event::OnButtonClick if &handle == &settings.show_key => {
                            if settings.show_key.check_state() == nwg::CheckBoxState::Checked {
                                settings.api_input.set_password_char(None);
                            } else {
                                settings.api_input.set_password_char(Some('*'));
                            }
                        }
                        nwg::Event::OnButtonClick if &handle == &settings.save_button => {
                            match settings.read_config() {
                                Ok(config) => {
                                    app.apply_config(config);
                                    app.settings_closed();
                                }
                                Err(message) => {
                                    nwg::modal_error_message(
                                        &settings.window,
                                        tr("zh", "warn_title"),
                                        &message,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                });
            settings.handler.borrow_mut().replace(handler);
            Ok(settings)
        }

        fn read_config(&self) -> Result<AppConfig, String> {
            let api_key = self.api_input.text().trim().to_string();
            if api_key.is_empty() {
                return Err("API Key 不能为空".to_string());
            }
            let interval_minutes = self
                .interval_input
                .text()
                .trim()
                .parse::<u64>()
                .map_err(|_| "查询间隔必须是数字".to_string())?;
            if !(1..=1440).contains(&interval_minutes) {
                return Err("查询间隔必须在 1 到 1440 分钟之间".to_string());
            }
            let threshold_yuan = self
                .threshold_input
                .text()
                .trim()
                .parse::<f64>()
                .map_err(|_| "余额预警线必须是数字".to_string())?;
            if threshold_yuan < 0.0 {
                return Err("余额预警线不能小于 0".to_string());
            }
            Ok(AppConfig {
                api_key,
                interval_minutes,
                threshold_yuan,
                language: if self.language_combo.selection() == Some(1) {
                    "en".to_string()
                } else {
                    "zh".to_string()
                },
                auto_start: self.auto_start.check_state() == nwg::CheckBoxState::Checked,
                enable_alerts: self.enable_alerts.check_state() == nwg::CheckBoxState::Checked,
            })
        }
    }

    impl Drop for SettingsWindow {
        fn drop(&mut self) {
            if let Some(handler) = self.handler.borrow_mut().take() {
                nwg::unbind_event_handler(&handler);
            }
        }
    }

    impl AppUi {
        fn status_line(&self) -> String {
            let state = self.state.lock().unwrap();
            let lang = state.config.language.as_str();
            let last = state
                .last_check
                .map(|v| v.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| tr(lang, "not_checked").to_string());
            if let Some((code, balance)) = preferred_balance(&state.balances) {
                format!(
                    "{}: {} | {}: {} {}",
                    tr(lang, "last_check"),
                    last,
                    tr(lang, "total_balance"),
                    format_amount(balance.total_balance),
                    code
                )
            } else {
                format!("{}: {}", tr(lang, "last_check"), last)
            }
        }
    }

    fn fetch_balance(api_key: &str) -> Result<BTreeMap<String, Balance>, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let key = api_key.chars().filter(|c| c.is_ascii()).collect::<String>();
        let response = client
            .get("https://api.deepseek.com/user/balance")
            .header("Accept", "application/json")
            .bearer_auth(key)
            .send()
            .map_err(|e| e.to_string())?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err("Invalid API Key (401 Unauthorized)".to_string());
        }
        let payload: ApiResponse = response
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .map_err(|e| e.to_string())?;
        if payload.balance_infos.is_empty() {
            return Err("No balance information in response".to_string());
        }
        let mut balances = BTreeMap::new();
        for item in payload.balance_infos {
            balances.insert(
                item.currency,
                Balance {
                    total_balance: parse_amount(&item.total_balance),
                    granted_balance: parse_amount(&item.granted_balance),
                    topped_up_balance: parse_amount(&item.topped_up_balance),
                },
            );
        }
        Ok(balances)
    }

    fn parse_amount(value: &str) -> f64 {
        value.parse::<f64>().unwrap_or(0.0)
    }

    fn format_amount(value: f64) -> String {
        format!("{value:.2}")
    }

    fn preferred_balance(balances: &BTreeMap<String, Balance>) -> Option<(&String, &Balance)> {
        balances.iter().next()
    }

    fn is_low_balance(state: &RuntimeState) -> bool {
        preferred_balance(&state.balances)
            .map(|(_, balance)| balance.total_balance < state.config.threshold_yuan)
            .unwrap_or(false)
    }

    fn icon_label(value: f64) -> String {
        let int_value = value.max(0.0) as u64;
        if int_value <= 99 {
            int_value.to_string()
        } else {
            "OK".to_string()
        }
    }

    fn write_tray_icon(path: &Path, label: &str, low_balance: bool) -> Result<(), String> {
        ensure_dir(&config_dir()).map_err(|e| e.to_string())?;
        let fill = match label {
            "!" => Rgba([185, 70, 60, 255]),
            "..." => Rgba([105, 105, 110, 255]),
            _ if low_balance => Rgba([185, 70, 60, 255]),
            _ => Rgba([60, 105, 102, 255]),
        };
        let mut image = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        draw_rounded_square(&mut image, fill);
        if let Some(font) = load_font() {
            let font_size = if label.len() <= 1 {
                48.0
            } else if label.len() == 2 {
                44.0
            } else {
                34.0
            };
            let scale = Scale::uniform(font_size);
            let (x, y) = centered_text_position(&font, scale, label, 64, 64);
            draw_text_mut(
                &mut image,
                Rgba([255, 255, 255, 255]),
                x,
                y,
                scale,
                &font,
                label,
            );
        }
        DynamicImage::ImageRgba8(image)
            .save_with_format(path, ImageFormat::Ico)
            .map_err(|e| e.to_string())
    }

    fn draw_rounded_square(image: &mut RgbaImage, fill: Rgba<u8>) {
        let size = 64i32;
        let radius = 12i32;
        for y in 0..size {
            for x in 0..size {
                if inside_rounded_rect(x, y, size, radius) {
                    image.put_pixel(x as u32, y as u32, fill);
                }
            }
        }
    }

    fn inside_rounded_rect(x: i32, y: i32, size: i32, radius: i32) -> bool {
        let left = x < radius;
        let right = x >= size - radius;
        let top = y < radius;
        let bottom = y >= size - radius;
        if !(left || right) || !(top || bottom) {
            return true;
        }
        let cx = if left { radius } else { size - radius - 1 };
        let cy = if top { radius } else { size - radius - 1 };
        let dx = x - cx;
        let dy = y - cy;
        dx * dx + dy * dy <= radius * radius
    }

    fn load_font() -> Option<Font<'static>> {
        for path in [
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arialbd.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ] {
            if let Ok(bytes) = fs::read(path) {
                if let Some(font) = Font::try_from_vec(bytes) {
                    return Some(font);
                }
            }
        }
        None
    }

    fn centered_text_position(
        font: &Font<'_>,
        scale: Scale,
        text: &str,
        width: i32,
        height: i32,
    ) -> (i32, i32) {
        let glyphs: Vec<_> = font.layout(text, scale, point(0.0, 0.0)).collect();
        let mut min_x = 0;
        let mut min_y = 0;
        let mut max_x = 0;
        let mut max_y = 0;
        for bounds in glyphs.iter().filter_map(|g| g.pixel_bounding_box()) {
            min_x = min_x.min(bounds.min.x);
            min_y = min_y.min(bounds.min.y);
            max_x = max_x.max(bounds.max.x);
            max_y = max_y.max(bounds.max.y);
        }
        let text_width = max_x - min_x;
        let text_height = max_y - min_y;
        (
            (width - text_width) / 2 - min_x,
            (height - text_height) / 2 - min_y,
        )
    }

    fn config_dir() -> PathBuf {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                home.join("AppData").join("Roaming")
            })
            .join(APP_NAME)
    }

    fn config_file() -> PathBuf {
        config_dir().join("config.json")
    }

    fn log_file() -> PathBuf {
        config_dir().join("app.log")
    }

    fn ensure_dir(path: &Path) -> std::io::Result<()> {
        fs::create_dir_all(path)
    }

    fn load_config() -> AppConfig {
        let path = config_file();
        let mut config = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<AppConfig>(&text).ok())
            .unwrap_or_default();
        config.interval_minutes = config.interval_minutes.clamp(1, 1440);
        config
    }

    fn save_config(config: &AppConfig) -> std::io::Result<()> {
        ensure_dir(&config_dir())?;
        let file = File::create(config_file())?;
        serde_json::to_writer_pretty(file, config)?;
        Ok(())
    }

    fn log_line(message: &str) {
        if ensure_dir(&config_dir()).is_err() {
            return;
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file())
        {
            let _ = writeln!(
                file,
                "[{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                message
            );
        }
    }

    fn get_auto_start_state() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = hkcu.open_subkey(RUN_KEY) else {
            return false;
        };
        let Ok(value): Result<String, _> = key.get_value(APP_ID) else {
            return false;
        };
        let Ok(current) = std::env::current_exe() else {
            return false;
        };
        value == current.to_string_lossy()
    }

    fn set_auto_start(enable: bool) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            key.set_value(APP_ID, &exe.to_string_lossy().to_string())
                .map_err(|e| e.to_string())
        } else {
            match key.delete_value(APP_ID) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        }
    }

    fn path_text(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    fn tr(lang: &str, key: &str) -> &'static str {
        match (lang, key) {
            ("en", "checking") => "Checking...",
            ("en", "error") => "Error",
            ("en", "view_balance") => "View Balance",
            ("en", "check_now") => "Check Now",
            ("en", "settings") => "Settings...",
            ("en", "quit") => "Quit",
            ("en", "settings_title") => "DeepSeek Balance Monitor - Settings",
            ("en", "api_key_label") => "DeepSeek API Key:",
            ("en", "show_key") => "Show API Key",
            ("en", "interval_label") => "Check interval (minutes, 1-1440):",
            ("en", "threshold_label") => "Low balance threshold:",
            ("en", "language_label") => "Language:",
            ("en", "auto_start") => "Auto-start on boot",
            ("en", "enable_alerts") => "Enable balance alerts",
            ("en", "save") => "Save",
            ("en", "cancel") => "Cancel",
            ("en", "not_checked") => "Not checked",
            ("en", "total_balance") => "Total balance",
            ("en", "topped_up") => "Topped",
            ("en", "granted") => "Granted",
            ("en", "last_check") => "Last check",
            ("en", "balance_title") => "DeepSeek Balance",
            ("en", "balance_empty") => {
                "No balance data yet. Click Check Now or wait for the next check."
            }
            ("en", "balance_error_title") => "DeepSeek Balance - Error",
            ("en", "low_balance_title") => "DeepSeek Low Balance",
            ("en", "low_balance_body") => "Balance is only",
            ("en", "threshold") => "threshold",
            ("en", "warn_title") => "Warning",
            (_, "checking") => "查询中...",
            (_, "error") => "错误",
            (_, "view_balance") => "查看余额",
            (_, "check_now") => "立即查询",
            (_, "settings") => "设置...",
            (_, "quit") => "退出",
            (_, "settings_title") => "DeepSeek Balance Monitor - 设置",
            (_, "api_key_label") => "DeepSeek API Key:",
            (_, "show_key") => "显示 API Key",
            (_, "interval_label") => "查询间隔（分钟，1-1440）：",
            (_, "threshold_label") => "余额预警线：",
            (_, "language_label") => "语言 / Language:",
            (_, "auto_start") => "开机自动启动",
            (_, "enable_alerts") => "开启预警提醒",
            (_, "save") => "保存",
            (_, "cancel") => "取消",
            (_, "not_checked") => "尚未查询",
            (_, "total_balance") => "总余额",
            (_, "topped_up") => "充值",
            (_, "granted") => "赠送",
            (_, "last_check") => "上次查询",
            (_, "balance_title") => "DeepSeek 余额",
            (_, "balance_empty") => "尚未查询到余额，请稍后或点击立即查询。",
            (_, "balance_error_title") => "DeepSeek 余额 - 错误",
            (_, "low_balance_title") => "DeepSeek 余额不足",
            (_, "low_balance_body") => "当前余额仅剩",
            (_, "threshold") => "预警线",
            (_, "warn_title") => "警告",
            _ => "",
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("{error}");
    }
}

#[cfg(not(windows))]
fn main() {
    println!("This crate builds the Windows tray app. Use target x86_64-pc-windows-msvc.");
}
