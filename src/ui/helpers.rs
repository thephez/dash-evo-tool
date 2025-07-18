use std::sync::Arc;

use crate::{
    app::AppAction,
    context::AppContext,
    model::{qualified_contract::QualifiedContract, qualified_identity::QualifiedIdentity},
};
use dash_sdk::{
    dpp::{
        data_contract::{
            accessors::v0::DataContractV0Getters,
            document_type::{DocumentType, accessors::DocumentTypeV0Getters},
            group::{Group, accessors::v0::GroupV0Getters},
        },
        identity::{
            Purpose, SecurityLevel, accessors::IdentityGettersV0,
            identity_public_key::accessors::v0::IdentityPublicKeyGettersV0,
        },
        platform_value::string_encoding::Encoding,
    },
    platform::{Identifier, IdentityPublicKey},
};
use egui::{Color32, ComboBox, Response, Ui};

use super::tokens::tokens_screen::IdentityTokenInfo;

/// Helper function to create a styled info icon button
pub fn info_icon_button(ui: &mut egui::Ui, hover_text: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        // Draw circle background
        ui.painter().circle(
            rect.center(),
            8.0,
            if response.hovered() {
                Color32::from_rgb(0, 100, 200)
            } else {
                Color32::from_rgb(100, 100, 100)
            },
            egui::Stroke::NONE,
        );

        // Draw "i" text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "i",
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    }

    response.on_hover_text(hover_text)
}

/// Returns the newly selected identity (if changed), otherwise the existing one.
pub fn render_identity_selector(
    ui: &mut Ui,
    qualified_identities: &[QualifiedIdentity],
    selected_identity: &Option<QualifiedIdentity>,
) -> Option<QualifiedIdentity> {
    let mut new_selected_identity = selected_identity.clone();

    ui.horizontal(|ui| {
        ui.label("Identity:");
        ComboBox::from_id_salt("identity_selector")
            .selected_text(
                selected_identity
                    .as_ref()
                    .map(|qi| {
                        qi.alias
                            .as_ref()
                            .unwrap_or(&qi.identity.id().to_string(Encoding::Base58))
                            .clone()
                    })
                    .unwrap_or_else(|| "Choose identity…".into()),
            )
            .show_ui(ui, |cb| {
                for qi in qualified_identities {
                    let label = qi
                        .alias
                        .as_ref()
                        .unwrap_or(&qi.identity.id().to_string(Encoding::Base58))
                        .clone();

                    if cb
                        .selectable_label(selected_identity.as_ref() == Some(qi), label)
                        .clicked()
                    {
                        new_selected_identity = Some(qi.clone());
                    }
                }
            });
    });

    new_selected_identity
}

/// Returns the newly selected key (if changed), otherwise the existing one.
// Allow dead_code: This function provides UI for key selection within identities,
// useful for identity-based operations and key management interfaces
#[allow(dead_code)]
pub fn render_key_selector(
    ui: &mut Ui,
    selected_identity: &QualifiedIdentity,
    selected_key: &Option<IdentityPublicKey>,
) -> Option<IdentityPublicKey> {
    let mut new_selected_key = selected_key.clone();

    ui.horizontal(|ui| {
        ui.label("Key:");
        ComboBox::from_id_salt("key_selector")
            .selected_text(
                selected_key
                    .as_ref()
                    .map(|k| format!("Key {} Security {}", k.id(), k.security_level()))
                    .unwrap_or_else(|| "Choose key…".into()),
            )
            .show_ui(ui, |cb| {
                for key_ref in selected_identity.available_authentication_keys_non_master() {
                    let key = &key_ref.identity_public_key;
                    let label = format!("Key {} Security {}", key.id(), key.security_level());
                    if cb
                        .selectable_label(Some(key) == selected_key.as_ref(), label)
                        .clicked()
                    {
                        new_selected_key = Some(key.clone());
                    }
                }
            });
    });

    new_selected_key
}

/// Transaction types that require specific key filtering
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransactionType {
    /// Register a new data contract - requires Authentication keys with High or Critical security level
    RegisterContract,
    /// Update an existing data contract - requires Authentication keys with Critical security level only
    UpdateContract,
    /// Transfer credits - requires Transfer purpose keys
    Transfer,
    /// Withdraw from identity - requires Transfer or Owner purpose keys
    Withdraw,
    /// Generic document creation/update - security level depends on document type
    DocumentAction,
    /// Token actions such as minting - requires Critical Authentication keys
    TokenAction,
    /// Token action of transferring tokens
    TokenTransfer,
    /// Token action of claiming
    TokenClaim,
}

impl TransactionType {
    /// Returns the allowed purposes for this transaction type
    pub fn allowed_purposes(&self) -> Vec<Purpose> {
        match self {
            TransactionType::RegisterContract | TransactionType::UpdateContract => {
                vec![Purpose::AUTHENTICATION]
            }
            TransactionType::Transfer => vec![Purpose::TRANSFER],
            TransactionType::Withdraw => vec![Purpose::TRANSFER, Purpose::OWNER], // Owner keys handled separately
            TransactionType::DocumentAction | TransactionType::TokenAction => {
                vec![Purpose::AUTHENTICATION]
            }
            TransactionType::TokenTransfer | TransactionType::TokenClaim => {
                vec![Purpose::TRANSFER, Purpose::AUTHENTICATION]
            }
        }
    }

    /// Returns the allowed security levels for this transaction type
    pub fn allowed_security_levels(&self) -> Vec<SecurityLevel> {
        match self {
            TransactionType::RegisterContract => vec![SecurityLevel::CRITICAL, SecurityLevel::HIGH],
            TransactionType::UpdateContract => vec![SecurityLevel::CRITICAL],
            TransactionType::Transfer => vec![SecurityLevel::CRITICAL],
            TransactionType::Withdraw => vec![SecurityLevel::CRITICAL],
            TransactionType::DocumentAction => vec![
                SecurityLevel::CRITICAL,
                SecurityLevel::HIGH,
                SecurityLevel::MEDIUM,
            ],
            TransactionType::TokenAction
            | TransactionType::TokenTransfer
            | TransactionType::TokenClaim => vec![SecurityLevel::CRITICAL],
        }
    }

    /// Returns a descriptive label for the transaction type
    pub fn label(&self) -> &'static str {
        match self {
            TransactionType::RegisterContract => "Register Contract",
            TransactionType::UpdateContract => "Update Contract",
            TransactionType::Transfer => "Transfer",
            TransactionType::Withdraw => "Withdraw",
            TransactionType::DocumentAction => "Document Action",
            TransactionType::TokenAction => "Token Action",
            TransactionType::TokenTransfer => "Token Transfer",
            TransactionType::TokenClaim => "Token Claim",
        }
    }
}

/// Identity key chooser that filters keys based on transaction type and dev mode
pub fn add_identity_key_chooser<'a, T>(
    ui: &mut Ui,
    app_context: &AppContext,
    identities: T,
    selected_identity: &mut Option<QualifiedIdentity>,
    selected_key: &mut Option<IdentityPublicKey>,
    transaction_type: TransactionType,
) where
    T: Iterator<Item = &'a QualifiedIdentity>,
{
    add_identity_key_chooser_with_doc_type(
        ui,
        app_context,
        identities,
        selected_identity,
        selected_key,
        transaction_type,
        None,
    )
}

/// Identity key chooser that filters keys based on transaction type, document type and dev mode
pub fn add_identity_key_chooser_with_doc_type<'a, T>(
    ui: &mut Ui,
    app_context: &AppContext,
    identities: T,
    selected_identity: &mut Option<QualifiedIdentity>,
    selected_key: &mut Option<IdentityPublicKey>,
    transaction_type: TransactionType,
    document_type: Option<&DocumentType>,
) where
    T: Iterator<Item = &'a QualifiedIdentity>,
{
    let is_dev_mode = app_context.is_developer_mode();

    egui::Grid::new("identity_key_chooser_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .striped(false)
        .show(ui, |ui| {
            ui.label("Identity:");
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ComboBox::from_id_salt("identity_combo")
                    .width(220.0)
                    .selected_text(match selected_identity {
                        Some(qi) => qi
                            .alias
                            .clone()
                            .unwrap_or_else(|| qi.identity.id().to_string(Encoding::Base58)),
                        None => "Select Identity…".into(),
                    })
                    .show_ui(ui, |iui| {
                        for qi in identities {
                            let label = qi
                                .alias
                                .clone()
                                .unwrap_or_else(|| qi.identity.id().to_string(Encoding::Base58));
                            if iui
                                .selectable_label(
                                    selected_identity.as_ref() == Some(qi),
                                    label.clone(),
                                )
                                .clicked()
                            {
                                *selected_identity = Some(qi.clone());
                                *selected_key = None; // Clear key selection when identity changes
                            }
                        }
                    });
            });

            ui.end_row();

            ui.label("Key:");

            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ComboBox::from_id_salt("key_combo")
                    .width(220.0)
                    .selected_text(
                        selected_key
                            .as_ref()
                            .map(|k| {
                                format!(
                                    "Key {} Type {} Security {}",
                                    k.id(),
                                    k.key_type(),
                                    k.security_level()
                                )
                            })
                            .unwrap_or_else(|| "Select Key…".into()),
                    )
                    .show_ui(ui, |kui| {
                        if let Some(qi) = selected_identity {
                            let allowed_purposes = transaction_type.allowed_purposes();
                            let allowed_security_levels = if transaction_type
                                == TransactionType::DocumentAction
                                && document_type.is_some()
                            {
                                // For document actions with a specific document type, use its security requirement
                                let required_level =
                                    document_type.unwrap().security_level_requirement();
                                let allowed_levels =
                                    SecurityLevel::CRITICAL as u8..=required_level as u8;
                                let allowed_levels: Vec<SecurityLevel> = [
                                    SecurityLevel::CRITICAL,
                                    SecurityLevel::HIGH,
                                    SecurityLevel::MEDIUM,
                                ]
                                .iter()
                                .cloned()
                                .filter(|level| allowed_levels.contains(&(*level as u8)))
                                .collect();
                                allowed_levels
                            } else {
                                transaction_type.allowed_security_levels()
                            };

                            for key_ref in qi.private_keys.identity_public_keys() {
                                let key = &key_ref.1.identity_public_key;

                                // In dev mode, show all keys
                                // In production mode, filter by transaction requirements
                                let is_allowed = if is_dev_mode {
                                    true
                                } else {
                                    allowed_purposes.contains(&key.purpose())
                                        && allowed_security_levels.contains(&key.security_level())
                                };

                                if is_allowed {
                                    let label = if is_dev_mode
                                        && (!allowed_purposes.contains(&key.purpose())
                                            || !allowed_security_levels
                                                .contains(&key.security_level()))
                                    {
                                        // In dev mode, mark keys that wouldn't normally be allowed
                                        format!(
                                            "Key {} Security {} [DEV]",
                                            key.id(),
                                            key.security_level()
                                        )
                                    } else {
                                        format!(
                                            "Key {} Security {}",
                                            key.id(),
                                            key.security_level()
                                        )
                                    };

                                    if kui
                                        .selectable_label(selected_key.as_ref() == Some(key), label)
                                        .clicked()
                                    {
                                        *selected_key = Some(key.clone());
                                    }
                                }
                            }

                            if !is_dev_mode
                                && qi
                                    .private_keys
                                    .identity_public_keys()
                                    .iter()
                                    .all(|key_ref| {
                                        let key = &key_ref.1.identity_public_key;
                                        !allowed_purposes.contains(&key.purpose())
                                            || !allowed_security_levels
                                                .contains(&key.security_level())
                                    })
                            {
                                kui.label(format!(
                                    "No suitable keys for {}",
                                    transaction_type.label()
                                ));
                            }
                        } else {
                            kui.label("Pick an identity first");
                        }
                    });
            });
            ui.end_row();
        });
}

pub fn add_contract_doc_type_chooser_with_filtering(
    ui: &mut Ui,
    search_term: &mut String,
    app_context: &Arc<AppContext>,
    selected_contract: &mut Option<QualifiedContract>,
    selected_doc_type: &mut Option<DocumentType>,
) {
    let contracts = app_context.get_contracts(None, None).unwrap_or_default();
    let search_term_lowercase = search_term.to_lowercase();
    let filtered = contracts.iter().filter(|qc| {
        let key = qc
            .alias
            .clone()
            .unwrap_or_else(|| qc.contract.id().to_string(Encoding::Base58));
        key.to_lowercase().contains(&search_term_lowercase)
    });

    add_contract_doc_type_chooser_pre_filtered(
        ui,
        search_term,
        filtered,
        selected_contract,
        selected_doc_type,
    );
}

/// Extremely compact chooser: just two combo-boxes (Contract ▸ Doc-Type)
///
/// * No collapsible tree.
/// * Optional search box on top.
/// * Emits `ContractTask::RemoveContract` via a small “🗑” button next to the contract picker.
pub fn add_contract_doc_type_chooser_pre_filtered<'a, T>(
    ui: &mut Ui,
    search_term: &mut String,
    filtered_contracts: T,
    selected_contract: &mut Option<QualifiedContract>,
    selected_doc_type: &mut Option<DocumentType>,
) where
    T: Iterator<Item = &'a QualifiedContract>,
{
    egui::Grid::new("contract_doc_type_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .striped(false)
        .show(ui, |ui| {
            ui.label("Filter contracts:");
            ui.text_edit_singleline(search_term);
            ui.end_row();

            ui.label("Contract:");
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ComboBox::from_id_salt("contract_combo")
                    .width(220.0)
                    .selected_text(match selected_contract {
                        Some(qc) => qc
                            .alias
                            .clone()
                            .unwrap_or_else(|| qc.contract.id().to_string(Encoding::Base58)),
                        None => "Select Contract…".into(),
                    })
                    .show_ui(ui, |cui| {
                        for qc in filtered_contracts {
                            let label = qc
                                .alias
                                .clone()
                                .unwrap_or_else(|| qc.contract.id().to_string(Encoding::Base58));
                            if cui
                                .selectable_label(
                                    selected_contract.as_ref() == Some(qc),
                                    label.clone(),
                                )
                                .clicked()
                            {
                                *selected_contract = Some(qc.clone());
                            }
                        }
                    });
            });

            ui.end_row();

            ui.label("Doc Type:");
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ComboBox::from_id_salt("doctype_combo")
                    .width(220.0)
                    .selected_text(
                        selected_doc_type
                            .as_ref()
                            .map(|d| d.name().to_owned())
                            .unwrap_or_else(|| "Select Doc Type…".into()),
                    )
                    .show_ui(ui, |dui| {
                        if let Some(qc) = selected_contract {
                            for name in qc.contract.document_types().keys() {
                                if dui
                                    .selectable_label(
                                        selected_doc_type
                                            .as_ref()
                                            .map(|cur| cur.name() == name)
                                            .unwrap_or(false),
                                        name,
                                    )
                                    .clicked()
                                {
                                    *selected_doc_type =
                                        qc.contract.document_type_cloned_for_name(name).ok();
                                }
                            }
                        } else {
                            dui.label("Pick a contract first");
                        }
                    });
            });
            ui.end_row();
        });
}

/// Extremely compact chooser: just two combo-boxes (Contract ▸ Doc-Type)
///
/// * No collapsible tree.
/// * Optional search box on top.
pub fn add_contract_chooser_pre_filtered<'a, T>(
    ui: &mut Ui,
    search_term: &mut String,
    filtered_contracts: T,
    selected_contract: &mut Option<QualifiedContract>,
) where
    T: Iterator<Item = &'a QualifiedContract>,
{
    egui::Grid::new("contract_doc_type_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .striped(false)
        .show(ui, |ui| {
            ui.label("Filter contracts:");
            ui.text_edit_singleline(search_term);
            ui.end_row();

            ui.label("Contract:");
            ComboBox::from_id_salt("contract_chooser")
                .width(220.0)
                .selected_text(match selected_contract {
                    Some(qc) => qc
                        .alias
                        .clone()
                        .unwrap_or_else(|| qc.contract.id().to_string(Encoding::Base58)),
                    None => "Select Contract…".into(),
                })
                .show_ui(ui, |cui| {
                    for qc in filtered_contracts {
                        let label = qc
                            .alias
                            .clone()
                            .unwrap_or_else(|| qc.contract.id().to_string(Encoding::Base58));
                        if cui
                            .selectable_label(selected_contract.as_ref() == Some(qc), label.clone())
                            .clicked()
                        {
                            *selected_contract = Some(qc.clone());
                        }
                    }
                });

            ui.end_row();
        });
}

pub fn render_group_action_text(
    ui: &mut Ui,
    group: &Option<(u16, Group)>,
    identity_token_info: &IdentityTokenInfo,
    group_action_type_str: &str,
    group_action_id: &Option<Identifier>,
) -> String {
    if let Some(group_action_id) = group_action_id {
        ui.add_space(20.0);
        ui.add(egui::Label::new(
            egui::RichText::new("This is a group action.")
                .heading()
                .color(egui::Color32::DARK_RED),
        ));

        ui.add_space(10.0);
        ui.label(format!(
            "You are signing an active {} group action (Action ID {})",
            group_action_type_str,
            group_action_id.to_string(Encoding::Base58)
        ));
        format!("Sign {}", group_action_type_str)
    } else if let Some((_, group)) = group.as_ref() {
        let your_power = group
            .members()
            .get(&identity_token_info.identity.identity.id());

        ui.add_space(20.0);
        ui.add(egui::Label::new(
            egui::RichText::new("This is a group action.")
                .heading()
                .color(egui::Color32::DARK_RED),
        ));

        if your_power.is_none() {
            ui.add_space(10.0);
            ui.colored_label(
                Color32::DARK_RED,
                format!(
                    "You are not a valid group member for {} on this token",
                    group_action_type_str
                ),
            );
            return format!("Test {} (Should fail)", group_action_type_str);
        }

        ui.add_space(10.0);
        if let Some(your_power) = your_power {
            if *your_power >= group.required_power() {
                ui.label("You are a unilateral group member.\nYou do not need other group members to sign off on this action for it to process.".to_string());
                group_action_type_str.to_string()
            } else {
                ui.label(format!("You are not a unilateral group member.\nYou can initiate the {group_action_type_str} action but will need other group members to sign off on it for it to process.\nThis action requires a total power of {}.\nYour power is {your_power}.", group.required_power()));

                ui.add_space(10.0);
                ui.label(format!(
                    "Other group members are : \n{}",
                    group
                        .members()
                        .iter()
                        .filter_map(|(member, power)| {
                            if member != &identity_token_info.identity.identity.id() {
                                Some(format!(" - {} with power {}", member, power))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", \n")
                ));
                format!("Initiate Group {}", group_action_type_str)
            }
        } else {
            format!("Test {} (It should fail)", group_action_type_str)
        }
    } else {
        group_action_type_str.to_string()
    }
}

pub fn show_success_screen(
    ui: &mut Ui,
    success_message: String,
    action_buttons: Vec<(String, AppAction)>,
) -> AppAction {
    let mut action = AppAction::None;
    ui.vertical_centered(|ui| {
        ui.add_space(100.0);
        ui.heading("🎉");
        ui.heading(success_message);

        ui.add_space(20.0);
        for button in action_buttons {
            if ui.button(button.0).clicked() {
                action = button.1;
            }
        }
    });
    action
}
