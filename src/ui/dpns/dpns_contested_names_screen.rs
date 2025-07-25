use std::sync::{Arc, Mutex};

use chrono::{DateTime, LocalResult, TimeZone, Utc};
use chrono_humanize::HumanTime;
use dash_sdk::dpp::identity::accessors::IdentityGettersV0;
use dash_sdk::dpp::platform_value::string_encoding::Encoding;
use dash_sdk::dpp::voting::vote_choices::resource_vote_choice::ResourceVoteChoice;
use dash_sdk::platform::Identifier;
use eframe::egui::{self, Button, Color32, ComboBox, Context, Label, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;

use crate::app::{AppAction, BackendTasksExecutionMode, DesiredAppAction};
use crate::backend_task::BackendTask;
use crate::backend_task::contested_names::{ContestedResourceTask, ScheduledDPNSVote};
use crate::backend_task::identity::IdentityTask;
use crate::context::AppContext;
use crate::model::contested_name::{ContestState, ContestedName};
use crate::model::qualified_identity::{DPNSNameInfo, QualifiedIdentity};
use crate::ui::components::dpns_subscreen_chooser_panel::add_dpns_subscreen_chooser_panel;
use crate::ui::components::left_panel::add_left_panel;
use crate::ui::components::styled::island_central_panel;
use crate::ui::components::top_panel::add_top_panel;
use crate::ui::theme::DashColors;
use crate::ui::{BackendTaskSuccessResult, MessageType, RootScreenType, ScreenLike, ScreenType};

/// Which DPNS sub-screen is currently showing.
#[derive(PartialEq)]
pub enum DPNSSubscreen {
    Active,
    Past,
    Owned,
    ScheduledVotes,
}

impl DPNSSubscreen {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active contests",
            Self::Past => "Past contests",
            Self::Owned => "My usernames",
            Self::ScheduledVotes => "Scheduled votes",
        }
    }
}

/// Minimal object for storing the user’s currently selected vote on a single contested name.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedVote {
    pub contested_name: String,
    pub vote_choice: ResourceVoteChoice,
    pub end_time: Option<u64>,
}

#[derive(Clone)]
pub enum VoteOption {
    NoVote,
    CastNow,
    Scheduled { days: u32, hours: u32, minutes: u32 },
}

/// Tracks the casting status for each scheduled vote item.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScheduledVoteCastingStatus {
    NotStarted,
    InProgress,
    Failed,
    Completed,
}

#[derive(PartialEq)]
pub enum VoteHandlingStatus {
    NotStarted,
    CastingVotes(u64),
    SchedulingVotes,
    Completed,
    Failed(String),
}

#[derive(PartialEq)]
pub enum RefreshingStatus {
    Refreshing(u64),
    NotRefreshing,
}

/// Sorting columns
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    ContestedName,
    LockedVotes,
    AbstainVotes,
    EndingTime,
    LastUpdated,
    AwardedTo,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortOrder {
    Ascending,
    Descending,
}

/// The main, combined DPNSScreen:
/// - Displays active/past/owned DPNS contests
/// - Allows clicking selection of votes (bulk scheduling)
/// - Allows single immediate vote or single schedule
/// - Shows scheduled votes listing
pub struct DPNSScreen {
    voting_identities: Vec<QualifiedIdentity>,
    user_identities: Vec<QualifiedIdentity>,
    contested_names: Arc<Mutex<Vec<ContestedName>>>,
    local_dpns_names: Arc<Mutex<Vec<(Identifier, DPNSNameInfo)>>>,
    pub scheduled_votes: Arc<Mutex<Vec<(ScheduledDPNSVote, ScheduledVoteCastingStatus)>>>,
    pub scheduled_vote_cast_in_progress: bool,
    pub selected_votes: Vec<SelectedVote>,
    pub app_context: Arc<AppContext>,
    message: Option<(String, MessageType, DateTime<Utc>)>,
    pending_backend_task: Option<BackendTask>,

    /// Sorting
    sort_column: SortColumn,
    sort_order: SortOrder,
    active_filter_term: String,
    past_filter_term: String,
    owned_filter_term: String,

    /// Which sub-screen is active: Active contests, Past, Owned, or Scheduled
    pub dpns_subscreen: DPNSSubscreen,
    refreshing_status: RefreshingStatus,

    /// Selected vote handling
    show_bulk_schedule_popup: bool,
    bulk_identity_options: Vec<VoteOption>,
    bulk_schedule_message: Option<(MessageType, String)>,
    bulk_vote_handling_status: VoteHandlingStatus,
    set_all_option: VoteOption,
}

impl DPNSScreen {
    pub fn new(app_context: &Arc<AppContext>, dpns_subscreen: DPNSSubscreen) -> Self {
        // Load contested names, local dpns, scheduled, etc.:
        let contested_names = Arc::new(Mutex::new(match dpns_subscreen {
            DPNSSubscreen::Active => app_context.ongoing_contested_names().unwrap_or_default(),
            DPNSSubscreen::Past => app_context.all_contested_names().unwrap_or_default(),
            DPNSSubscreen::Owned => Vec::new(),
            DPNSSubscreen::ScheduledVotes => Vec::new(),
        }));

        let local_dpns_names = Arc::new(Mutex::new(match dpns_subscreen {
            DPNSSubscreen::Active => Vec::new(),
            DPNSSubscreen::Past => Vec::new(),
            DPNSSubscreen::Owned => app_context.local_dpns_names().unwrap_or_default(),
            DPNSSubscreen::ScheduledVotes => Vec::new(),
        }));

        let scheduled_votes = app_context.get_scheduled_votes().unwrap_or_default();
        let scheduled_votes_with_status = Arc::new(Mutex::new(
            scheduled_votes
                .iter()
                .map(|vote| {
                    if vote.executed_successfully {
                        (vote.clone(), ScheduledVoteCastingStatus::Completed)
                    } else {
                        (vote.clone(), ScheduledVoteCastingStatus::NotStarted)
                    }
                })
                .collect::<Vec<_>>(),
        ));

        let voting_identities = app_context
            .db
            .get_local_voting_identities(app_context)
            .unwrap_or_default();
        let user_identities = app_context.load_local_user_identities().unwrap_or_default();

        // Initialize vote handling pop-up state to hidden
        let identity_count = voting_identities.len();
        let bulk_identity_options = vec![VoteOption::CastNow; identity_count];

        Self {
            voting_identities,
            user_identities,
            contested_names,
            local_dpns_names,
            scheduled_votes: scheduled_votes_with_status,
            selected_votes: Vec::new(),
            app_context: app_context.clone(),
            message: None,
            sort_column: SortColumn::ContestedName,
            sort_order: SortOrder::Ascending,
            active_filter_term: String::new(),
            past_filter_term: String::new(),
            owned_filter_term: String::new(),
            scheduled_vote_cast_in_progress: false,
            pending_backend_task: None,
            dpns_subscreen,
            refreshing_status: RefreshingStatus::NotRefreshing,

            // Vote handling
            show_bulk_schedule_popup: false,
            bulk_identity_options,
            bulk_schedule_message: None,
            bulk_vote_handling_status: VoteHandlingStatus::NotStarted,
            set_all_option: VoteOption::CastNow,
        }
    }

    // ---------------------------
    // Error handling
    // ---------------------------
    fn dismiss_message(&mut self) {
        self.message = None;
    }

    fn check_error_expiration(&mut self) {
        if let Some((_, _, timestamp)) = &self.message {
            let now = Utc::now();
            let elapsed = now.signed_duration_since(*timestamp);
            if elapsed.num_seconds() >= 10 {
                self.dismiss_message();
            }
        }
    }

    // ---------------------------
    // Sorting
    // ---------------------------
    fn toggle_sort(&mut self, column: SortColumn) {
        if self.sort_column == column {
            self.sort_order = match self.sort_order {
                SortOrder::Ascending => SortOrder::Descending,
                SortOrder::Descending => SortOrder::Ascending,
            };
        } else {
            self.sort_column = column;
            self.sort_order = SortOrder::Ascending;
        }
    }

    fn sort_contested_names(&self, contested_names: &mut [ContestedName]) {
        contested_names.sort_by(|a, b| {
            let order = match self.sort_column {
                SortColumn::ContestedName => a
                    .normalized_contested_name
                    .cmp(&b.normalized_contested_name),
                SortColumn::LockedVotes => a.locked_votes.cmp(&b.locked_votes),
                SortColumn::AbstainVotes => a.abstain_votes.cmp(&b.abstain_votes),
                SortColumn::EndingTime => a.end_time.cmp(&b.end_time),
                SortColumn::LastUpdated => a.last_updated.cmp(&b.last_updated),
                SortColumn::AwardedTo => a.awarded_to.cmp(&b.awarded_to),
            };
            if self.sort_order == SortOrder::Descending {
                order.reverse()
            } else {
                order
            }
        });
    }

    // ---------------------------
    // Rendering: Empty states
    // ---------------------------
    fn render_no_active_contests_or_owned_names(&mut self, ui: &mut Ui) -> AppAction {
        let mut app_action = AppAction::None;
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            match self.dpns_subscreen {
                DPNSSubscreen::Active => {
                    ui.label(
                        egui::RichText::new("No active contests at the moment.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
                DPNSSubscreen::Past => {
                    ui.label(
                        egui::RichText::new("No active or past contests at the moment.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
                DPNSSubscreen::Owned => {
                    ui.label(
                        egui::RichText::new("No owned usernames.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
                DPNSSubscreen::ScheduledVotes => {
                    ui.label(
                        egui::RichText::new("No scheduled votes.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
            }
            ui.add_space(10.0);

            if self.dpns_subscreen != DPNSSubscreen::ScheduledVotes {
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                ui.label(RichText::new("Please check back later or try refreshing the list.").color(DashColors::text_primary(dark_mode)));
                ui.add_space(20.0);
                if ui.button("Refresh").clicked() {
                    if let RefreshingStatus::Refreshing(_) = self.refreshing_status {
                        app_action = AppAction::None;
                    } else {
                        let now = Utc::now().timestamp() as u64;
                        self.refreshing_status = RefreshingStatus::Refreshing(now);
                        self.message = None; // Clear any existing message
                        match self.dpns_subscreen {
                            DPNSSubscreen::Active | DPNSSubscreen::Past => {
                                app_action = AppAction::BackendTask(BackendTask::ContestedResourceTask(
                                    ContestedResourceTask::QueryDPNSContests,
                                ));
                            }
                            DPNSSubscreen::Owned => {
                                app_action = AppAction::BackendTask(BackendTask::IdentityTask(
                                    IdentityTask::RefreshLoadedIdentitiesOwnedDPNSNames,
                                ));
                            }
                            _ => {
                                app_action = AppAction::Refresh;
                            }
                        }
                    }
                }
            } else {
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                let text_color = DashColors::text_primary(dark_mode);
                ui.label(
                    RichText::new("To schedule votes, go to the Active Contests subscreen, click your choices, and then click the 'Vote' button in the top-right.").color(text_color)
                );
            }
        });

        app_action
    }

    // ---------------------------
    // Rendering: Active, Past, Owned, Scheduled
    // ---------------------------

    /// Show the Active Contests table
    fn render_table_active_contests(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let dark_mode = ui.ctx().style().visuals.dark_mode;
            ui.label(RichText::new("Filter by name:").color(DashColors::text_primary(dark_mode)));
            ui.text_edit_singleline(&mut self.active_filter_term);
        });

        let contested_names = {
            let guard = self.contested_names.lock().unwrap();
            let mut cn = guard.clone();
            if !self.active_filter_term.is_empty() {
                let mut filter_lc = self.active_filter_term.to_lowercase();
                // Convert o and O to 0 and l to 1 in filter_lc
                filter_lc = filter_lc
                    .chars()
                    .map(|c| match c {
                        'o' | 'O' => '0',
                        'l' => '1',
                        _ => c,
                    })
                    .collect();
                cn.retain(|c| {
                    c.normalized_contested_name
                        .to_lowercase()
                        .contains(&filter_lc)
                });
            }
            self.sort_contested_names(&mut cn);
            cn
        };

        // Space allocation for UI elements is handled by the layout system

        egui::ScrollArea::both().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(200.0).resizable(true)) // Contested Name
                .column(Column::initial(100.0).resizable(true)) // Locked
                .column(Column::initial(100.0).resizable(true)) // Abstain
                .column(Column::initial(200.0).resizable(true)) // Ending Time
                .column(Column::initial(200.0).resizable(true)) // Last Updated
                .column(Column::remainder()) // Contestants
                .header(30.0, |mut header| {
                    header.col(|ui| {
                        if ui.button("Contested Name").clicked() {
                            self.toggle_sort(SortColumn::ContestedName);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Locked Votes").clicked() {
                            self.toggle_sort(SortColumn::LockedVotes);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Abstain Votes").clicked() {
                            self.toggle_sort(SortColumn::AbstainVotes);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Ending Time").clicked() {
                            self.toggle_sort(SortColumn::EndingTime);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Last Updated").clicked() {
                            self.toggle_sort(SortColumn::LastUpdated);
                        }
                    });
                    header.col(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.heading(
                            RichText::new("Contestants").color(DashColors::text_primary(dark_mode)),
                        );
                    });
                })
                .body(|mut body| {
                    for contested_name in &contested_names {
                        body.row(25.0, |mut row| {
                            let locked_votes = contested_name.locked_votes.unwrap_or(0);
                            let max_contestant_votes = contested_name
                                .contestants
                                .as_ref()
                                .map(|contestants| {
                                    contestants.iter().map(|c| c.votes).max().unwrap_or(0)
                                })
                                .unwrap_or(0);
                            let is_locked_votes_bold = locked_votes > max_contestant_votes;

                            // Contested Name
                            row.col(|ui| {
                                let (used_name, highlighted) =
                                    if let Some(contestants) = &contested_name.contestants {
                                        if let Some(first) = contestants.first() {
                                            if contestants.iter().all(|c| c.name == first.name) {
                                                // Everyone has same name
                                                (
                                                    first.name.clone(),
                                                    Some(
                                                        contested_name
                                                            .normalized_contested_name
                                                            .clone(),
                                                    ),
                                                )
                                            } else {
                                                // Multiple different names
                                                (
                                                    contestants
                                                        .iter()
                                                        .map(|c| c.name.clone())
                                                        .join(" or "),
                                                    Some(
                                                        contestants
                                                            .iter()
                                                            .map(|c| {
                                                                format!(
                                                                    "{} trying to get {}",
                                                                    c.id,
                                                                    c.name.clone()
                                                                )
                                                            })
                                                            .join(" and "),
                                                    ),
                                                )
                                            }
                                        } else {
                                            (contested_name.normalized_contested_name.clone(), None)
                                        }
                                    } else {
                                        (contested_name.normalized_contested_name.clone(), None)
                                    };

                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                let label_response = ui.label(
                                    RichText::new(used_name)
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                                if let Some(tooltip) = highlighted {
                                    label_response.on_hover_text(tooltip);
                                }
                            });

                            // LOCK button
                            row.col(|ui| {
                                let label_text = format!("{}", locked_votes);
                                let text_widget = if is_locked_votes_bold {
                                    RichText::new(label_text).strong()
                                } else {
                                    RichText::new(label_text)
                                };

                                // See if this (LOCK) is selected
                                let is_selected = self.selected_votes.iter().any(|sv| {
                                    sv.contested_name == contested_name.normalized_contested_name
                                        && sv.vote_choice == ResourceVoteChoice::Lock
                                });

                                let button = if is_selected {
                                    Button::new(text_widget).fill(Color32::from_rgb(0, 150, 255))
                                } else {
                                    Button::new(text_widget)
                                };
                                let resp = ui.add(button);
                                if resp.clicked() {
                                    // Is there already a selection for this contested name?
                                    if let Some(existing_index) =
                                        self.selected_votes.iter().position(|sv| {
                                            sv.contested_name
                                                == contested_name.normalized_contested_name
                                        })
                                    {
                                        // If the user clicked the same choice, that toggles it off (unselect).
                                        if self.selected_votes[existing_index].vote_choice
                                            == ResourceVoteChoice::Lock
                                        {
                                            // Remove it entirely -> no selection
                                            self.selected_votes.remove(existing_index);
                                        } else {
                                            // Otherwise replace the old choice with Lock
                                            self.selected_votes[existing_index].vote_choice =
                                                ResourceVoteChoice::Lock;
                                        }
                                    } else {
                                        // No existing selection for this name, so add this new Lock
                                        self.selected_votes.push(SelectedVote {
                                            contested_name: contested_name
                                                .normalized_contested_name
                                                .clone(),
                                            vote_choice: ResourceVoteChoice::Lock,
                                            end_time: contested_name.end_time,
                                        });
                                    }
                                }
                            });

                            // ABSTAIN button
                            row.col(|ui| {
                                let abstain_votes = contested_name.abstain_votes.unwrap_or(0);
                                let label_text = format!("{}", abstain_votes);

                                let is_selected = self.selected_votes.iter().any(|sv| {
                                    sv.contested_name == contested_name.normalized_contested_name
                                        && sv.vote_choice == ResourceVoteChoice::Abstain
                                });

                                let button = if is_selected {
                                    Button::new(label_text).fill(Color32::from_rgb(0, 150, 255))
                                } else {
                                    Button::new(label_text)
                                };
                                let resp = ui.add(button);
                                if resp.clicked() {
                                    // Is there already a selection for this contested name?
                                    if let Some(existing_index) =
                                        self.selected_votes.iter().position(|sv| {
                                            sv.contested_name
                                                == contested_name.normalized_contested_name
                                        })
                                    {
                                        // If the user clicked the same choice, that toggles it off (unselect).
                                        if self.selected_votes[existing_index].vote_choice
                                            == ResourceVoteChoice::Abstain
                                        {
                                            // Remove it entirely -> no selection
                                            self.selected_votes.remove(existing_index);
                                        } else {
                                            // Otherwise replace the old choice with Abstain
                                            self.selected_votes[existing_index].vote_choice =
                                                ResourceVoteChoice::Abstain;
                                        }
                                    } else {
                                        // No existing selection for this name, so add this new Abstain
                                        self.selected_votes.push(SelectedVote {
                                            contested_name: contested_name
                                                .normalized_contested_name
                                                .clone(),
                                            vote_choice: ResourceVoteChoice::Abstain,
                                            end_time: contested_name.end_time,
                                        });
                                    }
                                }
                            });

                            // Ending Time
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                if let Some(ending_time) = contested_name.end_time {
                                    if let LocalResult::Single(dt) =
                                        Utc.timestamp_millis_opt(ending_time as i64)
                                    {
                                        let iso_date = dt.format("%Y-%m-%d %H:%M:%S");
                                        let relative_time = HumanTime::from(dt).to_string();
                                        let text = format!("{} ({})", iso_date, relative_time);
                                        ui.label(
                                            RichText::new(text)
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new("Invalid timestamp")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Fetching")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                            });

                            // Last Updated
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                if let Some(last_updated) = contested_name.last_updated {
                                    if let LocalResult::Single(dt) =
                                        Utc.timestamp_opt(last_updated as i64, 0)
                                    {
                                        let rel_time = HumanTime::from(dt).to_string();
                                        if rel_time.contains("seconds") {
                                            ui.label(
                                                RichText::new("now")
                                                    .color(DashColors::text_primary(dark_mode)),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new(rel_time)
                                                    .color(DashColors::text_primary(dark_mode)),
                                            );
                                        }
                                    } else {
                                        ui.label(
                                            RichText::new("Invalid timestamp")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Fetching")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                            });

                            // Contestants
                            row.col(|ui| {
                                self.show_contestants_for_contested_name(
                                    ui,
                                    contested_name,
                                    is_locked_votes_bold,
                                    max_contestant_votes,
                                );
                            });
                        });
                    }
                });
        });
    }

    /// Show a Past Contests table
    fn render_table_past_contests(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let dark_mode = ui.ctx().style().visuals.dark_mode;
            ui.label(RichText::new("Filter by name:").color(DashColors::text_primary(dark_mode)));
            ui.text_edit_singleline(&mut self.past_filter_term);
        });

        let contested_names = {
            let guard = self.contested_names.lock().unwrap();
            let mut cn = guard.clone();
            cn.retain(|c| c.awarded_to.is_some() || c.state == ContestState::Locked);
            // 1) Filter by `past_filter_term`
            if !self.past_filter_term.is_empty() {
                let mut filter_lc = self.past_filter_term.to_lowercase();
                // Convert o and O to 0 and l to 1 in filter_lc
                filter_lc = filter_lc
                    .chars()
                    .map(|c| match c {
                        'o' | 'O' => '0',
                        'l' => '1',
                        _ => c,
                    })
                    .collect();

                cn.retain(|c| {
                    c.normalized_contested_name
                        .to_lowercase()
                        .contains(&filter_lc)
                });
            }
            self.sort_contested_names(&mut cn);
            cn
        };

        // Allocate space for refreshing indicator
        // Space allocation for UI elements is handled by the layout system

        egui::ScrollArea::both().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(200.0).resizable(true)) // Name
                .column(Column::initial(200.0).resizable(true)) // Ended Time
                .column(Column::initial(200.0).resizable(true)) // Last Updated
                .column(Column::initial(200.0).resizable(true)) // Awarded To
                .header(30.0, |mut header| {
                    header.col(|ui| {
                        if ui.button("Contested Name").clicked() {
                            self.toggle_sort(SortColumn::ContestedName);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Ended Time").clicked() {
                            self.toggle_sort(SortColumn::EndingTime);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Last Updated").clicked() {
                            self.toggle_sort(SortColumn::LastUpdated);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Awarded To").clicked() {
                            self.toggle_sort(SortColumn::AwardedTo);
                        }
                    });
                })
                .body(|mut body| {
                    for contested_name in &contested_names {
                        body.row(25.0, |mut row| {
                            // Name
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.label(
                                    RichText::new(&contested_name.normalized_contested_name)
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                            });
                            // Ended Time
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                if let Some(ended_time) = contested_name.end_time {
                                    if let LocalResult::Single(dt) =
                                        Utc.timestamp_millis_opt(ended_time as i64)
                                    {
                                        let iso = dt.format("%Y-%m-%d %H:%M:%S").to_string();
                                        let relative = HumanTime::from(dt).to_string();
                                        ui.label(
                                            RichText::new(format!("{} ({})", iso, relative))
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new("Invalid timestamp")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Fetching")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                            });
                            // Last Updated
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                if let Some(last_updated) = contested_name.last_updated {
                                    if let LocalResult::Single(dt) =
                                        Utc.timestamp_opt(last_updated as i64, 0)
                                    {
                                        let rel = HumanTime::from(dt).to_string();
                                        if rel.contains("seconds") {
                                            ui.label(
                                                RichText::new("now")
                                                    .color(DashColors::text_primary(dark_mode)),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new(rel)
                                                    .color(DashColors::text_primary(dark_mode)),
                                            );
                                        }
                                    } else {
                                        ui.label(
                                            RichText::new("Invalid timestamp")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Fetching")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                            });
                            // Awarded To
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                match contested_name.state {
                                    ContestState::Unknown => {
                                        ui.label(
                                            RichText::new("Fetching")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                    ContestState::Joinable | ContestState::Ongoing => {
                                        ui.label(
                                            RichText::new("Active")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                    ContestState::WonBy(identifier) => {
                                        ui.add(
                                            egui::Label::new(
                                                identifier.to_string(Encoding::Base58),
                                            )
                                            .sense(egui::Sense::hover())
                                            .truncate(),
                                        );
                                    }
                                    ContestState::Locked => {
                                        ui.label(
                                            RichText::new("Locked")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                }
                            });
                        });
                    }
                });
        });
    }

    /// Show the Owned DPNS names table
    fn render_table_local_dpns_names(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let dark_mode = ui.ctx().style().visuals.dark_mode;
            ui.label(RichText::new("Filter by name:").color(DashColors::text_primary(dark_mode)));
            ui.text_edit_singleline(&mut self.owned_filter_term);
        });

        let mut filtered_names = {
            let guard = self.local_dpns_names.lock().unwrap();
            let mut name_infos = guard.clone();
            if !self.owned_filter_term.is_empty() {
                let filter_lc = self.owned_filter_term.to_lowercase();
                name_infos.retain(|c| c.1.name.to_lowercase().contains(&filter_lc));
            }
            name_infos
        };
        // Sort
        filtered_names.sort_by(|a, b| match self.sort_column {
            SortColumn::ContestedName => {
                let order = a.1.name.cmp(&b.1.name);
                if self.sort_order == SortOrder::Descending {
                    order.reverse()
                } else {
                    order
                }
            }
            SortColumn::AwardedTo => {
                let order = a.0.cmp(&b.0);
                if self.sort_order == SortOrder::Descending {
                    order.reverse()
                } else {
                    order
                }
            }
            SortColumn::EndingTime => {
                let order = a.1.acquired_at.cmp(&b.1.acquired_at);
                if self.sort_order == SortOrder::Descending {
                    order.reverse()
                } else {
                    order
                }
            }
            _ => std::cmp::Ordering::Equal,
        });

        // Space allocation for UI elements is handled by the layout system

        egui::ScrollArea::both().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(200.0).resizable(true)) // DPNS Name
                .column(Column::initial(400.0).resizable(true)) // Owner ID
                .column(Column::initial(300.0).resizable(true)) // Acquired At
                .header(30.0, |mut header| {
                    header.col(|ui| {
                        if ui.button("Name").clicked() {
                            self.toggle_sort(SortColumn::ContestedName);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Owner ID").clicked() {
                            self.toggle_sort(SortColumn::AwardedTo);
                        }
                    });
                    header.col(|ui| {
                        if ui.button("Acquired At").clicked() {
                            self.toggle_sort(SortColumn::EndingTime);
                        }
                    });
                })
                .body(|mut body| {
                    for (identifier, dpns_info) in filtered_names {
                        body.row(25.0, |mut row| {
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.label(
                                    RichText::new(dpns_info.name)
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                            });
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.label(
                                    RichText::new(identifier.to_string(Encoding::Base58))
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                            });
                            let dt = DateTime::from_timestamp(
                                dpns_info.acquired_at as i64 / 1000,
                                ((dpns_info.acquired_at % 1000) * 1_000_000) as u32,
                            )
                            .map(|dt| dt.to_string())
                            .unwrap_or_else(|| "Invalid timestamp".to_string());
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.label(
                                    RichText::new(dt).color(DashColors::text_primary(dark_mode)),
                                );
                            });
                        });
                    }
                });
        });
    }

    /// Show the Scheduled Votes table
    fn render_table_scheduled_votes(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;
        let mut sorted_votes = {
            let guard = self.scheduled_votes.lock().unwrap();
            guard.clone()
        };
        // Sort by contested_name or time
        sorted_votes.sort_by(|a, b| {
            let order = a.0.contested_name.cmp(&b.0.contested_name);
            if self.sort_order == SortOrder::Descending {
                order.reverse()
            } else {
                order
            }
        });

        egui::ScrollArea::both().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(100.0).resizable(true)) // ContestedName
                .column(Column::initial(200.0).resizable(true)) // Voter
                .column(Column::initial(200.0).resizable(true)) // Choice
                .column(Column::initial(200.0).resizable(true)) // Time
                .column(Column::initial(100.0).resizable(true)) // Status
                .column(Column::initial(100.0).resizable(true)) // Actions
                .header(30.0, |mut header| {
                    header.col(|ui| {
                        if ui.button("Contested Name").clicked() {
                            self.toggle_sort(SortColumn::ContestedName);
                        }
                    });
                    header.col(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.heading(
                            RichText::new("Voter").color(DashColors::text_primary(dark_mode)),
                        );
                    });
                    header.col(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.heading(
                            RichText::new("Vote Choice").color(DashColors::text_primary(dark_mode)),
                        );
                    });
                    header.col(|ui| {
                        if ui.button("Scheduled Time").clicked() {
                            self.toggle_sort(SortColumn::EndingTime);
                        }
                    });
                    header.col(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.heading(
                            RichText::new("Status").color(DashColors::text_primary(dark_mode)),
                        );
                    });
                    header.col(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.heading(
                            RichText::new("Actions").color(DashColors::text_primary(dark_mode)),
                        );
                    });
                })
                .body(|mut body| {
                    for vote in sorted_votes.iter_mut() {
                        body.row(25.0, |mut row| {
                            // Contested name
                            row.col(|ui| {
                                ui.add(Label::new(&vote.0.contested_name));
                            });
                            // Voter
                            row.col(|ui| {
                                ui.add(
                                    Label::new(vote.0.voter_id.to_string(Encoding::Hex)).truncate(),
                                );
                            });
                            // Choice
                            row.col(|ui| {
                                let display_text = match &vote.0.choice {
                                    ResourceVoteChoice::TowardsIdentity(id) => {
                                        id.to_string(Encoding::Base58)
                                    }
                                    other => other.to_string(),
                                };
                                ui.add(Label::new(display_text));
                            });
                            // Time
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                if let LocalResult::Single(dt) =
                                    Utc.timestamp_millis_opt(vote.0.unix_timestamp as i64)
                                {
                                    let iso = dt.format("%Y-%m-%d %H:%M:%S").to_string();
                                    let rel_time = HumanTime::from(dt).to_string();
                                    let relative = if rel_time.contains("seconds") {
                                        "now".to_string()
                                    } else {
                                        rel_time
                                    };
                                    let text = format!("{} ({})", iso, relative);
                                    ui.label(
                                        RichText::new(text)
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new("Invalid timestamp")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                            });
                            // Status
                            row.col(|ui| {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                match vote.1 {
                                    ScheduledVoteCastingStatus::NotStarted => {
                                        ui.label(
                                            RichText::new("Pending")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                    ScheduledVoteCastingStatus::InProgress => {
                                        ui.label(
                                            RichText::new("Casting...")
                                                .color(DashColors::text_primary(dark_mode)),
                                        );
                                    }
                                    ScheduledVoteCastingStatus::Failed => {
                                        ui.colored_label(Color32::DARK_RED, "Failed");
                                    }
                                    ScheduledVoteCastingStatus::Completed => {
                                        ui.colored_label(Color32::DARK_GREEN, "Casted");
                                    }
                                }
                            });
                            // Actions
                            row.col(|ui| {
                                if ui.button("Remove").clicked() {
                                    action =
                                        AppAction::BackendTask(BackendTask::ContestedResourceTask(
                                            ContestedResourceTask::DeleteScheduledVote(
                                                vote.0.voter_id,
                                                vote.0.contested_name.clone(),
                                            ),
                                        ));
                                }
                                // If the user wants to do "Cast Now" from here, they can
                                // if NotStarted or Failed. If in progress or done, disabled.
                                let cast_button_enabled = matches!(
                                    vote.1,
                                    ScheduledVoteCastingStatus::NotStarted
                                        | ScheduledVoteCastingStatus::Failed
                                ) && !self
                                    .scheduled_vote_cast_in_progress;

                                let cast_button = if cast_button_enabled {
                                    Button::new("Cast Now")
                                } else {
                                    Button::new("Cast Now").sense(egui::Sense::hover())
                                };

                                if ui.add(cast_button).clicked() && cast_button_enabled {
                                    self.scheduled_vote_cast_in_progress = true;
                                    vote.1 = ScheduledVoteCastingStatus::InProgress;

                                    // Mark in our Arc as well
                                    if let Ok(mut sched_guard) = self.scheduled_votes.lock() {
                                        if let Some(t) = sched_guard.iter_mut().find(|(sv, _)| {
                                            sv.voter_id == vote.0.voter_id
                                                && sv.contested_name == vote.0.contested_name
                                        }) {
                                            t.1 = ScheduledVoteCastingStatus::InProgress;
                                        }
                                    }
                                    // dispatch the actual cast
                                    let local_ids =
                                        match self.app_context.load_local_voting_identities() {
                                            Ok(ids) => ids,
                                            Err(e) => {
                                                eprintln!("Error: {}", e);
                                                return;
                                            }
                                        };
                                    if let Some(found) = local_ids
                                        .iter()
                                        .find(|i| i.identity.id() == vote.0.voter_id)
                                    {
                                        action = AppAction::BackendTask(
                                            BackendTask::ContestedResourceTask(
                                                ContestedResourceTask::CastScheduledVote(
                                                    vote.0.clone(),
                                                    Box::new(found.clone()),
                                                ),
                                            ),
                                        );
                                    }
                                }
                            });
                        });
                    }
                });
        });

        action
    }

    /// For each contested name row, show the possible contestants. This is the old `show_contested_name_details` function.
    fn show_contestants_for_contested_name(
        &mut self,
        ui: &mut Ui,
        contested_name: &ContestedName,
        is_locked_votes_bold: bool,
        max_contestant_votes: u32,
    ) {
        if let Some(contestants) = &contested_name.contestants {
            for contestant in contestants {
                let first_6_chars: String = contestant
                    .id
                    .to_string(Encoding::Base58)
                    .chars()
                    .take(6)
                    .collect();
                let button_text = format!("{}... - {} votes", first_6_chars, contestant.votes);

                // Bold if highest
                let text = if contestant.votes == max_contestant_votes && !is_locked_votes_bold {
                    RichText::new(button_text)
                        .strong()
                        .color(Color32::from_rgb(0, 100, 0))
                } else {
                    RichText::new(button_text)
                };

                // Check if selected
                let is_selected = self.selected_votes.iter().any(|sv| {
                    sv.contested_name == contested_name.normalized_contested_name
                        && sv.vote_choice == ResourceVoteChoice::TowardsIdentity(contestant.id)
                });

                let button = if is_selected {
                    Button::new(text).fill(Color32::from_rgb(0, 150, 255))
                } else {
                    Button::new(text)
                };
                let resp = ui.add(button);
                if resp.clicked() {
                    // Is there already a selection for this contested name?
                    if let Some(existing_index) = self.selected_votes.iter().position(|sv| {
                        sv.contested_name == contested_name.normalized_contested_name
                    }) {
                        // If the user clicked the same choice, that toggles it off (unselect).
                        if self.selected_votes[existing_index].vote_choice
                            == ResourceVoteChoice::TowardsIdentity(contestant.id)
                        {
                            // Remove it entirely -> no selection
                            self.selected_votes.remove(existing_index);
                        } else {
                            // Otherwise replace the old choice with TowardsIdentity
                            self.selected_votes[existing_index].vote_choice =
                                ResourceVoteChoice::TowardsIdentity(contestant.id);
                        }
                    } else {
                        // No existing selection for this name, so add this new TowardsIdentity
                        self.selected_votes.push(SelectedVote {
                            contested_name: contested_name.normalized_contested_name.clone(),
                            vote_choice: ResourceVoteChoice::TowardsIdentity(contestant.id),
                            end_time: contested_name.end_time,
                        });
                    }
                }
            }
        }
    }

    // ---------------------------
    // Bulk scheduling ephemeral UI
    // ---------------------------
    fn show_bulk_schedule_popup_window(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;

        let dark_mode = ui.ctx().style().visuals.dark_mode;
        ui.heading(
            RichText::new("Cast or Schedule Votes").color(DashColors::text_primary(dark_mode)),
        );
        ui.add_space(10.0);

        // If self.bulk_vote_handling_status is Complete, show completed message
        if self.bulk_vote_handling_status == VoteHandlingStatus::Completed {
            action |= self.show_bulk_vote_handling_complete(ui);
            return action;
        }

        // If no voting identities are loaded, display a message and return
        if self.voting_identities.is_empty() {
            ui.add_space(5.0);
            ui.colored_label(Color32::DARK_RED, "No masternode identities loaded. Please go to the Identities screen to load your masternodes.");
            ui.add_space(10.0);
            if ui.button("Close").clicked() {
                self.show_bulk_schedule_popup = false;
            }
            return action;
        }

        // If no votes are selected, display a message and return
        if self.selected_votes.is_empty() {
            ui.add_space(5.0);
            ui.colored_label(Color32::DARK_RED, "No votes selected. Please click the votes you want to cast or schedule in the Active Contests screen.");
            ui.add_space(10.0);
            if ui.button("Close").clicked() {
                self.show_bulk_schedule_popup = false;
            }
            return action;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Show which votes were clicked
            ui.group(|ui| {
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                ui.heading(
                    RichText::new("Selected Votes:").color(DashColors::text_primary(dark_mode)),
                );
                ui.separator();
                for sv in &self.selected_votes {
                    // Convert end_time -> readable
                    let end_str = if let Some(e) = sv.end_time {
                        if let LocalResult::Single(dt) = Utc.timestamp_millis_opt(e as i64) {
                            let iso = dt.format("%Y-%m-%d %H:%M:%S").to_string();
                            let rel = HumanTime::from(dt).to_string();
                            format!("{} ({})", iso, rel)
                        } else {
                            "Invalid timestamp".to_string()
                        }
                    } else {
                        "N/A".to_string()
                    };
                    let display_text = match &sv.vote_choice {
                        ResourceVoteChoice::TowardsIdentity(id) => id.to_string(Encoding::Base58),
                        other => other.to_string(),
                    };
                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                    ui.label(
                        RichText::new(format!(
                            "{}   =>   {}   |   Contest ends at {}",
                            sv.contested_name, display_text, end_str
                        ))
                        .color(DashColors::text_primary(dark_mode)),
                    );
                }
            });

            ui.add_space(10.0);

            // Show each identity + let user pick None / Immediate / Scheduled
            let dark_mode = ui.ctx().style().visuals.dark_mode;
            ui.heading(
                RichText::new("Select cast method for each node:")
                    .color(DashColors::text_primary(dark_mode)),
            );
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                    ui.label(RichText::new("Set all:").color(DashColors::text_primary(dark_mode)));

                    // A ComboBox to pick No Vote / Cast Now / Schedule
                    ComboBox::from_id_salt("set_all_combo")
                        .width(120.0)
                        .selected_text(match self.set_all_option {
                            VoteOption::NoVote => "No Vote".to_string(),
                            VoteOption::CastNow => "Cast Now".to_string(),
                            VoteOption::Scheduled { .. } => "Schedule".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    matches!(self.set_all_option, VoteOption::NoVote),
                                    "No Vote",
                                )
                                .clicked()
                            {
                                self.set_all_option = VoteOption::NoVote;
                            }
                            if ui
                                .selectable_label(
                                    matches!(self.set_all_option, VoteOption::CastNow),
                                    "Cast Now",
                                )
                                .clicked()
                            {
                                self.set_all_option = VoteOption::CastNow;
                            }
                            if ui
                                .selectable_label(
                                    matches!(self.set_all_option, VoteOption::Scheduled { .. }),
                                    "Schedule",
                                )
                                .clicked()
                            {
                                // Default scheduled values if none set yet
                                let (d, h, m) = match &self.set_all_option {
                                    VoteOption::Scheduled {
                                        days,
                                        hours,
                                        minutes,
                                    } => (*days, *hours, *minutes),
                                    _ => (0, 0, 0),
                                };
                                self.set_all_option = VoteOption::Scheduled {
                                    days: d,
                                    hours: h,
                                    minutes: m,
                                };
                            }
                        });

                    // If scheduling, show the days/hours/minutes widgets inline
                    if let VoteOption::Scheduled {
                        ref mut days,
                        ref mut hours,
                        ref mut minutes,
                    } = self.set_all_option
                    {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.label(
                            RichText::new("Schedule In:")
                                .color(DashColors::text_primary(dark_mode)),
                        );
                        ui.add(egui::DragValue::new(days).prefix("Days: ").range(0..=14));
                        ui.add(egui::DragValue::new(hours).prefix("Hours: ").range(0..=23));
                        ui.add(egui::DragValue::new(minutes).prefix("Min: ").range(0..=59));
                    }

                    // Button to apply the "Set all" choice to each identity in bulk_identity_options
                    if ui.button("Apply to All").clicked() {
                        for option in &mut self.bulk_identity_options {
                            *option = self.set_all_option.clone();
                        }
                    }
                });
            });
            ui.add_space(10.0);
            for (i, identity) in self.voting_identities.iter().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let label = identity
                            .alias
                            .clone()
                            .unwrap_or_else(|| identity.identity.id().to_string(Encoding::Base58));
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.label(
                            RichText::new(format!("Identity: {}", label))
                                .color(DashColors::text_primary(dark_mode)),
                        );

                        // This is a hack
                        // I'm seeing a panic if I load the app in mainnet context where I have no voting identities,
                        // and then switch to testnet and pressed "Vote".
                        if self.bulk_identity_options.len() <= i {
                            let voting_identities = self
                                .app_context
                                .db
                                .get_local_voting_identities(&self.app_context)
                                .unwrap_or_default();
                            // Initialize ephemeral bulk-schedule state to hidden
                            let identity_count = voting_identities.len();
                            self.bulk_identity_options = vec![VoteOption::CastNow; identity_count];
                        }

                        let current_option = &mut self.bulk_identity_options[i];
                        ComboBox::from_id_salt(format!("combo_bulk_identity_{}", i))
                            .width(120.0)
                            .selected_text(match current_option {
                                VoteOption::NoVote => "No Vote".to_string(),
                                VoteOption::CastNow => "Cast Now".to_string(),
                                VoteOption::Scheduled { .. } => "Schedule".to_string(),
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        matches!(current_option, VoteOption::NoVote),
                                        "No Vote",
                                    )
                                    .clicked()
                                {
                                    *current_option = VoteOption::NoVote;
                                }
                                if ui
                                    .selectable_label(
                                        matches!(current_option, VoteOption::CastNow),
                                        "Cast Now",
                                    )
                                    .clicked()
                                {
                                    *current_option = VoteOption::CastNow;
                                }
                                if ui
                                    .selectable_label(
                                        matches!(current_option, VoteOption::Scheduled { .. }),
                                        "Schedule",
                                    )
                                    .clicked()
                                {
                                    let (d, h, m) = match current_option {
                                        VoteOption::Scheduled {
                                            days,
                                            hours,
                                            minutes,
                                        } => (*days, *hours, *minutes),
                                        _ => (0, 0, 0),
                                    };
                                    *current_option = VoteOption::Scheduled {
                                        days: d,
                                        hours: h,
                                        minutes: m,
                                    };
                                }
                            });

                        if let VoteOption::Scheduled {
                            days,
                            hours,
                            minutes,
                        } = current_option
                        {
                            let dark_mode = ui.ctx().style().visuals.dark_mode;
                            ui.label(
                                RichText::new("Schedule In:")
                                    .color(DashColors::text_primary(dark_mode)),
                            );
                            ui.add(egui::DragValue::new(days).prefix("Days: ").range(0..=14));
                            ui.add(egui::DragValue::new(hours).prefix("Hours: ").range(0..=23));
                            ui.add(egui::DragValue::new(minutes).prefix("Min: ").range(0..=59));
                        }
                    });
                });
                ui.add_space(10.0);
            }
        });

        // If any selected votes are scheduled, show a warning
        if self
            .bulk_identity_options
            .iter()
            .any(|o| matches!(o, VoteOption::Scheduled { .. }))
        {
            ui.colored_label(Color32::DARK_RED, "NOTE: Dash Evo Tool must remain running and connected for scheduled votes to execute on time.");
            ui.add_space(10.0);
        }

        // "Apply Votes" button
        let button = egui::Button::new(RichText::new("Apply Votes").color(Color32::WHITE))
            .fill(Color32::from_rgb(0, 128, 255))
            .corner_radius(3.0);
        if ui.add(button).clicked() {
            action = self.bulk_apply_votes();
        }

        ui.add_space(5.0);
        if ui.button("Cancel").clicked() {
            self.selected_votes.clear();
            self.show_bulk_schedule_popup = false;
            self.bulk_schedule_message = None;
            self.bulk_vote_handling_status = VoteHandlingStatus::NotStarted;
        }

        // Handle status
        ui.add_space(10.0);
        match &self.bulk_vote_handling_status {
            VoteHandlingStatus::NotStarted => {}
            VoteHandlingStatus::CastingVotes(start_time) => {
                let now = Utc::now().timestamp() as u64;
                let elapsed = now - start_time;
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                ui.label(
                    RichText::new(format!("Casting votes... Time taken so far: {}", elapsed))
                        .color(DashColors::text_primary(dark_mode)),
                );
            }
            VoteHandlingStatus::SchedulingVotes => {
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                ui.label(
                    RichText::new("Scheduling votes...").color(DashColors::text_primary(dark_mode)),
                );
            }
            VoteHandlingStatus::Completed => {
                // handled above
            }
            VoteHandlingStatus::Failed(message) => {
                ui.colored_label(
                    Color32::RED,
                    format!("Error casting/scheduling votes: {}", message),
                );
            }
        }

        action
    }

    /// The logic that was in BulkScheduleVoteScreen::schedule_votes
    fn bulk_apply_votes(&mut self) -> AppAction {
        // Partition immediate vs scheduled
        let mut immediate_list = Vec::new();
        let mut scheduled_list = Vec::new();

        for (identity, option) in self
            .voting_identities
            .iter()
            .zip(&self.bulk_identity_options)
        {
            match option {
                VoteOption::NoVote => {}
                VoteOption::CastNow => {
                    immediate_list.push(identity.clone());
                }
                VoteOption::Scheduled {
                    days,
                    hours,
                    minutes,
                } => {
                    let now = Utc::now();
                    let offset = chrono::Duration::days(*days as i64)
                        + chrono::Duration::hours(*hours as i64)
                        + chrono::Duration::minutes(*minutes as i64);
                    let scheduled_time = (now + offset).timestamp_millis() as u64;

                    for sv in &self.selected_votes {
                        let new_vote = ScheduledDPNSVote {
                            contested_name: sv.contested_name.clone(),
                            voter_id: identity.identity.id(),
                            choice: sv.vote_choice,
                            unix_timestamp: scheduled_time,
                            executed_successfully: false,
                        };
                        scheduled_list.push(new_vote);
                    }
                }
            }
        }

        if immediate_list.is_empty() && scheduled_list.is_empty() {
            self.bulk_vote_handling_status = VoteHandlingStatus::Failed(
                "No votes selected. Please select votes to cast or schedule.".to_string(),
            );
            return AppAction::None;
        }

        // 1) If immediate_list is not empty, vote now, possibly scheduling votes as well
        if !immediate_list.is_empty() {
            let votes_for_all: Vec<(String, ResourceVoteChoice)> = self
                .selected_votes
                .iter()
                .map(|sv| (sv.contested_name.clone(), sv.vote_choice))
                .collect();
            let now = Utc::now().timestamp() as u64;
            self.bulk_vote_handling_status = VoteHandlingStatus::CastingVotes(now);
            if !scheduled_list.is_empty() {
                AppAction::BackendTasks(
                    vec![
                        BackendTask::ContestedResourceTask(ContestedResourceTask::VoteOnDPNSNames(
                            votes_for_all,
                            immediate_list,
                        )),
                        BackendTask::ContestedResourceTask(
                            ContestedResourceTask::ScheduleDPNSVotes(scheduled_list),
                        ),
                    ],
                    BackendTasksExecutionMode::Concurrent,
                )
            } else {
                AppAction::BackendTask(BackendTask::ContestedResourceTask(
                    ContestedResourceTask::VoteOnDPNSNames(votes_for_all, immediate_list),
                ))
            }
        } else {
            // 2) Otherwise just schedule them
            self.bulk_vote_handling_status = VoteHandlingStatus::SchedulingVotes;
            AppAction::BackendTask(BackendTask::ContestedResourceTask(
                ContestedResourceTask::ScheduleDPNSVotes(scheduled_list),
            ))
        }
    }

    /// If voting/scheduling is successful, show success message
    fn show_bulk_vote_handling_complete(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;

        self.selected_votes.clear();

        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            match &self.bulk_vote_handling_status {
                VoteHandlingStatus::Completed => {
                    // This means DET side was successful, but Platform may have returned errors
                    if let Some(message) = &self.bulk_schedule_message {
                        match message.0 {
                            MessageType::Error => {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.heading(
                                    RichText::new("❌").color(DashColors::text_primary(dark_mode)),
                                );
                                if message.1.contains("Successes") {
                                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                                    ui.heading(
                                        RichText::new("Only some votes succeeded")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                } else {
                                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                                    ui.heading(
                                        RichText::new("No votes succeeded")
                                            .color(DashColors::text_primary(dark_mode)),
                                    );
                                }
                                ui.add_space(10.0);
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.label(
                                    RichText::new(message.1.clone())
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                            }
                            MessageType::Success => {
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.heading(
                                    RichText::new("🎉").color(DashColors::text_primary(dark_mode)),
                                );
                                let dark_mode = ui.ctx().style().visuals.dark_mode;
                                ui.heading(
                                    RichText::new("Successfully casted and scheduled all votes")
                                        .color(DashColors::text_primary(dark_mode)),
                                );
                            }
                            _ => {}
                        }
                    }
                }
                VoteHandlingStatus::Failed(message) => {
                    // This means there was a DET-side error, not Platform-side
                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                    ui.heading(RichText::new("❌").color(DashColors::text_primary(dark_mode)));
                    ui.heading(
                        RichText::new("Error casting and scheduling votes (DET-side)")
                            .color(DashColors::text_primary(dark_mode)),
                    );
                    ui.add_space(10.0);
                    ui.label(RichText::new(message).color(DashColors::text_primary(dark_mode)));
                }
                _ => {
                    // this should not occur
                }
            }

            ui.add_space(20.0);
            if ui.button("Go back to Active Contests").clicked() {
                self.bulk_vote_handling_status = VoteHandlingStatus::NotStarted;
                self.show_bulk_schedule_popup = false;
                action = AppAction::BackendTask(BackendTask::ContestedResourceTask(
                    ContestedResourceTask::QueryDPNSContests,
                ))
            }
            ui.add_space(5.0);
            if ui.button("Go to Scheduled Votes Screen").clicked() {
                self.show_bulk_schedule_popup = false;
                self.bulk_vote_handling_status = VoteHandlingStatus::NotStarted;
                action = AppAction::SetMainScreenThenPopScreen(
                    RootScreenType::RootScreenDPNSScheduledVotes,
                );
            }
        });

        action
    }
}

// ---------------------------
// ScreenLike implementation
// ---------------------------
impl ScreenLike for DPNSScreen {
    fn refresh(&mut self) {
        self.scheduled_vote_cast_in_progress = false;
        let mut contested_names = self.contested_names.lock().unwrap();
        let mut dpns_names = self.local_dpns_names.lock().unwrap();
        let mut scheduled_votes = self.scheduled_votes.lock().unwrap();

        match self.dpns_subscreen {
            DPNSSubscreen::Active => {
                *contested_names = self
                    .app_context
                    .ongoing_contested_names()
                    .unwrap_or_default();
            }
            DPNSSubscreen::Past => {
                *contested_names = self.app_context.all_contested_names().unwrap_or_default();
            }
            DPNSSubscreen::Owned => {
                *dpns_names = self.app_context.local_dpns_names().unwrap_or_default();
            }
            DPNSSubscreen::ScheduledVotes => {
                let new_scheduled = self.app_context.get_scheduled_votes().unwrap_or_default();
                *scheduled_votes = new_scheduled
                    .iter()
                    .map(|newv| {
                        if newv.executed_successfully {
                            (newv.clone(), ScheduledVoteCastingStatus::Completed)
                        } else if let Some(existing) = scheduled_votes.iter().find(|(old, _)| {
                            old.contested_name == newv.contested_name
                                && old.voter_id == newv.voter_id
                        }) {
                            // preserve old status if InProgress/Failed
                            match existing.1 {
                                ScheduledVoteCastingStatus::InProgress => {
                                    (newv.clone(), ScheduledVoteCastingStatus::InProgress)
                                }
                                ScheduledVoteCastingStatus::Failed => {
                                    (newv.clone(), ScheduledVoteCastingStatus::Failed)
                                }
                                _ => (newv.clone(), ScheduledVoteCastingStatus::NotStarted),
                            }
                        } else {
                            (newv.clone(), ScheduledVoteCastingStatus::NotStarted)
                        }
                    })
                    .collect();
            }
        }
    }

    fn refresh_on_arrival(&mut self) {
        self.voting_identities = self
            .app_context
            .db
            .get_local_voting_identities(&self.app_context)
            .unwrap_or_default();
        self.user_identities = self
            .app_context
            .load_local_user_identities()
            .unwrap_or_default();
        self.refresh();
    }

    fn display_message(&mut self, message: &str, message_type: MessageType) {
        // Sync error states
        if message.contains("Error casting scheduled vote") {
            self.scheduled_vote_cast_in_progress = false;
            if let Ok(mut guard) = self.scheduled_votes.lock() {
                for vote in guard.iter_mut() {
                    if vote.1 == ScheduledVoteCastingStatus::InProgress {
                        vote.1 = ScheduledVoteCastingStatus::Failed;
                    }
                }
            }
        }
        if message.contains("Successfully cast scheduled vote") {
            self.scheduled_vote_cast_in_progress = false;
        }
        // If it's from a DPNS query or identity refresh, remove refreshing state
        if message.contains("Successfully refreshed DPNS contests")
            || message.contains("Successfully refreshed loaded identities dpns names")
            || message.contains("Contested resource query failed")
            || message.contains("Error refreshing owned DPNS names")
        {
            self.refreshing_status = RefreshingStatus::NotRefreshing;
        }

        if message.contains("Votes scheduled")
            && self.bulk_vote_handling_status == VoteHandlingStatus::SchedulingVotes
        {
            self.bulk_vote_handling_status = VoteHandlingStatus::Completed;
        }

        // Save into general error_message for top-of-screen
        self.message = Some((message.to_string(), message_type, Utc::now()));
    }

    fn display_task_result(&mut self, backend_task_success_result: BackendTaskSuccessResult) {
        match backend_task_success_result {
            // If immediate cast finished, see if we have pending to schedule next
            BackendTaskSuccessResult::DPNSVoteResults(results) => {
                let errors: Vec<String> = results
                    .iter()
                    .filter_map(|(_, _, r)| r.as_ref().err().cloned())
                    .collect();
                let successes: Vec<String> = results
                    .iter()
                    .filter_map(|(name, _, r)| r.as_ref().ok().map(|_| name.clone()))
                    .collect();

                if !errors.is_empty() {
                    let errors_string = errors.join("\n\n");
                    if !successes.is_empty() {
                        // partial success
                        self.bulk_schedule_message = Some((
                            MessageType::Error,
                            format!(
                                "Successes: {}/{}\n\nErrors:\n\n{:?}",
                                successes.len(),
                                successes.len() + errors.len(),
                                errors_string
                            ),
                        ));
                    } else {
                        // all failed
                        self.bulk_schedule_message =
                            Some((MessageType::Error, format!("Errors:\n\n{}", errors_string)));
                    }
                } else {
                    // no errors => all success
                    self.bulk_schedule_message = Some((
                        MessageType::Success,
                        "Votes all cast successfully.".to_string(),
                    ));
                }

                self.bulk_vote_handling_status = VoteHandlingStatus::Completed;
            }
            // If scheduling succeeded
            BackendTaskSuccessResult::Message(msg) => {
                if msg.contains("Votes scheduled") {
                    if self.bulk_vote_handling_status == VoteHandlingStatus::SchedulingVotes {
                        self.bulk_vote_handling_status = VoteHandlingStatus::Completed;
                    }
                    self.bulk_schedule_message =
                        Some((MessageType::Success, "Votes scheduled".to_string()));
                }
            }
            BackendTaskSuccessResult::CastScheduledVote(vote) => {
                if let Ok(mut guard) = self.scheduled_votes.lock() {
                    if let Some((_, status)) = guard.iter_mut().find(|(v, _)| {
                        v.contested_name == vote.contested_name && v.voter_id == vote.voter_id
                    }) {
                        *status = ScheduledVoteCastingStatus::Completed;
                    }
                }
            }
            _ => {}
        }
    }

    fn ui(&mut self, ctx: &Context) -> AppAction {
        self.check_error_expiration();
        let has_identity_that_can_register = !self.user_identities.is_empty();
        let has_active_contests = {
            let guard = self.contested_names.lock().unwrap();
            !guard.is_empty()
        };

        // Build top-right buttons
        let mut right_buttons = match self.dpns_subscreen {
            DPNSSubscreen::Active => {
                let refresh_button = (
                    "Refresh",
                    DesiredAppAction::BackendTask(Box::new(BackendTask::ContestedResourceTask(
                        ContestedResourceTask::QueryDPNSContests,
                    ))),
                );
                if has_active_contests {
                    vec![
                        refresh_button,
                        (
                            "Cast/Schedule Votes",
                            DesiredAppAction::Custom("Vote".to_string()),
                        ),
                    ]
                } else {
                    vec![refresh_button]
                }
            }
            DPNSSubscreen::Past => {
                let refresh_button = (
                    "Refresh",
                    DesiredAppAction::BackendTask(Box::new(BackendTask::ContestedResourceTask(
                        ContestedResourceTask::QueryDPNSContests,
                    ))),
                );
                vec![refresh_button]
            }
            DPNSSubscreen::Owned => {
                let refresh_button = (
                    "Refresh",
                    DesiredAppAction::BackendTask(Box::new(BackendTask::IdentityTask(
                        IdentityTask::RefreshLoadedIdentitiesOwnedDPNSNames,
                    ))),
                );
                vec![refresh_button]
            }
            DPNSSubscreen::ScheduledVotes => {
                vec![
                    (
                        "Clear All",
                        DesiredAppAction::BackendTask(Box::new(
                            BackendTask::ContestedResourceTask(
                                ContestedResourceTask::ClearAllScheduledVotes,
                            ),
                        )),
                    ),
                    (
                        "Clear Casted",
                        DesiredAppAction::BackendTask(Box::new(
                            BackendTask::ContestedResourceTask(
                                ContestedResourceTask::ClearExecutedScheduledVotes,
                            ),
                        )),
                    ),
                ]
            }
        };

        if has_identity_that_can_register && self.dpns_subscreen != DPNSSubscreen::ScheduledVotes {
            // "Register Name" button on the left
            right_buttons.insert(
                0,
                (
                    "Register Name",
                    DesiredAppAction::AddScreenType(Box::new(ScreenType::RegisterDpnsName)),
                ),
            );
        }

        let mut action = add_top_panel(
            ctx,
            &self.app_context,
            vec![("DPNS", AppAction::None)],
            right_buttons,
        );

        // If user clicked "Apply Votes" in the top bar
        if action == AppAction::Custom("Vote".to_string()) {
            // That means the user clicked "Apply Votes"
            self.show_bulk_schedule_popup = true;
            action = AppAction::None; // clear it out so we don't re-trigger
        }

        // Left panel
        match self.dpns_subscreen {
            DPNSSubscreen::Active => {
                action |= add_left_panel(
                    ctx,
                    &self.app_context,
                    RootScreenType::RootScreenDPNSActiveContests,
                );
            }
            DPNSSubscreen::Past => {
                action |= add_left_panel(
                    ctx,
                    &self.app_context,
                    RootScreenType::RootScreenDPNSPastContests,
                );
            }
            DPNSSubscreen::Owned => {
                action |= add_left_panel(
                    ctx,
                    &self.app_context,
                    RootScreenType::RootScreenDPNSOwnedNames,
                );
            }
            DPNSSubscreen::ScheduledVotes => {
                action |= add_left_panel(
                    ctx,
                    &self.app_context,
                    RootScreenType::RootScreenDPNSScheduledVotes,
                );
            }
        }

        // Subscreen chooser
        action |= add_dpns_subscreen_chooser_panel(ctx, self.app_context.as_ref());

        // Main panel
        action |= island_central_panel(ctx, |ui| {
            let mut inner_action = AppAction::None;
            // Bulk-schedule ephemeral popup
            if self.show_bulk_schedule_popup {
                egui::Window::new("Voting")
                    .collapsible(false)
                    .resizable(true)
                    .vscroll(true)
                    .show(ui.ctx(), |ui| {
                        inner_action |= self.show_bulk_schedule_popup_window(ui);
                    });
            }

            // Render sub-screen
            match self.dpns_subscreen {
                DPNSSubscreen::Active => {
                    let has_any = {
                        let guard = self.contested_names.lock().unwrap();
                        !guard.is_empty()
                    };
                    if has_any {
                        self.render_table_active_contests(ui);
                    } else {
                        inner_action |= self.render_no_active_contests_or_owned_names(ui);
                    }
                }
                DPNSSubscreen::Past => {
                    let has_any = {
                        let guard = self.contested_names.lock().unwrap();
                        !guard.is_empty()
                    };
                    if has_any {
                        self.render_table_past_contests(ui);
                    } else {
                        inner_action |= self.render_no_active_contests_or_owned_names(ui);
                    }
                }
                DPNSSubscreen::Owned => {
                    let has_any = {
                        let guard = self.local_dpns_names.lock().unwrap();
                        !guard.is_empty()
                    };
                    if has_any {
                        self.render_table_local_dpns_names(ui);
                    } else {
                        inner_action |= self.render_no_active_contests_or_owned_names(ui);
                    }
                }
                DPNSSubscreen::ScheduledVotes => {
                    let has_any = {
                        let guard = self.scheduled_votes.lock().unwrap();
                        !guard.is_empty()
                    };
                    if has_any {
                        inner_action |= self.render_table_scheduled_votes(ui);
                    } else {
                        inner_action |= self.render_no_active_contests_or_owned_names(ui);
                    }
                }
            }

            // Show either refreshing indicator or message, but not both
            if let RefreshingStatus::Refreshing(start_time) = self.refreshing_status {
                ui.add_space(25.0); // Space above
                let now = Utc::now().timestamp() as u64;
                let elapsed = now - start_time;
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    let dark_mode = ui.ctx().style().visuals.dark_mode;
                    ui.label(
                        RichText::new(format!("Refreshing... Time taken so far: {}", elapsed))
                            .color(DashColors::text_primary(dark_mode)),
                    );
                    ui.add(egui::widgets::Spinner::default().color(Color32::from_rgb(0, 128, 255)));
                });
                ui.add_space(2.0); // Space below
            } else if let Some((msg, msg_type, timestamp)) = self.message.clone() {
                ui.add_space(25.0); // Same space as refreshing indicator
                let dark_mode = ui.ctx().style().visuals.dark_mode;
                let color = match msg_type {
                    MessageType::Error => Color32::DARK_RED,
                    MessageType::Info => DashColors::text_primary(dark_mode),
                    MessageType::Success => Color32::DARK_GREEN,
                };
                ui.horizontal(|ui| {
                    ui.add_space(10.0);

                    // Calculate remaining seconds
                    let now = Utc::now();
                    let elapsed = now.signed_duration_since(timestamp);
                    let remaining = (10 - elapsed.num_seconds()).max(0);

                    // Add the message with auto-dismiss countdown
                    let full_msg = format!("{} ({}s)", msg, remaining);
                    ui.label(egui::RichText::new(full_msg).color(color));
                });
                ui.add_space(2.0); // Same space below as refreshing indicator
            }
            inner_action
        });

        // Extra handling for actions
        match action {
            // If refreshing contested names, set self.refreshing = true
            AppAction::BackendTask(BackendTask::ContestedResourceTask(
                ContestedResourceTask::QueryDPNSContests,
            )) => {
                self.refreshing_status =
                    RefreshingStatus::Refreshing(Utc::now().timestamp() as u64);
                self.message = None; // Clear any existing message
            }
            // If refreshing owned names, set self.refreshing = true
            AppAction::BackendTask(BackendTask::IdentityTask(
                IdentityTask::RefreshLoadedIdentitiesOwnedDPNSNames,
            )) => {
                self.refreshing_status =
                    RefreshingStatus::Refreshing(Utc::now().timestamp() as u64);
                self.message = None; // Clear any existing message
            }
            AppAction::SetMainScreen(_) => {
                self.refreshing_status = RefreshingStatus::NotRefreshing;
            }
            _ => {}
        }

        // If we have a pending backend task from scheduling (e.g. after immediate votes)
        if action == AppAction::None {
            if let Some(bt) = self.pending_backend_task.take() {
                action = AppAction::BackendTask(bt);
            }
        }
        action
    }
}
