//! BETA wallet panel: resettable Beta MINI *test value* for exercising product
//! flows. It is not production MINI, has no conversion right, and carries no
//! governance, review, release or personhood weight. Balances are provisional
//! state derived from the replicated signed-object set by the application
//! core; this panel holds no keys and signs nothing itself.
use eframe::egui;
use mini_app_protocol::{
    BetaBalancesView, MAX_BETA_CAMPAIGNS, MAX_BETA_MEMO_BYTES, MAX_OBJECT_ID_BYTES,
};

use crate::theme;

const MICRO_PER_BETA: u64 = 1_000_000;
const DISCLAIMER: &str =
    "BETA test value only: not production MINI, no conversion, no governance weight. \
Balances are provisional and every epoch starts again from zero.";

/// What the panel asks the host to send to the application core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    Register {
        operation_id: String,
        epoch: [u8; 32],
    },
    Balances {
        epoch: [u8; 32],
        campaigns: Vec<String>,
    },
    Transfer {
        operation_id: String,
        epoch: [u8; 32],
        campaigns: Vec<String>,
        from: [u8; 32],
        to: [u8; 32],
        amount_micro: u64,
        memo: String,
    },
    /// Approve one grant, signed by the primary identity (policy members are
    /// public DIDs by design, unlike the per-account BETA identities).
    GrantApprove {
        operation_id: String,
        policy_id: String,
        grant_id: String,
    },
    /// Read-only approval count for one grant.
    GrantStatus { policy_id: String, grant_id: String },
}

#[derive(Default)]
pub(crate) struct Panel {
    epoch: String,
    campaigns: String,
    from: String,
    to: String,
    amount: String,
    memo: String,
    balances: Option<BetaBalancesView>,
    /// Retry identity of an unfinished request: reused while the same request
    /// is retried so the core can replay it instead of signing twice, and
    /// dropped once the core answers.
    register_op: Option<([u8; 32], String)>,
    transfer_op: Option<(Request, String)>,
    grant_policy: String,
    grant_id: String,
    approve_op: Option<(Request, String)>,
    /// Explicit per-action signing confirmation; reset after every signed request.
    confirm: bool,
    balances_for: (String, String),
    status: String,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Exactly 32 bytes of hex (an epoch or an opaque account id).
pub(crate) fn parse_hex32(value: &str, what: &str) -> Result<[u8; 32], String> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{what} must be 64 hex characters"));
    }
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("{what} must be 64 hex characters"))?;
    }
    if out == [0u8; 32] {
        return Err(format!("{what} must not be all zero"));
    }
    Ok(out)
}

/// One authorized campaign ID per line: the explicit issuance trust anchor.
pub(crate) fn parse_campaigns(value: &str) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = Vec::new();
    for line in value.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !ids.iter().any(|existing| existing == line) {
            ids.push(line.to_string());
        }
    }
    if ids.is_empty() {
        return Err("enter at least one authorized campaign ID (one per line)".into());
    }
    if ids.len() > MAX_BETA_CAMPAIGNS {
        return Err(format!("at most {MAX_BETA_CAMPAIGNS} campaign IDs"));
    }
    Ok(ids)
}

/// Decimal BETA ("12", "0.5", "3.000001") to integer micro-units. No floats.
pub(crate) fn parse_amount_micro(value: &str) -> Result<u64, String> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let digits_ok = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() && fraction.is_empty()
        || !digits_ok(whole)
        || !digits_ok(fraction)
        || fraction.len() > 6
    {
        return Err("amount must be a decimal BETA value with at most 6 decimals".into());
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| "amount is too large".to_string())?
    };
    let fraction_micro: u64 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<6}")
            .parse()
            .map_err(|_| "amount is invalid".to_string())?
    };
    let micro = whole
        .checked_mul(MICRO_PER_BETA)
        .and_then(|w| w.checked_add(fraction_micro))
        .ok_or_else(|| "amount is too large".to_string())?;
    if micro == 0 {
        return Err("amount must be greater than zero".into());
    }
    Ok(micro)
}

pub(crate) fn format_beta(micro: u64) -> String {
    format!(
        "{}.{:06} BETA",
        micro / MICRO_PER_BETA,
        micro % MICRO_PER_BETA
    )
}

impl Panel {
    /// Validate the inputs into a core request without sending anything.
    pub(crate) fn register_request(&mut self) -> Result<Request, String> {
        let epoch = parse_hex32(&self.epoch, "epoch")?;
        let operation_id = match &self.register_op {
            Some((saved, id)) if *saved == epoch => id.clone(),
            _ => {
                let id = crate::app_service::operation_id("beta-register");
                self.register_op = Some((epoch, id.clone()));
                id
            }
        };
        Ok(Request::Register {
            operation_id,
            epoch,
        })
    }

    pub(crate) fn balances_request(&self) -> Result<Request, String> {
        Ok(Request::Balances {
            epoch: parse_hex32(&self.epoch, "epoch")?,
            campaigns: parse_campaigns(&self.campaigns)?,
        })
    }

    pub(crate) fn transfer_request(&mut self) -> Result<Request, String> {
        let memo = self.memo.trim();
        if memo.is_empty() || memo.len() > MAX_BETA_MEMO_BYTES {
            return Err(format!("memo must be 1-{MAX_BETA_MEMO_BYTES} bytes"));
        }
        let from = parse_hex32(&self.from, "sender account")?;
        let to = parse_hex32(&self.to, "recipient account")?;
        if from == to {
            return Err("sender and recipient must differ".into());
        }
        let mut request = Request::Transfer {
            operation_id: String::new(),
            epoch: parse_hex32(&self.epoch, "epoch")?,
            campaigns: parse_campaigns(&self.campaigns)?,
            from,
            to,
            amount_micro: parse_amount_micro(&self.amount)?,
            memo: memo.to_string(),
        };
        let operation_id = match &self.transfer_op {
            Some((saved, id)) if *saved == request => id.clone(),
            _ => {
                let id = crate::app_service::operation_id("beta-transfer");
                self.transfer_op = Some((request.clone(), id.clone()));
                id
            }
        };
        if let Request::Transfer {
            operation_id: slot, ..
        } = &mut request
        {
            *slot = operation_id;
        }
        Ok(request)
    }

    fn grant_ids(&self) -> Result<(String, String), String> {
        let check = |value: &str, what: &str| {
            let value = value.trim();
            if value.is_empty() || value.len() > MAX_OBJECT_ID_BYTES {
                Err(format!("enter the {what}"))
            } else {
                Ok(value.to_string())
            }
        };
        Ok((
            check(&self.grant_policy, "grant policy ID")?,
            check(&self.grant_id, "grant ID")?,
        ))
    }

    pub(crate) fn grant_approve_request(&mut self) -> Result<Request, String> {
        let (policy_id, grant_id) = self.grant_ids()?;
        let mut request = Request::GrantApprove {
            operation_id: String::new(),
            policy_id,
            grant_id,
        };
        let operation_id = match &self.approve_op {
            Some((saved, id)) if *saved == request => id.clone(),
            _ => {
                let id = crate::app_service::operation_id("beta-grant-approve");
                self.approve_op = Some((request.clone(), id.clone()));
                id
            }
        };
        if let Request::GrantApprove {
            operation_id: slot, ..
        } = &mut request
        {
            *slot = operation_id;
        }
        Ok(request)
    }

    pub(crate) fn grant_status_request(&self) -> Result<Request, String> {
        let (policy_id, grant_id) = self.grant_ids()?;
        Ok(Request::GrantStatus {
            policy_id,
            grant_id,
        })
    }

    pub(crate) fn on_grant_approval(&mut self) {
        self.approve_op = None;
        self.status = "Approval signed with your primary identity (grant policy members are \
public DIDs by design) and saved locally. Exchange with peers to share it; check the \
status to see the count. Test value only."
            .into();
    }

    pub(crate) fn on_grant_status(
        &mut self,
        accepted: bool,
        counted: u16,
        threshold: u16,
        equivocations: u16,
    ) {
        let mut text = if accepted {
            format!("Grant ACCEPTED: {counted} distinct approvals (threshold {threshold}).")
        } else {
            format!("Grant NOT accepted: {counted} of {threshold} distinct approvals.")
        };
        if equivocations > 0 {
            text.push_str(&format!(
                " Warning: {equivocations} authorizer equivocation(s) detected."
            ));
        }
        text.push_str(
            " Test value only; an accepted grant is evidence, not a conversion, and this \
view counts only the approvals present in this device's store.",
        );
        self.status = text;
    }

    pub(crate) fn on_account(&mut self, account: &[u8; 32]) {
        self.register_op = None;
        self.from = hex(account);
        self.status = "Registered a fresh BETA account (shown as the sender). Share its \
id to receive BETA; nothing is credited until a threshold-accepted grant or a transfer."
            .into();
    }

    pub(crate) fn on_balances(&mut self, view: BetaBalancesView) {
        self.status = if view.conflicts > 0 {
            format!(
                "Balances refreshed with {} conflicting item(s) excluded; values may be understated.",
                view.conflicts
            )
        } else {
            "Balances refreshed (provisional).".into()
        };
        self.balances_for = (self.epoch.clone(), self.campaigns.clone());
        self.balances = Some(view);
    }

    pub(crate) fn on_transfer(&mut self, amount_micro: u64, change_micro: u64) {
        self.transfer_op = None;
        self.status = format!(
            "Sent {} (change {}). The transfer is provisional until peers agree; refresh to see it.",
            format_beta(amount_micro),
            format_beta(change_micro)
        );
        self.amount.clear();
        self.memo.clear();
    }

    /// Draws the panel. `busy` disables actions while a core request is pending.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, busy: bool) -> Option<Request> {
        let mut request = None;
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new(DISCLAIMER).strong());
            ui.add_space(6.0);
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("Epoch (64 hex)");
                ui.text_edit_singleline(&mut self.epoch);
                ui.label("Authorized campaign IDs (one per line): the only source of issuance this view trusts");
                ui.add(
                    egui::TextEdit::multiline(&mut self.campaigns)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.checkbox(
                &mut self.confirm,
                "I confirm signing this BETA object with my unlocked identity",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy && self.confirm,
                        theme::secondary_button("Register a new BETA account"),
                    )
                    .clicked()
                {
                    request = Some(self.register_request());
                }
                if ui
                    .add_enabled(!busy, theme::secondary_button("Refresh balances"))
                    .clicked()
                {
                    request = Some(self.balances_request());
                }
            });
        });
        ui.add_space(8.0);
        // A cached view belongs to the inputs it was fetched with.
        if self.balances.is_some()
            && self.balances_for != (self.epoch.clone(), self.campaigns.clone())
        {
            self.balances = None;
        }
        if let Some(view) = &self.balances {
            theme::card_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("Your BETA accounts (provisional)").strong());
                if view.accounts.is_empty() {
                    ui.label("No accounts registered by this identity in this epoch.");
                }
                for entry in &view.accounts {
                    ui.horizontal(|ui| {
                        ui.monospace(hex(&entry.account));
                        ui.label(format_beta(entry.amount_micro));
                    });
                }
                ui.label(format!(
                    "Total issued this epoch: {}",
                    format_beta(view.total_issued_micro)
                ));
            });
            ui.add_space(8.0);
        }
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Send BETA").strong());
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("From account (64 hex)");
                ui.text_edit_singleline(&mut self.from);
                ui.label("To account (64 hex)");
                ui.text_edit_singleline(&mut self.to);
                ui.label("Amount (BETA, up to 6 decimals)");
                ui.text_edit_singleline(&mut self.amount);
                ui.label("Memo");
                ui.text_edit_singleline(&mut self.memo);
            });
            if ui
                .add_enabled(!busy && self.confirm, theme::primary_button("Send BETA"))
                .clicked()
            {
                request = Some(self.transfer_request());
            }
        });
        ui.add_space(8.0);
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Approve a grant").strong());
            ui.label(
                "Signs with your primary identity, which must be a member of the grant \
policy (members are public identities). Test value only; an accepted grant is evidence, \
not a conversion.",
            );
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("Grant policy ID");
                ui.text_edit_singleline(&mut self.grant_policy);
                ui.label("Grant ID");
                ui.text_edit_singleline(&mut self.grant_id);
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy && self.confirm,
                        theme::secondary_button("Approve this grant"),
                    )
                    .clicked()
                {
                    request = Some(self.grant_approve_request());
                }
                if ui
                    .add_enabled(!busy, theme::secondary_button("Check grant status"))
                    .clicked()
                {
                    request = Some(self.grant_status_request());
                }
            });
        });
        match request {
            Some(Ok(request)) => {
                if matches!(
                    request,
                    Request::Register { .. }
                        | Request::Transfer { .. }
                        | Request::GrantApprove { .. }
                ) {
                    self.confirm = false;
                }
                self.status.clear();
                Some(request)
            }
            Some(Err(error)) => {
                self.status = error;
                None
            }
            None => {
                if !self.status.is_empty() {
                    ui.add_space(6.0);
                    ui.label(&self.status);
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Panel {
        Panel {
            epoch: "07".repeat(32),
            campaigns: "zcampaign\n  zcampaign  \n\n".into(),
            from: "01".repeat(32),
            to: "02".repeat(32),
            amount: "0.5".into(),
            memo: "lunch".into(),
            ..Panel::default()
        }
    }

    #[test]
    fn amounts_are_exact_decimal_micro_units_without_floats() {
        for (text, micro) in [
            ("1", 1_000_000),
            ("0.5", 500_000),
            (".25", 250_000),
            ("3.000001", 3_000_001),
            (" 12.5 ", 12_500_000),
        ] {
            assert_eq!(parse_amount_micro(text), Ok(micro), "{text}");
        }
        for bad in [
            "",
            ".",
            "0",
            "0.000000",
            "-1",
            "1e3",
            "1.2345678",
            "abc",
            "1,5",
            "18446744073710",
        ] {
            assert!(parse_amount_micro(bad).is_err(), "{bad:?}");
        }
        assert_eq!(format_beta(1_500_000), "1.500000 BETA");
        assert_eq!(format_beta(7), "0.000007 BETA");
    }

    #[test]
    fn ids_and_campaigns_are_validated_and_deduplicated() {
        assert!(parse_hex32(&"ab".repeat(32), "x").is_ok());
        for bad in [
            "",
            "ab",
            &"zz".repeat(32),
            &"00".repeat(32),
            &"ab".repeat(33),
        ] {
            assert!(parse_hex32(bad, "x").is_err(), "{bad:?}");
        }
        assert_eq!(
            parse_campaigns("a\n b \na\n").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(parse_campaigns(" \n").is_err());
        let many = (0..=MAX_BETA_CAMPAIGNS)
            .map(|i| format!("c{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_campaigns(&many).is_err());
    }

    #[test]
    fn requests_are_built_only_from_valid_input() {
        let mut p = panel();
        assert!(matches!(p.register_request(), Ok(Request::Register { .. })));
        match p.transfer_request().unwrap() {
            Request::Transfer {
                campaigns,
                amount_micro,
                memo,
                ..
            } => {
                assert_eq!(campaigns, vec!["zcampaign".to_string()]);
                assert_eq!(amount_micro, 500_000);
                assert_eq!(memo, "lunch");
            }
            other => panic!("unexpected {other:?}"),
        }
        let mut same = panel();
        same.to = same.from.clone();
        assert!(same.transfer_request().is_err(), "self transfer refused");
        let mut no_memo = panel();
        no_memo.memo = "  ".into();
        assert!(no_memo.transfer_request().is_err());
        let mut no_campaign = panel();
        no_campaign.campaigns.clear();
        assert!(no_campaign.balances_request().is_err());
        let mut big_memo = panel();
        big_memo.memo = "m".repeat(MAX_BETA_MEMO_BYTES + 1);
        assert!(big_memo.transfer_request().is_err());
    }

    #[test]
    fn a_retried_request_reuses_its_operation_id_until_the_core_answers() {
        let id = |r: Request| match r {
            Request::Register { operation_id, .. } | Request::Transfer { operation_id, .. } => {
                operation_id
            }
            other => panic!("unexpected {other:?}"),
        };
        let mut p = panel();
        let first = id(p.register_request().unwrap());
        assert_eq!(first, id(p.register_request().unwrap()), "retry reuses id");
        p.on_account(&[9; 32]);
        assert_ne!(
            first,
            id(p.register_request().unwrap()),
            "next account is new"
        );

        let mut p = panel();
        let t1 = id(p.transfer_request().unwrap());
        assert_eq!(t1, id(p.transfer_request().unwrap()), "retry reuses id");
        p.amount = "0.6".into();
        let changed = id(p.transfer_request().unwrap());
        assert_ne!(t1, changed, "different content gets a new id");
        p.on_transfer(600_000, 0);
        p.amount = "0.6".into();
        p.memo = "lunch".into();
        assert_ne!(
            changed,
            id(p.transfer_request().unwrap()),
            "answered id is not reused"
        );
    }

    #[test]
    fn grant_requests_need_both_ids_and_reuse_their_operation_id_until_answered() {
        let id = |r: Request| match r {
            Request::GrantApprove { operation_id, .. } => operation_id,
            other => panic!("unexpected {other:?}"),
        };
        let mut p = panel();
        assert!(p.grant_approve_request().is_err());
        assert!(p.grant_status_request().is_err());
        p.grant_policy = " zpolicy ".into();
        assert!(p.grant_approve_request().is_err(), "grant id still empty");
        p.grant_id = "zgrant".into();
        match p.grant_status_request().unwrap() {
            Request::GrantStatus {
                policy_id,
                grant_id,
            } => assert_eq!(
                (policy_id.as_str(), grant_id.as_str()),
                ("zpolicy", "zgrant")
            ),
            other => panic!("unexpected {other:?}"),
        }
        let first = id(p.grant_approve_request().unwrap());
        assert_eq!(
            first,
            id(p.grant_approve_request().unwrap()),
            "retry reuses id"
        );
        p.grant_id = "zother".into();
        let changed = id(p.grant_approve_request().unwrap());
        assert_ne!(first, changed, "different grant gets a new id");
        p.on_grant_approval();
        assert_ne!(
            changed,
            id(p.grant_approve_request().unwrap()),
            "answered id is not reused"
        );
        p.grant_id = "g".repeat(MAX_OBJECT_ID_BYTES + 1);
        assert!(p.grant_approve_request().is_err(), "oversized id refused");
    }

    #[test]
    fn grant_status_text_is_honest_and_flags_equivocation() {
        let mut p = panel();
        p.on_grant_status(false, 1, 2, 0);
        assert!(p.status.contains("NOT accepted") && p.status.contains("1 of 2"));
        assert!(p.status.contains("not a conversion"));
        assert!(!p.status.contains("equivocation"));
        p.on_grant_status(true, 2, 2, 1);
        assert!(p.status.contains("ACCEPTED") && p.status.contains("equivocation"));
    }

    #[test]
    fn replies_update_state_and_never_hide_conflicts() {
        let mut p = panel();
        p.on_account(&[9; 32]);
        assert_eq!(p.from, "09".repeat(32));
        p.on_balances(BetaBalancesView {
            epoch: [7; 32],
            accounts: Vec::new(),
            total_issued_micro: 0,
            conflicts: 2,
            snapshot_digest: [0; 32],
        });
        assert!(p.status.contains("2 conflicting"), "{}", p.status);
        p.on_transfer(500_000, 500_000);
        assert!(p.amount.is_empty() && p.memo.is_empty());
        assert!(p.status.contains("provisional"));
    }
}
