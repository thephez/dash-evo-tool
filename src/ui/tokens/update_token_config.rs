use super::tokens_screen::IdentityTokenInfo;
use crate::app::AppAction;
use crate::backend_task::BackendTask;
use crate::backend_task::tokens::TokenTask;
use crate::context::AppContext;
use crate::model::qualified_identity::QualifiedIdentity;
use crate::model::wallet::Wallet;
use crate::ui::components::left_panel::add_left_panel;
use crate::ui::components::styled::island_central_panel;
use crate::ui::components::tokens_subscreen_chooser_panel::add_tokens_subscreen_chooser_panel;
use crate::ui::components::top_panel::add_top_panel;
use crate::ui::components::wallet_unlock::ScreenWithWalletUnlock;
use crate::ui::contracts_documents::group_actions_screen::GroupActionsScreen;
use crate::ui::helpers::{TransactionType, add_identity_key_chooser, render_group_action_text};
use crate::ui::identities::get_selected_wallet;
use crate::ui::identities::keys::add_key_screen::AddKeyScreen;
use crate::ui::identities::keys::key_info_screen::KeyInfoScreen;
use crate::ui::{MessageType, RootScreenType, Screen, ScreenLike};
use chrono::{DateTime, Utc};
use dash_sdk::dpp::data_contract::GroupContractPosition;
use dash_sdk::dpp::data_contract::accessors::v0::DataContractV0Getters;
use dash_sdk::dpp::data_contract::accessors::v1::DataContractV1Getters;
use dash_sdk::dpp::data_contract::associated_token::token_configuration::accessors::v0::TokenConfigurationV0Getters;
use dash_sdk::dpp::data_contract::associated_token::token_configuration_convention::TokenConfigurationConvention;
use dash_sdk::dpp::data_contract::associated_token::token_configuration_item::TokenConfigurationChangeItem;
use dash_sdk::dpp::data_contract::associated_token::token_distribution_rules::accessors::v0::TokenDistributionRulesV0Getters;
use dash_sdk::dpp::data_contract::change_control_rules::authorized_action_takers::AuthorizedActionTakers;
use dash_sdk::dpp::data_contract::group::Group;
use dash_sdk::dpp::data_contract::group::accessors::v0::GroupV0Getters;
use dash_sdk::dpp::group::{GroupStateTransitionInfo, GroupStateTransitionInfoStatus};
use dash_sdk::dpp::identity::accessors::IdentityGettersV0;
use dash_sdk::dpp::identity::{KeyType, Purpose, SecurityLevel};
use dash_sdk::dpp::platform_value::string_encoding::Encoding;
use dash_sdk::platform::{DataContract, Identifier, IdentityPublicKey};
use eframe::egui::{self, Color32, Context, Ui};
use egui::RichText;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateTokenConfigStatus {
    NotUpdating,
    Updating(DateTime<Utc>),
}

pub struct UpdateTokenConfigScreen {
    pub identity_token_info: IdentityTokenInfo,
    backend_message: Option<(String, MessageType, DateTime<Utc>)>,
    update_status: UpdateTokenConfigStatus,
    pub app_context: Arc<AppContext>,
    pub change_item: TokenConfigurationChangeItem,
    pub update_text: String,
    pub text_input_error: String,
    signing_key: Option<IdentityPublicKey>,
    identity: QualifiedIdentity,
    pub public_note: Option<String>,
    group: Option<(GroupContractPosition, Group)>,
    is_unilateral_group_member: bool,
    pub group_action_id: Option<Identifier>,

    // Input state fields
    pub authorized_identity_input: Option<String>,
    pub authorized_group_input: Option<String>,

    selected_wallet: Option<Arc<RwLock<Wallet>>>,
    wallet_password: String,
    show_password: bool,
    error_message: Option<String>, // unused
}

impl UpdateTokenConfigScreen {
    pub fn new(identity_token_info: IdentityTokenInfo, app_context: &Arc<AppContext>) -> Self {
        let possible_key = identity_token_info
            .identity
            .identity
            .get_first_public_key_matching(
                Purpose::AUTHENTICATION,
                HashSet::from([SecurityLevel::CRITICAL]),
                KeyType::all_key_types().into(),
                false,
            )
            .cloned();

        let mut error_message = None;

        // Initialize with no group - will be set when user selects a change item
        let group = None;

        // Initialize as false - will be updated when group is determined
        let is_unilateral_group_member = false;

        // Attempt to get an unlocked wallet reference
        let selected_wallet = get_selected_wallet(
            &identity_token_info.identity,
            None,
            possible_key.as_ref(),
            &mut error_message,
        );

        Self {
            identity_token_info: identity_token_info.clone(),
            backend_message: None,
            update_status: UpdateTokenConfigStatus::NotUpdating,
            app_context: app_context.clone(),
            change_item: TokenConfigurationChangeItem::TokenConfigurationNoChange,
            update_text: "".to_string(),
            text_input_error: "".to_string(),
            signing_key: possible_key,
            public_note: None,

            authorized_identity_input: None,
            authorized_group_input: None,

            selected_wallet,
            wallet_password: String::new(),
            show_password: false,
            error_message,

            identity: identity_token_info.identity,
            group,
            is_unilateral_group_member,
            group_action_id: None,
        }
    }

    fn update_group_based_on_change_item(&mut self) {
        let authorized_action_takers = self
            .identity_token_info
            .token_config
            .authorized_action_takers_for_configuration_item(&self.change_item);

        let mut error_message = None;
        let group = match authorized_action_takers {
            AuthorizedActionTakers::NoOne => {
                error_message = Some("This action is not allowed on this token".to_string());
                None
            }
            AuthorizedActionTakers::ContractOwner => {
                if self.identity_token_info.data_contract.contract.owner_id()
                    != self.identity_token_info.identity.identity.id()
                {
                    error_message = Some(
                        "You are not allowed to perform this action. Only the contract owner is."
                            .to_string(),
                    );
                }
                None
            }
            AuthorizedActionTakers::Identity(identifier) => {
                if identifier != self.identity_token_info.identity.identity.id() {
                    error_message = Some("You are not allowed to perform this action".to_string());
                }
                None
            }
            AuthorizedActionTakers::MainGroup => {
                match self.identity_token_info.token_config.main_control_group() {
                    None => {
                        error_message = Some(
                            "Invalid contract: No main control group, though one should exist"
                                .to_string(),
                        );
                        None
                    }
                    Some(group_pos) => {
                        match self
                            .identity_token_info
                            .data_contract
                            .contract
                            .expected_group(group_pos)
                        {
                            Ok(group) => Some((group_pos, group.clone())),
                            Err(e) => {
                                error_message = Some(format!("Invalid contract: {}", e));
                                None
                            }
                        }
                    }
                }
            }
            AuthorizedActionTakers::Group(group_pos) => {
                match self
                    .identity_token_info
                    .data_contract
                    .contract
                    .expected_group(group_pos)
                {
                    Ok(group) => Some((group_pos, group.clone())),
                    Err(e) => {
                        error_message = Some(format!("Invalid contract: {}", e));
                        None
                    }
                }
            }
        };

        self.group = group;
        if let Some(error) = error_message {
            self.error_message = Some(error);
        } else {
            self.error_message = None;
        }

        // Update is_unilateral_group_member based on new group
        self.is_unilateral_group_member = false;
        if let Some((_, group)) = &self.group {
            let your_power = group
                .members()
                .get(&self.identity_token_info.identity.identity.id());

            if let Some(your_power) = your_power {
                if your_power >= &group.required_power() {
                    self.is_unilateral_group_member = true;
                }
            }
        }
    }

    fn render_token_config_updater(&mut self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;

        ui.heading("2. Select the item to update");
        ui.add_space(10.0);
        if self.group_action_id.is_some() {
            ui.label(
                "You are signing an existing group action so you are not allowed to edit the item.",
            );
            if self.group.is_none() {
                // we need to initialize group based on the change item
                self.update_group_based_on_change_item();
            }
        }

        // Clone the token configuration to avoid borrowing issues
        let default_token_configuration = self.identity_token_info.token_config.clone();

        ui.horizontal(|ui| {
            let label = token_change_item_label(&self.change_item);
            // user cannot change the item if it's part of a group action
            ui.add_enabled_ui(self.group_action_id.is_none(), |ui| {
            egui::ComboBox::from_id_salt("cfg_item_type".to_string())
                .selected_text(label)
                .width(270.0)
                .show_ui(ui, |ui| {
                    /* ───────── “No change” ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::TokenConfigurationNoChange,
                            "No Change",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── Conventions + groups ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::Conventions(
                                default_token_configuration.conventions().clone(),
                            ),
                            "Conventions",
                        )
                        .clicked()
                    {
                        self.update_text =
                            serde_json::to_string_pretty(default_token_configuration.conventions())
                                .unwrap_or_default();
                        self.text_input_error = "".to_string();
                        self.update_group_based_on_change_item();
                    };
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::ConventionsControlGroup(
                                *default_token_configuration
                                    .conventions_change_rules()
                                    .authorized_to_make_change_action_takers(),
                            ),
                            "Conventions Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::ConventionsAdminGroup(
                                *default_token_configuration
                                    .conventions_change_rules()
                                    .admin_action_takers(),
                            ),
                            "Conventions Admin Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── Max‑supply + groups ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MaxSupply(
                                default_token_configuration.max_supply(),
                            ),
                            "Max Supply",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MaxSupplyControlGroup(
                                *default_token_configuration
                                    .max_supply_change_rules()
                                    .authorized_to_make_change_action_takers(),
                            ),
                            "Max Supply Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MaxSupplyAdminGroup(
                                *default_token_configuration
                                    .max_supply_change_rules()
                                    .admin_action_takers(),
                            ),
                            "Max Supply Admin Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── Perpetual‑dist + groups ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::PerpetualDistribution(
                                default_token_configuration
                                    .distribution_rules()
                                    .perpetual_distribution()
                                    .cloned(),
                            ),
                            "Perpetual Distribution",
                        )
                        .clicked()
                    {
                        self.update_text = "".to_string();
                        self.text_input_error =
                            "The perpetual distribution can not be modified".to_string();
                        self.update_group_based_on_change_item();
                    };
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::PerpetualDistributionControlGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .perpetual_distribution_rules()
                                    .authorized_to_make_change_action_takers(),
                            ),
                            "Perpetual Distribution Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::PerpetualDistributionAdminGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .perpetual_distribution_rules()
                                    .admin_action_takers(),
                            ),
                            "Perpetual Distribution Admin Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── New‑tokens destination + groups ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::NewTokensDestinationIdentity(
                                default_token_configuration
                                    .distribution_rules()
                                    .new_tokens_destination_identity()
                                    .copied(),
                            ),
                            "New‑Tokens Destination",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::NewTokensDestinationIdentityControlGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .new_tokens_destination_identity_rules()
                                    .authorized_to_make_change_action_takers(),
                            ),
                            "New‑Tokens Destination Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::NewTokensDestinationIdentityAdminGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .new_tokens_destination_identity_rules()
                                    .admin_action_takers(),
                            ),
                            "New‑Tokens Destination Admin Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── Mint‑dest‑choice + groups ───────── */
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MintingAllowChoosingDestination(
                                default_token_configuration
                                    .distribution_rules()
                                    .minting_allow_choosing_destination(),
                            ),
                            "Minting Allow Choosing Destination",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MintingAllowChoosingDestinationControlGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .minting_allow_choosing_destination_rules()
                                    .authorized_to_make_change_action_takers(),
                            ),
                            "Minting Allow Choosing Destination Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MintingAllowChoosingDestinationAdminGroup(
                                *default_token_configuration
                                    .distribution_rules()
                                    .minting_allow_choosing_destination_rules()
                                    .admin_action_takers(),
                            ),
                            "Minting Allow Choosing Destination Admin Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }

                    ui.separator();

                    /* ───────── Remaining AuthorizedActionTakers variants ───────── */
                    macro_rules! aat_item {
                        ($variant:ident, $label:expr) => {
                            if ui
                                .selectable_value(
                                    &mut self.change_item,
                                    TokenConfigurationChangeItem::$variant(
                                        AuthorizedActionTakers::ContractOwner,
                                    ),
                                    $label,
                                )
                                .clicked()
                            {
                                self.update_group_based_on_change_item();
                            }
                        };
                    }

                    aat_item!(ManualMinting, "Manual Minting");
                    aat_item!(ManualMintingAdminGroup, "Manual Minting Admin Group");
                    ui.separator();
                    aat_item!(ManualBurning, "Manual Burning");
                    aat_item!(ManualBurningAdminGroup, "Manual Burning Admin Group");
                    ui.separator();
                    aat_item!(Freeze, "Freeze");
                    aat_item!(FreezeAdminGroup, "Freeze Admin Group");
                    ui.separator();
                    aat_item!(Unfreeze, "Unfreeze");
                    aat_item!(UnfreezeAdminGroup, "Unfreeze Admin Group");
                    ui.separator();
                    aat_item!(DestroyFrozenFunds, "Destroy Frozen Funds");
                    aat_item!(
                        DestroyFrozenFundsAdminGroup,
                        "Destroy Frozen Funds Admin Group"
                    );
                    ui.separator();
                    aat_item!(EmergencyAction, "Emergency Action");
                    aat_item!(EmergencyActionAdminGroup, "Emergency Action Admin Group");

                    ui.separator();

                    aat_item!(MarketplaceTradeModeControlGroup, "Marketplace Trade Mode Management");
                    aat_item!(MarketplaceTradeModeAdminGroup, "Marketplace Trade Mode Admin");

                    ui.separator();

                    if ui
                        .selectable_value(
                            &mut self.change_item,
                            TokenConfigurationChangeItem::MainControlGroup(
                                default_token_configuration.main_control_group(),
                            ),
                            "Main Control Group",
                        )
                        .clicked()
                    {
                        self.update_group_based_on_change_item();
                    }
                });


        ui.add_space(10.0);

        /* ========== PER‑VARIANT EDITING ========== */
        match &mut self.change_item {
            TokenConfigurationChangeItem::Conventions(conv) => {
                ui.label("Update the JSON formatted text below to change the token conventions.");
                ui.add_space(5.0);

                let text_response = ui.text_edit_multiline(&mut self.update_text);

                if text_response.changed() {
                    match serde_json::from_str::<TokenConfigurationConvention>(&self.update_text) {
                        Ok(new_conv) => {
                            *conv = new_conv;
                            self.text_input_error = "".to_string();
                        }
                        Err(e) => {
                            self.text_input_error = format!("Invalid JSON: {}", e);
                        }
                    }
                }

                ui.horizontal(|ui| {
                    if ui.button("Reset to Current").clicked() {
                        *conv = self.identity_token_info.token_config.conventions().clone();
                        self.update_text = serde_json::to_string_pretty(conv).unwrap_or_default(); // Update displayed text
                        self.text_input_error = "".to_string();
                    }

                    if !self.text_input_error.is_empty() {
                        ui.colored_label(Color32::RED, &self.text_input_error);
                    }
                });
            }
            TokenConfigurationChangeItem::MaxSupply(opt_amt) => {
                let mut txt = opt_amt.map(|a| a.to_string()).unwrap_or_default();
                if ui.text_edit_singleline(&mut txt).changed() {
                    *opt_amt = txt.parse::<u64>().ok();
                }
            }
            TokenConfigurationChangeItem::MintingAllowChoosingDestination(b) => {
                ui.checkbox(b, "Allow user to choose destination when minting");
            }
            TokenConfigurationChangeItem::NewTokensDestinationIdentity(opt_id) => {
                let mut txt = opt_id
                    .map(|id| id.to_string(Encoding::Base58))
                    .unwrap_or_default();
                if ui.text_edit_singleline(&mut txt).changed() {
                    *opt_id = Identifier::from_string(&txt, Encoding::Base58).ok();
                }
            }
            TokenConfigurationChangeItem::PerpetualDistribution(opt_json) => {
                ui.add_space(5.0);

                ui.label(&self.update_text);

                ui.horizontal(|ui| {
                    if let Some(opt_json) = opt_json {
                        if ui.button("View Current").clicked() {
                            self.update_text =
                                serde_json::to_string_pretty(opt_json).unwrap_or_default();
                            // Update displayed text
                        }
                    }

                    if !self.text_input_error.is_empty() {
                        ui.colored_label(Color32::RED, &self.text_input_error);
                    }
                });
            }
            TokenConfigurationChangeItem::MainControlGroup(opt_grp) => {
                let mut grp_txt = opt_grp.map(|g| g).unwrap_or_default();
                let mut grp_txt_str = grp_txt.to_string();
                if ui.text_edit_singleline(&mut grp_txt_str).changed() {
                    grp_txt = grp_txt_str.parse::<u16>().unwrap_or_default();
                }
                *opt_grp = Some(grp_txt);
            }
            TokenConfigurationChangeItem::ManualMinting(t)
            | TokenConfigurationChangeItem::ManualMintingAdminGroup(t)
            | TokenConfigurationChangeItem::ManualBurning(t)
            | TokenConfigurationChangeItem::ManualBurningAdminGroup(t)
            | TokenConfigurationChangeItem::Freeze(t)
            | TokenConfigurationChangeItem::FreezeAdminGroup(t)
            | TokenConfigurationChangeItem::Unfreeze(t)
            | TokenConfigurationChangeItem::UnfreezeAdminGroup(t)
            | TokenConfigurationChangeItem::DestroyFrozenFunds(t)
            | TokenConfigurationChangeItem::DestroyFrozenFundsAdminGroup(t)
            | TokenConfigurationChangeItem::EmergencyAction(t)
            | TokenConfigurationChangeItem::EmergencyActionAdminGroup(t)
            | TokenConfigurationChangeItem::ConventionsControlGroup(t)
            | TokenConfigurationChangeItem::ConventionsAdminGroup(t)
            | TokenConfigurationChangeItem::MaxSupplyControlGroup(t)
            | TokenConfigurationChangeItem::MaxSupplyAdminGroup(t)
            | TokenConfigurationChangeItem::PerpetualDistributionControlGroup(t)
            | TokenConfigurationChangeItem::PerpetualDistributionAdminGroup(t)
            | TokenConfigurationChangeItem::NewTokensDestinationIdentityControlGroup(t)
            | TokenConfigurationChangeItem::NewTokensDestinationIdentityAdminGroup(t)
            | TokenConfigurationChangeItem::MintingAllowChoosingDestinationControlGroup(t)
            | TokenConfigurationChangeItem::MintingAllowChoosingDestinationAdminGroup(t)
            | TokenConfigurationChangeItem::MarketplaceTradeModeControlGroup(t)
            | TokenConfigurationChangeItem::MarketplaceTradeModeAdminGroup(t) => {
                Self::render_authorized_action_takers_editor(
                    ui,
                    t,
                    &mut self.authorized_identity_input,
                    &mut self.authorized_group_input,
                    &self.identity_token_info.data_contract.contract,
                );
            }
            TokenConfigurationChangeItem::TokenConfigurationNoChange => {
                ui.label("No parameters to edit for this entry.");
            }
            TokenConfigurationChangeItem::MarketplaceTradeMode(_) => {
                unimplemented!("marketplace settings not implemented yet")
            }
        }
        });
        });
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.heading("3. Public note (optional)");
        ui.add_space(5.0);
        if self.group_action_id.is_some() {
            ui.label(
                "You are signing an existing group ConfigUpdate so you are not allowed to put a note.",
            );
            ui.add_space(5.0);
            ui.label(format!(
                "Note: {}",
                self.public_note.clone().unwrap_or("None".to_string())
            ));
        } else {
            ui.horizontal(|ui| {
                ui.label("Public note (optional):");
                ui.add_space(10.0);
                let mut txt = self.public_note.clone().unwrap_or_default();
                if ui
                    .text_edit_singleline(&mut txt)
                    .on_hover_text("A note about the transaction that can be seen by the public.")
                    .changed()
                {
                    self.public_note = if !txt.is_empty() { Some(txt) } else { None };
                }
            });
        }

        let button_text = render_group_action_text(
            ui,
            &self.group,
            &self.identity_token_info,
            "Update Config",
            &self.group_action_id,
        );

        let button = egui::Button::new(RichText::new(&button_text).color(Color32::WHITE))
            .fill(Color32::from_rgb(0, 128, 255))
            .frame(true)
            .corner_radius(3.0);

        if (self.app_context.is_developer_mode() || !button_text.contains("Test"))
            && self.change_item != TokenConfigurationChangeItem::TokenConfigurationNoChange
        {
            ui.add_space(20.0);
            if ui.add(button).clicked() {
                let group_info = if self.group_action_id.is_some() {
                    self.group.as_ref().map(|(pos, _)| {
                        GroupStateTransitionInfoStatus::GroupStateTransitionInfoOtherSigner(
                            GroupStateTransitionInfo {
                                group_contract_position: *pos,
                                action_id: self.group_action_id.unwrap(),
                                action_is_proposer: false,
                            },
                        )
                    })
                } else {
                    self.group.as_ref().map(|(pos, _)| {
                        GroupStateTransitionInfoStatus::GroupStateTransitionInfoProposer(*pos)
                    })
                };

                self.update_status = UpdateTokenConfigStatus::Updating(Utc::now());
                action |= AppAction::BackendTask(BackendTask::TokenTask(Box::new(
                    TokenTask::UpdateTokenConfig {
                        identity_token_info: Box::new(self.identity_token_info.clone()),
                        change_item: self.change_item.clone(),
                        signing_key: self.signing_key.clone().expect("Signing key must be set"),
                        public_note: if self.group_action_id.is_some() {
                            None
                        } else {
                            self.public_note.clone()
                        },
                        group_info,
                    },
                )));
            }
        }

        action
    }

    /* ===================================================================== */
    /* Helper: render AuthorizedActionTakers editor                          */
    /* ===================================================================== */
    pub fn render_authorized_action_takers_editor(
        ui: &mut Ui,
        takers: &mut AuthorizedActionTakers,
        authorized_identity_input: &mut Option<String>,
        authorized_group_input: &mut Option<String>,
        data_contract: &DataContract,
    ) {
        ui.horizontal(|ui| {
            // Display label
            ui.label("Authorized:");

            // Combo box for selecting the type of authorized taker
            egui::ComboBox::from_id_salt("authorized_action_takers")
                .selected_text(match takers {
                    AuthorizedActionTakers::NoOne => "No One".to_string(),
                    AuthorizedActionTakers::ContractOwner => "Contract Owner".to_string(),
                    AuthorizedActionTakers::MainGroup => "Main Group".to_string(),
                    AuthorizedActionTakers::Identity(id) => {
                        if id == &Identifier::default() {
                            "Identity".to_string()
                        } else {
                            format!("Identity({})", id)
                        }
                    }
                    AuthorizedActionTakers::Group(_) => "Group".to_string(),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(takers, AuthorizedActionTakers::NoOne, "No One");
                    ui.selectable_value(
                        takers,
                        AuthorizedActionTakers::ContractOwner,
                        "Contract Owner",
                    );
                    ui.selectable_value(takers, AuthorizedActionTakers::MainGroup, "Main Group");

                    // Set temporary input fields on select
                    if ui
                        .selectable_label(
                            matches!(takers, AuthorizedActionTakers::Identity(_)),
                            "Identity",
                        )
                        .clicked()
                    {
                        *takers = AuthorizedActionTakers::Identity(Identifier::default());
                        authorized_identity_input.get_or_insert_with(String::new);
                    }

                    if ui
                        .selectable_label(
                            matches!(takers, AuthorizedActionTakers::Group(_)),
                            "Group",
                        )
                        .clicked()
                    {
                        *takers = AuthorizedActionTakers::Group(0);
                        authorized_group_input.get_or_insert_with(|| "0".to_owned());
                    }
                });

            // Render input for Identity
            if let AuthorizedActionTakers::Identity(id) = takers {
                authorized_identity_input.get_or_insert_with(String::new);
                if let Some(id_str) = authorized_identity_input {
                    ui.horizontal(|ui| {
                        let dark_mode = ui.ctx().style().visuals.dark_mode;
                        ui.add_sized(
                            [300.0, 22.0],
                            egui::TextEdit::singleline(id_str)
                                .hint_text("Enter base58 identity")
                                .text_color(crate::ui::theme::DashColors::text_primary(dark_mode))
                                .background_color(crate::ui::theme::DashColors::input_background(
                                    dark_mode,
                                )),
                        );

                        if !id_str.is_empty() {
                            let is_valid =
                                Identifier::from_string(id_str, Encoding::Base58).is_ok();
                            let (symbol, color) = if is_valid {
                                ("✔", Color32::DARK_GREEN)
                            } else {
                                ("×", Color32::RED)
                            };
                            ui.label(RichText::new(symbol).color(color).strong());

                            if is_valid {
                                *id = Identifier::from_string(id_str, Encoding::Base58).unwrap();
                            }
                        }
                    });
                }
            }

            let contract_group_positions: Vec<u16> =
                data_contract.groups().keys().cloned().collect();
            if let AuthorizedActionTakers::Group(g) = takers {
                authorized_group_input.get_or_insert_with(|| g.to_string());
                egui::ComboBox::from_id_salt("group_position_selector")
                    .selected_text(format!(
                        "Group Position: {}",
                        authorized_group_input.as_deref().unwrap_or(&g.to_string())
                    ))
                    .show_ui(ui, |ui| {
                        for position in &contract_group_positions {
                            if ui
                                .selectable_value(g, *position, format!("Group {}", position))
                                .clicked()
                            {
                                *authorized_group_input = Some(position.to_string());
                            }
                        }
                    });
            }
        });
    }

    fn show_success_screen(&self, ui: &mut Ui) -> AppAction {
        let mut action = AppAction::None;
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            ui.heading("🎉");
            if self.group_action_id.is_some() {
                // This ConfigUpdate is already initiated by the group, we are just signing it
                ui.heading("Group ConfigUpdate Signing Successful.");
            } else if !self.is_unilateral_group_member {
                ui.heading("Group ConfigUpdate Initiated.");
            } else {
                ui.heading("ConfigUpdate Successful.");
            }

            ui.add_space(20.0);

            if self.group_action_id.is_some() {
                if ui.button("Back to Group Actions").clicked() {
                    action |= AppAction::PopScreenAndRefresh;
                }
                if ui.button("Back to Tokens").clicked() {
                    action |= AppAction::SetMainScreenThenGoToMainScreen(
                        RootScreenType::RootScreenMyTokenBalances,
                    );
                }
            } else {
                if ui.button("Back to Tokens").clicked() {
                    action |= AppAction::PopScreenAndRefresh;
                }

                if !self.is_unilateral_group_member && ui.button("Go to Group Actions").clicked() {
                    action |= AppAction::PopThenAddScreenToMainScreen(
                        RootScreenType::RootScreenDocumentQuery,
                        Screen::GroupActionsScreen(GroupActionsScreen::new(
                            &self.app_context.clone(),
                        )),
                    );
                }
            }
        });
        action
    }
}

impl ScreenLike for UpdateTokenConfigScreen {
    fn display_message(&mut self, message: &str, message_type: MessageType) {
        match message_type {
            MessageType::Success => {
                self.backend_message =
                    Some((message.to_string(), MessageType::Success, Utc::now()));
                if message.contains("Successfully updated token config item") {
                    self.update_status = UpdateTokenConfigStatus::NotUpdating;
                }
            }
            MessageType::Error => {
                self.backend_message = Some((message.to_string(), MessageType::Error, Utc::now()));
                if message.contains("Failed to update token config") {
                    self.update_status = UpdateTokenConfigStatus::NotUpdating;
                }
            }
            MessageType::Info => {
                self.backend_message = Some((message.to_string(), MessageType::Info, Utc::now()));
            }
        }
    }

    fn ui(&mut self, ctx: &Context) -> AppAction {
        let mut action;

        // Build a top panel
        if self.group_action_id.is_some() {
            action = add_top_panel(
                ctx,
                &self.app_context,
                vec![
                    ("Contracts", AppAction::GoToMainScreen),
                    ("Group Actions", AppAction::PopScreen),
                    ("Update Token Config", AppAction::None),
                ],
                vec![],
            );
        } else {
            action = add_top_panel(
                ctx,
                &self.app_context,
                vec![
                    ("Tokens", AppAction::GoToMainScreen),
                    (&self.identity_token_info.token_alias, AppAction::PopScreen),
                    ("Update Token Config", AppAction::None),
                ],
                vec![],
            );
        }

        // Left panel
        action |= add_left_panel(
            ctx,
            &self.app_context,
            crate::ui::RootScreenType::RootScreenMyTokenBalances,
        );

        // Subscreen chooser
        action |= add_tokens_subscreen_chooser_panel(ctx, &self.app_context);

        // Central panel
        island_central_panel(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(msg) = &self.backend_message {
                    if msg.1 == MessageType::Success {
                        action |= self.show_success_screen(ui);
                        return;
                    }
                }

                ui.heading("Update Token Configuration");
                ui.add_space(10.0);

            // Check if user has any auth keys
            let has_keys = if self.app_context.is_developer_mode() {
                !self.identity.identity.public_keys().is_empty()
            } else {
                !self
                    .identity
                    .available_authentication_keys_with_critical_security_level()
                    .is_empty()
            };

            if !has_keys {
                ui.colored_label(
                    Color32::DARK_RED,
                    format!(
                        "No authentication keys with CRITICAL security level found for this {} identity.",
                        self.identity.identity_type,
                    ),
                );
                ui.add_space(10.0);

                // Show "Add key" or "Check keys" option
                let first_key = self.identity.identity.get_first_public_key_matching(
                    Purpose::AUTHENTICATION,
                    HashSet::from([SecurityLevel::CRITICAL]),
                    KeyType::all_key_types().into(),
                    false,
                );

                if let Some(key) = first_key {
                    if ui.button("Check Keys").clicked() {
                        action |= AppAction::AddScreen(Screen::KeyInfoScreen(KeyInfoScreen::new(
                            self.identity.clone(),
                            key.clone(),
                            None,
                            &self.app_context,
                        )));
                    }
                    ui.add_space(5.0);
                }

                if ui.button("Add key").clicked() {
                    action |= AppAction::AddScreen(Screen::AddKeyScreen(AddKeyScreen::new(
                        self.identity.clone(),
                        &self.app_context,
                    )));
                }
            } else {
                // Possibly handle locked wallet scenario (similar to TransferTokens)
                if self.selected_wallet.is_some() {
                    let (needed_unlock, just_unlocked) = self.render_wallet_unlock_if_needed(ui);

                    if needed_unlock && !just_unlocked {
                        // Must unlock before we can proceed
                        return;
                    }
                }

                // 1) Key selection
                ui.heading("1. Select the key to sign the transaction with");
                ui.add_space(10.0);

                let mut selected_identity = Some(self.identity.clone());
                add_identity_key_chooser(
                    ui,
                    &self.app_context,
                    std::iter::once(&self.identity),
                    &mut selected_identity,
                    &mut self.signing_key,
                    TransactionType::TokenAction,
                );

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                action |= self.render_token_config_updater(ui);

                if let Some((msg, msg_type, _)) = &self.backend_message {
                    ui.add_space(10.0);
                    match msg_type {
                        MessageType::Success => {
                            ui.colored_label(Color32::DARK_GREEN, msg);
                        }
                        MessageType::Error => {
                            ui.colored_label(Color32::DARK_RED, msg);
                        }
                        MessageType::Info => {
                            ui.label(msg);
                        }
                    };
                }

                if self.update_status != UpdateTokenConfigStatus::NotUpdating {
                    ui.add_space(10.0);
                    if let UpdateTokenConfigStatus::Updating(start_time) = &self.update_status {
                        let elapsed = Utc::now().signed_duration_since(*start_time);
                        ui.label(format!("Updating... ({} seconds)", elapsed.num_seconds()));
                    }
                }
            }
            }); // end of ScrollArea
        });

        action
    }
}

impl ScreenWithWalletUnlock for UpdateTokenConfigScreen {
    fn selected_wallet_ref(&self) -> &Option<Arc<RwLock<Wallet>>> {
        &self.selected_wallet
    }

    fn wallet_password_ref(&self) -> &String {
        &self.wallet_password
    }

    fn wallet_password_mut(&mut self) -> &mut String {
        &mut self.wallet_password
    }

    fn show_password(&self) -> bool {
        self.show_password
    }

    fn show_password_mut(&mut self) -> &mut bool {
        &mut self.show_password
    }

    fn set_error_message(&mut self, error_message: Option<String>) {
        self.error_message = error_message;
    }

    fn error_message(&self) -> Option<&String> {
        self.error_message.as_ref()
    }
}

/// Returns a simple label for UI display
fn token_change_item_label(item: &TokenConfigurationChangeItem) -> &'static str {
    match item {
        TokenConfigurationChangeItem::TokenConfigurationNoChange => "No Change",
        TokenConfigurationChangeItem::Conventions(_) => "Conventions",
        TokenConfigurationChangeItem::ConventionsControlGroup(_) => "Conventions Control Group",
        TokenConfigurationChangeItem::ConventionsAdminGroup(_) => "Conventions Admin Group",
        TokenConfigurationChangeItem::MaxSupply(_) => "Max Supply",
        TokenConfigurationChangeItem::MaxSupplyControlGroup(_) => "Max Supply Control Group",
        TokenConfigurationChangeItem::MaxSupplyAdminGroup(_) => "Max Supply Admin Group",
        TokenConfigurationChangeItem::PerpetualDistribution(_) => "Perpetual Distribution",
        TokenConfigurationChangeItem::PerpetualDistributionControlGroup(_) => {
            "Perpetual Distribution Control Group"
        }
        TokenConfigurationChangeItem::PerpetualDistributionAdminGroup(_) => {
            "Perpetual Distribution Admin Group"
        }
        TokenConfigurationChangeItem::NewTokensDestinationIdentity(_) => "New‑Tokens Destination",
        TokenConfigurationChangeItem::NewTokensDestinationIdentityControlGroup(_) => {
            "New‑Tokens Destination Control Group"
        }
        TokenConfigurationChangeItem::NewTokensDestinationIdentityAdminGroup(_) => {
            "New‑Tokens Destination Admin Group"
        }
        TokenConfigurationChangeItem::MintingAllowChoosingDestination(_) => {
            "Minting Allow Choosing Destination"
        }
        TokenConfigurationChangeItem::MintingAllowChoosingDestinationControlGroup(_) => {
            "Minting Allow Choosing Destination Control Group"
        }
        TokenConfigurationChangeItem::MintingAllowChoosingDestinationAdminGroup(_) => {
            "Minting Allow Choosing Destination Admin Group"
        }
        TokenConfigurationChangeItem::ManualMinting(_) => "Manual Minting",
        TokenConfigurationChangeItem::ManualMintingAdminGroup(_) => "Manual Minting Admin Group",
        TokenConfigurationChangeItem::ManualBurning(_) => "Manual Burning",
        TokenConfigurationChangeItem::ManualBurningAdminGroup(_) => "Manual Burning Admin Group",
        TokenConfigurationChangeItem::Freeze(_) => "Freeze",
        TokenConfigurationChangeItem::FreezeAdminGroup(_) => "Freeze Admin Group",
        TokenConfigurationChangeItem::Unfreeze(_) => "Unfreeze",
        TokenConfigurationChangeItem::UnfreezeAdminGroup(_) => "Unfreeze Admin Group",
        TokenConfigurationChangeItem::DestroyFrozenFunds(_) => "Destroy Frozen Funds",
        TokenConfigurationChangeItem::DestroyFrozenFundsAdminGroup(_) => {
            "Destroy Frozen Funds Admin Group"
        }
        TokenConfigurationChangeItem::EmergencyAction(_) => "Emergency Action",
        TokenConfigurationChangeItem::EmergencyActionAdminGroup(_) => {
            "Emergency Action Admin Group"
        }
        TokenConfigurationChangeItem::MarketplaceTradeMode(_) => "Marketplace Trade Mode",
        TokenConfigurationChangeItem::MarketplaceTradeModeControlGroup(_) => {
            "Marketplace Trade Mode Control Group"
        }
        TokenConfigurationChangeItem::MarketplaceTradeModeAdminGroup(_) => {
            "Marketplace Trade Mode Admin Group"
        }
        TokenConfigurationChangeItem::MainControlGroup(_) => "Main Control Group",
    }
}
