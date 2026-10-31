//! BETA wallet panel: resettable Beta MINI *test value* for exercising product
//! flows. It is not production MINI, has no conversion right, and carries no
//! governance, review, release or personhood weight. Balances are provisional
//! state derived from the replicated signed-object set by the application
//! core; this panel holds no keys and signs nothing itself.
use eframe::egui;
use mini_app_protocol::{BetaBalancesView, MAX_BETA_CAMPAIGNS, MAX_BETA_MEMO_BYTES};

use crate::theme;

const MICRO_PER_BETA: u64 = 1_000_000;
const DISCLAIMER: &str =
    "BETA test value only: not production MINI, no conversion, no governance weight. \
Balances are provisional and every epoch starts again from zero.";

/// What the panel asks the host to send to the application core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    Register {
        epoch: [u8; 32],
    },
    Balances {
        epoch: [u8; 32],
        campaigns: Vec<String>,
    },
    Transfer {
        epoch: [u8; 32],
        campaigns: Vec<String>,
        from: [u8; 32],
        to: [u8; 32],
        amount_micro: u64,
        memo: String,
    },
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
    pub(crate) fn register_request(&self) -> Result<Request, String> {
        Ok(Request::Register {
            epoch: parse_hex32(&self.epoch, "epoch")?,
        })
    }

    pub(crate) fn balances_request(&self) -> Result<Request, String> {
        Ok(Request::Balances {
            epoch: parse_hex32(&self.epoch, "epoch")?,
            campaigns: parse_campaigns(&self.campaigns)?,
        })
    }

    pub(crate) fn transfer_request(&self) -> Result<Request, String> {
        let memo = self.memo.trim();
        if memo.is_empty() || memo.len() > MAX_BETA_MEMO_BYTES {
            return Err(format!("memo must be 1-{MAX_BETA_MEMO_BYTES} bytes"));
        }
        let from = parse_hex32(&self.from, "sender account")?;
        let to = parse_hex32(&self.to, "recipient account")?;
        if from == to {
            return Err("sender and recipient must differ".into());
        }
        Ok(Request::Transfer {
            epoch: parse_hex32(&self.epoch, "epoch")?,
            campaigns: parse_campaigns(&self.campaigns)?,
            from,
            to,
            amount_micro: parse_amount_micro(&self.amount)?,
            memo: memo.to_string(),
        })
    }

    pub(crate) fn on_account(&mut self, account: &[u8; 32]) {
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
        match request {
            Some(Ok(request)) => {
                if matches!(request, Request::Register { .. } | Request::Transfer { .. }) {
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
        let p = panel();
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
