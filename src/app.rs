use crate::context::AppContext;
use crate::ui::main::MainScreen;
use crate::ui::{Screen, ScreenLike, ScreenType};
use eframe::{egui, App};
use std::sync::Arc;

pub struct AppState {
    pub main_screen: Screen,
    pub screen_stack: Vec<Screen>,
    pub app_context: Arc<AppContext>,
}

#[derive(Debug, PartialEq)]
pub enum DesiredAppAction {
    None,
    PopScreen,
    GoToMainScreen,
    AddScreenType(ScreenType),
}

impl DesiredAppAction {
    pub fn create_action(&self, app_context: &Arc<AppContext>) -> AppAction {
        match self {
            DesiredAppAction::None => AppAction::None,
            DesiredAppAction::PopScreen => AppAction::PopScreen,
            DesiredAppAction::GoToMainScreen => AppAction::GoToMainScreen,
            DesiredAppAction::AddScreenType(screen_type) => {
                AppAction::AddScreen(screen_type.create_screen(app_context))
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AppAction {
    None,
    PopScreen,
    PopScreenAndRefresh,
    GoToMainScreen,
    AddScreen(Screen),
}
impl AppState {
    pub fn new() -> Self {
        let app_context = Arc::new(AppContext::new());

        let main_screen = MainScreen::new(&app_context);

        Self {
            main_screen: Screen::MainScreen(main_screen),
            screen_stack: vec![],
            app_context,
        }
    }

    pub fn visible_screen(&self) -> &Screen {
        if let Some(last_screen) = self.screen_stack.last() {
            last_screen
        } else {
            &self.main_screen
        }
    }

    pub fn visible_screen_mut(&mut self) -> &mut Screen {
        if let Some(last_screen) = self.screen_stack.last_mut() {
            last_screen
        } else {
            &mut self.main_screen
        }
    }

    pub fn visible_screen_type(&self) -> ScreenType {
        if let Some(last_screen) = self.screen_stack.last() {
            last_screen.screen_type()
        } else {
            ScreenType::Main
        }
    }
}

impl App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let action = self.visible_screen_mut().ui(ctx);

        match action {
            AppAction::AddScreen(screen) => self.screen_stack.push(screen),
            AppAction::None => {}
            AppAction::PopScreen => {
                if !self.screen_stack.is_empty() {
                    self.screen_stack.pop();
                }
            }
            AppAction::PopScreenAndRefresh => {
                if !self.screen_stack.is_empty() {
                    self.screen_stack.pop();
                }
                if let Some(screen) = self.screen_stack.last_mut() {
                    screen.refresh();
                } else {
                    self.main_screen.refresh();
                }
            }
            AppAction::GoToMainScreen => {
                self.screen_stack = vec![];
                self.main_screen.refresh();
            }
        }
    }
}
