use crate::com::atproto::server::InviteCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Delete a user account as an administrator.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeleteAccountInput {
    pub did: String,
}

/// Disable an account from receiving new invite codes, but does not invalidate existing codes.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DisableAccountInvitesInput {
    pub account: String,
    /// Optional reason for disabled invites.
    pub note: Option<String>,
}

/// Disable some set of codes and/or all codes associated with a set of users.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DisableInviteCodesInput {
    pub codes: Option<Vec<String>>,
    pub accounts: Option<Vec<String>>,
}

/// Re-enable an account's ability to receive invite codes.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnableAccountInvitesInput {
    pub account: String,
    /// Optional reason for enabled invites.
    pub note: Option<String>,
}

/// Administrative action to update an account's email.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateAccountEmailInput {
    /// The handle or DID of the repo.
    pub account: String,
    pub email: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GetInviteCodesOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub codes: Vec<InviteCode>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AccountView {
    pub did: String,
    pub handle: String,
    pub email: Option<String>,
    #[serde(rename = "relatedRecords")]
    pub related_records: Option<Vec<Value>>,
    #[serde(rename = "indexedAt")]
    pub indexed_at: String,
    #[serde(rename = "invitedBy")]
    pub invited_by: Option<InviteCode>,
    pub invites: Option<Vec<InviteCode>>,
    #[serde(rename = "invitesDisabled")]
    pub invites_disabled: Option<bool>,
    #[serde(rename = "emailConfirmedAt")]
    pub email_confirmed_at: Option<String>,
    #[serde(rename = "inviteNote")]
    pub invite_note: Option<String>,
}