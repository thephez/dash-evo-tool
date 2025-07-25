use crate::app::AppAction;
use crate::backend_task::BackendTask;
use crate::backend_task::tokens::TokenTask;
use crate::ui::Screen;
use crate::ui::components::styled::{StyledButton, ClickableCollapsingHeader};
use crate::ui::components::wallet_unlock::ScreenWithWalletUnlock;
use crate::ui::theme::DashColors;
use crate::ui::tokens::burn_tokens_screen::BurnTokensScreen;
use crate::ui::tokens::claim_tokens_screen::ClaimTokensScreen;
use crate::ui::tokens::destroy_frozen_funds_screen::DestroyFrozenFundsScreen;
use crate::ui::tokens::direct_token_purchase_screen::PurchaseTokenScreen;
use crate::ui::tokens::freeze_tokens_screen::FreezeTokensScreen;
use crate::ui::tokens::mint_tokens_screen::MintTokensScreen;
use crate::ui::tokens::pause_tokens_screen::PauseTokensScreen;
use crate::ui::tokens::resume_tokens_screen::ResumeTokensScreen;
use crate::ui::tokens::set_token_price_screen::SetTokenPriceScreen;
use crate::ui::tokens::tokens_screen::{
    IdentityTokenIdentifier, IdentityTokenInfo, IdentityTokenMaybeBalanceWithActions,
    RefreshingStatus, SortColumn, TokenInfoWithDataContract, TokensScreen, TokensSubscreen,
    get_available_token_actions_for_identity,
};
use crate::ui::tokens::transfer_tokens_screen::TransferTokensScreen;
use crate::ui::tokens::unfreeze_tokens_screen::UnfreezeTokensScreen;
use crate::ui::tokens::update_token_config::UpdateTokenConfigScreen;
use crate::ui::tokens::view_token_claims_screen::ViewTokenClaimsScreen;
use chrono::{Local, Utc};
use dash_sdk::dpp::data_contract::accessors::v0::DataContractV0Getters;
use dash_sdk::dpp::data_contract::associated_token::token_configuration::accessors::v0::TokenConfigurationV0Getters;
use dash_sdk::dpp::data_contract::associated_token::token_configuration_convention::accessors::v0::TokenConfigurationConventionV0Getters;
use dash_sdk::dpp::data_contract::associated_token::token_distribution_rules::accessors::v0::TokenDistributionRulesV0Getters;
use dash_sdk::dpp::platform_value::string_encoding::Encoding;
use dash_sdk::dpp::tokens::token_pricing_schedule::TokenPricingSchedule;
use eframe::emath::Align;
use eframe::epaint::Color32;
use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use std::ops::Range;

/// Get the minimum price for purchasing one token from a pricing schedule
fn get_min_token_price(pricing_schedule: &TokenPricingSchedule) -> u64 {
    match pricing_schedule {
        TokenPricingSchedule::SinglePrice(price) => *price,
        TokenPricingSchedule::SetPrices(price_map) => {
            // Return the price for the first tier (smallest amount)
            price_map.values().next().copied().unwrap_or(0)
        }
    }
}

impl TokensScreen {
    fn render_token_info_popup_content(&self, ui: &mut Ui, token_info: &TokenInfoWithDataContract) {
        let config = &token_info.token_configuration;

        ui.heading(&token_info.token_name);
        ui.separator();

        // Basic Information
        ClickableCollapsingHeader::new("Basic Information")
            .id_salt("token_basic_info_header")
            .show(ui, |ui| {
            egui::Grid::new("token_basic_info")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Description:");
                    ui.label(
                        token_info
                            .token_configuration
                            .description()
                            .as_ref()
                            .unwrap_or(&"No description".to_string()),
                    );
                    ui.end_row();

                    // Base Supply
                    ui.label("Base Supply:");
                    ui.label(config.base_supply().to_string());
                    ui.end_row();

                    // Max Supply
                    ui.label("Max Supply:");
                    if let Some(max_supply) = config.max_supply() {
                        ui.label(max_supply.to_string());
                    } else {
                        ui.label("Unlimited");
                    }
                    ui.end_row();

                    // Distribution info
                    ui.label("Perpetual Distribution:");
                    ui.label(
                        if config
                            .distribution_rules()
                            .perpetual_distribution()
                            .is_some()
                        {
                            "Yes"
                        } else {
                            "No"
                        },
                    );
                    ui.end_row();

                    ui.label("Preprogrammed Distribution:");
                    ui.label(
                        if config
                            .distribution_rules()
                            .pre_programmed_distribution()
                            .is_some()
                        {
                            "Yes"
                        } else {
                            "No"
                        },
                    );
                    ui.end_row();

                    ui.label("Token ID:");
                    ui.label(token_info.token_id.to_string(Encoding::Base58));
                    ui.end_row();

                    ui.label("Contract ID:");
                    ui.label(token_info.data_contract.id().to_string(Encoding::Base58));
                    ui.end_row();

                    ui.label("Contract Owner:");
                    ui.label(
                        token_info
                            .data_contract
                            .owner_id()
                            .to_string(Encoding::Base58),
                    );
                    ui.end_row();
                });
        });

        // Token Configuration Summary
        ClickableCollapsingHeader::new("Token Configuration")
            .id_salt("token_config_header")
            .show(ui, |ui| {
            ui.label("This token has the following configuration:");
            ui.add_space(5.0);

            // Show raw configuration as JSON for now
            if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.code(&json_str);
                    });
            } else {
                ui.label("Unable to display configuration details");
            }
        });
    }

    pub(super) fn render_my_tokens_subscreen(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;
        if self.all_known_tokens.is_empty() {
            // If no tokens, show a "no tokens found" message
            action |= self.render_no_owned_tokens(ui);
        } else {
            // Are we showing details for a selected token?
            if self.selected_token.is_some() {
                // Render detail view for one token
                action |= self.render_token_details(ui);
            } else {
                // Otherwise, show the list of all tokens
                match self.render_token_list(ui) {
                    Ok(list_action) => action |= list_action,
                    Err(e) => self.set_error_message(Some(e)),
                }
            }
        }

        // Show token info popup
        if let Some(token_id) = self.show_token_info_popup {
            if let Some(token_info) = self.all_known_tokens.get(&token_id).cloned() {
                let mut is_open = true;
                let mut close_popup = false;
                let dark_mode = ui.ctx().style().visuals.dark_mode;

                egui::Window::new("Token Configuration Details")
                    .resizable(true)
                    .collapsible(false)
                    .default_width(600.0)
                    .default_height(500.0)
                    .open(&mut is_open)
                    .show(ui.ctx(), |ui| {
                        // Add theme-aware background frame
                        egui::Frame::new()
                            .fill(DashColors::surface(dark_mode))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    self.render_token_info_popup_content(ui, &token_info);

                                    ui.separator();
                                    if ui.button("Close").clicked() {
                                        close_popup = true;
                                    }
                                });
                            });
                    });

                // Handle close actions
                if !is_open || close_popup {
                    self.show_token_info_popup = None;
                }
            } else {
                // Token not found, close popup
                self.show_token_info_popup = None;
            }
        }

        action
    }
    fn render_no_owned_tokens(&mut self, ui: &mut Ui) -> AppAction {
        let mut app_action = AppAction::None;
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            match self.tokens_subscreen {
                TokensSubscreen::MyTokens => {
                    ui.label(
                        RichText::new("No tracked tokens.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
                TokensSubscreen::SearchTokens => {
                    ui.label(
                        RichText::new("No matching tokens found.")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
                TokensSubscreen::TokenCreator => {
                    ui.label(
                        RichText::new("Cannot render token creator for some reason")
                            .heading()
                            .strong()
                            .color(Color32::GRAY),
                    );
                }
            }
            ui.add_space(10.0);

            let dark_mode = ui.ctx().style().visuals.dark_mode;
            ui.label(
                RichText::new("Please check back later or try refreshing the list.")
                    .color(DashColors::text_primary(dark_mode)),
            );
            ui.add_space(20.0);
            if StyledButton::primary("Refresh").show(ui).clicked() {
                if let RefreshingStatus::Refreshing(_) = self.refreshing_status {
                    app_action = AppAction::None;
                } else {
                    let now = Utc::now().timestamp() as u64;
                    self.refreshing_status = RefreshingStatus::Refreshing(now);
                    match self.tokens_subscreen {
                        TokensSubscreen::MyTokens => {
                            app_action = AppAction::BackendTask(BackendTask::TokenTask(Box::new(
                                TokenTask::QueryMyTokenBalances,
                            )));
                        }
                        TokensSubscreen::SearchTokens => {
                            app_action = AppAction::Refresh;
                        }
                        TokensSubscreen::TokenCreator => {
                            app_action = AppAction::Refresh;
                        }
                    }
                }
            }
        });

        app_action
    }

    /// Renders details for the selected token_id: a row per identity that holds that token.
    fn render_token_details(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;

        let Some(token_id) = self.selected_token else {
            return action;
        };

        let Some(token_info) = self.all_known_tokens.get(&token_id).cloned() else {
            return action;
        };

        let identities = &self.identities;

        let mut detail_list: Vec<IdentityTokenMaybeBalanceWithActions> = vec![];

        let in_dev_mode = self.app_context.is_developer_mode();

        for (identity_id, identity) in identities {
            let record = if let Some(known_token_balance) =
                self.my_tokens.get(&IdentityTokenIdentifier {
                    identity_id: *identity_id,
                    token_id,
                }) {
                IdentityTokenMaybeBalanceWithActions {
                    token_id,
                    token_alias: known_token_balance.token_alias.clone(),
                    token_config: known_token_balance.token_config.clone(),
                    identity_id: *identity_id,
                    identity_alias: identity.alias.clone(),
                    balance: Some(known_token_balance.balance),
                    estimated_unclaimed_rewards: known_token_balance.estimated_unclaimed_rewards,
                    data_contract_id: known_token_balance.data_contract_id,
                    token_position: known_token_balance.token_position,
                    available_actions: known_token_balance.available_actions,
                }
            } else {
                IdentityTokenMaybeBalanceWithActions {
                    token_id,
                    token_alias: token_info.token_name.clone(),
                    token_config: token_info.token_configuration.clone(),
                    identity_id: *identity_id,
                    identity_alias: identity.alias.clone(),
                    balance: None,
                    estimated_unclaimed_rewards: None,
                    data_contract_id: token_info.data_contract.id(),
                    token_position: token_info.token_position,
                    available_actions: get_available_token_actions_for_identity(
                        None,
                        identity,
                        &token_info.token_configuration,
                        &token_info.data_contract,
                        in_dev_mode,
                        self.token_pricing_data
                            .get(&token_id)
                            .and_then(|opt| opt.as_ref()),
                    ),
                }
            };
            detail_list.push(record);
        }

        // Space allocation for UI elements is handled by the layout system

        let in_dev_mode = self.app_context.is_developer_mode();

        let shows_estimation_column = in_dev_mode
            || token_info
                .token_configuration
                .distribution_rules()
                .perpetual_distribution()
                .is_some();

        // A simple table with columns: [Token Name | Token ID | Total Balance]
        egui::ScrollArea::both()
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_max_width(ui.available_width());
                let mut table = TableBuilder::new(ui)
                            .striped(false)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(Align::Center))
                            .column(Column::initial(60.0).resizable(true)) // Identity Alias
                            .column(Column::initial(200.0).resizable(true)) // Identity ID
                            .column(Column::initial(60.0).resizable(true)); // Balance


                        if shows_estimation_column {
                            table = table.column(Column::initial(85.0).resizable(true)); // Estimated Rewards
                        }

                        table = table.column(Column::initial(200.0).resizable(true));// Actions
                        table.header(30.0, |mut header| {
                            header.col(|ui| {
                                if ui.button("Identity Alias").clicked() {
                                    self.toggle_sort(SortColumn::OwnerIdentityAlias);
                                }
                            });
                            header.col(|ui| {
                                if ui.button("Identity ID").clicked() {
                                    self.toggle_sort(SortColumn::OwnerIdentity);
                                }
                            });
                            header.col(|ui| {
                                if ui.button("Balance").clicked() {
                                    self.toggle_sort(SortColumn::Balance);
                                }
                            });
                            if shows_estimation_column {
                                header.col(|ui| {
                                    ui.label("Rewards");
                                });
                            }
                            header.col(|ui| {
                                ui.label("Actions");
                            });
                        })
                            .body(|mut body| {
                                for itb in &detail_list {
                                    body.row(30.0, |mut row| {
                                        row.col(|ui| {
                                            // Show identity alias or ID
                                            if let Some(alias) = self
                                                .app_context
                                                .get_identity_alias(&itb.identity_id)
                                                .expect("Expected to get alias")
                                            {
                                                ui.label(alias);
                                            } else {
                                                ui.label("-");
                                            }
                                        });
                                        row.col(|ui| {
                                            if itb.identity_id == token_info.data_contract.owner_id() {
                                                ui.label(
                                                    RichText::new(itb.identity_id.to_string(Encoding::Base58))
                                                        .color(Color32::from_rgb(0, 100, 0)) // Dark green
                                                ).on_hover_text("Owner of the contract");
                                            } else {
                                                ui.label(itb.identity_id.to_string(Encoding::Base58));
                                            }
                                        });
                                        row.col(|ui| {
                                            if let Some(balance) = itb.balance {
                                                let formatted_balance = balance.to_string();
                                                ui.label(formatted_balance);
                                            } else if ui.button("Check").clicked() {
                                                action = AppAction::BackendTask(BackendTask::TokenTask(Box::new(TokenTask::QueryIdentityTokenBalance(itb.clone().into()))));
                                            }
                                        });
                                        if shows_estimation_column {
                                            row.col(|ui| {
                                                if itb.available_actions.can_estimate {
                                                        if let Some(known_rewards) = itb.estimated_unclaimed_rewards  {
                                                            ui.horizontal(|ui| {
                                                                let formatted_rewards = known_rewards.to_string();
                                                                ui.label(formatted_rewards);

                                                                // Info button to show explanation
                                                                let identity_token_id = IdentityTokenIdentifier {
                                                                    identity_id: itb.identity_id,
                                                                    token_id: itb.token_id,
                                                                };
                                                                if crate::ui::helpers::info_icon_button(ui, "Show reward calculation explanation").clicked() {
                                                                    self.show_explanation_popup = Some(identity_token_id);
                                                                }
                                                                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                                                    ui.add_space(-9.0);
                                                                    ui.horizontal(|ui| {
                                                                        if ui.button("Estimate").clicked() {
                                                                            action = AppAction::BackendTask(BackendTask::TokenTask(Box::new(TokenTask::EstimatePerpetualTokenRewardsWithExplanation {
                                                                                identity_id: itb.identity_id,
                                                                                token_id: itb.token_id,
                                                                            })));
                                                                            self.refreshing_status = RefreshingStatus::Refreshing(Utc::now().timestamp() as u64);
                                                                        }
                                                                    })
                                                                });
                                                            });
                                                        } else if ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                                            ui.add_space(-3.0);
                                                            ui.button("Estimate").clicked()
                                                        }).inner {
                                                            action = AppAction::BackendTask(BackendTask::TokenTask(Box::new(TokenTask::EstimatePerpetualTokenRewardsWithExplanation {
                                                                identity_id: itb.identity_id,
                                                                token_id: itb.token_id,
                                                            })));
                                                            self.refreshing_status = RefreshingStatus::Refreshing(Utc::now().timestamp() as u64);
                                                        }

                                                }
                                            });
                                        }
                                        row.col(|ui| {
                                            ui.horizontal(|ui| {
                                                if itb.available_actions.shown_buttons() < 6 {
                                                    action |= self.render_actions(itb, &token_info, 0..10, ui);
                                                } else {
                                                    action |= self.render_actions(itb, &token_info, 0..3, ui);
                                                    // Expandable advanced actions menu
                                                    ui.menu_button("...", |ui| {
                                                        action |= self.render_actions(itb, &token_info, 3..128, ui);
                                                    });
                                                }

                                                // Remove
                                                if ui
                                                    .button("X")
                                                    .on_hover_text(
                                                        "Remove identity token balance from DET",
                                                    )
                                                    .clicked()
                                                {
                                                    self.confirm_remove_identity_token_balance_popup = true;
                                                    self.identity_token_balance_to_remove = Some(itb.into());
                                                }
                                            });
                                        });
                                    });
                                }
                            });
            });

        // Show explanation popup if requested
        if let Some(identity_token_id) = self.show_explanation_popup {
            if let Some(explanation) = self.reward_explanations.get(&identity_token_id) {
                let mut is_open = true;
                egui::Window::new("Reward Calculation Explanation")
                    .resizable(true)
                    .collapsible(false)
                    .default_width(600.0)
                    .default_height(400.0)
                    .open(&mut is_open)
                    .show(ui.ctx(), |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.heading("Reward Estimation Details");
                            ui.separator();

                            let formatted_total = explanation.total_amount.to_string();
                            ui.label(format!(
                                "Total Estimated Rewards: {} tokens",
                                formatted_total
                            ));
                            ui.separator();

                            ClickableCollapsingHeader::new("Basic Explanation")
                                .id_salt("basic_explanation_header")
                                .show(ui, |ui| {
                                let local_time = Local::now();
                                let timezone = local_time.format("%Z").to_string();

                                let short_explanation = explanation.short_explanation(
                                    token_info.token_configuration.conventions().decimals(),
                                    self.app_context.platform_version(),
                                    &timezone,
                                );

                                ui.label(short_explanation);
                            });

                            ClickableCollapsingHeader::new("Detailed Explanation")
                                .id_salt("detailed_explanation_header")
                                .show(ui, |ui| {
                                ui.label(explanation.detailed_explanation());
                            });

                            if !explanation.evaluation_steps.is_empty() {
                                ClickableCollapsingHeader::new("Step-by-Step Breakdown")
                                    .id_salt("step_by_step_header")
                                    .show(ui, |ui| {
                                    for (i, step) in explanation.evaluation_steps.iter().enumerate()
                                    {
                                        ClickableCollapsingHeader::new(format!("Step {}", i + 1))
                                            .id_salt(format!("step_{}_header", i))
                                            .show(ui, |ui| {
                                            if let Some(step_explanation) =
                                                explanation.explanation_for_step(step.step_index)
                                            {
                                                ui.label(step_explanation);
                                            }
                                        });
                                    }
                                });
                            }

                            ui.separator();
                            if ui.button("Close").clicked() {
                                self.show_explanation_popup = None;
                            }
                        });
                    });

                // If the window was closed via the X button
                if !is_open {
                    self.show_explanation_popup = None;
                }
            } else {
                // No explanation available yet, close popup
                self.show_explanation_popup = None;
            }
        }

        action
    }

    fn render_actions(
        &mut self,
        itb: &IdentityTokenMaybeBalanceWithActions,
        token_info: &TokenInfoWithDataContract,
        range: Range<usize>,
        ui: &mut Ui,
    ) -> AppAction {
        let mut pos = 0;
        let mut action = AppAction::None;

        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.add_space(-9.0);
            ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;

            if range.contains(&pos) {
                if itb.available_actions.can_transfer {
                    if let Some(balance) = itb.balance {
                        // Transfer
                        if ui.button("Transfer").clicked() {
                            action = AppAction::AddScreen(Screen::TransferTokensScreen(
                                TransferTokensScreen::new(
                                    itb.to_token_balance(balance),
                                    &self.app_context,
                                ),
                            ));
                        }
                    }
                } else {
                    // Disabled, grayed-out Transfer button
                    ui.add_enabled(
                        false,
                        egui::Button::new(RichText::new("Transfer").color(Color32::GRAY)),
                    )
                    .on_hover_text("Transfer not available");
                }
            }

            pos += 1;

            // Claim
            if itb.available_actions.can_claim {
                if range.contains(&pos) && ui.button("Claim").clicked() {
                    match self.app_context.get_contract_by_token_id(&itb.token_id) {
                        Ok(Some(contract)) => {
                            action = AppAction::AddScreen(Screen::ClaimTokensScreen(ClaimTokensScreen::new(
                                itb.into(),
                                contract,
                                token_info.token_configuration.clone(),
                                &self.app_context,
                            )));
                            ui.close_menu();
                        }
                        Ok(None) => {
                            self.set_error_message(Some("Token contract not found".to_string()));
                        }
                        Err(e) => {
                            self.set_error_message(Some(format!("Error fetching token contract: {e}")));
                        }
                    }
                }
                pos += 1;
            }

            if itb.available_actions.can_mint {
                if range.contains(&pos) && ui.button("Mint").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::MintTokensScreen(
                                    MintTokensScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };

                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_burn {
                if range.contains(&pos) && ui.button("Burn").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::BurnTokensScreen(
                                    BurnTokensScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_freeze {
                if range.contains(&pos) && ui.button("Freeze").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::FreezeTokensScreen(
                                    FreezeTokensScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_destroy {
                if range.contains(&pos) && ui.button("Destroy Frozen Identity Tokens").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::DestroyFrozenFundsScreen(
                                    DestroyFrozenFundsScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_unfreeze {
                if range.contains(&pos) && ui.button("Unfreeze").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::UnfreezeTokensScreen(
                                    UnfreezeTokensScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_do_emergency_action {
                if range.contains(&pos) {
                    if ui.button("Pause").clicked() {
                        match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                            Ok(info) => {
                                action = AppAction::AddScreen(
                                    Screen::PauseTokensScreen(
                                        PauseTokensScreen::new(
                                            info,
                                            &self.app_context,
                                        ),
                                    ),
                                );
                            }
                            Err(e) => {
                                self.set_error_message(Some(e));
                            }
                        };
                        ui.close_menu();
                    }
                    pos += 1;
                }

                if range.contains(&pos) {
                    if ui.button("Resume").clicked() {
                        match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                            Ok(info) => {
                                action = AppAction::AddScreen(
                                    Screen::ResumeTokensScreen(
                                        ResumeTokensScreen::new(
                                            info,
                                            &self.app_context,
                                        ),
                                    ),
                                );
                            }
                            Err(e) => {
                                self.set_error_message(Some(e));
                            }
                        };
                        ui.close_menu();
                    }
                    pos += 1;
                }
            }
            if itb.available_actions.can_claim {
                if range.contains(&pos) && ui.button("View Claims").clicked() {
                    action = AppAction::AddScreen(Screen::ViewTokenClaimsScreen(
                        ViewTokenClaimsScreen::new(itb.into(), &self.app_context),
                    ));
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_update_config {
                if range.contains(&pos) && ui.button("Update Config").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::UpdateTokenConfigScreen(Box::new(
                                    UpdateTokenConfigScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                )),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };
                    ui.close_menu();
                }
                pos += 1;
            }
            if itb.available_actions.can_maybe_purchase {
                if range.contains(&pos) {
                    // Check if we have pricing data
                    let has_pricing_data = self.token_pricing_data.contains_key(&itb.token_id);
                    let is_loading = self
                        .pricing_loading_state
                        .get(&itb.token_id)
                        .copied()
                        .unwrap_or(false);

                    if is_loading {
                        // Show loading spinner
                        ui.add(egui::Spinner::new());
                    } else if has_pricing_data {
                        // Check if identity has enough credits for at least one token
                        let has_credits = self
                            .app_context
                            .get_identity_by_id(&itb.identity_id)
                            .map(|identity_opt| {
                                identity_opt
                                    .map(|identity| {
                                        use dash_sdk::dpp::identity::accessors::IdentityGettersV0;
                                        // Check if identity has enough credits for the minimum token price
                                        if let Some(Some(pricing)) =
                                            self.token_pricing_data.get(&itb.token_id)
                                        {
                                            let min_price = get_min_token_price(pricing);
                                            identity.identity.balance() >= min_price
                                        } else {
                                            false
                                        }
                                    })
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);

                        if has_credits {
                            // Purchase button enabled
                            if ui.button("Purchase").clicked() {
                                match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                                    Ok(info) => {
                                        action = AppAction::AddScreen(
                                            Screen::PurchaseTokenScreen(
                                                PurchaseTokenScreen::new(
                                                    info,
                                                    &self.app_context,
                                                ),
                                            ),
                                        );
                                    }
                                    Err(e) => {
                                        self.set_error_message(Some(e));
                                    }
                                };
                                ui.close_menu();
                            }
                        } else {
                            // Disabled, grayed-out Purchase button
                            ui.add_enabled(
                                false,
                                egui::Button::new(RichText::new("Purchase").color(egui::Color32::GRAY)),
                            )
                            .on_hover_text({
                                if let Some(Some(pricing)) = self.token_pricing_data.get(&itb.token_id) {
                                    let min_price = get_min_token_price(pricing);
                                    format!("Insufficient credits. Need at least {} credits to purchase one token", min_price)
                                } else {
                                    "No credits available for purchase".to_string()
                                }
                            });
                        }
                    }
                }
                pos += 1;
            }
            if itb.available_actions.can_set_price && range.contains(&pos) {
                // Set Price
                if ui.button("Set Price").clicked() {
                    match IdentityTokenInfo::try_from_identity_token_maybe_balance_with_actions_with_lookup(itb, &self.app_context) {
                        Ok(info) => {
                            action = AppAction::AddScreen(
                                Screen::SetTokenPriceScreen(
                                    SetTokenPriceScreen::new(
                                        info,
                                        &self.app_context,
                                    ),
                                ),
                            );
                        }
                        Err(e) => {
                            self.set_error_message(Some(e));
                        }
                    };

                    ui.close_menu();
                }
            }
        });

            });

        action
    }

    /// Renders the top-level token list (one row per unique token).
    /// When the user clicks on a token, we set `selected_token_id`.
    fn render_token_list(&mut self, ui: &mut Ui) -> Result<AppAction, String> {
        let mut action = AppAction::None;
        // Space allocation for UI elements is handled by the layout system

        // A simple table with columns: [Token Name | Token ID | Total Balance]
        egui::ScrollArea::both().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());

            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .column(Column::initial(150.0).resizable(true)) // Token Name
                .column(Column::initial(200.0).resizable(true)) // Token ID
                .column(Column::initial(80.0).resizable(true)) // Description
                .column(Column::initial(80.0).resizable(true)) // Actions
                .header(30.0, |mut header| {
                    header.col(|ui| {
                        ui.label("Token Name");
                    });
                    header.col(|ui| {
                        ui.label("Token ID");
                    });
                    header.col(|ui| {
                        ui.label("Description");
                    });
                    header.col(|ui| {
                        ui.label("Actions");
                    });
                })
                .body(|mut body| {
                    for token_info in self.all_known_tokens.values() {
                        let TokenInfoWithDataContract {
                            token_id,
                            token_name,
                            description,
                            ..
                        } = token_info;
                        body.row(30.0, |mut row| {
                            row.col(|ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                    ui.add_space(-1.0);
                                    // By making the label into a button or using `ui.selectable_label`,
                                    // we can respond to clicks.
                                    if ui.button(token_name).clicked() {
                                        self.selected_token = Some(*token_id);
                                        // Check if we need to fetch pricing data for this token
                                        if !self.token_pricing_data.contains_key(token_id) {
                                            // Mark as loading
                                            self.pricing_loading_state.insert(*token_id, true);
                                            // Trigger backend task to fetch pricing
                                            action = AppAction::BackendTask(
                                                BackendTask::TokenTask(Box::new(
                                                    TokenTask::QueryTokenPricing(*token_id),
                                                )),
                                            );
                                        }
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.label(token_id.to_string(Encoding::Base58));
                            });
                            row.col(|ui| {
                                ui.label(description.as_ref().unwrap_or(&"None".to_string()));
                            });
                            row.col(|ui| {
                                // Info button
                                if ui.button("More Info").clicked() {
                                    self.show_token_info_popup = Some(*token_id);
                                }

                                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                    ui.add_space(-1.0);

                                    ui.horizontal(|ui| {
                                        // Remove button
                                        if ui
                                            .button("X")
                                            .on_hover_text("Remove token from DET")
                                            .clicked()
                                        {
                                            self.confirm_remove_token_popup = true;
                                            self.token_to_remove = Some(*token_id);
                                        }
                                    });
                                });
                            });
                        });
                    }
                });
        });
        Ok(action)
    }
}
