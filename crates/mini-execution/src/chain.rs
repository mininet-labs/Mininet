//! [`LedgerChain`]: the one thing this crate exists to guarantee — a
//! [`LedgerState`] only ever advances behind a real, verified quorum
//! certificate ([`mini_chain::verify_finality`]). Nothing in this module
//! can apply a block's claims without first proving that block is final;
//! there is no "preview" or "tentative apply" path, on purpose (M2:
//! offline/local acceptance is a *risk decision* elsewhere in this tree
//! — `mini_settlement::evaluate_local_acceptance` — never something this
//! crate, the canonical-truth layer, blurs with finality).

use mini_chain::{verify_finality, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet};
use mini_economy::Amount;

use crate::body::SettlementBlockBody;
use crate::error::{ExecutionError, Result};
use crate::state::{apply_block, LedgerState};

/// A chain of finalized settlement state, advanced one verified block at a
/// time. Two independent [`LedgerChain`]s fed the identical sequence of
/// `(header, body, qc)` inputs are guaranteed — by construction, not just
/// by convention — to hold bit-identical [`LedgerState`]s at every height
/// (see this crate's integration tests): the same property Directive 4
/// demands of the real network this stands in for.
#[derive(Debug, Clone)]
pub struct LedgerChain {
    height: u64,
    tip_hash: [u8; 32],
    state: LedgerState,
}

impl LedgerChain {
    /// A fresh chain at genesis: height 0, an all-zero tip hash (matching
    /// `mini_chain::BlockHeader::prev_hash`'s own genesis convention), and
    /// empty settlement state.
    pub fn genesis() -> Self {
        Self::genesis_with_supply(Amount::ZERO)
    }

    /// Start a chain from an explicitly governed genesis circulating supply.
    pub fn genesis_with_supply(genesis_circulating: Amount) -> Self {
        Self::genesis_for_network(mini_settlement::MININET_NETWORK_ID, genesis_circulating)
    }

    pub fn genesis_for_network(network_id: [u8; 32], genesis_circulating: Amount) -> Self {
        LedgerChain {
            height: 0,
            tip_hash: [0u8; 32],
            state: LedgerState::with_network_and_genesis_supply(network_id, genesis_circulating),
        }
    }

    /// Start from a fully allocated transparent Tier-0 genesis balance set.
    pub fn genesis_with_balances(
        genesis_circulating: Amount,
        allocations: Vec<(Vec<u8>, Amount)>,
    ) -> Result<Self> {
        Self::genesis_with_network_balances(
            mini_settlement::MININET_NETWORK_ID,
            genesis_circulating,
            allocations,
        )
    }

    pub fn genesis_with_network_balances(
        network_id: [u8; 32],
        genesis_circulating: Amount,
        allocations: Vec<(Vec<u8>, Amount)>,
    ) -> Result<Self> {
        Ok(LedgerChain {
            height: 0,
            tip_hash: [0u8; 32],
            state: LedgerState::with_network_and_genesis_balances(
                network_id,
                genesis_circulating,
                allocations,
            )?,
        })
    }

    /// Restore a chain from a complete state whose commitment is bound to
    /// one locally-verified quorum-finalized header. This is the only state-
    /// replacement path; an unsigned state blob can never become canonical.
    pub fn from_finalized_snapshot(
        header: &BlockHeader,
        state: LedgerState,
        qc: &QuorumCertificate,
        validators: &ValidatorSet,
        oracle: &dyn ValidatorOracle,
        expected_network_id: [u8; 32],
    ) -> Result<Self> {
        if state.network_id() != expected_network_id {
            return Err(ExecutionError::SnapshotWrongNetwork);
        }
        state.verify_supply_conservation()?;
        state.verify_balance_map_total()?;
        verify_finality(qc, validators, oracle)?;
        if header.height == 0
            || header.timestamp_ms != header.height
            || qc.height != header.height
            || qc.block_hash != header.hash()
        {
            return Err(ExecutionError::SnapshotProofMismatch);
        }
        if header.state_root != state.commitment() {
            return Err(ExecutionError::StateRootMismatch);
        }
        Ok(LedgerChain {
            height: header.height,
            tip_hash: header.hash(),
            state,
        })
    }

    /// The current finalized height.
    pub fn height(&self) -> u64 {
        self.height
    }

    /// The current tip's header hash — what the next block's `prev_hash`
    /// must equal.
    pub fn tip_hash(&self) -> [u8; 32] {
        self.tip_hash
    }

    /// A read-only view of the current finalized settlement state — pass
    /// this directly wherever a [`mini_settlement::CanonicalLedgerView`]
    /// is needed (e.g. [`mini_settlement::reconcile`]).
    pub fn state(&self) -> &LedgerState {
        &self.state
    }

    /// Verify `qc` finalizes `header`, verify `header` legitimately
    /// extends this chain, verify `header.state_root` honestly commits to
    /// the state `body` actually produces, and — only if every one of
    /// those holds — advance. Returns the new state's commitment on
    /// success; changes nothing on any error.
    pub fn apply_finalized_block(
        &mut self,
        header: &BlockHeader,
        body: &SettlementBlockBody,
        qc: &QuorumCertificate,
        validators: &ValidatorSet,
        oracle: &dyn ValidatorOracle,
    ) -> Result<[u8; 32]> {
        // Reject attacker-controlled oversized or substituted bodies before
        // spending work on quorum-signature verification.
        if body.claims.len() > crate::MAX_CLAIMS_PER_BLOCK {
            return Err(ExecutionError::TooManyClaims);
        }
        if body.monetary_epochs.len() > crate::MAX_MONETARY_EPOCHS_PER_BLOCK {
            return Err(ExecutionError::TooManyMonetaryEpochs);
        }
        for epoch in &body.monetary_epochs {
            epoch
                .to_wire_bytes()
                .map_err(ExecutionError::InvalidMonetaryEpoch)?;
        }
        if header.body_root != body.hash() {
            return Err(ExecutionError::BodyRootMismatch);
        }

        verify_finality(qc, validators, oracle)?;

        let expected_height = self.height + 1;
        if header.height != expected_height {
            return Err(ExecutionError::WrongHeight {
                expected: expected_height,
                got: header.height,
            });
        }
        if header.prev_hash != self.tip_hash {
            return Err(ExecutionError::WrongParent);
        }
        if header.timestamp_ms != expected_height {
            return Err(ExecutionError::TimestampNotDeterministic {
                expected: expected_height,
                got: header.timestamp_ms,
            });
        }
        if qc.height != header.height || qc.block_hash != header.hash() {
            return Err(ExecutionError::NotFinalized(
                mini_chain::ChainError::QuorumNotMet {
                    needed: validators.quorum_threshold(),
                    got: 0,
                },
            ));
        }

        let next_state = apply_block(&self.state, body)?;
        let commitment = next_state.commitment();
        if header.state_root != commitment {
            return Err(ExecutionError::StateRootMismatch);
        }

        self.height = header.height;
        self.tip_hash = header.hash();
        self.state = next_state;
        Ok(commitment)
    }
}

impl Default for LedgerChain {
    fn default() -> Self {
        Self::genesis()
    }
}
