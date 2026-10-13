const NAV_SCREENS: &[crate::navigation::Screen] = &[crate::navigation::Screen {
    name: "Overview",
    key: '1',
}];
impl App {
    pub fn navigation_skin(&self) -> crate::navigation::Skin {
        let t = &self.theme;
        crate::navigation::Skin {
            base: t.base(),
            muted: ratatui::style::Style::default().fg(t.muted),
            accent: ratatui::style::Style::default().fg(t.accent),
            selected: t.selected(),
            border: ratatui::style::Style::default().fg(t.border),
        }
    }
    pub fn navigation_screen(&self) -> usize {
        0
    }
    pub fn pane_names(&self) -> &'static [&'static str] {
        &["Loaded models", "Inspector"]
    }
    pub fn focused_pane(&self) -> usize {
        usize::from(self.inspect_focus)
    }
    pub fn navigation_editing(&self) -> bool {
        false
    }
    pub fn navigation_entries(&self) -> Vec<crate::navigation::Entry> {
        crate::navigation::entries(NAV_SCREENS, self.pane_names())
    }
    pub fn navigation_bar(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        self.navigation.bar(
            frame,
            area,
            NAV_SCREENS,
            self.navigation_screen(),
            self.pane_names(),
            self.focused_pane(),
            self.navigation_editing(),
            self.navigation_skin(),
        );
    }
    pub fn draw_navigation(&self, frame: &mut ratatui::Frame) {
        self.navigation
            .draw(frame, &self.navigation_entries(), self.navigation_skin());
    }
    pub fn focus_pane(&mut self, p: usize) {
        let p = p.min(self.pane_names().len().saturating_sub(1));
        let screen = self.navigation_screen();
        self.navigation.focus[screen] = p;
        self.inspect_focus = p == 1;
    }
    pub fn select_screen(&mut self, screen: usize) {
        if screen >= NAV_SCREENS.len() {
            return;
        }
        let old = self.navigation_screen();
        if old == screen {
            return;
        }
        self.navigation.focus[old] = self.focused_pane();
        self.navigation.scroll[old] = self.detail_scroll;
        self.detail_scroll = self.navigation.scroll[screen];
        self.focus_pane(self.navigation.focus[screen]);
    }
    fn navigation_key(&mut self, key: KeyEvent) -> bool {
        if self.commands.open {
            return false;
        }
        if self.navigation.open {
            let entries = self.navigation_entries();
            if let Some(intent) = self.navigation.key(key, &entries) {
                self.dispatch_intent(intent);
            }
            return true;
        }
        match key.code {
            KeyCode::F(2) => {
                self.navigation.toggle();
                true
            }
            KeyCode::F(6) => {
                let direction = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    -1
                } else {
                    1
                };
                self.focus_pane(crate::navigation::step(
                    self.focused_pane(),
                    self.pane_names().len(),
                    direction,
                ));
                true
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if !self.navigation_editing() {
                    let d = if key.code == KeyCode::Tab {
                        -1i32 + 2
                    } else {
                        -1
                    };
                    self.select_screen(crate::navigation::step(
                        self.navigation_screen(),
                        NAV_SCREENS.len(),
                        d,
                    ));
                }
                true
            }
            KeyCode::Char(c)
                if !self.navigation_editing()
                    && key.modifiers.is_empty()
                    && NAV_SCREENS.iter().any(|s| s.key == c) =>
            {
                let i = NAV_SCREENS.iter().position(|s| s.key == c).unwrap();
                self.select_screen(i);
                true
            }
            KeyCode::Esc => {
                let p = if self.navigation_editing()
                    && self.focused_pane() == 0
                    && self.pane_names().len() > 1
                {
                    1
                } else {
                    self.focused_pane().saturating_sub(1)
                };
                self.focus_pane(p);
                true
            }
            KeyCode::Down | KeyCode::Up | KeyCode::Char('j' | 'k')
                if !self.navigation_editing()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let d = if matches!(key.code, KeyCode::Down | KeyCode::Char('j')) {
                    1
                } else {
                    -1
                };
                self.navigation_scroll(self.focused_pane(), d);
                true
            }
            KeyCode::PageDown | KeyCode::PageUp
                if !self.navigation_editing()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let d = if key.code == KeyCode::PageDown { 8 } else { -8 };
                self.navigation_scroll(self.focused_pane(), d);
                true
            }
            KeyCode::F(1) => {
                self.mode = Mode::Help;
                true
            }
            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.navigation_cancel();
                true
            }
            _ => false,
        }
    }
    fn navigation_cancel(&mut self) {
        self.status = "No cancellable operation; use Actions for service/model controls".into();
    }
    pub fn dispatch_intent(&mut self, intent: crate::navigation::Intent) {
        use crate::navigation::Intent;
        match intent {
            Intent::Screen(screen) => {
                self.navigation.open = false;
                self.select_screen(screen)
            }
            Intent::Pane(p) => {
                self.navigation.open = false;
                self.focus_pane(p)
            }
            Intent::Menu => self.navigation.toggle(),
            Intent::Details(p) => self.focus_pane(p),
            Intent::ScrollDetails(p, d) => {
                self.focus_pane(p);
                self.navigation_details_scroll(p, d)
            }
            Intent::Selector(i) => {
                self.navigation.open = false;
                self.navigation_selector(i)
            }
            Intent::Theme(id) => self.apply_aurora_theme(id),
            Intent::Command(id) => self.navigation_command(id),
            Intent::Row(p, i) => {
                self.focus_pane(p);
                self.navigation_row(p, i, false)
            }
            Intent::Activate(p, i) => {
                self.focus_pane(p);
                self.navigation_row(p, i, true)
            }
            Intent::Scroll(p, d) => {
                self.focus_pane(p);
                self.navigation_scroll(p, d)
            }
        }
    }
    pub fn command_actions(&self) -> Vec<crate::commands::Action> {
        let mut actions = NAV_SCREENS
            .iter()
            .enumerate()
            .map(|(i, s)| crate::commands::Action::screen(s.name, i, s.key))
            .collect::<Vec<_>>();
        actions.extend(self.pane_names().iter().enumerate().map(|(i, p)| {
            crate::commands::Action::function(p, 6, crate::navigation::Intent::Pane(i))
        }));
        actions.push(crate::commands::Action::function(
            "Navigate",
            2,
            crate::navigation::Intent::Menu,
        ));
        actions.extend(
            ACTIONS
                .iter()
                .filter(|a| !matches!(a.key, KeyCode::Char('0'..='9')))
                .cloned(),
        );
        for action in &mut actions {
            let reason = match action.label {
                "Unload selected model" if self.selected_tenant().is_none() => {
                    Some("Select a loaded model")
                }
                "Kill selected process"
                    if self.selected_tenant().is_none_or(|t| {
                        !matches!(t.kind, TenantKind::Process | TenantKind::Orphan)
                    }) =>
                {
                    Some("Select an unmanaged GPU process")
                }
                _ => None,
            };
            if let Some(reason) = reason {
                action.disabled = Some(reason.into());
            }
        }
        actions
    }
}

impl App {
    fn navigation_command(&mut self, id: &str) {
        match id {
            "Refresh" => self.refresh(),
            "Unload selected model" => self.unload_selected(),
            "Load model" => self.open_load(),
            "Toggle Ollama" => self.toggle_ollama(),
            "Toggle LM Studio" => self.toggle_lms(),
            "Park other stack" => self.park_other(),
            "Kill selected process" => self.kill_selected(),
            "Help" => self.mode = Mode::Help,
            "Cancel" => self.mode = Mode::Normal,
            "Confirm" => {
                if let Mode::Confirm(c) = self.mode.clone() {
                    self.mode = Mode::Normal;
                    self.run_confirm(c.action);
                }
            }
            _ => {}
        }
    }
    fn navigation_row(&mut self, _pane: usize, index: usize, activate: bool) {
        if index < self.tenants.len() {
            self.selected = index;
            if activate {
                self.focus_pane(1);
            }
        }
    }
    fn navigation_scroll(&mut self, pane: usize, d: i32) {
        if pane == 0 {
            self.move_sel(d)
        } else if d < 0 {
            self.detail_scroll = self.detail_scroll.saturating_sub(3)
        } else {
            self.detail_scroll = self.detail_scroll.saturating_add(3)
        }
    }
    pub fn navigation_mouse(&mut self, event: crossterm::event::MouseEvent) {
        if matches!(self.mode, Mode::Load(_)) {
            if let Some(intent) = self.navigation.mouse(event) {
                match intent {
                    crate::navigation::Intent::Row(98, i) => {
                        if let Mode::Load(ui) = &mut self.mode {
                            ui.selected = i;
                        }
                    }
                    crate::navigation::Intent::Activate(98, i) => {
                        if let Mode::Load(ui) = &mut self.mode {
                            ui.selected = i;
                        }
                        self.submit_load();
                    }
                    crate::navigation::Intent::Scroll(98, d) => {
                        if let Mode::Load(ui) = &mut self.mode {
                            ui.selected = (ui.selected as i32 + d).clamp(
                                0,
                                crate::app::visible_load_items(ui).len().saturating_sub(1) as i32,
                            ) as usize;
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        if self.navigation.open {
            let entries = self.navigation_entries();
            if let Some(intent) = self.navigation.menu_mouse(event, &entries) {
                self.dispatch_intent(intent);
            }
            return;
        }
        if self.commands.open {
            if let Some(action) = self.commands.mouse(event, &self.command_actions()) {
                self.dispatch_intent(action.target);
            }
            return;
        }
        if !matches!(self.mode, Mode::Normal | Mode::Confirm(_)) {
            return;
        }
        if let Some(intent) = self.navigation.mouse(event) {
            if !matches!(self.mode, Mode::Normal)
                && !matches!(
                    intent,
                    crate::navigation::Intent::Command("Confirm" | "Cancel")
                )
            {
                return;
            }
            self.dispatch_intent(intent);
        }
    }
}

impl App {
    fn navigation_details_scroll(&mut self, _p: usize, d: i32) {
        self.detail_scroll = (self.detail_scroll as i32 + d * 3).clamp(0, u16::MAX as i32) as u16;
    }
    fn navigation_selector(&mut self, _i: usize) {}
}

impl App {
    pub fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.inspect_focus {
            vec![
                ("PgUp/PgDn", "scroll"),
                ("Ctrl+K", "actions"),
                ("F6", "models"),
            ]
        } else {
            vec![
                ("u", "unload"),
                ("Ctrl+K", "actions"),
                ("l", "load"),
                ("r", "refresh"),
                ("q", "quit"),
            ]
        }
    }
}
